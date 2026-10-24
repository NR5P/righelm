//! Yaesu FTDX1200: the FTDX3000's little brother, the same generation of
//! CAT commands.
//!
//! From the *FTDX1200 Series CAT Operation Reference Book* (1507-E0). The
//! command layouts are the FTDX3000's, the width table (Table 5) too. Where
//! it differs:
//!
//! * Menu: 196 items, numbered like the FTDX3000's but not all the same
//!   (DISPLAY, GENERAL and MIC SEL items, CW PITCH in two digits, ...).
//! * Up to 56 MHz; two antennas; no 60 m band key (`BS02`).
//! * `DA`: P1 is fixed (00), the LED dimmer has two steps (01-02).
//! * `RF`: AUTO / 15 / 6 / 3 kHz only (no 600/300 Hz roofing filters).
//! * Modes: no DATA-FM (`A`) and no AM-N (`D`).
//! * `MS` has no PO (2).
//! * `EM`/`EN` (RTTY/PSK message memories) need the FFT-1 option, which
//!   also changes the `ID` answer (0582 with it, 0583 without), so the
//!   radio is listed twice.
//! * It has no USB port of its own: CAT is RS-232C or the SCU-17 USB
//!   interface, whose audio goes to the DATA jack.

use crate::dialect::text::{Ctx, TextSpec, menu_down, menu_up, parts, read_prefix};
use crate::menu::{Kind, MenuItem, num, signed};
use crate::model::{Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::models::ftdx5000::{
    AGC_DELAY, CW_MEM, CW_WEIGHT, DELAY, ENABLE, EQ_BW, EQ1_FREQ, EQ2_FREQ, EQ3_FREQ, HCUT, KEYER_TYPE, LCUT, LEVEL, NORREV,
    SHAPE, SLOPE, SLOPE3, TX_BPF, m, upper,
};
use crate::types::{self, Band, Mode, WidthClass};

pub static SPEC: TextSpec = TextSpec { down, up, answer_prefix: read_prefix };

fn down(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !cx.model.has(&code) {
        return Vec::new();
    }
    match code.as_str() {
        "EX" => menu_down(cx, f, 3),
        "DA" => dimmer_down(f),
        // No 600/300 Hz roofing filters.
        "RF" if matches!(p.get(1..2), Some("4" | "5")) && p.len() == 2 => Vec::new(),
        _ => vec![upper(f)],
    }
}

fn up(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, _)) = parts(f) else { return Vec::new() };
    if !cx.model.has(&code) {
        return Vec::new();
    }
    match code.as_str() {
        "EX" => menu_up(cx, f, 3),
        _ => vec![upper(f)],
    }
}

/// `DA`: P1 is fixed (00); the LED dimmer (RigHelm's second) is 01-02.
fn dimmer_down(f: &str) -> Vec<String> {
    let Some((_, p)) = parts(f) else { return Vec::new() };
    if p.is_empty() {
        return vec!["DA;".into()];
    }
    let (Some(led), Some(tft)) = (p.get(2..4).and_then(|v| v.parse::<u8>().ok()), p.get(4..6)) else { return Vec::new() };
    vec![format!("DA00{:02}{tft};", led.clamp(1, 2))]
}

fn widths(class: WidthClass, narrow: bool) -> Vec<(u8, u32)> {
    types::width_choices(class, narrow)
}

/// Every RigHelm command; EM and EN only with the FFT-1.
const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AM", "AN", "BA", "BC", "BD", "BI", "BP", "BS", "BU", "BY", "CH", "CN", "CO", "CS", "CT",
    "DA", "DN", "ED", "EK", "EU", "EX", "FA", "FB", "FR", "FS", "FT", "GT", "ID", "IF", "IS", "KM", "KP", "KR", "KS",
    "KY", "LK", "LM", "MA", "MC", "MD", "MG", "ML", "MR", "MS", "MW", "MX", "NA", "NB", "NL", "NR", "OI", "OS", "PA",
    "PB", "PC", "PL", "PR", "PS", "QI", "QR", "QS", "RA", "RC", "RD", "RF", "RG", "RI", "RL", "RM", "RO", "RS", "RT",
    "RU", "SC", "SD", "SF", "SH", "SM", "SQ", "SV", "TS", "TX", "UL", "UP", "VD", "VF", "VG", "VM", "VS", "VX", "XT",
];

