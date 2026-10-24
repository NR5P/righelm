//! Yaesu FT-950: the generation before the FTDX3000, most commands the
//! same.
//!
//! From the *FT-950 CAT Operation Reference Book* (2007). Where it differs:
//!
//! * Menu: 118 items of its own (`EX001`-`EX118`). The book gives no digit
//!   counts; the widths follow the value formats it shows.
//! * Up to 56 MHz; two antennas; no 60 m band key (`BS02`).
//! * `MG` and the monitor level `ML1` are 0-255 (RigHelm 0-100); `NL`
//!   0-255. `PL`, `VG`, `PC` are the FTDX3000's.
//! * `KP` is 00-15 in 50 Hz steps (RigHelm 10 Hz steps).
//! * `LK` and `FS` are one switch, not one per VFO.
//! * `PR P1;` is one value: 0 off, 1 speech processor, 2 mic EQ.
//! * `DA` is one dimmer (P2 fixed); `SF` has BAND (0) and no DIAL-B, CLAR,
//!   MODE or uTUNE; `ED`/`EU` P1 2 is the SELECT knob; `CO` has no APF.
//! * `RF`: AUTO / 15 / 6 / 3 kHz only. `MS` has no PO.
//! * Its own width table (page 16); PKT uses the RTTY widths.
//! * No `BA`, `EM`, `EN`.

use crate::dialect::text::{Ctx, TextSpec, menu_down, menu_up, parts, read_prefix};
use crate::menu::{Kind, MenuItem, num, signed};
use crate::model::{Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::models::ftdx5000::{
    AGC_DELAY, CW_WEIGHT, DELAY, ENABLE, EQ_BW, EQ1_FREQ, EQ2_FREQ, EQ3_FREQ, LEVEL, ab_switch_down, ab_switch_up,
    encoder, level, m, pitch50_down, pitch50_up, pr_down, pr_up, sf_basic, upper,
};
use crate::types::{self, Mode, WidthClass};

pub static SPEC: TextSpec = TextSpec { down, up, answer_prefix: read_prefix };

fn down(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !cx.model.has(&code) {
        return Vec::new();
    }
    match code.as_str() {
        "EX" => menu_down(cx, f, 3),
        "MG" => level(f, 0, 100, 255),
        "ML" if p.starts_with('1') => level(f, 1, 100, 255),
        "KP" => pitch50_down(f),
        "LK" | "FS" => ab_switch_down(f),
        "PR" => pr_down(cx, f),
        "DA" => dimmer_down(f),
        "SF" => sf_basic(f),
        "ED" | "EU" => encoder(f),
        "CO" if p.get(1..2) == Some("2") => Vec::new(),
        "RF" if matches!(p.get(1..2), Some("4" | "5")) && p.len() == 2 => Vec::new(),
        _ => vec![upper(f)],
    }
}

fn up(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !cx.model.has(&code) {
        return Vec::new();
    }
    match code.as_str() {
        "EX" => menu_up(cx, f, 3),
        "MG" => level(f, 0, 255, 100),
        "ML" if p.starts_with('1') => level(f, 1, 255, 100),
        "KP" => pitch50_up(f),
        "LK" | "FS" => ab_switch_up(f),
        "PR" => pr_up(cx, f),
        "DA" => dimmer_up(f),
        "SF" => sf_basic(f),
        "ED" | "EU" => encoder(f),
        _ => vec![upper(f)],
    }
}

/// `DA`: one dimmer (RigHelm's first); P2 is fixed (00).
fn dimmer_down(f: &str) -> Vec<String> {
    match parts(f).map(|(_, p)| p) {
        Some("") => vec!["DA;".into()],
        Some(p) if p.len() == 6 => vec![format!("DA{}00;", &p[..2])],
        _ => Vec::new(),
    }
}

fn dimmer_up(f: &str) -> Vec<String> {
    match parts(f).map(|(_, p)| p) {
        Some("") => vec!["DA;".into()],
        Some(p) if p.len() == 4 => vec![format!("DA{}0000;", &p[..2])],
        _ => Vec::new(),
    }
}

/// Width table (page 16). 00 is the default for the mode and NARROW.
fn widths(class: WidthClass, narrow: bool) -> Vec<(u8, u32)> {
    const SSB: [u32; 21] =
        [0, 200, 400, 600, 850, 1100, 1350, 1500, 1650, 1800, 1950, 2100, 2250, 2400, 2450, 2500, 2600, 2700, 2800, 2900, 3000];
    const CW: [u32; 14] = [0, 0, 0, 100, 200, 300, 400, 500, 800, 1200, 1400, 1700, 2000, 2400];
    let (table, range, default): (&[u32], std::ops::RangeInclusive<usize>, u32) = match (class, narrow) {
        (WidthClass::Ssb, true) => (&SSB, 1..=9, 1800),
        (WidthClass::Ssb, false) => (&SSB, 9..=20, 2400),
        (WidthClass::Cw, true) => (&CW, 3..=7, 500),
        (WidthClass::Cw, false) => (&CW, 7..=13, 2400),
        (WidthClass::Rtty, true) => (&CW, 3..=7, 300),
        (WidthClass::Rtty, false) => (&CW, 7..=13, 500),
        (WidthClass::Fixed, _) => return Vec::new(),
    };
    std::iter::once((0, default)).chain(range.map(|i| (i as u8, table[i]))).collect()
}

/// Every RigHelm command but BA, EM and EN.
const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AM", "AN", "BC", "BD", "BI", "BP", "BS", "BU", "BY", "CH", "CN", "CO00", "CO01", "CS", "CT", "DA",
    "DN", "ED", "EK", "EU", "EX", "FA", "FB", "FR", "FS", "FT", "GT", "ID", "IF", "IS", "KM", "KP", "KR", "KS", "KY",
    "LK", "LM", "MA", "MC", "MD", "MG", "ML", "MR", "MS", "MW", "MX", "NA", "NB", "NL", "NR", "OI", "OS", "PA", "PB",
    "PC", "PL", "PR", "PS", "QI", "QR", "QS", "RA", "RC", "RD", "RF", "RG", "RI", "RL", "RM", "RO", "RS", "RT", "RU",
    "SC", "SD", "SF", "SH", "SM", "SQ", "SV", "TS", "TX", "UL", "UP", "VD", "VF", "VG", "VM", "VS", "VX", "XT",
];

