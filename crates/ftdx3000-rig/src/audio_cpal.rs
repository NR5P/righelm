//! Audio through cpal (CoreAudio on macOS). Devices run at their native
//! format and rate; we convert to/from mono i16 at the requested rate.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use crossbeam_channel::{Receiver, Sender, bounded};
use parking_lot::Mutex;

use super::{AudioDevice, DeviceEntry, Resampler, frame_len};

fn device_name(d: &cpal::Device) -> String {
    d.to_string()
}

/// Find a device by name (exact, then case-insensitive substring).
fn find(dev: &AudioDevice, input: bool) -> Option<cpal::Device> {
    let host = cpal::default_host();
    let name = match dev {
        AudioDevice::Default => None,
        AudioDevice::Named(n) | AudioDevice::PipeWire(n) => Some(n.clone()),
        AudioDevice::Alsa(_) => None,
    };
    let Some(name) = name else {
        return if input { host.default_input_device() } else { host.default_output_device() };
    };
    let list: Vec<cpal::Device> = if input {
        host.input_devices().ok()?.collect()
    } else {
        host.output_devices().ok()?.collect()
    };
    let lower = name.to_lowercase();
    list.iter()
        .position(|d| device_name(d) == name)
        .or_else(|| list.iter().position(|d| device_name(d).to_lowercase().contains(&lower)))
        .map(|i| list[i].clone())
}

/// A running capture. Frames of 20 ms arrive on `frames`. Stops on drop.
pub struct Capture {
    stop: Arc<AtomicBool>,
    ok: Arc<AtomicBool>,
    pub frames: Receiver<Vec<i16>>,
}

impl Capture {
    pub fn start(dev: &AudioDevice, rate: u32) -> std::io::Result<Capture> {
        let (tx, rx) = bounded(100);
        let stop = Arc::new(AtomicBool::new(false));
        let ok = Arc::new(AtomicBool::new(true));
        let (ready_tx, ready_rx) = bounded::<Result<(), String>>(1);
        let (dev, stop2, ok2) = (dev.clone(), stop.clone(), ok.clone());
        // cpal streams can't move between threads on every platform, so each
        // lives on its own thread for its whole life.
        thread::Builder::new().name("audio-capture".into()).spawn(move || {
            let stream = match open_input(&dev, rate, tx, ok2.clone()) {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            if let Err(e) = stream.play() {
                let _ = ready_tx.send(Err(e.to_string()));
                return;
            }
            let _ = ready_tx.send(Ok(()));
            while !stop2.load(Ordering::Relaxed) && ok2.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(100));
            }
            drop(stream);
        })?;
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(Capture { stop, ok, frames: rx }),
            Ok(Err(e)) => Err(std::io::Error::other(e)),
            Err(_) => Err(std::io::Error::other("audio input did not start")),
        }
    }

    pub fn alive(&mut self) -> bool {
        self.ok.load(Ordering::Relaxed)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn open_input(dev: &AudioDevice, rate: u32, tx: Sender<Vec<i16>>, ok: Arc<AtomicBool>) -> Result<cpal::Stream, String> {
    let device = find(dev, true).ok_or_else(|| format!("audio input \"{dev}\" not found"))?;
    let supported = device.default_input_config().map_err(|e| e.to_string())?;
    let config = supported.config();
    let channels = config.channels as usize;
    let mut resampler = Resampler::new(config.sample_rate, rate);
    let n = frame_len(rate);
    let mut acc: Vec<i16> = Vec::with_capacity(n * 2);
    let mut mono = Vec::new();
    let mut resampled = Vec::new();
    let mut handle = move |data: &[f32]| {
        mono.clear();
        mono.extend(data.chunks(channels.max(1)).map(|c| c.iter().sum::<f32>() / c.len() as f32));
        resampled.clear();
        resampler.process(&mono, &mut resampled);
        acc.extend(resampled.iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16));
        while acc.len() >= n {
            let frame: Vec<i16> = acc.drain(..n).collect();
            let _ = tx.try_send(frame);
        }
    };
    let ok_err = ok.clone();
    let err = move |e: cpal::Error| {
        log::warn!("audio input error: {e}");
        ok_err.store(false, Ordering::Relaxed);
    };
    fn build<T: SizedSample>(
        device: &cpal::Device,
        config: cpal::StreamConfig,
        mut handle: impl FnMut(&[f32]) + Send + 'static,
        err: impl FnMut(cpal::Error) + Send + 'static,
    ) -> Result<cpal::Stream, cpal::Error>
    where
        f32: FromSample<T>,
    {
        let mut buf = Vec::new();
        device.build_input_stream::<T, _, _>(
            config,
            move |data: &[T], _| {
                buf.clear();
                buf.extend(data.iter().map(|s| s.to_sample::<f32>()));
                handle(&buf);
            },
            err,
            None,
        )
    }
    let stream = match supported.sample_format() {
        SampleFormat::F32 => device.build_input_stream::<f32, _, _>(config, move |d: &[f32], _| handle(d), err, None),
        SampleFormat::I16 => build::<i16>(&device, config, handle, err),
        SampleFormat::I32 => build::<i32>(&device, config, handle, err),
        SampleFormat::U16 => build::<u16>(&device, config, handle, err),
        other => return Err(format!("unsupported input sample format {other:?}")),
    };
    stream.map_err(|e| e.to_string())
}

