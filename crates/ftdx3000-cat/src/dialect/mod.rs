//! Translators between RigHelm's language (FTDX3000 CAT) and other radios'.
//!
//! A [`Dialect`] sits on the serial line of a radio that isn't an FTDX3000:
//! RigHelm writes FTDX3000 commands, the dialect turns each one into what
//! that radio understands, and turns what the radio sends back into
//! FTDX3000 answers. The engine above never knows the difference.
//!
//! The same translators also work the other way round ([`Emulator`]): a
//! virtual port or the simulator can *be* that radio to another program,
//! turning its commands into RigHelm's and RigHelm's answers into its own.

pub mod icom;
pub mod text;

use crate::model::{Proto, RadioModel};

/// What to do for one RigHelm command, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Out {
    /// Bytes for the radio. `reply`: the radio answers this one (every CI-V
    /// command; a text radio's reads), so nothing more is sent until it has
    /// answered or timed out.
    Radio { bytes: Vec<u8>, reply: bool },
    /// Answer RigHelm directly, without the radio (an FTDX3000 frame: `?;`
    /// for something this radio doesn't have, or a value it doesn't need
    /// asking for).
    Answer(String),
}

impl Out {
    pub fn text(s: impl Into<String>) -> Out {
        Out::Radio { bytes: s.into().into_bytes(), reply: false }
    }

    pub fn reject() -> Out {
        Out::Answer("?;".into())
    }
}

/// What came of one frame from the radio.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Back {
    /// RigHelm (FTDX3000) frames to pass up.
    pub frames: Vec<String>,
    /// It was the answer to the command being waited for.
    pub reply: bool,
}

impl Back {
    pub fn frames(frames: Vec<String>) -> Back {
        Back { frames, reply: false }
    }

    pub fn reply(frames: Vec<String>) -> Back {
        Back { frames, reply: true }
    }
}

/// Radio side: RigHelm commands in, radio frames out, and back.
pub trait Dialect: Send {
    /// One RigHelm command (a complete frame like `FA14074000;`).
    fn command(&mut self, frame: &str) -> Vec<Out>;
    /// Split bytes from the radio into its frames.
    fn split(&mut self, bytes: &[u8]) -> Vec<Vec<u8>>;
    /// One frame from the radio.
    fn answer(&mut self, frame: &[u8]) -> Back;
    /// The command being waited for got no answer.
    fn timeout(&mut self) -> Vec<String> {
        Vec::new()
    }
    /// How long to wait for an answer to an `Out::Radio { reply: true }`.
    fn reply_timeout_ms(&self) -> u64 {
        300
    }
}

/// Program side: the radio's commands in from a program, RigHelm commands
/// out; RigHelm answers in, the radio's answers out.
pub trait Emulator: Send {
    /// Split bytes from the program into the radio's frames.
    fn split(&mut self, bytes: &[u8]) -> Vec<Vec<u8>>;
    /// One command from the program.
    fn request(&mut self, frame: &[u8]) -> Request;
    /// RigHelm's answers to the reads of the last request, in order (`?;`
    /// for one RigHelm rejected or couldn't get), turned into what the
    /// program expects back.
    fn respond(&mut self, request: &Request, answers: &[Option<String>]) -> Vec<u8>;
    /// An unsolicited RigHelm frame (auto-information) as the radio would
    /// send it, if the program asked for those.
    fn unsolicited(&mut self, frame: &str) -> Option<Vec<u8>>;
}

/// A command from a program, as RigHelm commands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Request {
    /// Set commands to carry out first, in order.
    pub sets: Vec<String>,
    /// Then these reads; their answers go to [`Emulator::respond`].
    pub reads: Vec<String>,
    /// Bytes to answer at once (an acknowledgement, or a refusal), when
    /// there are no reads.
    pub immediate: Option<Vec<u8>>,
    /// The emulator's own note of what was asked (for `respond`).
    pub tag: String,
    /// The program turned auto-information on (`Some(true)`) or off.
    pub auto_info: Option<bool>,
}

/// The translator for a model, or `None` for the FTDX3000 (no translation).
pub fn for_model(model: &'static RadioModel) -> Option<Box<dyn Dialect>> {
    match model.proto {
        Proto::Native => None,
        Proto::Text(spec) => Some(Box::new(text::TextDialect::new(model, spec))),
        Proto::CiV(spec) => Some(Box::new(icom::Icom::new(model, spec))),
    }
}

/// An emulator that makes a program see `model`, or `None` for the FTDX3000
/// (RigHelm's own language needs none).
pub fn emulator_for(model: &'static RadioModel) -> Option<Box<dyn Emulator>> {
    match model.proto {
        Proto::Native => None,
        Proto::Text(spec) => Some(Box::new(text::TextEmulator::new(model, spec))),
        Proto::CiV(spec) => Some(Box::new(icom::IcomEmulator::new(model, spec))),
    }
}

/// Split `;`-terminated text frames (Yaesu, Kenwood), dropping control
/// characters like the radios do.
#[derive(Debug, Default, Clone)]
pub struct TextSplitter(crate::Framer);

impl TextSplitter {
    pub fn split(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.0.push(bytes).into_iter().map(String::into_bytes).collect()
    }
}

/// Parse a decimal number, allowing a leading `+`.
pub fn num<T: std::str::FromStr>(s: &str) -> Option<T> {
    s.trim().trim_start_matches('+').parse().ok()
}

/// Map a value on one meter calibration onto the FTDX3000's raw scale:
/// the reading `raw` means `cal(raw)`; find the FTDX3000 raw value that
/// means the same.
pub fn rescale(raw: u8, from: &[(u8, f32)], to: &[(u8, f32)]) -> u8 {
    let v = crate::types::interpolate(from, raw);
    if to.is_empty() {
        return raw;
    }
    if v <= to[0].1 {
        return to[0].0;
    }
    for w in to.windows(2) {
        let (r0, v0) = w[0];
        let (r1, v1) = w[1];
        if v <= v1 {
            let t = if v1 > v0 { (v - v0) / (v1 - v0) } else { 0.0 };
            return (r0 as f32 + t * (r1 as f32 - r0 as f32)).round() as u8;
        }
    }
    to[to.len() - 1].0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PO_CAL, S_METER_CAL};

    #[test]
    fn rescale_round_trips_on_the_same_scale() {
        for raw in [0u8, 12, 100, 130, 200, 255] {
            let r = rescale(raw, &S_METER_CAL, &S_METER_CAL);
            assert!((r as i32 - raw as i32).abs() <= 1, "{raw} -> {r}");
        }
        // 100 W on a linear 0-255 = 0-100 W meter is raw 161 on the FTDX3000's.
        assert_eq!(rescale(255, &[(0, 0.0), (255, 100.0)], &PO_CAL), 161);
    }
}
