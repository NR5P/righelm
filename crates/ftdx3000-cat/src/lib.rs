//! Yaesu FTDX3000 CAT protocol, RigHelm's language for every radio, and the
//! table of radios it can control.
//!
//! Pure data and parsing: no I/O. Based on the *FTDX3000 Series CAT Operation
//! Reference Book* plus field experience collected in Hamlib's newcat driver.

pub mod command;
pub mod frame;
pub mod dialect;
pub mod menu;
pub mod model;
pub mod models;
pub mod state;
pub mod types;

pub use model::{Family, RadioModel};

pub use command::{CmdInfo, Request, answer_key, classify, lookup};
pub use frame::Framer;
pub use state::{ChannelInfo, RadioState};
pub use types::{Meter, Mode};

/// `ID` answer value for the FTDX3000.
pub const RADIO_ID: u16 = 462;

/// Format a frequency Set command for VFO-A or VFO-B (the FTDX3000's
/// range; [`set_freq_cmd_for`] for another radio's).
pub fn set_freq_cmd(vfo_b: bool, hz: u64) -> String {
    set_freq_cmd_for(&model::FTDX3000, vfo_b, hz)
}

/// Format a frequency Set command within `model`'s range. Above 99.999999
/// MHz the frequency has nine digits.
pub fn set_freq_cmd_for(model: &RadioModel, vfo_b: bool, hz: u64) -> String {
    let hz = model.clamp_freq(hz);
    format!("F{}{hz:08};", if vfo_b { 'B' } else { 'A' })
}

/// Format a mode Set command (`MD0x;`) — the FTDX3000 only has one MD target.
pub fn set_mode_cmd(mode: Mode) -> String {
    format!("MD0{};", mode.code())
}
