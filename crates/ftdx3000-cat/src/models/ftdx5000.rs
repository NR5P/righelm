//! Yaesu FTDX5000 (FTDX5000, FTDX5000D, FTDX5000MP): the FTDX3000's elder
//! brother, with a sub receiver.
//!
//! From the *FTDX5000 Series CAT Operation Reference Book* (1907-D). Nearly
//! every command is the FTDX3000's; P1 of the receiver commands picks the
//! main (VFO-A, `0`) or sub (VFO-B, `1`) receiver. RigHelm's commands are the
//! main receiver's (`P1 = 0`), so they pass as they are, and the sub
//! receiver's own answers, which RigHelm has no place for, are dropped.
//!
//! Where it differs:
//!
//! * Menu: 177 items of its own (`EX001`-`EX177`).
//! * `MG`, `PL`, `VG` and the monitor level `ML1` are 0-255 (RigHelm 0-100).
//! * `PC` is in watts, 5-200. `NL` 0-255. `SD`/`VD` 20-5000 ms.
//! * `PR P1;` is one value: 0 off, 1 speech processor, 2 mic EQ (RigHelm's
//!   `PR0`/`PR1` are two switches).
//! * `FR` 0-3: main RX / main mute / main + sub RX / main mute + sub RX.
//! * `SF` has two digits and other codes; `DA` has four dimmers;
//!   `ED`/`EU` have no MIC/PROC/NOTCH/CONT encoders; `CO` has no APF
//!   frequency (`CO02`).
//! * `PA`: IPO1, AMP1, AMP2, IPO2. Four antennas (plus an RX antenna).
//! * Width table (`SH`): the radio's own (page 15); PKT uses the PSK widths.
//! * No `EK`, `EM`, `EN`; modes have no AM-N (`MD0D`).

use crate::dialect::text::{Ctx, TextSpec, menu_down, menu_up, parts, read_prefix};
use crate::menu::{Kind, MenuItem, num, signed};
use crate::model::{Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::types::{self, Mode, WidthClass};

pub static SPEC: TextSpec = TextSpec { down, up, answer_prefix: read_prefix };

/// Commands whose P1 picks the main (0) or sub (1) receiver.
const MAIN_SUB: &[&str] = &[
    "AG", "AN", "BC", "BD", "BP", "BU", "CN", "CO", "CT", "GT", "IS", "MD", "NA", "NB", "NL", "NR", "OS", "PA", "RA", "RF",
    "RG", "RL", "SH", "SM", "SQ", "VF",
];

fn down(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !cx.model.has(&code) {
        return Vec::new();
    }
    match code.as_str() {
        "EX" => menu_down(cx, f, 3),
        "MG" | "PL" | "VG" => level(f, 0, 100, 255),
        "ML" if p.starts_with('1') => level(f, 1, 100, 255),
        // No APF frequency.
        "CO" if p.get(1..2) == Some("2") => Vec::new(),
        "DA" => dimmer_down(cx, f),
        "ED" | "EU" => encoder(f),
        "FR" => fr_down(f),
        "PR" => pr_down(cx, f),
        "SF" => sf_down(f),
        _ => vec![upper(f)],
    }
}

fn up(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !cx.model.has(&code) {
        return Vec::new();
    }
    if MAIN_SUB.contains(&code.as_str()) && p.starts_with('1') {
        return Vec::new();
    }
    match code.as_str() {
        "EX" => menu_up(cx, f, 3),
        "MG" | "PL" | "VG" => level(f, 0, 255, 100),
        "ML" if p.starts_with('1') => level(f, 1, 255, 100),
        "CO" if p.get(1..2) == Some("2") => Vec::new(),
        "DA" => dimmer_up(cx, f),
        "ED" | "EU" => encoder(f),
        "FR" => fr_up(f),
        "PR" => pr_up(cx, f),
        "SF" => sf_up(f),
        _ => vec![upper(f)],
    }
}

// ------------------------------------------------------------- helpers
// Shared with the FTDX1200, FT-950 and FT-2000, which have the same kinds
// of differences.

/// The frame with its command letters upper-cased.
pub(crate) fn upper(f: &str) -> String {
    crate::command::normalize(f)
}

/// A level whose parameters from `at` on are 0-`from` as 0-`to` (3 digits).
/// Reads pass as they are.
pub(crate) fn level(f: &str, at: usize, from: u32, to: u32) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if p.len() <= at {
        return vec![upper(f)];
    }
    let Ok(v) = p[at..].parse::<u32>() else { return Vec::new() };
    let v = ((v.min(from) * to + from / 2) / from).min(to);
    vec![format!("{code}{}{v:03};", &p[..at])]
}

/// `PR P1` (0 off, 1 processor, 2 mic EQ) as RigHelm's two switches
/// `PR0`/`PR1`, and back. The last one turned on wins.
pub(crate) fn pr_down(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((_, p)) = parts(f) else { return Vec::new() };
    if p.len() == 1 {
        return vec!["PR;".into()];
    }
    let (Some(which), Some(on)) = (p.get(0..1), p.get(1..2)) else { return Vec::new() };
    let on = on == "1";
    let (this, other) = match which {
        "0" => ("pr0", "pr1"),
        "1" => ("pr1", "pr0"),
        _ => return Vec::new(),
    };
    cx.remember(this, if on { "1" } else { "0" });
    if on {
        cx.remember(other, "0");
    }
    let pr0 = cx.recall("pr0") == Some("1");
    let pr1 = cx.recall("pr1") == Some("1");
    let v = if pr0 { 1 } else if pr1 { 2 } else { 0 };
    vec![format!("PR{v};")]
}

