//! Yaesu FT-450D (`ID0244;`) and FT-450 (`ID0241;`): HF/50 MHz, 100 W.
//!
//! From the *FT-450D CAT Operation Reference Book* (1710-B). The FT-450
//! has the same CAT commands and menu as far as Hamlib knows (it drives
//! both with the same code, models 1027 and 1046); it answers `ID0241;`.
//! What the 450D added (the built-in tuner as standard, a 300 Hz CW
//! filter, more DSP) doesn't change the commands.
//!
//! An older, simpler command set than the FTDX3000's. Frequencies have 8
//! digits and the `IF`/`OI`/`MR`/`MW` layout is the FTDX3000's. Mapped onto
//! the radio's own commands:
//!
//! * `FT` (split) is relative here: 0 transmits on the VFO shown, 1 on the
//!   other one. RigHelm's absolute `FT` (0 VFO-A, 1 VFO-B) is translated
//!   with the VFO shown (`VS`).
//! * `FR` (receive on A/B) is `VS` (the VFO shown is the one received).
//! * `AB` (A to B) and `BA` (B to A) are `VV` (shown VFO to the other one),
//!   selecting the VFO first where needed.
//! * `KY1`-`KY3` play beacon texts 1-3 (`KY6`-`KY8`); 3 keyer memories
//!   (`KM1`-`KM3`) of 40 characters.
//! * `PB0n` (voice memory n) is `PBn`, 2 voice memories.
//! * `KP` is 400-800 Hz in 100 Hz steps; `MG` and `VG` are 0-255 (RigHelm
//!   0-100); `VD` is in 100 ms steps; `DA` is one 0-8 level.
//! * `CO`: contour on/off with a level (-12/-6/+6/+12 dB) and a frequency
//!   code 01-32 (250 Hz-4 kHz). No APF: `CO02` reads as centre (25) and
//!   can't be set.
//! * `SH`: three widths (narrow/normal/wide: the radio answers 00/16/31);
//!   `NA` switches narrow.
//! * `ML` is only monitor on/off (`ML1` reads as 50 and can't be set);
//!   `MS` ALC/PO/SWR; `RM` S, ALC, PO, SWR.
//!
//! Not there: `AM`, `AN`, `BC`, `EK`, `EM`, `EN`, `MA`, `MX` (PTT is `TX`),
//! `NL`, `PL`, `PR`, `RF`, `RO`, `SF`, `VF`, `XT`. (Its `ST` is the
//! channel step, not split.)

