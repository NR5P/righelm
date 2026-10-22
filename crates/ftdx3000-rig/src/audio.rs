//! Audio capture and playback, mono signed 16-bit in 20 ms frames.
//!
//! Linux uses PipeWire/ALSA command-line tools (sharing the radio's codec
//! with other programs); other systems (macOS) use CoreAudio through cpal.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
#[path = "audio_linux.rs"]
mod backend;
#[cfg(not(target_os = "linux"))]
#[path = "audio_cpal.rs"]
mod backend;

pub use backend::{Capture, Playback, default_input, list_devices, radio_devices};

/// A speaker or microphone that can be chosen in the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceEntry {
    pub device: AudioDevice,
    pub label: String,
    pub input: bool,
}

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
    /// A device by its display name (CoreAudio on macOS, e.g. "USB Audio CODEC").
    Named(String),
}

impl std::fmt::Display for AudioDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioDevice::Default => f.write_str("system default"),
            AudioDevice::PipeWire(n) => write!(f, "PipeWire {n}"),
            AudioDevice::Alsa(n) => write!(f, "ALSA {n}"),
            AudioDevice::Named(n) => f.write_str(n),
        }
    }
}

/// Looks like the radio's own USB codec (a TI PCM2903B, "USB Audio CODEC").
/// Its input is the radio's receive audio, never a microphone.
pub fn is_radio_codec(dev: &AudioDevice) -> bool {
    let n = match dev {
        AudioDevice::Default => return false,
        AudioDevice::PipeWire(n) | AudioDevice::Alsa(n) | AudioDevice::Named(n) => n.to_lowercase(),
    };
    n.contains("usb_audio_codec") || n.contains("usb audio codec") || n.contains("pcm2903") || n.contains("card=codec")
}

/// The computer's microphone: `chosen`, or for `Default` the system's
/// default input unless that is the radio's codec (PipeWire often makes the
/// radio the default input when it is plugged in), then the first other one.
pub fn computer_mic(chosen: &AudioDevice) -> Result<AudioDevice, String> {
    if is_radio_codec(chosen) {
        return Err("the chosen microphone is the radio's receive audio; pick the computer's microphone".into());
    }
    if *chosen != AudioDevice::Default {
        return Ok(chosen.clone());
    }
    // Prefer the computer's built-in microphone over e.g. a dock's jack.
    let other = || {
        let mut inputs: Vec<DeviceEntry> = list_devices()
            .into_iter()
            .filter(|d| d.input && !is_radio_codec(&d.device) && !is_radio_codec(&AudioDevice::Named(d.label.clone())))
            .collect();
        let built_in = |d: &DeviceEntry| {
            let t = format!("{} {}", d.device, d.label).to_lowercase();
            t.contains("pci") || t.contains("built-in") || t.contains("internal") || t.contains("macbook")
        };
        inputs.sort_by_key(|d| !built_in(d));
        inputs.into_iter().next().map(|d| d.device)
    };
    match default_input() {
        Some(d) if !is_radio_codec(&d) => Ok(d),
        Some(_) => other().ok_or_else(|| "no microphone found on this computer (only the radio's audio input)".into()),
        // The default isn't known: prefer a listed non-radio input.
        None => Ok(other().unwrap_or(AudioDevice::Default)),
    }
}

/// A friendly name for a device (its description where known).
pub fn describe(dev: &AudioDevice) -> String {
    match dev {
        AudioDevice::Default => "system default".into(),
        AudioDevice::PipeWire(_) => list_devices().into_iter().find(|d| d.device == *dev).map(|d| d.label).unwrap_or_else(|| dev.to_string()),
        _ => dev.to_string(),
    }
}

/// Samples per 20 ms frame.
pub fn frame_len(rate: u32) -> usize {
    (rate / 50) as usize
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

/// Linear-interpolating sample-rate converter that keeps state across chunks.
pub(crate) struct Resampler {
    step: f64,
    pos: f64,
    buf: Vec<f32>,
}

#[cfg_attr(target_os = "linux", allow(dead_code))] // used by the cpal backend
impl Resampler {
    pub(crate) fn new(from: u32, to: u32) -> Self {
        Resampler { step: from as f64 / to as f64, pos: 0.0, buf: Vec::new() }
    }

    pub(crate) fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.buf.extend_from_slice(input);
        while self.pos + 1.0 < self.buf.len() as f64 {
            let i = self.pos as usize;
            let f = (self.pos - i as f64) as f32;
            out.push(self.buf[i] * (1.0 - f) + self.buf[i + 1] * f);
            self.pos += self.step;
        }
        let used = (self.pos as usize).min(self.buf.len());
        self.buf.drain(..used);
        self.pos -= used as f64;
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

    #[test]
    fn resampler_halves_and_keeps_frequency() {
        let mut r = Resampler::new(48_000, 24_000);
        let input: Vec<f32> = (0..4800).map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 48_000.0).sin()).collect();
        let mut out = Vec::new();
        for chunk in input.chunks(333) {
            r.process(chunk, &mut out);
        }
        assert!((2398..=2400).contains(&out.len()), "{}", out.len());
        // 1 kHz at 24 kHz: 24 samples per cycle -> zero crossings every 12.
        let crossings = out.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        assert!((99..=101).contains(&crossings), "{crossings}");
    }
}
