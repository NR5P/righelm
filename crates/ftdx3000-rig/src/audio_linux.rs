//! Linux audio: PipeWire (`pw-record`/`pw-play`) or plain ALSA
//! (`arecord`/`aplay`). Through PipeWire the radio's codec can be recorded by
//! us *and* by WSJT-X at the same time.

use std::io::{Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, bounded};

use super::{AudioDevice, DeviceEntry, Pacer, frame_len};

fn have(cmd: &str) -> bool {
    Command::new("sh").args(["-c", &format!("command -v {cmd} >/dev/null")]).status().is_ok_and(|s| s.success())
}

/// `pw-cat` 1.x reads/writes a sound file on `-` unless told `--raw`
/// (playback then fails outright, recording gets a file header); older
/// versions have no such flag and are raw already.
fn pw_raw_flag() -> Option<&'static str> {
    static RAW: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let raw = *RAW.get_or_init(|| {
        Command::new("pw-cat").arg("--help").output().is_ok_and(|o| {
            let text = [o.stdout, o.stderr].concat();
            String::from_utf8_lossy(&text).contains("--raw")
        })
    });
    raw.then_some("--raw")
}

fn record_command(dev: &AudioDevice, rate: u32) -> Command {
    let pipewire = !matches!(dev, AudioDevice::Alsa(_)) && have("pw-record");
    if pipewire {
        let mut c = Command::new("pw-record");
        c.args(pw_raw_flag());
        if let AudioDevice::PipeWire(n) | AudioDevice::Named(n) = dev {
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
        c.args(pw_raw_flag());
        if let AudioDevice::PipeWire(n) | AudioDevice::Named(n) = dev {
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


/// Speakers and microphones known to PipeWire.
pub fn list_devices() -> Vec<DeviceEntry> {
    crate::detect::list_audio_nodes()
        .into_iter()
        .map(|n| DeviceEntry { input: n.class == "Audio/Source", label: n.description, device: AudioDevice::PipeWire(n.name) })
        .collect()
}

/// The system's default input (PipeWire's default source), if known.
pub fn default_input() -> Option<AudioDevice> {
    let out = Command::new("wpctl").args(["inspect", "@DEFAULT_AUDIO_SOURCE@"]).stderr(Stdio::null()).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .find_map(|l| l.trim().trim_start_matches('*').trim().strip_prefix("node.name = "))
        .map(|v| AudioDevice::PipeWire(v.trim().trim_matches('"').to_string()))
}

/// The radio codec's capture and playback devices, if detected.
pub fn radio_devices() -> (Option<AudioDevice>, Option<AudioDevice>) {
    let Some(a) = crate::detect::detect_radios().into_iter().find_map(|r| r.audio) else { return (None, None) };
    let rx = a.source.as_ref().map(|(n, _)| AudioDevice::PipeWire(n.clone())).unwrap_or(AudioDevice::Alsa(a.alsa_plughw()));
    let tx = a.sink.as_ref().map(|(n, _)| AudioDevice::PipeWire(n.clone())).unwrap_or(AudioDevice::Alsa(a.alsa_plughw()));
    (Some(rx), Some(tx))
}