pub static FT950: RadioModel = RadioModel {
    key: "ft950",
    name: "FT-950",
    maker: Maker::Yaesu,
    family: Family::Yaesu,
    proto: Proto::Text(&SPEC),
    id: "0310",
    civ_addr: 0,
    hamlib: 1028,
    freq_min: 30_000,
    freq_max: 56_000_000,
    bands: crate::models::ftdx1200::BANDS,
    modes: &Mode::ALL,
    menu: MENU,
    roles: Roles {
        // No MIC SEL item, no USB port.
        mic_sel: &[],
        mic_front: 0,
        mic_usb: 0,
        pc_keying: None,
        cat_tot: Some((27, "1")),
        cat_items: &[26, 27, 28],
        cw_bfo: Some((42, ["0", "1", "2"])),
        cw_pitch_display: Some((47, "1")),
        // No DATA MODE item: PKT uses the RTTY/PKT widths (page 16).
        // Pointing at PKT DISPLAY OFFSET with a value it never has makes
        // RigHelm use them.
        data_other: None,
        data_disp: None,
        data_psk: true,
    },
    caps: Caps {
        commands: COMMANDS,
        widths,
        narrow: true,
        preamp: &[(0, "IPO"), (1, "AMP1"), (2, "AMP2")],
        att: &[(0, "OFF"), (1, "6"), (2, "12"), (3, "18")],
        agc: types::AGC_SET,
        roofing: &[('0', "AUTO"), ('1', "15 kHz"), ('2', "6 kHz"), ('3', "3 kHz")],
        roofing_answers: &[('1', "15 kHz"), ('2', "6 kHz"), ('3', "3 kHz"), ('4', "AUTO 15 kHz"), ('5', "AUTO 6 kHz"), ('6', "AUTO 3 kHz")],
        nb: &[(0, "OFF"), (1, "NB"), (2, "WIDE")],
        nr_max: 15,
        nb_level_max: 255,
        antennas: 2,
        power: (5, 100),
        meter_select: &[(0, "COMP"), (1, "ALC"), (3, "SWR"), (4, "ID"), (5, "VDD")],
        voice_memories: 5,
        keyer_memories: 5,
        keyer_memory_len: 50,
        memory: Memory { first: 1, last: 117, pms: Some(100) },
        shared_clarifier: true,
    },
    // RS-232C; the radio's default is 4800 bps, 8N2, hardware handshake.
    bauds: &[38400, 19200, 9600, 4800],
    stop_bits: 2,
    rts_cts: true,
    sim: &[],
};

