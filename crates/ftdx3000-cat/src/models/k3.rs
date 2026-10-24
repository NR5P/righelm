//! Elecraft K3 (and K3S), KX3 and KX2: Kenwood-style text commands, much
//! extended ("K3S & K3 & KX3 & KX2 Programmer's Reference", rev G5).
//!
//! One translator for all three; where they differ (switch codes, RF gain
//! range, power meter, menu numbers) it looks at the model. The K3S speaks
//! the K3's protocol (its attenuator also takes 5 and 15 dB, and it has
//! preamp 2), so it is the K3 model here.
//!
//! How RigHelm's commands become the radio's:
//!
//! * RigHelm turns the K3 extended mode on (`K31;`) with its first commands
//!   (`ID;` and `AI`), so `IF` reports the DATA sub-mode and `SM` has the
//!   finer 0-21 scale. Hamlib and most K3 programs do the same.
//! * Modes: `MD` plus the DATA sub-mode `DT` (DATA A = DATA-U/L, AFSK A and
//!   FSK D = RTTY, PSK D = PSK). The mode is read from `IF` (one answer has
//!   both).
//! * Menu (`EX`): the radio selects an entry with `MNnnn;` and reads/sets
//!   its 8-bit value with `MP` (`MQ`, 16 bits, for TXCRNUL), then `MN255;`
//!   leaves the menu. Only entries marked as MP-accessible in the manual are
//!   in the tables; most values are the radio's raw numbers, the manual
//!   gives no scale for them. On the KX3/KX2 some entries are single bits
//!   (listed in the manual), those have names.
//! * Levels with other ranges are scaled (RF gain 0-250, KX3/KX2 190-250;
//!   mic 0-60; compression 0-40; monitor 0-60; squelch 0-29). The value
//!   RigHelm set is remembered, so the radio's answer reads back exactly.
//! * Meters: S-meter `SM`; SWR `SW`; power: KX3/KX2 `PO` (watts), the K3
//!   only has its bargraph `BG` (power or ALC bars, as its TX meter switch
//!   `TM` is set).
//! * Split: `FT1;` on, `FR0;` off; the K3 always receives on VFO A. VFO B's
//!   mode is `MD$`.

use crate::dialect::text::{Ctx, REPLY, TextSpec, freq_digits, parts, read_prefix};
use crate::dialect::rescale;
use crate::menu::{Kind, MenuItem};
use crate::model::{Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::state::ChannelInfo;
use crate::types::{Band, Mode, PO_CAL, S_METER_CAL, SWR_CAL, WidthClass, interpolate};

pub static SPEC: TextSpec = TextSpec { down, up, answer_prefix };

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rig {
    K3,
    Kx3,
    Kx2,
}

fn rig(cx: &Ctx) -> Rig {
    match cx.model.key {
        "kx3" => Rig::Kx3,
        "kx2" => Rig::Kx2,
        _ => Rig::K3,
    }
}

/// The answer to a write of several frames is the answer to its one read:
/// `MN012;MP;MN255;` -> `MP`, `K31;ID;` -> `ID`.
pub(crate) fn answer_prefix(n: &str) -> String {
    let read = n.split_inclusive(';').find(|f| {
        let body = f.trim_end_matches(';');
        body.len() >= 2 && body.chars().all(|c| c.is_ascii_alphabetic() || c == '$')
    });
    read_prefix(read.unwrap_or(n))
}

// ------------------------------------------------------------- values

/// Scale `v` from `a0..=a1` onto `b0..=b1`, rounded.
pub(crate) fn scale(v: u32, a0: u32, a1: u32, b0: u32, b1: u32) -> u32 {
    let v = v.clamp(a0, a1);
    b0 + (((v - a0) as f32 * (b1 - b0) as f32) / (a1 - a0) as f32).round() as u32
}

/// RigHelm level `v` (0..=`hi`) as the radio's (`lo2..=hi2`), remembered so
/// the radio's answer comes back as exactly `v`.
pub(crate) fn level_down(cx: &mut Ctx, key: &str, v: u32, hi: u32, lo2: u32, hi2: u32) -> u32 {
    let n = scale(v, 0, hi, lo2, hi2);
    if !cx.program {
        cx.remember(key, format!("{n}={}", v.min(hi)));
    }
    n
}

pub(crate) fn level_up(cx: &Ctx, key: &str, n: u32, hi: u32, lo2: u32, hi2: u32) -> u32 {
    if let Some((a, b)) = cx.recall(key).and_then(|s| s.split_once('=')) {
        if a.parse() == Ok(n) {
            return b.parse().unwrap_or(0);
        }
    }
    scale(n, lo2, hi2, 0, hi)
}

pub(crate) fn int(s: &str) -> Option<u32> {
    s.trim().trim_start_matches('+').parse().ok()
}

pub(crate) fn memo_u32(cx: &Ctx, key: &str) -> Option<u32> {
    cx.recall(key).and_then(int)
}

/// RF gain range: the KX3/KX2 use 190 (-60 dB) to 250 (full gain).
fn rg_range(cx: &Ctx) -> (u32, u32) {
    if rig(cx) == Rig::K3 { (0, 250) } else { (190, 250) }
}

/// The radio's mode (`MD`) and DATA sub-mode (`DT`) for a RigHelm mode.
/// RTTY keeps the sub-mode last seen (AFSK A or FSK D, AFSK A at first).
pub(crate) fn k3_mode(cx: &Ctx, m: Mode) -> Option<(u8, Option<u8>)> {
    let rtty = memo_u32(cx, "rtty_dt").map_or(1, |d| d as u8);
    Some(match m {
        Mode::Lsb => (1, None),
        Mode::Usb => (2, None),
        Mode::Cw => (3, None),
        Mode::Fm => (4, None),
        Mode::Am => (5, None),
        Mode::CwR => (7, None),
        Mode::DataUsb => (6, Some(0)),
        Mode::DataLsb => (9, Some(0)),
        Mode::RttyLsb => (6, Some(rtty)),
        Mode::RttyUsb => (9, Some(rtty)),
        Mode::Psk => (6, Some(3)),
        _ => return None,
    })
}

/// RigHelm's mode for the radio's `MD` and `DT`.
pub(crate) fn rh_mode(md: u8, dt: u8) -> Option<Mode> {
    let rev = md == 9;
    Some(match md {
        1 => Mode::Lsb,
        2 => Mode::Usb,
        3 => Mode::Cw,
        4 => Mode::Fm,
        5 => Mode::Am,
        7 => Mode::CwR,
        6 | 9 => match dt {
            1 | 2 if rev => Mode::RttyUsb,
            1 | 2 => Mode::RttyLsb,
            3 => Mode::Psk,
            _ if rev => Mode::DataLsb,
            _ => Mode::DataUsb,
        },
        _ => return None,
    })
}

pub(crate) fn note_dt(cx: &mut Ctx, dt: u8) {
    cx.remember("dt", dt.to_string());
    if dt == 1 || dt == 2 {
        cx.remember("rtty_dt", dt.to_string());
    }
}

/// The mode RigHelm reports from the radio's `MD` (and the last `DT`).
pub(crate) fn mode_now(cx: &Ctx) -> Option<Mode> {
    let md = memo_u32(cx, "md")? as u8;
    rh_mode(md, memo_u32(cx, "dt").unwrap_or(0) as u8)
}

pub(crate) fn md_frame(m: Mode) -> String {
    format!("MD0{};", m.code())
}

/// Filter widths, `(SH index, Hz)`: 50 Hz steps to 1 kHz, 100 Hz steps to
/// 4 kHz. The radio sets any width in 10 Hz units (`BW`), quantized by mode.
pub(crate) fn widths(class: WidthClass, _narrow: bool) -> Vec<(u8, u32)> {
    if class == WidthClass::Fixed {
        return Vec::new();
    }
    let mut v: Vec<u32> = (1..=20).map(|i| i * 50).collect();
    v.extend((11..=40).map(|i| i * 100));
    v.into_iter().enumerate().map(|(i, hz)| (i as u8 + 1, hz)).collect()
}

pub(crate) fn width_index(hz: u32) -> u8 {
    widths(WidthClass::Ssb, false).into_iter().min_by_key(|(_, w)| w.abs_diff(hz)).map_or(1, |(i, _)| i)
}

/// S-meter: `SM` in K31 mode is 0-21 (S9 = 9, S9+20 = 13, S9+60 = 21), in
/// K30 mode 0-15 (S9 = 6, S9+20 = 9, S9+60 = 15). In dB over S9.
pub(crate) const SM_K31: [(u8, f32); 5] = [(0, -54.0), (9, 0.0), (13, 20.0), (17, 40.0), (21, 60.0)];
pub(crate) const SM_K30: [(u8, f32); 5] = [(0, -54.0), (6, 0.0), (9, 20.0), (12, 40.0), (15, 60.0)];
/// The K3's high-resolution S-meter `SMH`: S1 = 5, S9 = 40, S9+60 = 100,
/// about 140 at most.
pub(crate) const SMH_CAL: [(u8, f32); 5] = [(0, -54.0), (5, -48.0), (40, 0.0), (100, 60.0), (140, 84.0)];
/// A linear scale for values that are already in the unit wanted.

pub(crate) const UNITS: [(u8, f32); 2] = [(0, 0.0), (255, 255.0)];
pub(crate) const TENTHS: [(u8, f32); 2] = [(0, 0.0), (255, 25.5)];

/// RigHelm's raw meter value for `v` in the calibration's unit.
pub(crate) fn meter_raw(v: f32, tenths: bool, cal: &[(u8, f32)]) -> u8 {
    let n = if tenths { (v * 10.0).round() } else { v.round() };
    rescale(n.clamp(0.0, 255.0) as u8, if tenths { &TENTHS } else { &UNITS }, cal)
}

/// The radio's RIT/XIT offset as `RO` / the K3 `IF` field: `+0120`.
pub(crate) fn offset_text(off: i32) -> String {
    format!("{}{:04}", if off < 0 { '-' } else { '+' }, off.unsigned_abs().min(9999))
}

/// K3 `IF` answer: `IF[f]*****+yyyyrx*00tmvspbd1*;` (35 parameter
/// characters, `*` a space, the frequency 11 digits).
#[allow(clippy::too_many_arguments)]
fn k3_if(hz: u64, off: i32, rit: bool, xit: bool, tx: bool, md: u8, split: bool, dt: u8) -> String {
    format!("IF{hz:011}     {}{}{} 00{}{md}00{}0{dt}1 ;", offset_text(off), rit as u8, xit as u8, tx as u8, split as u8)
}

/// The fields of a K3 `IF` answer: frequency, offset, RIT, XIT, mode, DATA
/// sub-mode.
fn parse_k3_if(p: &str) -> Option<(u64, i32, bool, bool, u8, u8)> {
    if p.len() < 33 || !p.is_ascii() {
        return None;
    }
    let hz: u64 = p[0..11].parse().ok()?;
    let sign = if &p[16..17] == "-" { -1 } else { 1 };
    let off: i32 = p[17..21].trim().parse().ok()?;
    let md: u8 = p[27..28].parse().ok()?;
    let dt: u8 = p[32..33].parse().unwrap_or(0);
    Some((hz, sign * off, &p[21..22] == "1", &p[22..23] == "1", md, dt))
}

// ------------------------------------------------------------- switches

/// Switch emulation for A>B and A/B (swap), which differ per radio.
fn switch(cx: &Ctx, cmd: &str) -> Option<&'static str> {
    Some(match (rig(cx), cmd) {
        (Rig::K3, "AB") => "SWT13;",
        (Rig::K3, "SV") => "SWT11;",
        (Rig::Kx3, "AB") => "SWT25;",
        (Rig::Kx3, "SV") => "SWT24;",
        (Rig::Kx2, "AB") => "SWH44;",
        (Rig::Kx2, "SV") => "SWT44;",
        _ => return None,
    })
}

