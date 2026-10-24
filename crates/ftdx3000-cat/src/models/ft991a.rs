//! Yaesu FT-991A (`ID0670;`) and FT-991 (`ID0570;`): HF/50/144/430 MHz,
//! C4FM.
//!
//! From the *FT-991A CAT Operation Reference Book*. The FT-991 answers
//! `ID0570;` instead of `ID0670;` but has the same CAT commands and menu
//! (Hamlib drives both with one model, 1035); it is listed separately only so
//! that either is detected.
//!
//! How its commands differ from the FTDX3000's:
//!
//! * `FA`/`FB` frequencies have 9 digits (up to 470 MHz), and so do the
//!   frequencies in `IF`/`OI`/`MR` answers and `MW` sets.
//! * `EX` menu numbers are the radio's own, 3 digits (`EX032`), 153 items.
//! * `CN` has a CTCSS/DCS selector and a 3-digit number: `CN00012;` is
//!   RigHelm's `CN012;`.
//! * `CO` has four parts (contour on/off, contour Hz, APF on/off, APF
//!   offset) where the FTDX3000 has three (`CO00` off/contour/APF, `CO01`
//!   x100 Hz, `CO02` APF).
//! * `SQ` is 0-100 (RigHelm 0-255).
//! * `LK` and `FS` only have VFO-A's switch (RigHelm reports 4-7).
//! * `DA`: LED level (1-2) and TFT level; there is no VFO display level.
//! * `RA` is on/off, `NB` on/off, `NL` 0-10, manual notch up to 3200 Hz.
//! * No `AN`, `EM`, `EN`, `FR`, `RF`, `RO`, `SF`, `VF`, `VS`: one antenna
//!   jack per band group, no sub receiver, no roofing filters, no rotator
//!   control, no VRF. The VFOs are swapped with `SV`; split is `FT`.
//!
//! Meters (`SM`, `RM`) are 0-255 like the FTDX3000's; the manual gives no
//! other calibration, so they pass unchanged.