pub(crate) fn pr_up(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((_, p)) = parts(f) else { return Vec::new() };
    match p {
        "" => vec!["PR0;".into()],
        "0" | "1" | "2" => {
            let (pr0, pr1) = (p == "1", p == "2");
            cx.remember("pr0", if pr0 { "1" } else { "0" });
            cx.remember("pr1", if pr1 { "1" } else { "0" });
            vec![format!("PR0{};", pr0 as u8), format!("PR1{};", pr1 as u8)]
        }
        _ => Vec::new(),
    }
}

/// Encoders: only the main (0) and sub (1) dials are the same.
pub(crate) fn encoder(f: &str) -> Vec<String> {
    match parts(f) {
        Some((_, p)) if matches!(p.get(0..1), Some("0" | "1")) => vec![upper(f)],
        _ => Vec::new(),
    }
}

/// `LK` and `FS` with one switch for the dial (FT-950, FT-2000), where
/// RigHelm's are VFO-A and VFO-B (Sets 0-7, answers 4-7).
pub(crate) fn ab_switch_down(f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    match p {
        "" | "0" | "1" => vec![format!("{code}{p};")],
        "4" | "5" | "6" | "7" => vec![format!("{code}{};", p.parse::<u8>().unwrap() & 1)],
        _ => Vec::new(),
    }
}

pub(crate) fn ab_switch_up(f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    match p {
        "" => vec![format!("{code};")],
        "0" | "1" => vec![format!("{code}{};", 4 + p.parse::<u8>().unwrap())],
        _ => Vec::new(),
    }
}

/// `KP` in 50 Hz steps (00-15 = 300-1050 Hz) as RigHelm's 10 Hz (00-75).
pub(crate) fn pitch50_down(f: &str) -> Vec<String> {
    let Some((_, p)) = parts(f) else { return Vec::new() };
    if p.is_empty() {
        return vec!["KP;".into()];
    }
    let Ok(n) = p.parse::<u32>() else { return Vec::new() };
    vec![format!("KP{:02};", ((n + 2) / 5).min(15))]
}

pub(crate) fn pitch50_up(f: &str) -> Vec<String> {
    let Some((_, p)) = parts(f) else { return Vec::new() };
    if p.is_empty() {
        return vec!["KP;".into()];
    }
    let Ok(n) = p.parse::<u32>() else { return Vec::new() };
    vec![format!("KP{:02};", n.min(15) * 5)]
}

/// `SF`: only MHz (1), GRP (2) and MCH (3) are RigHelm's too.
pub(crate) fn sf_basic(f: &str) -> Vec<String> {
    match parts(f) {
        Some((_, p @ ("" | "1" | "2" | "3"))) => vec![format!("SF{p};")],
        _ => Vec::new(),
    }
}

/// `FR` of a radio with a sub receiver (0-3), as RigHelm's (0, 1, 4).
pub(crate) fn fr_main_sub_down(f: &str) -> Vec<String> {
    fr_down(f)
}

pub(crate) fn fr_main_sub_up(f: &str) -> Vec<String> {
    fr_up(f)
}

// ------------------------------------------------------------- the FTDX5000's own

/// `DA`: RigHelm's three dimmers (VFO, keypad, TFT) are the radio's VFD,
/// meter and OEL; the fourth (SM-5000 E.LCD) is kept as the radio had it.
fn dimmer_down(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((_, p)) = parts(f) else { return Vec::new() };
    if p.is_empty() {
        return vec!["DA;".into()];
    }
    if p.len() != 6 {
        return Vec::new();
    }
    let fourth = cx.recall("da4").unwrap_or(&p[4..6]).to_string();
    vec![format!("DA{p}{fourth};")]
}

fn dimmer_up(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((_, p)) = parts(f) else { return Vec::new() };
    if p.is_empty() {
        return vec!["DA;".into()];
    }
    if p.len() != 8 {
        return Vec::new();
    }
    cx.remember("da4", &p[6..8]);
    vec![format!("DA{};", &p[..6])]
}

/// `FR`: RigHelm 0/1 (A receives / muted) are the radio's 0/1; 4 (B
/// receives) is 3 (main muted, sub receiving). Dual receive (2) is A.
fn fr_down(f: &str) -> Vec<String> {
    match parts(f).map(|(_, p)| p) {
        Some("") => vec!["FR;".into()],
        Some(p @ ("0" | "1")) => vec![format!("FR{p};")],
        Some("4") => vec!["FR3;".into()],
        _ => Vec::new(),
    }
}

fn fr_up(f: &str) -> Vec<String> {
    match parts(f).map(|(_, p)| p) {
        Some("") => vec!["FR;".into()],
        Some("0" | "2") => vec!["FR0;".into()],
        Some("1") => vec!["FR1;".into()],
        Some("3") => vec!["FR4;".into()],
        _ => Vec::new(),
    }
}

/// `SF` (VFO-B knob): RigHelm 1 MHz / 2 GRP / 3 MCH / 4 DIAL-B / 5 CLAR are
/// the radio's 00 BAND / 02 GRP / 03 MCH / 07 S-DIAL / 08 CLAR.
const SF_CODES: &[(&str, &str)] = &[("1", "00"), ("2", "02"), ("3", "03"), ("4", "07"), ("5", "08")];

fn sf_down(f: &str) -> Vec<String> {
    match parts(f).map(|(_, p)| p) {
        Some("") => vec!["SF;".into()],
        Some(p) => SF_CODES.iter().find(|(r, _)| *r == p).map(|(_, n)| vec![format!("SF{n};")]).unwrap_or_default(),
        None => Vec::new(),
    }
}