fn switch_up(cx: &Ctx, sw: &str) -> Option<&'static str> {
    ["AB", "SV"].into_iter().find(|c| switch(cx, c).is_some_and(|s| s.trim_end_matches(';')[2..] == *sw))
}

// ------------------------------------------------------------- menu

/// A menu item: the radio's MN number is `addr`, its value is 3 digits
/// (`MP`) or 5 (`MQ`).
pub(crate) const fn it(num: u16, addr: u32, group: &'static str, name: &'static str, kind: Kind) -> MenuItem {
    MenuItem { num, addr, group, name, width: 3, kind }
}

const fn it16(num: u16, addr: u32, group: &'static str, name: &'static str) -> MenuItem {
    MenuItem { num, addr, group, name, width: 5, kind: crate::menu::num(0, 65535, 1, "") }
}

/// The radio's own 8-bit value; the manual gives no scale.
const RAW: Kind = crate::menu::num(0, 255, 1, "");
const ON_OFF: Kind = Kind::Choice(&["OFF", "ON"]);

const MAIN: &str = "MAIN";
const CONFIG: &str = "CONFIG";

/// K3/K3S entries readable and settable with MP (Table 5, marked ‡).
/// Left out: VCO MD, AGC THR and L-MIX-R, which are tech-mode entries that
/// MN skips (selecting another entry) unless CONFIG:TECH MD is on.
pub static K3_MENU: &[MenuItem] = &[
    it(1, 2, MAIN, "LCD ADJ", RAW),
    // The manual's example: MP005 sets LCD BRT to 6.
    it(2, 3, MAIN, "LCD BRT", RAW),
    it(3, 4, MAIN, "LED BRT", RAW),
    it(4, 5, MAIN, "MSG RPT", RAW),
    it(5, 7, MAIN, "RPT OFS", RAW),
    it(6, 105, MAIN, "AFX MD", RAW),
    it(7, 117, MAIN, "ATTEN", RAW),
    // The manual's example: MP001 turns DIGOUT1 on (for an amplifier).
    it(8, 19, CONFIG, "DIGOUT1", ON_OFF),
    it(9, 23, CONFIG, "KAT3", RAW),
    it(10, 32, CONFIG, "LIN OUT", RAW),
    it(11, 55, CONFIG, "KPA3", RAW),
    it(12, 58, CONFIG, "TUN PWR", RAW),
];

const CW: &str = "CW";
const TXG: &str = "TX";
const RXG: &str = "RX";
const ATU: &str = "ATU / PA";
const GEN: &str = "GENERAL";
const TECH: &str = "TECH";

const IAMBIC: Kind = Kind::Coded(&[(0, "A"), (128, "B")]);
const TECH_MD: Kind = Kind::Coded(&[(0, "OFF"), (4, "ON")]);
const MIC_BTN: Kind = Kind::Coded(&[(0, "PTT OFF, UP/DN OFF"), (1, "PTT ON, UP/DN OFF"), (4, "PTT OFF, UP/DN ON"), (5, "PTT ON, UP/DN ON")]);
const TX_GATE: Kind = Kind::Coded(&[(0, "OFF"), (2, "ON")]);
const VFO_CRS: Kind = Kind::Coded(&[(0, "SETTING 1"), (4, "SETTING 2"), (8, "SETTING 3"), (12, "SETTING 4")]);
const ATU_DATA: Kind = Kind::Coded(&[(0, "SET 1"), (8, "SET 2")]);
const KEY1: Kind = Kind::Coded(&[(0, "PADDLE, TIP = DOT"), (1, "PADDLE, TIP = DASH"), (2, "HAND KEY, TIP = DOT"), (3, "HAND KEY, TIP = DASH")]);
const KEY2: Kind = Kind::Coded(&[(0, "PADDLE, TIP = DOT"), (16, "PADDLE, TIP = DASH"), (32, "HAND KEY, TIP = DOT"), (48, "HAND KEY, TIP = DASH")]);
const AM_MODE: Kind = Kind::Coded(&[(0, "DISABLED"), (64, "ENABLED")]);
const AGC_MD: Kind = Kind::Coded(&[(0, "ON"), (1, "OFF")]);
const AGC_SPD: Kind = Kind::Coded(&[(0, "SLOW"), (2, "FAST")]);
const BIT4: Kind = Kind::Coded(&[(0, "OFF"), (16, "ON")]);

/// KX3 entries readable and settable with MP/MQ (Table 6, marked ‡). The
/// bit fields of the MP special cases are named; the rest are raw.
pub static KX3_MENU: &[MenuItem] = &[
    it(1, 0, GEN, "ALARM", Kind::Coded(&[(0, "ON"), (16, "OFF")])),
    it(2, 1, CW, "CW IAMB", IAMBIC),
    it(3, 5, TXG, "MSG RPT", RAW),
    it(4, 7, TXG, "RPT OFS", RAW),
    it(5, 10, TXG, "VOX GN", RAW),
    it(6, 12, CW, "CW WGHT", RAW),
    it(7, 16, TXG, "TX DLY", RAW),
    it(8, 18, GEN, "FM MODE", Kind::Coded(&[(0, "DISABLED"), (128, "ENABLED")])),
    it(9, 21, TXG, "FM DEV", RAW),
    it(10, 23, ATU, "ATU MD", RAW),
    it(11, 24, GEN, "BAT MIN", RAW),
    it(12, 45, GEN, "AUTOINF", RAW),
    it(13, 47, RXG, "AF LIM", RAW),
    it(14, 58, ATU, "TUN PWR", RAW),
    it(15, 60, RXG, "SMTR MD", Kind::Coded(&[(0, "NOR"), (128, "ABSOLUTE")])),
    it(16, 71, CW, "SW TONE", RAW),
    it(17, 72, TECH, "TECH MD", TECH_MD),
    it(18, 74, TECH, "AGC THR", RAW),
    it(19, 76, GEN, "BND MAP", Kind::Coded(&[(0, "IN"), (32, "OUT")])),
    it(20, 82, TXG, "MIC BTN", MIC_BTN),
    it(21, 84, GEN, "VFO CTS", RAW),
    it(22, 87, GEN, "VFO OFS", Kind::Coded(&[(0, "DISABLED"), (1, "ENABLED")])),
    it(23, 88, ATU, "WATTMTR", RAW),
    it(24, 96, TXG, "TX ESSB", ON_OFF),
    it(25, 101, TXG, "TX GATE", TX_GATE),
    it(26, 104, GEN, "VFO CRS", VFO_CRS),
    it(27, 105, RXG, "AFX MD", RAW),
    it(28, 112, ATU, "ATU DATA", ATU_DATA),
    it(29, 119, GEN, "VFO NR", Kind::Coded(&[(0, "ON"), (32, "OFF")])),
    it(30, 120, CW, "CW KEY1", KEY1),
    it(31, 121, CW, "CW KEY2", KEY2),
    it(32, 122, TXG, "VOX INH", RAW),
    it(33, 123, TECH, "RX I/Q", Kind::Coded(&[(0, "OFF"), (4, "ON")])),
    it(34, 124, TECH, "RX ISO", RAW),
    it(35, 125, TECH, "RXSBNUL", RAW),
    it(36, 126, GEN, "AM MODE", AM_MODE),
    it(37, 127, TECH, "TXSBNUL", RAW),
    it(38, 128, RXG, "AGC MD", AGC_MD),
    it(39, 129, RXG, "AGC SPD", AGC_SPD),
    it16(40, 132, TECH, "TXCRNUL"),
    it(41, 133, GEN, "AUTOOFF", RAW),
    it(42, 134, RXG, "RX XFIL", Kind::Coded(&[(0, "NOT INST"), (2, "INST")])),
    it(43, 135, TXG, "MICBIAS", BIT4),
    it(44, 136, RXG, "PREAMP", Kind::Coded(&[(1, "10 dB"), (2, "20 dB"), (3, "30 dB")])),
    it(45, 137, GEN, "BAT CHG", RAW),
    it(46, 138, GEN, "BKLIGHT", RAW),
    it(47, 139, GEN, "COR LVL", RAW),
    it(48, 140, RXG, "DUAL RX", BIT4),
    it(49, 141, GEN, "ACC2 IO", RAW),
    it(50, 142, RXG, "RX SHFT", Kind::Coded(&[(0, "NOR"), (1, "8.0 kHz")])),
    it(51, 143, RXG, "RX NR", RAW),
    it(52, 144, RXG, "PBT SSB", Kind::Coded(&[(0, "LO/HI CUT"), (128, "NOR")])),
    it(53, 145, GEN, "LED BRT", RAW),
    it(54, 146, ATU, "PA MODE", RAW),
    it(55, 147, GEN, "2M MODE", RAW),
];

