//! Yaesu FT-891 (`ID0650;`): HF/50 MHz, 100 W.
//!
//! From the *FT-891 CAT Operation Reference Book* (1909-C). How its commands
//! differ from the FTDX3000's:
//!
//! * `FA`/`FB` and the `IF`/`OI`/`MR`/`MW` frequencies have 9 digits.
//! * `EX` menu numbers are group + item, 4 digits (`EX0507` = CAT TOT).
//! * Split is `ST0`/`ST1` (RigHelm `FT`), the clarifier switch is `CF0x0`
//!   (RigHelm `RT`); there is one clarifier, applied to RX, TX or both by
//!   menu 05-18, so no `XT`.
//! * `IS` and `SH` have an on/off digit after P1 (`IS01+0120`, `SH0112`).
//! * Memory channels: PMS are `P1L`..`P9U` (RigHelm 100-117).
//! * `CN`/`CO` as on the FT-991A (CTCSS/DCS selector, four contour/APF
//!   parts); `RG` 0-30 and `SQ` 0-100 (RigHelm 0-255); `LK`/`FS` VFO-A only;
//!   `PA` IPO/AMP, `RA`/`NB` on/off, `NL` 0-10; no VDD meter (`RM8`).
//! * No `AN`, `DA` (four levels, set them in menu 02), `EM`, `EN`, `FR`, `RF`,
//!   `RO`, `SF`, `VF`, `VS`.
//!
//! `MD01`/`MD02` are both "SSB": which sideband is menu 11-07 SSB BFO (and
//! likewise CW 07-07, DATA 08-12, RTTY 10-11). Meters are 0-255 like the
//! FTDX3000's.