// ------------------------------------------------------------- menu

const PEAK: Kind = Kind::Choice(&["OFF", "0.5 s", "1.0 s", "2.0 s"]);
const ONOFF: Kind = Kind::Choice(&["OFF", "ON"]);
const KEY_JACK: Kind = Kind::Choice(&["OFF", "BUG", "IAMBIC W/O ACS", "IAMBIC W/ ACS"]);
const WIRING: Kind = Kind::Choice(&["NORMAL", "REVERSE"]);
const CW_MEM: Kind = Kind::Choice(&["TEXT MEMORY", "MESSAGE MEMORY"]);
const EQ_GAIN: Kind = signed(-10, 10, 1, "dB");
const AGC: &str = "AGC";
const DISP: &str = "DISPLAY";
const DVS: &str = "DVS";
const KEYER: &str = "KEYER";
const GEN: &str = "GENERAL";
const AM: &str = "MODE-AM";
const CW: &str = "MODE-CW";
const DAT: &str = "MODE-DAT";
const PKT: &str = "MODE-PKT";
const FM: &str = "MODE-FM";
const RTTY: &str = "MODE-RTY";
const SSB: &str = "MODE-SSB";
const DSP: &str = "RX DSP";
const SCOPE: &str = "SCOPE";
const TUNING: &str = "TUNING";
const TXA: &str = "TX AUDIO";
const TXG: &str = "TX GNRL";

