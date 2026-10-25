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

/// Playout buffer for audio that arrives in clumps: Wi-Fi often delivers the
/// 20 ms packets several at a time, 100 ms or more apart. It waits until it
/// holds `target` before playing, and when it runs dry anyway it fades out
/// (no click), waits for more next time, and fades back in.
///
/// The station's sound card and this one never run at quite the same rate
/// (a ThinkPad station measured 0.4 to 0.9 % slow); without allowing for it
/// a slow station drains any buffer, however deep, every half minute. So
/// playback speed is the measured rate difference (samples received against
/// samples played over the last half minute or so: long enough for clumps to
/// average out, short enough to follow a station whose USB audio loses more
/// at some times than others) plus a gentle pull toward `target`, never
/// more than 3 % off. Same design as the iPhone app's player.
pub(crate) struct Jitter {
    buf: std::collections::VecDeque<f32>,
    playing: bool,
    /// Depth to build up before (re)starting, and to hold while playing.
    target: usize,
    min_target: usize,
    max_target: usize,
    /// Hard limit: beyond this the oldest audio is dropped.
    cap: usize,
    /// Depth smoothed over about 2 s, so a clump doesn't swing the speed.
    smoothed: f64,
    /// Samples received and played while playing (prebuffering would
    /// count as a fast station), both fading by 3 % a second, and the rate
    /// difference they show.
    received: f64,
    ticks: f64,
    since_fade: usize,
    drift: f64,
    /// Playback speed now, and the position between the next two samples.
    speed: f64,
    frac: f64,
    /// Samples played since the last dropout, to give back depth that a
    /// long clean spell shows isn't needed.
    since_dry: usize,
    last: f32,
    gain: f32,
    gain_step: f32,
    rate: u32,
    /// Times it ran dry.
    pub(crate) dropouts: u32,
}

#[cfg_attr(target_os = "linux", allow(dead_code))] // used by the cpal backend
impl Jitter {
    /// Most the playback speed may move from 1.
    const MAX_SKEW: f64 = 0.03;

    pub(crate) fn new(rate: u32, start: Duration, max: Duration) -> Self {
        let n = |d: Duration| (d.as_secs_f64() * rate as f64) as usize;
        let target = n(start).max(1);
        let max_target = n(max).max(target);
        Jitter {
            buf: std::collections::VecDeque::new(),
            playing: false,
            target,
            min_target: target,
            max_target,
            cap: max_target * 2,
            smoothed: target as f64,
            // No difference to start with, worth 10 s of evidence.
            received: rate as f64 * 10.0,
            ticks: rate as f64 * 10.0,
            since_fade: 0,
            drift: 0.0,
            speed: 1.0,
            frac: 0.0,
            since_dry: 0,
            last: 0.0,
            gain: 0.0,
            gain_step: 1.0 / (rate as f32 * 0.005), // 5 ms fades
            rate,
            dropouts: 0,
        }
    }

    /// What it waits for before (re)starting.
    pub(crate) fn target(&self) -> Duration {
        Duration::from_secs_f64(self.target as f64 / self.rate as f64)
    }

    pub(crate) fn push(&mut self, samples: &[f32]) {
        if self.playing {
            self.received += samples.len() as f64;
        }
        self.buf.extend(samples.iter().copied());
        let excess = self.buf.len().saturating_sub(self.cap);
        self.buf.drain(..excess);
    }

    /// The next sample to play.
    pub(crate) fn next(&mut self) -> f32 {
        if !self.playing && self.buf.len() >= self.target {
            self.playing = true;
            self.frac = 0.0;
        }
        if self.playing {
            // Two samples to interpolate between.
            if self.buf.len() >= 2 {
                let (a, b) = (self.buf[0], self.buf[1]);
                let v = a + (b - a) * self.frac as f32;
                self.steer();
                self.frac += self.speed;
                while self.frac >= 1.0 && self.buf.len() >= 2 {
                    self.buf.pop_front();
                    self.frac -= 1.0;
                }
                self.gain = (self.gain + self.gain_step).min(1.0);
                self.last = v;
                return v * self.gain;
            }
            // Ran dry: rebuild a deeper buffer before playing again.
            self.playing = false;
            self.since_dry = 0;
            self.target = (self.target + self.target / 2).min(self.max_target);
            self.dropouts += 1;
        }
        self.gain = (self.gain - self.gain_step).max(0.0);
        self.last * self.gain
    }