/// KX2 entries readable and settable with MP/MQ (Table 6A, marked ‡).
/// SMTR MD and RX I/Q have bit meanings on the KX3 only: raw here.
pub static KX2_MENU: &[MenuItem] = &[
    it(1, 1, CW, "CW IAMB", IAMBIC),
    it(2, 5, TXG, "MSG RPT", RAW),
    it(3, 10, TXG, "VOX GN", RAW),
    it(4, 12, CW, "CW WGHT", RAW),
    it(5, 16, TXG, "TX DLY", RAW),
    it(6, 23, ATU, "ATU MD", RAW),
    it(7, 24, GEN, "BAT MIN", RAW),
    it(8, 45, GEN, "AUTOINF", RAW),
    it(9, 47, RXG, "AF LIM", RAW),
    it(10, 58, ATU, "TUN PWR", RAW),
    it(11, 60, RXG, "SMTR MD", RAW),
    it(12, 71, CW, "SW TONE", RAW),
    it(13, 72, TECH, "TECH MD", TECH_MD),
    it(14, 74, TECH, "AGC THR", RAW),
    it(15, 82, TXG, "MIC BTN", MIC_BTN),
    it(16, 88, ATU, "WATTMTR", RAW),
    it(17, 101, TXG, "TX GATE", TX_GATE),
    it(18, 104, GEN, "VFO CRS", VFO_CRS),
    it(19, 105, RXG, "AFX MD", RAW),
    it(20, 112, ATU, "ATU DATA", ATU_DATA),
    it(21, 120, CW, "CW KEY1", KEY1),
    it(22, 121, CW, "CW KEY2", KEY2),
    it(23, 122, TXG, "VOX INH", RAW),
    it(24, 123, TECH, "RX I/Q", RAW),
    it(25, 125, TECH, "RXSBNUL", RAW),
    it(26, 126, GEN, "AM MODE", AM_MODE),
    it(27, 127, TECH, "TXSBNUL", RAW),
    it(28, 128, RXG, "AGC MD", AGC_MD),
    it(29, 129, RXG, "AGC SPD", AGC_SPD),
    it16(30, 132, TECH, "TXCRNUL"),
    it(31, 133, GEN, "AUTOOFF", RAW),
    it(32, 135, TXG, "MICBIAS", BIT4),
    it(33, 138, GEN, "BKLIGHT", RAW),
    it(34, 139, GEN, "COR LVL", RAW),
    it(35, 140, RXG, "DUAL RX", BIT4),
    it(36, 145, GEN, "LED BRT", RAW),
    it(37, 146, ATU, "PA MODE", RAW),
    it(38, 148, CW, "PITCH", RAW),
    it(39, 154, TXG, "TX CMP", RAW),
    it(40, 155, RXG, "RF GAIN", RAW),
];

/// `MN012;MP;MN255;` (read) or the Set frames for a menu item.
fn menu_down(cx: &mut Ctx, p: &str) -> Vec<String> {
    let Some(num) = p.get(..3).and_then(|n| n.parse::<u16>().ok()) else { return Vec::new() };
    let Some(item) = cx.model.menu_item(num) else { return Vec::new() };
    let mp = if item.width == 5 { "MQ" } else { "MP" };
    let raw = &p[3..];
    if raw.is_empty() {
        cx.remember("menu", num.to_string());
        return vec![format!("MN{:03};{mp};MN255;", item.addr)];
    }
    let Some(v) = int(raw).filter(|v| *v <= if item.width == 5 { 65535 } else { 255 }) else { return Vec::new() };
    let value = format!("{mp}{v:0w$};", w = item.width as usize);
    if cx.program {
        return vec![value];
    }
    vec![format!("MN{:03};", item.addr), value, "MN255;".into()]
}

// ------------------------------------------------------------- down

pub(crate) fn down(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !cx.model.has(&code) {
        return Vec::new();
    }
    let p = p.to_string();
    if cx.program { to_program(cx, &code, &p) } else { to_radio(cx, f, &code, &p) }
}

/// A RigHelm Set or Read as the radio's frame(s).
fn to_radio(cx: &mut Ctx, f: &str, code: &str, p: &str) -> Vec<String> {
    let read = crate::dialect::text::is_read(f);
    let one = |s: String| vec![s];
    let n = int(p.get(1..).unwrap_or("")).unwrap_or(0);
    match code {
        // The extended mode goes with RigHelm's first commands.
        "ID" if read => one("K31;ID;".into()),
        "AI" if read => one("AI;".into()),
        "AI" => vec!["K31;".into(), format!("AI{};", if p == "0" { 0 } else { 2 })],
        "FA" | "FB" => freq_digits(f, 11),
        "IF" if read => one("IF;".into()),
        "MD" if read => one("IF;".into()),
        "MD" => {
            let Some(m) = p.get(1..2).and_then(|c| Mode::from_code(c.chars().next()?)).filter(|m| cx.model.modes.contains(m)) else {
                return Vec::new();
            };
            let Some((md, dt)) = k3_mode(cx, m) else { return Vec::new() };
            cx.remember("md", md.to_string());
            let mut v = vec![format!("MD{md};")];
            if let Some(dt) = dt {
                note_dt(cx, dt);
                v.push(format!("DT{dt};"));
            }
            v
        }
        "OI" if read => vec!["FB;".into(), "MD$;".into()],
        "AG" if read => one("AG;".into()),
        "AG" => one(format!("AG{:03};", n.min(255))),
        "RG" if read => one("RG;".into()),
        "RG" => {
            let (lo, hi) = rg_range(cx);
            one(format!("RG{:03};", level_down(cx, "rg", n, 255, lo, hi)))
        }
        "SQ" if read => one("SQ;".into()),
        "SQ" => one(format!("SQ{:03};", level_down(cx, "sq", n, 255, 0, 29))),
        "MG" if read => one("MG;".into()),
        "MG" => one(format!("MG{:03};", level_down(cx, "mg", int(p).unwrap_or(0), 100, 0, 60))),
        "PL" if read => one("CP;".into()),
        "PL" => one(format!("CP{:03};", level_down(cx, "cp", int(p).unwrap_or(0), 100, 0, 40))),
        "ML" if read => one("ML;".into()),
        // Monitor off is level 0; on brings back the last level.
        "ML" if p.starts_with('0') => {
            let lvl = if n == 0 { 0 } else { memo_u32(cx, "ml_on").filter(|v| *v > 0).unwrap_or(30) };
            one(format!("ML{lvl:03};"))
        }
        "ML" => one(format!("ML{:03};", level_down(cx, "ml", n, 100, 0, 60))),
        "PC" if !read => {
            cx.remember("pc", p);
            one(f.to_string())
        }
        "KS" | "PC" | "RT" | "XT" | "MC" | "PS" | "VX" | "UP" | "DN" => one(f.to_string()),
        "KP" if read => one("CW;".into()),
        "GT" if read => one("GT;".into()),
        "GT" => match n {
            1 => one("GT002;".into()),
            3 => one("GT004;".into()),
            _ => Vec::new(),
        },
        "PA" if read => one("PA;".into()),
        "PA" => one(format!("PA{};", n.min(2))),
        "RA" if read => one("RA;".into()),
        "RA" => match n {
            0 => one("RA00;".into()),
            1 => one("RA01;".into()),
            2 => one("RA05;".into()),
            3 => one("RA15;".into()),
            _ => Vec::new(),
        },
        "NB" if read => one("NB;".into()),
        "NB" => one(format!("NB{};", n.min(1))),
        "NL" if read => one("NL;".into()),
        "NL" => {
            let ii = cx.recall("nl_if").unwrap_or("00").to_string();
            one(format!("NL{:02}{ii};", n.min(21)))
        }
        "RF" if read => one("XF;".into()),
        "SH" if read => one("BW;".into()),
        "SH" => match widths(WidthClass::Ssb, false).into_iter().find(|(i, _)| *i as u32 == n) {
            Some((_, hz)) => one(format!("BW{:04};", hz / 10)),
            None => Vec::new(),
        },
        "AN" if read => one("AN;".into()),
        "AN" => one(format!("AN{};", n.clamp(1, cx.model.caps.antennas.max(1) as u32))),
        "MS" if read => one("TM;".into()),
        "MS" => one(format!("TM{};", int(p).unwrap_or(0).min(1))),
        "SM" if read => one("SM;".into()),
        "RM" => match (p, rig(cx)) {
            ("1", _) => one("SM;".into()),
            ("4" | "5", Rig::K3) => one("BG;".into()),
            ("5", _) => one("PO;".into()),
            ("6", _) => one("SW;".into()),
            _ => Vec::new(),
        },
        "TX" if read => one("TQ;".into()),
        "TX" => match p {
            "0" => {
                cx.remember("cat_tx", "0");
                one("RX;".into())
            }
            "1" => {
                cx.remember("cat_tx", "1");
                one("TX;".into())
            }
            _ => Vec::new(),
        },
        "FT" if read => one("FT;".into()),
        "FT" => match p {
            "1" | "3" => one("FT1;".into()),
            "0" | "2" => one("FR0;".into()),
            _ => Vec::new(),
        },
        "FR" | "VS" if read => one("FR;".into()),
        // The K3 always receives on VFO A: selecting it is nothing to do
        // (an extended-mode Set that changes nothing stands in).
        "VS" if p == "0" => one("K31;".into()),
        "LK" if read => vec!["LK;".into(), "LK$;".into()],
        "LK" => match int(p).unwrap_or(9) {
            v @ 0..=1 => one(format!("LK{v};")),
            v @ 2..=3 => one(format!("LK${};", v - 2)),
            v @ 4..=7 => vec![format!("LK{};", v & 1), format!("LK${};", (v >> 1) & 1)],
            _ => Vec::new(),
        },
        "RC" => {
            cx.remember("clar", "0");
            one("RC;".into())
        }
        "RU" | "RD" => {
            let step = int(p).unwrap_or(0) as i32;
            let now = cx.recall("clar").and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
            let off = (now + if code == "RU" { step } else { -step }).clamp(-9999, 9999);
            cx.remember("clar", off.to_string());
            one(format!("RO{};", offset_text(off)))
        }
        "AB" | "SV" => switch(cx, code).map(|s| one(s.into())).unwrap_or_default(),
        "BS" => one(format!("BN{:02};", int(p).unwrap_or(0))),
        "BU" | "BD" => {
            let bands: Vec<&Band> = cx.model.bands.iter().collect();
            let hz = cx.recall("hz").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            let here = bands.iter().position(|b| (b.lo..=b.hi).contains(&hz));
            let n = bands.len();
            let next = match (here, code) {
                (Some(i), "BU") => (i + 1) % n,
                (Some(i), _) => (i + n - 1) % n,
                (None, _) => bands.iter().position(|b| b.lo > hz).map_or(0, |i| if code == "BU" { i } else { (i + n - 1) % n }),
            };
            one(format!("BN{:02};", bands[next].code))
        }
        "EX" => menu_down(cx, p),
        _ => Vec::new(),
    }
}