use crate::dialect::text::{Ctx, TextSpec, menu_down, menu_up, parts, read_prefix};
use crate::menu::{Kind, MenuItem, num, signed};
use crate::model::{ALL_COMMANDS, Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::models::ft991a::{band_check, level_down, level_up, mode_check, from_radio, to_radio, vfo_a_switch_down, vfo_a_switch_up};
use crate::types::{self, Band, Mode, WidthClass};

pub static SPEC: TextSpec = TextSpec { down, up, answer_prefix: read_prefix };

const MENU_DIGITS: usize = 3;

/// The VFO shown (`VS`), as last seen: 0 A, 1 B.
fn shown(cx: &Ctx) -> u8 {
    if cx.recall("vs") == Some("1") { 1 } else { 0 }
}

fn down(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !cx.model.has(&code) {
        return Vec::new();
    }
    match code.as_str() {
        "EX" => menu_down(cx, f, MENU_DIGITS),
        "MD" => mode_check(cx, f, p),
        "BS" => band_check(cx, f, p),
        "FS" | "LK" => vfo_a_switch_down(&code, f, p),
        "VS" => match p {
            "" => vec![f.to_string()],
            "0" | "1" => {
                cx.remember("vs", p);
                vec![f.to_string()]
            }
            _ => Vec::new(),
        },
        "FR" => match p {
            "" => vec!["VS;".into()],
            "0" | "1" | "4" | "5" => {
                let b = if p >= "4" { "1" } else { "0" };
                cx.remember("vs", b);
                vec![format!("VS{b};")]
            }
            _ => Vec::new(),
        },
        "FT" => match p {
            "" => vec![f.to_string()],
            "0" | "1" | "2" | "3" => {
                let tx_b = matches!(p, "1" | "3") as u8;
                vec![format!("FT{};", tx_b ^ shown(cx))]
            }
            _ => Vec::new(),
        },
        "AB" if shown(cx) == 0 => vec!["VV;".into()],
        "AB" => vec!["VS0;".into(), "VV;".into(), "VS1;".into()],
        "BA" if shown(cx) == 1 => vec!["VV;".into()],
        "BA" => vec!["VS1;".into(), "VV;".into(), "VS0;".into()],
        "KY" => match p {
            "1" | "2" | "3" => vec![format!("KY{};", p.parse::<u8>().unwrap_or(1) + 5)],
            "6" | "7" | "8" => vec![f.to_string()],
            _ => Vec::new(),
        },
        "PB" => match p.len() {
            1 => vec!["PB;".into()],
            2 if matches!(&p[1..], "0" | "1" | "2") => vec![format!("PB{};", &p[1..])],
            _ => Vec::new(),
        },
        "ML" => match p {
            // No monitor level: it is read with the switch (see `up`).
            "1" => vec!["ML0;".into()],
            _ if p.starts_with('0') => vec![f.to_string()],
            _ => Vec::new(),
        },
        "MG" | "VG" => level_down(cx, f, 0, 100, 255),
        "KP" => kp_down(cx, f, p),
        "VD" => match p.parse::<u32>() {
            _ if p.is_empty() => vec![f.to_string()],
            Ok(ms) => {
                let n = to_radio(cx, "VD", ms, |ms| ((ms + 50) / 100 * 100).clamp(100, 3000));
                vec![format!("VD{n:04};")]
            }
            Err(_) => Vec::new(),
        },
        "DA" => da_down(cx, f, p),
        "CO" => co_down(cx, p),
        // Only S, ALC, PO and SWR.
        "RM" if !matches!(p, "0" | "1" | "4" | "5" | "6") && p.len() == 1 => Vec::new(),
        _ => vec![f.to_string()],
    }
}

fn up(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if code == "VV" {
        return vec![if shown(cx) == 0 { "AB;".into() } else { "BA;".into() }];
    }
    if !cx.model.has(&code) {
        return Vec::new();
    }
    match code.as_str() {
        "EX" => menu_up(cx, f, MENU_DIGITS),
        "MD" => mode_check(cx, f, p),
        "BS" => band_check(cx, f, p),
        "FS" | "LK" => vfo_a_switch_up(&code, f, p),
        "VS" => match p {
            "" => vec![f.to_string()],
            "0" | "1" => {
                cx.remember("vs", p);
                vec![f.to_string(), format!("FR{};", if p == "1" { 4 } else { 0 })]
            }
            _ => Vec::new(),
        },
        "FT" => match p.parse::<u8>() {
            _ if p.is_empty() => vec![f.to_string()],
            Ok(rel @ 0..=1) => vec![format!("FT{};", rel ^ shown(cx))],
            _ => Vec::new(),
        },
        "KY" => match p {
            "6" | "7" | "8" => vec![format!("KY{};", p.parse::<u8>().unwrap_or(6) - 5)],
            _ => Vec::new(),
        },
        "PB" => match p {
            "" => vec!["PB0;".into()],
            "0" | "1" | "2" => vec![format!("PB0{p};")],
            _ => Vec::new(),
        },
        "ML" => match p {
            "0" => vec![f.to_string()],
            _ if p.len() == 4 && p.starts_with('0') => vec![f.to_string(), "ML1050;".into()],
            _ => Vec::new(),
        },
        "MG" | "VG" => level_up(cx, f, 0, 100, 255),
        "KP" => kp_up(cx, f, p),
        "VD" => match p.parse::<u32>() {
            Ok(n) => vec![format!("VD{:04};", from_radio(cx, "VD", n, |n| n))],
            Err(_) => vec![f.to_string()],
        },
        "DA" => da_up(cx, f, p),
        "CO" => co_up(cx, p),
        _ => vec![f.to_string()],
    }
}

/// `KP`: RigHelm 0-75 (300 + 10n Hz); the radio sets 02/04/06/08/10
/// (400-800 Hz) and reports codes 00-15 like menu 020.
fn kp_down(cx: &mut Ctx, f: &str, p: &str) -> Vec<String> {
    let Ok(n) = p.parse::<u32>() else { return if p.is_empty() { vec![f.to_string()] } else { Vec::new() } };
    let code = to_radio(cx, "KP", n, |n| ((300 + 10 * n).clamp(400, 800) - 400 + 50) / 100 * 2 + 2);
    vec![format!("KP{code:02};")]
}

fn kp_up(cx: &mut Ctx, f: &str, p: &str) -> Vec<String> {
    let Ok(code) = p.parse::<u32>() else { return if p.is_empty() { vec![f.to_string()] } else { Vec::new() } };
    let hz = |code| match code {
        0..=2 => 400,
        3..=4 => 500,
        5..=6 => 600,
        7..=8 => 700,
        _ => 800,
    };
    vec![format!("KP{:02};", from_radio(cx, "KP", code, |c| (hz(c) - 300) / 10))]
}

/// `DA`: one level 00-08 (and a fixed 00); RigHelm has three 0-15.
fn da_down(cx: &mut Ctx, f: &str, p: &str) -> Vec<String> {
    if p.is_empty() {
        return vec![f.to_string()];
    }
    let Some(v) = p.get(..2).and_then(|v| v.parse::<u32>().ok()) else { return Vec::new() };
    let n = (v.min(15) * 8 + 7) / 15;
    cx.remember("sent DA", format!("{n}={p}"));
    vec![format!("DA{n:02}00;")]
}

fn da_up(cx: &mut Ctx, f: &str, p: &str) -> Vec<String> {
    if p.is_empty() {
        return vec![f.to_string()];
    }
    let Some(n) = p.get(..2).and_then(|v| v.parse::<u32>().ok()) else { return Vec::new() };
    if let Some((sent, orig)) = cx.recall("sent DA").and_then(|m| m.split_once('=')) {
        if sent == n.to_string() {
            return vec![format!("DA{orig};")];
        }
    }
    let v = (n.min(8) * 15 + 4) / 8;
    vec![format!("DA{v:02}{v:02}{v:02};")]
}

/// Contour frequency code 01-32: about 250 Hz to 4 kHz, evenly on a log
/// scale (the reference book groups the codes as 250 Hz, 500 Hz, 1, 2, 4
/// kHz).
fn contour_hz(code: u32) -> f64 {
    250.0 * 2f64.powf(4.0 * (code.clamp(1, 32) - 1) as f64 / 31.0)
}

fn contour_code(hz: f64) -> u32 {
    (1.0 + 31.0 * (hz.max(250.0) / 250.0).log2() / 4.0).round().clamp(1.0, 32.0) as u32
}

/// RigHelm `CO00` 0 off / 1 contour (2 APF: none here) <-> `CO00` with a
/// level (`-2`, `-1`, `00`, `01`, `02`); `CO01` x100 Hz <-> code 01-32.
fn co_down(cx: &mut Ctx, p: &str) -> Vec<String> {
    let p1 = p.get(..1).unwrap_or("0");
    let (Some(p2), rest) = (p.get(1..2), p.get(2..).unwrap_or("")) else { return Vec::new() };
    if rest.is_empty() {
        return match p2 {
            "0" | "1" => vec![format!("CO{p1}{p2};")],
            // No APF: its offset is read as the contour switch's answer,
            // which brings "centre" along.
            "2" => vec![format!("CO{p1}0;")],
            _ => Vec::new(),
        };
    }
    let Ok(v) = rest.parse::<u32>() else { return Vec::new() };
    match (p2, v) {
        ("0", 0) => vec![format!("CO{p1}000;")],
        // On at the level it had (-6 dB if not known).
        ("0", 1) => vec![format!("CO{p1}0{};", cx.recall("contour level").unwrap_or("-1"))],
        ("1", _) => {
            let c = to_radio(cx, "CO01", v.clamp(1, 40), |v| contour_code(v as f64 * 100.0));
            vec![format!("CO{p1}1{c:02};")]
        }
        _ => Vec::new(),
    }
}

fn co_up(cx: &mut Ctx, p: &str) -> Vec<String> {
    let p1 = p.get(..1).unwrap_or("0");
    let (Some(p2), rest) = (p.get(1..2), p.get(2..).unwrap_or("")) else { return Vec::new() };
    if rest.is_empty() {
        return match p2 {
            "0" | "1" => vec![format!("CO{p1}{p2};")],
            _ => Vec::new(),
        };
    }
    match p2 {
        "0" => match rest {
            "00" => vec![format!("CO{p1}000;"), format!("CO{p1}225;")],
            "-2" | "-1" | "01" | "02" => {
                cx.remember("contour level", rest);
                vec![format!("CO{p1}001;"), format!("CO{p1}225;")]
            }
            _ => Vec::new(),
        },
        "1" => match rest.parse::<u32>() {
            Ok(c) => {
                let v = from_radio(cx, "CO01", c, |c| (contour_hz(c) / 100.0).round().clamp(1.0, 40.0) as u32);
                vec![format!("CO{p1}1{v:02};")]
            }
            Err(_) => Vec::new(),
        },
        _ => Vec::new(),
    }
}

// ------------------------------------------------------------------ menu

const fn m(num: u16, addr: u32, group: &'static str, name: &'static str, width: u8, kind: Kind) -> MenuItem {
    MenuItem { num, addr, group, name, width, kind }
}

const OFF_ON: Kind = Kind::Choice(&["OFF", "ON"]);
const ENABLE: Kind = Kind::Choice(&["DISABLE", "ENABLE"]);
const NORREV: Kind = Kind::Choice(&["NORMAL", "REVERSE"]);
/// 000-100 fixed level, 101-201 linked to the AF gain -50..+50.
const LINKED: Kind = num(0, 201, 1, "");
/// Table 5: what a programmable key does.
const KEY_FN: Kind = Kind::Choice(&[
    "MONI", "N/A", "P/B", "PLAY1", "PLAY2", "PLAY3", "QSPL", "SPOT", "SQLOFF", "SWR", "TXW", "VCC", "VOICE2", "VM1MONI",
    "VM1REC", "VM1TX", "VM2MONI", "VM2REC", "VM2TX", "DOWN", "FAST", "UP", "DSP", "IPO/ATT", "NB", "AGC", "MODEDN", "MODEUP",
    "DSP/SEL", "KEYER", "CLAR", "BANDDN", "BANDUP", "A=B", "A/B", "LOCK", "TUNE", "VOICE", "MW", "V/M", "HOME", "RCL", "VOX",
    "STO", "STEP", "SPLIT", "PMS", "SCAN", "MENU", "DIMMER", "MTR",
]);

const GEN: &str = "GENERAL";
const CW: &str = "CW";
const DATA: &str = "DATA";
const MIC: &str = "MIC";
const MEM: &str = "MEMORY";
const KEYS: &str = "KEYS";
const FM: &str = "FM";
const RTTY: &str = "RTTY";
const TX: &str = "TX";

/// The FT-450D's menu (CAT reference book Table 2, pages 6-7), without the
/// two unused items (027, 028), 005 BEACON TEXT (no format given) and 040 MY
/// BAND / 041 MY MODE (one on/off per band or mode, not one value). The
/// reference book gives no digit counts; these are from the value ranges.
pub static MENU: &[MenuItem] = &[
    m(1, 1, GEN, "EXT MENU", 1, OFF_ON),
    m(2, 2, GEN, "AM & FM DIAL", 1, ENABLE),
    m(3, 3, GEN, "APO TIME (0=OFF)", 2, num(0, 12, 1, "h")),
    m(4, 4, CW, "BEACON TIME (0=OFF)", 3, num(0, 255, 1, "s")),
    m(5, 6, GEN, "BEEP TONE", 1, Kind::Choice(&["440 Hz", "880 Hz", "1760 Hz"])),
    m(6, 7, GEN, "BEEP VOL", 3, LINKED),
    m(7, 8, GEN, "CAT RTS", 1, ENABLE),
    m(8, 9, GEN, "CAT TIME OUT TIME", 1, Kind::Choice(&["10 ms", "100 ms", "1000 ms", "3000 ms"])),
    m(9, 10, GEN, "CAT RATE", 1, Kind::Coded(&[(1, "4800 bps"), (2, "9600 bps"), (3, "19200 bps"), (4, "38400 bps"), (5, "DATA")])),
    m(10, 11, GEN, "CLAR DIAL / SEL", 1, Kind::Choice(&["DIAL", "SEL"])),
    m(11, 12, GEN, "CLOCK SHIFT", 1, OFF_ON),
    m(12, 13, GEN, "DISP CONTRAST", 2, num(1, 24, 1, "")),
    m(13, 14, CW, "CW AUTO MODE", 1, OFF_ON),
    m(14, 15, CW, "CW BFO", 1, Kind::Choice(&["USB", "LSB", "AUTO"])),
    m(15, 16, CW, "CW DELAY (0=FULL)", 4, num(0, 3000, 10, "ms")),
    m(16, 17, CW, "CW KEY REVERSE", 1, NORREV),
    m(17, 18, CW, "CW QSK", 1, Kind::Choice(&["15 ms", "20 ms", "25 ms", "30 ms"])),
    m(18, 19, CW, "CW PADDLE", 1, Kind::Choice(&["KEY", "MIC"])),
    m(
        19,
        20,
        CW,
        "CW PITCH",
        2,
        Kind::Coded(&[
            (0, "400 Hz"),
            (1, "400 Hz"),
            (2, "400 Hz"),
            (3, "500 Hz"),
            (4, "500 Hz"),
            (5, "600 Hz"),
            (6, "600 Hz"),
            (7, "700 Hz"),
            (8, "700 Hz"),
            (9, "800 Hz"),
            (10, "800 Hz"),
            (11, "800 Hz"),
            (12, "800 Hz"),
            (13, "800 Hz"),
            (14, "800 Hz"),
            (15, "800 Hz"),
        ]),
    ),
    m(20, 21, CW, "CW SPEED", 2, num(4, 60, 1, "wpm")),
    m(21, 22, CW, "CW SIDE TONE", 3, LINKED),
    m(22, 23, CW, "CW TRAINING", 1, Kind::Choice(&["N (numbers)", "A (letters)", "M (mixed)"])),
    m(23, 24, CW, "CW WEIGHT", 2, Kind::Num { min: 25, max: 45, step: 1, unit: ":1", signed: false, div: 10 }),
    m(24, 25, DATA, "DATA DISP", 4, signed(-300, 300, 1, "x10 Hz")),
    m(25, 26, DATA, "DATA MODE", 1, Kind::Choice(&["RTTY", "USER-L", "USER-U"])),
    m(26, 29, GEN, "DIAL STEP", 1, Kind::Choice(&["1 Hz", "10 Hz", "20 Hz", "100 Hz", "200 Hz"])),
    m(27, 30, DATA, "DIG VOX (0=OFF)", 3, num(0, 100, 1, "")),
    m(28, 31, GEN, "EMERGENCY", 1, OFF_ON),
    m(29, 32, GEN, "KEY HOLD TIME", 1, Kind::Choice(&["0.5 s", "1.0 s", "1.5 s", "2.0 s"])),
    m(30, 33, GEN, "LOCK MODE", 1, Kind::Choice(&["FREQ", "PANEL", "ALL"])),
    m(31, 34, MEM, "M-TUNE", 1, OFF_ON),
    m(32, 35, MEM, "MEMORY GROUP", 1, OFF_ON),
    m(33, 36, MEM, "MEMORY TAG", 1, Kind::Choice(&["TAG-OFF", "TAG NAME"])),
    m(34, 37, MIC, "MIC EQ", 1, num(0, 9, 1, "")),
    m(35, 38, MIC, "MIC GAIN", 1, Kind::Choice(&["LOW", "NOR", "HIGH"])),
    m(36, 39, MIC, "MIC AUTO SCAN", 1, OFF_ON),
    m(37, 42, KEYS, "MIC-DOWN PG", 2, KEY_FN),
    m(38, 43, KEYS, "MIC-FAST PG", 2, KEY_FN),
    m(39, 44, KEYS, "MIC-UP PG", 2, KEY_FN),
    m(40, 45, GEN, "METER PEAK HOLD", 1, OFF_ON),
    m(41, 46, KEYS, "PANEL'S CUSTOM SWITCH", 2, KEY_FN),
    m(42, 47, GEN, "QUICK SPLIT FREQ", 3, signed(-20, 20, 1, "kHz")),
    m(43, 48, TX, "RF POWER SET", 3, num(5, 100, 1, "W")),
    m(44, 49, FM, "REPEATER SHIFT DIRECTION", 1, Kind::Choice(&["SIMPLEX", "+SHIFT", "-SHIFT"])),
    m(45, 50, FM, "REPEATER SHIFT OFFSET", 3, Kind::Num { min: 0, max: 999, step: 1, unit: " MHz", signed: false, div: 10 }),
    m(46, 51, RTTY, "RTTY SHIFT", 1, Kind::Coded(&[(1, "170 Hz"), (2, "200 Hz"), (3, "425 Hz"), (4, "850 Hz")])),
    m(47, 52, RTTY, "RTTY TONE", 1, Kind::Coded(&[(1, "1275 Hz"), (2, "2125 Hz")])),
    m(48, 53, RTTY, "RTTY RX POLARITY", 1, NORREV),
    m(49, 54, RTTY, "RTTY TX POLARITY", 1, NORREV),
    m(50, 55, GEN, "SCAN RESUME (0=BUSY)", 2, num(0, 10, 1, "s")),
    m(
        51,
        56,
        GEN,
        "SEL DIAL MODE",
        1,
        Kind::Choice(&["CW SIDETONE LEVEL", "CW KEYER SPEED", "100 kHz STEP", "1 MHz STEP", "MIC GAIN", "RF POWER"]),
    ),
    m(52, 57, FM, "SQL TYPE", 1, Kind::Choice(&["OFF", "ENC", "ENC DEC"])),
    m(53, 58, GEN, "SQL/RF GAIN", 1, Kind::Choice(&["SQL", "RF GAIN"])),
    m(54, 59, GEN, "STBY BEEP", 1, OFF_ON),
    m(55, 60, FM, "TONE FREQ", 2, Kind::Tone),
    m(56, 61, TX, "TOT TIME (0=OFF)", 2, num(0, 20, 1, "min")),
    m(57, 62, TX, "TUNER/ATAS", 1, Kind::Choice(&["ATAS", "EXT ATU", "INT ATU", "INTRATU", "F TRANS"])),
    // "01 (100 msec) ~ 30 (3000 msec)".
    m(58, 63, TX, "VOX DELAY", 2, Kind::Num { min: 1, max: 30, step: 1, unit: " s", signed: false, div: 10 }),
    m(59, 64, TX, "VOX GAIN", 3, num(1, 255, 1, "")),
];

// ------------------------------------------------------------- the radio

/// Three widths per mode: narrow (00-10), normal (11-21), wide (22-31); the
/// radio answers 00, 16 or 31. `NA` picks narrow too, so both tables are
/// the same.
fn widths(class: WidthClass, _narrow: bool) -> Vec<(u8, u32)> {
    match class {
        WidthClass::Ssb => vec![(0, 1800), (16, 2400), (31, 3000)],
        WidthClass::Cw | WidthClass::Rtty => vec![(0, 300), (16, 500), (31, 2400)],
        WidthClass::Fixed => Vec::new(),
    }
}

/// RigHelm commands the FT-450D carries out (`FR` as `VS`, `AB`/`BA` as
/// `VV`).
const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "BA", "BD", "BI", "BP", "BS", "BU", "BY", "CH", "CN", "CO", "CS", "CT", "DA", "DN", "ED", "EU", "EX",
    "FA", "FB", "FR", "FS", "FT", "GT", "ID", "IF", "IS", "KM", "KP", "KR", "KS", "KY", "LK", "LM", "MC", "MD", "MG", "ML", "MR",
    "MS", "MW", "NA", "NB", "NR", "OI", "OS", "PA", "PB", "PC", "PS", "QI", "QR", "QS", "RA", "RC", "RD", "RG", "RI", "RL", "RM",
    "RS", "RT", "RU", "SC", "SD", "SH", "SM", "SQ", "SV", "TS", "TX", "UL", "UP", "VD", "VG", "VM", "VS", "VX",
];

