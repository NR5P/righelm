//! Icom CI-V: binary frames `FE FE <to> <from> <cmd> [sub] [data] FD`.
//!
//! Radio side ([`Icom`]): every RigHelm command becomes one or more CI-V
//! commands, each answered by the radio (`FB` = done, `FA` = refused, or the
//! data asked for) before the next goes out. The radio's answers, and the
//! frequency/mode changes it broadcasts itself when "CI-V Transceive" is on,
//! become RigHelm answers.
//!
//! Program side ([`IcomEmulator`]): a program's CI-V commands become RigHelm
//! commands, and RigHelm's answers CI-V replies, so the simulator and the
//! virtual ports can be an Icom radio.
//!
//! What the radios have in common is here; what differs is in [`CivSpec`].
//! Icom has one "selected" VFO (A or B) that frequency and mode commands
//! act on, and commands `25`/`26` for the selected and the other one. RigHelm
//! keeps VFO-A selected unless told otherwise (`VS1;`, `FR4;`) and maps its
//! VFO-A/B onto them.

use std::collections::VecDeque;

use super::{Back, Dialect, Emulator, Out, Request};
use crate::command::{self, Request as CatRequest};
use crate::frame;
use crate::menu::{Kind, MenuItem};
use crate::model::RadioModel;
use crate::state::ChannelInfo;
use crate::types::{self, Mode};

/// The controller's (RigHelm's) CI-V address.
pub const CONTROLLER: u8 = 0xE0;

/// What differs between Icom radios.
#[derive(Debug)]
pub struct CivSpec {
    /// RigHelm mode -> Icom mode code and DATA on/off.
    pub modes: &'static [(Mode, u8, bool)],
    /// `RA0n` attenuator codes -> the value of command `11` (BCD, e.g.
    /// 0x20 for 20 dB).
    pub att: &'static [(u8, u8)],
    /// Watts at full RF power (`14 0A 02 55`).
    pub max_watts: f32,
    /// Meter calibrations: the radio's reading (0-255) -> S-meter dB
    /// relative to S9, PO % of full power, SWR ratio, ALC fraction of full
    /// scale, COMP dB, ID amps, VD volts.
    pub s_cal: &'static [(u8, f32)],
    pub po_cal: &'static [(u8, f32)],
    pub swr_cal: &'static [(u8, f32)],
    pub alc_cal: &'static [(u8, f32)],
    pub comp_cal: &'static [(u8, f32)],
    pub id_cal: &'static [(u8, f32)],
    pub vd_cal: &'static [(u8, f32)],
    /// `1A 05` item of "CI-V Transceive" (the radio reports front-panel
    /// changes), as its decimal digits (`0071` = 71).
    pub transceive: Option<u32>,
    /// Keyer memories `1A 02` (M1-M8) and their length.
    pub keyer_memories: u8,
    /// Voice TX memories `28 00` (T1-T8).
    pub voice_memories: u8,
    /// Antenna switch `12 00 0n` (ANT1/ANT2).
    pub antenna_switch: bool,
    /// APF with `16 32` and `14 05`.
    pub apf: bool,
    /// `16 32` has widths (01 WIDE, 02 MID, 03 NAR) instead of just ON;
    /// RigHelm's APF ON uses the last one the radio reported.
    pub apf_widths: bool,
    /// The APF frequency offset at `14 05 02 55` (and minus it at 00 00),
    /// in Hz.
    pub apf_hz: f32,
    /// Lock with `16 50`.
    pub lock: bool,
    /// Memory names in `1A 00` (characters), 0 = none.
    pub memory_name_len: usize,
    /// `VD` (VOX delay, ms) is this `1A 05` item: 00-20 = 0.0-2.0 s.
    pub vox_delay: Option<u32>,
    /// `KR` (keyer on/off) is this `1A 05` item: 00 straight key,
    /// 01 bug, 02 paddle (KR1 sets the paddle).
    pub keyer_type: Option<u32>,
    /// Menu items that are other CI-V commands than `1A 05` (front-panel
    /// settings like IP+): the item's `addr` and the command. Their
    /// addresses are 9000 and up, so they never clash with `1A 05` items.
    pub extra: &'static [(u32, &'static [u8])],
}

/// A menu item of an Icom radio: `addr` is the `1A 05` item as its
/// decimal digits (`1A 05 01 28` = 128), `width` twice its data bytes.
/// Its number is given by [`numbered`].
pub const fn item(addr: u32, group: &'static str, name: &'static str, width: u8, kind: Kind) -> MenuItem {
    MenuItem { num: 0, addr, group, name, width, kind }
}

/// A menu table with each item numbered by its place (1, 2, ...).
pub const fn numbered<const N: usize>(raw: &[MenuItem]) -> [MenuItem; N] {
    let mut out = [raw[0]; N];
    let mut i = 0;
    while i < N {
        out[i] = raw[i];
        out[i].num = (i + 1) as u16;
        i += 1;
    }
    out
}

/// RigHelm's number of the menu item at `addr` (compile-time lookup for
/// the roles).
pub const fn num_of(menu: &[MenuItem], addr: u32) -> u16 {
    let mut i = 0;
    while i < menu.len() {
        if menu[i].addr == addr {
            return menu[i].num;
        }
        i += 1;
    }
    panic!("no such menu item")
}

/// Kinds the Icom menus share.
pub const OFF_ON: Kind = Kind::Choice(&["OFF", "ON"]);
/// `00 00`-`02 55` = 0-100 %.
pub const PERCENT: Kind = Kind::Percent { max: 255 };
/// Tone controls: 00-10 = -5..+5.
pub const TONE: Kind = Kind::Steps { max_code: 10, off_at_zero: false, first: -5, step: 1, unit: "" };
/// SSB transmit bandwidth: the high digit picks the upper edge, the low
/// digit the lower edge.
pub const TBW: Kind = Kind::Coded(&[
    (0, "100-2500 Hz"),
    (1, "200-2500 Hz"),
    (2, "300-2500 Hz"),
    (3, "500-2500 Hz"),
    (10, "100-2700 Hz"),
    (11, "200-2700 Hz"),
    (12, "300-2700 Hz"),
    (13, "500-2700 Hz"),
    (20, "100-2800 Hz"),
    (21, "200-2800 Hz"),
    (22, "300-2800 Hz"),
    (23, "500-2800 Hz"),
    (30, "100-2900 Hz"),
    (31, "200-2900 Hz"),
    (32, "300-2900 Hz"),
    (33, "500-2900 Hz"),
]);
pub const TX_DELAY: Kind = Kind::Choice(&["OFF", "10 ms", "15 ms", "20 ms", "25 ms", "30 ms"]);
pub const TOT: Kind = Kind::Choice(&["OFF", "3 min", "5 min", "10 min", "20 min", "30 min"]);
pub const MOD_INPUT: Kind = Kind::Choice(&["MIC", "ACC", "MIC,ACC", "USB", "MIC,USB"]);
pub const USB_LINE: Kind = Kind::Choice(&["OFF", "DTR", "RTS"]);
pub const NOTCH_SW: Kind = Kind::Choice(&["Auto", "Manual", "Auto/Manual"]);
pub const AVERAGING: Kind = Kind::Choice(&["OFF", "2", "3", "4"]);
pub const FILL: Kind = Kind::Choice(&["Fill", "Fill+Line"]);
pub const SPEED3: Kind = Kind::Choice(&["Slow", "Mid", "Fast"]);
pub const KEY_FRONT: Kind = Kind::Choice(&["M-CH UP", "M-CH DOWN", "PRESET", "Memory 1", "Memory 2", "Memory 3", "Memory 4"]);

/// Typical Icom meter scales (IC-7300 family).
pub const S_CAL: [(u8, f32); 3] = [(0, -54.0), (120, 0.0), (241, 60.0)];
pub const PO_CAL: [(u8, f32); 3] = [(0, 0.0), (143, 50.0), (213, 100.0)];
pub const SWR_CAL: [(u8, f32); 4] = [(0, 1.0), (48, 1.5), (80, 2.0), (120, 3.0)];
pub const ALC_CAL: [(u8, f32); 2] = [(0, 0.0), (120, 1.0)];
pub const COMP_CAL: [(u8, f32); 3] = [(0, 0.0), (130, 15.0), (210, 30.0)];
pub const ID_CAL: [(u8, f32); 4] = [(0, 0.0), (97, 10.0), (146, 15.0), (241, 25.0)];
pub const VD_CAL: [(u8, f32); 3] = [(0, 0.0), (13, 10.0), (241, 16.0)];

/// The IF filter width table of command `1A 03` (`(index, Hz)`).
pub fn widths(class: types::WidthClass, _narrow: bool) -> Vec<(u8, u32)> {
    use types::WidthClass::*;
    let top = match class {
        Ssb | Cw => 40,
        Rtty => 31,
        Fixed => return Vec::new(),
    };
    (0..=top).map(|i: u8| (i, if i < 10 { 50 * (i as u32 + 1) } else { 600 + 100 * (i as u32 - 10) })).collect()
}

// ------------------------------------------------------------------ frames

/// Split a CI-V byte stream into frames (`FE FE ... FD`).
#[derive(Debug, Default, Clone)]
pub struct CivSplitter {
    buf: Vec<u8>,
}

impl CivSplitter {
    pub fn split(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for &b in bytes {
            if b == 0xFE && self.buf.len() >= 2 && self.buf.last() != Some(&0xFE) {
                // A new preamble in the middle of a frame: the last one was cut.
                self.buf.clear();
            }
            if self.buf.is_empty() && b != 0xFE {
                continue;
            }
            // Collapse long runs of FE (wake-up preambles) to two.
            if b == 0xFE && self.buf.len() >= 2 && self.buf.iter().all(|&x| x == 0xFE) {
                continue;
            }
            self.buf.push(b);
            if b == 0xFD {
                let f = std::mem::take(&mut self.buf);
                if f.len() >= 5 && f[0] == 0xFE && f[1] == 0xFE {
                    out.push(f);
                }
            } else if self.buf.len() > 256 {
                self.buf.clear();
            }
        }
        out
    }
}