fn sf_up(f: &str) -> Vec<String> {
    match parts(f).map(|(_, p)| p) {
        Some("") => vec!["SF;".into()],
        Some(p) => SF_CODES.iter().find(|(_, n)| *n == p).map(|(r, _)| vec![format!("SF{r};")]).unwrap_or_default(),
        None => Vec::new(),
    }
}

/// Width table (`SH`, page 15). 00 is the default for the mode and the
/// NARROW switch.
fn widths(class: WidthClass, narrow: bool) -> Vec<(u8, u32)> {
    const SSB: [u32; 26] = [
        0, 200, 400, 600, 850, 1100, 1350, 1500, 1650, 1800, 1950, 2100, 2250, 2400, 0, 2500, 2600, 2700, 2800, 2900, 3000,
        3200, 3400, 3600, 3800, 4000,
    ];
    const CW: [u32; 17] = [0, 50, 100, 150, 200, 250, 300, 350, 400, 450, 500, 800, 1200, 1400, 1700, 2000, 2400];
    let (table, range, default): (&[u32], std::ops::RangeInclusive<usize>, u32) = match (class, narrow) {
        (WidthClass::Ssb, true) => (&SSB, 1..=7, 1500),
        (WidthClass::Ssb, false) => (&SSB, 7..=25, 2400),
        (WidthClass::Cw, true) => (&CW, 1..=10, 500),
        (WidthClass::Cw, false) => (&CW, 10..=16, 2400),
        (WidthClass::Rtty, true) => (&CW, 1..=10, 300),
        (WidthClass::Rtty, false) => (&CW, 10..=16, 500),
        (WidthClass::Fixed, _) => return Vec::new(),
    };
    std::iter::once((0, default)).chain(range.filter(|&i| table[i] != 0).map(|i| (i as u8, table[i]))).collect()
}

/// Every RigHelm command but EK, EM and EN.
pub(crate) const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AM", "AN", "BA", "BC", "BD", "BI", "BP", "BS", "BU", "BY", "CH", "CN", "CO00", "CO01", "CS", "CT",
    "DA", "DN", "ED", "EU", "EX", "FA", "FB", "FR", "FS", "FT", "GT", "ID", "IF", "IS", "KM", "KP", "KR", "KS", "KY",
    "LK", "LM", "MA", "MC", "MD", "MG", "ML", "MR", "MS", "MW", "MX", "NA", "NB", "NL", "NR", "OI", "OS", "PA", "PB",
    "PC", "PL", "PR", "PS", "QI", "QR", "QS", "RA", "RC", "RD", "RF", "RG", "RI", "RL", "RM", "RO", "RS", "RT", "RU",
    "SC", "SD", "SF", "SH", "SM", "SQ", "SV", "TS", "TX", "UL", "UP", "VD", "VF", "VG", "VM", "VS", "VX", "XT",
];

/// The front panel's modes (no AM-N).
pub(crate) const MODES: &[Mode] = &[
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
    Mode::DataFm,
];

pub static FTDX5000: RadioModel = RadioModel {
    key: "ftdx5000",
    name: "FTDX5000",
    maker: Maker::Yaesu,
    family: Family::Yaesu,
    proto: Proto::Text(&SPEC),
    id: "0362",
    civ_addr: 0,
    hamlib: 1032,
    freq_min: 30_000,
    freq_max: 60_000_000,
    bands: types::BANDS,
    modes: MODES,
    menu: MENU,
    roles: Roles {
        // AM, FM, SSB MIC SEL: 0 FRONT, 1 DATA, 2 PC (the rear PC audio
        // input; the radio has no USB audio).
        mic_sel: &[52, 80, 103],
        mic_front: 0,
        mic_usb: 2,
        // PC KEYING is on the rear-panel jacks, not a USB port's RTS.
        pc_keying: None,
        cat_tot: Some((33, "1")),
        cat_items: &[32, 33, 34],
        cw_bfo: Some((62, ["0", "1", "2"])),
        cw_pitch_display: Some((66, "1")),
        // There is no DATA MODE item: PKT always uses the PSK widths (SH
        // table, and Hamlib's driver). Pointing at PKT DISP with a value it
        // never has makes RigHelm use them.
        data_other: Some((87, "OTHER")),
        data_disp: None,
    },
    caps: Caps {
        commands: COMMANDS,
        widths,
        narrow: true,
        preamp: &[(0, "IPO1"), (3, "IPO2"), (1, "AMP1"), (2, "AMP2")],
        att: &[(0, "OFF"), (1, "6"), (2, "12"), (3, "18")],
        agc: types::AGC_SET,
        roofing: &[('0', "AUTO"), ('1', "15 kHz"), ('2', "6 kHz"), ('3', "3 kHz"), ('4', "600 Hz"), ('5', "300 Hz")],
        roofing_answers: &[
            ('1', "15 kHz"),
            ('2', "6 kHz"),
            ('3', "3 kHz"),
            ('4', "AUTO 15 kHz"),
            ('5', "AUTO 6 kHz"),
            ('6', "AUTO 3 kHz"),
            ('7', "600 Hz"),
            ('8', "300 Hz"),
            ('9', "AUTO 600 Hz"),
            ('A', "AUTO 300 Hz"),
        ],
        nb: &[(0, "OFF"), (1, "NB"), (2, "WIDE")],
        nr_max: 15,
        nb_level_max: 255,
        antennas: 4,
        power: (5, 200),
        meter_select: &[(0, "COMP"), (1, "ALC"), (2, "PO"), (3, "SWR"), (4, "ID"), (5, "VDD")],
        voice_memories: 5,
        keyer_memories: 5,
        keyer_memory_len: 50,
        memory: Memory { first: 1, last: 117, pms: Some(100) },
        shared_clarifier: true,
    },
    // RS-232C only; the radio's default is 4800 bps, hardware handshake.
    bauds: &[38400, 19200, 9600, 4800],
    stop_bits: 2,
    rts_cts: true,
    sim: &[],
};