const _: () = assert!(COMMANDS.len() + 16 == ALL_COMMANDS.len());

/// Band keys (`BS`); 02 (5 MHz) is invalid.
static BANDS: &[Band] = &[
    Band { code: 0, label: "160", lo: 1_800_000, hi: 2_000_000 },
    Band { code: 1, label: "80", lo: 3_500_000, hi: 4_000_000 },
    Band { code: 3, label: "40", lo: 7_000_000, hi: 7_300_000 },
    Band { code: 4, label: "30", lo: 10_100_000, hi: 10_150_000 },
    Band { code: 5, label: "20", lo: 14_000_000, hi: 14_350_000 },
    Band { code: 6, label: "17", lo: 18_068_000, hi: 18_168_000 },
    Band { code: 7, label: "15", lo: 21_000_000, hi: 21_450_000 },
    Band { code: 8, label: "12", lo: 24_890_000, hi: 24_990_000 },
    Band { code: 9, label: "10", lo: 28_000_000, hi: 29_700_000 },
    Band { code: 10, label: "6", lo: 50_000_000, hi: 54_000_000 },
    Band { code: 11, label: "GEN", lo: 30_000, hi: 56_000_000 },
];

/// "DATA" (RTTY-L/U, codes 6/9) and "USER-L/U" (DATA-L/U, 8/C).
static MODES: &[Mode] = &[
    Mode::Lsb,
    Mode::Usb,
    Mode::Cw,
    Mode::CwR,
    Mode::Am,
    Mode::Fm,
    Mode::FmN,
    Mode::RttyLsb,
    Mode::RttyUsb,
    Mode::DataLsb,
    Mode::DataUsb,
];