    /// Set the playback speed: the clocks' measured difference, plus up to
    /// 1 % more to bring the depth back to `target`.
    fn steer(&mut self) {
        let rate = self.rate as f64;
        self.ticks += 1.0;
        self.since_fade += 1;
        if self.since_fade >= self.rate as usize {
            self.since_fade = 0;
            self.received *= 0.97;
            self.ticks *= 0.97;
        }
        self.smoothed += (self.buf.len() as f64 - self.smoothed) / (2.0 * rate);
        self.drift = ((self.received - self.ticks) / self.ticks).clamp(-Self::MAX_SKEW, Self::MAX_SKEW);
        // Depth error in units of 100 ms, so it behaves the same at any target.
        let err = (self.smoothed - self.target as f64) / (rate / 10.0);
        let pull = (err * 0.005).clamp(-0.01, 0.01);
        self.speed = 1.0 + (self.drift + pull).clamp(-Self::MAX_SKEW, Self::MAX_SKEW);
        // Five minutes without a dropout: try 10 % less latency. Sooner
        // trims it below what the next Wi-Fi gap needs.
        self.since_dry += 1;
        if self.since_dry >= self.rate as usize * 300 {
            self.since_dry = 0;
            self.target = (self.target - self.target / 10).max(self.min_target);
        }
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
    fn jitter_rides_out_clumps_without_clicks() {
        let rate = 24_000;
        let mut j = Jitter::new(rate, Duration::from_millis(40), Duration::from_millis(300));
        let tone = |i: usize| (std::f32::consts::TAU * 700.0 * i as f32 / rate as f32).sin() * 0.5;
        let frame = 480;
        let (mut sent, mut out) = (0usize, Vec::new());
        // Packets arrive five at a time every 100 ms, now and then 60 ms late
        // (and the next clump that much sooner).
        for clump in 0..100 {
            for _ in 0..5 {
                let f: Vec<f32> = (sent..sent + frame).map(tone).collect();
                j.push(&f);
                sent += frame;
            }
            let gap = match clump % 10 {
                8 => 160,
                9 => 40,
                _ => 100,
            };
            out.extend((0..rate as usize * gap / 1000).map(|_| j.next()));
        }
        // No clicks: no jump much bigger than the tone itself makes (700 Hz
        // at 0.5: ~0.09 per sample).
        let worst = out.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        assert!(worst < 0.2, "{worst}");
        // After the first runs dry, the deeper buffer holds: no more dropouts late on.
        let tail = &out[out.len() / 2..];
        let silent_runs = tail.windows(48).filter(|w| w.iter().all(|s| s.abs() < 1e-3)).count();
        assert_eq!(silent_runs, 0);
    }

    #[test]
    fn jitter_takes_back_latency_it_does_not_need() {
        let rate = 24_000;
        let mut j = Jitter::new(rate, Duration::from_millis(40), Duration::from_millis(300));
        // 250 ms queued up front, then a steady feed for 80 s: latency comes
        // back down to about 20 ms.
        j.push(&vec![0.1; rate as usize / 4]);
        for _ in 0..80 * 50 {
            j.push(&[0.1; 480]);
            for _ in 0..480 {
                j.next();
            }
        }
        assert!(j.buf.len() < rate as usize / 20, "{}", j.buf.len());
    }

    #[test]
    fn jitter_keeps_up_with_a_slow_station() {
        let rate = 24_000;
        let mut j = Jitter::new(rate, Duration::from_millis(100), Duration::from_millis(400));
        // The station sends 1 % slow (475 samples per 480 played), in
        // clumps of five packets every 100 ms, for five minutes.
        for _ in 0..5 * 60 * 10 {
            for _ in 0..5 {
                j.push(&[0.1; 475]);
            }
            for _ in 0..2400 {
                j.next();
            }
        }
        // A slow start, then it learns the difference and holds.
        let early = j.dropouts;
        for _ in 0..2 * 60 * 10 {
            for _ in 0..5 {
                j.push(&[0.1; 475]);
            }
            for _ in 0..2400 {
                j.next();
            }
        }
        assert!(early <= 3, "{early}");
        assert_eq!(j.dropouts, early);
        assert!((j.drift + 0.0104).abs() < 0.002, "{}", j.drift);
    }

    #[test]
    fn jitter_follows_a_station_whose_rate_changes() {
        let rate = 24_000;
        let mut j = Jitter::new(rate, Duration::from_millis(100), Duration::from_millis(400));
        let mut feed = |j: &mut Jitter, per_frame: usize, secs: usize| {
            for _ in 0..secs * 10 {
                for _ in 0..5 {
                    j.push(&vec![0.1; per_frame]);
                }
                for _ in 0..2400 {
                    j.next();
                }
            }
        };
        // Three minutes 2.5 % slow, then the loss stops for three minutes.
        feed(&mut j, 468, 180);
        assert!((j.drift + 0.025).abs() < 0.004, "{}", j.drift);
        let before = j.dropouts;
        feed(&mut j, 480, 180);
        assert!(j.drift.abs() < 0.004, "{}", j.drift);
        assert_eq!(j.dropouts, before);
        // No backlog built up while it caught on.
        assert!(j.buf.len() < j.max_target, "{}", j.buf.len());
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