use crate::dialect::text::{Ctx, TextSpec, freq_digits, info_digits, menu_down, menu_up, parts, read_prefix};
use crate::menu::{Kind, MenuItem, num, signed};
use crate::model::{ALL_COMMANDS, Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::types::{self, Band, Mode, WidthClass};

pub static SPEC: TextSpec = TextSpec { down, up, answer_prefix: read_prefix };

/// The FT-991A's menu numbers have 3 digits.
const MENU_DIGITS: usize = 3;

fn down(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !cx.model.has(&code) {
        return Vec::new();
    }
    match code.as_str() {
        "FA" | "FB" => freq_digits(f, 9),
        "IF" | "OI" | "MR" => info_digits(f, 9),
        "MW" => mw_down(f),
        "EX" => menu_down(cx, f, MENU_DIGITS),
        "MD" => mode_check(cx, f, p),
        "BS" => band_check(cx, f, p),
        "CN" => cn_down(p),
        "CO" => co_down(p),
        "DA" => da_down(f, p),
        "FS" | "LK" => vfo_a_switch_down(&code, f, p),
        "SQ" => level_down(cx, f, 1, 255, 100),
        "BP" => bp_down(f, p),
        _ => vec![f.to_string()],
    }
}

fn up(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !cx.model.has(&code) {
        return Vec::new();
    }
    match code.as_str() {
        "FA" | "FB" => freq_digits(f, 8),
        "IF" | "OI" | "MR" | "MW" => info_digits(f, 8),
        "EX" => menu_up(cx, f, MENU_DIGITS),
        "MD" => mode_check(cx, f, p),
        "BS" => band_check(cx, f, p),
        "CN" => cn_up(p),
        "CO" => co_up(cx, p),
        "FS" | "LK" => vfo_a_switch_up(&code, f, p),
        "SQ" => level_up(cx, f, 1, 255, 100),
        _ => vec![f.to_string()],
    }
}

/// Only the modes the radio has (`MD0x`), both ways.
pub(crate) fn mode_check(cx: &Ctx, f: &str, p: &str) -> Vec<String> {
    match p.get(1..2).and_then(|c| c.chars().next()) {
        None => vec![f.to_string()],
        Some(c) => match Mode::from_code(c) {
            Some(m) if p.len() == 2 && cx.model.modes.contains(&m) => vec![f.to_string()],
            _ => Vec::new(),
        },
    }
}

/// Only the radio's own band keys (`BSnn`).
pub(crate) fn band_check(cx: &Ctx, f: &str, p: &str) -> Vec<String> {
    match p.parse::<u8>() {
        Ok(b) if p.len() == 2 && cx.model.bands.iter().any(|x| x.code == b) => vec![f.to_string()],
        _ => Vec::new(),
    }
}

/// `MW`: 9-digit frequency, and P9 is fixed "00" on this radio (the
/// FTDX3000 puts the tone number there).
pub(crate) fn mw_down(f: &str) -> Vec<String> {
    info_digits(f, 9)
        .into_iter()
        .map(|n| {
            let body = &n[..n.len() - 1];
            if body.len() >= 5 {
                let at = body.len() - 3;
                format!("{}00{};", &body[..at], &body[at + 2..])
            } else {
                n.clone()
            }
        })
        .collect()
}

/// RigHelm `CN0nn` (tone number) <-> `CN00nnn` (P2 0 = CTCSS).
pub(crate) fn cn_down(p: &str) -> Vec<String> {
    match p.len() {
        1 => vec![format!("CN{p}0;")],
        3 => match p[1..].parse::<u16>() {
            Ok(n) => vec![format!("CN{}0{n:03};", &p[..1])],
            Err(_) => Vec::new(),
        },
        _ => Vec::new(),
    }
}

pub(crate) fn cn_up(p: &str) -> Vec<String> {
    match (p.get(..2), p.len()) {
        // Only the CTCSS tone; RigHelm has no DCS code.
        (Some(pp), 2) if &pp[1..] == "0" => vec![format!("CN{};", &pp[..1])],
        (Some(pp), 5) if &pp[1..] == "0" => match p[2..].parse::<u16>() {
            Ok(n) if n < 50 => vec![format!("CN{}{n:02};", &pp[..1])],
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// RigHelm's `CO00` (0 off, 1 contour, 2 APF) is two switches here
/// (`CO00`, `CO02`); `CO01` (x100 Hz) is in Hz; `CO02` (APF 0-50) is `CO03`.
pub(crate) fn co_down(p: &str) -> Vec<String> {
    let p1 = p.get(..1).unwrap_or("0");
    let (Some(p2), rest) = (p.get(1..2), p.get(2..).unwrap_or("")) else { return Vec::new() };
    if rest.is_empty() {
        // A Read.
        return match p2 {
            "0" => vec![format!("CO{p1}0;"), format!("CO{p1}2;")],
            "1" => vec![format!("CO{p1}1;")],
            "2" => vec![format!("CO{p1}3;")],
            _ => Vec::new(),
        };
    }
    let Ok(v) = rest.parse::<u32>() else { return Vec::new() };
    match p2 {
        "0" => match v {
            0 => vec![format!("CO{p1}00000;"), format!("CO{p1}20000;")],
            1 => vec![format!("CO{p1}20000;"), format!("CO{p1}00001;")],
            2 => vec![format!("CO{p1}00000;"), format!("CO{p1}20001;")],
            _ => Vec::new(),
        },
        "1" => vec![format!("CO{p1}1{:04};", (v * 100).clamp(10, 3200))],
        "2" => vec![format!("CO{p1}3{:04};", v.min(50))],
        _ => Vec::new(),
    }
}

pub(crate) fn co_up(cx: &mut Ctx, p: &str) -> Vec<String> {
    let p1 = p.get(..1).unwrap_or("0");
    let (Some(p2), rest) = (p.get(1..2), p.get(2..).unwrap_or("")) else { return Vec::new() };
    if rest.is_empty() {
        return match p2 {
            "0" | "2" => vec![format!("CO{p1}0;")],
            "1" => vec![format!("CO{p1}1;")],
            "3" => vec![format!("CO{p1}2;")],
            _ => Vec::new(),
        };
    }
    let Ok(v) = rest.parse::<u32>() else { return Vec::new() };
    let switch = |cx: &Ctx| {
        let on = |k| cx.recall(k) == Some("1");
        if on("contour") {
            1
        } else if on("apf") {
            2
        } else {
            0
        }
    };
    match p2 {
        "0" | "2" => {
            cx.remember(if p2 == "0" { "contour" } else { "apf" }, if v == 1 { "1" } else { "0" });
            vec![format!("CO{p1}0{:02};", switch(cx))]
        }
        "1" => vec![format!("CO{p1}1{:02};", ((v + 50) / 100).clamp(1, 40))],
        "3" => vec![format!("CO{p1}2{:02};", v.min(50))],
        _ => Vec::new(),
    }
}

/// `DA`: P1 is fixed 00, P2 the LED level 1-2, P3 the TFT level.
fn da_down(f: &str, p: &str) -> Vec<String> {
    if p.len() != 6 {
        return vec![f.to_string()];
    }
    let Ok(led) = p[2..4].parse::<u8>() else { return Vec::new() };
    vec![format!("DA00{:02}{};", led.clamp(1, 2), &p[4..])]
}

/// `LK`/`FS` with only VFO-A's switch: RigHelm's 4-7 (both VFOs) as 0/1,
/// VFO-B alone not at all.
pub(crate) fn vfo_a_switch_down(code: &str, f: &str, p: &str) -> Vec<String> {
    match p.parse::<u8>() {
        _ if p.is_empty() => vec![f.to_string()],
        Ok(v @ 0..=1) => vec![format!("{code}{v};")],
        Ok(v @ 4..=7) => vec![format!("{code}{};", v & 1)],
        _ => Vec::new(),
    }
}

pub(crate) fn vfo_a_switch_up(code: &str, f: &str, p: &str) -> Vec<String> {
    match p.parse::<u8>() {
        _ if p.is_empty() => vec![f.to_string()],
        Ok(v @ 0..=1) => vec![format!("{code}{};", 4 | v)],
        _ => Vec::new(),
    }
}

/// A value RigHelm's scale and the radio's don't share exactly: RigHelm's
/// `v` as the radio's value (`scale(v)`), unless `v` is the one last seen
/// with a radio value, which is then used again. Either way the pair is
/// remembered, so a value read back is the one that was sent, in both
/// directions (see [`from_radio`]).
pub(crate) fn to_radio(cx: &mut Ctx, what: &str, v: u32, scale: impl Fn(u32) -> u32) -> u32 {
    let key = format!("pair {what}");
    let known = cx.recall(&key).and_then(|m| m.split_once('=')).and_then(|(n, w)| (w == v.to_string()).then(|| n.parse().ok()).flatten());
    let n = known.unwrap_or_else(|| scale(v));
    cx.remember(&key, format!("{n}={v}"));
    n
}

/// The radio's `n` as RigHelm's value (`scale(n)`, or the RigHelm value it
/// was last paired with).
pub(crate) fn from_radio(cx: &mut Ctx, what: &str, n: u32, scale: impl Fn(u32) -> u32) -> u32 {
    let key = format!("pair {what}");
    let known = cx.recall(&key).and_then(|m| m.split_once('=')).and_then(|(m, v)| (m == n.to_string()).then(|| v.parse().ok()).flatten());
    let v = known.unwrap_or_else(|| scale(n));
    cx.remember(&key, format!("{n}={v}"));
    v
}

/// A level (`SQ0nnn`, `MGnnn`: `at` characters before the number) on
/// RigHelm's 0-`full` scale as the radio's 0-`native`.
pub(crate) fn level_down(cx: &mut Ctx, f: &str, at: usize, full: u32, native: u32) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if p.len() <= at {
        return vec![f.to_string()];
    }
    let Ok(v) = p[at..].parse::<u32>() else { return Vec::new() };
    let n = to_radio(cx, &code, v.min(full), |v| (v * native + full / 2) / full);
    vec![format!("{code}{}{n:03};", &p[..at])]
}

pub(crate) fn level_up(cx: &mut Ctx, f: &str, at: usize, full: u32, native: u32) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if p.len() <= at {
        return vec![f.to_string()];
    }
    let Ok(n) = p[at..].parse::<u32>() else { return Vec::new() };
    let v = from_radio(cx, &code, n.min(native), |n| (n * full + native / 2) / native);
    vec![format!("{code}{}{v:03};", &p[..at])]
}

/// Manual notch frequency: up to 320 (x10 Hz) here, 400 on the FTDX3000.
pub(crate) fn bp_down(f: &str, p: &str) -> Vec<String> {
    match (p.get(1..2), p.get(2..).and_then(|v| v.parse::<u16>().ok())) {
        (Some("1"), Some(v)) if p.len() == 5 => vec![format!("BP{}1{:03};", &p[..1], v.clamp(1, 320))],
        _ => vec![f.to_string()],
    }
}

// ------------------------------------------------------------------ menu

const fn m(num: u16, group: &'static str, name: &'static str, width: u8, kind: Kind) -> MenuItem {
    MenuItem { num, addr: num as u32, group, name, width, kind }
}

const LCUT: Kind = Kind::Steps { max_code: 19, off_at_zero: true, first: 100, step: 50, unit: "Hz" };
const HCUT: Kind = Kind::Steps { max_code: 67, off_at_zero: true, first: 700, step: 50, unit: "Hz" };
const SLOPE: Kind = Kind::Choice(&["6 dB/oct", "18 dB/oct"]);
const ENABLE: Kind = Kind::Choice(&["DISABLE", "ENABLE"]);
const OFF_ON: Kind = Kind::Choice(&["OFF", "ON"]);
const LEVEL: Kind = num(0, 100, 1, "");
const MIC_REAR: Kind = Kind::Choice(&["MIC", "REAR"]);
const PTT: Kind = Kind::Choice(&["DAKY", "RTS", "DTR"]);
const DATA_USB: Kind = Kind::Choice(&["DATA", "USB"]);
// The reference book numbers these two from 1 (where AM/SSB PORT SELECT
// start at 0).
const DATA_USB_1: Kind = Kind::Coded(&[(1, "DATA"), (2, "USB")]);
const NORREV: Kind = Kind::Choice(&["NORMAL", "REVERSE"]);
const DELAY: Kind = num(30, 3000, 10, "ms");
const AGC_DELAY: Kind = num(20, 4000, 20, "ms");
const RATE: Kind = Kind::Choice(&["4800 bps", "9600 bps", "19200 bps", "38400 bps"]);
const TOT: Kind = Kind::Choice(&["10 ms", "100 ms", "1000 ms", "3000 ms"]);
const CW_MEM: Kind = Kind::Choice(&["TEXT", "MESSAGE"]);
const COLOR: Kind = Kind::Choice(&["BLUE", "GRAY", "GREEN", "ORANGE", "PURPLE", "RED", "SKY BLUE"]);
const EQ1_FREQ: Kind = Kind::Steps { max_code: 7, off_at_zero: true, first: 100, step: 100, unit: "Hz" };
const EQ2_FREQ: Kind = Kind::Steps { max_code: 9, off_at_zero: true, first: 700, step: 100, unit: "Hz" };
const EQ3_FREQ: Kind = Kind::Steps { max_code: 18, off_at_zero: true, first: 1500, step: 100, unit: "Hz" };
const EQ_LEVEL: Kind = signed(-20, 10, 1, "dB");
const EQ_BW: Kind = num(1, 10, 1, "");
const WATTS: Kind = num(5, 100, 1, "W");
const WATTS_V: Kind = num(5, 50, 1, "W");

const AGC: &str = "AGC";
const DISP: &str = "DISPLAY";
const DVS: &str = "DVS";
const KEYER: &str = "KEYER";
const GEN: &str = "GENERAL";
const AM: &str = "MODE-AM";
const CW: &str = "MODE-CW";
const DATA: &str = "MODE-DATA";
const FM: &str = "MODE-FM";
const C4FM: &str = "C4FM";
const RTTY: &str = "MODE-RTTY";
const SSB: &str = "MODE-SSB";
const DSP: &str = "RX DSP";
const SCOPE: &str = "SCOPE";
const TXA: &str = "TX AUDIO";
const TXG: &str = "TX GENERAL";
const WIRES: &str = "WIRES-X";

/// The FT-991A's 153 menu items (CAT reference book pages 7-9).
pub static MENU: &[MenuItem] = &[
    m(1, AGC, "AGC FAST DELAY", 4, AGC_DELAY),
    m(2, AGC, "AGC MID DELAY", 4, AGC_DELAY),
    m(3, AGC, "AGC SLOW DELAY", 4, AGC_DELAY),
    m(4, DISP, "HOME FUNCTION", 1, Kind::Choice(&["SCOPE", "FUNCTION"])),
    m(5, DISP, "MY CALL INDICATION", 1, num(0, 5, 1, "s")),
    m(6, DISP, "DISPLAY COLOR", 1, COLOR),
    m(7, DISP, "DIMMER LED", 1, Kind::Choice(&["1", "2"])),
    m(8, DISP, "DIMMER TFT", 2, num(0, 15, 1, "")),
    m(9, DISP, "BAR MTR PEAK HOLD", 1, Kind::Choice(&["OFF", "0.5 s", "1.0 s", "2.0 s"])),
    m(10, DVS, "DVS RX OUT LEVEL", 3, LEVEL),
    m(11, DVS, "DVS TX OUT LEVEL", 3, LEVEL),
    m(12, KEYER, "KEYER TYPE", 1, Kind::Choice(&["OFF", "BUG", "ELEKEY-A", "ELEKEY-B", "ELEKEY-Y", "ACS"])),
    m(13, KEYER, "KEYER DOT/DASH", 1, NORREV),
    m(14, KEYER, "CW WEIGHT", 2, Kind::Num { min: 25, max: 45, step: 1, unit: ":1", signed: false, div: 10 }),
    m(15, KEYER, "BEACON INTERVAL (0=OFF)", 3, num(0, 690, 1, "s")),
    m(16, KEYER, "NUMBER STYLE", 1, Kind::Choice(&["1290", "AUNO", "AUNT", "A2NO", "A2NT", "12NO", "12NT"])),
    m(17, KEYER, "CONTEST NUMBER", 4, num(0, 9999, 1, "")),
    m(18, KEYER, "CW MEMORY 1", 1, CW_MEM),
    m(19, KEYER, "CW MEMORY 2", 1, CW_MEM),
    m(20, KEYER, "CW MEMORY 3", 1, CW_MEM),
    m(21, KEYER, "CW MEMORY 4", 1, CW_MEM),
    m(22, KEYER, "CW MEMORY 5", 1, CW_MEM),
    m(23, GEN, "NB WIDTH", 1, Kind::Choice(&["1 ms", "3 ms", "10 ms"])),
    m(24, GEN, "NB REJECTION", 1, Kind::Choice(&["10 dB", "30 dB", "50 dB"])),
    m(25, GEN, "NB LEVEL", 2, num(0, 10, 1, "")),
    m(26, GEN, "BEEP LEVEL", 3, LEVEL),
    // "+hhmm" / "-hhmm", UTC -12:00 to +14:00.
    m(27, GEN, "TIME ZONE", 5, Kind::Text { max: 5 }),
    // The reference book has no code 2 here.
    m(28, GEN, "GPS/232C SELECT", 1, Kind::Coded(&[(0, "GPS1"), (1, "GPS2"), (3, "RS232C")])),
    m(29, GEN, "232C RATE", 1, RATE),
    m(30, GEN, "232C TOT", 1, TOT),
    m(31, GEN, "CAT RATE", 1, RATE),
    m(32, GEN, "CAT TOT", 1, TOT),
    m(33, GEN, "CAT RTS", 1, ENABLE),
    m(34, GEN, "MEM GROUP", 1, ENABLE),
    m(35, GEN, "QUICK SPLIT FREQ", 3, signed(-20, 20, 1, "kHz")),
    m(36, GEN, "TX TOT (0=OFF)", 2, num(0, 30, 1, "min")),
    m(37, GEN, "MIC SCAN", 1, ENABLE),
    m(38, GEN, "MIC SCAN RESUME", 1, Kind::Choice(&["PAUSE", "TIME"])),
    m(39, GEN, "REF FREQ ADJ", 3, signed(-25, 25, 1, "")),
    m(40, GEN, "CLAR MODE SELECT", 1, Kind::Choice(&["RX", "TX", "TRX"])),
    m(41, AM, "AM LCUT FREQ", 2, LCUT),
    m(42, AM, "AM LCUT SLOPE", 1, SLOPE),
    m(43, AM, "AM HCUT FREQ", 2, HCUT),
    m(44, AM, "AM HCUT SLOPE", 1, SLOPE),
    m(45, AM, "AM MIC SELECT", 1, MIC_REAR),
    m(46, AM, "AM OUT LEVEL", 3, LEVEL),
    m(47, AM, "AM PTT SELECT", 1, PTT),
    m(48, AM, "AM PORT SELECT", 1, DATA_USB),
    m(49, AM, "AM DATA GAIN", 3, LEVEL),
    m(50, CW, "CW LCUT FREQ", 2, LCUT),
    m(51, CW, "CW LCUT SLOPE", 1, SLOPE),
    m(52, CW, "CW HCUT FREQ", 2, HCUT),
    m(53, CW, "CW HCUT SLOPE", 1, SLOPE),
    m(54, CW, "CW OUT LEVEL", 3, LEVEL),
    m(55, CW, "CW AUTO MODE", 1, Kind::Choice(&["OFF", "50 MHz", "ON"])),
    m(56, CW, "CW BK-IN TYPE", 1, Kind::Choice(&["SEMI BREAK-IN", "FULL BREAK-IN"])),
    m(57, CW, "CW BK-IN DELAY", 4, DELAY),
    m(58, CW, "CW WAVE SHAPE", 1, Kind::Choice(&["1 ms", "2 ms", "4 ms", "6 ms"])),
    m(59, CW, "CW FREQ DISPLAY", 1, Kind::Choice(&["DIRECT FREQ", "PITCH OFFSET"])),
    m(60, CW, "PC KEYING", 1, Kind::Choice(&["OFF", "DAKY", "RTS", "DTR"])),
    m(61, CW, "QSK DELAY TIME", 1, Kind::Choice(&["15 ms", "20 ms", "25 ms", "30 ms"])),
    m(62, DATA, "DATA MODE", 1, Kind::Choice(&["PSK", "OTHER"])),
    m(63, DATA, "PSK TONE", 1, Kind::Choice(&["1000 Hz", "1500 Hz", "2000 Hz"])),
    m(64, DATA, "OTHER DISP (SSB)", 5, signed(-3000, 3000, 10, "Hz")),
    m(65, DATA, "OTHER SHIFT (SSB)", 5, signed(-3000, 3000, 10, "Hz")),
    m(66, DATA, "DATA LCUT FREQ", 2, LCUT),
    m(67, DATA, "DATA LCUT SLOPE", 1, SLOPE),
    // The reference book swaps the digit counts of these two.
    m(68, DATA, "DATA HCUT FREQ", 2, HCUT),
    m(69, DATA, "DATA HCUT SLOPE", 1, SLOPE),
    m(70, DATA, "DATA IN SELECT", 1, MIC_REAR),
    m(71, DATA, "DATA PTT SELECT", 1, PTT),
    m(72, DATA, "DATA PORT SELECT", 1, DATA_USB_1),
    m(73, DATA, "DATA OUT LEVEL", 3, LEVEL),
    m(74, FM, "FM MIC SELECT", 1, MIC_REAR),
    m(75, FM, "FM OUT LEVEL", 3, LEVEL),
    m(76, FM, "FM PKT PTT SELECT", 1, PTT),
    m(77, FM, "FM PKT PORT SELECT", 1, DATA_USB_1),
    m(78, FM, "FM PKT TX GAIN", 3, LEVEL),
    m(79, FM, "FM PKT MODE", 1, Kind::Choice(&["1200", "9600"])),
    m(80, FM, "RPT SHIFT 28MHz", 4, num(0, 1000, 10, "kHz")),
    m(81, FM, "RPT SHIFT 50MHz", 4, num(0, 4000, 10, "kHz")),
    m(82, FM, "RPT SHIFT 144MHz", 4, num(0, 4000, 10, "kHz")),
    m(83, FM, "RPT SHIFT 430MHz", 5, num(0, 10000, 10, "kHz")),
    m(84, FM, "ARS 144MHz", 1, OFF_ON),
    m(85, FM, "ARS 430MHz", 1, OFF_ON),
    m(86, FM, "DCS POLARITY", 1, Kind::Choice(&["Tn-Rn", "Tn-Riv", "Tiv-Rn", "Tiv-Riv"])),
    // Read only (the radio's own System Fusion ID).
    m(87, C4FM, "RADIO ID", 5, Kind::Text { max: 5 }),
    m(88, C4FM, "GM DISPLAY", 1, Kind::Choice(&["DISTANCE", "STRENGTH"])),
    m(89, C4FM, "DISTANCE", 1, Kind::Choice(&["km", "mile"])),
    m(90, C4FM, "AMS TX MODE", 1, Kind::Choice(&["AUTO", "MANUAL", "DN", "VW", "ANALOG"])),
    m(91, C4FM, "STANDBY BEEP", 1, OFF_ON),
    m(92, RTTY, "RTTY LCUT FREQ", 2, LCUT),
    m(93, RTTY, "RTTY LCUT SLOPE", 1, SLOPE),
    m(94, RTTY, "RTTY HCUT FREQ", 2, HCUT),
    m(95, RTTY, "RTTY HCUT SLOPE", 1, SLOPE),
    m(96, RTTY, "RTTY SHIFT PORT", 1, Kind::Choice(&["SHIFT", "DTR", "RTS"])),
    m(97, RTTY, "RTTY POLARITY-RX", 1, NORREV),
    m(98, RTTY, "RTTY POLARITY-TX", 1, NORREV),
    m(99, RTTY, "RTTY OUT LEVEL", 3, LEVEL),
    // The reference book's "1: 170 Hz 1: 200 Hz ..." and "1: 1275 Hz
    // 2: 2125 Hz" are misprints of 0-based lists (as on the FT-891).
    m(100, RTTY, "RTTY SHIFT FREQ", 1, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz", "850 Hz"])),
    m(101, RTTY, "RTTY MARK FREQ", 1, Kind::Choice(&["1275 Hz", "2125 Hz"])),
    m(102, SSB, "SSB LCUT FREQ", 2, LCUT),
    m(103, SSB, "SSB LCUT SLOPE", 1, SLOPE),
    m(104, SSB, "SSB HCUT FREQ", 2, HCUT),
    m(105, SSB, "SSB HCUT SLOPE", 1, SLOPE),
    m(106, SSB, "SSB MIC SELECT", 1, MIC_REAR),
    m(107, SSB, "SSB OUT LEVEL", 3, LEVEL),
    m(108, SSB, "SSB PTT SELECT", 1, PTT),
    m(109, SSB, "SSB PORT SELECT", 1, DATA_USB),
    m(110, SSB, "SSB TX BPF", 1, Kind::Choice(&["50-3000 Hz", "100-2900 Hz", "200-2800 Hz", "300-2700 Hz", "400-2600 Hz"])),
    m(111, DSP, "APF WIDTH", 1, Kind::Choice(&["NARROW", "MEDIUM", "WIDE"])),
    m(112, DSP, "CONTOUR LEVEL", 3, signed(-40, 20, 1, "dB")),
    m(113, DSP, "CONTOUR WIDTH", 2, num(1, 11, 1, "")),
    m(114, DSP, "IF NOTCH WIDTH", 1, Kind::Choice(&["NARROW", "WIDE"])),
    m(115, SCOPE, "SCP DISPLAY MODE", 1, Kind::Choice(&["SPECTRUM", "WATER FALL"])),
    m(116, SCOPE, "SCP SPAN FREQ", 2, Kind::Coded(&[(3, "50 kHz"), (4, "100 kHz"), (5, "200 kHz"), (6, "500 kHz"), (7, "1000 kHz")])),
    m(117, SCOPE, "SPECTRUM COLOR", 1, COLOR),
    m(118, SCOPE, "WATER FALL COLOR", 1, Kind::Choice(&["BLUE", "GRAY", "GREEN", "ORANGE", "PURPLE", "RED", "SKY BLUE", "MULTI"])),
    m(119, TXA, "PRMTRC EQ1 FREQ", 2, EQ1_FREQ),
    m(120, TXA, "PRMTRC EQ1 LEVEL", 3, EQ_LEVEL),
    m(121, TXA, "PRMTRC EQ1 BWTH", 2, EQ_BW),
    m(122, TXA, "PRMTRC EQ2 FREQ", 2, EQ2_FREQ),
    m(123, TXA, "PRMTRC EQ2 LEVEL", 3, EQ_LEVEL),
    m(124, TXA, "PRMTRC EQ2 BWTH", 2, EQ_BW),
    m(125, TXA, "PRMTRC EQ3 FREQ", 2, EQ3_FREQ),
    m(126, TXA, "PRMTRC EQ3 LEVEL", 3, EQ_LEVEL),
    m(127, TXA, "PRMTRC EQ3 BWTH", 2, EQ_BW),
    m(128, TXA, "P-PRMTRC EQ1 FREQ", 2, EQ1_FREQ),
    m(129, TXA, "P-PRMTRC EQ1 LEVEL", 3, EQ_LEVEL),
    m(130, TXA, "P-PRMTRC EQ1 BWTH", 2, EQ_BW),
    m(131, TXA, "P-PRMTRC EQ2 FREQ", 2, EQ2_FREQ),
    m(132, TXA, "P-PRMTRC EQ2 LEVEL", 3, EQ_LEVEL),
    m(133, TXA, "P-PRMTRC EQ2 BWTH", 2, EQ_BW),
    m(134, TXA, "P-PRMTRC EQ3 FREQ", 2, EQ3_FREQ),
    m(135, TXA, "P-PRMTRC EQ3 LEVEL", 3, EQ_LEVEL),
    m(136, TXA, "P-PRMTRC EQ3 BWTH", 2, EQ_BW),
    m(137, TXG, "HF TX MAX POWER", 3, WATTS),
    m(138, TXG, "50M TX MAX POWER", 3, WATTS),
    m(139, TXG, "144M TX MAX POWER", 3, WATTS_V),
    m(140, TXG, "430M TX MAX POWER", 3, WATTS_V),
    m(141, TXG, "TUNER SELECT", 1, Kind::Choice(&["OFF", "INTERNAL", "EXTERNAL", "ATAS", "LAMP"])),
    m(142, TXG, "VOX SELECT", 1, Kind::Choice(&["MIC", "DATA"])),
    m(143, TXG, "VOX GAIN", 3, LEVEL),
    m(144, TXG, "VOX DELAY", 4, DELAY),
    m(145, TXG, "ANTI VOX GAIN", 3, LEVEL),
    m(146, TXG, "DATA VOX GAIN", 3, LEVEL),
    m(147, TXG, "DATA VOX DELAY", 4, DELAY),
    m(148, TXG, "ANTI DVOX GAIN", 3, LEVEL),
    m(149, TXG, "EMERGENCY FREQ TX", 1, ENABLE),
    m(150, WIRES, "PRT/WIRES FREQ", 1, Kind::Choice(&["MANUAL", "PRESET"])),
    // "00030000 ~ 47000000": 300 kHz to 470 MHz in 10 Hz units.
    m(151, WIRES, "PRESET FREQUENCY", 8, num(30_000, 47_000_000, 1, "x10 Hz")),
    m(152, WIRES, "SEARCH SETUP", 1, Kind::Choice(&["HISTORY", "ACTIVITY"])),
    m(153, WIRES, "WIRES DG-ID (0=AUTO)", 2, num(0, 99, 1, "")),
];

// ------------------------------------------------------------- the radio

/// `SH0nn` widths (CAT reference book page 16).
pub(crate) fn widths(class: WidthClass, narrow: bool) -> Vec<(u8, u32)> {
    const SSB_N: &[u32] = &[1500, 200, 400, 600, 850, 1100, 1350, 1500, 1650, 1800];
    const SSB_W: &[(u8, u32)] = &[
        (0, 2400),
        (9, 1800),
        (10, 1950),
        (11, 2100),
        (12, 2200),
        (13, 2300),
        (14, 2400),
        (15, 2500),
        (16, 2600),
        (17, 2700),
        (18, 2800),
        (19, 2900),
        (20, 3000),
        (21, 3200),
    ];
    const CW_N: &[u32] = &[500, 50, 100, 150, 200, 250, 300, 350, 400, 450, 500];
    const RTTY_N: &[u32] = &[300, 50, 100, 150, 200, 250, 300, 350, 400, 450, 500];
    const WIDE: &[(u8, u32)] = &[(10, 500), (11, 800), (12, 1200), (13, 1400), (14, 1700), (15, 2000), (16, 2400), (17, 3000)];
    let list = |t: &[u32]| t.iter().enumerate().map(|(i, &w)| (i as u8, w)).collect::<Vec<_>>();
    match (class, narrow) {
        (WidthClass::Ssb, true) => list(SSB_N),
        (WidthClass::Ssb, false) => SSB_W.to_vec(),
        (WidthClass::Cw, true) => list(CW_N),
        (WidthClass::Cw, false) => std::iter::once((0, 2400)).chain(WIDE.iter().copied()).collect(),
        (WidthClass::Rtty, true) => list(RTTY_N),
        (WidthClass::Rtty, false) => std::iter::once((0, 500)).chain(WIDE.iter().copied()).collect(),
        (WidthClass::Fixed, _) => Vec::new(),
    }
}

/// RigHelm commands the FT-991A has (everything but `AN`, `EM`, `EN`, `FR`,
/// `RF`, `RO`, `SF`, `VF`, `VS`).
const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AM", "BA", "BC", "BD", "BI", "BP", "BS", "BU", "BY", "CH", "CN", "CO", "CS", "CT", "DA", "DN",
    "ED", "EK", "EU", "EX", "FA", "FB", "FS", "FT", "GT", "ID", "IF", "IS", "KM", "KP", "KR", "KS", "KY", "LK", "LM", "MA",
    "MC", "MD", "MG", "ML", "MR", "MS", "MW", "MX", "NA", "NB", "NL", "NR", "OI", "OS", "PA", "PB", "PC", "PL", "PR", "PS",
    "QI", "QR", "QS", "RA", "RC", "RD", "RG", "RI", "RL", "RM", "RS", "RT", "RU", "SC", "SD", "SH", "SM", "SQ", "SV", "TS",
    "TX", "UL", "UP", "VD", "VG", "VM", "VX", "XT",
];

/// Band keys (`BS`): the HF bands, 50, 144 and 430 MHz. The radio's MW (12)
/// and AIR (14) keys are receive-only broadcast/airband and left out.
static BANDS: &[Band] = &[
    Band { code: 0, label: "160", lo: 1_800_000, hi: 2_000_000 },
    Band { code: 1, label: "80", lo: 3_500_000, hi: 4_000_000 },
    Band { code: 2, label: "60", lo: 5_250_000, hi: 5_450_000 },
    Band { code: 3, label: "40", lo: 7_000_000, hi: 7_300_000 },
    Band { code: 4, label: "30", lo: 10_100_000, hi: 10_150_000 },
    Band { code: 5, label: "20", lo: 14_000_000, hi: 14_350_000 },
    Band { code: 6, label: "17", lo: 18_068_000, hi: 18_168_000 },
    Band { code: 7, label: "15", lo: 21_000_000, hi: 21_450_000 },
    Band { code: 8, label: "12", lo: 24_890_000, hi: 24_990_000 },
    Band { code: 9, label: "10", lo: 28_000_000, hi: 29_700_000 },
    Band { code: 10, label: "6", lo: 50_000_000, hi: 54_000_000 },
    Band { code: 15, label: "2", lo: 144_000_000, hi: 148_000_000 },
    Band { code: 16, label: "70cm", lo: 430_000_000, hi: 450_000_000 },
    Band { code: 11, label: "GEN", lo: 30_000, hi: 470_000_000 },
];

static MODES: &[Mode] = &[
    Mode::Lsb,
    Mode::Usb,
    Mode::Cw,
    Mode::CwR,
    Mode::Am,
    Mode::AmN,
    Mode::Fm,
    Mode::FmN,
    Mode::RttyLsb,
    Mode::RttyUsb,
    Mode::DataLsb,
    Mode::DataUsb,
    Mode::DataFm,
    Mode::C4fm,
];

const _: () = assert!(COMMANDS.len() + 9 == ALL_COMMANDS.len());

pub static FT991A: RadioModel = RadioModel {
    key: "ft991a",
    name: "FT-991A",
    maker: Maker::Yaesu,
    family: Family::Yaesu,
    proto: Proto::Text(&SPEC),
    id: "0670",
    civ_addr: 0,
    hamlib: 1035,
    freq_min: 30_000,
    freq_max: 470_000_000,
    bands: BANDS,
    modes: MODES,
    menu: MENU,
    roles: Roles {
        // SSB, AM, FM MIC SELECT and DATA IN SELECT: MIC or REAR (the rear
        // input is the USB audio when that mode's PORT SELECT is USB).
        mic_sel: &[106, 45, 74, 70],
        mic_front: 0,
        mic_usb: 1,
        pc_keying: Some((60, 0, 2)),
        cat_tot: Some((32, "1")),
        cat_items: &[28, 29, 30, 31, 32, 33],
        // No CW BFO menu: CW (CW-U) and CW-R (CW-L) are separate modes.
        cw_bfo: None,
        cw_pitch_display: Some((59, "1")),
        data_other: Some((62, "1")),
        data_disp: Some(64),
        data_psk: false,
    },
    caps: Caps {
        commands: COMMANDS,
        widths,
        narrow: true,
        preamp: &[(0, "IPO"), (1, "AMP1"), (2, "AMP2")],
        att: &[(0, "OFF"), (1, "ON")],
        agc: types::AGC_SET,
        roofing: &[],
        roofing_answers: &[],
        nb: &[(0, "OFF"), (1, "ON")],
        nr_max: 15,
        nb_level_max: 10,
        antennas: 1,
        power: (5, 100),
        meter_select: &[(0, "COMP"), (1, "ALC"), (2, "PO"), (3, "SWR"), (4, "ID"), (5, "VDD")],
        voice_memories: 5,
        keyer_memories: 5,
        keyer_memory_len: 50,
        memory: Memory { first: 1, last: 117, pms: Some(100) },
        shared_clarifier: true,
    },
    bauds: &[38400, 19200, 9600, 4800],
    stop_bits: 2,
    rts_cts: true,
    // LED level 1-2, TFT 10.
    sim: &["DA000210;"],
};

/// The FT-991 (before the A): the same CAT, another `ID;` answer.
pub static FT991: RadioModel = RadioModel { key: "ft991", name: "FT-991", id: "0570", ..FT991A };

#[cfg(test)]
mod tests {
    use super::*;

    fn cx() -> Ctx {
        Ctx::new(&FT991A)
    }

    fn d(f: &str) -> Vec<String> {
        down(&mut cx(), f)
    }

    fn u(f: &str) -> Vec<String> {
        up(&mut cx(), f)
    }

    #[test]
    fn frequencies_have_nine_digits() {
        assert_eq!(d("FA14250000;"), vec!["FA014250000;"]);
        assert_eq!(d("FB144174000;"), vec!["FB144174000;"]);
        assert_eq!(d("FA;"), vec!["FA;"]);
        assert_eq!(u("FA014250000;"), vec!["FA14250000;"]);
        assert_eq!(u("FA430100000;"), vec!["FA430100000;"]);
        // The manual's IF answer: P1 3, P2 9, P3 5, then 7 more.
        assert_eq!(u("IF001014074000+001010200000;"), vec!["IF00114074000+001010200000;"]);
        assert_eq!(d("IF00114074000+001010200000;"), vec!["IF001014074000+001010200000;"]);
        assert_eq!(u("OI001145500000-060000E10002;"), vec!["OI001145500000-060000E10002;"]);
        assert_eq!(d("MR005;"), vec!["MR005;"]);
        assert_eq!(u("MR005007074000+000000C10000;"), vec!["MR00507074000+000000C10000;"]);
        // MW: P9 is fixed 00 here.
        assert_eq!(d("MW00507074000+000000C01230;"), vec!["MW005007074000+000000C01000;"]);
        assert_eq!(u("MW005007074000+000000C01000;"), vec!["MW00507074000+000000C01000;"]);
    }

    #[test]
    fn menu_numbers_are_the_radios() {
        assert_eq!(MENU.len(), 153);
        assert_eq!(d("EX031;"), vec!["EX031;"]);
        assert_eq!(d("EX0313;"), vec!["EX0313;"]);
        assert_eq!(u("EX0313;"), vec!["EX0313;"]);
        assert_eq!(u("EX064+0100;"), vec!["EX064+0100;"]);
        assert_eq!(d("EX154;"), Vec::<String>::new());
        assert_eq!(FT991A.menu_item(32).unwrap().name, "CAT TOT");
        assert_eq!(FT991A.menu_item(60).unwrap().name, "PC KEYING");
        assert_eq!(FT991A.menu_item(106).unwrap().name, "SSB MIC SELECT");
        assert_eq!(FT991A.menu_item(153).unwrap().name, "WIRES DG-ID (0=AUTO)");
    }

    #[test]
    fn modes_and_bands() {
        assert_eq!(d("MD0E;"), vec!["MD0E;"]);
        assert_eq!(u("MD0E;"), vec!["MD0E;"]);
        assert_eq!(d("MD0I;"), Vec::<String>::new()); // no PSK mode
        assert_eq!(d("BS15;"), vec!["BS15;"]);
        assert_eq!(d("BS16;"), vec!["BS16;"]);
        assert_eq!(d("BS12;"), Vec::<String>::new());
        assert_eq!(FT991A.band_of(145_000_000).map(|b| b.label), Some("2"));
    }

    #[test]
    fn tone_number() {
        assert_eq!(d("CN0;"), vec!["CN00;"]);
        assert_eq!(d("CN012;"), vec!["CN00012;"]);
        assert_eq!(u("CN00012;"), vec!["CN012;"]);
        assert_eq!(u("CN00;"), vec!["CN0;"]);
        assert_eq!(u("CN01023;"), Vec::<String>::new()); // DCS
    }

    #[test]
    fn contour_and_apf() {
        assert_eq!(d("CO00;"), vec!["CO00;", "CO02;"]);
        assert_eq!(d("CO01;"), vec!["CO01;"]);
        assert_eq!(d("CO02;"), vec!["CO03;"]);
        assert_eq!(d("CO0001;"), vec!["CO020000;", "CO000001;"]);
        assert_eq!(d("CO0002;"), vec!["CO000000;", "CO020001;"]);
        assert_eq!(d("CO0000;"), vec!["CO000000;", "CO020000;"]);
        assert_eq!(d("CO0110;"), vec!["CO011000;"]);
        assert_eq!(d("CO0225;"), vec!["CO030025;"]);
        let mut c = cx();
        assert_eq!(up(&mut c, "CO000000;"), vec!["CO0000;"]);
        assert_eq!(up(&mut c, "CO020001;"), vec!["CO0002;"]);
        assert_eq!(up(&mut c, "CO000001;"), vec!["CO0001;"]);
        assert_eq!(up(&mut c, "CO000000;"), vec!["CO0002;"]);
        assert_eq!(up(&mut c, "CO011234;"), vec!["CO0112;"]);
        assert_eq!(up(&mut c, "CO030050;"), vec!["CO0250;"]);
        // A program's reads.
        assert_eq!(up(&mut c, "CO02;"), vec!["CO00;"]);
        assert_eq!(up(&mut c, "CO03;"), vec!["CO02;"]);
    }

    #[test]
    fn levels_and_switches() {
        assert_eq!(d("SQ0255;"), vec!["SQ0100;"]);
        assert_eq!(d("SQ0128;"), vec!["SQ0050;"]);
        assert_eq!(u("SQ0100;"), vec!["SQ0255;"]);
        assert_eq!(u("SQ0050;"), vec!["SQ0128;"]);
        assert_eq!(d("SQ0;"), vec!["SQ0;"]);
        assert_eq!(d("LK5;"), vec!["LK1;"]);
        assert_eq!(d("LK1;"), vec!["LK1;"]);
        assert_eq!(d("LK3;"), Vec::<String>::new());
        assert_eq!(u("LK1;"), vec!["LK5;"]);
        assert_eq!(u("FS0;"), vec!["FS4;"]);
        assert_eq!(u("FS;"), vec!["FS;"]);
        assert_eq!(d("DA101010;"), vec!["DA000210;"]);
        assert_eq!(d("BP01400;"), vec!["BP01320;"]);
        assert_eq!(d("BP00001;"), vec!["BP00001;"]);
        assert_eq!(d("ID;"), vec!["ID;"]);
        assert_eq!(u("ID0670;"), vec!["ID0670;"]);
    }

    #[test]
    fn missing_commands() {
        for f in ["AN0;", "AN01;", "FR;", "FR4;", "VS;", "VS1;", "RF0;", "RF01;", "VF0;", "SF;", "RO;", "EN01;", "EM1;"] {
            assert!(d(f).is_empty(), "{f}");
        }
        assert!(u("ZI;").is_empty());
        assert!(u("DT0;").is_empty());
    }

    #[test]
    fn round_trip() {
        for x in [
            "FA14074000;", "FB144390000;", "FA;", "MD02;", "MD0E;", "MD0;", "IF00114074000+012010200000;", "OI001430500000-000000400000;",
            "MR00507074000+000000C10000;", "EX032;", "EX0321;", "EX064-0300;", "EX027+0900;", "EX1514614000;", "AG0128;", "RG0255;",
            "SQ0000;", "SQ0255;", "SQ0051;", "CN0;", "CN012;", "CO00;", "CO0000;", "CO0001;", "CO0002;", "CO0110;", "CO0225;", "LK4;",
            "LK5;", "FS5;", "DA000215;", "BP00001;", "BP01150;", "BS15;", "SH0;", "SH012;", "NA01;", "PA02;", "RA01;", "NB01;",
            "NL0005;", "RL015;", "PC050;", "KP40;", "KS025;", "IS0+0120;", "FT1;", "TX1;", "MX0;", "AI1;", "ID0670;", "SM0123;",
            "RM5100;", "GT04;", "MS2;", "PR01;", "KM1CQ TEST};", "AC002;", "VD0300;", "CT01;",
        ] {
            let mut c = cx();
            let native = down(&mut c, x);
            assert!(!native.is_empty(), "{x} has no translation");
            let back: Vec<String> = native.iter().flat_map(|n| up(&mut c, n)).collect();
            assert_eq!(back.last().map(String::as_str), Some(x), "{x} -> {native:?} -> {back:?}");
        }
    }

    #[test]
    fn widths() {
        assert_eq!(FT991A.width_hz(WidthClass::Ssb, false, 21), Some(3200));
        assert_eq!(FT991A.width_hz(WidthClass::Ssb, false, 22), None);
        assert_eq!(FT991A.width_hz(WidthClass::Cw, false, 17), Some(3000));
        assert_eq!(FT991A.width_hz(WidthClass::Rtty, true, 0), Some(300));
        assert_eq!(FT991A.width_hz(WidthClass::Rtty, false, 0), Some(500));
    }

    #[test]
    fn detected_by_id() {
        assert_eq!(crate::model::by_id_answer("ID0670;").map(|m| m.key), Some("ft991a"));
        assert_eq!(crate::model::by_id_answer("ID0570;").map(|m| m.key), Some("ft991"));
    }
}