pub static FT450D: RadioModel = RadioModel {
    key: "ft450d",
    name: "FT-450D",
    maker: Maker::Yaesu,
    family: Family::Yaesu,
    proto: Proto::Text(&SPEC),
    id: "0244",
    civ_addr: 0,
    hamlib: 1046,
    freq_min: 30_000,
    freq_max: 56_000_000,
    bands: BANDS,
    modes: MODES,
    menu: MENU,
    roles: Roles {
        // No USB audio and no MIC SEL: the DATA jack is the only other input.
        mic_sel: &[],
        mic_front: 0,
        mic_usb: 0,
        pc_keying: None,
        // 009 CAT TIME OUT TIME 100 ms.
        cat_tot: Some((8, "1")),
        cat_items: &[7, 8, 9],
        cw_bfo: Some((14, ["0", "1", "2"])),
        cw_pitch_display: None,
        data_other: None,
        data_disp: None,
    },
    caps: Caps {
        commands: COMMANDS,
        widths,
        narrow: false,
        preamp: &[(0, "IPO"), (1, "AMP")],
        att: &[(0, "OFF"), (1, "ON")],
        agc: types::AGC_SET,
        roofing: &[],
        roofing_answers: &[],
        nb: &[(0, "OFF"), (1, "ON")],
        nr_max: 11,
        nb_level_max: 0,
        antennas: 1,
        power: (5, 100),
        meter_select: &[(1, "ALC"), (2, "PO"), (3, "SWR")],
        voice_memories: 2,
        keyer_memories: 3,
        keyer_memory_len: 40,
        memory: Memory { first: 1, last: 504, pms: Some(501) },
        shared_clarifier: true,
    },
    bauds: &[38400, 19200, 9600, 4800],
    stop_bits: 2,
    rts_cts: true,
    // Normal width; the radio's own answer for it.
    sim: &["SH016;"],
};

