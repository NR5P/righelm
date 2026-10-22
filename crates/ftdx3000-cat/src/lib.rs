//! Yaesu FTDX3000 CAT protocol.
//!
//! Pure data and parsing: no I/O. Based on the *FTDX3000 Series CAT Operation
//! Reference Book* plus field experience collected in Hamlib's newcat driver.

pub mod command;
pub mod frame;
pub mod menu;
pub mod state;
pub mod types;

pub use command::{CmdInfo, Request, answer_key, classify, lookup};
pub use frame::Framer;
pub use state::{ChannelInfo, RadioState};
pub use types::{Meter, Mode};

/// `ID` answer value for the FTDX3000.
pub const RADIO_ID: u16 = 462;

/// Format a frequency Set command for VFO-A or VFO-B.
pub fn set_freq_cmd(vfo_b: bool, hz: u64) -> String {
    let hz = hz.clamp(types::FREQ_MIN, types::FREQ_MAX);
    format!("F{}{hz:08};", if vfo_b { 'B' } else { 'A' })
}

/// Format a mode Set command (`MD0x;`) — the FTDX3000 only has one MD target.
pub fn set_mode_cmd(mode: Mode) -> String {
    format!("MD0{};", mode.code())
}