/// The program put the radio in K2 extended mode (`K22;`), which changes
/// the `PC` answer to `PCnnnx`.

fn k22(cx: &Ctx) -> bool {
    matches!(cx.recall("k2"), Some("2" | "3"))
}

/// A RigHelm Answer as the radio would send it to a program.

fn to_program(cx: &mut Ctx, code: &str, p: &str) -> Vec<String> {
    let asked = cx.recall("asked").unwrap_or("").to_string();
    let one = |s: String| vec![s];
    let n = int(p.get(1..).unwrap_or("")).unwrap_or(0);
    match code {
        "ID" => one(format!("ID{p};")),
        "AI" => {
            let lvl = cx.recall("ai").filter(|_| p != "0").unwrap_or(if p == "0" { "0" } else { "2" }).to_string();
            one(format!("AI{lvl};"))
        }
        "FA" if asked == "BN" => {
            let hz = int(p).unwrap_or(0) as u64;
            let band = cx.model.band_of(hz).map_or(0, |b| b.code);
            one(format!("BN{band:02};"))
        }
        "FA" | "FB" => freq_digits(&format!("{code}{p};"), 11),
        "IF" => {
            let Some(info) = ChannelInfo::parse(p) else { return Vec::new() };
            if asked == "RO" {
                return one(format!("RO{};", offset_text(info.clar_offset)));
            }
            let Some((md, dt)) = info.mode.and_then(|m| k3_mode(cx, m)) else { return Vec::new() };
            let k31 = cx.recall("k3") == Some("1");
            let tx = cx.recall("tq") == Some("1");
            let split = cx.recall("ft") == Some("1");
            one(k3_if(info.freq, info.clar_offset, info.rx_clar, info.tx_clar, tx, md, split, if k31 { dt.unwrap_or(0) } else { 0 }))
        }
        "OI" => {
            let Some(info) = ChannelInfo::parse(p) else { return Vec::new() };
            match info.mode.and_then(|m| k3_mode(cx, m)) {
                Some((md, _)) => one(format!("MD${md};")),
                None => Vec::new(),
            }
        }
        "MD" => {
            let Some((md, dt)) = p.get(1..2).and_then(|c| Mode::from_code(c.chars().next()?)).and_then(|m| k3_mode(cx, m)) else {
                return Vec::new();
            };
            cx.remember("md", md.to_string());
            if let Some(d) = dt {
                note_dt(cx, d);
            }
            if asked == "DT" {
                let d = dt.map(u32::from).or_else(|| memo_u32(cx, "dt")).unwrap_or(0);
                return one(format!("DT{d};"));
            }
            one(format!("MD{md};"))
        }
        "AG" => one(format!("AG{:03};", n.min(255))),
        "RG" => {
            let (lo, hi) = rg_range(cx);
            one(format!("RG{:03};", scale(n, 0, 255, lo, hi)))
        }
        "SQ" => one(format!("SQ{:03};", scale(n, 0, 255, 0, 29))),
        "MG" => one(format!("MG{:03};", scale(int(p).unwrap_or(0), 0, 100, 0, 60))),
        "PL" => one(format!("CP{:03};", scale(int(p).unwrap_or(0), 0, 100, 0, 40))),
        "ML" if p.starts_with('1') => one(format!("ML{:03};", scale(n, 0, 100, 0, 60))),
        "PC" => {
            cx.remember("pc", p);
            let w = int(p).unwrap_or(0);
            match k22(cx) {
                true if w * 10 <= 120 => one(format!("PC{:03}0;", w * 10)),
                true => one(format!("PC{w:03}1;")),
                false => one(format!("PC{p};")),
            }
        }
        "KS" | "RT" | "XT" | "MC" | "PS" | "VX" => one(format!("{code}{p};")),
        "KP" => one(format!("CW{:02};", (int(p).unwrap_or(0) + 30).min(80))),
        // The manual has K22 mode add AGC on/off here (GTnnnx) and a 0 to
        // NB; Hamlib's K3 driver turns K22 on and wants neither, so neither.
        "GT" => one(if matches!(n, 3 | 6) { "GT004;" } else { "GT002;" }.into()),
        "PA" => one(format!("PA{};", n.min(2))),
        "RA" => one(match n {
            1 => "RA01;",
            2 => "RA05;",
            3 => "RA15;",
            _ => "RA00;",
        }
        .into()),
        "NB" => one(format!("NB{};", n.min(1))),
        "NL" => one(format!("NL{:02}00;", n.min(21))),
        "RF" => one(format!("XF{};", p.get(1..2).unwrap_or("1"))),
        "SH" => match cx.model.width_hz(WidthClass::Ssb, false, n as u8) {
            Some(hz) => one(format!("{}{:04};", if asked.ends_with('$') { &asked } else if asked == "FW" { "FW" } else { "BW" }, hz / 10)),

            None => Vec::new(),
        },
        "AN" => one(format!("AN{};", p.get(1..2).unwrap_or("1"))),
        "MS" => one(format!("TM{};", int(p).unwrap_or(0).min(1))),
        "SM" if asked == "BG" => {
            let v = int(p.get(1..).unwrap_or("")).unwrap_or(0).min(255) as u8;
            cx.remember("bg_s", rescale(v, &S_METER_CAL, &SM_K31).to_string());
            Vec::new()
        }
        "RM" if asked == "BG" && p.starts_with('5') => {
            let w = interpolate(&PO_CAL, n.min(255) as u8);
            if w < 0.5 {
                return one(format!("BG{:02}{};", memo_u32(cx, "bg_s").unwrap_or(0), if rig(cx) == Rig::K3 { "R" } else { "" }));
            }
            let qro = memo_u32(cx, "pc").is_some_and(|p| p > 12);
            let bars = ((if qro { w / 10.0 } else { w }).round() as u32).min(12);
            one(format!("BG{bars:02}{};", if rig(cx) == Rig::K3 { "T" } else { "" }))
        }
        "SM" if asked == "SMH" => {
            let v = int(p.get(1..).unwrap_or("")).unwrap_or(0).min(255) as u8;
            one(format!("SMH{:03};", rescale(v, &S_METER_CAL, &SMH_CAL)))
        }
        "SM" | "RM" if p.starts_with('1') || code == "SM" => {
            let v = int(p.get(1..).unwrap_or("")).unwrap_or(0).min(255) as u8;
            let k31 = cx.recall("k3") == Some("1");
            let s = rescale(v, &S_METER_CAL, if k31 { &SM_K31 } else { &SM_K30 });
            one(format!("SM{s:04};"))
        }
        "RM" if p.starts_with('6') => {
            let swr = interpolate(&SWR_CAL, n.min(255) as u8);
            one(format!("SW{:03};", ((swr * 10.0).round() as u32).clamp(10, 999)))
        }
        "RM" if p.starts_with('5') && rig(cx) != Rig::K3 => {
            let w = interpolate(&PO_CAL, n.min(255) as u8);
            one(format!("PO{:03};", ((w * 10.0).round() as u32).min(999)))
        }
        "TX" => {
            let on = p != "0";
            cx.remember("tq", (on as u8).to_string());
            one(format!("TQ{};", on as u8))
        }
        "FT" => {
            cx.remember("ft", p.to_string());
            one(format!("FT{p};"))
        }
        "FR" => one("FR0;".into()),
        "LK" => {
            let v = int(p).unwrap_or(4);
            if asked == "LK$" { one(format!("LK${};", (v >> 1) & 1)) } else { one(format!("LK{};", v & 1)) }
        }
        "EX" => menu_down(cx, p),
        _ => Vec::new(),
    }
}

// ------------------------------------------------------------- up

pub(crate) fn up(cx: &mut Ctx, f: &str) -> Vec<String> {
    // K2 and K3 (the meta-commands) aren't two letters.
    let meta = f.get(..2).filter(|c| matches!(*c, "K2" | "K3"));
    let Some((code, p)) = meta.map(|c| (c.to_string(), f[2..].trim_end_matches(';'))).or_else(|| parts(f)) else { return Vec::new() };
    let p = p.to_string();
    if cx.program { from_program(cx, &code, &p) } else { from_radio(cx, &code, &p) }
}