/// The FT-450: the same CAT, another `ID;` answer.
pub static FT450: RadioModel = RadioModel { key: "ft450", name: "FT-450", id: "0241", hamlib: 1027, ..FT450D };

#[cfg(test)]
mod tests {
    use super::*;

    fn cx() -> Ctx {
        Ctx::new(&FT450D)
    }

    fn d(f: &str) -> Vec<String> {
        down(&mut cx(), f)
    }

    fn u(f: &str) -> Vec<String> {
        up(&mut cx(), f)
    }

    #[test]
    fn same_as_ftdx3000() {
        for f in ["FA14250000;", "FA;", "IF00114074000+012010200000;", "MR005;", "MW00507074000+000000C01230;"] {
            assert_eq!(d(f), vec![f], "{f}");
        }
        assert_eq!(u("FA14250000;"), vec!["FA14250000;"]);
        assert_eq!(u("ID0244;"), vec!["ID0244;"]);
        assert_eq!(u("OI00114076000+000000200000;"), vec!["OI00114076000+000000200000;"]);
    }

    #[test]
    fn menu_numbers_are_the_radios() {
        assert_eq!(MENU.len(), 59);
        assert_eq!(d("EX008;"), vec!["EX009;"]);
        assert_eq!(d("EX0081;"), vec!["EX0091;"]);
        assert_eq!(u("EX0091;"), vec!["EX0081;"]);
        assert_eq!(u("EX064128;"), vec!["EX059128;"]);
        assert!(u("EX0271;").is_empty()); // not used
        assert!(u("EX040051;").is_empty()); // MY BAND
        let at = |addr: u32| MENU.iter().find(|m| m.addr == addr).unwrap().num;
        assert_eq!(FT450D.roles.cat_tot.unwrap().0, at(9));
        assert_eq!(FT450D.roles.cat_items, &[at(8), at(9), at(10)]);
        assert_eq!(FT450D.roles.cw_bfo.unwrap().0, at(15));
    }