/// A parsed CI-V frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Civ {
    pub to: u8,
    pub from: u8,
    /// Command and everything after it, without the `FD`.
    pub body: Vec<u8>,
}

impl Civ {
    pub fn parse(f: &[u8]) -> Option<Civ> {
        if f.len() < 5 || f[0] != 0xFE || f[1] != 0xFE || *f.last()? != 0xFD {
            return None;
        }
        Some(Civ { to: f[2], from: f[3], body: f[4..f.len() - 1].to_vec() })
    }

    pub fn bytes(to: u8, from: u8, body: &[u8]) -> Vec<u8> {
        let mut v = vec![0xFE, 0xFE, to, from];
        v.extend_from_slice(body);
        v.push(0xFD);
        v
    }
}

/// `n` as `bytes` BCD bytes, most significant first (`255` -> `02 55`).
pub fn bcd(n: u64, bytes: usize) -> Vec<u8> {
    let mut v = vec![0u8; bytes];
    let mut n = n;
    for b in v.iter_mut().rev() {
        *b = ((n / 10 % 10) << 4 | n % 10) as u8;
        n /= 100;
    }
    v
}

/// BCD bytes, most significant first, as a number.
pub fn from_bcd(b: &[u8]) -> Option<u64> {
    let mut n = 0u64;
    for &x in b {
        let (hi, lo) = (x >> 4, x & 0x0F);
        if hi > 9 || lo > 9 {
            return None;
        }
        n = n * 100 + hi as u64 * 10 + lo as u64;
    }
    Some(n)
}

/// A frequency: 5 BCD bytes, least significant first.
pub fn freq_bytes(hz: u64) -> Vec<u8> {
    let mut v = bcd(hz, 5);
    v.reverse();
    v
}

pub fn freq_from(b: &[u8]) -> Option<u64> {
    let mut v = b.get(..5)?.to_vec();
    v.reverse();
    from_bcd(&v)
}

/// The digits of BCD bytes as text (`02 55` -> `"0255"`), the raw value of
/// a menu item in RigHelm.
pub fn bcd_text(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

/// The reverse of [`bcd_text`] (an odd number of digits gets a leading 0).
pub fn text_bcd(s: &str) -> Option<Vec<u8>> {
    let s = if s.len() % 2 == 1 { format!("0{s}") } else { s.to_string() };
    if !s.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

fn scale(v: i64, from_max: i64, to_max: i64) -> i64 {
    ((v.clamp(0, from_max) * to_max) as f64 / from_max as f64).round() as i64
}

// ---------------------------------------------------------- the commands

/// How a RigHelm value relates to the radio's.
#[derive(Debug, Clone, Copy)]
enum Conv {
    /// 0-255 both, `14 xx`.
    Same255,
    /// RigHelm 0..=max, the radio 0-255.
    Of255(i64),
    /// RigHelm 1-15, the radio 0-255.
    Nr,
    /// Watts, the radio 0-255 of full power.
    Watts,
    /// Words per minute, the radio 0-255 = 6-48 wpm.
    Wpm,
    /// RigHelm KP (0-75: 300+10n Hz), the radio 0-255 = 300-900 Hz.
    Pitch,
    /// The same small number on both sides (one byte).
    Byte,
    /// On/off, RigHelm `000`/`001`, one byte.
    Flag,
    /// RigHelm 0/1, the radio 0, 1 or 2 (break-in: semi or full is on).
    Bool,
}

/// RigHelm commands that are one Icom setting.
struct Simple {
    /// RigHelm Read (`AG0`): what comes before the value.
    canon: &'static str,
    /// Digits of the RigHelm value.
    digits: usize,
    /// The radio's command and subcommand.
    cmd: &'static [u8],
    /// Bytes of the radio's value.
    bytes: usize,
    conv: Conv,
}

const fn s(canon: &'static str, digits: usize, cmd: &'static [u8], bytes: usize, conv: Conv) -> Simple {
    Simple { canon, digits, cmd, bytes, conv }
}

const SIMPLE: &[Simple] = &[
    s("AG0", 3, &[0x14, 0x01], 2, Conv::Same255),
    s("RG0", 3, &[0x14, 0x02], 2, Conv::Same255),
    s("SQ0", 3, &[0x14, 0x03], 2, Conv::Same255),
    s("RL0", 2, &[0x14, 0x06], 2, Conv::Nr),
    s("KP", 2, &[0x14, 0x09], 2, Conv::Pitch),
    s("PC", 3, &[0x14, 0x0A], 2, Conv::Watts),
    s("MG", 3, &[0x14, 0x0B], 2, Conv::Of255(100)),
    s("KS", 3, &[0x14, 0x0C], 2, Conv::Wpm),
    s("PL", 3, &[0x14, 0x0E], 2, Conv::Of255(100)),
    s("NL0", 3, &[0x14, 0x12], 2, Conv::Of255(100)),
    s("ML1", 3, &[0x14, 0x15], 2, Conv::Of255(100)),
    s("VG", 3, &[0x14, 0x16], 2, Conv::Of255(100)),
    s("PA0", 1, &[0x16, 0x02], 1, Conv::Byte),
    s("NB0", 1, &[0x16, 0x22], 1, Conv::Byte),
    s("NR0", 1, &[0x16, 0x40], 1, Conv::Byte),
    s("BC0", 1, &[0x16, 0x41], 1, Conv::Byte),
    s("PR0", 1, &[0x16, 0x44], 1, Conv::Byte),
    s("ML0", 3, &[0x16, 0x45], 1, Conv::Flag),
    s("VX", 1, &[0x16, 0x46], 1, Conv::Byte),
    s("BI", 1, &[0x16, 0x47], 1, Conv::Bool),
    s("BP00", 3, &[0x16, 0x48], 1, Conv::Flag),
    s("RT", 1, &[0x21, 0x01], 1, Conv::Byte),
    s("XT", 1, &[0x21, 0x02], 1, Conv::Byte),
    s("SH0", 2, &[0x1A, 0x03], 1, Conv::Byte),
];

impl Simple {
    fn to_radio(&self, v: i64, spec: &CivSpec) -> Vec<u8> {
        let n = match self.conv {
            Conv::Same255 => v.clamp(0, 255),
            Conv::Of255(max) => scale(v, max, 255),
            Conv::Nr => scale(v.clamp(1, 15), 15, 255),
            Conv::Watts => ((v as f64 / spec.max_watts as f64 * 255.0).round() as i64).clamp(0, 255),
            Conv::Wpm => (((v.clamp(6, 48) - 6) as f64 * 255.0 / 42.0).round()) as i64,
            Conv::Pitch => {
                let hz = (300 + 10 * v).clamp(300, 900);
                (((hz - 300) as f64 * 255.0 / 600.0).round()) as i64
            }
            Conv::Byte | Conv::Bool | Conv::Flag => v,
        };
        bcd(n.max(0) as u64, self.bytes)
    }

    fn from_radio(&self, data: &[u8], spec: &CivSpec) -> Option<i64> {
        let n = from_bcd(data.get(..self.bytes)?)? as i64;
        Some(match self.conv {
            Conv::Same255 => n,
            Conv::Of255(max) => scale(n, 255, max),
            Conv::Nr => scale(n, 255, 15).max(1),
            Conv::Watts => (n as f64 * spec.max_watts as f64 / 255.0).round() as i64,
            Conv::Wpm => 6 + ((n as f64 * 42.0 / 255.0).round() as i64),
            Conv::Pitch => ((n as f64 * 600.0 / 255.0 / 10.0).round() as i64).clamp(0, 75),
            Conv::Bool => (n != 0) as i64,
            Conv::Byte | Conv::Flag => n,
        })
    }

    fn answer(&self, v: i64) -> String {
        format!("{}{:0w$};", self.canon, v, w = self.digits)
    }
}

fn simple_by_canon(f: &str) -> Option<&'static Simple> {
    SIMPLE.iter().filter(|s| f.len() > s.canon.len() && f[..s.canon.len()].eq_ignore_ascii_case(s.canon)).max_by_key(|s| s.canon.len())
}

fn simple_by_cmd(body: &[u8]) -> Option<&'static Simple> {
    SIMPLE.iter().find(|s| body.len() >= s.cmd.len() && body[..s.cmd.len()] == *s.cmd)
}

/// Meter readings `15 xx` and the RigHelm read for them.
const METERS: &[(&str, u8)] = &[("SM0", 0x02), ("RM1", 0x02), ("RM3", 0x14), ("RM4", 0x13), ("RM5", 0x11), ("RM6", 0x12), ("RM7", 0x16), ("RM8", 0x15)];

/// A meter reading of the radio on RigHelm's (the FTDX3000's) scale.
fn meter_to_canon(spec: &CivSpec, sub: u8, raw: u8) -> u8 {
    let lin = |cal: &[(u8, f32)], full: f32| -> u8 { ((types::interpolate(cal, raw) / full).clamp(0.0, 1.0) * 255.0).round() as u8 };
    match sub {
        0x02 => super::rescale(raw, spec.s_cal, &types::S_METER_CAL),
        0x11 => {
            let watts: Vec<(u8, f32)> = spec.po_cal.iter().map(|&(r, pct)| (r, pct * spec.max_watts / 100.0)).collect();
            super::rescale(raw, &watts, &types::PO_CAL)
        }
        0x12 => super::rescale(raw, spec.swr_cal, &types::SWR_CAL),
        0x13 => lin(spec.alc_cal, 1.0),
        0x14 => lin(spec.comp_cal, 30.0),
        0x15 => lin(spec.vd_cal, 15.0),
        0x16 => lin(spec.id_cal, 25.0),
        _ => raw,
    }
}