/// A running playback with bounded latency. Stops on drop.
pub struct Playback {
    stop: Arc<AtomicBool>,
    ok: Arc<AtomicBool>,
    tx: Sender<Vec<i16>>,
}

impl Playback {
    pub fn start(dev: &AudioDevice, rate: u32, max_latency: Duration) -> std::io::Result<Playback> {
        let (tx, rx) = bounded::<Vec<i16>>(64);
        let stop = Arc::new(AtomicBool::new(false));
        let ok = Arc::new(AtomicBool::new(true));
        let (ready_tx, ready_rx) = bounded::<Result<(), String>>(1);
        let (dev, stop2, ok2) = (dev.clone(), stop.clone(), ok.clone());
        thread::Builder::new().name("audio-playback".into()).spawn(move || {
            let ring: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
            let (stream, out_rate) = match open_output(&dev, ring.clone(), ok2.clone()) {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            if let Err(e) = stream.play() {
                let _ = ready_tx.send(Err(e.to_string()));
                return;
            }
            let _ = ready_tx.send(Ok(()));
            let max = (max_latency.as_secs_f64() * out_rate as f64) as usize;
            // A little silence first so the first packets don't underrun.
            ring.lock().extend(std::iter::repeat_n(0.0, out_rate as usize / 25));
            let mut resampler = Resampler::new(rate, out_rate);
            let mut out = Vec::new();
            while !stop2.load(Ordering::Relaxed) && ok2.load(Ordering::Relaxed) {
                let Ok(frame) = rx.recv_timeout(Duration::from_millis(100)) else { continue };
                let input: Vec<f32> = frame.iter().map(|s| *s as f32 / 32768.0).collect();
                out.clear();
                resampler.process(&input, &mut out);
                let mut r = ring.lock();
                r.extend(out.iter().copied());
                // Keep latency bounded: drop the oldest audio if we're ahead.
                let excess = r.len().saturating_sub(max.max(out.len() * 2));
                r.drain(..excess);
            }
            drop(stream);
        })?;
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(Playback { stop, ok, tx }),
            Ok(Err(e)) => Err(std::io::Error::other(e)),
            Err(_) => Err(std::io::Error::other("audio output did not start")),
        }
    }

    /// Queue a frame (never blocks; drops if the queue is full).
    pub fn push(&self, frame: Vec<i16>) {
        let _ = self.tx.try_send(frame);
    }

    pub fn alive(&mut self) -> bool {
        self.ok.load(Ordering::Relaxed)
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn open_output(dev: &AudioDevice, ring: Arc<Mutex<VecDeque<f32>>>, ok: Arc<AtomicBool>) -> Result<(cpal::Stream, u32), String> {
    let device = find(dev, false).ok_or_else(|| format!("audio output \"{dev}\" not found"))?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let config = supported.config();
    let rate = config.sample_rate;
    let channels = config.channels as usize;
    let err = move |e: cpal::Error| {
        log::warn!("audio output error: {e}");
        ok.store(false, Ordering::Relaxed);
    };
    fn build<T: SizedSample + FromSample<f32>>(
        device: &cpal::Device,
        config: cpal::StreamConfig,
        channels: usize,
        ring: Arc<Mutex<VecDeque<f32>>>,
        err: impl FnMut(cpal::Error) + Send + 'static,
    ) -> Result<cpal::Stream, cpal::Error> {
        device.build_output_stream::<T, _, _>(
            config,
            move |data: &mut [T], _| {
                let mut r = ring.lock();
                for frame in data.chunks_mut(channels.max(1)) {
                    let v = T::from_sample(r.pop_front().unwrap_or(0.0));
                    frame.fill(v);
                }
            },
            err,
            None,
        )
    }
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, config, channels, ring, err),
        SampleFormat::I16 => build::<i16>(&device, config, channels, ring, err),
        SampleFormat::I32 => build::<i32>(&device, config, channels, ring, err),
        SampleFormat::U16 => build::<u16>(&device, config, channels, ring, err),
        other => return Err(format!("unsupported output sample format {other:?}")),
    };
    stream.map(|s| (s, rate)).map_err(|e| e.to_string())
}

/// All input and output devices.
pub fn list_devices() -> Vec<DeviceEntry> {
    let host = cpal::default_host();
    let mut out = Vec::new();
    if let Ok(devs) = host.input_devices() {
        out.extend(devs.map(|d| {
            let n = device_name(&d);
            DeviceEntry { device: AudioDevice::Named(n.clone()), label: n, input: true }
        }));
    }
    if let Ok(devs) = host.output_devices() {
        out.extend(devs.map(|d| {
            let n = device_name(&d);
            DeviceEntry { device: AudioDevice::Named(n.clone()), label: n, input: false }
        }));
    }
    out
}

/// The system's default input device.
pub fn default_input() -> Option<AudioDevice> {
    cpal::default_host().default_input_device().map(|d| AudioDevice::Named(device_name(&d)))
}

/// The radio's USB codec ("USB Audio CODEC", a TI PCM2903B), if present.
pub fn radio_devices() -> (Option<AudioDevice>, Option<AudioDevice>) {
    let is_radio = |n: &str| {
        let n = n.to_lowercase();
        n.contains("usb audio codec") || n.contains("pcm2903")
    };
    let devs = list_devices();
    let pick = |input: bool| devs.iter().find(|d| d.input == input && is_radio(&d.label)).map(|d| d.device.clone());
    (pick(true), pick(false))
}