    #[test]
    fn split_and_vfos() {
        let mut c = cx();
        // VFO-A shown: FT is absolute.
        assert_eq!(down(&mut c, "FT3;"), vec!["FT1;"]);
        assert_eq!(up(&mut c, "FT1;"), vec!["FT1;"]);
        assert_eq!(down(&mut c, "AB;"), vec!["VV;"]);
        assert_eq!(down(&mut c, "BA;"), vec!["VS1;", "VV;", "VS0;"]);
        // VFO-B shown (receiving on B): FT0 transmits on B.
        assert_eq!(up(&mut c, "VS1;"), vec!["VS1;", "FR4;"]);
        assert_eq!(up(&mut c, "FT0;"), vec!["FT1;"]);
        assert_eq!(up(&mut c, "FT1;"), vec!["FT0;"]);
        assert_eq!(down(&mut c, "FT2;"), vec!["FT1;"]);
        assert_eq!(down(&mut c, "BA;"), vec!["VV;"]);
        assert_eq!(down(&mut c, "AB;"), vec!["VS0;", "VV;", "VS1;"]);
        assert_eq!(up(&mut c, "VV;"), vec!["BA;"]);
        assert_eq!(down(&mut c, "FR0;"), vec!["VS0;"]);
        assert_eq!(down(&mut c, "FR;"), vec!["VS;"]);
        assert_eq!(up(&mut c, "VV;"), vec!["AB;"]);
        assert_eq!(d("SV;"), vec!["SV;"]);
    }