const COMMANDS_FFT1: &[&str] = &[
    "AB", "AC", "AG", "AI", "AM", "AN", "BA", "BC", "BD", "BI", "BP", "BS", "BU", "BY", "CH", "CN", "CO", "CS", "CT",
    "DA", "DN", "ED", "EK", "EM", "EN", "EU", "EX", "FA", "FB", "FR", "FS", "FT", "GT", "ID", "IF", "IS", "KM", "KP",
    "KR", "KS", "KY", "LK", "LM", "MA", "MC", "MD", "MG", "ML", "MR", "MS", "MW", "MX", "NA", "NB", "NL", "NR", "OI",
    "OS", "PA", "PB", "PC", "PL", "PR", "PS", "QI", "QR", "QS", "RA", "RC", "RD", "RF", "RG", "RI", "RL", "RM", "RO",
    "RS", "RT", "RU", "SC", "SD", "SF", "SH", "SM", "SQ", "SV", "TS", "TX", "UL", "UP", "VD", "VF", "VG", "VM", "VS",
    "VX", "XT",
];

/// The FTDX3000's bands without 60 m (`BS02` is not used).
pub(crate) const BANDS: &[Band] = &[
    types::BANDS[0],
    types::BANDS[1],
    types::BANDS[3],
    types::BANDS[4],
    types::BANDS[5],
    types::BANDS[6],
    types::BANDS[7],
    types::BANDS[8],
    types::BANDS[9],
    types::BANDS[10],
    Band { code: 11, label: "GEN", lo: 30_000, hi: 56_000_000 },
];