/// One of the radio's answers (or auto-information) as RigHelm's.
fn from_radio(cx: &mut Ctx, code: &str, p: &str) -> Vec<String> {
    let one = |s: String| vec![s];
    let n = int(p).unwrap_or(0);
    match code {
        "ID" => one(format!("ID{p};")),
        "AI" => one(format!("AI{};", (n != 0) as u8)),
        "FA" | "FB" => {
            let v = freq_digits(&format!("{code}{p};"), 8);
            if let Some(hz) = int(p) {
                cx.remember(if code == "FA" { "hz" } else { "hz_b" }, hz.to_string());
            }
            v
        }
        "IF" => {
            let Some((hz, off, rit, xit, md, dt)) = parse_k3_if(p) else { return Vec::new() };
            cx.remember("hz", hz.to_string());
            cx.remember("clar", off.to_string());
            cx.remember("md", md.to_string());
            if md == 6 || md == 9 {
                note_dt(cx, dt);
            }
            let mode = rh_mode(md, dt);
            let ch = memo_u32(cx, "mc").unwrap_or(0) as u16;
            let info = ChannelInfo { channel: ch, freq: hz, clar_offset: off, rx_clar: rit, tx_clar: xit, mode, ..Default::default() };
            let mut v = vec![info.to_answer("IF")];
            if let Some(m) = mode {
                v.push(md_frame(m));
            }
            v
        }
        "MD" if p.starts_with('$') => {
            let Some(md) = p.get(1..2).and_then(|c| c.parse::<u8>().ok()) else { return Vec::new() };
            let hz = cx.recall("hz_b").and_then(|s| s.parse().ok()).unwrap_or(0);
            let mode = rh_mode(md, memo_u32(cx, "dt").unwrap_or(0) as u8);
            one(ChannelInfo { freq: hz, mode, ..Default::default() }.to_answer("OI"))
        }
        "MD" => {
            cx.remember("md", p.to_string());
            mode_now(cx).map(|m| one(md_frame(m))).unwrap_or_default()
        }
        "DT" => {
            note_dt(cx, n as u8);
            match memo_u32(cx, "md") {
                Some(6 | 9) => mode_now(cx).map(|m| one(md_frame(m))).unwrap_or_default(),
                _ => Vec::new(),
            }
        }
        "AG" if !p.starts_with('$') => one(format!("AG0{:03};", n.min(255))),
        "RG" if !p.starts_with('$') => {
            let (lo, hi) = rg_range(cx);
            one(format!("RG0{:03};", level_up(cx, "rg", n, 255, lo, hi)))
        }
        "SQ" if !p.starts_with('$') => one(format!("SQ0{:03};", level_up(cx, "sq", n, 255, 0, 29))),
        "MG" => one(format!("MG{:03};", level_up(cx, "mg", n, 100, 0, 60))),
        "CP" => one(format!("PL{:03};", level_up(cx, "cp", n, 100, 0, 40))),
        "ML" => {
            if n > 0 {
                cx.remember("ml_on", n.to_string());
            }
            let lvl = level_up(cx, "ml", n, 100, 0, 60);
            vec![format!("ML0{:03};", (n > 0) as u8), format!("ML1{lvl:03};")]
        }
        "PC" => {
            cx.remember("pc", p);
            one(format!("PC{p};"))
        }
        "KS" | "RT" | "XT" | "PS" | "VX" => one(format!("{code}{p};")),
        "MC" => {
            cx.remember("mc", p.to_string());
            one(format!("MC{p};"))
        }
        "CW" => one(format!("KP{:02};", n.clamp(30, 80) - 30)),
        "GT" => {
            let speed = int(p.get(..3).unwrap_or(p)).unwrap_or(2);
            one(format!("GT0{};", if speed >= 4 { 3 } else { 1 }))
        }
        "PA" if !p.starts_with('$') => one(format!("PA0{};", n.min(2))),
        "RA" if !p.starts_with('$') => one(format!(
            "RA0{};",
            match n {
                0 => 0,
                5 => 2,
                15 => 3,
                _ => 1,
            }
        )),
        "NB" if !p.starts_with('$') => one(format!("NB0{};", p.get(..1).and_then(int).unwrap_or(0).min(1))),
        "NL" if !p.starts_with('$') && p.len() >= 2 => {
            if let Some(ii) = p.get(2..4) {
                cx.remember("nl_if", ii);
            }
            one(format!("NL0{:03};", int(&p[..2]).unwrap_or(0)))
        }
        "XF" if !p.starts_with('$') => one(format!("RF0{};", p.get(..1).unwrap_or("1"))),
        "BW" | "FW" if !p.starts_with('$') => one(format!("SH0{:02};", width_index(int(p.get(..4).unwrap_or(p)).unwrap_or(0) * 10))),
        "AN" => one(format!("AN0{n}0;")),
        "TM" => {
            cx.remember("tm", p.to_string());
            one(format!("MS{p};"))
        }
        "SM" if !p.starts_with('$') && !p.starts_with('H') => {
            let s = rescale(n.min(255) as u8, &SM_K31, &S_METER_CAL);
            vec![format!("SM0{s:03};"), format!("RM1{s:03};")]
        }
        // Bargraph: S-meter bars in receive, power (TM0) or ALC (TM1) bars
        // in transmit. 1 W a bar at QRP power settings, 10 W at QRO.
        "BG" => {
            let bars = int(p.get(..2).unwrap_or("0")).unwrap_or(0);
            let tx = p.ends_with('T');
            let (mut alc, mut po) = (0u8, 0u8);
            if tx && cx.recall("tm") == Some("1") {
                alc = scale(bars, 0, 7, 0, 255) as u8;
            } else if tx {
                let qro = memo_u32(cx, "pc").is_some_and(|w| w > 12);
                po = meter_raw((bars * if qro { 10 } else { 1 }) as f32, false, &PO_CAL);
            }
            vec![format!("RM4{alc:03};"), format!("RM5{po:03};")]
        }
        // Tenths of a watt, or watts with a KXPA100 at more than 15 W.
        "PO" => {
            let qro = memo_u32(cx, "pc").is_some_and(|w| w > 15);
            one(format!("RM5{:03};", meter_raw(if qro { n as f32 } else { n as f32 / 10.0 }, false, &PO_CAL)))
        }
        "SW" => one(format!("RM6{:03};", meter_raw(n as f32 / 10.0, true, &SWR_CAL))),
        "TQ" => {
            let cat = cx.recall("cat_tx") == Some("1");
            one(format!("TX{};", if n == 0 { 0 } else if cat { 1 } else { 2 }))
        }
        "FT" => one(format!("FT{};", n.min(1))),
        "FR" => vec!["FR0;".into(), "VS0;".into()],
        "LK" => {
            let (key, v) = if let Some(b) = p.strip_prefix('$') { ("lk_b", b) } else { ("lk_a", p) };
            cx.remember(key, v);
            let a = memo_u32(cx, "lk_a").unwrap_or(0) & 1;
            let b = memo_u32(cx, "lk_b").unwrap_or(0) & 1;
            one(format!("LK{};", 4 + a + 2 * b))
        }
        "MP" | "MQ" => {
            let Some(num) = memo_u32(cx, "menu") else { return Vec::new() };
            let Some(item) = cx.model.menu_item(num as u16) else { return Vec::new() };
            one(format!("EX{:03}{:0w$};", item.num, n, w = item.width as usize))
        }
        _ => Vec::new(),
    }
}