/// The reverse, for the emulator.
fn meter_from_canon(spec: &CivSpec, sub: u8, v: u8) -> u8 {
    let inv = |cal: &[(u8, f32)], full: f32| -> u8 {
        let target = v as f32 / 255.0 * full;
        // The radio reading with that value.
        (0..=255u8).min_by(|a, b| (types::interpolate(cal, *a) - target).abs().total_cmp(&(types::interpolate(cal, *b) - target).abs())).unwrap_or(0)
    };
    match sub {
        0x02 => super::rescale(v, &types::S_METER_CAL, spec.s_cal),
        0x11 => {
            let watts: Vec<(u8, f32)> = spec.po_cal.iter().map(|&(r, pct)| (r, pct * spec.max_watts / 100.0)).collect();
            super::rescale(v, &types::PO_CAL, &watts)
        }
        0x12 => super::rescale(v, &types::SWR_CAL, spec.swr_cal),
        0x13 => inv(spec.alc_cal, 1.0),
        0x14 => inv(spec.comp_cal, 30.0),
        0x15 => inv(spec.vd_cal, 15.0),
        0x16 => inv(spec.id_cal, 25.0),
        _ => v,
    }
}

/// One VFO's mode as the radio has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IcomMode {
    mode: u8,
    data: bool,
    filter: u8,
}

/// What the translator knows about the radio.
#[derive(Debug, Clone, Default)]
struct Known {
    /// VFO-B is the selected one.
    vfo_b: bool,
    freq: [Option<u64>; 2],
    mode: [Option<IcomMode>; 2],
    split: bool,
    rit_hz: i32,
    rit: bool,
    xit: bool,
    /// The transmitter was keyed by a RigHelm command.
    cat_keyed: bool,
    keyer: [Option<String>; 8],
    memory_channel: u16,
    ai: bool,
    /// Last APF width the radio reported (`16 32`).
    apf_width: u8,
}

impl Known {
    /// Index of RigHelm's VFO-A/B (false/true) in Icom's selected (0) /
    /// unselected (1) terms.
    fn sel(&self, b: bool) -> u8 {
        (b != self.vfo_b) as u8
    }
}