/// Tables 2-3 (pages 7-8).
pub static MENU: &[MenuItem] = &[
    m(1, AGC, "FAST DELAY TIME", 4, AGC_DELAY),
    m(2, AGC, "MID DELAY TIME", 4, AGC_DELAY),
    m(3, AGC, "SLOW DELAY TIME", 4, AGC_DELAY),
    m(4, DISP, "DISPLAY COLOR", 1, Kind::Choice(&["COOL BLUE", "CONTRAST BLUE", "FLASH BLUE", "CONTRAST UMBER", "UMBER"])),
    m(5, DISP, "DISPLAY DIMMER", 2, num(0, 15, 1, "")),
    m(6, DISP, "TUNING OFFSET INDICATOR", 1, Kind::Choice(&["CLARIFIER OFFSET", "CW TUNING METER", "VRF PEAK POSITION"])),
    m(7, DISP, "S-METER PEAK HOLD", 1, PEAK),
    m(8, DISP, "PO METER PEAK HOLD", 1, PEAK),
    m(9, DISP, "ALC METER PEAK HOLD", 1, PEAK),
    m(10, DISP, "ID METER PEAK HOLD", 1, PEAK),
    m(11, DISP, "ROTATOR STARTING POINT", 1, Kind::Choice(&["0°", "90°", "180°", "270°"])),
    m(12, DISP, "ROTATOR NEEDLE PRECISELY (minus)", 2, num(0, 30, 2, "°")),
    m(13, DISP, "QMB MARKER", 1, ONOFF),
    m(14, DVS, "DVS-6 OUTPUT LEVEL", 3, LEVEL),
    m(15, DVS, "DVS-6 TX LEVEL", 3, LEVEL),
    m(16, KEYER, "CW BEACON (0=OFF)", 3, num(0, 255, 1, "s")),
    m(17, KEYER, "CONTEST NUMBER STYLE", 1, Kind::Choice(&["1290", "AUNO", "AUNT", "A2NO", "A2NT", "12NO", "12NT"])),
    m(18, KEYER, "CONTEST NUMBER", 4, num(0, 9999, 1, "")),
    m(19, KEYER, "CW MEMORY 1", 1, CW_MEM),
    m(20, KEYER, "CW MEMORY 2", 1, CW_MEM),
    m(21, KEYER, "CW MEMORY 3", 1, CW_MEM),
    m(22, KEYER, "CW MEMORY 4", 1, CW_MEM),
    m(23, KEYER, "CW MEMORY 5", 1, CW_MEM),
    m(24, GEN, "ANTENNA SELECTION MODE", 1, Kind::Choice(&["BAND", "STACK"])),
    m(25, GEN, "BEEP LEVEL", 3, LEVEL),
    m(26, GEN, "CAT BAUD RATE", 1, Kind::Choice(&["4800 bps", "9600 bps", "19200 bps", "38400 bps"])),
    m(27, GEN, "CAT TIME-OUT TIMER", 1, Kind::Choice(&["10 ms", "100 ms", "1000 ms", "3000 ms"])),
    m(28, GEN, "CAT RTS PORT", 1, ONOFF),
    m(29, GEN, "MEMORY GROUP", 1, ONOFF),
    m(30, GEN, "QUICK SPLIT TUNING OFFSET", 3, signed(-20, 20, 1, "kHz")),
    m(31, GEN, "TX TIME OUT TIMER (0=OFF)", 2, num(0, 30, 1, "min")),
    m(32, GEN, "uTUNE DIAL", 1, Kind::Choice(&["STEP-2", "STEP-1"])),
    m(33, GEN, "MIC SCAN", 1, ONOFF),
    m(34, GEN, "SCAN RESUME", 1, Kind::Choice(&["PAUSE", "TIME"])),
    m(35, GEN, "FREQUENCY ADJUST", 3, signed(-25, 25, 1, "")),
    m(36, AM, "AM MIC GAIN", 4, Kind::MicGain),
    m(37, CW, "FRONT KEY JACK TYPE", 1, KEY_JACK),
    m(38, CW, "FRONT KEY JACK WIRING", 1, WIRING),
    m(39, CW, "REAR KEY JACK TYPE", 1, KEY_JACK),
    m(40, CW, "REAR KEY JACK WIRING", 1, WIRING),
    m(41, CW, "CW AUTO MODE", 1, Kind::Choice(&["OFF", "50 MHz ONLY", "ON"])),
    m(42, CW, "CW BFO INJECTION SIDE", 1, Kind::Choice(&["USB", "LSB", "AUTO"])),
    m(43, CW, "CW BREAK-IN MODE", 1, Kind::Choice(&["SEMI BREAK-IN", "FULL BREAK-IN"])),
    m(44, CW, "CW BREAK-IN DELAY TIME", 4, DELAY),
    m(45, CW, "CW PITCH FREQUENCY", 2, Kind::Steps { max_code: 15, off_at_zero: false, first: 300, step: 50, unit: "Hz" }),
    m(46, CW, "CW WEIGHT", 2, CW_WEIGHT),
    m(47, CW, "CW FREQUENCY DISPLAY", 1, Kind::Choice(&["DIRECT FREQUENCY", "PITCH OFFSET"])),
    m(48, CW, "CW PC KEYING", 1, ONOFF),
    m(49, CW, "CW QSK TIME", 1, Kind::Choice(&["15 ms", "20 ms", "25 ms", "30 ms"])),
    m(50, DAT, "DATA TX GAIN", 3, LEVEL),
    m(51, DAT, "DATA OUTPUT LEVEL", 3, LEVEL),
    m(52, DAT, "DATA VOX DELAY TIME", 4, DELAY),
    m(53, DAT, "DATA VOX GAIN", 3, LEVEL),
    m(54, PKT, "PKT FREQUENCY DISPLAY OFFSET", 5, signed(-3000, 3000, 10, "Hz")),
    m(55, PKT, "PKT CARRIER POINT FREQUENCY", 5, signed(-3000, 3000, 10, "Hz")),
    m(56, FM, "FM MIC GAIN", 4, Kind::MicGain),
    m(57, FM, "28 MHz REPEATER SHIFT", 4, num(0, 1000, 10, "kHz")),
    m(58, FM, "50 MHz REPEATER SHIFT", 4, num(0, 4000, 10, "kHz")),
    m(59, RTTY, "RX POLARITY", 1, WIRING),
    m(60, RTTY, "TX POLARITY", 1, WIRING),
    m(61, RTTY, "DATA OUTPUT LEVEL", 3, LEVEL),
    // The book numbers these two from 1 by mistake.
    m(62, RTTY, "SHIFT FREQUENCY", 1, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz", "850 Hz"])),
    m(63, RTTY, "MARK FREQUENCY", 1, Kind::Choice(&["1275 Hz", "2125 Hz"])),
    m(64, SSB, "TX BPF BANDWIDTH", 1, Kind::Choice(&["100-3000 Hz", "100-2900 Hz", "200-2800 Hz", "300-2700 Hz", "400-2600 Hz"])),
    m(65, SSB, "LSB CARRIER POINT", 4, signed(-200, 200, 10, "Hz")),
    m(66, SSB, "USB CARRIER POINT", 4, signed(-200, 200, 10, "Hz")),
    m(67, DSP, "NB LEVEL (NARROW)", 3, num(0, 255, 1, "")),
    m(68, DSP, "NB LEVEL (WIDE)", 3, num(0, 255, 1, "")),
    m(69, DSP, "CONTOUR GAIN", 3, signed(-40, 20, 1, "dB")),
    m(70, DSP, "CONTOUR WIDTH", 2, num(1, 11, 1, "")),
    m(71, DSP, "DIGITAL NOTCH FILTER", 1, ONOFF),
    m(72, DSP, "DIGITAL NOISE REDUCTION (0=OFF)", 2, num(0, 15, 1, "")),
    m(73, SCOPE, "SCAN START 1.8 MHz", 5, num(1800, 1999, 1, "kHz")),
    m(74, SCOPE, "SCAN START 3.5 MHz", 5, num(3500, 3999, 1, "kHz")),
    m(75, SCOPE, "SCAN START 5.0 MHz", 5, num(5250, 5499, 1, "kHz")),
    m(76, SCOPE, "SCAN START 7.0 MHz", 5, num(7000, 7299, 1, "kHz")),
    m(77, SCOPE, "SCAN START 10 MHz", 5, num(10100, 10149, 1, "kHz")),
    m(78, SCOPE, "SCAN START 14 MHz", 5, num(14000, 14349, 1, "kHz")),
    m(79, SCOPE, "SCAN START 18 MHz", 5, num(18000, 18199, 1, "kHz")),
    m(80, SCOPE, "SCAN START 21 MHz", 5, num(21000, 21449, 1, "kHz")),
    m(81, SCOPE, "SCAN START 24.5 MHz", 5, num(24800, 24989, 1, "kHz")),
    m(82, SCOPE, "SCAN START 28 MHz", 5, num(28000, 29699, 1, "kHz")),
    m(83, SCOPE, "SCAN START 50 MHz", 5, num(50000, 53999, 1, "kHz")),
    m(84, TUNING, "DIAL STEP", 1, Kind::Choice(&["1 Hz", "5 Hz", "10 Hz"])),
    m(85, TUNING, "DIAL CW FINE TUNING", 1, ONOFF),
    m(86, TUNING, "CLAR/VFO-B KNOB MHz STEP", 1, Kind::Choice(&["0.1 MHz", "1 MHz"])),
    m(87, TUNING, "MIC UP/DOWN AM STEP", 1, Kind::Choice(&["2.5 kHz", "5 kHz", "9 kHz", "10 kHz", "12.5 kHz"])),
    m(88, TUNING, "MIC UP/DOWN FM STEP", 1, Kind::Choice(&["5 kHz", "6.25 kHz", "10 kHz", "12.5 kHz", "20 kHz", "25 kHz"])),
    m(89, TUNING, "DIAL STEP (FM)", 1, Kind::Choice(&["10 Hz", "100 Hz"])),
    // One digit (0/1) per band: 1.8 MHz first, transverter last.
    m(90, TUNING, "MY BAND SELECT", 13, Kind::Text { max: 13 }),
    m(91, TXA, "MIC EQ LOW FREQ", 2, EQ1_FREQ),
    m(92, TXA, "MIC EQ LOW GAIN", 3, EQ_GAIN),
    m(93, TXA, "MIC EQ LOW BWTH", 2, EQ_BW),
    m(94, TXA, "MIC EQ MID FREQ", 2, EQ2_FREQ),
    m(95, TXA, "MIC EQ MID GAIN", 3, EQ_GAIN),
    m(96, TXA, "MIC EQ MID BWTH", 2, EQ_BW),
    m(97, TXA, "MIC EQ HIGH FREQ", 2, EQ3_FREQ),
    m(98, TXA, "MIC EQ HIGH GAIN", 3, EQ_GAIN),
    m(99, TXA, "MIC EQ HIGH BWTH", 2, EQ_BW),
    m(100, TXA, "PROC EQ LOW FREQ", 2, EQ1_FREQ),
    m(101, TXA, "PROC EQ LOW GAIN", 3, EQ_GAIN),
    m(102, TXA, "PROC EQ LOW BWTH", 2, EQ_BW),
    m(103, TXA, "PROC EQ MID FREQ", 2, EQ2_FREQ),
    m(104, TXA, "PROC EQ MID GAIN", 3, EQ_GAIN),
    m(105, TXA, "PROC EQ MID BWTH", 2, EQ_BW),
    m(106, TXA, "PROC EQ HIGH FREQ", 2, EQ3_FREQ),
    m(107, TXA, "PROC EQ HIGH GAIN", 3, EQ_GAIN),
    m(108, TXA, "PROC EQ HIGH BWTH", 2, EQ_BW),
    m(109, TXA, "SPEECH PROCESSOR LEVEL", 3, LEVEL),
    m(110, TXG, "ANTENNA TUNER SELECT", 1, Kind::Choice(&["INTERNAL", "EXTERNAL (FC-40)"])),
    m(111, TXG, "TX OUTPUT POWER", 3, num(5, 100, 1, "W")),
    m(112, TXG, "AM CARRIER LEVEL", 3, LEVEL),
    m(113, TXG, "TX POWER CONTROL", 1, Kind::Choice(&["ALL MODE", "CARRIER"])),
    m(114, TXG, "VOX OPERATION", 1, Kind::Choice(&["MIC", "DATA"])),
    m(115, TXG, "VOX GAIN", 3, LEVEL),
    m(116, TXG, "VOX DELAY TIME", 4, DELAY),
    m(117, TXG, "VOX ANTI-TRIP GAIN", 3, LEVEL),
    m(118, TXG, "EMERGENCY CHANNEL", 1, ENABLE),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_is_complete() {
        assert_eq!(MENU.len(), 118);
        assert_eq!(FT950.menu_item(27).unwrap().name, "CAT TIME-OUT TIMER");
        assert_eq!(FT950.menu_item(42).unwrap().name, "CW BFO INJECTION SIDE");
    }

    #[test]
    fn frames_from_the_manual() {
        let mut c = Ctx::new(&FT950);
        for f in ["FA14250000;", "FA;", "IS0+1000;", "MD0D;", "SH020;", "RF03;", "AN02;", "PC100;", "PL050;", "VG050;", "EX0453;", "ID;"] {
            assert_eq!(down(&mut c, f), vec![f], "{f}");
            assert_eq!(up(&mut c, f), vec![f], "{f}");
        }
        assert_eq!(up(&mut c, "ID0310;"), vec!["ID0310;"]);
        assert_eq!(down(&mut c, "MG050;"), vec!["MG128;"]);
        assert_eq!(up(&mut c, "MG255;"), vec!["MG100;"]);
        assert_eq!(down(&mut c, "ML1100;"), vec!["ML1255;"]);
        // Key pitch in 50 Hz steps.
        assert_eq!(down(&mut c, "KP40;"), vec!["KP08;"]);
        assert_eq!(down(&mut c, "KP42;"), vec!["KP08;"]);
        assert_eq!(up(&mut c, "KP15;"), vec!["KP75;"]);
        // One lock, one fast switch.
        assert_eq!(down(&mut c, "LK5;"), vec!["LK1;"]);
        assert_eq!(down(&mut c, "LK0;"), vec!["LK0;"]);
        assert!(down(&mut c, "LK3;").is_empty());
        assert_eq!(up(&mut c, "LK1;"), vec!["LK5;"]);
        assert_eq!(up(&mut c, "FS0;"), vec!["FS4;"]);
        // Processor / mic EQ.
        assert_eq!(up(&mut c, "PR1;"), vec!["PR01;", "PR10;"]);
        assert_eq!(down(&mut c, "PR11;"), vec!["PR2;"]);
        // One dimmer.
        assert_eq!(down(&mut c, "DA101010;"), vec!["DA1000;"]);
        assert_eq!(up(&mut c, "DA0700;"), vec!["DA070000;"]);
        // Not on this radio.
        assert!(down(&mut c, "SF5;").is_empty());
        assert!(up(&mut c, "SF0;").is_empty());
        assert!(down(&mut c, "ED205;").is_empty());
        assert!(down(&mut c, "CO0210;").is_empty());
        assert!(down(&mut c, "RF04;").is_empty());
        assert!(down(&mut c, "BA;").is_empty());
        assert!(up(&mut c, "FK1;").is_empty());
        assert!(down(&mut c, "EX119;").is_empty());
    }

    #[test]
    fn round_trip() {
        let mut c = Ctx::new(&FT950);
        for f in [
            "FA07074000;", "FB50100000;", "MD0D;", "AG0128;", "MG000;", "MG050;", "MG100;", "ML1030;", "KP00;", "KP40;",
            "KP75;", "LK4;", "LK5;", "FS5;", "PR00;", "SH013;", "NA01;", "RF02;", "NL0200;", "DA150000;", "SF2;",
            "EX0900000000000000;", "EX054+0010;", "EX111050;", "IF00114074000+012010200000;", "PC005;",
        ] {
            let native = down(&mut c, f);
            let back: Vec<String> = native.iter().flat_map(|n| up(&mut c, n)).collect();
            assert!(back.iter().any(|b| b == f), "{f} -> {native:?} -> {back:?}");
        }
    }

    #[test]
    fn widths_follow_the_manual() {
        assert_eq!(FT950.width_hz(WidthClass::Ssb, false, 14), Some(2450));
        assert_eq!(FT950.width_hz(WidthClass::Ssb, true, 9), Some(1800));
        assert_eq!(FT950.width_hz(WidthClass::Cw, true, 3), Some(100));
        assert_eq!(FT950.width_hz(WidthClass::Cw, true, 1), None);
        assert_eq!(FT950.width_hz(WidthClass::Rtty, false, 0), Some(500));
        let s = crate::RadioState::for_model(&FT950);
        assert_eq!(s.width_class(Mode::DataUsb), WidthClass::Rtty);
    }
}
