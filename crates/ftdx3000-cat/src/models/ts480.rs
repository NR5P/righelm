//! Kenwood TS-480 (TS-480SAT 100 W with antenna tuner, TS-480HX 200 W), from
//! the "PC Control Command Reference for the TS-480HX/SAT Transceiver".
//!
//! This file also has the Kenwood translator the TS-2000 shares
//! ([`Kw`], [`down_kw`], [`up_kw`]): both radios speak the same command set
//! with a few differences, which [`Kw`] lists.
//!
//! How RigHelm's (FTDX3000) commands become Kenwood's:
//!
//! * Frequencies have 11 digits (`FA00014074000;`), `MD` has no P1 (`MD2;`),
//!   and the mode codes are the same (FSK = RTTY-L, FSR = RTTY-U). Kenwood
//!   has no DATA modes.
//! * `IF` is laid out differently: it is translated field by field. Kenwood's
//!   `IF` (and `MD`) describe the VFO in use, which can be VFO B; then it
//!   becomes RigHelm's `OI` (VFO-B information) and RigHelm's `IF` is made
//!   from what is known of VFO A. Kenwood has no VFO-B read, so RigHelm's
//!   `OI` read is answered from `FB` and VFO B's last known mode.
//! * Split: Kenwood `FR` is the receive VFO (0 A, 1 B, 2 memory), `FT` the
//!   transmit VFO. RigHelm `FR0`/`FR4` (receive on A/B) and `FT0`-`FT3`.
//! * `TX;`/`RX;` key and un-key; there is no TX read, so RigHelm's `TX;` and
//!   `MX;` reads are answered from `IF` (P8). RigHelm's CAT TX (`TX1;`) sends
//!   `TX1;`: transmit with the audio from the DATA terminal (ANI), like
//!   Hamlib's "PTT on data"; RigHelm's `MX1;` sends `TX0;` (microphone).
//! * Auto-information: RigHelm `AI1;` turns on the extended format (`AI2;`).
//! * The menu is `EX` P1 (3) P2 (2) P3 P4 + value: `MenuItem.addr` is those
//!   seven digits as a number (TS-480 menu 34 = `EX0340000` = 340000).
//! * Meters: `SM0` (0-20 dots, S9 at 11) is the S-meter, and the power
//!   meter while transmitting (RigHelm `RM5`); `RM;` answers with all three
//!   of SWR, COMP and ALC (Hamlib's TS-480 driver reads it so; the reference
//!   shows one answer and a 0-10 scale, Hamlib 0-20). Calibrations are
//!   Hamlib's.
//! * Width (`SH0nn`): in CW and FSK the DSP filter width (`FW`), in the other
//!   modes the high cut (`SH`) — the index is the radio's own `SH` number.
//! * Tones: Kenwood keeps separate tone (`TO`/`TN`) and CTCSS (`CT`/`CN`)
//!   settings; RigHelm's `CT0x`/`CN0nn` drive both (`CT02` = tone, `CT01` =
//!   CTCSS). Tone numbers are mapped by frequency.
//! * Break-in (`BI`) is the VOX switch in CW (`VX`), as on the radio.
//! * Keyer/voice message playback `PB`; RigHelm's `KY1`-`KY3` play the
//!   electric keyer's messages the same way (`PB1`-`PB3`).
//! * CW pitch (`KP`) is menu 34 (400-1000 Hz in 50 Hz steps).
//! * Levels with another range are scaled (RF gain 0-100, VOX gain 0-9,
//!   monitor 1-9, break-in and VOX delays in 50/150 ms steps); a value the
//!   radio reports back for one RigHelm set comes up as exactly that value.
//!
//! The same functions work towards a program that wants to see a Kenwood
//! (virtual ports, the simulator); where a Kenwood Set and Answer differ
//! (`PA1;` / `PA10;`) the direction is known from the frames: only a program
//! sends Kenwood reads (`FA;`), only RigHelm sends RigHelm reads.

use crate::dialect::rescale;
use crate::dialect::text::{Ctx, TextSpec, is_read, menu_down, menu_up, parts, read_prefix};
use crate::menu::{Kind, MenuItem, num};
use crate::model::{Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::state::ChannelInfo;
use crate::types::{self, Band, Mode, PO_CAL, S_METER_CAL, SWR_CAL, WidthClass};

pub static SPEC: TextSpec = TextSpec { down, up, answer_prefix: read_prefix };
pub static SPEC_HX: TextSpec = TextSpec { down: down_hx, up: up_hx, answer_prefix: read_prefix };

fn down(cx: &mut Ctx, f: &str) -> Vec<String> {
    down_kw(&KW, cx, f)
}

fn up(cx: &mut Ctx, f: &str) -> Vec<String> {
    up_kw(&KW, cx, f)
}

fn down_hx(cx: &mut Ctx, f: &str) -> Vec<String> {
    down_kw(&KW_HX, cx, f)
}

fn up_hx(cx: &mut Ctx, f: &str) -> Vec<String> {
    up_kw(&KW_HX, cx, f)
}

// ------------------------------------------------------------ the radio

/// The 42 CTCSS tones of the TS-480 (`CN` 00-41, `TN` 00-41; `TN` 42 is
/// 1750 Hz), in 0.1 Hz.
pub(super) const TONES_42: &[u16] = &[
    670, 693, 719, 744, 770, 797, 825, 854, 885, 915, 948, 974, 1000, 1035, 1072, 1109, 1148, 1188, 1230, 1273, 1318,
    1365, 1413, 1462, 1514, 1567, 1622, 1679, 1738, 1799, 1862, 1928, 2035, 2065, 2107, 2181, 2257, 2291, 2336, 2418,
    2503, 2541,
];

/// S-meter dots (0-20) -> dB relative to S9 (Hamlib's TS-480 calibration:
/// S9 at 11 dots, +60 dB at 20).
const SM_CAL_20: &[(u8, f32)] =
    &[(0, -60.0), (3, -48.0), (5, -36.0), (7, -24.0), (9, -12.0), (11, 0.0), (14, 20.0), (17, 40.0), (20, 60.0)];

static KW: Kw = Kw {
    tones: TONES_42,
    tone_first: 0,
    rg_max: 100,
    sm_cal: SM_CAL_20,
    sm_max: 20,
    rm_max: 20,
    po_watts: 100.0,
    if_p2: "     ",
    clar_digits: 4,
    if_shift: false,
    pitch_addr: 340000,
    gt: &[(0, 0), (1, 1), (3, 2)],
    ptt: "TX1;",
    mox: "TX0;",
    ssb_widths: SSB_WIDTHS,
    scan_from: 90,
    max_ch: 99,
    sc_answer_2: true,
    pb_answer_3: true,
    has_vv: true,
    has_is: true,
    has_os: false,
    rm_all: true,
};

static KW_HX: Kw = Kw { po_watts: 200.0, ..KW };

/// `SH` 00-13 in SSB/FM: the high cut, Hz.
const SSB_WIDTHS: &[u32] = &[1000, 1200, 1400, 1600, 1800, 2000, 2200, 2400, 2600, 2800, 3000, 3400, 4000, 5000];

/// `FW` widths in CW and FSK (the TS-2000's are the same).
const CW_WIDTHS: &[u32] = &[50, 80, 100, 150, 200, 300, 400, 500, 600, 1000, 2000];
const FSK_WIDTHS: &[u32] = &[250, 500, 1000, 1500];

pub(super) fn widths_from(class: WidthClass, ssb: &[u32]) -> Vec<(u8, u32)> {
    let t: &[u32] = match class {
        WidthClass::Ssb => ssb,
        WidthClass::Cw => CW_WIDTHS,
        WidthClass::Rtty => FSK_WIDTHS,
        WidthClass::Fixed => &[],
    };
    t.iter().enumerate().map(|(i, w)| (i as u8, *w)).collect()
}

fn widths(class: WidthClass, _narrow: bool) -> Vec<(u8, u32)> {
    widths_from(class, SSB_WIDTHS)
}

/// The modes, in front-panel order (LSB/USB, CW/CW-R, FSK/FSR, FM/AM).
pub(super) const MODES: &[Mode] = &[Mode::Lsb, Mode::Usb, Mode::Cw, Mode::CwR, Mode::RttyLsb, Mode::RttyUsb, Mode::Fm, Mode::Am];

/// HF and 6 m. The TS-480 has no band-select command; RigHelm tunes there.
pub(super) const HF_BANDS: &[Band] = &[
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
];

const BANDS: &[Band] = &[
    HF_BANDS[0],
    HF_BANDS[1],
    HF_BANDS[2],
    HF_BANDS[3],
    HF_BANDS[4],
    HF_BANDS[5],
    HF_BANDS[6],
    HF_BANDS[7],
    HF_BANDS[8],
    HF_BANDS[9],
    HF_BANDS[10],
    Band { code: 11, label: "GEN", lo: 100_000, hi: 59_999_999 },
];

/// The RigHelm commands the TS-480 can carry out.
const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AN", "BC", "BD", "BI", "BU", "BY", "CH", "CN", "CT", "DN", "EX", "FA", "FB", "FR", "FT",
    "GT", "ID", "IF", "IS", "KP", "KS", "KY", "LK", "MC", "MD", "MG", "ML", "MR", "MW", "MX", "NB", "NL", "NR",
    "OI", "PA", "PB", "PC", "PL", "PR0", "PS", "QI", "RA", "RC", "RD", "RG", "RL", "RM", "RS", "RT", "RU", "SC", "SD",
    "SH", "SM", "SQ", "TS", "TX", "UL", "UP", "VD", "VG", "VX", "XT",
];

/// The HX has no antenna tuner.
const COMMANDS_HX: &[&str] = &[
    "AB", "AG", "AI", "AN", "BC", "BD", "BI", "BU", "BY", "CH", "CN", "CT", "DN", "EX", "FA", "FB", "FR", "FT", "GT",
    "ID", "IF", "IS", "KP", "KS", "KY", "LK", "MC", "MD", "MG", "ML", "MR", "MW", "MX", "NB", "NL", "NR", "OI",
    "PA", "PB", "PC", "PL", "PR0", "PS", "QI", "RA", "RC", "RD", "RG", "RL", "RM", "RS", "RT", "RU", "SC", "SD", "SH",
    "SM", "SQ", "TS", "TX", "UL", "UP", "VD", "VG", "VX", "XT",
];