// ------------------------------------------------------------- menu

pub(crate) const fn m(num: u16, group: &'static str, name: &'static str, width: u8, kind: Kind) -> MenuItem {
    MenuItem { num, addr: num as u32, group, name, width, kind }
}

pub(crate) const LCUT: Kind = Kind::Steps { max_code: 19, off_at_zero: true, first: 100, step: 50, unit: "Hz" };
pub(crate) const HCUT: Kind = Kind::Steps { max_code: 67, off_at_zero: true, first: 700, step: 50, unit: "Hz" };
pub(crate) const SLOPE: Kind = Kind::Choice(&["6 dB/oct", "18 dB/oct"]);
pub(crate) const ENABLE: Kind = Kind::Choice(&["DISABLE", "ENABLE"]);
pub(crate) const NORREV: Kind = Kind::Choice(&["NOR", "REV"]);
pub(crate) const LEVEL: Kind = num(0, 100, 1, "");
pub(crate) const KEYER_TYPE: Kind = Kind::Choice(&["OFF", "BUG", "ELEKEY", "ACS"]);
pub(crate) const CW_MEM: Kind = Kind::Choice(&["TEXT", "MESSAGE"]);
pub(crate) const SHAPE: Kind = Kind::Choice(&["SOFT", "SHARP"]);
pub(crate) const SLOPE3: Kind = Kind::Choice(&["STEEP", "MEDIUM", "GENTLE"]);
pub(crate) const DELAY: Kind = num(30, 3000, 10, "ms");
pub(crate) const AGC_DELAY: Kind = num(20, 4000, 20, "ms");
pub(crate) const AGC_HOLD: Kind = num(0, 2000, 20, "ms");
pub(crate) const EQ1_FREQ: Kind = Kind::Steps { max_code: 7, off_at_zero: true, first: 100, step: 100, unit: "Hz" };
pub(crate) const EQ2_FREQ: Kind = Kind::Steps { max_code: 9, off_at_zero: true, first: 700, step: 100, unit: "Hz" };
pub(crate) const EQ3_FREQ: Kind = Kind::Steps { max_code: 18, off_at_zero: true, first: 1500, step: 100, unit: "Hz" };
pub(crate) const EQ_BW: Kind = num(1, 10, 1, "");
pub(crate) const CW_WEIGHT: Kind = Kind::Num { min: 25, max: 45, step: 1, unit: ":1", signed: false, div: 10 };
pub(crate) const TX_BPF: Kind =
    Kind::Choice(&["50-3000 Hz", "100-2900 Hz", "200-2800 Hz", "300-2700 Hz", "400-2600 Hz", "3000WB"]);
const MICSEL: Kind = Kind::Choice(&["FRONT", "DATA", "PC"]);
const EQ_LEVEL: Kind = signed(-10, 10, 1, "dB");
const TX_POWER: Kind = Kind::Choice(&["10 W (20 W)", "20 W (50 W)", "50 W (100 W)", "100 W (200 W)"]);
const AGC: &str = "AGC";
const DISP: &str = "DISPLAY";
const DVS: &str = "DVS";
const KEYER: &str = "KEYER";
const GEN: &str = "GENERAL";
const AM: &str = "MODE-AM";
const CW: &str = "MODE-CW";
const DAT: &str = "MODE-DAT";
const FM: &str = "MODE-FM";
const PKT: &str = "MODE-PKT";
const RTTY: &str = "MODE-RTY";
const SSB: &str = "MODE-SSB";
const RXG: &str = "RX GNRL";
const DSP: &str = "RX DSP";
const SCOPE: &str = "SCOPE";
const TUNING: &str = "TUNING";
const TXA: &str = "TX AUDIO";
const TXG: &str = "TX GNRL";