/// A program's Set or Read (as it would send the radio) as RigHelm's, or
/// an answer of the translator's own (`REPLY`).
fn from_program(cx: &mut Ctx, code: &str, p: &str) -> Vec<String> {
    let read = p.is_empty() || p == "$" || (code == "SM" && p == "H");
    if read {
        cx.remember("asked", format!("{code}{p}"));
    } else {
        cx.memo.remove("asked");
    }
    let one = |s: &str| vec![s.to_string()];
    let reply = |s: String| vec![format!("{REPLY}{s}")];
    let n = int(p).unwrap_or(0);
    let k = rig(cx);
    match code {
        // VFO B's filter: one width in RigHelm (the KX3 also answers BW for
        // BW$ in dual watch).
        "BW" | "FW" if p == "$" => one("SH0;"),
        _ if p.starts_with('$') && !matches!(code, "MD" | "LK") => Vec::new(),
        "ID" if read => one("ID;"),
        "K2" | "K3" if read => reply(format!("{code}{};", cx.recall(&code.to_lowercase()).unwrap_or("0"))),
        "K2" | "K3" => {
            cx.remember(&code.to_lowercase(), p);
            reply(String::new())
        }
        "OM" if read => reply(
            match k {
                Rig::K3 => "OM -P----------;",
                Rig::Kx3 => "OM ----------02;",
                Rig::Kx2 => "OM ----------01;",
            }
            .into(),
        ),
        // Firmware revisions; 99.99 is what the radio reports for a module
        // it doesn't have.
        "RV" if p.len() == 1 => reply(format!("RV{p}99.99;")),
        "AI" if read => one("AI;"),
        "AI" => {
            cx.remember("ai", p);
            vec![format!("AI{};", (n != 0) as u8)]
        }
        "FA" | "FB" if read => vec![format!("{code};")],
        "FA" | "FB" => freq_digits(&format!("{code}{p};"), 8),
        "IF" if read => one("IF;"),
        "RO" if read => one("IF;"),
        "MD" if p == "$" => one("OI;"),
        "MD" if read => one("MD0;"),
        "MD" => {
            let Some(md) = p.get(..1).and_then(|c| c.parse::<u8>().ok()) else { return Vec::new() };
            cx.remember("md", md.to_string());
            mode_now(cx).map(|m| vec![md_frame(m)]).unwrap_or_default()
        }
        "DT" if read => one("MD0;"),
        "DT" => {
            note_dt(cx, n as u8);
            match memo_u32(cx, "md") {
                Some(6 | 9) => mode_now(cx).map(|m| vec![md_frame(m)]).unwrap_or_default(),
                _ => reply(String::new()),
            }
        }
        "AG" | "RG" | "SQ" if read => vec![format!("{code}0;")],
        "AG" => vec![format!("AG0{:03};", n.min(255))],
        "RG" => {
            let (lo, hi) = rg_range(cx);
            vec![format!("RG0{:03};", scale(n, lo, hi, 0, 255))]
        }
        "SQ" => vec![format!("SQ0{:03};", scale(n, 0, 29, 0, 255))],
        "MG" if read => one("MG;"),
        "MG" => vec![format!("MG{:03};", scale(n, 0, 60, 0, 100))],
        "CP" if read => one("PL;"),
        "CP" => vec![format!("PL{:03};", scale(n, 0, 40, 0, 100))],
        "ML" if read => one("ML1;"),
        "ML" => vec![format!("ML1{:03};", scale(n, 0, 60, 0, 100))],
        // K22 mode: PCnnnx, x = 0 for tenths of a watt, 1 for watts.
        "PC" if p.len() == 4 => {
            let w = if p.ends_with('0') { (int(&p[..3]).unwrap_or(0) as f32 / 10.0).round() as u32 } else { int(&p[..3]).unwrap_or(0) };
            vec![format!("PC{w:03};")]
        }
        "KS" | "PC" | "RT" | "XT" | "MC" | "PS" | "VX" | "RC" => vec![format!("{code}{p};")],
        "CW" if read => one("KP;"),
        "GT" if read => one("GT0;"),
        "GT" => vec![format!("GT0{};", if p.get(..3) == Some("004") { 3 } else { 1 })],
        "PA" | "RA" | "NB" | "NL" | "AN" if read => vec![format!("{code}0;")],
        "PA" => vec![format!("PA0{};", n.min(2))],
        "RA" => vec![format!(
            "RA0{};",
            match n {
                0 => 0,
                5 => 2,
                15 => 3,
                _ => 1,
            }
        )],
        "NB" => vec![format!("NB0{};", p.get(..1).and_then(int).unwrap_or(0).min(1))],
        "NL" => vec![format!("NL0{:03};", p.get(..2).and_then(int).unwrap_or(0))],
        "AN" => vec![format!("AN0{};", n.clamp(1, cx.model.caps.antennas.max(1) as u32))],
        "XF" if read => one("RF0;"),
        "BW" | "FW" if read => one("SH0;"),
        "BW" | "FW" => vec![format!("SH0{:02};", width_index(int(p.get(..4).unwrap_or(p)).unwrap_or(0) * 10))],
        "TM" if read => one("MS;"),
        "TM" => vec![format!("MS{};", n.min(1))],
        "SM" if read || p == "H" => one("SM0;"),
        "SW" if read => one("RM6;"),
        "PO" if read && k != Rig::K3 => one("RM5;"),
        // The K3's bargraph: S-meter bars, or power bars while sending.
        // (The R/T flag is the K3's only.)
        "BG" if read => vec!["SM0;".into(), "RM5;".into()],

        "TQ" if read => one("TX;"),
        // `TX;` and `RX;` are the radio's Sets (PTT on and off).
        "TX" | "RX" if read => {
            cx.remember("tq", if code == "TX" { "1" } else { "0" });
            one(if code == "TX" { "TX1;" } else { "TX0;" })
        }

        "FT" if read => one("FT;"),
        "FT" => one(if n == 1 { "FT3;" } else { "FT2;" }),
        "FR" if read => one("FR;"),
        "FR" => one("FT2;"),
        "LK" if read => one("LK;"),
        "LK" => match p.strip_prefix('$') {
            Some(b) => vec![format!("LK{};", 2 + int(b).unwrap_or(0).min(1))],
            None => vec![format!("LK{};", n.min(1))],
        },
        "RO" => {
            let v: i32 = p.trim().trim_start_matches('+').parse().unwrap_or(0);
            let mut out = vec!["RC;".to_string()];
            if v != 0 {
                out.push(format!("{}{:04};", if v > 0 { "RU" } else { "RD" }, v.unsigned_abs().min(9999)));
            }
            out
        }
        "RU" | "RD" => vec![format!("{code}0010;")],
        "UP" | "DN" if !p.starts_with('B') => vec![format!("{code};")],
        "BN" if read => one("FA;"),
        "BN" => vec![format!("BS{:02};", n)],
        "SW" => switch_up(cx, p).map(|c| vec![format!("{c};")]).unwrap_or_default(),
        "MN" if read => {
            let addr = memo_u32(cx, "menu").and_then(|m| cx.model.menu_item(m as u16)).map_or(255, |i| i.addr);
            reply(format!("MN{addr:03};"))
        }
        "MN" => {
            match cx.model.menu.iter().find(|m| m.addr == n) {
                Some(item) => cx.remember("menu", item.num.to_string()),
                None => {
                    cx.memo.remove("menu");
                }
            }
            reply(String::new())
        }
        "MP" | "MQ" => {
            let Some(item) = memo_u32(cx, "menu").and_then(|m| cx.model.menu_item(m as u16)) else { return Vec::new() };
            if (item.width == 5) != (code == "MQ") {
                return Vec::new();
            }
            if read { vec![format!("EX{:03};", item.num)] } else { vec![format!("EX{:03}{:0w$};", item.num, n, w = item.width as usize)] }
        }
        _ => Vec::new(),
    }
}

// ------------------------------------------------------------- the radios

/// RigHelm commands the K3 can carry out (through its own).
const K3_COMMANDS: &[&str] = &[
    "AB", "AG", "AI", "AN", "BD", "BS", "BU", "DN", "EX", "FA", "FB", "FR", "FT", "GT", "ID", "IF", "KP", "KS", "LK", "MC", "MD", "MG",
    "ML", "MS", "NB", "NL", "OI", "PA", "PC", "PL", "PS", "RA", "RC", "RD", "RF", "RG", "RM", "RT", "RU", "SH", "SM", "SQ", "SV", "TX",
    "UP", "VS", "VX", "XT",
];

/// The KX3: no antenna switch, no TX meter switch, VOX only readable.
const KX3_COMMANDS: &[&str] = &[
    "AB", "AG", "AI", "BD", "BS", "BU", "DN", "EX", "FA", "FB", "FR", "FT", "GT", "ID", "IF", "KP", "KS", "LK", "MC", "MD", "MG", "ML",
    "NB", "NL", "OI", "PA", "PC", "PL", "PS", "RA", "RC", "RD", "RF", "RG", "RM", "RT", "RU", "SH", "SM", "SQ", "SV", "TX", "UP", "VS",
    "XT",
];

/// The KX2: as the KX3, and only DSP filters (no roofing selection).
const KX2_COMMANDS: &[&str] = &[
    "AB", "AG", "AI", "BD", "BS", "BU", "DN", "EX", "FA", "FB", "FR", "FT", "GT", "ID", "IF", "KP", "KS", "LK", "MC", "MD", "MG", "ML",
    "NB", "NL", "OI", "PA", "PC", "PL", "PS", "RA", "RC", "RD", "RG", "RM", "RT", "RU", "SH", "SM", "SQ", "SV", "TX", "UP", "VS", "XT",
];

const fn band(code: u8, label: &'static str, lo: u64, hi: u64) -> Band {
    Band { code, label, lo, hi }
}

/// `BN` band numbers 0-10 (160-6 m).
pub(crate) const BANDS_160_6: &[Band] = &[
    band(0, "160", 1_800_000, 2_000_000),
    band(1, "80", 3_500_000, 4_000_000),
    band(2, "60", 5_250_000, 5_450_000),
    band(3, "40", 7_000_000, 7_300_000),
    band(4, "30", 10_100_000, 10_150_000),
    band(5, "20", 14_000_000, 14_350_000),
    band(6, "17", 18_068_000, 18_168_000),
    band(7, "15", 21_000_000, 21_450_000),
    band(8, "12", 24_890_000, 24_990_000),
    band(9, "10", 28_000_000, 29_700_000),
    band(10, "6", 50_000_000, 54_000_000),
];

/// The KX2: 80-10 m.
const BANDS_80_10: &[Band] = &[
    band(1, "80", 3_500_000, 4_000_000),
    band(2, "60", 5_250_000, 5_450_000),
    band(3, "40", 7_000_000, 7_300_000),
    band(4, "30", 10_100_000, 10_150_000),
    band(5, "20", 14_000_000, 14_350_000),
    band(6, "17", 18_068_000, 18_168_000),
    band(7, "15", 21_000_000, 21_450_000),
    band(8, "12", 24_890_000, 24_990_000),
    band(9, "10", 28_000_000, 29_700_000),
];

/// Front-panel order: LSB/USB, CW, DATA (DATA A, AFSK A / FSK D, PSK D),
/// AM, FM.
pub(crate) const MODES: &[Mode] = &[
    Mode::Lsb,
    Mode::Usb,
    Mode::Cw,
    Mode::CwR,
    Mode::DataUsb,
    Mode::DataLsb,
    Mode::RttyLsb,
    Mode::RttyUsb,
    Mode::Psk,
    Mode::Am,
    Mode::Fm,
];

/// The KX2 has no FM.
const MODES_KX2: &[Mode] =
    &[Mode::Lsb, Mode::Usb, Mode::Cw, Mode::CwR, Mode::DataUsb, Mode::DataLsb, Mode::RttyLsb, Mode::RttyUsb, Mode::Psk, Mode::Am];

const FL: &[(char, &str)] = &[('1', "FL1"), ('2', "FL2"), ('3', "FL3"), ('4', "FL4"), ('5', "FL5")];

const K3_CAPS: Caps = Caps {
    commands: K3_COMMANDS,
    widths,
    narrow: false,
    preamp: &[(0, "OFF"), (1, "ON"), (2, "PRE 2")],
    // 5 and 15 dB: the K3S only.
    att: &[(0, "OFF"), (1, "10 dB"), (2, "5 dB"), (3, "15 dB")],
    agc: &[(1, "FAST"), (3, "SLOW")],
    // Read only: XFIL is chosen with the radio's own switch.
    roofing: &[],
    roofing_answers: FL,
    nb: &[(0, "OFF"), (1, "ON")],
    nr_max: 0,
    nb_level_max: 21,
    antennas: 2,
    power: (0, 110),
    meter_select: &[(0, "RF/SWR"), (1, "CMP/ALC")],
    voice_memories: 0,
    keyer_memories: 0,
    keyer_memory_len: 0,
    memory: Memory { first: 0, last: 99, pms: None },
    shared_clarifier: true,
};