/// What to make of the answer to one CI-V command.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Want {
    /// A Set: `FB` expected.
    Done,
    /// Data, translated as it comes (the common case).
    Data,
    /// Data, answered as this RigHelm read (`TX` vs `MX` for `1C 00`, `RMn`).
    As(&'static str),
    /// Make this RigHelm answer from what is known once the answer is in.
    Compose(Compose),
    /// The band stack of a band: tune to it.
    BandStack,
    /// A memory channel's contents for `MR`.
    Memory(u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Compose {
    If,
    Oi,
}

struct Pend {
    want: Want,
    /// Pass a refusal (`FA`) up as `?;`.
    report: bool,
}

/// Radio side.
pub struct Icom {
    model: &'static RadioModel,
    spec: &'static CivSpec,
    addr: u8,
    split: CivSplitter,
    pending: VecDeque<Pend>,
    k: Known,
}

impl Icom {
    pub fn new(model: &'static RadioModel, spec: &'static CivSpec) -> Self {
        Self { model, spec, addr: model.civ_addr, split: CivSplitter::default(), pending: VecDeque::new(), k: Known::default() }
    }

    /// Talk to another address than the model's default (the radio's CI-V
    /// address was changed in its menu).
    pub fn with_address(mut self, addr: u8) -> Self {
        self.addr = addr;
        self
    }

    fn msg(&mut self, body: Vec<u8>, want: Want, report: bool) -> Out {
        self.pending.push_back(Pend { want, report });
        Out::Radio { bytes: Civ::bytes(self.addr, CONTROLLER, &body), reply: true }
    }

    fn set(&mut self, body: Vec<u8>) -> Out {
        self.msg(body, Want::Done, true)
    }

    fn read(&mut self, body: Vec<u8>) -> Out {
        self.msg(body, Want::Data, true)
    }

    fn mode_bytes(&self, m: Mode, keep: Option<IcomMode>) -> Option<IcomMode> {
        let &(_, mode, data) = self.spec.modes.iter().find(|(x, _, _)| *x == m)?;
        let filter = keep.map_or(1, |k| k.filter);
        Some(IcomMode { mode, data, filter })
    }

    fn mode_of(&self, m: IcomMode) -> Option<Mode> {
        self.spec.modes.iter().find(|(_, code, data)| *code == m.mode && *data == m.data).map(|x| x.0)
    }

    /// Set VFO `b`'s frequency.
    fn tune(&mut self, b: bool, hz: u64) -> Out {
        self.k.freq[b as usize] = Some(hz);
        let mut body = vec![0x25, self.k.sel(b)];
        body.extend(freq_bytes(hz));
        self.set(body)
    }

    fn set_mode(&mut self, b: bool, m: IcomMode) -> Out {
        self.k.mode[b as usize] = Some(m);
        let body = vec![0x26, self.k.sel(b), m.mode, m.data as u8, m.filter.max(1)];
        self.set(body)
    }

    /// RigHelm command -> what to send. Empty: refuse.
    fn down(&mut self, f: &str) -> Vec<Out> {
        let Some((code, p)) = frame::split(f) else { return Vec::new() };
        let read = matches!(command::classify(f), CatRequest::Read { .. });
        if let Some(sm) = simple_by_canon(f) {
            let rest = &f[sm.canon.len()..f.len() - 1];
            if rest.is_empty() {
                return vec![self.msg(sm.cmd.to_vec(), Want::Data, true)];
            }
            let Some(v) = super::num::<i64>(rest) else { return Vec::new() };
            let mut body = sm.cmd.to_vec();
            body.extend(sm.to_radio(v, self.spec));
            return vec![self.set(body)];
        }
        if read {
            if let Some(&(canon, sub)) = METERS.iter().find(|(c, _)| f.trim_end_matches(';').eq_ignore_ascii_case(c)) {
                return vec![self.msg(vec![0x15, sub], Want::As(canon), true)];
            }
        }
        let num = |s: &str| super::num::<i64>(s);
        match (code.as_str(), read) {
            ("FA" | "FB", true) => {
                let b = code == "FB";
                let sel = self.k.sel(b);
                vec![self.read(vec![0x25, sel])]
            }
            ("FA" | "FB", false) => {
                let Some(hz) = num(p) else { return Vec::new() };
                vec![self.tune(code == "FB", hz as u64)]
            }
            ("MD", true) => {
                let sel = self.k.sel(false);
                vec![self.read(vec![0x26, sel])]
            }
            ("MD", false) => {
                let Some(m) = p.get(1..2).and_then(|c| Mode::from_code(c.chars().next()?)) else { return Vec::new() };
                let Some(im) = self.mode_bytes(m, self.k.mode[0]) else { return Vec::new() };
                vec![self.set_mode(false, im)]
            }
            ("IF", true) => {
                let (a, _) = (self.k.sel(false), ());
                vec![
                    self.msg(vec![0x25, a], Want::Data, true),
                    self.msg(vec![0x26, a], Want::Data, false),
                    self.msg(vec![0x21, 0x00], Want::Data, false),
                    self.msg(vec![0x21, 0x01], Want::Data, false),
                    self.msg(vec![0x21, 0x02], Want::Compose(Compose::If), false),
                ]
            }
            ("OI", true) => {
                let b = self.k.sel(true);
                vec![self.msg(vec![0x25, b], Want::Data, true), self.msg(vec![0x26, b], Want::Compose(Compose::Oi), false)]
            }
            ("FT", true) => vec![self.read(vec![0x0F])],
            ("FT", false) => {
                // 0/2: transmit on VFO-A, 1/3: on VFO-B.
                let Some(n) = num(p) else { return Vec::new() };
                let tx_b = n % 2 == 1;
                self.k.split = tx_b != self.k.vfo_b;
                vec![self.set(vec![0x0F, self.k.split as u8])]
            }
            ("FR", true) => vec![Out::Answer(format!("FR{};", if self.k.vfo_b { 4 } else { 0 }))],
            ("VS", true) => vec![Out::Answer(format!("VS{};", self.k.vfo_b as u8))],
            ("FR" | "VS", false) => {
                let Some(n) = num(p) else { return Vec::new() };
                let b = if code == "FR" { n >= 4 } else { n == 1 };
                self.k.vfo_b = b;
                vec![self.set(vec![0x07, b as u8])]
            }
            ("AB", false) => vec![self.set(vec![0x07, 0xA0])],
            ("SV", false) => {
                self.k.freq.swap(0, 1);
                self.k.mode.swap(0, 1);
                vec![self.set(vec![0x07, 0xB0])]
            }
            ("BA", false) => {
                let (Some(hz), m) = (self.k.freq[1], self.k.mode[1]) else { return Vec::new() };
                let mut out = vec![self.tune(false, hz)];
                if let Some(m) = m {
                    out.push(self.set_mode(false, m));
                }
                out
            }
            ("TX" | "MX", true) => vec![self.msg(vec![0x1C, 0x00], Want::As(if code == "TX" { "TX" } else { "MX" }), true)],
            ("TX" | "MX", false) => {
                let on = p != "0";
                self.k.cat_keyed = on;
                vec![self.set(vec![0x1C, 0x00, on as u8])]
            }
            ("AC", true) => vec![self.read(vec![0x1C, 0x01])],
            ("AC", false) => {
                let Some(n) = p.get(2..3).and_then(|c| c.parse::<u8>().ok()) else { return Vec::new() };
                vec![self.set(vec![0x1C, 0x01, n.min(2)])]
            }
            ("PS", true) => vec![Out::Answer("PS1;".into())],
            ("PS", false) if p == "0" => vec![self.set(vec![0x18, 0x00])],
            ("PS", false) => {
                // Wake-up: a run of FE, then "ON".
                let mut bytes = vec![0xFE; 25];
                bytes.extend(Civ::bytes(self.addr, CONTROLLER, &[0x18, 0x01]));
                vec![Out::Radio { bytes, reply: false }]
            }
            ("ID", true) => vec![self.read(vec![0x19, 0x00])],
            ("AI", true) => vec![Out::Answer(format!("AI{};", self.k.ai as u8))],
            ("AI", false) => {
                self.k.ai = p == "1";
                match self.spec.transceive {
                    Some(item) => {
                        let mut body = vec![0x1A, 0x05];
                        body.extend(bcd(item as u64, 2));
                        body.push(self.k.ai as u8);
                        vec![self.msg(body, Want::Done, false)]
                    }
                    None => vec![Out::Answer(String::new())],
                }
            }
            ("BY", true) => vec![self.read(vec![0x15, 0x01])],
            ("GT", true) => vec![self.read(vec![0x16, 0x12])],
            ("GT", false) => match p.get(1..2).and_then(|c| c.parse::<u8>().ok()) {
                Some(n @ 1..=3) => vec![self.set(vec![0x16, 0x12, n])],
                _ => Vec::new(),
            },
            ("RA", true) => vec![self.read(vec![0x11])],
            ("RA", false) => {
                let Some(c) = p.get(1..2).and_then(|c| c.parse::<u8>().ok()) else { return Vec::new() };
                let Some(&(_, v)) = self.spec.att.iter().find(|(x, _)| *x == c) else { return Vec::new() };
                vec![self.set(vec![0x11, v])]
            }
            ("AN", true) if self.spec.antenna_switch => vec![self.read(vec![0x12])],
            ("AN", false) if self.spec.antenna_switch => {
                let Some(a) = p.get(1..2).and_then(|c| c.parse::<u8>().ok()).filter(|a| (1..=2).contains(a)) else { return Vec::new() };
                vec![self.set(vec![0x12, a - 1])]
            }
            ("CO", true) if self.spec.apf && p == "00" => vec![self.msg(vec![0x16, 0x32], Want::As("CO00"), true)],
            ("CO", true) if self.spec.apf && p == "02" => vec![self.msg(vec![0x14, 0x05], Want::As("CO02"), true)],
            ("CO", false) if self.spec.apf => {
                let (Some(which), Some(v)) = (p.get(..2), p.get(2..).and_then(num)) else { return Vec::new() };
                match which {
                    "00" if v == 0 || v == 2 => vec![self.set(vec![0x16, 0x32, if v == 2 { self.k.apf_width.max(1) } else { 0 }])],
                    "02" => {
                        let mut body = vec![0x14, 0x05];
                        body.extend(bcd(apf_to_radio(self.spec, v), 2));
                        vec![self.set(body)]
                    }
                    _ => Vec::new(),
                }
            }
            ("LK", true) if self.spec.lock => vec![self.read(vec![0x16, 0x50])],
            ("LK", false) if self.spec.lock => match p {
                "0" | "1" => vec![self.set(vec![0x16, 0x50, p.parse::<u8>().unwrap()])],
                "4" | "5" | "6" | "7" => vec![self.set(vec![0x16, 0x50, (p.parse::<u8>().unwrap() & 1)])],
                _ => Vec::new(),
            },
            ("RU" | "RD", false) => {
                let Some(v) = num(p) else { return Vec::new() };
                let hz = (self.k.rit_hz + if code == "RU" { v as i32 } else { -(v as i32) }).clamp(-9999, 9999);
                vec![self.rit(hz)]
            }
            ("RC", false) => vec![self.rit(0)],
            ("MC", true) => vec![Out::Answer(format!("MC{:03};", self.k.memory_channel.max(1)))],
            ("MC", false) => {
                let Some(ch) = num(p).filter(|c| self.model.caps.memory.channels().contains(&(*c as u16))) else { return Vec::new() };
                self.k.memory_channel = ch as u16;
                let mut body = vec![0x08];
                body.extend(mem_bytes(ch as u16));
                vec![self.set(body)]
            }
            ("MA", false) => vec![self.set(vec![0x0A])],
            ("AM", false) => vec![self.set(vec![0x09])],
            ("MR", true) => {
                let Some(ch) = num(p) else { return Vec::new() };
                let mut body = vec![0x1A, 0x00];
                body.extend(mem_bytes(ch as u16));
                vec![self.msg(body, Want::Memory(ch as u16), true)]
            }
            ("MW", false) => {
                let Some(info) = ChannelInfo::parse(p) else { return Vec::new() };
                let Some(m) = info.mode.and_then(|m| self.mode_bytes(m, None)) else { return Vec::new() };
                let mut body = vec![0x1A, 0x00];
                body.extend(mem_bytes(info.channel));
                body.extend(memory_body(&info, m, self.spec));
                vec![self.set(body)]
            }
            ("KM", true) => {
                let Some(n) = num(p).filter(|n| (1..=self.spec.keyer_memories as i64).contains(n)) else { return Vec::new() };
                vec![self.read(vec![0x1A, 0x02, n as u8])]
            }
            ("KM", false) => {
                let Some(n) = p.get(..1).and_then(|c| c.parse::<u8>().ok()).filter(|n| (1..=self.spec.keyer_memories).contains(n)) else { return Vec::new() };
                let text = p[1..].trim_end_matches('}').to_string();
                self.k.keyer[n as usize - 1] = Some(text.clone());
                let mut body = vec![0x1A, 0x02, n];
                // Spaces clear a memory.
                body.extend(if text.is_empty() { b" ".to_vec() } else { text.into_bytes() });
                vec![self.set(body)]
            }
            ("KY", false) => {
                // Play a keyer memory: send its text.
                let Some(n) = p.get(..1).and_then(|c| c.parse::<usize>().ok()).filter(|n| (1..=8).contains(n)) else { return Vec::new() };
                let Some(text) = self.k.keyer[n - 1].clone().filter(|t| !t.is_empty()) else { return Vec::new() };
                let mut out = Vec::new();
                for chunk in text.as_bytes().chunks(30) {
                    let mut body = vec![0x17];
                    body.extend_from_slice(chunk);
                    out.push(self.set(body));
                }
                out
            }
            ("PB", true) => vec![Out::Answer("PB00;".into())],
            ("PB", false) => {
                let Some(n) = p.get(1..2).and_then(|c| c.parse::<u8>().ok()).filter(|n| *n <= self.spec.voice_memories) else { return Vec::new() };
                vec![self.set(vec![0x28, 0x00, n])]
            }
            ("EX", _) => {
                let Some(num) = p.get(..3).and_then(|n| n.parse::<u16>().ok()) else { return Vec::new() };
                let Some(item) = self.model.menu_item(num) else { return Vec::new() };
                let mut body = menu_cmd(self.spec, item.addr);
                if read {
                    return vec![self.msg(body, Want::As("EX"), true)];
                }
                let Some(data) = menu_data(item, &p[3..]) else { return Vec::new() };
                body.extend(data);
                vec![self.set(body)]
            }
            ("VD", _) if self.spec.vox_delay.is_some() => {
                let mut body = menu_cmd(self.spec, self.spec.vox_delay.unwrap_or(0));
                if read {
                    return vec![self.msg(body, Want::As("VD"), true)];
                }
                let Some(ms) = num(p) else { return Vec::new() };
                body.extend(bcd(((ms as f64 / 100.0).round() as i64).clamp(0, 20) as u64, 1));
                vec![self.set(body)]
            }
            ("KR", _) if self.spec.keyer_type.is_some() => {
                let mut body = menu_cmd(self.spec, self.spec.keyer_type.unwrap_or(0));
                if read {
                    return vec![self.msg(body, Want::As("KR"), true)];
                }
                body.push(if p == "0" { 0x00 } else { 0x02 });
                vec![self.set(body)]
            }
            ("BS", false) => {
                let Some(band) = num(p).filter(|b| self.model.bands.iter().any(|x| x.code as i64 == *b)) else { return Vec::new() };
                vec![self.msg(vec![0x1A, 0x01, bcd(band as u64, 1)[0], 0x01], Want::BandStack, true)]
            }
            ("BU" | "BD", false) => {
                let bands: Vec<&types::Band> = self.model.bands.iter().filter(|b| b.label != "GEN").collect();
                if bands.is_empty() {
                    return Vec::new();
                }
                let cur = self.k.freq[0].and_then(|f| bands.iter().position(|b| (b.lo..=b.hi).contains(&f)));
                let n = bands.len();
                let next = match (cur, code == "BU") {
                    (Some(i), true) => (i + 1) % n,
                    (Some(i), false) => (i + n - 1) % n,
                    (None, _) => 0,
                };
                vec![self.msg(vec![0x1A, 0x01, bcd(bands[next].code as u64, 1)[0], 0x01], Want::BandStack, true)]
            }
            _ => Vec::new(),
        }
    }

    fn rit(&mut self, hz: i32) -> Out {
        self.k.rit_hz = hz;
        let mut d = bcd(hz.unsigned_abs() as u64, 2);
        d.reverse();
        let mut body = vec![0x21, 0x00];
        body.extend(d);
        body.push((hz < 0) as u8);
        self.set(body)
    }

    /// RigHelm frames for data from the radio (an answer or a broadcast).
    fn up(&mut self, body: &[u8], want: Option<&Want>) -> Vec<String> {
        let Some(&cmd) = body.first() else { return Vec::new() };
        let d = &body[1..];
        // Menu items that are commands of their own.
        if let Some((item, n)) = extra_item(self.model, self.spec, body) {
            return if body.len() > n { vec![format!("EX{:03}{};", item.num, menu_raw(item, &body[n..]))] } else { Vec::new() };
        }
        // Commands with a fixed subcommand.
        if let Some(sm) = simple_by_cmd(body) {
            let Some(v) = sm.from_radio(&body[sm.cmd.len()..], self.spec) else { return Vec::new() };
            return vec![sm.answer(v)];
        }
        let freq_frames = |k: &Known, b: bool| -> Vec<String> {
            match k.freq[b as usize] {
                Some(hz) => vec![format!("F{}{hz:08};", if b { 'B' } else { 'A' })],
                None => Vec::new(),
            }
        };
        match cmd {
            // Frequency of the selected VFO.
            0x00 | 0x03 => {
                let Some(hz) = freq_from(d) else { return Vec::new() };
                let b = self.k.vfo_b;
                self.k.freq[b as usize] = Some(hz);
                freq_frames(&self.k, b)
            }
            // Mode of the selected VFO.
            0x01 | 0x04 => {
                let (Some(&mode), filter) = (d.first(), d.get(1).copied().unwrap_or(1)) else { return Vec::new() };
                let b = self.k.vfo_b;
                let data = self.k.mode[b as usize].is_some_and(|m| m.data && m.mode == mode);
                let m = IcomMode { mode, data, filter };
                self.k.mode[b as usize] = Some(m);
                self.mode_frames(b)
            }
            0x25 => {
                let (Some(&sel), Some(hz)) = (d.first(), d.get(1..).and_then(freq_from)) else { return Vec::new() };
                let b = (sel == 1) != self.k.vfo_b;
                self.k.freq[b as usize] = Some(hz);
                freq_frames(&self.k, b)
            }
            0x26 => {
                let (Some(&sel), Some(&mode)) = (d.first(), d.get(1)) else { return Vec::new() };
                let b = (sel == 1) != self.k.vfo_b;
                let m = IcomMode { mode, data: d.get(2).is_some_and(|x| *x != 0), filter: d.get(3).copied().unwrap_or(1) };
                self.k.mode[b as usize] = Some(m);
                self.mode_frames(b)
            }
            0x0F => {
                let Some(&on) = d.first() else { return Vec::new() };
                self.k.split = on == 1;
                vec![format!("FT{};", (self.k.split != self.k.vfo_b) as u8)]
            }
            0x11 => {
                let Some(&v) = d.first() else { return Vec::new() };
                let c = self.spec.att.iter().find(|(_, x)| *x == v).map_or(0, |x| x.0);
                vec![format!("RA0{c};")]
            }
            0x12 if self.spec.antenna_switch => {
                let Some(&a) = d.first() else { return Vec::new() };
                vec![format!("AN0{}0;", a + 1)]
            }
            0x15 => {
                if d.first() == Some(&0x01) {
                    let Some(&v) = d.get(1) else { return Vec::new() };
                    return vec![format!("BY{}0;", (v != 0) as u8)];
                }
                let (Some(&sub), Some(v)) = (d.first(), d.get(1..3).and_then(from_bcd)) else { return Vec::new() };
                let c = meter_to_canon(self.spec, sub, v.min(255) as u8);
                match want {
                    Some(Want::As(canon)) => vec![format!("{canon}{c:03};")],
                    _ if sub == 0x02 => vec![format!("SM0{c:03};")],
                    _ => METERS.iter().filter(|(n, s)| *s == sub && n.starts_with("RM")).map(|(n, _)| format!("{n}{c:03};")).take(1).collect(),
                }
            }
            0x16 => match (d.first(), d.get(1)) {
                (Some(0x12), Some(&v)) => vec![format!("GT0{v};")],
                (Some(0x32), Some(&v)) => {
                    if v != 0 && self.spec.apf_widths {
                        self.k.apf_width = v;
                    }
                    vec![format!("CO00{:02};", if v != 0 { 2 } else { 0 })]
                }
                (Some(0x50), Some(&v)) => vec![format!("LK{};", 4 + (v & 1))],
                _ => Vec::new(),
            },
            0x14 if d.first() == Some(&0x05) => {
                let Some(v) = d.get(1..3).and_then(from_bcd) else { return Vec::new() };
                vec![format!("CO02{:02};", apf_from_radio(self.spec, v))]
            }
            0x19 => {
                let addr = d.get(1).copied().unwrap_or(self.addr);
                vec![format!("ID{:04};", addr)]
            }
            0x1C => match (d.first(), d.get(1)) {
                (Some(0x00), Some(&on)) => {
                    let tx = if on == 0 { 0 } else if self.k.cat_keyed { 1 } else { 2 };
                    match want {
                        Some(Want::As("MX")) => vec![format!("MX{};", (on != 0 && self.k.cat_keyed) as u8)],
                        _ => vec![format!("TX{tx};")],
                    }
                }
                (Some(0x01), Some(&v)) => vec![format!("AC00{};", v.min(2))],
                _ => Vec::new(),
            },
            0x21 => match d.first() {
                Some(0x00) => {
                    let (Some(lo), Some(hi), Some(&sign)) = (d.get(1), d.get(2), d.get(3)) else { return Vec::new() };
                    let Some(v) = from_bcd(&[*hi, *lo]) else { return Vec::new() };
                    self.k.rit_hz = if sign == 1 { -(v as i32) } else { v as i32 };
                    Vec::new()
                }
                _ => Vec::new(),
            },
            0x1A => match d.first() {
                Some(0x02) => {
                    let Some(&n) = d.get(1) else { return Vec::new() };
                    let text = String::from_utf8_lossy(&d[2..]).trim_end().to_string();
                    if (1..=8).contains(&n) {
                        self.k.keyer[n as usize - 1] = Some(text.clone());
                    }
                    if n > 5 {
                        return Vec::new();
                    }
                    vec![format!("KM{n}{text}}};")]
                }
                Some(0x05) => {
                    let Some(addr) = d.get(1..3).and_then(from_bcd) else { return Vec::new() };
                    match want {
                        Some(Want::As("VD")) => {
                            let Some(n) = d.get(3..4).and_then(from_bcd) else { return Vec::new() };
                            return vec![format!("VD{:04};", n * 100)];
                        }
                        Some(Want::As("KR")) => return vec![format!("KR{};", (d.get(3).copied().unwrap_or(0) != 0) as u8)],
                        _ => {}
                    }
                    let Some(item) = self.model.menu.iter().find(|m| m.addr as u64 == addr) else { return Vec::new() };
                    vec![format!("EX{:03}{};", item.num, menu_raw(item, &d[3..]))]
                }
                Some(0x06) => {
                    // DATA mode of the selected VFO.
                    let Some(&on) = d.get(1) else { return Vec::new() };
                    let b = self.k.vfo_b;
                    if let Some(m) = self.k.mode[b as usize].as_mut() {
                        m.data = on != 0;
                    }
                    self.mode_frames(b)
                }
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// RigHelm's view of VFO `b`'s mode: `MD0` for VFO-A.
    fn mode_frames(&self, b: bool) -> Vec<String> {
        if b {
            return Vec::new();
        }
        match self.k.mode[0].and_then(|m| self.mode_of(m)) {
            Some(m) => vec![format!("MD0{};", m.code())],
            None => Vec::new(),
        }
    }

    fn compose(&self, c: &Compose) -> Vec<String> {
        let info = |b: bool| -> Option<ChannelInfo> {
            Some(ChannelInfo {
                channel: self.k.memory_channel.max(1),
                freq: self.k.freq[b as usize]?,
                clar_offset: self.k.rit_hz,
                rx_clar: self.k.rit && !b,
                tx_clar: self.k.xit && !b,
                mode: self.k.mode[b as usize].and_then(|m| self.mode_of(m)),
                ..Default::default()
            })
        };
        match c {
            Compose::If => info(false).map(|i| vec![i.to_answer("IF")]).unwrap_or_default(),
            Compose::Oi => info(true).map(|i| vec![i.to_answer("OI")]).unwrap_or_default(),
        }
    }
}

/// RigHelm's APF frequency (`CO02`: 0-50 = -250..+250 Hz) as `14 05`.
fn apf_to_radio(spec: &CivSpec, v: i64) -> u64 {
    let hz = (v.clamp(0, 50) - 25) as f32 * 10.0;
    (128.0 + hz * 127.0 / spec.apf_hz).round().clamp(0.0, 255.0) as u64
}

fn apf_from_radio(spec: &CivSpec, n: u64) -> i64 {
    let hz = (n as f32 - 128.0) * spec.apf_hz / 127.0;
    ((hz / 10.0).round() as i64 + 25).clamp(0, 50)
}

/// The command that reads a menu item (and, with the value after it, sets
/// it): `1A 05 xx xx`, or the item's own command for the extra ones.
fn menu_cmd(spec: &CivSpec, addr: u32) -> Vec<u8> {
    match spec.extra.iter().find(|(a, _)| *a == addr) {
        Some((_, cmd)) => cmd.to_vec(),
        None => {
            let mut body = vec![0x1A, 0x05];
            body.extend(bcd(addr as u64, 2));
            body
        }
    }
}

/// The extra menu item (not `1A 05`) a CI-V frame is about, and the length
/// of its command.
fn extra_item(model: &RadioModel, spec: &CivSpec, body: &[u8]) -> Option<(&'static crate::menu::MenuItem, usize)> {
    let (addr, cmd) = spec.extra.iter().find(|(_, c)| body.len() >= c.len() && body[..c.len()] == **c)?;
    Some((model.menu.iter().find(|m| m.addr == *addr)?, cmd.len()))
}

/// A menu item's value as RigHelm's raw text (BCD digits, or text).
fn menu_raw(item: &crate::menu::MenuItem, data: &[u8]) -> String {
    match item.kind {
        Kind::Text { max } => format!("{:<max$}", String::from_utf8_lossy(data)),
        _ => bcd_text(data),
    }
}

/// RigHelm's raw text of a menu item as the radio's data bytes.
fn menu_data(item: &crate::menu::MenuItem, raw: &str) -> Option<Vec<u8>> {
    match item.kind {
        Kind::Text { .. } => Some(raw.trim_end().as_bytes().to_vec()),
        _ => text_bcd(raw.trim()),
    }
}

/// A memory channel number as `1A 00`/`08` want it (`00 05`; P1/P2 =
/// `01 00`/`01 01` are RigHelm's 100/101).
fn mem_bytes(ch: u16) -> Vec<u8> {
    match ch {
        100 => vec![0x01, 0x00],
        101 => vec![0x01, 0x01],
        c => bcd(c as u64, 2),
    }
}

/// The contents of a memory channel for `1A 00` (after the channel).
fn memory_body(info: &ChannelInfo, m: IcomMode, spec: &CivSpec) -> Vec<u8> {
    let mut v = vec![0x00];
    v.extend(freq_bytes(info.freq));
    v.push(m.mode);
    v.push(m.filter.max(1));
    v.push((m.data as u8) << 4 | info.ctcss.min(2));
    // Repeater tone and tone squelch: 88.5 Hz.
    v.extend([0x00, 0x08, 0x85, 0x00, 0x08, 0x85]);
    // The same again for transmit (split).
    let first = v[1..].to_vec();
    v.extend(&first[..first.len()]);
    if spec.memory_name_len > 0 {
        v.extend(std::iter::repeat_n(b' ', spec.memory_name_len));
    }
    v
}

impl Dialect for Icom {
    fn command(&mut self, f: &str) -> Vec<Out> {
        let key = command::answer_key(f).unwrap_or_default();
        let has = !key.is_empty() && self.model.has_key(&key);
        let out = if has { self.down(f) } else { Vec::new() };
        if out.is_empty() {
            return vec![Out::reject()];
        }
        out.into_iter().filter(|o| !matches!(o, Out::Answer(a) if a.is_empty())).collect()
    }

    fn split(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.split.split(bytes)
    }

    fn answer(&mut self, raw: &[u8]) -> Back {
        let Some(f) = Civ::parse(raw) else { return Back::default() };
        // Our own command echoed back (USB echo, or the CI-V bus).
        if f.from == CONTROLLER {
            return Back::default();
        }
        if f.from != self.addr {
            return Back::default();
        }
        // Broadcasts (CI-V Transceive): the front panel changed something.
        if f.to == 0x00 {
            return Back::frames(self.up(&f.body, None));
        }
        let Some(p) = self.pending.pop_front() else { return Back::frames(self.up(&f.body, None)) };
        match f.body.as_slice() {
            [0xFB] => {
                let frames = match &p.want {
                    Want::Compose(c) => self.compose(c),
                    _ => Vec::new(),
                };
                Back::reply(frames)
            }
            [0xFA] => {
                let mut frames = if p.report { vec!["?;".to_string()] } else { Vec::new() };
                if let Want::Compose(c) = &p.want {
                    frames.extend(self.compose(c));
                }
                Back::reply(frames)
            }
            body => {
                let mut frames = Vec::new();
                match &p.want {
                    Want::BandStack => {
                        // 1A 01 <band> <reg> <frequency 5> <mode> <filter> <data/tone> ...
                        let d = &body[4.min(body.len())..];
                        if let (Some(hz), Some(&mode)) = (freq_from(d), d.get(5)) {
                            let filter = d.get(6).copied().unwrap_or(1);
                            let data = d.get(7).is_some_and(|x| x >> 4 != 0);
                            let mut b = Back::reply(Vec::new());
                            b.then = vec![self.tune(false, hz), self.set_mode(false, IcomMode { mode, data, filter })];
                            return b;
                        }
                        frames.push("?;".into());
                    }
                    Want::Memory(ch) => frames.extend(self.memory_answer(*ch, body)),
                    w => {
                        frames = self.up(body, Some(w));
                        if let Want::Compose(c) = w {
                            frames.extend(self.compose(c));
                        }
                        if p.report && frames.is_empty() && matches!(w, Want::As(_) | Want::Data) && !matches!(body.first(), Some(0x21)) {
                            frames.push("?;".into());
                        }
                    }
                }
                Back::reply(frames)
            }
        }
    }

    fn timeout(&mut self) -> Vec<String> {
        match self.pending.pop_front() {
            Some(Pend { want: Want::Compose(c), .. }) => self.compose(&c),
            _ => Vec::new(),
        }
    }

    fn reply_timeout_ms(&self) -> u64 {
        250
    }
}

impl Icom {
    fn memory_answer(&self, ch: u16, body: &[u8]) -> Vec<String> {
        // 1A 00 <ch 2> <split/select> <frequency 5> <mode> <filter> <data/tone> ...
        let d = &body[3.min(body.len())..];
        if d.first() == Some(&0xFF) || d.len() < 9 {
            return vec!["?;".into()];
        }
        let (Some(hz), Some(&mode)) = (freq_from(&d[1..]), d.get(6)) else { return vec!["?;".into()] };
        let filter = d.get(7).copied().unwrap_or(1);
        let dt = d.get(8).copied().unwrap_or(0);
        let m = IcomMode { mode, data: dt >> 4 != 0, filter };
        let info = ChannelInfo { channel: ch, freq: hz, mode: self.mode_of(m), mem_mode: 1, ctcss: dt & 0x0F, ..Default::default() };
        vec![info.to_answer("MR")]
    }
}

// ---------------------------------------------------------------- emulator

/// Program side: be an Icom radio.
pub struct IcomEmulator {
    model: &'static RadioModel,
    spec: &'static CivSpec,
    addr: u8,
    split: CivSplitter,
    /// VFO-B is the selected one.
    vfo_b: bool,
    /// The last command, for the reply's command and subcommand.
    last: Vec<u8>,
    /// A RigHelm set to carry out after answering (DATA on/off).
    pending_set: Option<String>,
}

impl IcomEmulator {
    pub fn new(model: &'static RadioModel, spec: &'static CivSpec) -> Self {
        Self { model, spec, addr: model.civ_addr, split: CivSplitter::default(), vfo_b: false, last: Vec::new(), pending_set: None }
    }

    fn reply(&self, body: &[u8]) -> Vec<u8> {
        Civ::bytes(CONTROLLER, self.addr, body)
    }

    fn ok(&self) -> Vec<u8> {
        self.reply(&[0xFB])
    }

    fn ng(&self) -> Vec<u8> {
        self.reply(&[0xFA])
    }

    fn vfo(&self, sel: u8) -> bool {
        (sel == 1) != self.vfo_b
    }

    fn icom_mode(&self, m: Mode) -> Option<(u8, bool)> {
        self.spec.modes.iter().find(|x| x.0 == m).map(|x| (x.1, x.2))
    }

    fn canon_mode(&self, mode: u8, data: bool) -> Option<Mode> {
        self.spec.modes.iter().find(|x| x.1 == mode && x.2 == data).or_else(|| self.spec.modes.iter().find(|x| x.1 == mode)).map(|x| x.0)
    }
}

impl Emulator for IcomEmulator {
    fn split(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.split.split(bytes)
    }

    fn request(&mut self, raw: &[u8]) -> Request {
        let mut req = Request::default();
        let Some(f) = Civ::parse(raw) else { return req };
        if f.to != self.addr && f.to != 0x00 {
            return req;
        }
        self.last = f.body.clone();
        let b = &f.body;
        let Some(&cmd) = b.first() else { return req };
        let d = &b[1..];
        let ok = |mut r: Request, e: &Self| {
            r.immediate.get_or_insert_with(|| e.ok());
            r
        };
        let ng = Request { immediate: Some(self.ng()), ..Default::default() };
        // Menu items that are commands of their own.
        if let Some((item, n)) = extra_item(self.model, self.spec, b) {
            if b.len() == n {
                req.reads.push(crate::menu::read_cmd(item.num));
                req.tag = "menu".into();
                return req;
            }
            req.sets.push(format!("EX{:03}{};", item.num, menu_raw(item, &b[n..])));
            return ok(req, self);
        }
        // One-setting commands.
        if let Some(sm) = simple_by_cmd(b) {
            let rest = &b[sm.cmd.len()..];
            if !self.model.has(&sm.canon[..2]) {
                return ng;
            }
            if rest.is_empty() {
                req.reads.push(format!("{};", sm.canon));
                req.tag = "simple".into();
                return req;
            }
            let Some(v) = sm.from_radio(rest, self.spec) else { return ng };
            req.sets.push(sm.answer(v));
            return ok(req, self);
        }
        let a_or_b = |b: bool| if b { "FB;" } else { "FA;" };
        match cmd {
            0x03 => {
                req.reads.push(a_or_b(self.vfo_b).into());
                req.tag = "freq".into();
            }
            0x04 => {
                req.reads.push(if self.vfo_b { "OI;".into() } else { "MD0;".into() });
                req.tag = "mode".into();
            }
            0x05 | 0x00 => {
                let Some(hz) = freq_from(d) else { return ng };
                req.sets.push(format!("F{}{hz:08};", if self.vfo_b { 'B' } else { 'A' }));
                return ok(req, self);
            }
            0x06 | 0x01 => {
                let Some(&mode) = d.first() else { return ng };
                let Some(m) = self.canon_mode(mode, false) else { return ng };
                if self.vfo_b {
                    req.sets.extend(["SV;".into(), format!("MD0{};", m.code()), "SV;".into()]);
                } else {
                    req.sets.push(format!("MD0{};", m.code()));
                }
                return ok(req, self);
            }
            0x07 => match d.first() {
                Some(&v @ (0x00 | 0x01)) => {
                    self.vfo_b = v == 1;
                    req.sets.push(format!("VS{v};"));
                    return ok(req, self);
                }
                Some(0xA0) => {
                    req.sets.push("AB;".into());
                    return ok(req, self);
                }
                Some(0xB0) => {
                    req.sets.push("SV;".into());
                    return ok(req, self);
                }
                Some(0xD2) => {
                    // Main/sub: always main.
                    req.immediate = Some(self.reply(&[0x07, 0xD2, 0x00]));
                }
                None => return ok(req, self),
                _ => return ng,
            },
            0x0F => match d.first() {
                None => {
                    req.reads.push("FT;".into());
                    req.tag = "split".into();
                }
                Some(&on) => {
                    let tx_b = (on == 1) != self.vfo_b;
                    req.sets.push(format!("FT{};", if tx_b { 3 } else { 2 }));
                    return ok(req, self);
                }
            },
            0x11 => match d.first() {
                None => {
                    req.reads.push("RA0;".into());
                    req.tag = "att".into();
                }
                Some(&v) => {
                    let Some(&(c, _)) = self.spec.att.iter().find(|x| x.1 == v) else { return ng };
                    req.sets.push(format!("RA0{c};"));
                    return ok(req, self);
                }
            },
            0x15 => match d.first() {
                Some(0x01) => {
                    req.reads.push("BY;".into());
                    req.tag = "busy".into();
                }
                Some(&sub) => {
                    let Some(&(canon, _)) = METERS.iter().find(|(c, s)| *s == sub && *c != "RM1") else { return ng };
                    req.reads.push(format!("{canon};"));
                    req.tag = format!("meter{sub}");
                }
                None => return ng,
            },
            0x16 => match (d.first(), d.get(1)) {
                (Some(0x12), None) => {
                    req.reads.push("GT0;".into());
                    req.tag = "agc".into();
                }
                (Some(0x12), Some(&v)) => {
                    req.sets.push(format!("GT0{v};"));
                    return ok(req, self);
                }
                (Some(0x32), None) if self.spec.apf => {
                    req.reads.push("CO00;".into());
                    req.tag = "apf".into();
                }
                (Some(0x32), Some(&v)) if self.spec.apf => {
                    req.sets.push(format!("CO00{:02};", if v != 0 { 2 } else { 0 }));
                    return ok(req, self);
                }
                (Some(0x50), None) if self.spec.lock => {
                    req.reads.push("LK;".into());
                    req.tag = "lock".into();
                }
                (Some(0x50), Some(&v)) if self.spec.lock => {
                    req.sets.push(format!("LK{};", v & 1));
                    return ok(req, self);
                }
                _ => return ng,
            },
            0x14 if d.first() == Some(&0x05) && self.spec.apf => match d.get(1..3).and_then(from_bcd) {
                None => {
                    req.reads.push("CO02;".into());
                    req.tag = "apff".into();
                }
                Some(v) => {
                    req.sets.push(format!("CO02{:02};", apf_from_radio(self.spec, v)));
                    return ok(req, self);
                }
            },
            0x12 if self.spec.antenna_switch => match d.first() {
                None => {
                    req.reads.push("AN0;".into());
                    req.tag = "ant".into();
                }
                Some(&a) => {
                    req.sets.push(format!("AN0{};", a + 1));
                    return ok(req, self);
                }
            },
            0x17 => return ok(req, self),
            0x28 => match d.get(1) {
                Some(&n) if n <= self.spec.voice_memories => {
                    req.sets.push(format!("PB0{n};"));
                    return ok(req, self);
                }
                _ => return ng,
            },
            0x08 => match d.get(..2).and_then(|b| from_bcd(b)) {
                Some(ch) => {
                    req.sets.push(format!("MC{:03};", if d[0] == 0x01 { 100 + (d[1] as u64 & 1) } else { ch }));
                    return ok(req, self);
                }
                None => return ok(req, self),
            },
            0x09 => {
                req.sets.push("AM;".into());
                return ok(req, self);
            }
            0x0A => {
                req.sets.push("MA;".into());
                return ok(req, self);
            }
            0x19 => req.immediate = Some(self.reply(&[0x19, 0x00, self.addr])),
            0x1C => match (d.first(), d.get(1)) {
                (Some(0x00), None) => {
                    req.reads.push("TX;".into());
                    req.tag = "ptt".into();
                }
                (Some(0x00), Some(&on)) => {
                    req.sets.push(format!("TX{};", (on != 0) as u8));
                    return ok(req, self);
                }
                (Some(0x01), None) => {
                    req.reads.push("AC;".into());
                    req.tag = "tuner".into();
                }
                (Some(0x01), Some(&v)) => {
                    req.sets.push(format!("AC00{};", v.min(2)));
                    return ok(req, self);
                }
                _ => return ng,
            },
            0x21 => match (d.first(), d.len()) {
                (Some(0x00), 1) => {
                    req.reads.push("IF;".into());
                    req.tag = "rit".into();
                }
                (Some(0x00), _) => {
                    let (Some(lo), Some(hi), Some(&sign)) = (d.get(1), d.get(2), d.get(3)) else { return ng };
                    let Some(v) = from_bcd(&[*hi, *lo]) else { return ng };
                    req.sets.push("RC;".into());
                    if v > 0 {
                        req.sets.push(format!("{}{v:04};", if sign == 1 { "RD" } else { "RU" }));
                    }
                    return ok(req, self);
                }
                _ => return ng,
            },
            0x25 => {
                let Some(&sel) = d.first() else { return ng };
                let b = self.vfo(sel);
                match d.get(1..).and_then(freq_from) {
                    None => {
                        req.reads.push(a_or_b(b).into());
                        req.tag = format!("vfo{sel}");
                    }
                    Some(hz) => {
                        req.sets.push(format!("F{}{hz:08};", if b { 'B' } else { 'A' }));
                        return ok(req, self);
                    }
                }
            }
            0x26 => {
                let Some(&sel) = d.first() else { return ng };
                let b = self.vfo(sel);
                match d.get(1) {
                    None => {
                        req.reads.push(if b { "OI;".into() } else { "MD0;".into() });
                        req.tag = format!("vmode{sel}");
                    }
                    Some(&mode) => {
                        let data = d.get(2).is_some_and(|x| *x != 0);
                        let Some(m) = self.canon_mode(mode, data) else { return ng };
                        if b {
                            req.sets.extend(["SV;".into(), format!("MD0{};", m.code()), "SV;".into()]);
                        } else {
                            req.sets.push(format!("MD0{};", m.code()));
                        }
                        return ok(req, self);
                    }
                }
            }
            0x1A => match d.first() {
                Some(0x02) => match (d.get(1), d.len()) {
                    (Some(&n), 2) if n <= 5 => {
                        req.reads.push(format!("KM{n};"));
                        req.tag = "keyer".into();
                    }
                    (Some(&n), _) if n <= 5 => {
                        let text = String::from_utf8_lossy(&d[2..]).trim().to_string();
                        req.sets.push(format!("KM{n}{text}}};"));
                        return ok(req, self);
                    }
                    (Some(_), _) => return ok(req, self),
                    _ => return ng,
                },
                Some(0x00) => {
                    let Some(chb) = d.get(1..3) else { return ng };
                    let ch = if chb[0] == 0x01 { 100 + (chb[1] as u16 & 1) } else { from_bcd(chb).unwrap_or(0) as u16 };
                    if d.len() == 3 {
                        req.reads.push(format!("MR{ch:03};"));
                        req.tag = "mem".into();
                    } else {
                        // <split/select> <frequency 5> <mode> <filter> <data/tone> ...
                        let m = &d[3..];
                        if m.first() == Some(&0xFF) {
                            return ok(req, self);
                        }
                        let (Some(hz), Some(&mode)) = (m.get(1..).and_then(freq_from), m.get(6)) else { return ng };
                        let data = m.get(8).is_some_and(|x| x >> 4 != 0);
                        let Some(md) = self.canon_mode(mode, data) else { return ng };
                        let info = ChannelInfo { channel: ch, freq: hz, mode: Some(md), ..Default::default() };
                        req.sets.push(info.to_mw(ch));
                        return ok(req, self);
                    }
                }
                Some(0x01) if d.len() == 3 => {
                    // Band stack read: be on that band, and say where.
                    let Some(band) = d.get(1..2).and_then(from_bcd) else { return ng };
                    if !self.model.bands.iter().any(|b| b.code as u64 == band) {
                        return ng;
                    }
                    req.sets.push(format!("BS{band:02};"));
                    req.reads.extend(["FA;".to_string(), "MD0;".to_string()]);
                    req.tag = "bandstack".into();
                }
                Some(0x05) => {
                    let Some(addr) = d.get(1..3).and_then(from_bcd) else { return ng };
                    let Some(item) = self.model.menu.iter().find(|m| m.addr as u64 == addr) else { return ng };
                    if d.len() == 3 {
                        req.reads.push(crate::menu::read_cmd(item.num));
                        req.tag = "menu".into();
                    } else {
                        req.sets.push(format!("EX{:03}{};", item.num, menu_raw(item, &d[3..])));
                        return ok(req, self);
                    }
                }
                Some(0x06) => match d.get(1) {
                    None => {
                        req.reads.push("MD0;".into());
                        req.tag = "data".into();
                    }
                    Some(&on) => {
                        // DATA on/off for the current mode: read it first.
                        req.reads.push("MD0;".into());
                        req.tag = format!("setdata{on}");
                    }
                },
                Some(0x03) if d.len() == 1 => {
                    req.reads.push("SH0;".into());
                    req.tag = "simple".into();
                }
                _ => return ng,
            },
            0x18 => {
                req.sets.push(format!("PS{};", (d.first() == Some(&1)) as u8));
                return ok(req, self);
            }
            _ => return ng,
        }
        req
    }

    fn respond(&mut self, req: &Request, answers: &[Option<String>]) -> Vec<u8> {
        if req.reads.is_empty() {
            return req.immediate.clone().unwrap_or_else(|| self.ok());
        }
        let Some(Some(a)) = answers.first() else { return self.ng() };
        let Some((code, p)) = frame::split(a) else { return self.ng() };
        let last = self.last.clone();
        let mut body = last.clone();
        let tag = req.tag.as_str();
        if tag == "bandstack" {
            let hz = super::num::<u64>(p).unwrap_or(0);
            let m = answers.get(1).cloned().flatten().and_then(|md| md.get(3..4).and_then(|c| Mode::from_code(c.chars().next()?)));
            let (mode, data) = m.and_then(|m| self.icom_mode(m)).unwrap_or((0x01, false));
            body.extend(freq_bytes(hz));
            body.extend([mode, 0x01, (data as u8) << 4, 0x00, 0x08, 0x85, 0x00, 0x08, 0x85]);
            return self.reply(&body);
        }
        match tag {
            "simple" => {
                let Some(sm) = simple_by_cmd(&last) else { return self.ng() };
                let Some(v) = super::num::<i64>(&a[sm.canon.len()..a.len() - 1]) else { return self.ng() };
                body.extend(sm.to_radio(v, self.spec));
            }
            "freq" | "vfo0" | "vfo1" => {
                let Some(hz) = super::num::<u64>(p) else { return self.ng() };
                body.extend(freq_bytes(hz));
            }
            "mode" | "vmode0" | "vmode1" | "data" => {
                let m = match code.as_str() {
                    "MD" => p.get(1..2).and_then(|c| Mode::from_code(c.chars().next()?)),
                    _ => ChannelInfo::parse(p).and_then(|i| i.mode),
                };
                let Some((mode, data)) = m.and_then(|m| self.icom_mode(m)) else { return self.ng() };
                match tag {
                    "mode" => body.extend([mode, 0x01]),
                    "data" => body.extend([data as u8, if data { 0x01 } else { 0x00 }]),
                    _ => body.extend([mode, data as u8, 0x01]),
                }
            }
            "split" => {
                let tx_b = p == "1";
                body.push((tx_b != self.vfo_b) as u8);
            }
            "att" => {
                let c: u8 = p.get(1..2).and_then(|c| c.parse().ok()).unwrap_or(0);
                body.push(self.spec.att.iter().find(|x| x.0 == c).map_or(0, |x| x.1));
            }
            "busy" => body.extend(bcd((p.get(..1) == Some("1")) as u64, 1)),
            t if t.starts_with("meter") => {
                let sub: u8 = t[5..].parse().unwrap_or(2);
                let v: u8 = a.get(3..a.len() - 1).and_then(|x| x.parse().ok()).unwrap_or(0);
                body.extend(bcd(meter_from_canon(self.spec, sub, v) as u64, 2));
            }
            "apf" => body.push((p.get(2..4) == Some("02")) as u8),
            "apff" => {
                let v: i64 = p.get(2..).and_then(super::num).unwrap_or(25);
                body.extend(bcd(apf_to_radio(self.spec, v), 2));
            }
            "lock" => body.push(p.parse::<u8>().unwrap_or(4) & 1),
            "ant" => body.push(p.get(1..2).and_then(|c| c.parse::<u8>().ok()).unwrap_or(1).saturating_sub(1)),
            "keyer" => {
                let text = p.get(1..).unwrap_or("").split('}').next().unwrap_or("").trim_end();
                body.extend(if text.is_empty() { b" ".to_vec() } else { text.as_bytes().to_vec() });
            }
            "mem" => {
                let Some(info) = ChannelInfo::parse(p) else { return self.ng() };
                let Some(m) = info.mode else { return self.ng() };
                let Some((mode, data)) = self.icom_mode(m) else { return self.ng() };
                body.extend(memory_body(&info, IcomMode { mode, data, filter: 1 }, self.spec));
            }
            "agc" => body.push(p.get(1..2).and_then(|c| c.parse::<u8>().ok()).unwrap_or(2).clamp(1, 3)),
            "ptt" => body.push((p != "0") as u8),
            "tuner" => body.push(p.get(2..3).and_then(|c| c.parse::<u8>().ok()).unwrap_or(0)),
            "rit" => {
                let Some(info) = ChannelInfo::parse(p) else { return self.ng() };
                let mut d = bcd(info.clar_offset.unsigned_abs() as u64, 2);
                d.reverse();
                body.extend(d);
                body.push((info.clar_offset < 0) as u8);
            }
            "menu" => {
                let Some(num) = p.get(..3).and_then(|n| n.parse::<u16>().ok()) else { return self.ng() };
                let Some(item) = self.model.menu_item(num) else { return self.ng() };
                let Some(data) = menu_data(item, &p[3..]) else { return self.ng() };
                body.extend(data);
            }
            t if t.starts_with("setdata") => {
                // The program turned DATA on/off: keep the mode, change DATA.
                let on = t.ends_with('1');
                let Some(m) = p.get(1..2).and_then(|c| Mode::from_code(c.chars().next()?)) else { return self.ng() };
                let Some((mode, _)) = self.icom_mode(m) else { return self.ng() };
                let Some(new) = self.canon_mode(mode, on) else { return self.ng() };
                self.pending_set = Some(format!("MD0{};", new.code()));
                return self.ok();
            }
            _ => return self.ng(),
        }
        self.reply(&body)
    }

    fn after(&mut self) -> Vec<String> {
        self.pending_set.take().into_iter().collect()
    }

    fn unsolicited(&mut self, f: &str) -> Option<Vec<u8>> {
        // Transceive broadcasts: frequency and mode of the selected VFO.
        let (code, p) = frame::split(f)?;
        let body = match code.as_str() {
            "FA" | "FB" if (code == "FB") == self.vfo_b => {
                let mut b = vec![0x00];
                b.extend(freq_bytes(super::num(p)?));
                b
            }
            "MD" if !self.vfo_b => {
                let m = p.get(1..2).and_then(|c| Mode::from_code(c.chars().next()?))?;
                let (mode, _) = self.icom_mode(m)?;
                vec![0x01, mode, 0x01]
            }
            _ => return None,
        };
        Some(Civ::bytes(0x00, self.addr, &body))
    }
}

/// Helpers for the models' tests: literal CI-V frames through the
/// translator and the emulator.
#[cfg(test)]
pub mod testing {
    use super::*;
    pub use crate::dialect::{Dialect, Emulator};
    use crate::model::Proto;

    /// `"FE FE 94 E0 03 FD"` as bytes.
    pub fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    pub fn spec(m: &'static RadioModel) -> &'static CivSpec {
        match m.proto {
            Proto::CiV(s) => s,
            _ => panic!("{} is not CI-V", m.key),
        }
    }

    pub fn radio(m: &'static RadioModel) -> Icom {
        Icom::new(m, spec(m))
    }

    pub fn emulator(m: &'static RadioModel) -> IcomEmulator {
        IcomEmulator::new(m, spec(m))
    }

    /// The CI-V frames (as hex text) a RigHelm command becomes.
    pub fn down(r: &mut Icom, f: &str) -> Vec<String> {
        r.command(f)
            .into_iter()
            .map(|o| match o {
                Out::Radio { bytes, .. } => bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "),
                Out::Answer(a) => a,
            })
            .collect()
    }

    /// RigHelm frames for one frame from the radio (hex text).
    pub fn up(r: &mut Icom, frame: &str) -> Vec<String> {
        r.answer(&hex(frame)).frames
    }

    /// A RigHelm Set goes out as CI-V and the emulator turns it back into
    /// the same Set.
    pub fn round_trip_set(m: &'static RadioModel, f: &str) {
        let mut r = radio(m);
        let mut e = emulator(m);
        let mut sets = Vec::new();
        for o in r.command(f) {
            let Out::Radio { bytes, .. } = o else { panic!("{}: {f} not sent: {o:?}", m.key) };
            let req = e.request(&bytes);
            assert!(req.reads.is_empty(), "{}: {f} read {:?}", m.key, req.reads);
            sets.extend(req.sets);
        }
        assert_eq!(sets, vec![f.to_string()], "{}: {f}", m.key);
    }

    /// A RigHelm Read goes out as CI-V, the emulator asks RigHelm for
    /// `answer`, and the radio side reads that answer back.
    pub fn round_trip_read(m: &'static RadioModel, read: &str, answer: &str) {
        let mut r = radio(m);
        let mut e = emulator(m);
        let mut got = Vec::new();
        for o in r.command(read) {
            match o {
                Out::Radio { bytes, .. } => {
                    let req = e.request(&bytes);
                    let reply = if req.reads.is_empty() {
                        e.respond(&req, &[])
                    } else {
                        let answers: Vec<Option<String>> = req.reads.iter().map(|_| Some(answer.to_string())).collect();
                        e.respond(&req, &answers)
                    };
                    got.extend(r.answer(&reply).frames);
                }
                Out::Answer(a) => got.push(a),
            }
        }
        assert!(got.iter().any(|g| g == answer), "{}: {read} gave {got:?}, wanted {answer}", m.key);
    }

    /// Every menu item's first and last choice goes out and comes back.
    pub fn menu_round_trips(m: &'static RadioModel) {
        for item in m.menu {
            let codes: Vec<i32> = match item.kind {
                Kind::Text { .. } => {
                    round_trip_set(m, &item.set_cmd(&crate::menu::Value::Text("AB1CD".into()), None));
                    continue;
                }
                Kind::Num { min, max, .. } => vec![min, max],
                Kind::Percent { max } => vec![0, max],
                _ => item.options().iter().map(|o| o.0).collect(),
            };
            for c in [codes.first(), codes.last()].into_iter().flatten() {
                round_trip_set(m, &item.set_cmd(&crate::menu::Value::Int(*c), None));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    #[test]
    fn frames_and_bcd() {
        assert_eq!(bcd(255, 2), vec![0x02, 0x55]);
        assert_eq!(from_bcd(&[0x01, 0x28]), Some(128));
        assert_eq!(freq_bytes(14_074_000), hex("00 40 07 14 00"));
        assert_eq!(freq_from(&hex("00 40 07 14 00")), Some(14_074_000));
        assert_eq!(text_bcd("0255"), Some(vec![0x02, 0x55]));
        assert_eq!(bcd_text(&[0x02, 0x55]), "0255");
        let mut s = CivSplitter::default();
        assert_eq!(s.split(&hex("FE FE FE FE 94 E0 03 FD FE FE")), vec![hex("FE FE 94 E0 03 FD")]);
    }
}
