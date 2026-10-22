//! Audio capture and playback through PipeWire (`pw-record`/`pw-play`) or
//! plain ALSA (`arecord`/`aplay`).
//!
//! Using the system tools keeps us out of device-locking trouble: through
//! PipeWire the radio's codec can be recorded by us *and* by WSJT-X at the
//! same time. Audio is mono signed 16-bit little-endian.

use std::io::{Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, bounded};
use serde::{Deserialize, Serialize};

/// Where audio comes from or goes to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AudioDevice {
    /// The system default input/output.
    #[default]
    Default,
    /// A PipeWire node by `node.name` (e.g. the radio's
    /// `alsa_input.usb-Burr-Brown_from_TI_USB_Audio_CODEC-00.analog-stereo`).
    PipeWire(String),
    /// An ALSA PCM name (e.g. `plughw:CARD=CODEC,DEV=0`).
    Alsa(String),
}

impl std::fmt::Display for AudioDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioDevice::Default => f.write_str("system default"),
            AudioDevice::PipeWire(n) => write!(f, "PipeWire {n}"),
            AudioDevice::Alsa(n) => write!(f, "ALSA {n}"),
        }
    }
}

fn have(cmd: &str) -> bool {
    Command::new("sh").args(["-c", &format!("command -v {cmd} >/dev/null")]).status().is_ok_and(|s| s.success())
}

fn record_command(dev: &AudioDevice, rate: u32) -> Command {
    let pipewire = !matches!(dev, AudioDevice::Alsa(_)) && have("pw-record");
    if pipewire {
        let mut c = Command::new("pw-record");
        if let AudioDevice::PipeWire(n) = dev {
            c.args(["--target", n]);
        }
        c.args(["--rate", &rate.to_string(), "--channels", "1", "--format", "s16", "--latency", "20ms"]);
        c.args(["-P", "{ node.description = \"FTDX3000 rig control\" }", "-"]);
        c
    } else {
        let mut c = Command::new("arecord");
        if let AudioDevice::Alsa(n) = dev {
            c.args(["-D", n]);
        }
        c.args(["-q", "-t", "raw", "-f", "S16_LE", "-c", "1", "-r", &rate.to_string(), "--buffer-time=40000"]);
        c
    }
}

fn play_command(dev: &AudioDevice, rate: u32) -> Command {
    let pipewire = !matches!(dev, AudioDevice::Alsa(_)) && have("pw-play");
    if pipewire {
        let mut c = Command::new("pw-play");
        if let AudioDevice::PipeWire(n) = dev {
            c.args(["--target", n]);
        }
        c.args(["--rate", &rate.to_string(), "--channels", "1", "--format", "s16", "--latency", "40ms"]);
        c.args(["-P", "{ node.description = \"FTDX3000 rig control\" }", "-"]);
        c
    } else {
        let mut c = Command::new("aplay");
        if let AudioDevice::Alsa(n) = dev {
            c.args(["-D", n]);
        }
        c.args(["-q", "-t", "raw", "-f", "S16_LE", "-c", "1", "-r", &rate.to_string(), "--buffer-time=80000"]);
        c
    }
}

/// Samples per 20 ms frame.
pub fn frame_len(rate: u32) -> usize {
    (rate / 50) as usize
}

/// A running capture. Frames of 20 ms arrive on `frames`. Stops on drop.
pub struct Capture {
    child: Child,
    stop: Arc<AtomicBool>,
    pub frames: Receiver<Vec<i16>>,
}

impl Capture {
    pub fn start(dev: &AudioDevice, rate: u32) -> std::io::Result<Capture> {
        let mut child = record_command(dev, rate).stdout(Stdio::piped()).stderr(Stdio::null()).stdin(Stdio::null()).spawn()?;
        let mut out = child.stdout.take().expect("piped stdout");
        let (tx, rx) = bounded(100);
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let n = frame_len(rate);
        thread::Builder::new().name("audio-capture".into()).spawn(move || {
            let mut buf = vec![0u8; n * 2];
            while !stop2.load(Ordering::Relaxed) {
                if out.read_exact(&mut buf).is_err() {
                    break;
                }
                let frame: Vec<i16> = buf.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
                // Drop frames if nobody keeps up rather than growing latency.
                let _ = tx.try_send(frame);
            }
        })?;
        Ok(Capture { child, stop, frames: rx })
    }

