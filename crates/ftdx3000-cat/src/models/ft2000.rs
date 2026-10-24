//! Yaesu FT-2000 and FT-2000D: the generation before the FTDX5000, with a
//! sub receiver.
//!
//! From the *FT-2000 Series CAT Operation Reference Book*. P1 of several
//! receiver commands picks the main (VFO-A, `0`) or sub (VFO-B, `1`)
//! receiver; RigHelm's are the main's, and the sub's own answers are
//! dropped. Where it differs from the FTDX3000:
//!
//! * Menu: 149 items of its own (`EX001`-`EX149`).
//! * `MG`, `PL`, `VG` and the monitor level `ML1` are 0-255 (RigHelm
//!   0-100); `NL` 0-255; `PC` in watts (5-100, FT-2000D 5-200).
//! * `KP` is 00-15 in 50 Hz steps; `LK`/`FS` are one switch, not per VFO.
//! * `PR` is the speech processor only (no mic EQ switch).
//! * `FR` 0-3: main RX / main mute / main + sub RX / main mute + sub RX.
//! * `SH` is the WIDTH knob's position, 00-31 (16 = centre), not a table.
//!   The book gives no widths; the ones here are Hamlib's (rigs/yaesu,
//!   newcat.c), interpolated between its points.
//! * `DA`: VFD (00-07) and meter (00-15) dimmers; `SF` has BAND, A/B and
//!   VFO-B keys of its own; `CO` has no APF frequency.
//! * `RF`: AUTO / 15 / 6 / 3 kHz only. Two antennas plus an RX antenna.
//! * No `BA`, `EM`, `EN`; no AM-N mode (`MD0D`).
//! * `ID`: 0251 FT-2000, 0252 FT-2000D (200 W), listed as two radios.