    #[test]
    fn keyer_voice_levels() {
        assert_eq!(d("KY1;"), vec!["KY6;"]);
        assert_eq!(d("KY3;"), vec!["KY8;"]);
        assert!(d("KY4;").is_empty());
        assert_eq!(u("KY7;"), vec!["KY2;"]);
        assert_eq!(d("PB01;"), vec!["PB1;"]);
        assert_eq!(d("PB00;"), vec!["PB0;"]);
        assert_eq!(d("PB0;"), vec!["PB;"]);
        assert_eq!(u("PB2;"), vec!["PB02;"]);
        assert!(d("PB03;").is_empty());
        assert_eq!(d("MG100;"), vec!["MG255;"]);
        assert_eq!(d("MG050;"), vec!["MG128;"]);
        assert_eq!(u("MG255;"), vec!["MG100;"]);
        assert_eq!(u("VG128;"), vec!["VG050;"]);
        assert_eq!(d("KP40;"), vec!["KP08;"]);
        assert_eq!(d("KP00;"), vec!["KP02;"]);
        assert_eq!(d("KP75;"), vec!["KP10;"]);
        assert_eq!(u("KP06;"), vec!["KP30;"]);
        assert_eq!(u("KP12;"), vec!["KP50;"]);
        assert_eq!(d("VD0250;"), vec!["VD0300;"]);
        assert_eq!(d("VD0030;"), vec!["VD0100;"]);
        assert_eq!(u("VD1500;"), vec!["VD1500;"]);
        assert_eq!(d("ML0001;"), vec!["ML0001;"]);
        assert!(d("ML1050;").is_empty());
        assert_eq!(d("ML1;"), vec!["ML0;"]);
        assert_eq!(u("ML0001;"), vec!["ML0001;", "ML1050;"]);
        assert_eq!(d("DA151515;"), vec!["DA0800;"]);
        assert_eq!(u("DA0400;"), vec!["DA080808;"]);
        assert!(d("RM7;").is_empty());
        assert_eq!(d("RM5;"), vec!["RM5;"]);
        assert_eq!(d("LK5;"), vec!["LK1;"]);
        assert_eq!(u("FS1;"), vec!["FS5;"]);
    }