    /// The capture process is still running.
    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A running playback with latency control. Stops on drop.
pub struct Playback {
    child: Child,
    tx: Sender<Vec<i16>>,
}

/// Keeps playback latency bounded: drops audio when we're feeding faster
/// than real time (sender clock faster than ours), resets when starved.
pub(crate) struct Pacer {
    rate: f64,
    start: Instant,
    written: f64,
    max_ahead: f64,
}

impl Pacer {
    pub(crate) fn new(rate: u32, max_ahead: Duration) -> Self {
        Pacer { rate: rate as f64, start: Instant::now(), written: 0.0, max_ahead: max_ahead.as_secs_f64() * rate as f64 }
    }

    /// Should this many samples be written now?
    pub(crate) fn admit(&mut self, n: usize) -> bool {
        let expected = self.start.elapsed().as_secs_f64() * self.rate;
        if expected - self.written > self.rate * 0.25 {
            // We fell behind (network gap): restart the reference.
            self.start = Instant::now();
            self.written = 0.0;
        }
        if self.written - expected > self.max_ahead {
            return false;
        }
        self.written += n as f64;
        true
    }
}

impl Playback {
    pub fn start(dev: &AudioDevice, rate: u32, max_latency: Duration) -> std::io::Result<Playback> {
        let mut child = play_command(dev, rate).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
        let mut stdin: ChildStdin = child.stdin.take().expect("piped stdin");
        let (tx, rx) = bounded::<Vec<i16>>(64);
        thread::Builder::new().name("audio-playback".into()).spawn(move || {
            let mut pacer = Pacer::new(rate, max_latency);
            // Prime with a little silence so the first packets don't underrun.
            let prime = vec![0u8; frame_len(rate) * 2 * 2];
            let _ = stdin.write_all(&prime);
            pacer.admit(frame_len(rate) * 2);
            while let Ok(frame) = rx.recv() {
                if !pacer.admit(frame.len()) {
                    continue;
                }
                let bytes: Vec<u8> = frame.iter().flat_map(|s| s.to_le_bytes()).collect();
                if stdin.write_all(&bytes).is_err() {
                    break;
                }
            }
        })?;
        Ok(Playback { child, tx })
    }

    /// Queue a frame (never blocks; drops if the queue is full).
    pub fn push(&self, frame: Vec<i16>) {
        let _ = self.tx.try_send(frame);
    }

    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// RMS level of a frame in dBFS (-100 for silence).
pub fn level_dbfs(frame: &[i16]) -> f32 {
    if frame.is_empty() {
        return -100.0;
    }
    let sum: f64 = frame.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum / frame.len() as f64).sqrt() / 32768.0;
    if rms <= 1e-5 { -100.0 } else { (20.0 * rms.log10()) as f32 }
}

/// Apply a gain (linear) with clipping.
pub fn apply_gain(frame: &mut [i16], gain: f32) {
    if (gain - 1.0).abs() < 1e-3 {
        return;
    }
    for s in frame {
        *s = ((*s as f32) * gain).clamp(-32768.0, 32767.0) as i16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pacer_bounds_latency() {
        let mut p = Pacer::new(24000, Duration::from_millis(100));
        // Feeding 1 s of audio instantly: only ~100 ms (+ one frame) gets through.
        let admitted = (0..50).filter(|_| p.admit(480)).count();
        assert!((5..=7).contains(&admitted), "{admitted}");
    }

    #[test]
    fn levels() {
        assert_eq!(level_dbfs(&[0; 480]), -100.0);
        let full: Vec<i16> = (0..480).map(|i| if i % 2 == 0 { 32767 } else { -32767 }).collect();
        assert!(level_dbfs(&full) > -0.1);
        let mut f = vec![1000i16; 4];
        apply_gain(&mut f, 2.0);
        assert_eq!(f, vec![2000; 4]);
    }
}