use crate::dialect::text::{Ctx, TextSpec, menu_down, menu_up, parts, read_prefix};
use crate::menu::{Kind, MenuItem, num, signed};
use crate::model::{Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::models::ftdx5000::{
    AGC_DELAY, AGC_HOLD, CW_WEIGHT, DELAY, ENABLE, EQ_BW, EQ1_FREQ, EQ2_FREQ, EQ3_FREQ, LEVEL, MODES, TX_BPF,
    ab_switch_down, ab_switch_up, encoder, fr_main_sub_down, fr_main_sub_up, level, m, pitch50_down, pitch50_up, sf_basic,
    upper,
};
use crate::types::{self, WidthClass};

pub static SPEC: TextSpec = TextSpec { down, up, answer_prefix: read_prefix };

/// Commands whose P1 picks the main (0) or sub (1) receiver.
const MAIN_SUB: &[&str] = &["AG", "BD", "BU", "CN", "CT", "GT", "MD", "NA", "NB", "NL", "NR", "OS", "RG", "SM", "SQ"];

fn down(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !cx.model.has(&code) {
        return Vec::new();
    }
    match code.as_str() {
        "EX" => menu_down(cx, f, 3),
        "MG" | "PL" | "VG" => level(f, 0, 100, 255),
        "ML" if p.starts_with('1') => level(f, 1, 100, 255),
        "KP" => pitch50_down(f),
        "LK" | "FS" => ab_switch_down(f),
        "PR" => match p {
            "0" => vec!["PR;".into()],
            "00" | "01" => vec![format!("PR{};", &p[1..])],
            _ => Vec::new(),
        },
        "FR" => fr_main_sub_down(f),
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
    if MAIN_SUB.contains(&code.as_str()) && p.starts_with('1') {
        return Vec::new();
    }
    match code.as_str() {
        "EX" => menu_up(cx, f, 3),
        "MG" | "PL" | "VG" => level(f, 0, 255, 100),
        "ML" if p.starts_with('1') => level(f, 1, 255, 100),
        "KP" => pitch50_up(f),
        "LK" | "FS" => ab_switch_up(f),
        "PR" => match p {
            "" => vec!["PR0;".into()],
            "0" | "1" => vec![format!("PR0{p};")],
            _ => Vec::new(),
        },
        "FR" => fr_main_sub_up(f),
        "DA" => dimmer_up(f),
        "SF" => sf_basic(f),
        "ED" | "EU" => encoder(f),
        _ => vec![upper(f)],
    }
}

/// `DA`: RigHelm's first two dimmers are the VFD (00-07) and the meter.
fn dimmer_down(f: &str) -> Vec<String> {
    match parts(f).map(|(_, p)| p) {
        Some("") => vec!["DA;".into()],
        Some(p) if p.len() == 6 => {
            let Ok(vfd) = p[..2].parse::<u8>() else { return Vec::new() };
            vec![format!("DA{:02}{};", vfd.min(7), &p[2..4])]
        }
        _ => Vec::new(),
    }
}

fn dimmer_up(f: &str) -> Vec<String> {
    match parts(f).map(|(_, p)| p) {
        Some("") => vec!["DA;".into()],
        Some(p) if p.len() == 4 => vec![format!("DA{p}00;")],
        _ => Vec::new(),
    }
}

/// WIDTH knob positions 00-31 with the widths Hamlib's driver gives them
/// (newcat_get_rx_bandwidth), straight lines between its points. Positions
/// outside them are left out.
fn widths(class: WidthClass, _narrow: bool) -> Vec<(u8, u32)> {
    let points: &[(u8, u32)] = match class {
        WidthClass::Ssb => &[(8, 1800), (16, 2400), (25, 3000), (31, 4000)],
        WidthClass::Cw => &[(4, 200), (6, 500), (16, 2400), (31, 3000)],
        WidthClass::Rtty => &[(8, 300), (16, 500), (31, 2400)],
        WidthClass::Fixed => return Vec::new(),
    };
    let mut out = Vec::new();
    for w in points.windows(2) {
        let ((i0, h0), (i1, h1)) = (w[0], w[1]);
        for i in i0..i1 {
            let hz = h0 as f32 + (h1 as f32 - h0 as f32) * (i - i0) as f32 / (i1 - i0) as f32;
            out.push((i, (hz / 10.0).round() as u32 * 10));
        }
    }
    out.push(points[points.len() - 1]);
    out
}

/// Every RigHelm command but BA, EM and EN.
const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AM", "AN", "BC", "BD", "BI", "BP", "BS", "BU", "BY", "CH", "CN", "CO00", "CO01", "CS", "CT", "DA",
    "DN", "ED", "EK", "EU", "EX", "FA", "FB", "FR", "FS", "FT", "GT", "ID", "IF", "IS", "KM", "KP", "KR", "KS", "KY",
    "LK", "LM", "MA", "MC", "MD", "MG", "ML", "MR", "MS", "MW", "MX", "NA", "NB", "NL", "NR", "OI", "OS", "PA", "PB",
    "PC", "PL", "PR0", "PS", "QI", "QR", "QS", "RA", "RC", "RD", "RF", "RG", "RI", "RL", "RM", "RO", "RS", "RT", "RU",
    "SC", "SD", "SF", "SH", "SM", "SQ", "SV", "TS", "TX", "UL", "UP", "VD", "VF", "VG", "VM", "VS", "VX", "XT",
];

pub static FT2000: RadioModel = RadioModel {
    key: "ft2000",
    name: "FT-2000",
    maker: Maker::Yaesu,
    family: Family::Yaesu,
    proto: Proto::Text(&SPEC),
    id: "0251",
    civ_addr: 0,
    hamlib: 1029,
    freq_min: 30_000,
    freq_max: 60_000_000,
    bands: types::BANDS,
    modes: MODES,
    menu: MENU,
    roles: Roles {
        // AM, FM, SSB MIC SELECT: 0 MIC JACK, 1 DATA JACK (where a computer
        // interface brings its audio; the radio has no USB port).
        mic_sel: &[53, 75, 84],
        mic_front: 0,
        mic_usb: 1,
        pc_keying: None,
        cat_tot: Some((29, "1")),
        cat_items: &[28, 29, 30],
        cw_bfo: Some((59, ["0", "1", "2"])),
        cw_pitch_display: Some((63, "1")),
        // No DATA MODE item: PKT is narrow like RTTY (Hamlib's widths).
        // Pointing at PKT DISPLAY OFFSET with a value it never has makes
        // RigHelm use the RTTY widths.
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
        meter_select: &[(0, "COMP"), (1, "ALC"), (2, "PO"), (3, "SWR"), (4, "ID"), (5, "VDD")],
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
    // The WIDTH knob in its centre.
    sim: &["SH016;"],
};

/// The 200 W FT-2000D.
pub static FT2000D: RadioModel = RadioModel {
    key: "ft2000d",
    name: "FT-2000D",
    id: "0252",
    caps: Caps { power: (5, 200), ..FT2000.caps },
    ..FT2000
};

// ------------------------------------------------------------- menu

const KEY_JACK: Kind = Kind::Choice(&["OFF", "BUG", "IAMBIC W/O ACS", "IAMBIC W/ ACS"]);
const WIRING: Kind = Kind::Choice(&["NORMAL", "REVERSE"]);
const CW_MEM: Kind = Kind::Choice(&["TEXT MEMORY", "MESSAGE MEMORY"]);
const MICSEL: Kind = Kind::Choice(&["MIC JACK", "DATA JACK", "N.C."]);
const BAND_AB: Kind = Kind::Choice(&["MAIN (VFO-A)", "SUB (VFO-B)"]);
const PASSBAND: Kind = Kind::Choice(&["SOFT", "SHARP"]);
const SHAPE: Kind = Kind::Choice(&["STEEP", "MEDIUM", "GENTLE"]);
const IF_SHIFT: Kind = signed(-1000, 1000, 10, "Hz");
const EQ_GAIN: Kind = signed(-20, 10, 1, "dB");
const NARROW_BW: Kind = Kind::Choice(&[
    "25 Hz", "50 Hz", "100 Hz", "200 Hz", "300 Hz", "400 Hz", "500 Hz", "800 Hz", "1200 Hz", "1400 Hz", "1700 Hz", "2000 Hz",
]);
const DATA_BW: Kind = Kind::Choice(&["25 Hz", "50 Hz", "100 Hz", "200 Hz", "300 Hz", "400 Hz"]);
const MAX_POWER: Kind = Kind::Choice(&["10 W (D: 20 W)", "20 W (D: 50 W)", "50 W (D: 100 W)", "100 W (D: 200 W)"]);
const AGC: &str = "AGC";
const DISP: &str = "DISPLAY";
const DVS: &str = "DVS";
const KEYER: &str = "KEYER";
const GEN: &str = "GENERAL";
const AM: &str = "MODE-AM";
const CW: &str = "MODE-CW";
const DAT: &str = "MODE-DAT";
const FM: &str = "MODE-FM";
const RTTY: &str = "MODE-RTY";
const SSB: &str = "MODE-SSB";
const RXA: &str = "RX AUDIO";
const DSP: &str = "RX DSP";
const SCOPE: &str = "SCOPE";
const TUNING: &str = "TUNING";
const TXA: &str = "TX AUDIO";
const TXG: &str = "TX GNRL";

/// Tables 1-2 (pages 6-7), with the BYTE column as the widths. The book
/// numbers two items 138; the second is 139.
pub static MENU: &[MenuItem] = &[
    m(1, AGC, "FAST DELAY TIME", 4, AGC_DELAY),
    m(2, AGC, "FAST PEAK HOLD TIME", 4, AGC_HOLD),
    m(3, AGC, "MID DELAY TIME", 4, AGC_DELAY),
    m(4, AGC, "MID PEAK HOLD TIME", 4, AGC_HOLD),
    m(5, AGC, "SLOW DELAY TIME", 4, AGC_DELAY),
    m(6, AGC, "SLOW PEAK HOLD TIME", 4, AGC_HOLD),
    m(7, DISP, "DISPLAY COLOR", 1, Kind::Choice(&["COOL BLUE", "CONTRAST BLUE", "FLASH BLUE", "CONTRAST UMBER", "UMBER"])),
    m(8, DISP, "METER DIMMER", 2, num(0, 15, 1, "")),
    m(9, DISP, "DISPLAY DIMMER", 2, num(0, 7, 1, "")),
    m(10, DISP, "TUNING OFFSET INDICATOR", 1, Kind::Choice(&["CLARIFIER OFFSET", "CW TUNING METER", "VRF PEAK POSITION"])),
    m(11, DISP, "S-METER PEAK HOLD", 1, Kind::Choice(&["OFF", "0.5 s", "1.0 s", "2.0 s"])),
    m(12, DISP, "ROTATOR STARTING POINT", 1, Kind::Choice(&["0°", "90°", "180°", "270°"])),
    m(13, DISP, "ROTATOR NEEDLE PRECISELY (minus)", 2, num(0, 30, 2, "°")),
    m(14, DISP, "QMB MARKER", 1, ENABLE),
    // One digit (0/1) per indicator, page 8.
    m(15, DISP, "LEVEL INDICATOR", 7, Kind::Text { max: 7 }),
    m(16, DVS, "VOICE MEMORY OUTPUT LEVEL", 3, LEVEL),
    m(17, DVS, "VOICE MEMORY TX LEVEL", 3, LEVEL),
    m(18, KEYER, "CW BEACON (0=OFF)", 3, num(0, 255, 1, "s")),
    m(19, KEYER, "CONTEST NUMBER STYLE", 1, Kind::Choice(&["1290", "AUNO", "AUNT", "A2NO", "A2NT", "12NO", "12NT"])),
    m(20, KEYER, "CONTEST NUMBER", 4, num(0, 9999, 1, "")),
    m(21, KEYER, "CW MEMORY 1", 1, CW_MEM),
    m(22, KEYER, "CW MEMORY 2", 1, CW_MEM),
    m(23, KEYER, "CW MEMORY 3", 1, CW_MEM),
    m(24, KEYER, "CW MEMORY 4", 1, CW_MEM),
    m(25, KEYER, "CW MEMORY 5", 1, CW_MEM),
    m(26, GEN, "ANTENNA SELECTION MODE", 1, Kind::Choice(&["BAND", "STACK"])),
    m(27, GEN, "BEEP LEVEL", 3, LEVEL),
    m(28, GEN, "CAT BAUD RATE", 1, Kind::Choice(&["4800 bps", "9600 bps", "19200 bps", "38400 bps"])),
    m(29, GEN, "CAT TIME-OUT TIMER", 1, Kind::Choice(&["10 ms", "100 ms", "1000 ms", "3000 ms"])),
    m(30, GEN, "CAT RTS PORT", 1, ENABLE),
    m(31, GEN, "CAT DATA INDICATOR", 1, ENABLE),
    m(32, GEN, "MEMORY GROUP", 1, ENABLE),
    m(33, GEN, "QUICK SPLIT TUNING OFFSET", 3, signed(-20, 20, 1, "kHz")),
    m(34, GEN, "VFO TRACK", 1, Kind::Choice(&["OFF", "BAND", "FREQUENCY"])),
    m(35, GEN, "TX TIME OUT TIMER", 1, Kind::Choice(&["OFF", "3 min", "5 min", "10 min", "15 min", "20 min", "30 min"])),
    m(36, GEN, "TRANSVERTER FREQUENCY DISPLAY", 2, num(30, 49, 1, "MHz")),
    m(37, GEN, "uTUNE DIAL", 1, Kind::Choice(&["STEP-2", "STEP-1", "OFF"])),
    // 1000 = the NB knob, 0000-0100 a fixed level (shown like MIC GAIN).
    m(38, GEN, "SUB NB LEVEL", 4, Kind::MicGain),
    m(39, GEN, "SUB CW NARROW FILTER", 1, Kind::Choice(&["1200 Hz", "500 Hz", "300 Hz"])),
    m(40, GEN, "MIC SCAN", 1, ENABLE),
    m(41, GEN, "SCAN RESUME", 1, Kind::Choice(&["PAUSE", "TIME"])),
    m(42, GEN, "VOX ANTI-TRIP LEVEL", 3, LEVEL),
    m(43, GEN, "FREQUENCY ADJUST", 3, signed(-25, 25, 1, "")),
    m(44, GEN, "SUB IF SHIFT (LSB)", 5, IF_SHIFT),
    m(45, GEN, "SUB IF SHIFT (USB)", 5, IF_SHIFT),
    m(46, GEN, "SUB IF SHIFT (CW-LSB)", 5, IF_SHIFT),
    m(47, GEN, "SUB IF SHIFT (CW-USB)", 5, IF_SHIFT),
    m(48, GEN, "SUB IF SHIFT (RTTY-LSB)", 5, IF_SHIFT),
    m(49, GEN, "SUB IF SHIFT (RTTY-USB)", 5, IF_SHIFT),
    m(50, GEN, "SUB IF SHIFT (PKT-LSB)", 5, IF_SHIFT),
    m(51, GEN, "SUB IF SHIFT (PKT-USB)", 5, IF_SHIFT),
    m(52, AM, "AM MIC GAIN", 4, Kind::MicGain),
    m(53, AM, "AM MIC SELECT", 1, MICSEL),
    m(54, CW, "FRONT KEY JACK TYPE", 1, KEY_JACK),
    m(55, CW, "FRONT KEY JACK WIRING", 1, WIRING),
    m(56, CW, "REAR KEY JACK TYPE", 1, KEY_JACK),
    m(57, CW, "REAR KEY JACK WIRING", 1, WIRING),
    m(58, CW, "CW AUTO MODE", 1, Kind::Choice(&["OFF", "50 MHz ONLY", "ON"])),
    m(59, CW, "CW BFO INJECTION SIDE", 1, Kind::Choice(&["USB", "LSB", "AUTO"])),
    m(60, CW, "CW BREAK-IN MODE", 1, Kind::Choice(&["SEMI BREAK-IN", "FULL BREAK-IN"])),
    m(61, CW, "CW WAVE FORM SHAPE", 1, Kind::Choice(&["1 ms", "2 ms", "4 ms", "6 ms"])),
    m(62, CW, "CW WEIGHT", 2, CW_WEIGHT),
    m(63, CW, "CW FREQUENCY DISPLAY", 1, Kind::Choice(&["DIRECT FREQUENCY", "PITCH OFFSET"])),
    m(64, CW, "CW PC KEYING", 1, ENABLE),
    m(65, CW, "CW QSK TIME", 1, Kind::Choice(&["15 ms", "20 ms", "25 ms", "30 ms"])),
    m(66, DAT, "AFSK DATA INPUT PORT", 1, Kind::Choice(&["DATA JACK", "N.C."])),
    m(67, DAT, "AFSK DATA INPUT LEVEL", 3, LEVEL),
    m(68, DAT, "AFSK DATA OUTPUT BAND", 1, BAND_AB),
    m(69, DAT, "AFSK DATA OUTPUT LEVEL", 3, LEVEL),
    m(70, DAT, "PSK VOX DELAY TIME", 4, DELAY),
    m(71, DAT, "PSK VOX GAIN", 3, LEVEL),
    m(72, DAT, "PKT FREQUENCY DISPLAY OFFSET", 5, signed(-3000, 3000, 10, "Hz")),
    m(73, DAT, "PKT CARRIER POINT FREQUENCY", 5, signed(-3000, 3000, 10, "Hz")),
    m(74, FM, "FM MIC GAIN", 4, Kind::MicGain),
    m(75, FM, "FM MIC SELECT", 1, MICSEL),
    m(76, FM, "28 MHz REPEATER SHIFT", 4, num(0, 1000, 10, "kHz")),
    m(77, FM, "50 MHz REPEATER SHIFT", 4, num(0, 4000, 10, "kHz")),
    m(78, RTTY, "RX POLARITY", 1, WIRING),
    m(79, RTTY, "TX POLARITY", 1, WIRING),
    m(80, RTTY, "DATA OUTPUT BAND", 1, BAND_AB),
    m(81, RTTY, "DATA OUTPUT LEVEL", 3, LEVEL),
    m(82, RTTY, "SHIFT FREQUENCY", 1, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz", "850 Hz"])),
    m(83, RTTY, "MARK FREQUENCY", 1, Kind::Choice(&["1275 Hz", "2125 Hz"])),
    m(84, SSB, "SSB MIC SELECT", 1, MICSEL),
    m(85, SSB, "TX BPF BANDWIDTH", 1, TX_BPF),
    m(86, SSB, "MAIN LSB CARRIER POINT", 4, signed(-200, 200, 10, "Hz")),
    m(87, SSB, "MAIN USB CARRIER POINT", 4, signed(-200, 200, 10, "Hz")),
    m(88, SSB, "SUB LSB CARRIER POINT", 4, signed(-200, 200, 10, "Hz")),
    m(89, SSB, "SUB USB CARRIER POINT", 4, signed(-200, 200, 10, "Hz")),
    m(90, RXA, "AGC GAIN CURVE", 1, Kind::Choice(&["NORMAL", "SLOPED"])),
    m(91, RXA, "HEADPHONE MODE", 1, Kind::Choice(&["SEPARATE", "COMBINE 1", "COMBINE 2"])),
    m(92, DSP, "CONTOUR GAIN", 3, signed(-40, 20, 1, "dB")),
    m(93, DSP, "CONTOUR WIDTH", 2, num(1, 11, 1, "")),
    m(94, DSP, "IF NOTCH WIDTH", 1, Kind::Choice(&["NARROW", "WIDE"])),
    m(95, DSP, "CW FILTER PASSBAND", 1, PASSBAND),
    m(96, DSP, "CW FILTER SHAPE FACTOR", 1, SHAPE),
    m(97, DSP, "CW NARROW FILTER BANDWIDTH", 2, NARROW_BW),
    m(98, DSP, "PKT FILTER PASSBAND", 1, PASSBAND),
    m(99, DSP, "PKT FILTER SHAPE FACTOR", 1, SHAPE),
    m(100, DSP, "PKT FILTER BANDWIDTH", 2, DATA_BW),
    m(101, DSP, "RTTY FILTER PASSBAND", 1, PASSBAND),
    m(102, DSP, "RTTY FILTER SHAPE FACTOR", 1, SHAPE),
    m(103, DSP, "RTTY FILTER BANDWIDTH", 2, DATA_BW),
    m(104, DSP, "SSB FILTER PASSBAND", 1, PASSBAND),
    m(105, DSP, "SSB FILTER SHAPE FACTOR", 1, SHAPE),
    m(
        106,
        DSP,
        "SSB NARROW FILTER BANDWIDTH",
        2,
        Kind::Choice(&[
            "200 Hz", "400 Hz", "600 Hz", "850 Hz", "1100 Hz", "1350 Hz", "1500 Hz", "1650 Hz", "1800 Hz", "1950 Hz", "2100 Hz",
            "2250 Hz",
        ]),
    ),
    m(107, SCOPE, "SCAN START 1.8 MHz", 5, num(1800, 1999, 1, "kHz")),
    m(108, SCOPE, "SCAN START 3.5 MHz", 5, num(3500, 3999, 1, "kHz")),
    m(109, SCOPE, "SCAN START 5.0 MHz", 5, num(5250, 5499, 1, "kHz")),
    m(110, SCOPE, "SCAN START 7.0 MHz", 5, num(7000, 7299, 1, "kHz")),
    m(111, SCOPE, "SCAN START 10 MHz", 5, num(10100, 10149, 1, "kHz")),
    m(112, SCOPE, "SCAN START 14 MHz", 5, num(14000, 14349, 1, "kHz")),
    m(113, SCOPE, "SCAN START 18 MHz", 5, num(18000, 18199, 1, "kHz")),
    m(114, SCOPE, "SCAN START 21 MHz", 5, num(21000, 21449, 1, "kHz")),
    m(115, SCOPE, "SCAN START 24.5 MHz", 5, num(24800, 24989, 1, "kHz")),
    m(116, SCOPE, "SCAN START 28 MHz", 5, num(28000, 29699, 1, "kHz")),
    m(117, SCOPE, "SCAN START 50 MHz", 5, num(50000, 53999, 1, "kHz")),
    m(118, TUNING, "DIAL STEP", 1, Kind::Choice(&["1 Hz", "5 Hz", "10 Hz"])),
    m(119, TUNING, "DIAL CW FINE TUNING", 1, ENABLE),
    m(120, TUNING, "SUB VFO-B KNOB MHz STEP", 1, Kind::Choice(&["1 MHz", "100 kHz"])),
    m(121, TUNING, "MIC UP/DOWN AM STEP", 1, Kind::Choice(&["2.5 kHz", "5 kHz", "9 kHz", "10 kHz", "12.5 kHz"])),
    m(122, TUNING, "MIC UP/DOWN FM STEP", 1, Kind::Choice(&["5 kHz", "6.25 kHz", "10 kHz", "12.5 kHz", "25 kHz"])),
    m(123, TUNING, "DIAL STEP (FM)", 1, Kind::Choice(&["10 Hz", "100 Hz"])),
    // One digit (0/1) per band, page 8.
    m(124, TUNING, "MY BAND SELECT", 13, Kind::Text { max: 13 }),
    m(125, TXA, "MIC EQ LOW FREQ", 2, EQ1_FREQ),
    m(126, TXA, "MIC EQ LOW GAIN", 3, EQ_GAIN),
    m(127, TXA, "MIC EQ LOW BWTH", 2, EQ_BW),
    m(128, TXA, "MIC EQ MID FREQ", 2, EQ2_FREQ),
    m(129, TXA, "MIC EQ MID GAIN", 3, EQ_GAIN),
    m(130, TXA, "MIC EQ MID BWTH", 2, EQ_BW),
    m(131, TXA, "MIC EQ HIGH FREQ", 2, EQ3_FREQ),
    m(132, TXA, "MIC EQ HIGH GAIN", 3, EQ_GAIN),
    m(133, TXA, "MIC EQ HIGH BWTH", 2, EQ_BW),
    m(134, TXA, "PROC EQ LOW FREQ", 2, EQ1_FREQ),
    m(135, TXA, "PROC EQ LOW GAIN", 3, EQ_GAIN),
    m(136, TXA, "PROC EQ LOW BWTH", 2, EQ_BW),
    m(137, TXA, "PROC EQ MID FREQ", 2, EQ2_FREQ),
    m(138, TXA, "PROC EQ MID GAIN", 3, EQ_GAIN),
    m(139, TXA, "PROC EQ MID BWTH", 2, EQ_BW),
    m(140, TXA, "PROC EQ HIGH FREQ", 2, EQ3_FREQ),
    m(141, TXA, "PROC EQ HIGH GAIN", 3, EQ_GAIN),
    m(142, TXA, "PROC EQ HIGH BWTH", 2, EQ_BW),
    // FT-2000D only: 0000-0100 class A bias, 1000 class AB (shown like MIC GAIN).
    m(143, TXG, "BIAS LEVEL (FT-2000D)", 4, Kind::MicGain),
    m(144, TXG, "MAXIMUM OUTPUT POWER LIMIT", 1, MAX_POWER),
    m(145, TXG, "RF PWR KNOB FUNCTION", 1, Kind::Choice(&["ALL MODE", "CARRIER"])),
    m(146, TXG, "TX-GND JACK", 1, ENABLE),
    m(147, TXG, "TUNER DRIVING POWER", 1, MAX_POWER),
    m(148, TXG, "VOX OPERATION", 1, Kind::Choice(&["MIC", "DATA"])),
    m(149, TXG, "EMERGENCY CHANNEL", 1, ENABLE),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_is_complete() {
        assert_eq!(MENU.len(), 149);
        assert_eq!(FT2000.menu_item(29).unwrap().name, "CAT TIME-OUT TIMER");
        assert_eq!(FT2000.menu_item(84).unwrap().name, "SSB MIC SELECT");
        assert_eq!(crate::model::by_id_answer("ID0252;").map(|m| m.key), Some("ft2000d"));
    }

    #[test]
    fn frames_from_the_manual() {
        let mut c = Ctx::new(&FT2000);
        for f in ["FA14250000;", "FA;", "IS0+1000;", "MD0C;", "SH016;", "RF03;", "AN020;", "PC100;", "NL0255;", "EX0291;", "BS02;", "ID;"] {
            assert_eq!(down(&mut c, f), vec![f], "{f}");
            assert_eq!(up(&mut c, f), vec![f], "{f}");
        }
        assert_eq!(up(&mut c, "ID0251;"), vec!["ID0251;"]);
        for f in ["AG1128;", "MD12;", "SM1100;", "GT13;", "NA11;"] {
            assert!(up(&mut c, f).is_empty(), "{f}");
        }
        assert_eq!(down(&mut c, "MG050;"), vec!["MG128;"]);
        assert_eq!(down(&mut c, "PL100;"), vec!["PL255;"]);
        assert_eq!(up(&mut c, "VG255;"), vec!["VG100;"]);
        assert_eq!(down(&mut c, "KP40;"), vec!["KP08;"]);
        assert_eq!(up(&mut c, "KP03;"), vec!["KP15;"]);
        assert_eq!(down(&mut c, "LK7;"), vec!["LK1;"]);
        assert_eq!(up(&mut c, "FS1;"), vec!["FS5;"]);
        assert_eq!(down(&mut c, "PR0;"), vec!["PR;"]);
        assert_eq!(down(&mut c, "PR01;"), vec!["PR1;"]);
        assert!(down(&mut c, "PR1;").is_empty());
        assert!(down(&mut c, "PR11;").is_empty());
        assert_eq!(up(&mut c, "PR1;"), vec!["PR01;"]);
        assert_eq!(down(&mut c, "FR4;"), vec!["FR3;"]);
        assert_eq!(up(&mut c, "FR2;"), vec!["FR0;"]);
        assert_eq!(down(&mut c, "DA101010;"), vec!["DA0710;"]);
        assert_eq!(up(&mut c, "DA0515;"), vec!["DA051500;"]);
        assert!(down(&mut c, "SF4;").is_empty());
        assert!(up(&mut c, "SF7;").is_empty());
        assert!(down(&mut c, "CO0210;").is_empty());
        assert!(down(&mut c, "BA;").is_empty());
        assert!(up(&mut c, "DS1;").is_empty());
        assert!(down(&mut c, "EX150;").is_empty());
    }

    #[test]
    fn round_trip() {
        let mut c = Ctx::new(&FT2000);
        for f in [
            "FA07074000;", "FB14250000;", "MD03;", "AG0128;", "MG037;", "PL050;", "VG100;", "ML1001;", "KP00;", "KP75;",
            "LK4;", "FS5;", "PR00;", "PR01;", "FR0;", "FR1;", "FR4;", "SH031;", "NL0100;", "DA070300;", "SF3;", "EX0150101010;",
            "EX044-0500;", "EX1431000;", "IF00114074000+012010200000;", "PC050;",
        ] {
            let native = down(&mut c, f);
            let back: Vec<String> = native.iter().flat_map(|n| up(&mut c, n)).collect();
            assert_eq!(back, vec![f], "{f} -> {native:?}");
        }
    }

    #[test]
    fn widths_are_hamlibs() {
        assert_eq!(FT2000.width_hz(WidthClass::Ssb, false, 16), Some(2400));
        assert_eq!(FT2000.width_hz(WidthClass::Ssb, false, 12), Some(2100));
        assert_eq!(FT2000.width_hz(WidthClass::Ssb, false, 31), Some(4000));
        assert_eq!(FT2000.width_hz(WidthClass::Ssb, false, 0), None);
        assert_eq!(FT2000.width_hz(WidthClass::Cw, true, 5), Some(350));
        assert_eq!(FT2000.width_hz(WidthClass::Rtty, false, 8), Some(300));
    }
}