use crate::dialect::text::{Ctx, TextSpec, freq_digits, info_digits, menu_down, menu_up, parts, read_prefix};
use crate::menu::{Kind, MenuItem, num, signed};
use crate::model::{ALL_COMMANDS, Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::models::ft991a::{
    band_check, bp_down, cn_down, cn_up, co_down, co_up, level_down, level_up, mode_check, mw_down, vfo_a_switch_down, vfo_a_switch_up,
    widths,
};
use crate::types::{self, Band, Mode};

pub static SPEC: TextSpec = TextSpec { down, up, answer_prefix: read_prefix };

/// Group + item.
const MENU_DIGITS: usize = 4;

fn down(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !cx.model.has(&code) {
        return Vec::new();
    }
    match code.as_str() {
        "FA" | "FB" => freq_digits(f, 9),
        "IF" | "OI" | "MR" => info_digits(f, 9).iter().flat_map(|n| channel_down(n)).collect(),
        "MW" => mw_down(f).iter().flat_map(|n| channel_down(n)).collect(),
        "MC" => channel_down(f),
        "EX" => menu_down(cx, f, MENU_DIGITS),
        "MD" => mode_check(cx, f, p),
        "BS" => band_check(cx, f, p),
        "CN" => cn_down(p),
        "CO" => co_down(p),
        "FS" | "LK" => vfo_a_switch_down(&code, f, p),
        "SQ" => level_down(cx, f, 1, 255, 100),
        "RG" => level_down(cx, f, 1, 255, 30),
        "BP" => bp_down(f, p),
        // VFO-B transmits: split.
        "FT" => match p {
            "" => vec!["ST;".into()],
            "0" | "2" => vec!["ST0;".into()],
            "1" | "3" => vec!["ST1;".into()],
            _ => Vec::new(),
        },
        "RT" => match p {
            "" => vec!["CF0;".into()],
            "0" | "1" => vec![format!("CF0{p}0;")],
            _ => Vec::new(),
        },
        "IS" => match p.len() {
            1 => vec![f.to_string()],
            6 => {
                let on = p[2..].parse::<u32>().is_ok_and(|v| v != 0);
                vec![format!("IS{}{}{};", &p[..1], on as u8, &p[1..])]
            }
            _ => Vec::new(),
        },
        "SH" => match p.len() {
            1 => vec![f.to_string()],
            3 => vec![format!("SH{}1{};", &p[..1], &p[1..])],
            _ => Vec::new(),
        },
        // No VDD meter.
        "RM" if p == "8" => Vec::new(),
        _ => vec![f.to_string()],
    }
}

fn up(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    match code.as_str() {
        "ST" => {
            return match p {
                "" => vec!["FT;".into()],
                "0" => vec!["FT0;".into()],
                "1" | "2" => vec!["FT1;".into()],
                _ => Vec::new(),
            };
        }
        "CF" => {
            return match (p.len(), p.get(1..2)) {
                (1, _) => vec!["RT;".into()],
                (3, Some(on @ ("0" | "1"))) => vec![format!("RT{on};")],
                _ => Vec::new(),
            };
        }
        "FT" | "RT" => return Vec::new(),
        _ => {}
    }
    if !cx.model.has(&code) {
        return Vec::new();
    }
    match code.as_str() {
        "FA" | "FB" => freq_digits(f, 8),
        "IF" | "OI" | "MR" | "MW" => channel_up(f).iter().flat_map(|n| info_digits(n, 8)).collect(),
        "MC" => channel_up(f),
        "EX" => menu_up(cx, f, MENU_DIGITS),
        "MD" => mode_check(cx, f, p),
        "BS" => band_check(cx, f, p),
        "CN" => cn_up(p),
        "CO" => co_up(cx, p),
        "FS" | "LK" => vfo_a_switch_up(&code, f, p),
        "SQ" => level_up(cx, f, 1, 255, 100),
        "RG" => level_up(cx, f, 1, 255, 30),
        "IS" => match p.len() {
            1 => vec![f.to_string()],
            // Shift off: no shift.
            7 if &p[1..2] == "0" => vec![format!("IS{}+0000;", &p[..1])],
            7 => vec![format!("IS{}{};", &p[..1], &p[2..])],
            _ => Vec::new(),
        },
        "SH" => match p.len() {
            1 => vec![f.to_string()],
            4 => vec![format!("SH{}{};", &p[..1], &p[2..])],
            _ => Vec::new(),
        },
        _ => vec![f.to_string()],
    }
}

/// RigHelm's channels 100-117 are the PMS pairs `P1L`..`P9U` here: the
/// three characters after the command (`MC`, `MR`, `IF`, `MW`).
fn channel_down(f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    match p.get(..3).and_then(|c| c.parse::<u16>().ok()) {
        Some(ch @ 100..=117) => {
            let pair = (ch - 100) / 2 + 1;
            let side = if (ch - 100) % 2 == 0 { 'L' } else { 'U' };
            vec![format!("{code}P{pair}{side}{};", &p[3..])]
        }
        _ => vec![f.to_string()],
    }
}

fn channel_up(f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    let Some(ch) = p.get(..3) else { return vec![f.to_string()] };
    let b = ch.as_bytes();
    let n = match (b[0], b[1], b[2]) {
        (b'P' | b'p', d @ b'1'..=b'9', s @ (b'L' | b'U' | b'l' | b'u')) => {
            100 + (d - b'1') as u16 * 2 + matches!(s, b'U' | b'u') as u16
        }
        // The emergency channel (Alaska): not a channel RigHelm knows.
        _ if ch.eq_ignore_ascii_case("EMG") => 0,
        _ => return vec![f.to_string()],
    };
    vec![format!("{code}{n:03}{};", &p[3..])]
}

// ------------------------------------------------------------------ menu

const fn m(num: u16, addr: u32, group: &'static str, name: &'static str, width: u8, kind: Kind) -> MenuItem {
    MenuItem { num, addr, group, name, width, kind }
}

const LCUT: Kind = Kind::Steps { max_code: 19, off_at_zero: true, first: 100, step: 50, unit: "Hz" };
const HCUT: Kind = Kind::Steps { max_code: 67, off_at_zero: true, first: 700, step: 50, unit: "Hz" };
const SLOPE: Kind = Kind::Choice(&["6 dB/oct", "18 dB/oct"]);
const ENABLE: Kind = Kind::Choice(&["DISABLE", "ENABLE"]);
const LEVEL: Kind = num(0, 100, 1, "");
const MIC_REAR: Kind = Kind::Choice(&["MIC", "REAR"]);
const PTT: Kind = Kind::Choice(&["DAKY", "RTS", "DTR"]);
const NORREV: Kind = Kind::Choice(&["NOR", "REV"]);
const USB_LSB: Kind = Kind::Choice(&["USB", "LSB"]);
const USB_LSB_AUTO: Kind = Kind::Choice(&["USB", "LSB", "AUTO"]);
const DELAY: Kind = num(30, 3000, 10, "ms");
const AGC_DELAY: Kind = num(20, 4000, 20, "ms");
const DIM: Kind = num(1, 15, 1, "");
const CW_MEM: Kind = Kind::Choice(&["TEXT", "MESSAGE"]);
const STEP_2_5_10: Kind = Kind::Choice(&["2 Hz", "5 Hz", "10 Hz"]);
const STEP_10_100: Kind = Kind::Choice(&["10 Hz", "100 Hz"]);
const EQ1_FREQ: Kind = Kind::Steps { max_code: 7, off_at_zero: true, first: 100, step: 100, unit: "Hz" };
const EQ2_FREQ: Kind = Kind::Steps { max_code: 9, off_at_zero: true, first: 700, step: 100, unit: "Hz" };
const EQ3_FREQ: Kind = Kind::Steps { max_code: 18, off_at_zero: true, first: 1500, step: 100, unit: "Hz" };
const EQ_LEVEL: Kind = signed(-20, 10, 1, "dB");
const EQ_BW: Kind = num(1, 10, 1, "");
const WATTS: Kind = num(5, 100, 1, "W");
const WATTS_AM: Kind = num(5, 40, 1, "W");
const VERSION: Kind = num(0, 9999, 1, "");

const AGC: &str = "AGC";
const DISP: &str = "DISPLAY";
const DVS: &str = "DVS";
const KEYER: &str = "KEYER";
const GEN: &str = "GENERAL";
const AM: &str = "MODE AM";
const CW: &str = "MODE CW";
const DATA: &str = "MODE DAT";
const FM: &str = "MODE FM";
const RTTY: &str = "MODE RTY";
const SSB: &str = "MODE SSB";
const DSP: &str = "RX DSP";
const SCOPE: &str = "SCOPE";
const TUNING: &str = "TUNING";
const TXA: &str = "TX AUDIO";
const TXG: &str = "TX GNRL";
const VER: &str = "VERSION";

/// The FT-891's menu (CAT reference book pages 7-9), without 17-01 RESET,
/// which isn't a setting (writing it would reset the radio).
pub static MENU: &[MenuItem] = &[
    m(1, 101, AGC, "AGC FAST DELAY", 4, AGC_DELAY),
    m(2, 102, AGC, "AGC MID DELAY", 4, AGC_DELAY),
    m(3, 103, AGC, "AGC SLOW DELAY", 4, AGC_DELAY),
    m(4, 201, DISP, "LCD CONTRAST", 2, DIM),
    m(5, 202, DISP, "DIMMER BACKLIT", 2, DIM),
    m(6, 203, DISP, "DIMMER LCD", 2, DIM),
    m(7, 204, DISP, "DIMMER TX/BUSY", 2, DIM),
    m(8, 205, DISP, "PEAK HOLD", 1, Kind::Choice(&["OFF", "0.5 s", "1.0 s", "2.0 s"])),
    m(9, 206, DISP, "ZIN LED", 1, ENABLE),
    m(10, 207, DISP, "POP-UP MENU", 1, Kind::Choice(&["UPPER", "LOWER"])),
    m(11, 301, DVS, "DVS RX OUT LVL", 3, LEVEL),
    m(12, 302, DVS, "DVS TX OUT LVL", 3, LEVEL),
    m(13, 401, KEYER, "KEYER TYPE", 1, Kind::Choice(&["OFF", "BUG", "ELEKEY-A", "ELEKEY-B", "ELEKEY-Y", "ACS"])),
    m(14, 402, KEYER, "KEYER DOT/DASH", 1, NORREV),
    m(15, 403, KEYER, "CW WEIGHT", 2, Kind::Num { min: 25, max: 45, step: 1, unit: ":1", signed: false, div: 10 }),
    m(16, 404, KEYER, "BEACON INTERVAL (0=OFF)", 3, num(0, 690, 1, "s")),
    m(17, 405, KEYER, "NUMBER STYLE", 1, Kind::Choice(&["1290", "AUNO", "AUNT", "A2NO", "A2NT", "12NO", "12NT"])),
    m(18, 406, KEYER, "CONTEST NUMBER", 4, num(0, 9999, 1, "")),
    m(19, 407, KEYER, "CW MEMORY 1", 1, CW_MEM),
    m(20, 408, KEYER, "CW MEMORY 2", 1, CW_MEM),
    m(21, 409, KEYER, "CW MEMORY 3", 1, CW_MEM),
    m(22, 410, KEYER, "CW MEMORY 4", 1, CW_MEM),
    m(23, 411, KEYER, "CW MEMORY 5", 1, CW_MEM),
    m(24, 501, GEN, "NB WIDTH", 1, Kind::Choice(&["1 ms", "3 ms", "10 ms"])),
    m(25, 502, GEN, "NB REJECTION", 1, Kind::Choice(&["10 dB", "30 dB", "50 dB"])),
    m(26, 503, GEN, "NB LEVEL", 2, num(0, 10, 1, "")),
    m(27, 504, GEN, "BEEP LEVEL", 3, LEVEL),
    m(28, 505, GEN, "RF/SQL VR", 1, Kind::Choice(&["RF", "SQL"])),
    m(29, 506, GEN, "CAT RATE", 1, Kind::Choice(&["4800 bps", "9600 bps", "19200 bps", "38400 bps"])),
    m(30, 507, GEN, "CAT TOT", 1, Kind::Choice(&["10 ms", "100 ms", "1000 ms", "3000 ms"])),
    m(31, 508, GEN, "CAT RTS", 1, ENABLE),
    m(32, 509, GEN, "MEM GROUP", 1, ENABLE),
    m(33, 510, GEN, "FM SETTING", 1, ENABLE),
    m(34, 511, GEN, "REC SETTING", 1, ENABLE),
    m(35, 512, GEN, "ATAS SETTING", 1, ENABLE),
    m(36, 513, GEN, "QUICK SPL FREQ", 3, signed(-20, 20, 1, "kHz")),
    m(37, 514, GEN, "TX TOT (0=OFF)", 2, num(0, 30, 1, "min")),
    m(38, 515, GEN, "MIC SCAN", 1, ENABLE),
    m(39, 516, GEN, "MIC SCAN RESUME", 1, Kind::Choice(&["PAUSE", "TIME"])),
    m(40, 517, GEN, "REF FREQ ADJ", 3, signed(-25, 25, 1, "")),
    m(41, 518, GEN, "CLAR SELECT", 1, Kind::Choice(&["RX", "TX", "TRX"])),
    m(42, 519, GEN, "APO", 1, Kind::Choice(&["OFF", "1 h", "2 h", "4 h", "6 h", "8 h", "10 h", "12 h"])),
    m(43, 520, GEN, "FAN CONTROL", 1, Kind::Choice(&["NORMAL", "CONTEST"])),
    m(44, 601, AM, "AM LCUT FREQ", 2, LCUT),
    m(45, 602, AM, "AM LCUT SLOPE", 1, SLOPE),
    m(46, 603, AM, "AM HCUT FREQ", 2, HCUT),
    m(47, 604, AM, "AM HCUT SLOPE", 1, SLOPE),
    m(48, 605, AM, "AM MIC SELECT", 1, MIC_REAR),
    m(49, 606, AM, "AM OUT LEVEL", 3, LEVEL),
    m(50, 607, AM, "AM PTT SELECT", 1, PTT),
    m(51, 701, CW, "CW LCUT FREQ", 2, LCUT),
    m(52, 702, CW, "CW LCUT SLOPE", 1, SLOPE),
    m(53, 703, CW, "CW HCUT FREQ", 2, HCUT),
    m(54, 704, CW, "CW HCUT SLOPE", 1, SLOPE),
    m(55, 705, CW, "CW OUT LEVEL", 3, LEVEL),
    m(56, 706, CW, "CW AUTO MODE", 1, Kind::Choice(&["OFF", "50M", "ON"])),
    m(57, 707, CW, "CW BFO", 1, USB_LSB_AUTO),
    m(58, 708, CW, "CW BK-IN TYPE", 1, Kind::Choice(&["SEMI", "FULL"])),
    m(59, 709, CW, "CW BK-IN DELAY", 4, DELAY),
    m(60, 710, CW, "CW WAVE SHAPE", 1, Kind::Coded(&[(1, "2 ms"), (2, "4 ms")])),
    m(61, 711, CW, "CW FREQ DISPLAY", 1, Kind::Choice(&["FREQ", "PITCH"])),
    m(62, 712, CW, "PC KEYING", 1, Kind::Choice(&["OFF", "DAKY", "RTS", "DTR"])),
    m(63, 713, CW, "QSK DELAY TIME", 1, Kind::Choice(&["15 ms", "20 ms", "25 ms", "30 ms"])),
    m(64, 801, DATA, "DATA MODE", 1, Kind::Choice(&["PSK", "OTHERS"])),
    m(65, 802, DATA, "PSK TONE", 1, Kind::Choice(&["1000 Hz", "1500 Hz", "2000 Hz"])),
    m(66, 803, DATA, "OTHER DISP", 5, signed(-3000, 3000, 10, "Hz")),
    m(67, 804, DATA, "OTHER SHIFT", 5, signed(-3000, 3000, 10, "Hz")),
    m(68, 805, DATA, "DATA LCUT FREQ", 2, LCUT),
    m(69, 806, DATA, "DATA LCUT SLOPE", 1, SLOPE),
    m(70, 807, DATA, "DATA HCUT FREQ", 2, HCUT),
    m(71, 808, DATA, "DATA HCUT SLOPE", 1, SLOPE),
    m(72, 809, DATA, "DATA IN SELECT", 1, MIC_REAR),
    m(73, 810, DATA, "DATA PTT SELECT", 1, PTT),
    m(74, 811, DATA, "DATA OUT LEVEL", 3, LEVEL),
    m(75, 812, DATA, "DATA BFO", 1, USB_LSB),
    m(76, 901, FM, "FM MIC SELECT", 1, MIC_REAR),
    m(77, 902, FM, "FM OUT LEVEL", 3, LEVEL),
    m(78, 903, FM, "PKT PTT SELECT", 1, PTT),
    m(79, 904, FM, "RPT SHIFT 28MHz", 4, num(0, 1000, 10, "kHz")),
    // The reference book says 1 digit; the values are 0000-4000.
    m(80, 905, FM, "RPT SHIFT 50MHz", 4, num(0, 4000, 10, "kHz")),
    m(81, 906, FM, "DCS POLARITY", 1, Kind::Choice(&["Tn-Rn", "Tn-Riv", "Tiv-Rn", "Tiv-Riv"])),
    m(82, 1001, RTTY, "RTTY LCUT FREQ", 2, LCUT),
    m(83, 1002, RTTY, "RTTY LCUT SLOPE", 1, SLOPE),
    m(84, 1003, RTTY, "RTTY HCUT FREQ", 2, HCUT),
    m(85, 1004, RTTY, "RTTY HCUT SLOPE", 1, SLOPE),
    m(86, 1005, RTTY, "RTTY SHIFT PORT", 1, Kind::Choice(&["SHIFT", "DTR", "RTS"])),
    m(87, 1006, RTTY, "RTTY POLARITY-R", 1, NORREV),
    m(88, 1007, RTTY, "RTTY POLARITY-T", 1, NORREV),
    m(89, 1008, RTTY, "RTTY OUT LEVEL", 3, LEVEL),
    m(90, 1009, RTTY, "RTTY SHIFT FREQ", 1, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz", "850 Hz"])),
    m(91, 1010, RTTY, "RTTY MARK FREQ", 1, Kind::Choice(&["1275 Hz", "2125 Hz"])),
    m(92, 1011, RTTY, "RTTY BFO", 1, USB_LSB),
    m(93, 1101, SSB, "SSB LCUT FREQ", 2, LCUT),
    m(94, 1102, SSB, "SSB LCUT SLOPE", 1, SLOPE),
    m(95, 1103, SSB, "SSB HCUT FREQ", 2, HCUT),
    m(96, 1104, SSB, "SSB HCUT SLOPE", 1, SLOPE),
    m(97, 1105, SSB, "SSB MIC SELECT", 1, MIC_REAR),
    m(98, 1106, SSB, "SSB OUT LEVEL", 3, LEVEL),
    m(99, 1107, SSB, "SSB BFO", 1, USB_LSB_AUTO),
    m(100, 1108, SSB, "SSB PTT SELECT", 1, PTT),
    m(101, 1109, SSB, "SSB TX BPF", 1, Kind::Choice(&["100-3000 Hz", "100-2900 Hz", "200-2800 Hz", "300-2700 Hz", "400-2600 Hz"])),
    m(102, 1201, DSP, "APF WIDTH", 1, Kind::Choice(&["NARROW", "MEDIUM", "WIDE"])),
    m(103, 1202, DSP, "CONTOUR LEVEL", 3, signed(-40, 20, 1, "dB")),
    m(104, 1203, DSP, "CONTOUR WIDTH", 2, num(1, 11, 1, "")),
    m(105, 1204, DSP, "IF NOTCH WIDTH", 1, Kind::Choice(&["NARROW", "WIDE"])),
    m(106, 1301, SCOPE, "SCP START CYCLE", 1, Kind::Choice(&["OFF", "3 s", "5 s", "10 s"])),
    m(107, 1302, SCOPE, "SCP SPAN FREQ", 1, Kind::Choice(&["37.5 kHz", "75 kHz", "150 kHz", "375 kHz", "750 kHz"])),
    m(108, 1401, TUNING, "QUICK DIAL", 1, Kind::Choice(&["50 kHz", "100 kHz", "500 kHz"])),
    m(109, 1402, TUNING, "SSB DIAL STEP", 1, STEP_2_5_10),
    m(110, 1403, TUNING, "AM DIAL STEP", 1, STEP_10_100),
    m(111, 1404, TUNING, "FM DIAL STEP", 1, STEP_10_100),
    m(112, 1405, TUNING, "DIAL STEP", 1, STEP_2_5_10),
    m(113, 1406, TUNING, "AM CH STEP", 1, Kind::Choice(&["2.5 kHz", "5 kHz", "9 kHz", "10 kHz", "12.5 kHz", "25 kHz"])),
    m(114, 1407, TUNING, "FM CH STEP", 1, Kind::Choice(&["5 kHz", "6.25 kHz", "10 kHz", "12.5 kHz", "15 kHz", "20 kHz", "25 kHz"])),
    m(115, 1501, TXA, "EQ1 FREQ", 2, EQ1_FREQ),
    m(116, 1502, TXA, "EQ1 LEVEL", 3, EQ_LEVEL),
    m(117, 1503, TXA, "EQ1 BWTH", 2, EQ_BW),
    m(118, 1504, TXA, "EQ2 FREQ", 2, EQ2_FREQ),
    m(119, 1505, TXA, "EQ2 LEVEL", 3, EQ_LEVEL),
    m(120, 1506, TXA, "EQ2 BWTH", 2, EQ_BW),
    m(121, 1507, TXA, "EQ3 FREQ", 2, EQ3_FREQ),
    m(122, 1508, TXA, "EQ3 LEVEL", 3, EQ_LEVEL),
    m(123, 1509, TXA, "EQ3 BWTH", 2, EQ_BW),
    m(124, 1510, TXA, "P-EQ1 FREQ", 2, EQ1_FREQ),
    m(125, 1511, TXA, "P-EQ1 LEVEL", 3, EQ_LEVEL),
    m(126, 1512, TXA, "P-EQ1 BWTH", 2, EQ_BW),
    m(127, 1513, TXA, "P-EQ2 FREQ", 2, EQ2_FREQ),
    m(128, 1514, TXA, "P-EQ2 LEVEL", 3, EQ_LEVEL),
    m(129, 1515, TXA, "P-EQ2 BWTH", 2, EQ_BW),
    m(130, 1516, TXA, "P-EQ3 FREQ", 2, EQ3_FREQ),
    m(131, 1517, TXA, "P-EQ3 LEVEL", 3, EQ_LEVEL),
    m(132, 1518, TXA, "P-EQ3 BWTH", 2, EQ_BW),
    m(133, 1601, TXG, "HF SSB PWR", 3, WATTS),
    m(134, 1602, TXG, "HF AM PWR", 3, WATTS_AM),
    m(135, 1603, TXG, "HF PWR", 3, WATTS),
    m(136, 1604, TXG, "50M SSB PWR", 3, WATTS),
    m(137, 1605, TXG, "50M AM PWR", 3, WATTS_AM),
    m(138, 1606, TXG, "50M PWR", 3, WATTS),
    m(139, 1607, TXG, "SSB MIC GAIN", 3, LEVEL),
    m(140, 1608, TXG, "AM MIC GAIN", 3, LEVEL),
    m(141, 1609, TXG, "FM MIC GAIN", 3, LEVEL),
    m(142, 1610, TXG, "DATA MIC GAIN", 3, LEVEL),
    m(143, 1611, TXG, "SSB DATA GAIN", 3, LEVEL),
    m(144, 1612, TXG, "AM DATA GAIN", 3, LEVEL),
    m(145, 1613, TXG, "FM DATA GAIN", 3, LEVEL),
    m(146, 1614, TXG, "DATA DATA GAIN", 3, LEVEL),
    m(147, 1615, TXG, "TUNER SELECT", 1, Kind::Choice(&["OFF", "EXTERNAL", "ATAS", "LAMP"])),
    m(148, 1616, TXG, "VOX SELECT", 1, Kind::Choice(&["MIC", "DATA"])),
    m(149, 1617, TXG, "VOX GAIN", 3, LEVEL),
    m(150, 1618, TXG, "VOX DELAY", 4, DELAY),
    m(151, 1619, TXG, "ANTI VOX GAIN", 3, LEVEL),
    m(152, 1620, TXG, "DATA VOX GAIN", 3, LEVEL),
    m(153, 1621, TXG, "DATA VOX DELAY", 4, DELAY),
    m(154, 1622, TXG, "ANTI DVOX GAIN", 3, LEVEL),
    m(155, 1623, TXG, "EMERGENCY FREQ", 1, ENABLE),
    // Read only ("V01-23" = 0123).
    m(156, 1801, VER, "MAIN VERSION", 4, VERSION),
    m(157, 1802, VER, "DSP VERSION", 4, VERSION),
    m(158, 1803, VER, "LCD VERSION", 4, VERSION),
];

// ------------------------------------------------------------- the radio

/// RigHelm commands the FT-891 carries out (`FT` as `ST`, `RT` as `CF`).
const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AM", "BA", "BC", "BD", "BI", "BP", "BS", "BU", "BY", "CH", "CN", "CO", "CS", "CT", "DN", "ED", "EK",
    "EU", "EX", "FA", "FB", "FS", "FT", "GT", "ID", "IF", "IS", "KM", "KP", "KR", "KS", "KY", "LK", "LM", "MA", "MC", "MD", "MG",
    "ML", "MR", "MS", "MW", "MX", "NA", "NB", "NL", "NR", "OI", "OS", "PA", "PB", "PC", "PL", "PR", "PS", "QI", "QR", "QS", "RA",
    "RC", "RD", "RG", "RI", "RL", "RM", "RS", "RT", "RU", "SC", "SD", "SH", "SM", "SQ", "SV", "TS", "TX", "UL", "UP", "VD", "VG",
    "VM", "VX",
];

const _: () = assert!(COMMANDS.len() + 11 == ALL_COMMANDS.len());

/// Band keys (`BS`); there is no 5 MHz key (02), and MW (12) is left out.
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
];