const MODES: &[Mode] = &[
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

const ROLES: Roles = Roles {
    // AM, FM, SSB MIC SEL: 0 FRONT, 1 DATA (the rear DATA jack, where the
    // SCU-17 USB interface brings the computer's audio).
    mic_sel: &[55, 86, 103],
    mic_front: 0,
    mic_usb: 1,
    // PC KEYING keys through the rear-panel jacks, not a USB port's RTS.
    pc_keying: None,
    cat_tot: Some((40, "1")),
    cat_items: &[39, 40, 41],
    cw_bfo: Some((62, ["0", "1", "2"])),
    cw_pitch_display: Some((66, "1")),
    data_other: Some((69, "1")),
    data_disp: Some(71),
    data_psk: false,
};

const CAPS: Caps = Caps {
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
    nb_level_max: 100,
    antennas: 2,
    power: (5, 100),
    meter_select: &[(0, "COMP"), (1, "ALC"), (3, "SWR"), (4, "ID"), (5, "VDD")],
    voice_memories: 5,
    keyer_memories: 5,
    keyer_memory_len: 50,
    memory: Memory { first: 1, last: 117, pms: Some(100) },
    shared_clarifier: true,
};

pub static FTDX1200: RadioModel = RadioModel {
    key: "ftdx1200",
    name: "FTDX1200",
    maker: Maker::Yaesu,
    family: Family::Yaesu,
    proto: Proto::Text(&SPEC),
    id: "0583",
    civ_addr: 0,
    hamlib: 1034,
    freq_min: 30_000,
    freq_max: 56_000_000,
    bands: BANDS,
    modes: MODES,
    menu: MENU,
    roles: ROLES,
    caps: CAPS,
    bauds: &[38400, 19200, 9600, 4800],
    stop_bits: 2,
    rts_cts: true,
    sim: &[],
};

/// With the FFT-1 option: RTTY/PSK message memories, another `ID` answer.
pub static FTDX1200_FFT1: RadioModel = RadioModel {
    key: "ftdx1200-fft1",
    name: "FTDX1200 (FFT-1)",
    id: "0582",
    caps: Caps { commands: COMMANDS_FFT1, ..CAPS },
    ..FTDX1200
};

// ------------------------------------------------------------- menu

const MICSEL: Kind = Kind::Choice(&["FRONT", "DATA"]);
const EQ_LEVEL: Kind = signed(-20, 10, 1, "dB");
const DIAL_STEP: Kind = Kind::Choice(&["1 Hz", "5 Hz", "10 Hz"]);
const SPAN: Kind = Kind::Coded(&[(2, "20 kHz"), (3, "50 kHz"), (4, "100 kHz"), (5, "200 kHz"), (6, "500 kHz"), (7, "1000 kHz")]);
const AGC: &str = "AGC";
const DISP: &str = "DISPLAY";
const DVS: &str = "DVS";
const KEYER: &str = "KEYER";
const GEN: &str = "GENERAL";
const AM: &str = "MODE-AM";
const CW: &str = "MODE-CW";
const DATA: &str = "MODE-DATA";
const FM: &str = "MODE-FM";
const RTTY: &str = "MODE-RTTY";
const SSB: &str = "MODE-SSB";
const DSP: &str = "RX DSP";
const SCOPE: &str = "SCOPE";
const TUNING: &str = "TUNING";
const TXA: &str = "TX AUDIO";
const TXG: &str = "TX GENERAL";
const AFS: &str = "AF SCOPE";
const DEC: &str = "DECODE CW";
const EDR: &str = "ENC/DEC RTTY";
const EDP: &str = "ENC/DEC PSK";

/// Tables 2-4 (pages 7-9), with the DIGITS column as the widths.
pub static MENU: &[MenuItem] = &[
    m(1, AGC, "FAST DELAY", 4, AGC_DELAY),
    m(2, AGC, "MID DELAY", 4, AGC_DELAY),
    m(3, AGC, "SLOW DELAY", 4, AGC_DELAY),
    m(4, AGC, "AGC SLOPE", 1, Kind::Choice(&["NORMAL", "SLOPE"])),
    m(5, DISP, "MY CALL", 12, Kind::Text { max: 12 }),
    m(6, DISP, "MY CALL TIME", 1, num(0, 5, 1, "s")),
    m(7, DISP, "VFO COLOR", 1, Kind::Choice(&["BLUE", "SKY BLUE", "GREEN", "PURPLE", "RED", "ORANGE", "GRAY", "BLACK"])),
    m(8, DISP, "TFT LAYOUT", 1, Kind::Choice(&["TYPE 1", "TYPE 2"])),
    m(9, DISP, "DIMMER LED", 1, Kind::Choice(&["1", "2"])),
    m(10, DISP, "DIMMER TFT", 2, num(0, 15, 1, "")),
    m(11, DISP, "BAR DISPLAY SELECT", 1, Kind::Choice(&["CLAR", "CW TUNE", "uTUNE"])),
    m(12, DISP, "METER TYPE SELECT", 1, Kind::Choice(&["ANALOG", "BAR GRAPH"])),
    m(13, DISP, "BAR MTR PEAK HOLD", 1, Kind::Choice(&["OFF", "0.5 s", "1.0 s", "2.0 s"])),
    m(14, DISP, "ROTATOR START UP", 1, Kind::Choice(&["0°", "90°", "180°", "270°"])),
    m(15, DISP, "ROTATOR OFFSET ADJ (minus)", 2, num(0, 30, 2, "°")),
    m(16, DVS, "RX OUT LEVEL", 3, LEVEL),
    m(17, DVS, "TX OUT LEVEL", 3, LEVEL),
    m(18, KEYER, "F KEYER TYPE", 1, KEYER_TYPE),
    m(19, KEYER, "F CW KEYER", 1, Kind::Choice(&["NORMAL", "REVERSE"])),
    m(20, KEYER, "R KEYER TYPE", 1, KEYER_TYPE),
    m(21, KEYER, "R CW KEYER", 1, Kind::Choice(&["NORMAL", "REVERSE"])),
    m(22, KEYER, "ELEKEY TYPE", 1, Kind::Choice(&["ELEKEY-A", "ELEKEY-B"])),
    m(23, KEYER, "CW WEIGHT", 2, CW_WEIGHT),
    // "OFF / 1 ~ 690 sec (P2 = 000 ~ 255, 000: OFF)": the raw code.
    m(24, KEYER, "BEACON TIME (0=OFF)", 3, num(0, 255, 1, "")),
    m(25, KEYER, "NUMBER STYLE", 1, Kind::Choice(&["1290", "AUNO", "AUNT", "A2NO", "A2NT", "12NO", "12NT"])),
    m(26, KEYER, "CONTEST NUMBER", 4, num(0, 9999, 1, "")),
    m(27, KEYER, "CW MEMORY 1", 1, CW_MEM),
    m(28, KEYER, "CW MEMORY 2", 1, CW_MEM),
    m(29, KEYER, "CW MEMORY 3", 1, CW_MEM),
    m(30, KEYER, "CW MEMORY 4", 1, CW_MEM),
    m(31, KEYER, "CW MEMORY 5", 1, CW_MEM),
    m(32, GEN, "ANT SELECT", 1, Kind::Choice(&["BAND", "STACK"])),
    m(33, GEN, "ANT2 SETTING", 1, Kind::Choice(&["TRX", "R2/1"])),
    m(34, GEN, "NB LEVEL", 3, LEVEL),
    m(35, GEN, "BEEP LEVEL", 3, LEVEL),
    m(36, GEN, "MONITOR LEVEL", 3, LEVEL),
    m(37, GEN, "MOX", 1, ENABLE),
    m(38, GEN, "RF/SQL VR", 1, Kind::Choice(&["RF", "SQL"])),
    m(39, GEN, "CAT RATE", 1, Kind::Choice(&["4800 bps", "9600 bps", "19200 bps", "38400 bps"])),
    m(40, GEN, "CAT TIME OUT TIMER", 1, Kind::Choice(&["10 ms", "100 ms", "1000 ms", "3000 ms"])),
    m(41, GEN, "CAT RTS", 1, ENABLE),
    m(42, GEN, "MEM GROUP", 1, ENABLE),
    m(43, GEN, "QUICK SPLIT FREQ", 3, signed(-20, 20, 1, "kHz")),
    m(44, GEN, "TXW DIAL SELECT", 1, Kind::Choice(&["VFO-A", "VFO-B"])),
    m(45, GEN, "TX TIME OUT TIMER (0=OFF)", 2, num(0, 30, 1, "min")),
    m(46, GEN, "uTUNE DIAL STEP", 1, Kind::Choice(&["DIAL STEP-2", "DIAL STEP-1"])),
    m(47, GEN, "MIC SCAN", 1, ENABLE),
    m(48, GEN, "MIC SCAN RESUME", 1, Kind::Choice(&["PAUSE", "TIME"])),
    m(49, GEN, "FREQ ADJ", 3, signed(-25, 25, 1, "")),
    m(50, AM, "LCUT FREQ", 2, LCUT),
    m(51, AM, "LCUT SLOPE", 1, SLOPE),
    m(52, AM, "HCUT FREQ", 2, HCUT),
    m(53, AM, "HCUT SLOPE", 1, SLOPE),
    m(54, AM, "MIC GAIN", 4, Kind::MicGain),
    m(55, AM, "MIC SEL", 1, MICSEL),
    m(56, CW, "CW PITCH", 2, Kind::Steps { max_code: 75, off_at_zero: false, first: 300, step: 10, unit: "Hz" }),
    m(57, CW, "LCUT FREQ", 2, LCUT),
    m(58, CW, "LCUT SLOPE", 1, SLOPE),
    m(59, CW, "HCUT FREQ", 2, HCUT),
    m(60, CW, "HCUT SLOPE", 1, SLOPE),
    m(61, CW, "CW AUTO MODE", 1, Kind::Choice(&["OFF", "50 MHz", "ON"])),
    m(62, CW, "CW BFO", 1, Kind::Choice(&["USB", "LSB", "AUTO"])),
    m(63, CW, "CW BK-IN", 1, Kind::Choice(&["SEMI BREAK-IN", "FULL BREAK-IN"])),
    m(64, CW, "CW BK-IN DELAY", 4, DELAY),
    m(65, CW, "CW WAVE SHAPE", 1, Kind::Choice(&["1 ms", "2 ms", "4 ms", "6 ms"])),
    m(66, CW, "CW FREQ DISPLAY", 1, Kind::Choice(&["DIRECT FREQ", "PITCH OFFSET"])),
    m(67, CW, "PC KEYING", 1, Kind::Choice(&["OFF", "ON"])),
    m(68, CW, "QSK", 1, Kind::Choice(&["15 ms", "20 ms", "25 ms", "30 ms"])),
    m(69, DATA, "DATA MODE", 1, Kind::Choice(&["PSK", "OTHER"])),
    m(70, DATA, "PSK TONE", 1, Kind::Choice(&["1000 Hz", "1500 Hz", "2000 Hz"])),
    m(71, DATA, "OTHER DISP (SSB)", 5, signed(-3000, 3000, 10, "Hz")),
    m(72, DATA, "OTHER SHIFT (SSB)", 5, signed(-3000, 3000, 10, "Hz")),
    m(73, DATA, "LCUT FREQ", 2, LCUT),
    m(74, DATA, "LCUT SLOPE", 1, SLOPE),
    m(75, DATA, "HCUT FREQ", 2, HCUT),
    m(76, DATA, "HCUT SLOPE", 1, SLOPE),
    m(77, DATA, "DATA MIC GAIN", 4, Kind::MicGain),
    m(78, DATA, "DATA OUT LEVEL", 3, LEVEL),
    m(79, DATA, "DATA VOX GAIN", 3, LEVEL),
    m(80, DATA, "DATA VOX DELAY", 4, DELAY),
    m(81, FM, "LCUT FREQ", 2, LCUT),
    m(82, FM, "LCUT SLOPE", 1, SLOPE),
    m(83, FM, "HCUT FREQ", 2, HCUT),
    m(84, FM, "HCUT SLOPE", 1, SLOPE),
    m(85, FM, "MIC GAIN", 4, Kind::MicGain),
    m(86, FM, "MIC SEL", 1, MICSEL),
    m(87, FM, "RPT SHIFT (28 MHz)", 4, num(0, 1000, 10, "kHz")),
    m(88, FM, "RPT SHIFT (50 MHz)", 4, num(0, 4000, 10, "kHz")),
    m(89, FM, "TONE FREQ", 2, Kind::Tone),
    m(90, RTTY, "LCUT FREQ", 2, LCUT),
    m(91, RTTY, "LCUT SLOPE", 1, SLOPE),
    m(92, RTTY, "HCUT FREQ", 2, HCUT),
    m(93, RTTY, "HCUT SLOPE", 1, SLOPE),
    m(94, RTTY, "POLARITY-R", 1, NORREV),
    m(95, RTTY, "POLARITY-T", 1, NORREV),
    m(96, RTTY, "OUT LEVEL", 3, LEVEL),
    // The book numbers these from 1 by mistake (as the FTDX3000's).
    m(97, RTTY, "RTTY SHIFT", 1, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz", "850 Hz"])),
    m(98, RTTY, "MARK FREQ", 1, Kind::Choice(&["1275 Hz", "2125 Hz"])),
    m(99, SSB, "LCUT FREQ", 2, LCUT),
    m(100, SSB, "LCUT SLOPE", 1, SLOPE),
    m(101, SSB, "HCUT FREQ", 2, HCUT),
    m(102, SSB, "HCUT SLOPE", 1, SLOPE),
    m(103, SSB, "MIC SEL", 1, MICSEL),
    m(104, SSB, "TX BPF", 1, TX_BPF),
    m(105, SSB, "LSB RX CARRIER", 4, signed(-200, 200, 10, "Hz")),
    m(106, SSB, "USB RX CARRIER", 4, signed(-200, 200, 10, "Hz")),
    m(107, DSP, "APF WIDTH", 1, Kind::Choice(&["NARROW", "MEDIUM", "WIDE"])),
    m(108, DSP, "CONTOUR LEVEL", 3, signed(-40, 20, 1, "dB")),
    m(109, DSP, "CONTOUR WIDTH", 2, num(1, 11, 1, "")),
    m(110, DSP, "DNR LEVEL", 2, num(1, 15, 1, "")),
    m(111, DSP, "IF NOTCH WIDTH", 1, Kind::Choice(&["NARROW", "WIDE"])),
    m(112, DSP, "HF CW SHAPE", 1, SHAPE),
    m(113, DSP, "HF CW SLOPE", 1, SLOPE3),
    m(114, DSP, "6M CW SHAPE", 1, SHAPE),
    m(115, DSP, "6M CW SLOPE", 1, SLOPE3),
    m(116, DSP, "HF PSK SHAPE", 1, SHAPE),
    m(117, DSP, "HF PSK SLOPE", 1, SLOPE3),
    m(118, DSP, "HF FSK SHAPE", 1, SHAPE),
    m(119, DSP, "HF FSK SLOPE", 1, SLOPE3),
    m(120, DSP, "HF SSB SHAPE", 1, SHAPE),
    m(121, DSP, "HF SSB SLOPE", 1, SLOPE3),
    m(122, DSP, "6M SSB SHAPE", 1, SHAPE),
    m(123, DSP, "6M SSB SLOPE", 1, SLOPE3),
    m(124, SCOPE, "SCOPE MODE", 1, Kind::Choice(&["CENTER", "CENTER-WF", "FIX", "FIX-WF"])),
    m(125, SCOPE, "SCOPE SPEED", 1, Kind::Choice(&["FAST", "SLOW"])),
    m(126, SCOPE, "SCOPE AUTO TIME", 1, Kind::Choice(&["OFF", "3 s", "5 s", "10 s", "30 s", "60 s"])),
    m(127, SCOPE, "START DIAL SPEED", 1, Kind::Choice(&["0.5 kHz/s", "1 kHz/s", "2 kHz/s", "4 kHz/s", "8 kHz/s", "16 kHz/s"])),
    m(128, SCOPE, "SPAN FREQ", 2, SPAN),
    m(129, SCOPE, "FIX 1.8 MHz", 5, num(1800, 1999, 1, "kHz")),
    m(130, SCOPE, "FIX 1.8 MHz SPAN", 2, SPAN),
    m(131, SCOPE, "FIX 3.5 MHz", 5, num(3500, 3999, 1, "kHz")),
    m(132, SCOPE, "FIX 3.5 MHz SPAN", 2, SPAN),
    m(133, SCOPE, "FIX 5.0 MHz", 5, num(5250, 5499, 1, "kHz")),
    m(134, SCOPE, "FIX 5.0 MHz SPAN", 2, SPAN),
    m(135, SCOPE, "FIX 7.0 MHz", 5, num(7000, 7299, 1, "kHz")),
    m(136, SCOPE, "FIX 7.0 MHz SPAN", 2, SPAN),
    m(137, SCOPE, "FIX 10 MHz", 5, num(10100, 10149, 1, "kHz")),
    m(138, SCOPE, "FIX 10 MHz SPAN", 2, SPAN),
    m(139, SCOPE, "FIX 14 MHz", 5, num(14000, 14349, 1, "kHz")),
    m(140, SCOPE, "FIX 14 MHz SPAN", 2, SPAN),
    m(141, SCOPE, "FIX 18 MHz", 5, num(18000, 18199, 1, "kHz")),
    m(142, SCOPE, "FIX 18 MHz SPAN", 2, SPAN),
    m(143, SCOPE, "FIX 21 MHz", 5, num(21000, 21449, 1, "kHz")),
    m(144, SCOPE, "FIX 21 MHz SPAN", 2, SPAN),
    m(145, SCOPE, "FIX 24 MHz", 5, num(24800, 24989, 1, "kHz")),
    m(146, SCOPE, "FIX 24 MHz SPAN", 2, SPAN),
    m(147, SCOPE, "FIX 28 MHz", 5, num(28000, 29699, 1, "kHz")),
    m(148, SCOPE, "FIX 28 MHz SPAN", 2, SPAN),
    m(149, SCOPE, "FIX 50 MHz", 5, num(50000, 53999, 1, "kHz")),
    m(150, SCOPE, "FIX 50 MHz SPAN", 2, SPAN),
    m(151, TUNING, "CW DIAL STEP", 1, DIAL_STEP),
    m(152, TUNING, "DATA DIAL STEP", 1, DIAL_STEP),
    m(153, TUNING, "AM/FM DIAL STEP", 1, Kind::Choice(&["10 Hz", "100 Hz"])),
    m(154, TUNING, "RTTY DIAL STEP", 1, DIAL_STEP),
    m(155, TUNING, "SSB DIAL STEP", 1, DIAL_STEP),
    m(156, TUNING, "AM CH STEP", 1, Kind::Choice(&["2.5 kHz", "5 kHz", "9 kHz", "10 kHz", "12.5 kHz"])),
    m(157, TUNING, "FM CH STEP", 1, Kind::Choice(&["5 kHz", "6.25 kHz", "10 kHz", "12.5 kHz", "20 kHz", "25 kHz"])),
    m(158, TUNING, "1MHz/100kHz SELECT", 1, Kind::Choice(&["1 MHz", "100 kHz"])),
    m(159, TXA, "PRMTRC EQ1 FREQ", 2, EQ1_FREQ),
    m(160, TXA, "PRMTRC EQ1 LEVEL", 3, EQ_LEVEL),
    m(161, TXA, "PRMTRC EQ1 BWTH", 2, EQ_BW),
    m(162, TXA, "PRMTRC EQ2 FREQ", 2, EQ2_FREQ),
    m(163, TXA, "PRMTRC EQ2 LEVEL", 3, EQ_LEVEL),
    m(164, TXA, "PRMTRC EQ2 BWTH", 2, EQ_BW),
    m(165, TXA, "PRMTRC EQ3 FREQ", 2, EQ3_FREQ),
    m(166, TXA, "PRMTRC EQ3 LEVEL", 3, EQ_LEVEL),
    m(167, TXA, "PRMTRC EQ3 BWTH", 2, EQ_BW),
    m(168, TXA, "P-PRMTRC EQ1 FREQ", 2, EQ1_FREQ),
    m(169, TXA, "P-PRMTRC EQ1 LEVEL", 3, EQ_LEVEL),
    m(170, TXA, "P-PRMTRC EQ1 BWTH", 2, EQ_BW),
    m(171, TXA, "P-PRMTRC EQ2 FREQ", 2, EQ2_FREQ),
    m(172, TXA, "P-PRMTRC EQ2 LEVEL", 3, EQ_LEVEL),
    m(173, TXA, "P-PRMTRC EQ2 BWTH", 2, EQ_BW),
    m(174, TXA, "P-PRMTRC EQ3 FREQ", 2, EQ3_FREQ),
    m(175, TXA, "P-PRMTRC EQ3 LEVEL", 3, EQ_LEVEL),
    m(176, TXA, "P-PRMTRC EQ3 BWTH", 2, EQ_BW),
    m(177, TXG, "TX PWR/PROC CONTROL", 1, Kind::Choice(&["TX PWR", "PROC"])),
    m(178, TXG, "EXT AMP TUNING PWR", 1, Kind::Choice(&["10 W", "20 W", "50 W", "100 W"])),
    m(179, TXG, "TUNER SELECT", 1, Kind::Choice(&["INTERNAL", "EXTERNAL"])),
    m(180, TXG, "VOX SELECT", 1, Kind::Choice(&["MIC", "DATA"])),
    m(181, TXG, "VOX GAIN", 3, LEVEL),
    m(182, TXG, "VOX DELAY", 4, DELAY),
    m(183, TXG, "ANTI VOX GAIN", 3, LEVEL),
    m(184, TXG, "EMERGENCY FREQ TX", 1, ENABLE),
    m(185, AFS, "FFT DISPLAY MODE", 1, Kind::Choice(&["SPECTRUM", "WATERFALL", "SPECTRUM-WF"])),
    m(186, AFS, "FFT ATT", 1, Kind::Choice(&["0 dB", "10 dB", "20 dB"])),
    m(187, DEC, "CW DECODE BW", 1, Kind::Choice(&["25 Hz", "50 Hz", "100 Hz", "250 Hz"])),
    m(188, EDR, "RX USOS", 1, ENABLE),
    m(189, EDR, "TX USOS", 1, ENABLE),
    m(190, EDR, "RX NEW LINE CODE", 1, Kind::Choice(&["CR, LF, CR+LF", "CR+LF"])),
    m(191, EDR, "TX AUTO CR+LF", 1, ENABLE),
    m(192, EDR, "TX DIDDLE", 1, Kind::Choice(&["OFF", "BLANK", "LTRS"])),
    m(193, EDR, "BAUDOT CODE", 1, Kind::Choice(&["CCITT", "US"])),
    m(194, EDP, "PSK MODE", 1, Kind::Choice(&["BPSK", "QPSK"])),
    m(195, EDP, "DECODE AFC RANGE", 1, Kind::Choice(&["±8 Hz", "±15 Hz", "±30 Hz"])),
    m(196, EDP, "QPSK POLARITY REV", 1, Kind::Choice(&["RX-N TX-N", "RX-R TX-N", "RX-N TX-R", "RX-R TX-R"])),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_is_complete() {
        assert_eq!(MENU.len(), 196);
        assert_eq!(FTDX1200.menu_item(40).unwrap().name, "CAT TIME OUT TIMER");
        assert_eq!(FTDX1200.menu_item(69).unwrap().name, "DATA MODE");
        assert_eq!(crate::model::by_id_answer("ID0582;").map(|m| m.key), Some("ftdx1200-fft1"));
        assert_eq!(crate::model::by_id_answer("ID0583;").map(|m| m.key), Some("ftdx1200"));
    }

    #[test]
    fn frames_from_the_manual() {
        let mut c = Ctx::new(&FTDX1200);
        for f in [
            "FA14250000;", "FA;", "IS0+1000;", "MD0C;", "SH016;", "RF03;", "AN02;", "PC100;", "PR01;", "SF7;", "EX0563;",
            "EX040;", "EX1280;", "EX071-0100;", "IF00114250000+000000200000;", "OI;", "VF001281;",
        ] {
            assert_eq!(down(&mut c, f), vec![f], "{f}");
            assert_eq!(up(&mut c, f), vec![f], "{f}");
        }
        assert_eq!(up(&mut c, "ID0583;"), vec!["ID0583;"]);
        assert_eq!(up(&mut c, "RF06;"), vec!["RF06;"]);
        assert!(down(&mut c, "RF04;").is_empty());
        assert_eq!(down(&mut c, "RF0;"), vec!["RF0;"]);
        assert!(down(&mut c, "EX197;").is_empty());
        assert_eq!(down(&mut c, "DA101010;"), vec!["DA000210;"]);
        assert_eq!(down(&mut c, "DA000115;"), vec!["DA000115;"]);
        assert_eq!(up(&mut c, "DA000207;"), vec!["DA000207;"]);
        // EM/EN only with the FFT-1.
        assert!(down(&mut c, "EM01CQ;").is_empty());
        let mut f = Ctx::new(&FTDX1200_FFT1);
        assert_eq!(down(&mut f, "EM01CQ;"), vec!["EM01CQ;"]);
        assert!(up(&mut c, "DT0123456;").is_empty());
        assert!(up(&mut c, "ZI;").is_empty());
    }

    #[test]
    fn round_trip() {
        let mut c = Ctx::new(&FTDX1200);
        for f in [
            "FA07074000;", "FB55000000;", "MD03;", "AG0128;", "MG050;", "PC050;", "SH025;", "NA01;", "RF03;", "PA02;",
            "GT04;", "NL0100;", "KP75;", "DA000215;", "EX0560075;", "EX005W1AW        ;", "EX1287;", "AN01;", "FR4;",
            "IF00114074000+012010200000;", "KM1CQ};", "PR11;",
        ] {
            let native = down(&mut c, f);
            let back: Vec<String> = native.iter().flat_map(|n| up(&mut c, n)).collect();
            assert_eq!(back, vec![f], "{f} -> {native:?}");
        }
    }

    #[test]
    fn widths_are_table_5() {
        assert_eq!(FTDX1200.width_hz(WidthClass::Ssb, false, 14), Some(2400));
        assert_eq!(FTDX1200.width_hz(WidthClass::Cw, true, 1), Some(50));
    }
}