const CAPS: Caps = Caps {
    commands: COMMANDS,
    widths,
    narrow: false,
    preamp: &[(0, "OFF"), (1, "ON")],
    att: &[(0, "OFF"), (1, "ON")],
    agc: &[(0, "OFF"), (1, "FAST"), (3, "SLOW")],
    roofing: &[],
    roofing_answers: &[],
    nb: &[(0, "OFF"), (1, "ON")],
    nr_max: 9,
    nb_level_max: 10,
    antennas: 2,
    power: (5, 100),
    // RM; reads all three meters; which one the radio shows isn't reported.
    meter_select: &[],
    voice_memories: 3,
    keyer_memories: 3,
    keyer_memory_len: 0,
    memory: Memory { first: 0, last: 99, pms: None },
    shared_clarifier: true,
};

const ROLES: Roles = Roles {
    mic_sel: &[],
    mic_front: 0,
    mic_usb: 0,
    pc_keying: None,
    cat_tot: None,
    // 056 COM port speed.
    cat_items: &[57],
    cw_bfo: None,
    cw_pitch_display: None,
    data_other: None,
    data_disp: None,
    data_psk: false,
};

/// TS-480SAT: 100 W, with the internal antenna tuner. Detected first (the HX
/// answers the same `ID020;`; choose it in the settings).
pub static TS480: RadioModel = RadioModel {
    key: "ts480",
    name: "TS-480SAT",
    maker: Maker::Kenwood,
    family: Family::Kenwood,
    proto: Proto::Text(&SPEC),
    id: "020",
    civ_addr: 0,
    hamlib: 2028,
    freq_min: 100_000,
    freq_max: 59_999_999,
    bands: BANDS,
    modes: MODES,
    menu: MENU,
    roles: ROLES,
    caps: CAPS,
    bauds: &[115200, 57600, 38400, 19200, 9600, 4800],
    stop_bits: 1,
    rts_cts: true,
    sim: &[],
};

/// TS-480HX: 200 W, no antenna tuner.
pub static TS480HX: RadioModel = RadioModel {
    key: "ts480hx",
    name: "TS-480HX",
    proto: Proto::Text(&SPEC_HX),
    caps: Caps { commands: COMMANDS_HX, power: (5, 200), ..CAPS },
    ..TS480
};

// ------------------------------------------------------------ the menu

pub(super) const fn item(num: u16, addr: u32, group: &'static str, name: &'static str, width: u8, kind: Kind) -> MenuItem {
    MenuItem { num, addr, group, name, width, kind }
}

/// TS-480 menu `nnn`: RigHelm item `nnn + 1`, address `nnn` 00 0 0.
const fn t(n: u16, group: &'static str, name: &'static str, width: u8, kind: Kind) -> MenuItem {
    item(n + 1, n as u32 * 10000, group, name, width, kind)
}

pub(super) const OFF_ON: Kind = Kind::Choice(&["OFF", "ON"]);
pub(super) const OFF_9: Kind = Kind::Choice(&["OFF", "1", "2", "3", "4", "5", "6", "7", "8", "9"]);
pub(super) const WEIGHT: Kind = Kind::Choice(&[
    "AUTO", "2.5", "2.6", "2.7", "2.8", "2.9", "3.0", "3.1", "3.2", "3.3", "3.4", "3.5", "3.6", "3.7", "3.8", "3.9", "4.0",
]);
pub(super) const PITCH: Kind = Kind::Steps { max_code: 12, off_at_zero: false, first: 400, step: 50, unit: "Hz" };
const EQ: Kind = Kind::Choice(&["OFF", "Hb1", "Hb2", "FP", "bb1", "bb2", "c", "U"]);
const PF: Kind = num(0, 99, 1, "");

const DISP: &str = "DISPLAY";
const TUNING: &str = "TUNING";
const SCAN: &str = "MEMORY/SCAN";
const SOUND: &str = "SOUND";
const DSP: &str = "DSP";
const TXG: &str = "TX";
const AT: &str = "ANTENNA TUNER";
const LIN: &str = "LINEAR AMP";
const MSG: &str = "MESSAGE PLAYBACK";
const CW: &str = "CW";
const FSK: &str = "FSK";
const DATA: &str = "FM/DATA";
const PFK: &str = "PF KEYS";
const GEN: &str = "GENERAL";