pub static FT891: RadioModel = RadioModel {
    key: "ft891",
    name: "FT-891",
    maker: Maker::Yaesu,
    family: Family::Yaesu,
    proto: Proto::Text(&SPEC),
    id: "0650",
    civ_addr: 0,
    hamlib: 1036,
    freq_min: 30_000,
    freq_max: 56_000_000,
    bands: BANDS,
    modes: MODES,
    menu: MENU,
    roles: Roles {
        // SSB (11-05), AM (06-05), FM (09-01) MIC SELECT, DATA IN SELECT
        // (08-09): MIC or REAR (the USB audio).
        mic_sel: &[97, 48, 76, 72],
        mic_front: 0,
        mic_usb: 1,
        // 07-12 PC KEYING: OFF / RTS.
        pc_keying: Some((62, 0, 2)),
        // 05-07 CAT TOT 100 ms.
        cat_tot: Some((30, "1")),
        cat_items: &[29, 30, 31],
        // 07-07 CW BFO.
        cw_bfo: Some((57, ["0", "1", "2"])),
        // 07-11 CW FREQ DISPLAY = PITCH.
        cw_pitch_display: Some((61, "1")),
        // 08-01 DATA MODE = OTHERS, 08-03 OTHER DISP.
        data_other: Some((64, "1")),
        data_disp: Some(66),
        data_psk: false,
    },
    caps: Caps {
        commands: COMMANDS,
        widths,
        narrow: true,
        preamp: &[(0, "IPO"), (1, "AMP")],
        att: &[(0, "OFF"), (1, "ON")],
        agc: types::AGC_SET,
        roofing: &[],
        roofing_answers: &[],
        nb: &[(0, "OFF"), (1, "ON")],
        nr_max: 15,
        nb_level_max: 10,
        antennas: 1,
        power: (5, 100),
        meter_select: &[(0, "COMP"), (1, "ALC"), (2, "PO"), (3, "SWR"), (4, "ID")],
        voice_memories: 5,
        keyer_memories: 5,
        keyer_memory_len: 50,
        memory: Memory { first: 1, last: 117, pms: Some(100) },
        shared_clarifier: true,
    },
    bauds: &[38400, 19200, 9600, 4800],
    stop_bits: 2,
    rts_cts: true,
    sim: &[],
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::WidthClass;

    fn cx() -> Ctx {
        Ctx::new(&FT891)
    }

    fn d(f: &str) -> Vec<String> {
        down(&mut cx(), f)
    }

    fn u(f: &str) -> Vec<String> {
        up(&mut cx(), f)
    }

    #[test]
    fn frequencies_and_channels() {
        assert_eq!(d("FA14250000;"), vec!["FA014250000;"]);
        assert_eq!(u("FA050313000;"), vec!["FA50313000;"]);
        assert_eq!(u("IF001007074000+000010200000;"), vec!["IF00107074000+000010200000;"]);
        assert_eq!(u("IFP1U007074000+000010C50000;"), vec!["IF10107074000+000010C50000;"]);
        assert_eq!(u("IFEMG005167500+000000250000;"), vec!["IF00005167500+000000250000;"]);
        assert_eq!(d("IF10107074000+000010C50000;"), vec!["IFP1U007074000+000010C50000;"]);
        assert_eq!(d("MC117;"), vec!["MCP9U;"]);
        assert_eq!(d("MC005;"), vec!["MC005;"]);
        assert_eq!(u("MCP1L;"), vec!["MC100;"]);
        assert_eq!(d("MR102;"), vec!["MRP2L;"]);
        assert_eq!(u("MRP2L014074000+000000200000;"), vec!["MR10214074000+000000200000;"]);
        assert_eq!(d("MW10014074000+000000201230;"), vec!["MWP1L014074000+000000201000;"]);
    }

    #[test]
    fn menu_numbers_are_group_and_item() {
        assert_eq!(MENU.len(), 158);
        assert_eq!(d("EX030;"), vec!["EX0507;"]);
        assert_eq!(d("EX0301;"), vec!["EX05071;"]);
        assert_eq!(u("EX05071;"), vec!["EX0301;"]);
        assert_eq!(u("EX0803+0100;"), vec!["EX066+0100;"]);
        assert_eq!(u("EX1701;"), Vec::<String>::new()); // RESET
        assert_eq!(d("EX158;"), vec!["EX1803;"]);
        // The roles point at the items they say.
        let at = |addr: u32| MENU.iter().find(|m| m.addr == addr).unwrap().num;
        assert_eq!(FT891.roles.mic_sel, &[at(1105), at(605), at(901), at(809)]);
        assert_eq!(FT891.roles.pc_keying.unwrap().0, at(712));
        assert_eq!(FT891.roles.cat_tot.unwrap().0, at(507));
        assert_eq!(FT891.roles.cat_items, &[at(506), at(507), at(508)]);
        assert_eq!(FT891.roles.cw_bfo.unwrap().0, at(707));
        assert_eq!(FT891.roles.cw_pitch_display.unwrap().0, at(711));
        assert_eq!(FT891.roles.data_other.unwrap().0, at(801));
        assert_eq!(FT891.roles.data_disp, Some(at(803)));
        for (i, w) in MENU.windows(2).enumerate() {
            assert!(w[0].addr < w[1].addr, "menu {} out of order", i + 1);
        }
    }

    #[test]
    fn split_and_clarifier() {
        assert_eq!(d("FT3;"), vec!["ST1;"]);
        assert_eq!(d("FT2;"), vec!["ST0;"]);
        assert_eq!(d("FT;"), vec!["ST;"]);
        assert_eq!(u("ST1;"), vec!["FT1;"]);
        assert_eq!(u("ST2;"), vec!["FT1;"]);
        assert_eq!(u("ST0;"), vec!["FT0;"]);
        assert_eq!(u("ST;"), vec!["FT;"]);
        assert_eq!(d("RT1;"), vec!["CF010;"]);
        assert_eq!(d("RT;"), vec!["CF0;"]);
        assert_eq!(u("CF000;"), vec!["RT0;"]);
        assert_eq!(u("CF0;"), vec!["RT;"]);
        assert!(d("XT1;").is_empty());
        assert!(u("FT1;").is_empty());
    }

    #[test]
    fn shift_width_levels() {
        assert_eq!(d("IS0+0120;"), vec!["IS01+0120;"]);
        assert_eq!(d("IS0-0000;"), vec!["IS00-0000;"]);
        assert_eq!(u("IS01-0300;"), vec!["IS0-0300;"]);
        assert_eq!(u("IS00+0300;"), vec!["IS0+0000;"]);
        assert_eq!(d("IS0;"), vec!["IS0;"]);
        assert_eq!(d("SH012;"), vec!["SH0112;"]);
        assert_eq!(u("SH0117;"), vec!["SH017;"]);
        assert_eq!(d("SH0;"), vec!["SH0;"]);
        assert_eq!(d("RG0255;"), vec!["RG0030;"]);
        assert_eq!(u("RG0015;"), vec!["RG0128;"]);
        // What was sent reads back as sent.
        let mut c = cx();
        assert_eq!(down(&mut c, "RG0200;"), vec!["RG0024;"]);
        assert_eq!(up(&mut c, "RG0024;"), vec!["RG0200;"]);
        assert_eq!(up(&mut c, "RG0010;"), vec!["RG0085;"]);
        assert!(d("RM8;").is_empty());
        assert_eq!(d("RM6;"), vec!["RM6;"]);
        assert_eq!(u("ID0650;"), vec!["ID0650;"]);
        assert!(d("MD0A;").is_empty());
        assert!(d("BS02;").is_empty());
    }

    #[test]
    fn missing_commands() {
        for f in ["AN0;", "FR;", "VS;", "VS1;", "RF0;", "VF0;", "SF;", "RO;", "EN01;", "EM1;", "DA;", "DA101010;", "XT;"] {
            assert!(d(f).is_empty(), "{f}");
        }
        assert!(u("ZI;").is_empty());
        assert!(u("MT001;").is_empty());
    }

    #[test]
    fn round_trip() {
        for x in [
            "FA14074000;", "FB50313000;", "FA;", "MD02;", "MD0C;", "IF00114074000+012010200000;", "IF10514074000+000010200000;",
            "OI00107074000-000000300000;", "MR11707074000+000000C10000;", "MC117;", "MC001;", "EX030;", "EX0301;", "EX066-0300;",
            "EX1560123;", "AG0128;", "RG0200;", "RG0000;", "RG0255;", "SQ0051;", "CN012;", "CO0002;", "CO0110;", "CO0225;", "LK5;",
            "FS4;", "BP01150;", "BS10;", "SH012;", "SH0;", "IS0+0120;", "IS0-1200;", "IS0+0000;", "FT1;", "FT0;", "FT;", "RT1;",
            "RT0;", "RT;", "NA01;", "PA01;", "RA01;", "NB01;", "NL0005;", "RL015;", "PC050;", "KP40;", "KS025;", "TX1;", "MX0;",
            "AI1;", "ID0650;", "SM0123;", "RM6100;", "GT04;", "MS2;", "PR01;", "KM1CQ TEST};", "AC002;", "VD0300;", "CT01;",
        ] {
            let mut c = cx();
            let native = down(&mut c, x);
            assert!(!native.is_empty(), "{x} has no translation");
            let back: Vec<String> = native.iter().flat_map(|n| up(&mut c, n)).collect();
            assert_eq!(back.last().map(String::as_str), Some(x), "{x} -> {native:?} -> {back:?}");
        }
    }

    #[test]
    fn widths_and_id() {
        assert_eq!(FT891.width_hz(WidthClass::Ssb, false, 21), Some(3200));
        assert_eq!(FT891.width_hz(WidthClass::Rtty, true, 0), Some(300));
        assert_eq!(crate::model::by_id_answer("ID0650;").map(|m| m.key), Some("ft891"));
    }
}