/// What the simulator starts with where the FTDX3000's values don't fit:
/// a 2.7 kHz filter, 50 W.
const SIM: &[&str] = &["SH037;", "PC050;"];

pub static K3: RadioModel = RadioModel {
    key: "k3",
    name: "K3",
    maker: Maker::Elecraft,
    family: Family::Kenwood,
    proto: Proto::Text(&SPEC),
    id: "017",
    civ_addr: 0,
    hamlib: 2029,
    // 100 kHz with the KSYN3A synthesizer (standard in the K3S), else 490.
    freq_min: 100_000,
    freq_max: 54_000_000,
    bands: BANDS_160_6,
    modes: MODES,
    menu: K3_MENU,
    roles: Roles::NONE,
    caps: K3_CAPS,
    bauds: &[38400, 19200, 9600, 4800],
    stop_bits: 1,
    rts_cts: false,
    sim: SIM,
};

pub static KX3: RadioModel = RadioModel {
    key: "kx3",
    name: "KX3",
    maker: Maker::Elecraft,
    family: Family::Kenwood,
    proto: Proto::Text(&SPEC),
    id: "017",
    civ_addr: 0,
    hamlib: 2045,
    freq_min: 500_000,
    freq_max: 54_000_000,
    bands: BANDS_160_6,
    modes: MODES,
    menu: KX3_MENU,
    roles: Roles { cat_items: &[12], ..Roles::NONE },
    caps: Caps {
        commands: KX3_COMMANDS,
        preamp: &[(0, "OFF"), (1, "ON")],
        att: &[(0, "OFF"), (1, "10 dB")],
        antennas: 1,
        // Without the KXPA100 amplifier.
        power: (0, 15),
        meter_select: &[],
        ..K3_CAPS
    },
    bauds: &[38400, 19200, 9600, 4800],
    stop_bits: 1,
    rts_cts: false,
    sim: &["SH037;", "PC005;"],
};

pub static KX2: RadioModel = RadioModel {
    key: "kx2",
    name: "KX2",
    hamlib: 2044,
    freq_max: 32_000_000,
    bands: BANDS_80_10,
    modes: MODES_KX2,
    menu: KX2_MENU,
    roles: Roles { cat_items: &[8], ..Roles::NONE },
    caps: Caps { commands: KX2_COMMANDS, roofing_answers: &[('1', "DSP")], ..KX3.caps },
    ..KX3
};

#[cfg(test)]
mod tests {
    use super::*;

    fn radio(m: &'static RadioModel) -> Ctx {
        Ctx::new(m)
    }

    fn program(m: &'static RadioModel) -> Ctx {
        Ctx { program: true, ..Ctx::new(m) }
    }

    fn d(cx: &mut Ctx, f: &str) -> Vec<String> {
        down(cx, f)
    }

    fn u(cx: &mut Ctx, f: &str) -> Vec<String> {
        up(cx, f)
    }

    #[test]
    fn frames_from_the_manual() {
        let cx = &mut radio(&K3);
        assert_eq!(d(cx, "FA14060000;"), ["FA00014060000;"]);
        assert_eq!(u(cx, "FA00014060000;"), ["FA14060000;"]);
        assert_eq!(d(cx, "FA;"), ["FA;"]);
        assert_eq!(d(cx, "ID;"), ["K31;ID;"]);
        assert_eq!(answer_prefix("K31;ID;"), "ID");
        assert_eq!(u(cx, "ID017;"), ["ID017;"]);
        assert_eq!(d(cx, "AI1;"), ["K31;", "AI2;"]);
        assert_eq!(d(cx, "AI0;"), ["K31;", "AI0;"]);
        assert_eq!(u(cx, "AI2;"), ["AI1;"]);
        assert_eq!(d(cx, "KS020;"), ["KS020;"]);
        assert_eq!(d(cx, "MD01;"), ["MD1;"]);
        assert_eq!(u(cx, "MD1;"), ["MD01;"]);
        assert_eq!(d(cx, "MD0C;"), ["MD6;", "DT0;"]);
        assert_eq!(d(cx, "MD08;"), ["MD9;", "DT0;"]);
        assert_eq!(d(cx, "MD0I;"), ["MD6;", "DT3;"]);
        assert_eq!(d(cx, "MD06;"), ["MD6;", "DT1;"]);
        assert_eq!(d(cx, "MD0;"), ["IF;"]);
        // IF with K31: the DATA sub-mode (here FSK D) is the digit before
        // the last "1".
        let k3if = "IF00014074000     -001210 0006000021 ;";
        assert_eq!(k3if, k3_if(14_074_000, -12, true, false, false, 6, false, 2));
        assert_eq!(u(cx, k3if), ["IF00014074000-001210600000;", "MD06;"]);
        assert_eq!(u(cx, "IF00014074000     +000000 0002000001 ;"), ["IF00014074000+000000200000;", "MD02;"]);
        assert_eq!(u(cx, "DT0;"), Vec::<String>::new());
        // DATA A reversed, then PSK D reversed.
        assert_eq!(u(cx, "MD9;"), ["MD08;"]);
        assert_eq!(u(cx, "DT3;"), ["MD0I;"]);
        assert_eq!(d(cx, "AG0128;"), ["AG128;"]);
        assert_eq!(u(cx, "AG128;"), ["AG0128;"]);
        assert_eq!(d(cx, "RG0127;"), ["RG125;"]);
        assert_eq!(u(cx, "RG125;"), ["RG0127;"]);
        assert_eq!(u(cx, "RG250;"), ["RG0255;"]);
        assert_eq!(d(cx, "SQ0255;"), ["SQ029;"]);
        assert_eq!(d(cx, "MG050;"), ["MG030;"]);
        assert_eq!(u(cx, "MG030;"), ["MG050;"]);
        assert_eq!(d(cx, "PL100;"), ["CP040;"]);
        assert_eq!(u(cx, "CP020;"), ["PL050;"]);
        assert_eq!(d(cx, "GT01;"), ["GT002;"]);
        assert_eq!(d(cx, "GT03;"), ["GT004;"]);
        assert_eq!(u(cx, "GT004;"), ["GT03;"]);
        assert_eq!(u(cx, "GT0021;"), ["GT01;"]);
        assert_eq!(d(cx, "RA01;"), ["RA01;"]);
        assert_eq!(d(cx, "RA02;"), ["RA05;"]);
        assert_eq!(u(cx, "RA10;"), ["RA01;"]);
        assert_eq!(u(cx, "RA15;"), ["RA03;"]);
        assert_eq!(d(cx, "PA02;"), ["PA2;"]);
        assert_eq!(u(cx, "PA1;"), ["PA01;"]);
        assert_eq!(u(cx, "NL0507;"), ["NL0005;"]);
        assert_eq!(d(cx, "NL0010;"), ["NL1007;"]);
        assert_eq!(u(cx, "NB10;"), ["NB01;"]);
        assert_eq!(d(cx, "SH037;"), ["BW0270;"]);
        assert_eq!(u(cx, "BW0270;"), ["SH037;"]);
        assert_eq!(u(cx, "BW0020;"), ["SH004;"]);
        assert_eq!(u(cx, "XF3;"), ["RF03;"]);
        assert_eq!(d(cx, "RF01;"), Vec::<String>::new());
        assert_eq!(u(cx, "CW60;"), ["KP30;"]);
        assert_eq!(d(cx, "TX1;"), ["TX;"]);
        assert_eq!(u(cx, "TQ1;"), ["TX1;"]);
        assert_eq!(d(cx, "TX0;"), ["RX;"]);
        assert_eq!(u(cx, "TQ1;"), ["TX2;"]);
        assert_eq!(d(cx, "TX;"), ["TQ;"]);
        assert_eq!(d(cx, "FT3;"), ["FT1;"]);
        assert_eq!(d(cx, "FT2;"), ["FR0;"]);
        assert_eq!(u(cx, "FR0;"), ["FR0;", "VS0;"]);
        assert_eq!(d(cx, "LK;"), ["LK;", "LK$;"]);
        assert_eq!(u(cx, "LK1;"), ["LK5;"]);
        assert_eq!(u(cx, "LK$1;"), ["LK7;"]);
        assert_eq!(d(cx, "LK6;"), ["LK0;", "LK$1;"]);
        assert_eq!(u(cx, "SM0009;"), ["SM0130;", "RM1130;"]);
        assert_eq!(u(cx, "SM0021;"), ["SM0255;", "RM1255;"]);
        assert_eq!(u(cx, "SW015;"), ["RM6052;"]);
        assert_eq!(d(cx, "RM5;"), ["BG;"]);
        assert_eq!(u(cx, "BG05R;"), ["RM4000;", "RM5000;"]);
        assert_eq!(u(cx, "PC100;"), ["PC100;"]);
        assert_eq!(u(cx, "BG10T;"), ["RM4000;", "RM5161;"]);
        assert_eq!(u(cx, "AN2;"), ["AN020;"]);
        assert_eq!(d(cx, "AN02;"), ["AN2;"]);
        assert_eq!(u(cx, "TM1;"), ["MS1;"]);
        assert_eq!(u(cx, "BG07T;"), ["RM4255;", "RM5000;"]);
        assert_eq!(d(cx, "RU0100;"), ["RO+0100;"]);
        assert_eq!(d(cx, "RC;"), ["RC;"]);
        assert_eq!(d(cx, "RD0050;"), ["RO-0050;"]);
        assert_eq!(d(cx, "AB;"), ["SWT13;"]);
        assert_eq!(d(cx, "SV;"), ["SWT11;"]);
        assert_eq!(d(cx, "BS05;"), ["BN05;"]);
        assert_eq!(d(cx, "VS0;"), ["K31;"]);
        assert_eq!(d(cx, "VS1;"), Vec::<String>::new());
        // The menu: select, read or set, leave.
        assert_eq!(d(cx, "EX003;"), ["MN004;MP;MN255;"]);
        assert_eq!(answer_prefix("MN004;MP;MN255;"), "MP");
        assert_eq!(u(cx, "MP002;"), ["EX003002;"]);
        assert_eq!(d(cx, "EX008001;"), ["MN019;", "MP001;", "MN255;"]);
        assert_eq!(d(cx, "EX0300;"), Vec::<String>::new());
    }

