//! Audio spectrum analysis for the waterfall, and a fan-out of the radio's
//! receive audio to whoever wants it (waterfall, ...).

use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, bounded};
use parking_lot::Mutex;
use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

/// Sliding-window FFT over mono 16-bit audio.
pub struct Spectrum {
    rate: u32,
    size: usize,
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    history: Vec<f32>,
    scratch: Vec<Complex32>,
    /// Compute a new line every `hop` samples.
    hop: usize,
    since: usize,
}

impl Spectrum {
    /// `size` is the FFT length (power of two), `hop` the samples between lines.
    pub fn new(rate: u32, size: usize, hop: usize) -> Spectrum {
        let fft = FftPlanner::new().plan_fft_forward(size);
        // Hann window, normalised so a full-scale sine reads 0 dBFS.
        let window: Vec<f32> = (0..size).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / size as f32).cos()).collect();
        Spectrum { rate, size, fft, window, history: vec![0.0; size], scratch: vec![Complex32::new(0.0, 0.0); size], hop: hop.clamp(1, size), since: 0 }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Hz per bin.
    pub fn resolution(&self) -> f32 {
        self.rate as f32 / self.size as f32
    }

    pub fn set_hop(&mut self, hop: usize) {
        self.hop = hop.clamp(1, self.size);
    }

    /// Feed samples; returns every new spectrum line (dBFS per bin,
    /// `size/2` bins from 0 Hz to Nyquist).
    pub fn push(&mut self, samples: &[i16]) -> Vec<Vec<f32>> {
        let mut out = Vec::new();
        for chunk in samples.chunks(self.hop) {
            self.history.drain(..chunk.len());
            self.history.extend(chunk.iter().map(|&s| s as f32 / 32768.0));
            self.since += chunk.len();
            if self.since >= self.hop {
                self.since = 0;
                out.push(self.line());
            }
        }
        out
    }

    fn line(&mut self) -> Vec<f32> {
        for (i, c) in self.scratch.iter_mut().enumerate() {
            *c = Complex32::new(self.history[i] * self.window[i], 0.0);
        }
        self.fft.process(&mut self.scratch);
        // Hann coherent gain is 0.5; a full-scale sine has amplitude N/4 per side.
        let scale = 4.0 / self.size as f32;
        self.scratch[..self.size / 2]
            .iter()
            .map(|c| {
                let mag = c.norm() * scale;
                20.0 * mag.max(1e-7).log10()
            })
            .collect()
    }
}

/// Fans the radio's receive audio out to subscribers. Frames are dropped for
/// subscribers that fall behind instead of buffering without bound.
#[derive(Clone, Default)]
pub struct AudioTap {
    subs: Arc<Mutex<Vec<Sender<Vec<i16>>>>>,
    rate: Arc<Mutex<u32>>,
}

impl AudioTap {
    pub fn subscribe(&self) -> Receiver<Vec<i16>> {
        let (tx, rx) = bounded(200);
        self.subs.lock().push(tx);
        rx
    }

    /// Anyone listening? (Departed subscribers are pruned by `publish`.)
    pub fn has_subscribers(&self) -> bool {
        !self.subs.lock().is_empty()
    }

    pub fn publish(&self, rate: u32, frame: &[i16]) {
        *self.rate.lock() = rate;
        self.subs.lock().retain(|s| match s.try_send(frame.to_vec()) {
            Ok(()) => true,
            Err(crossbeam_channel::TrySendError::Full(_)) => true,
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => false,
        });
    }

    /// Sample rate of the last published audio.
    pub fn rate(&self) -> u32 {
        *self.rate.lock()
    }
}

/// Records the local radio's receive audio into an [`AudioTap`] while
/// anybody is subscribed. Restarts the recorder if it dies (radio off,
/// cable unplugged).
pub struct LocalRxAudio {
    stop: Arc<std::sync::atomic::AtomicBool>,
    pub status: Arc<Mutex<Option<String>>>,
}

impl LocalRxAudio {
    /// `device` finds the radio's audio input each time recording (re)starts,
    /// so a radio that was off at start-up is picked up when it comes on.
    /// Never falls back to another input (a laptop mic is not the radio).
    pub fn start(tap: AudioTap, device: impl Fn() -> Option<crate::audio::AudioDevice> + Send + 'static, rate: u32) -> LocalRxAudio {
        use std::sync::atomic::{AtomicBool, Ordering};
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(None));
        let (stop2, status2) = (stop.clone(), status.clone());
        std::thread::Builder::new()
            .name("rx-audio".into())
            .spawn(move || {
                let mut capture: Option<crate::audio::Capture> = None;
                while !stop2.load(Ordering::Relaxed) {
                    if !tap.has_subscribers() {
                        capture = None;
                        std::thread::sleep(Duration::from_millis(200));
                        continue;
                    }
                    if capture.as_mut().is_none_or(|c| !c.alive()) {
                        let Some(device) = device() else {
                            *status2.lock() = Some("radio audio device not found (is the radio on?)".into());
                            std::thread::sleep(Duration::from_secs(3));
                            continue;
                        };
                        match crate::audio::Capture::start(&device, rate) {
                            Ok(c) => {
                                *status2.lock() = Some(format!("{device}"));
                                capture = Some(c);
                            }
                            Err(e) => {
                                *status2.lock() = Some(format!("error: {e}"));
                                std::thread::sleep(Duration::from_secs(3));
                                continue;
                            }
                        }
                    }
                    if let Some(c) = &capture {
                        match c.frames.recv_timeout(Duration::from_millis(500)) {
                            Ok(f) => tap.publish(rate, &f),
                            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                            Err(_) => capture = None,
                        }
                    }
                }
            })
            .expect("spawn rx audio");
        LocalRxAudio { stop, status }
    }
}

impl Drop for LocalRxAudio {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, hz: f32, amp: f32, n: usize) -> Vec<i16> {
        (0..n).map(|i| (amp * 32767.0 * (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin()) as i16).collect()
    }

    #[test]
    fn finds_a_tone_at_the_right_frequency_and_level() {
        let mut s = Spectrum::new(24000, 4096, 4096);
        let lines = s.push(&tone(24000, 1000.0, 0.5, 4096));
        assert_eq!(lines.len(), 1);
        let line = &lines[0];
        let (peak_bin, peak_db) = line.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map(|(i, v)| (i, *v)).unwrap();
        let peak_hz = peak_bin as f32 * s.resolution();
        assert!((peak_hz - 1000.0).abs() < 2.0 * s.resolution(), "{peak_hz}");
        // 0.5 of full scale = -6 dBFS (Hann scalloping can cost ~1.4 dB).
        assert!((-7.6..=-5.5).contains(&peak_db), "{peak_db}");
        // Far from the tone it is quiet.
        assert!(line[(3000.0 / s.resolution()) as usize] < -60.0);
    }

    #[test]
    fn hop_controls_line_rate() {
        let mut s = Spectrum::new(24000, 1024, 480);
        let lines = s.push(&vec![0i16; 4800]);
        assert_eq!(lines.len(), 10);
    }

    #[test]
    fn tap_fans_out_and_forgets_dropped_subscribers() {
        let tap = AudioTap::default();
        let a = tap.subscribe();
        let b = tap.subscribe();
        tap.publish(24000, &[1, 2, 3]);
        assert_eq!(a.try_recv().unwrap(), vec![1, 2, 3]);
        assert_eq!(b.try_recv().unwrap(), vec![1, 2, 3]);
        drop(b);
        tap.publish(24000, &[4]);
        assert_eq!(tap.subs.lock().len(), 1);
        assert_eq!(tap.rate(), 24000);
    }
}