/// Tables 1-3 (pages 6-8), with the BYTE column as the widths.
pub static MENU: &[MenuItem] = &[
    m(1, AGC, "MAIN FAST DELAY", 4, AGC_DELAY),
    m(2, AGC, "MAIN FAST HOLD", 4, AGC_HOLD),
    m(3, AGC, "MAIN MID DELAY", 4, AGC_DELAY),
    m(4, AGC, "MAIN MID HOLD", 4, AGC_HOLD),
    m(5, AGC, "MAIN SLOW DELAY", 4, AGC_DELAY),
    m(6, AGC, "MAIN SLOW HOLD", 4, AGC_HOLD),
    m(7, DISP, "TFT COLOR", 1, Kind::Choice(&["COOL BLUE", "CONTRAST BLUE", "FLASH WHITE", "CONTRAST UMBER", "UMBER"])),
    m(8, DISP, "DIMMER METER", 2, num(0, 15, 1, "")),
    m(9, DISP, "DIMMER VFD", 2, num(0, 15, 1, "")),
    m(10, DISP, "DIMMER OEL", 2, num(0, 15, 1, "")),
    m(11, DISP, "DIMMER E.LCD", 2, num(0, 15, 1, "")),
    m(12, DISP, "BAR DISPLAY SELECT", 1, Kind::Choice(&["CLAR", "CW TUNE"])),
    m(13, DISP, "SUB MTR PEAK HOLD", 1, Kind::Choice(&["OFF", "0.5 s", "1.0 s", "2.0 s"])),
    m(14, DISP, "ROTATOR START UP", 1, Kind::Choice(&["0°", "90°", "180°", "270°"])),
    m(15, DISP, "ROTATOR OFFSET ADJ (minus)", 2, num(0, 30, 2, "°")),
    m(16, DISP, "QMB MARKER", 1, ENABLE),
    // One digit (0/1) per indicator, page 9.
    m(17, DISP, "LEVEL INDICATOR", 13, Kind::Text { max: 13 }),
    m(18, DISP, "INDICATOR", 1, Kind::Choice(&["VFD", "OEL"])),
    m(19, DISP, "SELECT", 1, Kind::Choice(&["PTN1", "PTN2"])),
    m(20, DVS, "RX OUT LEVEL", 3, LEVEL),
    m(21, DVS, "TX OUT LEVEL", 3, LEVEL),
    m(22, KEYER, "BEACON TIME (0=OFF)", 3, num(0, 255, 1, "s")),
    m(23, KEYER, "NUMBER STYLE", 1, Kind::Choice(&["1290", "AUNO", "AUNT", "A2NO", "A2NT", "12NO", "12NT"])),
    m(24, KEYER, "CONTEST NUMBER", 4, num(0, 9999, 1, "")),
    m(25, KEYER, "CW MEMORY 1", 1, CW_MEM),
    m(26, KEYER, "CW MEMORY 2", 1, CW_MEM),
    m(27, KEYER, "CW MEMORY 3", 1, CW_MEM),
    m(28, KEYER, "CW MEMORY 4", 1, CW_MEM),
    m(29, KEYER, "CW MEMORY 5", 1, CW_MEM),
    m(30, GEN, "ANT SELECT", 1, Kind::Choice(&["BAND", "STACK"])),
    m(31, GEN, "BEEP LEVEL", 3, LEVEL),
    m(32, GEN, "CAT RATE", 1, Kind::Choice(&["4800 bps", "9600 bps", "19200 bps", "38400 bps"])),
    m(33, GEN, "CAT TIME OUT TIMER", 1, Kind::Choice(&["10 ms", "100 ms", "1000 ms", "3000 ms"])),
    m(34, GEN, "CAT RTS", 1, ENABLE),
    m(35, GEN, "CAT DATA INDICATOR", 1, ENABLE),
    m(36, GEN, "MEM GROUP", 1, ENABLE),
    m(37, GEN, "QUICK SPLIT FREQ", 3, signed(-20, 20, 1, "kHz")),
    m(38, GEN, "TRACKING", 1, Kind::Choice(&["OFF", "BAND", "FREQ"])),
    m(39, GEN, "TX TIME OUT TIMER (0=OFF)", 2, num(0, 30, 1, "min")),
    m(40, GEN, "TRV OFFSET (14 MHz)", 2, num(30, 46, 1, "MHz")),
    m(41, GEN, "TRV OFFSET (28 MHz)", 2, num(30, 46, 1, "MHz")),
    m(42, GEN, "TRV OFFSET (50 MHz)", 2, num(30, 46, 1, "MHz")),
    m(43, GEN, "uTUNE DIAL STEP", 1, Kind::Choice(&["DIAL STEP-2", "DIAL STEP-1", "OFF"])),
    m(44, GEN, "MIC SCAN", 1, ENABLE),
    m(45, GEN, "MIC SCAN RESUME", 1, Kind::Choice(&["PAUSE", "TIME"])),
    m(46, GEN, "FREQ ADJ", 3, signed(-25, 25, 1, "")),
    m(47, AM, "LCUT FREQ", 2, LCUT),
    m(48, AM, "LCUT SLOPE", 1, SLOPE),
    m(49, AM, "HCUT FREQ", 2, HCUT),
    m(50, AM, "HCUT SLOPE", 1, SLOPE),
    m(51, AM, "MIC GAIN", 4, Kind::MicGain),
    m(52, AM, "MIC SEL", 1, MICSEL),
    m(53, CW, "LCUT FREQ", 2, LCUT),
    m(54, CW, "LCUT SLOPE", 1, SLOPE),
    m(55, CW, "HCUT FREQ", 2, HCUT),
    m(56, CW, "HCUT SLOPE", 1, SLOPE),
    m(57, CW, "F KEYER TYPE", 1, KEYER_TYPE),
    m(58, CW, "F CW KEYER", 1, NORREV),
    m(59, CW, "R KEYER TYPE", 1, KEYER_TYPE),
    m(60, CW, "R CW KEYER", 1, NORREV),
    m(61, CW, "CW AUTO MODE", 1, Kind::Choice(&["OFF", "50 MHz", "ON"])),
    m(62, CW, "CW BFO", 1, Kind::Choice(&["USB", "LSB", "AUTO"])),
    m(63, CW, "CW BK-IN", 1, Kind::Choice(&["SEMI", "FULL"])),
    m(64, CW, "CW WAVE SHAPE", 1, Kind::Choice(&["1 ms", "2 ms", "4 ms", "6 ms"])),
    m(65, CW, "CW WEIGHT", 2, CW_WEIGHT),
    m(66, CW, "CW FREQ DISPLAY", 1, Kind::Choice(&["DIRECT FREQ", "PITCH OFFSET"])),
    m(67, CW, "PC KEYING", 1, ENABLE),
    m(68, CW, "QSK", 1, Kind::Choice(&["15 ms", "20 ms", "25 ms", "30 ms"])),
    m(69, DAT, "DATA IN SELECT", 1, Kind::Choice(&["DATA", "PC"])),
    m(70, DAT, "DATA TX GAIN", 3, LEVEL),
    m(71, DAT, "DATA OUT", 1, Kind::Choice(&["VFO-A", "VFO-B"])),
    m(72, DAT, "DATA OUT LEVEL", 3, LEVEL),
    m(73, DAT, "DATA VOX DELAY", 4, DELAY),
    m(74, DAT, "DATA VOX GAIN", 3, LEVEL),
    m(75, FM, "LCUT FREQ", 2, LCUT),
    m(76, FM, "LCUT SLOPE", 1, SLOPE),
    m(77, FM, "HCUT FREQ", 2, HCUT),
    m(78, FM, "HCUT SLOPE", 1, SLOPE),
    m(79, FM, "MIC GAIN", 4, Kind::MicGain),
    m(80, FM, "MIC SEL", 1, MICSEL),
    m(81, FM, "RPT SHIFT (28 MHz)", 4, num(0, 1000, 10, "kHz")),
    m(82, FM, "RPT SHIFT (50 MHz)", 4, num(0, 4000, 10, "kHz")),
    m(83, PKT, "LCUT FREQ", 2, LCUT),
    m(84, PKT, "LCUT SLOPE", 1, SLOPE),
    m(85, PKT, "HCUT FREQ", 2, HCUT),
    m(86, PKT, "HCUT SLOPE", 1, SLOPE),
    m(87, PKT, "PKT DISP (SSB)", 5, signed(-3000, 3000, 10, "Hz")),
    m(88, PKT, "PKT SHIFT (SSB)", 5, signed(-3000, 3000, 10, "Hz")),
    m(89, RTTY, "LCUT FREQ", 2, LCUT),
    m(90, RTTY, "LCUT SLOPE", 1, SLOPE),
    m(91, RTTY, "HCUT FREQ", 2, HCUT),
    m(92, RTTY, "HCUT SLOPE", 1, SLOPE),
    m(93, RTTY, "POLARITY-R", 1, NORREV),
    m(94, RTTY, "POLARITY-T", 1, NORREV),
    m(95, RTTY, "RTTY OUT", 1, Kind::Choice(&["VFO-A", "VFO-B"])),
    m(96, RTTY, "RTTY OUT LEVEL", 3, LEVEL),
    m(97, RTTY, "RTTY SHIFT", 1, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz", "850 Hz"])),
    m(98, RTTY, "RTTY TONE", 1, Kind::Choice(&["1275 Hz", "2125 Hz"])),
    m(99, SSB, "LCUT FREQ", 2, LCUT),
    m(100, SSB, "LCUT SLOPE", 1, SLOPE),
    m(101, SSB, "HCUT FREQ", 2, HCUT),
    m(102, SSB, "HCUT SLOPE", 1, SLOPE),
    m(103, SSB, "MIC SEL", 1, MICSEL),
    m(104, SSB, "TX BPF", 1, TX_BPF),
    m(105, SSB, "LSB RX CARRIER", 4, signed(-200, 200, 10, "Hz")),
    m(106, SSB, "USB RX CARRIER", 4, signed(-200, 200, 10, "Hz")),
    m(107, RXG, "AGC SLOPE", 1, Kind::Choice(&["NORMAL", "SLOPE"])),
    m(108, RXG, "HEADPHONE MIX", 1, Kind::Choice(&["SEPARATE", "COMBINE-1", "COMBINE-2"])),
    m(109, RXG, "IF OUT", 1, ENABLE),
    m(110, RXG, "MAIN NB LEVEL", 3, LEVEL),
    m(111, RXG, "MAIN NB WIDTH", 3, LEVEL),
    m(112, DSP, "APF WIDTH", 1, Kind::Choice(&["S.NARROW", "NARROW", "MEDIUM", "WIDE"])),
    m(113, DSP, "MAIN CONTOUR LEVEL", 3, signed(-40, 20, 1, "dB")),
    m(114, DSP, "MAIN CONTOUR WIDTH", 2, num(1, 11, 1, "")),
    m(115, DSP, "IF NOTCH WIDTH", 1, Kind::Choice(&["NARROW", "WIDE"])),
    m(116, DSP, "MAIN CW SHAPE", 1, SHAPE),
    m(117, DSP, "MAIN CW SLOPE", 1, SLOPE3),
    m(118, DSP, "MAIN PSK SHAPE", 1, SHAPE),
    m(119, DSP, "MAIN PSK SLOPE", 1, SLOPE3),
    m(120, DSP, "MAIN RTTY SHAPE", 1, SHAPE),
    m(121, DSP, "MAIN RTTY SLOPE", 1, SLOPE3),
    m(122, DSP, "MAIN SSB SHAPE", 1, SHAPE),
    m(123, DSP, "MAIN SSB SLOPE", 1, SLOPE3),
    m(124, DSP, "SUB CW SHAPE", 1, SHAPE),
    m(125, DSP, "SUB CW SLOPE", 1, SLOPE3),
    m(126, DSP, "SUB PSK SHAPE", 1, SHAPE),
    m(127, DSP, "SUB PSK SLOPE", 1, SLOPE3),
    m(128, DSP, "SUB RTTY SHAPE", 1, SHAPE),
    m(129, DSP, "SUB RTTY SLOPE", 1, SLOPE3),
    m(130, DSP, "SUB SSB SHAPE", 1, SHAPE),
    m(131, DSP, "SUB SSB SLOPE", 1, SLOPE3),
    m(132, SCOPE, "FIX 1.8 MHz", 5, num(1800, 1999, 1, "kHz")),
    m(133, SCOPE, "FIX 3.5 MHz", 5, num(3500, 3999, 1, "kHz")),
    m(134, SCOPE, "FIX 5.0 MHz", 5, num(5250, 5499, 1, "kHz")),
    m(135, SCOPE, "FIX 7.0 MHz", 5, num(7000, 7299, 1, "kHz")),
    m(136, SCOPE, "FIX 10 MHz", 5, num(10100, 10149, 1, "kHz")),
    m(137, SCOPE, "FIX 14 MHz", 5, num(14000, 14349, 1, "kHz")),
    m(138, SCOPE, "FIX 18 MHz", 5, num(18000, 18199, 1, "kHz")),
    m(139, SCOPE, "FIX 21 MHz", 5, num(21000, 21449, 1, "kHz")),
    m(140, SCOPE, "FIX 24 MHz", 5, num(24800, 24989, 1, "kHz")),
    m(141, SCOPE, "FIX 28 MHz", 5, num(28000, 29699, 1, "kHz")),
    m(142, SCOPE, "FIX 50 MHz", 5, num(50000, 53999, 1, "kHz")),
    m(143, TUNING, "DIAL STEP", 1, Kind::Choice(&["1 Hz", "5 Hz", "10 Hz"])),
    m(144, TUNING, "DIAL CW FINE", 1, ENABLE),
    m(145, TUNING, "UP/DOWN STEP", 1, Kind::Choice(&["1 MHz", "100 kHz"])),
    m(146, TUNING, "AM CH STEP", 1, Kind::Choice(&["2.5 kHz", "5 kHz", "9 kHz", "10 kHz", "12.5 kHz"])),
    m(147, TUNING, "FM CH STEP", 1, Kind::Choice(&["5 kHz", "6.25 kHz", "10 kHz", "12.5 kHz", "20 kHz", "25 kHz"])),
    m(148, TUNING, "AM DIAL LOCK", 1, ENABLE),
    m(149, TUNING, "FM DIAL LOCK", 1, ENABLE),
    m(150, TUNING, "FM DIAL STEP", 1, Kind::Choice(&["10 Hz", "100 Hz"])),
    // One digit (0/1) per band, page 9.
    m(151, TUNING, "MY BAND", 15, Kind::Text { max: 15 }),
    m(152, TXA, "PRMTRC EQ1 FREQ", 2, EQ1_FREQ),
    m(153, TXA, "PRMTRC EQ1 LEVEL", 3, EQ_LEVEL),
    m(154, TXA, "PRMTRC EQ1 BWTH", 2, EQ_BW),
    m(155, TXA, "PRMTRC EQ2 FREQ", 2, EQ2_FREQ),
    m(156, TXA, "PRMTRC EQ2 LEVEL", 3, EQ_LEVEL),
    m(157, TXA, "PRMTRC EQ2 BWTH", 2, EQ_BW),
    m(158, TXA, "PRMTRC EQ3 FREQ", 2, EQ3_FREQ),
    m(159, TXA, "PRMTRC EQ3 LEVEL", 3, EQ_LEVEL),
    m(160, TXA, "PRMTRC EQ3 BWTH", 2, EQ_BW),
    m(161, TXA, "P-PRMTRC EQ1 FREQ", 2, EQ1_FREQ),
    m(162, TXA, "P-PRMTRC EQ1 LEVEL", 3, EQ_LEVEL),
    m(163, TXA, "P-PRMTRC EQ1 BWTH", 2, EQ_BW),
    m(164, TXA, "P-PRMTRC EQ2 FREQ", 2, EQ2_FREQ),
    m(165, TXA, "P-PRMTRC EQ2 LEVEL", 3, EQ_LEVEL),
    m(166, TXA, "P-PRMTRC EQ2 BWTH", 2, EQ_BW),
    m(167, TXA, "P-PRMTRC EQ3 FREQ", 2, EQ3_FREQ),
    m(168, TXA, "P-PRMTRC EQ3 LEVEL", 3, EQ_LEVEL),
    m(169, TXA, "P-PRMTRC EQ3 BWTH", 2, EQ_BW),
    m(170, TXG, "FINAL BIAS (CLASS-A)", 4, num(1, 100, 1, "%")),
    m(171, TXG, "TX MAX POWER", 1, TX_POWER),
    m(172, TXG, "TX PWR CONTROL", 1, Kind::Choice(&["ALL MODE", "CARRIER"])),
    m(173, TXG, "EXT AMP TX-GND", 1, ENABLE),
    m(174, TXG, "EXT AMP TUNING PWR", 1, TX_POWER),
    m(175, TXG, "VOX SELECT", 1, Kind::Choice(&["MIC", "DATA"])),
    m(176, TXG, "ANTI VOX GAIN", 3, LEVEL),
    m(177, TXG, "EMERGENCY FREQ TX", 1, ENABLE),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn cx() -> Ctx {
        Ctx::new(&FTDX5000)
    }

    #[test]
    fn menu_is_complete() {
        assert_eq!(MENU.len(), 177);
        assert_eq!(FTDX5000.menu_item(103).unwrap().name, "MIC SEL");
        assert_eq!(FTDX5000.menu_item(112).unwrap().name, "APF WIDTH");
        assert_eq!(FTDX5000.menu_item(32).unwrap().name, "CAT RATE");
    }

    #[test]
    fn frames_from_the_manual() {
        let mut c = cx();
        // Identical commands pass.
        for f in ["FA14250000;", "FA;", "IS0+1000;", "IF;", "MD02;", "SH013;", "RF04;", "PA03;", "AN04;", "PC200;", "NL0255;", "ID;"] {
            assert_eq!(down(&mut c, f), vec![f], "{f}");
            assert_eq!(up(&mut c, f), vec![f], "{f}");
        }
        assert_eq!(up(&mut c, "ID0362;"), vec!["ID0362;"]);
        assert_eq!(up(&mut c, "IF00114250000+000000200000;"), vec!["IF00114250000+000000200000;"]);
        // The sub receiver's answers have no place in RigHelm.
        for f in ["AG1128;", "MD12;", "SM1100;", "SH110;", "RF19;", "NB11;", "GT13;"] {
            assert!(up(&mut c, f).is_empty(), "{f}");
        }
        // Menu.
        assert_eq!(down(&mut c, "EX103;"), vec!["EX103;"]);
        assert_eq!(up(&mut c, "EX1032;"), vec!["EX1032;"]);
        assert_eq!(up(&mut c, "EX0870+0100;"), vec!["EX0870+0100;"]);
        assert!(down(&mut c, "EX178;").is_empty());
        // 0-255 levels.
        assert_eq!(down(&mut c, "MG050;"), vec!["MG128;"]);
        assert_eq!(up(&mut c, "MG255;"), vec!["MG100;"]);
        assert_eq!(down(&mut c, "MG;"), vec!["MG;"]);
        assert_eq!(down(&mut c, "VG100;"), vec!["VG255;"]);
        assert_eq!(up(&mut c, "PL000;"), vec!["PL000;"]);
        assert_eq!(down(&mut c, "ML1050;"), vec!["ML1128;"]);
        assert_eq!(down(&mut c, "ML0001;"), vec!["ML0001;"]);
        assert_eq!(up(&mut c, "ML1255;"), vec!["ML1100;"]);
        // Speech processor / mic EQ.
        assert_eq!(down(&mut c, "PR0;"), vec!["PR;"]);
        assert_eq!(up(&mut c, "PR2;"), vec!["PR00;", "PR11;"]);
        assert_eq!(down(&mut c, "PR00;"), vec!["PR2;"]); // EQ stays on
        assert_eq!(down(&mut c, "PR01;"), vec!["PR1;"]);
        assert_eq!(down(&mut c, "PR00;"), vec!["PR0;"]);
        assert_eq!(up(&mut c, "PR;"), vec!["PR0;"]);
        // Receivers.
        assert_eq!(down(&mut c, "FR4;"), vec!["FR3;"]);
        assert_eq!(up(&mut c, "FR3;"), vec!["FR4;"]);
        assert_eq!(up(&mut c, "FR2;"), vec!["FR0;"]);
        assert!(down(&mut c, "FR5;").is_empty());
        // Sub dial.
        assert_eq!(down(&mut c, "SF5;"), vec!["SF08;"]);
        assert_eq!(up(&mut c, "SF07;"), vec!["SF4;"]);
        assert!(up(&mut c, "SF01;").is_empty());
        assert!(down(&mut c, "SF7;").is_empty());
        // Four dimmers.
        assert_eq!(down(&mut c, "DA101010;"), vec!["DA10101010;"]);
        assert_eq!(up(&mut c, "DA01020304;"), vec!["DA010203;"]);
        assert_eq!(down(&mut c, "DA050505;"), vec!["DA05050504;"]);
        // Not on this radio.
        assert!(down(&mut c, "CO0210;").is_empty());
        assert!(down(&mut c, "ED405;").is_empty());
        assert_eq!(down(&mut c, "ED005;"), vec!["ED005;"]);
        assert!(up(&mut c, "CA0001;").is_empty());
        assert!(up(&mut c, "MK1;").is_empty());
    }

    #[test]
    fn round_trip() {
        let mut c = cx();
        for f in [
            "FA07074000;", "FB14250000;", "MD03;", "AG0128;", "RG0255;", "SQ0010;", "MG000;", "MG037;", "MG100;", "PL050;",
            "VG077;", "ML1001;", "ML0000;", "PC005;", "PC150;", "SH000;", "SH025;", "NA01;", "RF05;", "PA02;", "RA03;",
            "GT04;", "NB02;", "NL0200;", "NR01;", "RL015;", "BC01;", "BP01040;", "CO0012;", "CO0001;", "IS0-0500;", "KS030;",
            "KP40;", "SD2000;", "VD0300;", "FT1;", "FR0;", "FR1;", "FR4;", "SF1;", "SF3;", "DA151515;", "EX0011000;",
            "EX017FTDX5000     ;", "EX087-1000;", "EX1773;", "AN020;", "IF00114074000+012010200000;", "ID0362;",
            "VF001281;", "KM1CQ TEST};", "TX1;", "AI1;", "MC005;", "MS2;",
        ] {
            let native = down(&mut c, f);
            let back: Vec<String> = native.iter().flat_map(|n| up(&mut c, n)).collect();
            assert!(back.iter().any(|b| b == f), "{f} -> {native:?} -> {back:?}");
        }
    }

    #[test]
    fn widths_follow_the_manual() {
        assert_eq!(FTDX5000.width_hz(WidthClass::Ssb, false, 13), Some(2400));
        assert_eq!(FTDX5000.width_hz(WidthClass::Ssb, false, 14), None);
        assert_eq!(FTDX5000.width_hz(WidthClass::Ssb, false, 7), Some(1500));
        assert_eq!(FTDX5000.width_hz(WidthClass::Ssb, true, 6), Some(1350));
        assert_eq!(FTDX5000.width_hz(WidthClass::Cw, false, 16), Some(2400));
        assert_eq!(FTDX5000.width_hz(WidthClass::Rtty, true, 0), Some(300));
        let s = crate::RadioState::for_model(&FTDX5000);
        assert_eq!(s.width_class(Mode::DataUsb), WidthClass::Rtty);
    }
}