    #[test]
    fn contour() {
        assert_eq!(d("CO00;"), vec!["CO00;"]);
        assert_eq!(d("CO02;"), vec!["CO00;"]);
        assert_eq!(u("CO0000;"), vec!["CO0000;", "CO0225;"]);
        assert!(d("CO0225;").is_empty());
        assert!(d("CO0002;").is_empty()); // no APF
        assert_eq!(d("CO0000;"), vec!["CO0000;"]);
        assert_eq!(d("CO0001;"), vec!["CO00-1;"]);
        let mut c = cx();
        assert_eq!(up(&mut c, "CO0002;"), vec!["CO0001;", "CO0225;"]);
        assert_eq!(down(&mut c, "CO0001;"), vec!["CO0002;"]);
        assert_eq!(up(&mut c, "CO0000;"), vec!["CO0000;", "CO0225;"]);
        assert_eq!(d("CO0110;"), vec!["CO0117;"]);
        assert_eq!(u("CO0101;"), vec!["CO0103;"]);
        assert_eq!(u("CO0132;"), vec!["CO0140;"]);
        assert_eq!(d("CO0103;"), vec!["CO0103;"]);
        assert_eq!(d("CO0140;"), vec!["CO0132;"]);
    }

    #[test]
    fn missing_commands() {
        for f in ["AM;", "MA;", "AN0;", "BC0;", "BC01;", "MX1;", "MX;", "NL0;", "PL;", "PR0;", "RF0;", "VF0;", "SF;", "RO;", "XT;", "EK;", "EN01;"] {
            assert!(d(f).is_empty(), "{f}");
        }
        assert!(u("ST1;").is_empty()); // channel step
        assert!(u("MK7;").is_empty());
        assert!(u("RP;").is_empty());
        assert!(d("MD0D;").is_empty()); // no AM-N
        assert!(d("BS02;").is_empty());
    }

    #[test]
    fn round_trip() {
        for x in [
            "FA14074000;", "FB50313000;", "FA;", "MD02;", "MD0C;", "MD06;", "IF00114074000+012010200000;", "OI00107074000-000000300000;",
            "MR50107074000+000000C10000;", "MC501;", "EX008;", "EX0081;", "EX024-030;", "EX042+12;", "AG0128;", "RG0200;", "SQ0051;",
            "CN012;", "CT01;", "CO0000;", "CO0001;", "CO0110;", "CO0101;", "CO0140;", "LK5;", "FS4;", "BP01150;", "BS10;", "SH016;",
            "SH0;", "NA01;", "IS0+0120;", "FT1;", "FT0;", "FT;", "FR4;", "FR0;", "VS1;", "VS0;", "VS;", "RT1;", "PA01;",
            "RA01;", "NB01;", "RL011;", "PC050;", "KP40;", "KP00;", "KP75;", "KS025;", "MG040;", "VG100;", "VD0250;", "VD1500;",
            "TX1;", "AI1;", "ID0244;", "SM0123;", "RM6100;", "GT04;", "MS2;", "KM1CQ TEST};", "AC002;", "SD0000;", "DA101010;",
            "DA000000;", "PB01;", "PB00;", "ML0001;", "KY1;", "KY3;",
        ] {
            let mut c = cx();
            let native = down(&mut c, x);
            assert!(!native.is_empty(), "{x} has no translation");
            let back: Vec<String> = native.iter().flat_map(|n| up(&mut c, n)).collect();
            assert!(back.iter().any(|b| b == x), "{x} -> {native:?} -> {back:?}");
        }
    }

    #[test]
    fn detected_by_id() {
        assert_eq!(crate::model::by_id_answer("ID0244;").map(|m| m.key), Some("ft450d"));
        assert_eq!(crate::model::by_id_answer("ID0241;").map(|m| m.key), Some("ft450"));
    }
}
