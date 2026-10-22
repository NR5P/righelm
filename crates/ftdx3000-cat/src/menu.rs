//! The 196 `EX` menu items (CAT reference Tables 2-4).
//!
//! The reference book does not state the digit count of every parameter, so
//! each item carries a best-guess `width`. The engine always reads a menu item
//! before writing it and re-uses the width the radio itself answered with.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// Codes 0, 1, 2, ... map to these labels.
    Choice(&'static [&'static str]),
    /// Explicit `(code, label)` pairs.
    Coded(&'static [(i32, &'static str)]),
    /// The parameter is the number itself. Shown as `value / div` with `unit`.
    Num { min: i32, max: i32, step: i32, unit: &'static str, signed: bool, div: i32 },
    /// The parameter is a step index: code 0 is OFF when `off_at_zero`,
    /// otherwise the first code means `first` and each code adds `step`.
    Steps { max_code: i32, off_at_zero: bool, first: i32, step: i32, unit: &'static str },
    /// `1000` = MCVR (front-panel MIC knob), `0000`-`0100` = fixed gain.
    MicGain,
    /// CTCSS tone number (Table 1).
    Tone,
    /// Free text (e.g. the opening call sign).
    Text { max: usize },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuItem {
    pub num: u16,
    pub group: &'static str,
    pub name: &'static str,
    /// Expected number of parameter characters (including any sign).
    pub width: u8,
    pub kind: Kind,
}

/// A decoded menu value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(i32),
    Text(String),
}

const fn m(num: u16, group: &'static str, name: &'static str, width: u8, kind: Kind) -> MenuItem {
    MenuItem { num, group, name, width, kind }
}

const fn num(min: i32, max: i32, step: i32, unit: &'static str) -> Kind {
    Kind::Num { min, max, step, unit, signed: false, div: 1 }
}

const fn signed(min: i32, max: i32, step: i32, unit: &'static str) -> Kind {
    Kind::Num { min, max, step, unit, signed: true, div: 1 }
}

const LCUT: Kind = Kind::Steps { max_code: 19, off_at_zero: true, first: 100, step: 50, unit: "Hz" };
const HCUT: Kind = Kind::Steps { max_code: 67, off_at_zero: true, first: 700, step: 50, unit: "Hz" };
const SLOPE: Kind = Kind::Choice(&["6 dB/oct", "18 dB/oct"]);
const ENABLE: Kind = Kind::Choice(&["DISABLE", "ENABLE"]);
const NORREV: Kind = Kind::Choice(&["NOR", "REV"]);
const LEVEL: Kind = num(0, 100, 1, "");
const MICSEL: Kind = Kind::Choice(&["FRONT", "DATA", "USB"]);
const KEYER_TYPE: Kind = Kind::Choice(&["OFF", "BUG", "ELEKEY", "ACS"]);
const KEYER_DIR: Kind = Kind::Choice(&["NORMAL", "REVERSE"]);
const CW_MEM: Kind = Kind::Choice(&["TEXT", "MESSAGE"]);
const SHAPE: Kind = Kind::Choice(&["SOFT", "SHARP"]);
const SLOPE3: Kind = Kind::Choice(&["STEEP", "MEDIUM", "GENTLE"]);
const DELAY: Kind = num(30, 3000, 10, "ms");
const AGC_DELAY: Kind = num(20, 4000, 20, "ms");
const SPAN: Kind = Kind::Coded(&[
    (2, "20 kHz"),
    (3, "50 kHz"),
    (4, "100 kHz"),
    (5, "200 kHz"),
    (6, "500 kHz"),
    (7, "1000 kHz"),
]);
const DIAL_STEP: Kind = Kind::Choice(&["1 Hz", "5 Hz", "10 Hz"]);
const EQ1_FREQ: Kind = Kind::Steps { max_code: 7, off_at_zero: true, first: 100, step: 100, unit: "Hz" };
const EQ2_FREQ: Kind = Kind::Steps { max_code: 9, off_at_zero: true, first: 700, step: 100, unit: "Hz" };
const EQ3_FREQ: Kind = Kind::Steps { max_code: 18, off_at_zero: true, first: 1500, step: 100, unit: "Hz" };
const EQ_LEVEL: Kind = signed(-20, 10, 1, "dB");
const EQ_BW: Kind = num(1, 10, 1, "");
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

pub const MENU: &[MenuItem] = &[
    m(1, AGC, "FAST DELAY", 4, AGC_DELAY),
    m(2, AGC, "MID DELAY", 4, AGC_DELAY),
    m(3, AGC, "SLOW DELAY", 4, AGC_DELAY),
    m(4, AGC, "AGC SLOPE", 1, Kind::Choice(&["NORMAL", "SLOPE"])),
    m(5, DISP, "MY CALL", 12, Kind::Text { max: 12 }),
    m(6, DISP, "MY CALL TIME", 2, num(0, 5, 1, "s")),
    m(7, DISP, "DIMMER VFO", 2, num(0, 15, 1, "")),
    // The reference book says "0: 1, 1: 2"; the radio reports a 0-15 level.
    m(8, DISP, "DIMMER BACKLIGHT", 2, num(0, 15, 1, "")),
    m(9, DISP, "DIMMER TFT", 2, num(0, 15, 1, "")),
    m(10, DISP, "BAR DISPLAY SELECT", 1, Kind::Choice(&["CLAR", "CW TUNE", "uTUNE"])),
    m(11, DISP, "METER TYPE SELECT", 1, Kind::Choice(&["ANALOG", "BAR"])),
    m(12, DISP, "BAR MTR PEAK HOLD", 1, Kind::Choice(&["OFF", "0.5 s", "1.0 s", "2.0 s"])),
    m(13, DISP, "ROTATOR START UP", 1, Kind::Choice(&["0°", "90°", "180°", "270°"])),
    m(14, DISP, "ROTATOR OFFSET ADJ (minus)", 2, num(0, 30, 2, "°")),
    m(15, DVS, "RX OUT LEVEL", 3, LEVEL),
    m(16, DVS, "TX OUT LEVEL", 3, LEVEL),
    m(17, KEYER, "F KEYER TYPE", 1, KEYER_TYPE),
    m(18, KEYER, "F CW KEYER", 1, KEYER_DIR),
    m(19, KEYER, "R KEYER TYPE", 1, KEYER_TYPE),
    m(20, KEYER, "R CW KEYER", 1, KEYER_DIR),
    m(21, KEYER, "ELEKEY TYPE", 1, Kind::Choice(&["ELEKEY-A", "ELEKEY-B"])),
    m(22, KEYER, "CW WEIGHT", 2, Kind::Num { min: 25, max: 45, step: 1, unit: ":1", signed: false, div: 10 }),
    m(23, KEYER, "BEACON TIME (0=OFF)", 3, num(0, 690, 1, "s")),
    m(24, KEYER, "NUMBER STYLE", 1, Kind::Choice(&["1290", "AUNO", "AUNT", "A2NO", "A2NT", "12NO", "12NT"])),
    m(25, KEYER, "CONTEST NUMBER", 4, num(0, 9999, 1, "")),
    m(26, KEYER, "CW MEMORY 1", 1, CW_MEM),
    m(27, KEYER, "CW MEMORY 2", 1, CW_MEM),
    m(28, KEYER, "CW MEMORY 3", 1, CW_MEM),
    m(29, KEYER, "CW MEMORY 4", 1, CW_MEM),
    m(30, KEYER, "CW MEMORY 5", 1, CW_MEM),
    m(31, GEN, "ANT SELECT", 1, Kind::Choice(&["BAND", "STACK"])),
    m(32, GEN, "ANT3 SETTING", 1, Kind::Choice(&["TRX", "R3/1", "R3/2"])),
    m(33, GEN, "NB LEVEL", 3, LEVEL),
    m(34, GEN, "BEEP LEVEL", 3, LEVEL),
    m(35, GEN, "MONITOR LEVEL", 3, LEVEL),
    m(36, GEN, "RF/SQL VR", 1, Kind::Choice(&["RF", "SQL"])),
    m(37, GEN, "CAT SELECT", 1, Kind::Choice(&["RS232C", "USB"])),
    m(38, GEN, "CAT RATE", 1, Kind::Choice(&["4800 bps", "9600 bps", "19200 bps", "38400 bps"])),
    m(39, GEN, "CAT TIME OUT TIMER", 1, Kind::Choice(&["10 ms", "100 ms", "1000 ms", "3000 ms"])),
    m(40, GEN, "CAT RTS", 1, ENABLE),
    m(41, GEN, "MEM GROUP", 1, ENABLE),
    m(42, GEN, "QUICK SPLIT FREQ", 3, signed(-20, 20, 1, "kHz")),
    m(43, GEN, "TX TIME OUT TIMER (0=OFF)", 2, num(0, 30, 1, "min")),
    m(44, GEN, "uTUNE DIAL STEP", 1, Kind::Choice(&["DIAL STEP-2", "DIAL STEP-1"])),
    m(45, GEN, "MIC SCAN", 1, ENABLE),
    m(46, GEN, "SCAN RESUME", 1, Kind::Choice(&["PAUSE", "TIME"])),
    m(47, GEN, "FREQ ADJ", 3, signed(-25, 25, 1, "")),
    m(48, AM, "LCUT FREQ", 2, LCUT),
    m(49, AM, "LCUT SLOPE", 1, SLOPE),
    m(50, AM, "HCUT FREQ", 2, HCUT),
    m(51, AM, "HCUT SLOPE", 1, SLOPE),
    m(52, AM, "MIC GAIN", 4, Kind::MicGain),
    m(53, AM, "MIC SEL", 1, MICSEL),
    // Same coding as the KP command: 0-75 = 300-1050 Hz in 10 Hz steps.
    m(54, CW, "CW PITCH", 3, Kind::Steps { max_code: 75, off_at_zero: false, first: 300, step: 10, unit: "Hz" }),
    m(55, CW, "LCUT FREQ", 2, LCUT),
    m(56, CW, "LCUT SLOPE", 1, SLOPE),
    m(57, CW, "HCUT FREQ", 2, HCUT),
    m(58, CW, "HCUT SLOPE", 1, SLOPE),
    m(59, CW, "CW AUTO MODE", 1, Kind::Choice(&["OFF", "50 MHz", "ON"])),
    m(60, CW, "CW BFO", 1, Kind::Choice(&["USB", "LSB", "AUTO"])),
    m(61, CW, "CW BK-IN", 1, Kind::Choice(&["SEMI BREAK-IN", "FULL BREAK-IN"])),
    m(62, CW, "CW BK-IN DELAY", 4, DELAY),
    m(63, CW, "CW WAVE SHAPE", 1, Kind::Choice(&["1 ms", "2 ms", "4 ms", "6 ms"])),
    m(64, CW, "CW FREQ DISPLAY", 1, Kind::Choice(&["DIRECT FREQ", "PITCH OFFSET"])),
    m(65, CW, "PC KEYING", 1, Kind::Choice(&["OFF", "DAKY", "RTS", "DTR"])),
    m(66, CW, "QSK", 1, Kind::Choice(&["15 ms", "20 ms", "25 ms", "30 ms"])),
    m(67, DATA, "DATA MODE", 1, Kind::Choice(&["PSK", "OTHER"])),
    m(68, DATA, "DATA TONE", 1, Kind::Choice(&["1000 Hz", "1500 Hz", "2000 Hz"])),
    m(69, DATA, "OTHER DISP (SSB)", 5, signed(-3000, 3000, 10, "Hz")),
    m(70, DATA, "OTHER SHIFT (SSB)", 5, signed(-3000, 3000, 10, "Hz")),
    m(71, DATA, "LCUT FREQ", 2, LCUT),
    m(72, DATA, "LCUT SLOPE", 1, SLOPE),
    m(73, DATA, "HCUT FREQ", 2, HCUT),
    m(74, DATA, "HCUT SLOPE", 1, SLOPE),
    m(75, DATA, "DATA IN SELECT", 1, Kind::Choice(&["DATA", "USB"])),
    m(76, DATA, "DATA MIC GAIN", 4, Kind::MicGain),
    m(77, DATA, "DATA OUT LEVEL", 3, LEVEL),
    m(78, DATA, "DATA VOX GAIN", 3, LEVEL),
    m(79, DATA, "DATA VOX DELAY", 4, DELAY),
    m(80, FM, "LCUT FREQ", 2, LCUT),
    m(81, FM, "LCUT SLOPE", 1, SLOPE),
    m(82, FM, "HCUT FREQ", 2, HCUT),
    m(83, FM, "HCUT SLOPE", 1, SLOPE),
    m(84, FM, "MIC GAIN", 4, Kind::MicGain),
    m(85, FM, "MIC SEL", 1, MICSEL),
    m(86, FM, "RPT SHIFT (28 MHz)", 4, num(0, 1000, 10, "kHz")),
    m(87, FM, "RPT SHIFT (50 MHz)", 4, num(0, 4000, 10, "kHz")),
    m(88, FM, "TONE FREQ", 2, Kind::Tone),
    m(89, RTTY, "LCUT FREQ", 2, LCUT),
    m(90, RTTY, "LCUT SLOPE", 1, SLOPE),
    m(91, RTTY, "HCUT FREQ", 2, HCUT),
    m(92, RTTY, "HCUT SLOPE", 1, SLOPE),
    m(93, RTTY, "SHIFT PORT", 1, Kind::Choice(&["REAR", "USB"])),
    m(94, RTTY, "POLARITY-R", 1, NORREV),
    m(95, RTTY, "POLARITY-T", 1, NORREV),
    m(96, RTTY, "OUT LEVEL", 3, LEVEL),
    m(97, RTTY, "RTTY SHIFT", 1, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz", "850 Hz"])),
    m(98, RTTY, "MARK FREQ", 1, Kind::Choice(&["1275 Hz", "2125 Hz"])),
    m(99, SSB, "LCUT FREQ", 2, LCUT),
    m(100, SSB, "LCUT SLOPE", 1, SLOPE),
    m(101, SSB, "HCUT FREQ", 2, HCUT),
    m(102, SSB, "HCUT SLOPE", 1, SLOPE),
    m(103, SSB, "MIC SEL", 1, MICSEL),
    m(
        104,
        SSB,
        "TX BPF",
        1,
        Kind::Choice(&["50-3000 Hz", "100-2900 Hz", "200-2800 Hz", "300-2700 Hz", "400-2600 Hz", "3000WB"]),
    ),
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
    // The radio has more scope modes than the reference book lists (it reports 5).
    m(124, SCOPE, "SCOPE MODE", 1, num(0, 9, 1, "")),
    m(125, SCOPE, "SCOPE SPEED", 1, Kind::Choice(&["FAST", "SLOW"])),
    m(126, SCOPE, "SCOPE AUTO TIME", 1, Kind::Choice(&["OFF", "3 s", "5 s", "10 s", "30 s", "60 s"])),
    m(127, SCOPE, "START DIAL SPEED", 1, Kind::Choice(&["0.5k", "1k", "2k", "4k", "8k", "16k"])),
    m(128, SCOPE, "SPAN FREQ", 2, Kind::Choice(&["20k", "50k", "100k", "200k", "500k", "1000k"])),
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
    m(
        157,
        TUNING,
        "FM CH STEP",
        1,
        Kind::Choice(&["2.5 kHz", "6.25 kHz", "10 kHz", "12.5 kHz", "20 kHz", "25 kHz"]),
    ),
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

pub fn item(num: u16) -> Option<&'static MenuItem> {
    MENU.get(num.checked_sub(1)? as usize).filter(|m| m.num == num)
}

/// Menu groups in front-panel order.
pub fn groups() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for m in MENU {
        if !out.contains(&m.group) {
            out.push(m.group);
        }
    }
    out
}

/// The Read command for a menu item: `EX038;`.
pub fn read_cmd(num: u16) -> String {
    format!("EX{num:03};")
}

impl MenuItem {
    /// Choices for list-like kinds, as `(code, label)`.
    pub fn options(&self) -> Vec<(i32, String)> {
        match self.kind {
            Kind::Choice(labels) => labels.iter().enumerate().map(|(i, l)| (i as i32, l.to_string())).collect(),
            Kind::Coded(pairs) => pairs.iter().map(|(c, l)| (*c, l.to_string())).collect(),
            Kind::Steps { max_code, off_at_zero, first, step, unit } => (0..=max_code)
                .map(|code| {
                    let label = if off_at_zero && code == 0 {
                        "OFF".to_string()
                    } else {
                        let n = if off_at_zero { code - 1 } else { code };
                        format!("{} {unit}", first + n * step)
                    };
                    (code, label)
                })
                .collect(),
            Kind::Tone => crate::types::CTCSS_DHZ
                .iter()
                .enumerate()
                .map(|(i, t)| (i as i32, format!("{}.{} Hz", t / 10, t % 10)))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Parse the parameter text of an `EX` answer (everything after `EXnnn`).
    pub fn decode(&self, raw: &str) -> Option<Value> {
        if let Kind::Text { .. } = self.kind {
            return Some(Value::Text(raw.trim_end().to_string()));
        }
        let v: i32 = raw.trim().trim_start_matches('+').parse().ok()?;
        let ok = match self.kind {
            Kind::Choice(l) => (0..l.len() as i32).contains(&v),
            Kind::Coded(p) => p.iter().any(|(c, _)| *c == v),
            Kind::Num { min, max, .. } => (min..=max).contains(&v),
            Kind::Steps { max_code, .. } => (0..=max_code).contains(&v),
            Kind::MicGain => (0..=100).contains(&v) || v == 1000,
            Kind::Tone => (0..50).contains(&v),
            Kind::Text { .. } => unreachable!(),
        };
        ok.then_some(Value::Int(v))
    }

    /// Format a value as parameter text. `width` is the width the radio used
    /// when it last reported this item (falls back to the table's guess).
    pub fn encode(&self, value: &Value, width: Option<usize>) -> String {
        let width = width.unwrap_or(self.width as usize);
        match (value, self.kind) {
            (Value::Text(t), Kind::Text { max }) => {
                // The radio stores and reports text space-padded to `max`.
                let t: String = t.chars().filter(|c| c.is_ascii_graphic() || *c == ' ').filter(|c| *c != ';').take(max).collect();
                format!("{t:<max$}")
            }
            (Value::Int(v), Kind::Num { signed: true, .. }) => {
                let sign = if *v < 0 { '-' } else { '+' };
                format!("{sign}{:0w$}", v.abs(), w = width.saturating_sub(1))
            }
            (Value::Int(v), _) => format!("{:0width$}", v.max(&0)),
            (Value::Text(t), _) => t.clone(),
        }
    }

    /// Human-readable rendering of a raw parameter.
    pub fn display(&self, raw: &str) -> String {
        match self.decode(raw) {
            Some(Value::Text(t)) => t,
            Some(Value::Int(v)) => match self.kind {
                Kind::Num { unit, signed, div, .. } => {
                    let sign = if signed && v > 0 { "+" } else { "" };
                    if div > 1 {
                        format!("{sign}{:.1}{unit}", v as f32 / div as f32)
                    } else if unit.is_empty() {
                        format!("{sign}{v}")
                    } else {
                        format!("{sign}{v} {unit}")
                    }
                }
                Kind::MicGain => {
                    if v == 1000 {
                        "MCVR".to_string()
                    } else {
                        format!("FIX {v}")
                    }
                }
                _ => self.options().into_iter().find(|(c, _)| *c == v).map(|(_, l)| l).unwrap_or_else(|| raw.to_string()),
            },
            None => format!("raw \"{raw}\""),
        }
    }

    /// Build the complete Set command.
    pub fn set_cmd(&self, value: &Value, width: Option<usize>) -> String {
        format!("EX{:03}{};", self.num, self.encode(value, width))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_complete_and_ordered() {
        assert_eq!(MENU.len(), 196);
        for (i, m) in MENU.iter().enumerate() {
            assert_eq!(m.num as usize, i + 1, "{}", m.name);
        }
        assert_eq!(item(38).unwrap().name, "CAT RATE");
        assert!(item(0).is_none());
        assert!(item(197).is_none());
    }

    #[test]
    fn decode_encode() {
        let rate = item(38).unwrap();
        assert_eq!(rate.decode("3"), Some(Value::Int(3)));
        assert_eq!(rate.display("3"), "38400 bps");
        assert_eq!(rate.set_cmd(&Value::Int(3), Some(1)), "EX0383;");

        let carrier = item(105).unwrap();
        assert_eq!(carrier.decode("-050"), Some(Value::Int(-50)));
        assert_eq!(carrier.encode(&Value::Int(50), Some(4)), "+050");
        assert_eq!(carrier.display("+050"), "+50 Hz");

        let hcut = item(101).unwrap();
        assert_eq!(hcut.display("67"), "4000 Hz");
        assert_eq!(hcut.display("00"), "OFF");

        let mic = item(52).unwrap();
        assert_eq!(mic.display("1000"), "MCVR");
        assert_eq!(mic.encode(&Value::Int(50), None), "0050");

        let call = item(5).unwrap();
        assert_eq!(call.set_cmd(&Value::Text("W1AW;".into()), None), "EX005W1AW        ;");
        assert_eq!(call.display("FTDX3000    "), "FTDX3000");

        assert_eq!(item(22).unwrap().display("30"), "3.0:1");
        assert_eq!(item(1).unwrap().display("xyz"), "raw \"xyz\"");
    }
}