    #[test]
    fn band_up_and_down_follow_the_dial() {
        let cx = &mut radio(&K3);
        assert_eq!(u(cx, "FA00014074000;"), ["FA14074000;"]);
        assert_eq!(d(cx, "BU0;"), ["BN06;"]);
        assert_eq!(d(cx, "BD0;"), ["BN04;"]);
        u(cx, "FA00050100000;");
        assert_eq!(d(cx, "BU0;"), ["BN00;"]);
        let cx = &mut radio(&KX2);
        u(cx, "FA00028100000;");
        assert_eq!(d(cx, "BU0;"), ["BN01;"]);
    }

    #[test]
    fn kx3_and_kx2_differences() {
        let cx = &mut radio(&KX3);
        assert_eq!(d(cx, "RG0255;"), ["RG250;"]);
        assert_eq!(d(cx, "RG0000;"), ["RG190;"]);
        assert_eq!(u(cx, "RG220;"), ["RG0128;"]);
        assert_eq!(d(cx, "RM5;"), ["PO;"]);
        assert_eq!(u(cx, "PO050;"), ["RM5028;"]);
        assert_eq!(d(cx, "AB;"), ["SWT25;"]);
        assert_eq!(d(cx, "AN02;"), Vec::<String>::new());
        assert_eq!(d(cx, "EX040;"), ["MN132;MQ;MN255;"]);
        assert_eq!(u(cx, "MQ01234;"), ["EX04001234;"]);
        assert_eq!(d(cx, "EX002128;"), ["MN001;", "MP128;", "MN255;"]);
        let cx = &mut radio(&KX2);
        assert_eq!(d(cx, "AB;"), ["SWH44;"]);
        assert_eq!(d(cx, "SV;"), ["SWT44;"]);
        assert_eq!(d(cx, "MD04;"), Vec::<String>::new());
        assert_eq!(d(cx, "RF0;"), Vec::<String>::new());
        assert_eq!(d(cx, "EX038;"), ["MN148;MP;MN255;"]);
    }

    /// A program talking to the simulated radio: its commands come up,
    /// RigHelm's answers go down as the radio's.
    #[test]
    fn as_seen_by_a_program() {
        let cx = &mut program(&K3);
        assert_eq!(u(cx, "K3;"), ["=K30;"]);
        assert_eq!(u(cx, "K31;"), ["="]);
        assert_eq!(u(cx, "K3;"), ["=K31;"]);
        assert_eq!(u(cx, "TX;"), ["TX1;"]);
        assert_eq!(u(cx, "RX;"), ["TX0;"]);
        assert_eq!(u(cx, "TQ;"), ["TX;"]);
        assert_eq!(d(cx, "TX0;"), ["TQ0;"]);
        assert_eq!(d(cx, "TX2;"), ["TQ1;"]);
        assert_eq!(u(cx, "MD;"), ["MD0;"]);
        assert_eq!(d(cx, "MD0C;"), ["MD6;"]);
        assert_eq!(u(cx, "DT;"), ["MD0;"]);
        assert_eq!(d(cx, "MD0C;"), ["DT0;"]);
        assert_eq!(u(cx, "MD6;"), ["MD0C;"]);
        assert_eq!(u(cx, "DT1;"), ["MD06;"]);
        assert_eq!(u(cx, "MD9;"), ["MD09;"]);
        assert_eq!(u(cx, "MD$;"), ["OI;"]);
        assert_eq!(d(cx, "OI00014076000+000000300000;"), ["MD$3;"]);
        assert_eq!(u(cx, "IF;"), ["IF;"]);
        // Transmitting (TX2 above), DATA A, RIT on; no sub-mode in K30 mode.
        assert_eq!(d(cx, "IF00114074000+012010C00000;"), ["IF00014074000     +012010 0016000001 ;"]);
        assert_eq!(u(cx, "FA00007074000;"), ["FA07074000;"]);
        assert_eq!(d(cx, "FA07074000;"), ["FA00007074000;"]);
        assert_eq!(u(cx, "BN;"), ["FA;"]);
        assert_eq!(d(cx, "FA07074000;"), ["BN03;"]);
        assert_eq!(u(cx, "BN05;"), ["BS05;"]);
        assert_eq!(u(cx, "LK$;"), ["LK;"]);
        assert_eq!(d(cx, "LK6;"), ["LK$1;"]);
        assert_eq!(u(cx, "LK$0;"), ["LK2;"]);
        assert_eq!(u(cx, "RG196;"), ["RG0200;"]);
        assert_eq!(d(cx, "RG0200;"), ["RG196;"]);
        assert_eq!(u(cx, "SM;"), ["SM0;"]);
        assert_eq!(d(cx, "SM0130;"), ["SM0009;"]);
        assert_eq!(u(cx, "SWT13;"), ["AB;"]);
        assert_eq!(u(cx, "SWT11;"), ["SV;"]);
        assert_eq!(u(cx, "SWT99;"), Vec::<String>::new());
        assert_eq!(u(cx, "RO+0100;"), ["RC;", "RU0100;"]);
        assert_eq!(u(cx, "RO;"), ["IF;"]);
        assert_eq!(d(cx, "IF00114074000-012010C00000;"), ["RO-0120;"]);
        // The menu, the way RigHelm (or K3 Utility) does it.
        assert_eq!(u(cx, "MN019;"), ["="]);
        assert_eq!(u(cx, "MN;"), ["=MN019;"]);
        assert_eq!(u(cx, "MP;"), ["EX008;"]);
        assert_eq!(d(cx, "EX0081;"), ["MP001;"]);
        assert_eq!(u(cx, "MP000;"), ["EX008000;"]);
        assert_eq!(u(cx, "MN255;"), ["="]);
        assert_eq!(u(cx, "MP;"), Vec::<String>::new());
        assert_eq!(u(cx, "OM;"), ["=OM -P----------;"]);
    }

    /// RigHelm's answers made into the radio's by the simulator, read back
    /// by RigHelm's side: the same; and RigHelm's Sets, made into the
    /// radio's, taken by the simulator: the same Sets.
    #[test]
    fn round_trips() {
        for m in [&K3, &KX3, &KX2] {
            let answers = [
                "FA14074000;", "FB07074000;", "AG0128;", "RG0200;", "MG040;", "PL050;", "KS025;", "PC010;", "GT01;", "GT03;",
                "PA01;", "RA00;", "RA01;", "NB01;", "NL0005;", "SH037;", "SH010;", "KP40;", "TX0;", "FT1;", "FT0;", "RT1;", "XT0;",
                "MC005;", "PS1;", "AI0;", "ID017;", "SM0130;", "SM0000;", "RM6052;", "IF00014074000+012010200000;",
                "IF00007074000-000500C00000;", "LK5;",
            ];
            for a in answers {
                let (code, _) = parts(a).unwrap();
                if !m.has(&code) {
                    continue;
                }
                let mut prog = program(m);
                prog.remember("k3", "1");
                let native = d(&mut prog, a);
                assert_eq!(native.len(), 1, "{} {a} -> {native:?}", m.key);
                let back = u(&mut radio(m), &native[0]);
                assert!(back.contains(&a.to_string()), "{} {a} -> {native:?} -> {back:?}", m.key);
            }
            let sets = [
                "FA14074000;", "FB07074000;", "AG0128;", "RG0200;", "MG040;", "PL050;", "KS025;", "PC010;", "GT01;", "GT03;",
                "PA01;", "RA01;", "NB01;", "NL0005;", "SH037;", "TX1;", "TX0;", "FT3;", "FT2;", "RT1;", "XT0;", "MC005;", "AB;",
                "SV;", "BS03;", "LK1;", "LK3;", "RC;", "MD01;", "MD02;", "MD03;", "MD07;", "MD05;", "MD04;", "MD0C;", "MD08;",
                "MD06;", "MD09;", "MD0I;", "EX001000;", "EX002128;",
            ];
            for s in sets {
                let (code, _) = parts(s).unwrap();
                if !m.has(&code) || (code == "MD" && !m.modes.contains(&Mode::from_code(s.as_bytes()[3] as char).unwrap())) {
                    continue;
                }
                let native = d(&mut radio(m), s);
                assert!(!native.is_empty(), "{} {s}", m.key);
                let mut prog = program(m);
                let back: Vec<String> = native.iter().flat_map(|n| u(&mut prog, n)).filter(|f| !f.starts_with(REPLY)).collect();
                let want = match s {
                    "FT2;" => "FT2;",
                    _ => s,
                };
                assert_eq!(back.last().map(String::as_str), Some(want), "{} {s} -> {native:?} -> {back:?}", m.key);
            }
        }
    }

    #[test]
    fn menus_are_numbered_in_order() {
        for m in [&K3, &KX3, &KX2] {
            for (i, item) in m.menu.iter().enumerate() {
                assert_eq!(item.num as usize, i + 1, "{} {}", m.key, item.name);
            }
            let mut addrs: Vec<u32> = m.menu.iter().map(|i| i.addr).collect();
            addrs.dedup();
            assert_eq!(addrs.len(), m.menu.len(), "{}", m.key);
        }
        assert_eq!(K3.menu.len(), 12);
        assert_eq!(KX3.menu.len(), 55);
        assert_eq!(KX2.menu.len(), 40);
        assert_eq!(KX3.menu_item(12).unwrap().name, "AUTOINF");
        assert_eq!(KX2.menu_item(8).unwrap().name, "AUTOINF");
    }
}
