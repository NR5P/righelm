//! Transmit audio from the computer's microphone (the "computer mic" switch).
//!
//! The radio takes phone audio from its front MIC jack or from its USB codec,
//! chosen per mode by the MIC SEL menus. Switching to the computer mic sets
//! those to USB and plays the microphone into the codec while the operator
//! transmits; switching back restores what the menus were.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

use crossbeam_channel::RecvTimeoutError;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::audio::{self, AudioDevice, Capture, Playback};
use crate::engine::{Rig, Source};

/// AM, FM and SSB MIC SEL menu items.
pub const MIC_SEL_MENUS: [u16; 3] = [53, 85, 103];
/// MIC SEL codes: FRONT (the radio's mic jack), DATA (rear jack), USB.
pub const MIC_SEL_FRONT: i32 = 0;
pub const MIC_SEL_USB: i32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TxAudioConfig {
    /// Talk through the computer's microphone instead of the radio's.
    pub computer_mic: bool,
    pub mic: AudioDevice,
    pub mic_gain: f32,
    /// MIC SEL values from before switching to the computer mic, restored
    /// when switching back: `(menu, code)`.
    pub saved_mic_sel: Vec<(u16, i32)>,
}

impl Default for TxAudioConfig {
    fn default() -> Self {
        Self { computer_mic: false, mic: AudioDevice::Default, mic_gain: 1.0, saved_mic_sel: Vec::new() }
    }
}

/// Microphone audio may go to the radio now: it is transmitting in a phone
/// mode, keyed by the operator (this program's MOX, the radio's own PTT,
/// footswitch or VOX, or a remote operator) and not by a program on a
/// virtual port or rigctld, which is most likely sending data.
///
/// `at_station`: this program owns the radio. A remote client sends its
/// microphone only for its own MOX, not for someone keying at the station.
pub fn mic_tx_allowed(rig: &Rig, at_station: bool) -> bool {
    {
        let s = rig.state();
        if !s.transmitting() || !s.tx_mode().is_some_and(|m| m.is_voice()) {
            return false;
        }
    }
    match rig.tx_source() {
        Some(Source::Gui) => true,
        _ if !at_station => false,
        None | Some(Source::Engine) => true,
        Some(Source::Client(c)) => c.starts_with("remote "),
        Some(Source::Poll) => false,
    }
}

/// The current MIC SEL code of `menu`, from the engine's cache.
pub fn mic_sel(rig: &Rig, menu: u16) -> Option<i32> {
    let ans = rig.cached(&format!("EX{menu:03}"))?;
    let item = ftdx3000_cat::menu::item(menu)?;
    match item.decode(ans.get(5..)?.trim_end_matches(';'))? {
        ftdx3000_cat::menu::Value::Int(v) => Some(v),
        _ => None,
    }
}

/// The Set command for MIC SEL `menu`.
pub fn mic_sel_cmd(menu: u16, code: i32) -> String {
    format!("EX{menu:03}{code};")
}

#[derive(Debug, Clone, Default)]
pub struct MicStatus {
    /// The microphone in use.
    pub mic: Option<String>,
    pub level_db: f32,
    /// Microphone audio is going to the radio right now.
    pub sending: bool,
    pub error: Option<String>,
}

/// Plays the computer's microphone into the radio's USB audio input while
/// [`mic_tx_allowed`]. Stops on drop.
pub struct MicBridge {
    stop: Arc<AtomicBool>,
    gain: Arc<AtomicU32>,
    status: Arc<Mutex<MicStatus>>,
}

impl MicBridge {
    pub fn start(rig: Rig, mic: AudioDevice, gain: f32) -> MicBridge {
        let stop = Arc::new(AtomicBool::new(false));
        let gain = Arc::new(AtomicU32::new(gain.to_bits()));
        let status = Arc::new(Mutex::new(MicStatus::default()));
        let (stop2, gain2, status2) = (stop.clone(), gain.clone(), status.clone());
        thread::Builder::new()
            .name("computer-mic".into())
            .spawn(move || {
                while !stop2.load(Ordering::Relaxed) {
                    if let Err(e) = run(&rig, &mic, &gain2, &status2, &stop2) {
                        *status2.lock() = MicStatus { error: Some(e), level_db: -100.0, ..Default::default() };
                        for _ in 0..20 {
                            if stop2.load(Ordering::Relaxed) {
                                break;
                            }
                            thread::sleep(Duration::from_millis(100));
                        }
                    }
                }
            })
            .expect("spawn computer-mic");
        MicBridge { stop, gain, status }
    }

    pub fn set_gain(&self, gain: f32) {
        self.gain.store(gain.to_bits(), Ordering::Relaxed);
    }

    pub fn status(&self) -> MicStatus {
        self.status.lock().clone()
    }
}

impl Drop for MicBridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn run(rig: &Rig, mic: &AudioDevice, gain: &AtomicU32, status: &Mutex<MicStatus>, stop: &AtomicBool) -> Result<(), String> {
    let rate = crate::remote::AUDIO_RATE;
    let mic = audio::computer_mic(mic)?;
    // Never fall back to the system output: that would be the PC speakers.
    let sink = audio::radio_devices().1.ok_or("the radio's sound card was not found (is the radio on and plugged in?)")?;
    let mut capture = Capture::start(&mic, rate).map_err(|e| format!("microphone {mic}: {e}"))?;
    let mut play = Playback::start(&sink, rate, Duration::from_millis(120)).map_err(|e| format!("radio audio input {sink}: {e}"))?;
    *status.lock() = MicStatus { mic: Some(audio::describe(&mic)), level_db: -100.0, ..Default::default() };
    while !stop.load(Ordering::Relaxed) {
        if !capture.alive() {
            return Err("microphone capture stopped".into());
        }
        if !play.alive() {
            return Err("the radio's audio input went away".into());
        }
        match capture.frames.recv_timeout(Duration::from_millis(200)) {
            Ok(mut frame) => {
                audio::apply_gain(&mut frame, f32::from_bits(gain.load(Ordering::Relaxed)));
                let level = audio::level_dbfs(&frame);
                let send = mic_tx_allowed(rig, true);
                if send {
                    play.push(frame);
                }
                let mut s = status.lock();
                s.level_db = level;
                s.sending = send;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err("microphone capture stopped".into()),
        }
    }
    Ok(())
}