/// Menu 000-060 (61 items).
pub const MENU: &[MenuItem] = &[
    t(0, DISP, "DISPLAY BRIGHTNESS", 1, Kind::Choice(&["OFF", "1", "2", "3", "4"])),
    t(1, DISP, "KEY ILLUMINATION", 1, OFF_ON),
    t(2, TUNING, "AUTO MODE", 1, OFF_ON),
    t(3, TUNING, "TUNING CONTROL RATE", 1, Kind::Choice(&["250 Hz", "500 Hz", "1000 Hz"])),
    t(4, TUNING, "TUNING CONTROL IN FM", 1, OFF_ON),
    t(5, TUNING, "MULTI ROUNDS OFF FREQUENCY", 1, OFF_ON),
    t(6, TUNING, "9 kHz MULTI STEP ON AM BC BAND", 1, OFF_ON),
    t(7, SCAN, "TUNABLE MEMORY CHANNELS", 1, OFF_ON),
    t(8, SCAN, "PROGRAM SCAN PARTIALLY SLOWED", 1, OFF_ON),
    t(9, SCAN, "PROGRAM SCAN SLOW-DOWN RANGE", 1, Kind::Steps { max_code: 4, off_at_zero: false, first: 100, step: 100, unit: "Hz" }),
    t(10, SCAN, "PROGRAM SCAN HOLD", 1, OFF_ON),
    t(11, SCAN, "SCAN RESUME", 1, Kind::Choice(&["TO", "CO"])),
    t(12, SOUND, "BEEP LEVEL", 1, OFF_9),
    t(13, SOUND, "TX SIDETONE VOLUME", 1, OFF_9),
    t(14, SOUND, "VGS-1 PLAYBACK VOLUME", 1, OFF_9),
    t(15, SOUND, "VGS-1 ANNOUNCEMENT VOLUME", 1, Kind::Choice(&["OFF", "1", "2", "3", "4", "5", "6", "7"])),
    t(16, SOUND, "VGS-1 ANNOUNCEMENT SPEED", 1, Kind::Choice(&["OFF", "1", "2", "3", "4"])),
    t(17, DSP, "CW IF FILTER FOR SSB RX", 1, OFF_ON),
    t(18, DSP, "DSP RX EQUALIZER", 1, EQ),
    t(19, DSP, "DSP TX EQUALIZER", 1, EQ),
    t(20, DSP, "DSP TX BANDWIDTH SSB/AM", 1, Kind::Choice(&["2.0 kHz", "2.4 kHz"])),
    t(21, TXG, "FINE TX POWER", 1, OFF_ON),
    t(22, TXG, "TIME-OUT TIMER", 1, Kind::Choice(&["OFF", "3 min", "5 min", "10 min", "20 min", "30 min"])),
    t(23, TXG, "TRANSVERTER DISPLAY", 1, OFF_ON),
    t(24, TXG, "TRANSVERTER POWER ADJUST", 1, OFF_ON),
    t(25, AT, "TX HOLD AFTER AT TUNING", 1, OFF_ON),
    t(26, AT, "IN-LINE AT WHILE RECEIVING", 1, OFF_ON),
    t(27, AT, "EXTERNAL AT CONTROL", 1, Kind::Choice(&["AT1", "AT2"])),
    t(28, LIN, "LINEAR DELAY HF", 1, Kind::Choice(&["OFF", "1", "2", "3"])),
    t(29, LIN, "LINEAR DELAY 50 MHz", 1, Kind::Choice(&["OFF", "1", "2", "3"])),
    t(30, MSG, "CONSTANT RECORDING", 1, OFF_ON),
    t(31, MSG, "REPEAT PLAYBACK", 1, OFF_ON),
    t(32, MSG, "PLAYBACK REPEAT INTERVAL", 2, num(0, 60, 1, "s")),
    t(33, MSG, "KEYING PRIORITY OVER PLAYBACK", 1, OFF_ON),
    t(34, CW, "CW PITCH / SIDETONE", 2, PITCH),
    t(35, CW, "CW WEIGHT", 2, WEIGHT),
    t(36, CW, "REVERSE AUTO WEIGHT", 1, OFF_ON),
    t(37, CW, "BUG KEY", 1, OFF_ON),
    t(38, CW, "SWAP DOT/DASH PADDLE", 1, OFF_ON),
    t(39, CW, "AUTO CW TX IN SSB", 1, OFF_ON),
    t(40, CW, "SSB TO CW FREQUENCY CORRECTION", 1, OFF_ON),
    t(41, FSK, "FSK SHIFT", 1, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz", "850 Hz"])),
    t(42, FSK, "FSK KEYING POLARITY REVERSE", 1, OFF_ON),
    t(43, FSK, "FSK TONE", 1, Kind::Choice(&["1275 Hz", "2125 Hz"])),
    t(44, DATA, "FM MIC GAIN", 1, Kind::Choice(&["1", "2", "3"])),
    t(45, DATA, "DATA FILTER BANDWIDTH", 1, OFF_ON),
    t(46, DATA, "DATA AF INPUT LEVEL", 1, num(0, 9, 1, "")),
    t(47, DATA, "DATA AF OUTPUT LEVEL", 1, num(0, 9, 1, "")),
    t(48, PFK, "REMOTE PANEL PF KEY", 2, PF),
    t(49, PFK, "MIC PF1 KEY", 2, PF),
    t(50, PFK, "MIC PF2 KEY", 2, PF),
    t(51, PFK, "MIC PF3 KEY", 2, PF),
    t(52, PFK, "MIC PF4 KEY", 2, PF),
    t(53, GEN, "SPLIT TRANSFER (MASTER/SLAVE)", 1, OFF_ON),
    t(54, GEN, "WRITE TRANSFERRED SPLIT TO VFOS", 1, OFF_ON),
    t(55, GEN, "TX INHIBIT", 1, OFF_ON),
    t(56, GEN, "COM PORT SPEED", 1, Kind::Choice(&["4800", "9600", "19200", "38400", "57600", "115200"])),
    t(57, GEN, "DTS POLARITY", 1, OFF_ON),
    t(58, GEN, "BUSY LOCKOUT (TX)", 1, OFF_ON),
    t(59, GEN, "AUTO POWER OFF", 1, Kind::Choice(&["OFF", "60 min", "120 min", "180 min"])),
    t(60, GEN, "TX WITH DATA TERMINAL AUDIO", 1, OFF_ON),
];

// ------------------------------------------------------------ the translator

/// How one Kenwood radio's commands differ from the others'.
#[derive(Debug, Clone, Copy)]
pub(super) struct Kw {
    /// Tone frequencies (0.1 Hz) of the `TN`/`CN` numbers, the first being
    /// number `tone_first`.
    pub tones: &'static [u16],
    pub tone_first: u8,
    /// `RG` full scale.
    pub rg_max: u32,
    /// `SM0` dots -> dB relative to S9, and the full scale of `SM` and `RM`.
    pub sm_cal: &'static [(u8, f32)],
    pub sm_max: u32,
    pub rm_max: u32,
    /// Watts at full scale of `SM0` while transmitting.
    pub po_watts: f32,
    /// `IF`: P2 (filler), the RIT/XIT offset's digits, P15 is the repeater shift.
    pub if_p2: &'static str,
    pub clar_digits: usize,
    pub if_shift: bool,
    /// The CW pitch menu item (400-1000 Hz in 50 Hz steps).
    pub pitch_addr: u32,
    /// `GT`: (RigHelm set code, Kenwood value) for the settings it has.
    pub gt: &'static [(u8, u16)],
    /// What RigHelm's `TX1;` (CAT transmit) and `MX1;` (MOX) send.
    pub ptt: &'static str,
    pub mox: &'static str,
    pub ssb_widths: &'static [u32],
    /// Memory channels from `scan_from` on hold program scan ranges (start
    /// and end frequency); `max_ch` is the last one.
    pub scan_from: u16,
    pub max_ch: u16,
    /// The `SC` answer has P3 (normal/program slow scan); the `PB` answer is
    /// a three-channel queue.
    pub sc_answer_2: bool,
    pub pb_answer_3: bool,
    /// `VV` (A=B), `IS` (IF shift), `OS` (repeater shift) exist.
    pub has_vv: bool,
    pub has_is: bool,
    pub has_os: bool,
    /// `RM;` answers all three meters (else only the one selected, which is
    /// RigHelm's `MS`).
    pub rm_all: bool,
}

/// Is this a translation for a program (emulator), not for the radio?
fn emu(cx: &Ctx) -> bool {
    cx.recall("dir") == Some("emu")
}

fn get<T: std::str::FromStr>(cx: &Ctx, key: &str) -> Option<T> {
    cx.recall(key)?.parse().ok()
}

fn flag(cx: &Ctx, key: &str) -> bool {
    cx.recall(key) == Some("1")
}

fn ch(p: &str, i: usize) -> char {
    p.as_bytes().get(i).map_or(' ', |b| *b as char)
}

fn n<T: std::str::FromStr>(s: &str) -> Option<T> {
    s.trim().trim_start_matches('+').parse().ok()
}

fn one(s: String) -> Vec<String> {
    vec![s]
}

/// The VFO in use is VFO B (receiving on B, or transmitting on B).
fn cur_b(cx: &Ctx) -> bool {
    if flag(cx, "tx") { cx.recall("ft") == Some("1") } else { cx.recall("fr") == Some("1") }
}

/// A Kenwood mode code RigHelm also has (`8` is unused).
fn kw_mode(c: char) -> bool {
    matches!(c, '1'..='7' | '9')
}

/// The nearest Kenwood mode, for answers about a RigHelm mode Kenwood lacks.
fn nearest_mode(m: Option<Mode>) -> char {
    match m {
        Some(Mode::Lsb | Mode::DataLsb) => '1',
        Some(Mode::Cw) => '3',
        Some(Mode::Fm | Mode::FmN | Mode::DataFm | Mode::DataFmN | Mode::C4fm | Mode::Dv | Mode::Wfm) => '4',
        Some(Mode::Am | Mode::AmN | Mode::DataAm) => '5',
        Some(Mode::RttyLsb) => '6',
        Some(Mode::CwR) => '7',
        Some(Mode::RttyUsb) => '9',
        _ => '2',
    }
}

/// The width class of the mode in use (the last `MD` seen).
fn cur_class(cx: &Ctx) -> WidthClass {
    cx.recall("mode").and_then(|m| m.chars().next()).and_then(Mode::from_code).map_or(WidthClass::Ssb, Mode::width_class)
}

/// A RigHelm value `v` (0..=`rh_max`) as the radio's (0..=`radio_max`),
/// remembered so the radio's answer comes back as `v`.
fn scale_down(cx: &mut Ctx, key: &str, v: u32, rh_max: u32, radio_max: u32) -> u32 {
    cx.remember(key, v.to_string());
    ((v.min(rh_max) * radio_max) as f64 / rh_max as f64).round() as u32
}

fn scale_up(cx: &mut Ctx, key: &str, k: u32, radio_max: u32, rh_max: u32) -> u32 {
    if let Some(v) = get::<u32>(cx, key) {
        if ((v.min(rh_max) * radio_max) as f64 / rh_max as f64).round() as u32 == k {
            return v;
        }
    }
    let v = ((k.min(radio_max) * rh_max) as f64 / radio_max as f64).round() as u32;
    cx.remember(key, v.to_string());
    v
}

/// A RigHelm value in the radio's steps (`step` units up to `max`).
fn step_down(cx: &mut Ctx, key: &str, v: u32, step: u32, max: u32) -> u32 {
    cx.remember(key, v.to_string());
    ((v as f64 / step as f64).round() as u32 * step).min(max)
}

fn step_up(cx: &mut Ctx, key: &str, k: u32, step: u32, max: u32) -> u32 {
    if let Some(v) = get::<u32>(cx, key) {
        if ((v as f64 / step as f64).round() as u32 * step).min(max) == k {
            return v;
        }
    }
    cx.remember(key, k.to_string());
    k
}

/// RigHelm (FTDX3000 Table 1) tone number -> the radio's.
fn tone_down(k: &Kw, rh: u32) -> u32 {
    let hz = types::CTCSS_DHZ[(rh as usize).min(types::CTCSS_DHZ.len() - 1)];
    let i = k.tones.iter().enumerate().min_by_key(|(_, t)| (**t as i32 - hz as i32).abs()).map_or(0, |(i, _)| i);
    i as u32 + k.tone_first as u32
}

fn tone_up(k: &Kw, n: u32) -> u32 {
    let i = (n.saturating_sub(k.tone_first as u32) as usize).min(k.tones.len() - 1);
    let hz = k.tones[i];
    types::CTCSS_DHZ.iter().enumerate().min_by_key(|(_, t)| (**t as i32 - hz as i32).abs()).map_or(0, |(i, _)| i as u32)
}

/// RigHelm's `CT0x` from the radio's tone (`TO`) and CTCSS (`CT`) switches.
fn ct_frame(cx: &Ctx) -> String {
    let v = if flag(cx, "to") {
        2
    } else if flag(cx, "ct") {
        1
    } else {
        0
    };
    format!("CT0{v};")
}

/// RigHelm `GT` set code <-> the radio's AGC value.
fn gt_down(k: &Kw, code: u8) -> u16 {
    let code = match code {
        4 => 3, // AUTO: SLOW
        c => c,
    };
    k.gt.iter().find(|(c, _)| *c >= code).or(k.gt.last()).map_or(0, |g| g.1)
}

fn gt_up(k: &Kw, v: u16) -> u8 {
    // The nearest setting (OFF only for 0).
    if v == 0 {
        return 0;
    }
    k.gt.iter().filter(|(_, g)| *g != 0).min_by_key(|(_, g)| (*g as i32 - v as i32).abs()).map_or(0, |g| g.0)
}

/// Kenwood `IF` from its fields.
#[allow(clippy::too_many_arguments)]
fn kenwood_if(k: &Kw, info: &ChannelInfo, freq: u64, mode: char, tx: bool, func: char, split: bool) -> String {
    let sign = if info.clar_offset < 0 { '-' } else { '+' };
    let max = if k.clar_digits >= 5 { 99_999 } else { 9_999 };
    let tone = match info.ctcss {
        1 => '2',
        2 => '1',
        _ => '0',
    };
    let tn = if info.ctcss == 0 { k.tone_first as u32 } else { tone_down(k, info.tone as u32) };
    let shift = if k.if_shift { (b'0' + info.shift.min(2)) as char } else { ' ' };
    format!(
        "IF{freq:011}{}{sign}{:0w$}{}{}{}{:02}{}{mode}{func}0{}{tone}{tn:02}{shift};",
        k.if_p2,
        info.clar_offset.unsigned_abs().min(max),
        info.rx_clar as u8,
        info.tx_clar as u8,
        info.channel / 100,
        info.channel % 100,
        tx as u8,
        split as u8,
        w = k.clar_digits
    )
}

/// RigHelm's `IF` answer as the radio's (for a program).
fn if_down(k: &Kw, cx: &Ctx, p: &str) -> Vec<String> {
    let Some(info) = ChannelInfo::parse(p) else { return Vec::new() };
    let (fr, ft) = (cx.recall("fr").unwrap_or("0").to_string(), cx.recall("ft").unwrap_or("0").to_string());
    let b = cur_b(cx);
    let freq = if b { get(cx, "fb").unwrap_or(info.freq) } else { info.freq };
    let mode = match cx.recall(if b { "mb" } else { "ma" }).and_then(|m| m.chars().next()) {
        Some(m) if b => m,
        _ => nearest_mode(info.mode),
    };
    let func = if info.mem_mode != 0 { '2' } else { fr.chars().next().unwrap_or('0') };
    let split = fr != ft && fr != "2";
    one(kenwood_if(k, &info, freq, mode, flag(cx, "tx"), func, split))
}

/// The radio's `IF` answer as RigHelm's: `IF` (VFO A), `OI` (when it
/// describes VFO B), `TX`, `MX`, `FR`, `FT`, `CT`.
fn if_up(k: &Kw, cx: &mut Ctx, p: &str) -> Vec<String> {
    if p.len() < 35 || !p.is_ascii() {
        return Vec::new();
    }
    let Some(freq) = n::<u64>(&p[0..11]) else { return Vec::new() };
    // P2 and P3: the offset is the signed number at the end of the field.
    let region = &p[11..21];
    let clar = match region.rfind(['+', '-']) {
        Some(i) => n::<i32>(&region[i + 1..]).map_or(0, |v| if &region[i..i + 1] == "-" { -v } else { v }),
        None => n::<i32>(region).unwrap_or(0),
    };
    let bank = n::<u16>(&p[23..24]).unwrap_or(0);
    let chan = bank * 100 + n::<u16>(&p[24..26]).unwrap_or(0);
    let tx = ch(p, 26) == '1';
    let mode = ch(p, 27);
    let func = ch(p, 28);
    let split = ch(p, 30) == '1';
    let ctcss = match ch(p, 31) {
        '1' => 2,
        '2' => 1,
        _ => 0,
    };
    let tone = n::<u32>(&p[32..34]).map_or(0, |t| tone_up(k, t)) as u8;
    let shift = match ch(p, 34) {
        '1' => 1,
        '2' | '3' => 2,
        _ => 0,
    };
    let fr = if matches!(func, '0'..='3') { func } else { '0' };
    let ft = match fr {
        '0' | '1' if split => {
            if fr == '0' {
                '1'
            } else {
                '0'
            }
        }
        f => f,
    };
    cx.remember("tx", if tx { "1" } else { "0" });
    cx.remember("fr", fr.to_string());
    cx.remember("ft", ft.to_string());
    match ch(p, 31) {
        '1' => {
            cx.remember("to", "1");
            cx.remember("ct", "0");
        }
        '2' => {
            cx.remember("to", "0");
            cx.remember("ct", "1");
        }
        _ => {
            cx.remember("to", "0");
            cx.remember("ct", "0");
        }
    }
    let mode = if kw_mode(mode) { Some(mode) } else { None };
    let mut info = ChannelInfo {
        channel: chan,
        freq,
        clar_offset: clar,
        rx_clar: ch(p, 21) == '1',
        tx_clar: ch(p, 22) == '1',
        mode: mode.and_then(Mode::from_code),
        mem_mode: if matches!(func, '2' | '3') { 1 } else { 0 },
        ctcss,
        tone,
        shift,
    };
    let mut out = Vec::new();
    if let Some(m) = mode {
        cx.remember("mode", m.to_string());
    }
    let on_b = if tx { ft == '1' } else { fr == '1' };
    if on_b {
        cx.remember("fb", freq.to_string());
        if let Some(m) = mode {
            cx.remember("mb", m.to_string());
        }
        out.push(info.to_answer("OI"));
        if let Some(fa) = get::<u64>(cx, "fa") {
            info.freq = fa;
        }
        if let Some(ma) = cx.recall("ma").and_then(|m| m.chars().next()).and_then(Mode::from_code) {
            info.mode = Some(ma);
        }
    } else if fr == '0' {
        cx.remember("fa", freq.to_string());
        if let Some(m) = mode {
            cx.remember("ma", m.to_string());
        }
    }
    out.insert(0, info.to_answer("IF"));
    let txv = if !tx {
        0
    } else if flag(cx, "cattx") {
        1
    } else {
        2
    };
    out.push(format!("TX{txv};"));
    out.push(format!("MX{};", (tx && flag(cx, "mox")) as u8));
    out.push(format!("FR{};", if fr == '1' { 4 } else { 0 }));
    out.push(format!("FT{};", if ft == '1' { 1 } else { 0 }));
    out.push(ct_frame(cx));
    out
}

/// RigHelm's `OI` (VFO B) from what is known of VFO B.
fn oi_from_memo(cx: &Ctx) -> Option<String> {
    let freq = get::<u64>(cx, "fb")?;
    let mode = cx.recall("mb").or(cx.recall("ma")).and_then(|m| m.chars().next()).and_then(Mode::from_code).unwrap_or(Mode::Usb);
    Some(ChannelInfo { freq, mode: Some(mode), ..Default::default() }.to_answer("OI"))
}

/// Kenwood `MR` answer / `MW` set (P1 0 RX, 1 TX frequency) for a channel.
fn kenwood_mem(k: &Kw, code: &str, p1: char, chan: u16, info: &ChannelInfo) -> String {
    let tone = match info.ctcss {
        1 => '2',
        2 => '1',
        _ => '0',
    };
    let tn = tone_down(k, info.tone as u32);
    let shift = if k.has_os { info.shift.min(2) } else { 0 };
    format!(
        "{code}{p1}{}{:02}{:011}{}0{tone}{tn:02}{tn:02}0000{shift}000000000000{};",
        chan / 100,
        chan % 100,
        info.freq,
        nearest_mode(info.mode),
        "",
    )
}

/// The radio's `MR` answer / `MW` set as RigHelm's (`MR`/`MW` with the
/// FTDX3000's channel information).
fn mem_up(k: &Kw, code: &str, p: &str) -> Option<String> {
    if p.len() < 39 || !p.is_ascii() {
        return None;
    }
    let bank = n::<u16>(&p[1..2]).unwrap_or(0);
    let chan = bank * 100 + n::<u16>(&p[2..4])?;
    let freq = n::<u64>(&p[4..15])?;
    let mode = Mode::from_code(ch(p, 15)).filter(|_| kw_mode(ch(p, 15)));
    let ctcss = match ch(p, 17) {
        '1' => 2,
        '2' => 1,
        _ => 0,
    };
    let tn = match ch(p, 17) {
        '2' => n::<u32>(&p[20..22]),
        _ => n::<u32>(&p[18..20]),
    }
    .map_or(0, |t| tone_up(k, t)) as u8;
    let shift = if k.has_os {
        match ch(p, 26) {
            '1' => 1,
            '2' | '3' => 2,
            _ => 0,
        }
    } else {
        0
    };
    let info = ChannelInfo { channel: chan, freq, mode, mem_mode: 1, ctcss, tone: tn, shift, ..Default::default() };
    Some(if code == "MW" { info.to_mw(chan) } else { info.to_answer("MR") })
}

/// The SWR scale (on 0-255 = the whole bar): 1.5 at a fifth, 2 at two
/// fifths, 3 at three fifths (Hamlib's TS-480 calibration).
const KW_SWR: &[(u8, f32)] = &[(0, 1.0), (51, 1.5), (102, 2.0), (153, 3.0), (255, 10.0)];

/// RigHelm's meter answers from the radio's `RMxnnnn` (SWR, COMP, ALC). The
/// TS-480 answers `RM;` with all three frames (as Hamlib expects); a radio
/// that answers only its selected meter leaves the others at their last
/// reading.
fn rm_up(k: &Kw, cx: &mut Ctx, p: &str) -> Vec<String> {
    let sel = ch(p, 0);
    let dots = n::<u32>(p.get(1..).unwrap_or("")).unwrap_or(0).min(k.rm_max);
    let mut out = Vec::new();
    if !k.rm_all {
        out.push(format!("MS{};", if matches!(sel, '1'..='3') { sel } else { '1' }));
    }
    for (kc, rh) in [('1', '6'), ('2', '3'), ('3', '4')] {
        let key = format!("rm{rh}");
        if kc == sel {
            let full = k.rm_max.max(1) as f32;
            let raw = (dots as f32 * 255.0 / full).round() as u8;
            let v = match rh {
                // SWR: 1 / 1.5 / 2 / 3 / infinity across the scale.
                '6' => rescale(raw, KW_SWR, &SWR_CAL),
                _ => raw,
            };
            cx.remember(&key, v.to_string());
        }
        out.push(format!("RM{rh}{:03};", get::<u32>(cx, &key).unwrap_or(0)));
    }
    out
}

/// The radio's frame (an answer, or a program's command) as RigHelm's.
pub(super) fn up_kw(k: &Kw, cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    // Only a program sends Kenwood reads; the radio's frames have values.
    let kenwood_read =
        p.is_empty() || (matches!(code.as_str(), "AG" | "SQ" | "SM") && p.len() == 1) || (code == "EX" && p.len() == 7) || (code == "MR" && p.len() == 4);
    if kenwood_read {
        cx.remember("dir", "emu");
    }
    let emu = emu(cx);
    let read = p.is_empty();
    let c0 = ch(p, 0);
    match code.as_str() {
        "FA" | "FB" => {
            if read {
                return one(f.to_string());
            }
            let Some(hz) = n::<u64>(p) else { return Vec::new() };
            cx.remember(if code == "FA" { "fa" } else { "fb" }, hz.to_string());
            let mut out = one(format!("{code}{hz:08};"));
            if code == "FB" && !emu {
                out.extend(oi_from_memo(cx));
            }
            out
        }
        "MD" => {
            if read {
                return one("MD0;".into());
            }
            if !kw_mode(c0) {
                return Vec::new();
            }
            cx.remember("mode", c0.to_string());
            if cur_b(cx) {
                cx.remember("mb", c0.to_string());
                let a = cx.recall("ma").map(str::to_string).unwrap_or_else(|| c0.to_string());
                let mut out = one(format!("MD0{a};"));
                if !emu {
                    out.extend(oi_from_memo(cx));
                }
                out
            } else {
                cx.remember("ma", c0.to_string());
                one(format!("MD0{c0};"))
            }
        }
        "IF" => {
            if read {
                return one("IF;".into());
            }
            if_up(k, cx, p)
        }
        "FR" | "FT" => {
            if read {
                return one(f.to_string());
            }
            if !matches!(c0, '0'..='3') {
                return Vec::new();
            }
            let b = c0 == '1';
            if code == "FR" {
                cx.remember("fr", c0.to_string());
                if emu {
                    // Kenwood's FR selects the transmit VFO too.
                    cx.remember("ft", c0.to_string());
                    return vec![format!("FR{};", if b { 4 } else { 0 }), format!("FT{};", if b { 3 } else { 2 })];
                }
                one(format!("FR{};", if b { 4 } else { 0 }))
            } else {
                cx.remember("ft", c0.to_string());
                one(format!("FT{};", if emu { if b { 3 } else { 2 } } else { b as u8 }))
            }
        }
        "TX" => {
            cx.remember("tx", "1");
            one("TX1;".into())
        }
        "RX" => {
            cx.remember("tx", "0");
            one("TX0;".into())
        }
        "AI" => match c0 {
            _ if read => one("AI;".into()),
            '0' => one("AI0;".into()),
            '1'..='3' => one("AI1;".into()),
            _ => Vec::new(),
        },
        "PC" if !read => {
            // Out of range: the nearest the radio has (it does so itself).
            let (lo, hi) = cx.model.caps.power;
            one(format!("PC{:03};", n::<u32>(p).unwrap_or(hi as u32).clamp(lo as u32, hi as u32)))
        }
        "ID" | "PS" | "MG" | "PC" | "KS" | "RT" | "XT" | "UL" | "TS" | "BY" | "CH" | "UP" | "DN" | "RC" | "QI" | "RS" => {
            match code.as_str() {
                "PS" if c0 == '9' => one("PS0;".into()),
                "UP" | "DN" => one(format!("{code};")),
                "BY" if !read => one(format!("BY{c0}0;")),
                _ => one(f.to_string()),
            }
        }
        "VV" => one("AB;".into()),
        "BD" | "BU" => one(format!("{code}0;")),
        "AG" | "SQ" => match c0 {
            _ if p.len() <= 1 && c0 != '1' => one(format!("{code}0;")),
            '0' => one(f.to_string()),
            _ => Vec::new(), // the sub-receiver's
        },
        "RG" => {
            if read {
                return one("RG0;".into());
            }
            let Some(v) = n::<u32>(p) else { return Vec::new() };
            let v = if k.rg_max == 255 { v.min(255) } else { scale_up(cx, "rg", v, k.rg_max, 255) };
            one(format!("RG0{v:03};"))
        }
        "AC" => {
            if read {
                return one("AC;".into());
            }
            let v = if ch(p, 2) == '1' {
                2
            } else if ch(p, 0) == '1' || ch(p, 1) == '1' {
                1
            } else {
                0
            };
            one(format!("AC00{v};"))
        }
        "AN" => match c0 {
            _ if read => one("AN0;".into()),
            '1' | '2' => one(format!("AN0{c0};")),
            _ => Vec::new(),
        },
        "BC" => match c0 {
            _ if read => one("BC0;".into()),
            '0' => one("BC00;".into()),
            '1' | '2' => {
                cx.remember("bc", c0.to_string());
                one("BC01;".into())
            }
            _ => Vec::new(),
        },
        "NR" => match c0 {
            _ if read => one("NR0;".into()),
            '0' => one("NR00;".into()),
            '1' | '2' => {
                cx.remember("nr", c0.to_string());
                one("NR01;".into())
            }
            _ => Vec::new(),
        },
        "NB" | "PR" | "PA" => match c0 {
            _ if read => one(format!("{code}0;")),
            '0' | '1' => one(format!("{code}0{c0};")),
            _ => Vec::new(),
        },
        "RA" => {
            if read {
                return one("RA0;".into());
            }
            let on = n::<u32>(p.get(..2).unwrap_or(p)).is_some_and(|v| v != 0);
            one(format!("RA0{};", on as u8))
        }
        "NL" => {
            if read {
                return one("NL0;".into());
            }
            let v = n::<u32>(p).unwrap_or(1).clamp(1, 10);
            one(format!("NL0{v:03};"))
        }
        "RL" => {
            if read {
                return one("RL0;".into());
            }
            let v = n::<u32>(p).unwrap_or(0).min(9);
            one(format!("RL0{v:02};"))
        }
        "GT" => {
            if read {
                return one("GT0;".into());
            }
            match n::<u16>(p) {
                Some(v) => {
                    let g = gt_up(k, v);
                    let s = format!("GT0{g};");
                    cx.remember("gt", s.clone());
                    one(s)
                }
                // FM: no AGC; the last setting.
                None if !emu => one(cx.recall("gt").unwrap_or("GT00;").to_string()),
                None => Vec::new(),
            }
        }
        "SM" => match c0 {
            _ if p.len() <= 1 && c0 == '0' => one("SM0;".into()),
            '0' if !emu => {
                let dots = n::<u32>(&p[1..]).unwrap_or(0).min(k.sm_max) as u8;
                if flag(cx, "tx") {
                    let watts = dots as f32 * k.po_watts / k.sm_max as f32;
                    let po = rescale((watts * 255.0 / 250.0).round() as u8, &[(0, 0.0), (255, 250.0)], &PO_CAL);
                    vec!["SM0000;".into(), "RM1000;".into(), format!("RM5{po:03};")]
                } else {
                    let s = rescale(dots, k.sm_cal, &S_METER_CAL);
                    vec![format!("SM0{s:03};"), format!("RM1{s:03};"), "RM5000;".into()]
                }
            }
            _ => Vec::new(),
        },
        "RM" => {
            if read {
                if k.rm_all {
                    // All three meters, like the radio.
                    return vec!["RM6;".into(), "RM3;".into(), "RM4;".into()];
                }
                // The meter selected with RMx.
                let rh = match cx.recall("ms") {
                    Some("2") => '3',
                    Some("3") => '4',
                    _ => '6',
                };
                return one(format!("RM{rh};"));
            }
            if emu {
                // A program selects the meter.
                return match c0 {
                    '1'..='3' => {
                        cx.remember("ms", c0.to_string());
                        one(format!("MS{c0};"))
                    }
                    _ => Vec::new(),
                };
            }
            rm_up(k, cx, p)
        }
        "MC" => {
            if read {
                return one("MC;".into());
            }
            let p = p.replace(' ', "0");
            match n::<u16>(&p) {
                Some(c) => one(format!("MC{c:03};")),
                None => Vec::new(),
            }
        }
        "MR" => {
            if p.len() == 4 {
                cx.remember("mr_p1", c0.to_string());
                let c = n::<u16>(&p[1..2]).unwrap_or(0) * 100 + n::<u16>(&p[2..4]).unwrap_or(0);
                cx.remember("mr_ch", c.to_string());
                return one(format!("MR{c:03};"));
            }
            if c0 == '1' && !emu {
                return Vec::new(); // the transmit frequency
            }
            mem_up(k, "MR", p).into_iter().collect()
        }
        "MW" => {
            let Some(mw) = mem_up(k, "MW", p) else { return Vec::new() };
            let key = format!("mw{}", &mw[2..5]);
            if c0 == '1' {
                // The transmit frequency: keep the channel as written.
                return one(cx.recall(&key).map(str::to_string).unwrap_or(mw));
            }
            cx.remember(&key, mw.clone());
            one(mw)
        }
        "EX" => {
            let mut out = menu_up(cx, f, 7);
            if !read && p.len() > 7 && p.get(..7).and_then(|a| a.parse::<u32>().ok()) == Some(k.pitch_addr) {
                if let Some(c) = n::<u32>(&p[7..]) {
                    let hz = 400 + 50 * c.min(12);
                    out.push(format!("KP{:02};", (hz - 300) / 10));
                }
            }
            out
        }
        "SH" | "FW" => {
            if read {
                return one("SH0;".into());
            }
            let Some(v) = n::<u32>(p) else { return Vec::new() };
            let class = cur_class(cx);
            match (code.as_str(), class) {
                ("FW", WidthClass::Cw | WidthClass::Rtty) => {
                    let t = if class == WidthClass::Cw { CW_WIDTHS } else { FSK_WIDTHS };
                    let i = t.iter().enumerate().min_by_key(|(_, w)| (**w as i64 - v as i64).abs()).map_or(0, |(i, _)| i);
                    one(format!("SH0{i:02};"))
                }
                ("SH", WidthClass::Ssb | WidthClass::Fixed) => {
                    cx.remember("sh", v.to_string());
                    one(format!("SH0{v:02};"))
                }
                // A program's FW0000 (normal) in SSB: the high cut as it is.
                ("FW", _) if emu => one(format!("SH0{:02};", get::<u32>(cx, "sh").unwrap_or(if class == WidthClass::Ssb { 7 } else { 1 }))),
                _ => Vec::new(),
            }
        }
        "IS" => {
            if read {
                return one("IS0;".into());
            }
            let sign = if c0 == '-' { '-' } else { '+' };
            let v = n::<u32>(p.get(1..).unwrap_or("")).unwrap_or(0).min(9999);
            one(format!("IS0{sign}{v:04};"))
        }
        "OS" => match c0 {
            _ if read => one("OS0;".into()),
            '0'..='3' => one(format!("OS0{};", (c0 as u8 - b'0').min(2))),
            _ => Vec::new(),
        },
        "RU" | "RD" => {
            if p.len() == 1 {
                return Vec::new(); // the scan speed
            }
            let v = n::<u32>(p).unwrap_or(10).min(9999);
            one(format!("{code}{v:04};"))
        }
        "VX" => match c0 {
            _ if read => one("VX;".into()),
            '0' | '1' => vec![format!("VX{c0};"), format!("BI{c0};")],
            _ => Vec::new(),
        },
        "VG" => {
            if read {
                return one("VG;".into());
            }
            let v = scale_up(cx, "vg", n(p).unwrap_or(0), 9, 100);
            one(format!("VG{v:03};"))
        }
        "VD" | "SD" => {
            if read {
                return one(f.to_string());
            }
            let (step, max) = if code == "VD" { (150, 3000) } else { (50, 1000) };
            let v = step_up(cx, &code.to_ascii_lowercase(), n(p).unwrap_or(0), step, max);
            one(format!("{code}{v:04};"))
        }
        "ML" => {
            if read {
                return one("ML0;".into());
            }
            let v = n::<u32>(p).unwrap_or(0).min(9);
            let level = if v == 0 {
                cx.remember("ml_on", "0");
                get::<u32>(cx, "ml").unwrap_or(50)
            } else {
                cx.remember("ml_on", "1");
                cx.remember("ml_k", v.to_string());
                scale_up(cx, "ml", v, 9, 100).max(1)
            };
            vec![format!("ML000{};", (v != 0) as u8), format!("ML1{level:03};")]
        }
        "PL" => {
            if read {
                return one("PL;".into());
            }
            if p.len() >= 6 {
                cx.remember("pl_out", p[3..6].to_string());
            }
            one(format!("PL{};", p.get(..3).unwrap_or("000")))
        }
        "SC" => match c0 {
            _ if read => one("SC;".into()),
            '0' => one("SC0;".into()),
            '1'..='9' => one("SC1;".into()),
            _ => Vec::new(),
        },
        "LK" => {
            if read {
                return one("LK;".into());
            }
            one(format!("LK{};", if c0 != '0' { 7 } else { 4 }))
        }
        "PB" => match c0 {
            _ if read => one("PB0;".into()),
            '0'..='3' => one(format!("PB0{c0};")),
            _ => Vec::new(),
        },
        "TO" | "CT" => {
            if read {
                return one("CT0;".into());
            }
            if !matches!(c0, '0' | '1') {
                return Vec::new();
            }
            let key = if code == "TO" { "to" } else { "ct" };
            cx.remember(key, c0.to_string());
            if c0 == '1' {
                cx.remember(if code == "TO" { "ct" } else { "to" }, "0");
            }
            one(ct_frame(cx))
        }
        "TN" | "CN" => {
            if read {
                return one("CN0;".into());
            }
            let Some(t) = n::<u32>(p) else { return Vec::new() };
            one(format!("CN0{:02};", tone_up(k, t)))
        }
        _ => Vec::new(),
    }
}

/// A RigHelm frame (a Set or Read for the radio, or an Answer for a
/// program) as the radio's.
pub(super) fn down_kw(k: &Kw, cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    let read = is_read(f);
    if read {
        cx.remember("dir", "radio");
    }
    let emu = emu(cx);
    let c1 = ch(p, 1);
    let c0 = ch(p, 0);
    match code.as_str() {
        "FA" | "FB" => {
            if read {
                return one(f.to_string());
            }
            let Some(hz) = n::<u64>(p) else { return Vec::new() };
            cx.remember(if code == "FA" { "fa" } else { "fb" }, hz.to_string());
            one(format!("{code}{hz:011};"))
        }
        "MD" => {
            if read {
                return one("MD;".into());
            }
            let m = if emu {
                if cur_b(cx) {
                    cx.recall("mb").and_then(|m| m.chars().next()).unwrap_or_else(|| nearest_mode(Mode::from_code(c1)))
                } else {
                    nearest_mode(Mode::from_code(c1))
                }
            } else {
                if !kw_mode(c1) {
                    return Vec::new();
                }
                c1
            };
            if !emu || !cur_b(cx) {
                cx.remember(if cur_b(cx) { "mb" } else { "ma" }, m.to_string());
            }
            cx.remember("mode", m.to_string());
            one(format!("MD{m};"))
        }
        "IF" => {
            if read {
                return one("IF;".into());
            }
            if_down(k, cx, p)
        }
        "OI" => {
            if read {
                return one("FB;".into());
            }
            if let Some(info) = ChannelInfo::parse(p) {
                cx.remember("fb", info.freq.to_string());
                if let Some(m) = info.mode {
                    cx.remember("mb", nearest_mode(Some(m)).to_string());
                }
            }
            Vec::new()
        }
        "FR" => match c0 {
            _ if read => one("FR;".into()),
            '0' | '1' | '4' | '5' => {
                let v = if matches!(c0, '4' | '5') { '1' } else { '0' };
                cx.remember("fr", v.to_string());
                if !emu {
                    // The radio selects the transmit VFO with it.
                    cx.remember("ft", v.to_string());
                }
                one(format!("FR{v};"))
            }
            _ => Vec::new(),
        },
        "FT" => match c0 {
            _ if read => one("FT;".into()),
            '0'..='3' => {
                let v = if matches!(c0, '1' | '3') { '1' } else { '0' };
                cx.remember("ft", v.to_string());
                one(format!("FT{v};"))
            }
            _ => Vec::new(),
        },
        "TX" | "MX" => {
            if read {
                return one("IF;".into());
            }
            if emu {
                cx.remember("tx", if c0 == '0' { "0" } else { "1" });
                return Vec::new();
            }
            match c0 {
                '0' => {
                    cx.remember("cattx", "0");
                    cx.remember("mox", "0");
                    one("RX;".into())
                }
                '1' => {
                    cx.remember("cattx", "1");
                    cx.remember("mox", if code == "MX" { "1" } else { "0" });
                    one(if code == "MX" { k.mox } else { k.ptt }.to_string())
                }
                _ => Vec::new(),
            }
        }
        "AI" => match c0 {
            _ if read => one("AI;".into()),
            '0' => one("AI0;".into()),
            '1' => one("AI2;".into()),
            _ => Vec::new(),
        },
        "PC" if !read => {
            let (lo, hi) = cx.model.caps.power;
            one(format!("PC{:03};", n::<u32>(p).unwrap_or(hi as u32).clamp(lo as u32, hi as u32)))
        }
        "ID" | "PS" | "MG" | "PC" | "KS" | "RT" | "XT" | "UL" | "TS" | "CH" | "UP" | "DN" | "RC" | "QI" | "RS" | "BY" => {
            one(f.to_string())
        }
        "AB" if k.has_vv => one("VV;".into()),
        "BD" | "BU" => one(format!("{code};")),
        "AG" | "SQ" => match c0 {
            '0' => one(f.to_string()),
            _ => Vec::new(),
        },
        "RG" => {
            if read {
                return one("RG;".into());
            }
            let Some(v) = n::<u32>(p.get(1..).unwrap_or("")) else { return Vec::new() };
            let v = if k.rg_max == 255 { v.min(255) } else { scale_down(cx, "rg", v, 255, k.rg_max) };
            one(format!("RG{v:03};"))
        }
        "AC" => {
            if read {
                return one("AC;".into());
            }
            match ch(p, 2) {
                '0' => one("AC000;".into()),
                '1' => one("AC110;".into()),
                '2' => one("AC111;".into()),
                _ => Vec::new(),
            }
        }
        "AN" => match c1 {
            _ if read => one("AN;".into()),
            '1' | '2' => one(format!("AN{c1};")),
            _ => Vec::new(),
        },
        "BC" => match c1 {
            _ if read => one("BC;".into()),
            '0' => one("BC0;".into()),
            '1' => one(format!("BC{};", cx.recall("bc").unwrap_or("1"))),
            _ => Vec::new(),
        },
        "NR" => match c1 {
            _ if read => one("NR;".into()),
            '0' => one("NR0;".into()),
            '1' => one(format!("NR{};", cx.recall("nr").unwrap_or("1"))),
            _ => Vec::new(),
        },
        "NB" | "PR" => match c1 {
            _ if read && c0 == '0' => one(format!("{code};")),
            '0' if c0 == '0' => one(format!("{code}0;")),
            '1' | '2' if c0 == '0' => one(format!("{code}1;")),
            _ => Vec::new(),
        },
        "PA" => match c1 {
            _ if read => one("PA;".into()),
            '0' | '1' if emu => one(format!("PA{c1}0;")),
            '0' | '1' => one(format!("PA{c1};")),
            _ => Vec::new(),
        },
        "RA" => match c1 {
            _ if read => one("RA;".into()),
            '0' | '1' if emu => one(format!("RA0{c1}00;")),
            '0' | '1' => one(format!("RA0{c1};")),
            _ => Vec::new(),
        },
        "NL" => {
            if read {
                return one("NL;".into());
            }
            let v = n::<u32>(p.get(1..).unwrap_or("")).unwrap_or(1).clamp(1, 10);
            one(format!("NL{v:03};"))
        }
        "RL" => {
            if read {
                return one("RL;".into());
            }
            let v = n::<u32>(p.get(1..).unwrap_or("")).unwrap_or(0).min(9);
            one(format!("RL{v:02};"))
        }
        "GT" => {
            if read {
                return one("GT;".into());
            }
            let Some(c) = c1.to_digit(10) else { return Vec::new() };
            let c = if emu {
                // Answers: 4-6 are AUTO-FAST/MID/SLOW.
                match c {
                    4 => 1,
                    5 => 2,
                    6 => 3,
                    c => c,
                }
            } else {
                c
            };
            one(format!("GT{:03};", gt_down(k, c as u8)))
        }
        "SM" => {
            if read {
                return if c0 == '0' { one("SM0;".into()) } else { Vec::new() };
            }
            let Some(v) = n::<u32>(p.get(1..).unwrap_or("")) else { return Vec::new() };
            let dots = rescale(v.min(255) as u8, &S_METER_CAL, k.sm_cal) as u32;
            one(format!("SM0{:04};", dots.min(k.sm_max)))
        }
        "RM" => {
            if read {
                return match c0 {
                    '1' | '5' => one("SM0;".into()),
                    '3' | '4' | '6' => one("RM;".into()),
                    _ => Vec::new(),
                };
            }
            // Answers, for a program.
            let Some(v) = n::<u32>(p.get(1..).unwrap_or("")) else { return Vec::new() };
            let raw = match c0 {
                '6' => rescale(v.min(255) as u8, &SWR_CAL, KW_SWR),
                _ => v.min(255) as u8,
            } as u32;
            let dots = (raw * k.rm_max + 127) / 255;
            let sel = match c0 {
                '6' => '1',
                '3' => '2',
                '4' => '3',
                _ => return Vec::new(),
            };
            one(format!("RM{sel}{dots:04};"))
        }
        "MS" => match c0 {
            _ if read => one("RM;".into()),
            '1'..='3' => {
                cx.remember("ms", c0.to_string());
                if emu { Vec::new() } else { one(format!("RM{c0};")) }
            }
            _ => Vec::new(),
        },
        "MC" => {
            if read {
                return one("MC;".into());
            }
            match n::<u16>(p) {
                Some(c) if c <= k.max_ch => one(format!("MC{c:03};")),
                _ => Vec::new(),
            }
        }
        "MR" => {
            if read {
                return match n::<u16>(p) {
                    Some(c) if c <= k.max_ch => one(format!("MR0{c:03};")),
                    _ => Vec::new(),
                };
            }
            // An answer, for a program: the channel and P1 it asked for.
            let Some(mut info) = ChannelInfo::parse(p) else { return Vec::new() };
            let c = get::<u16>(cx, "mr_ch").unwrap_or(info.channel);
            info.channel = c;
            let p1 = cx.recall("mr_p1").and_then(|s| s.chars().next()).unwrap_or('0');
            one(kenwood_mem(k, "MR", p1, c, &info))
        }
        "MW" => {
            let Some(info) = ChannelInfo::parse(p) else { return Vec::new() };
            let c = info.channel;
            if c > k.max_ch {
                return Vec::new();
            }
            let mut out = one(kenwood_mem(k, "MW", '0', c, &info));
            // A simplex channel: the same transmit frequency (the scan
            // channels hold a range instead).
            if c < k.scan_from {
                out.push(kenwood_mem(k, "MW", '1', c, &info));
            }
            out
        }
        "EX" => menu_down(cx, f, 7),
        "KP" => {
            let Some(item) = cx.model.menu.iter().find(|m| m.addr == k.pitch_addr) else { return Vec::new() };
            if read {
                return one(format!("EX{:07};", k.pitch_addr));
            }
            let Some(v) = n::<u32>(p) else { return Vec::new() };
            let hz = 300 + 10 * v;
            let c = ((hz as f64 - 400.0) / 50.0).round().clamp(0.0, 12.0) as u32;
            one(format!("EX{:07}{c:0w$};", k.pitch_addr, w = item.width as usize))
        }
        "SH" => {
            let class = cur_class(cx);
            let fw = matches!(class, WidthClass::Cw | WidthClass::Rtty);
            if read {
                return one(if fw { "FW;" } else { "SH;" }.into());
            }
            let Some(i) = n::<usize>(p.get(1..).unwrap_or("")) else { return Vec::new() };
            let t = if class == WidthClass::Cw { CW_WIDTHS } else { FSK_WIDTHS };
            let hz = t[i.min(t.len() - 1)];
            let sh = i.min(k.ssb_widths.len() - 1);
            match (emu, fw) {
                // For a program: both forms; it gets the one it asked for.
                (true, true) => vec![format!("FW{hz:04};"), "SH00;".into()],
                (true, false) => {
                    cx.remember("sh", sh.to_string());
                    vec![format!("SH{sh:02};"), "FW0000;".into()]
                }
                (false, true) => one(format!("FW{hz:04};")),
                (false, false) => one(format!("SH{sh:02};")),
            }
        }
        "IS" if k.has_is => {
            if read {
                return one("IS;".into());
            }
            let sign = if c1 == '-' { '-' } else { '+' };
            let v = n::<u32>(p.get(2..).unwrap_or("")).unwrap_or(0).min(1100);
            one(format!("IS{sign}{v:04};"))
        }
        "OS" if k.has_os => match c1 {
            _ if read => one("OS;".into()),
            '0'..='2' => one(format!("OS{c1};")),
            _ => Vec::new(),
        },
        "RU" | "RD" => {
            let Some(v) = n::<u32>(p) else { return Vec::new() };
            one(format!("{code}{v:05};"))
        }
        "VX" | "BI" => match c0 {
            _ if read => one("VX;".into()),
            '0' | '1' => one(format!("VX{c0};")),
            _ => Vec::new(),
        },
        "VG" => {
            if read {
                return one("VG;".into());
            }
            let v = scale_down(cx, "vg", n(p).unwrap_or(0), 100, 9);
            one(format!("VG{v:03};"))
        }
        "VD" | "SD" => {
            if read {
                return one(f.to_string());
            }
            let (step, max) = if code == "VD" { (150, 3000) } else { (50, 1000) };
            let v = step_down(cx, &code.to_ascii_lowercase(), n(p).unwrap_or(0), step, max);
            one(format!("{code}{v:04};"))
        }
        "ML" => {
            if read {
                return one("ML;".into());
            }
            let Some(v) = n::<u32>(p.get(1..).unwrap_or("")) else { return Vec::new() };
            let level = get::<u32>(cx, "ml_k").unwrap_or(5);
            match c0 {
                '0' => {
                    cx.remember("ml_on", if v != 0 { "1" } else { "0" });
                    one(if v != 0 { format!("ML{level:03};") } else { "ML000;".into() })
                }
                '1' => {
                    let kv = scale_down(cx, "ml", v, 100, 9).clamp(1, 9);
                    cx.remember("ml_k", kv.to_string());
                    // The level can only be set with the monitor on.
                    one(if flag(cx, "ml_on") { format!("ML{kv:03};") } else { "ML000;".into() })
                }
                _ => Vec::new(),
            }
        }
        "PL" => {
            if read {
                return one("PL;".into());
            }
            let out = cx.recall("pl_out").unwrap_or("050").to_string();
            one(format!("PL{}{out};", p.get(..3).unwrap_or("000")))
        }
        "SC" => match c0 {
            _ if read => one("SC;".into()),
            '0'..='2' if emu && k.sc_answer_2 => one(format!("SC{}0;", (c0 != '0') as u8)),
            '0'..='2' => one(format!("SC{};", (c0 != '0') as u8)),
            _ => Vec::new(),
        },
        "LK" => {
            if read {
                return one("LK;".into());
            }
            let Some(v) = c0.to_digit(10) else { return Vec::new() };
            let on = match v {
                0 | 2 | 4 => false,
                1 | 3 | 5..=7 => true,
                _ => return Vec::new(),
            };
            one(format!("LK{}0;", on as u8))
        }
        "PB" => match c1 {
            _ if read => one("PB;".into()),
            '0'..='3' if c0 == '0' && emu && k.pb_answer_3 => one(format!("PB{c1}00;")),
            '0'..='3' if c0 == '0' => one(format!("PB{c1};")),
            _ => Vec::new(),
        },
        "KY" => match c0 {
            '1'..='3' => one(format!("PB{c0};")),
            _ => Vec::new(),
        },
        "CT" => {
            if read {
                return vec!["TO;".into(), "CT;".into()];
            }
            let (to, ct) = match c1 {
                '0' => ('0', '0'),
                '1' => ('0', '1'),
                '2' => ('1', '0'),
                _ => return Vec::new(),
            };
            cx.remember("to", to.to_string());
            cx.remember("ct", ct.to_string());
            // Off first, then on.
            if to == '1' { vec![format!("CT{ct};"), format!("TO{to};")] } else { vec![format!("TO{to};"), format!("CT{ct};")] }
        }
        "CN" => {
            if read {
                return one(if flag(cx, "to") { "TN;" } else { "CN;" }.into());
            }
            let Some(t) = n::<u32>(p.get(1..).unwrap_or("")) else { return Vec::new() };
            let kt = tone_down(k, t);
            vec![format!("TN{kt:02};"), format!("CN{kt:02};")]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cx() -> Ctx {
        Ctx::new(&TS480)
    }

    fn d(cx: &mut Ctx, f: &str) -> Vec<String> {
        down(cx, f)
    }

    fn u(cx: &mut Ctx, f: &str) -> Vec<String> {
        up(cx, f)
    }

    #[test]
    fn frequencies_modes_and_split() {
        let mut c = cx();
        assert_eq!(d(&mut c, "FA14074000;"), vec!["FA00014074000;"]);
        assert_eq!(d(&mut c, "FA;"), vec!["FA;"]);
        assert_eq!(u(&mut c, "FA00007000000;"), vec!["FA07000000;"]);
        assert_eq!(d(&mut c, "MD02;"), vec!["MD2;"]);
        assert_eq!(d(&mut c, "MD0;"), vec!["MD;"]);
        assert_eq!(u(&mut c, "MD9;"), vec!["MD09;"]);
        assert!(d(&mut c, "MD0C;").is_empty());
        assert_eq!(d(&mut c, "FR4;"), vec!["FR1;"]);
        assert_eq!(d(&mut c, "FT2;"), vec!["FT0;"]);
        assert_eq!(u(&mut c, "FT1;"), vec!["FT1;"]);
        assert_eq!(u(&mut c, "FR2;"), vec!["FR0;"]);
        assert_eq!(d(&mut c, "TX1;"), vec!["TX1;"]);
        assert_eq!(d(&mut c, "MX1;"), vec!["TX0;"]);
        assert_eq!(d(&mut c, "TX0;"), vec!["RX;"]);
        assert_eq!(d(&mut c, "TX;"), vec!["IF;"]);
        assert_eq!(d(&mut c, "AI1;"), vec!["AI2;"]);
        assert_eq!(u(&mut c, "AI2;"), vec!["AI1;"]);
        assert_eq!(d(&mut c, "AB;"), vec!["VV;"]);
    }

    /// A TS-480 `IF` answer from its fields.
    fn kif(freq: &str, clar: &str, rit_xit: &str, ch: &str, tx: &str, mode: &str, func: &str, split: &str, tone: &str, tn: &str) -> String {
        format!("IF{freq}     {clar}{rit_xit}0{ch}{tx}{mode}{func}0{split}{tone}{tn} ;")
    }

    #[test]
    fn if_both_ways() {
        let mut c = cx();
        // 14.195 MHz, RIT +120 on, USB, VFO A, simplex.
        let f = kif("00014195000", "+0120", "10", "00", "0", "2", "0", "0", "0", "00");
        assert_eq!(f.len(), 38);
        let out = u(&mut c, &f);
        assert_eq!(out[0], "IF00014195000+012010200000;");
        for x in ["TX0;", "MX0;", "FR0;", "FT0;", "CT00;"] {
            assert!(out.contains(&x.to_string()), "{x} in {out:?}");
        }
        // Transmitting split, RX A / TX B (CW, 14.025): the IF is VFO B's.
        let out = u(&mut c, &kif("00014025000", "+0000", "00", "00", "1", "3", "0", "1", "0", "00"));
        assert_eq!(out[0], "IF00014195000+000000200000;");
        assert!(out.contains(&"OI00014025000+000000300000;".to_string()), "{out:?}");
        assert!(out.contains(&"TX2;".to_string()) && out.contains(&"FT1;".to_string()));
        // Memory channel 12, FM, 145.000 MHz, CTCSS 100.0 Hz (TS-480 12 = FTDX3000 12).
        let out = u(&mut c, &kif("00145000000", "+0000", "00", "12", "0", "4", "2", "0", "2", "12"));
        assert_eq!(out[0], "IF012145000000+000000411120;");
        assert!(out.contains(&"CT01;".to_string()));
        // For a program.
        let mut p = cx();
        p.remember("dir", "emu");
        assert_eq!(d(&mut p, "IF00014195000+012010200000;"), vec![f]);
        assert_eq!(d(&mut p, "IF012145000000+000000411120;"), vec![kif("00145000000", "+0000", "00", "12", "0", "4", "2", "0", "2", "12")]);
    }

    #[test]
    fn levels_and_switches() {
        let mut c = cx();
        assert_eq!(d(&mut c, "AG0128;"), vec!["AG0128;"]);
        assert_eq!(d(&mut c, "RG0200;"), vec!["RG078;"]);
        assert_eq!(u(&mut c, "RG078;"), vec!["RG0200;"]);
        assert_eq!(u(&mut c, "RG100;"), vec!["RG0255;"]);
        assert_eq!(d(&mut c, "PA01;"), vec!["PA1;"]);
        assert_eq!(u(&mut c, "PA10;"), vec!["PA01;"]);
        assert_eq!(d(&mut c, "RA01;"), vec!["RA01;"]);
        assert_eq!(u(&mut c, "RA0100;"), vec!["RA01;"]);
        assert_eq!(d(&mut c, "GT03;"), vec!["GT002;"]);
        assert_eq!(u(&mut c, "GT001;"), vec!["GT01;"]);
        assert_eq!(u(&mut c, "GT   ;"), vec!["GT01;"]);
        assert_eq!(d(&mut c, "NL0005;"), vec!["NL005;"]);
        assert_eq!(d(&mut c, "RL004;"), vec!["RL04;"]);
        assert_eq!(u(&mut c, "NR2;"), vec!["NR01;"]);
        assert_eq!(d(&mut c, "NR01;"), vec!["NR2;"]);
        assert_eq!(d(&mut c, "VG050;"), vec!["VG005;"]);
        assert_eq!(u(&mut c, "VG005;"), vec!["VG050;"]);
        assert_eq!(d(&mut c, "SD0220;"), vec!["SD0200;"]);
        assert_eq!(u(&mut c, "SD0200;"), vec!["SD0220;"]);
        assert_eq!(d(&mut c, "PL040;"), vec!["PL040050;"]);
        assert_eq!(u(&mut c, "PL030070;"), vec!["PL030;"]);
        assert_eq!(d(&mut c, "PL040;"), vec!["PL040070;"]);
        assert_eq!(d(&mut c, "IS0-0100;"), vec!["IS-0100;"]);
        assert_eq!(u(&mut c, "IS 0300;"), vec!["IS0+0300;"]);
        assert_eq!(d(&mut c, "BI1;"), vec!["VX1;"]);
        assert_eq!(u(&mut c, "VX1;"), vec!["VX1;", "BI1;"]);
        assert_eq!(d(&mut c, "LK1;"), vec!["LK10;"]);
        assert_eq!(u(&mut c, "LK10;"), vec!["LK7;"]);
        assert_eq!(d(&mut c, "AC002;"), vec!["AC111;"]);
        assert_eq!(u(&mut c, "AC110;"), vec!["AC001;"]);
        assert_eq!(d(&mut c, "AN02;"), vec!["AN2;"]);
        assert_eq!(u(&mut c, "AN1;"), vec!["AN01;"]);
        assert_eq!(d(&mut c, "KY2;"), vec!["PB2;"]);
        assert_eq!(d(&mut c, "RU0150;"), vec!["RU00150;"]);
        assert_eq!(u(&mut c, "PC000;"), vec!["PC005;"]);
        assert_eq!(u(&mut c, "PC255;"), vec!["PC100;"]);
    }

    #[test]
    fn monitor_and_tones() {
        let mut c = cx();
        assert_eq!(u(&mut c, "ML004;"), vec!["ML0001;", "ML1044;"]);
        assert_eq!(d(&mut c, "ML1100;"), vec!["ML009;"]);
        assert_eq!(d(&mut c, "ML0000;"), vec!["ML000;"]);
        assert_eq!(d(&mut c, "ML0001;"), vec!["ML009;"]);
        assert_eq!(d(&mut c, "ML0;"), vec!["ML;"]);
        assert_eq!(d(&mut c, "CT02;"), vec!["CT0;", "TO1;"]);
        assert_eq!(d(&mut c, "CT0;"), vec!["TO;", "CT;"]);
        assert_eq!(u(&mut c, "TO0;"), vec!["CT00;"]);
        assert_eq!(u(&mut c, "CT1;"), vec!["CT01;"]);
        // 88.5 Hz: FTDX3000 08, TS-480 08; 254.1: 49 / 41.
        assert_eq!(d(&mut c, "CN008;"), vec!["TN08;", "CN08;"]);
        assert_eq!(d(&mut c, "CN049;"), vec!["TN41;", "CN41;"]);
        assert_eq!(u(&mut c, "CN33;"), vec!["CN041;"]);
    }

    #[test]
    fn meters_and_widths() {
        let mut c = cx();
        assert_eq!(d(&mut c, "SM0;"), vec!["SM0;"]);
        assert_eq!(d(&mut c, "RM5;"), vec!["SM0;"]);
        assert_eq!(d(&mut c, "RM6;"), vec!["RM;"]);
        assert!(d(&mut c, "RM8;").is_empty());
        assert_eq!(u(&mut c, "SM00011;"), vec!["SM0130;", "RM1130;", "RM5000;"]);
        assert_eq!(u(&mut c, "SM00000;")[0], "SM0000;");
        assert_eq!(u(&mut c, "SM00020;")[0], "SM0255;");
        // RM; gives all three meters: SWR 2.0 (8 dots), COMP, ALC.
        assert!(u(&mut c, "RM10008;").contains(&"RM6089;".to_string()));
        assert!(u(&mut c, "RM30010;").contains(&"RM4128;".to_string()));
        assert_eq!(d(&mut c, "SH0;"), vec!["SH;"]);
        assert_eq!(d(&mut c, "SH007;"), vec!["SH07;"]);
        assert_eq!(u(&mut c, "MD3;"), vec!["MD03;"]);
        assert_eq!(d(&mut c, "SH0;"), vec!["FW;"]);
        assert_eq!(d(&mut c, "SH007;"), vec!["FW0500;"]);
        assert_eq!(u(&mut c, "FW0500;"), vec!["SH007;"]);
        assert_eq!(d(&mut c, "KP40;"), vec!["EX034000006;"]);
        assert_eq!(d(&mut c, "KP;"), vec!["EX0340000;"]);
        assert_eq!(u(&mut c, "EX034000006;"), vec!["EX03506;", "KP40;"]);
    }

    #[test]
    fn menu_and_memories() {
        let mut c = cx();
        assert_eq!(d(&mut c, "EX001;"), vec!["EX0000000;"]);
        assert_eq!(d(&mut c, "EX0013;"), vec!["EX00000003;"]);
        assert_eq!(u(&mut c, "EX00000003;"), vec!["EX0013;"]);
        assert_eq!(d(&mut c, "EX0575;"), vec!["EX05600005;"]);
        assert_eq!(d(&mut c, "MC005;"), vec!["MC005;"]);
        assert_eq!(d(&mut c, "MR005;"), vec!["MR0005;"]);
        let mr = format!("MR0005{}2{}{};", "00014074000", "0".repeat(23), "");
        assert_eq!(mr.len(), 42);
        assert_eq!(u(&mut c, &mr), vec!["MR00514074000+000000210000;"]);
        let mw = d(&mut c, "MW00514074000+000000210000;");
        assert_eq!(mw, vec![mr.replace("MR0", "MW0"), mr.replace("MR0", "MW1")]);
        assert_eq!(u(&mut c, &mw[0]), vec!["MW00514074000+000000200000;"]);
    }

    #[test]
    fn for_a_program() {
        let mut p = cx();
        assert_eq!(u(&mut p, "ID;"), vec!["ID;"]);
        assert_eq!(d(&mut p, "ID020;"), vec!["ID020;"]);
        assert_eq!(u(&mut p, "FA;"), vec!["FA;"]);
        assert_eq!(d(&mut p, "FA14074000;"), vec!["FA00014074000;"]);
        assert_eq!(u(&mut p, "TX;"), vec!["TX1;"]);
        assert_eq!(u(&mut p, "RX;"), vec!["TX0;"]);
        assert_eq!(u(&mut p, "FR1;"), vec!["FR4;", "FT3;"]);
        assert_eq!(u(&mut p, "FR0;"), vec!["FR0;", "FT2;"]);
        assert_eq!(d(&mut p, "PA01;"), vec!["PA10;"]);
        assert_eq!(d(&mut p, "RA01;"), vec!["RA0100;"]);
        assert_eq!(d(&mut p, "GT05;"), vec!["GT002;"]);
        assert_eq!(d(&mut p, "SM0130;"), vec!["SM00011;"]);
        assert_eq!(u(&mut p, "RM;"), vec!["RM6;", "RM3;", "RM4;"]);
        assert_eq!(d(&mut p, "RM6089;"), vec!["RM10008;"]);
        assert_eq!(u(&mut p, "MR0005;"), vec!["MR005;"]);
        assert_eq!(d(&mut p, "MR00514074000+000000210000;"), vec![format!("MR0005{}2{};", "00014074000", "0".repeat(23))]);
        assert_eq!(u(&mut p, "EX0340000;"), vec!["EX035;"]);
        assert!(d(&mut p, "TX0;").is_empty());
    }

    #[test]
    fn round_trips() {
        for f in [
            "FA14074000;", "FB07074000;", "FA50313000;", "MD01;", "MD02;", "MD03;", "MD04;", "MD05;", "MD06;", "MD07;",
            "MD09;", "AG0000;", "AG0255;", "SQ0010;", "RG0255;", "RG0000;", "RG0051;", "PC050;", "MG030;", "KS025;",
            "PA00;", "PA01;", "RA00;", "RA01;", "NB00;", "NB01;", "NL0005;", "NR00;", "NR01;", "RL005;", "BC00;",
            "BC01;", "PR00;", "PR01;", "VX0;", "VX1;", "VG000;", "VG100;", "VD0300;", "SD0050;", "GT00;", "GT01;",
            "GT03;", "RT1;", "XT0;", "FT0;", "FT1;", "FR0;", "FR4;", "AN01;", "AN02;", "AC000;", "AC001;", "IS0+0500;",
            "IS0-0200;", "MC012;", "EX0013;", "EX03506;", "EX0575;", "LK4;", "LK7;", "PB01;", "AI0;", "AI1;", "PS1;",
            "TS1;", "CN008;", "CN012;", "SC0;", "SC1;", "KP40;", "ML1050;", "PL050;",
        ] {
            let mut c = cx();
            let native = d(&mut c, f);
            assert!(!native.is_empty(), "{f}");
            let back: Vec<String> = native.iter().flat_map(|n| u(&mut c, n)).collect();
            assert!(back.contains(&f.to_string()), "{f} -> {native:?} -> {back:?}");
        }
    }
}
