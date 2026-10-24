//! Icom IC-7300 (CI-V address 94h) and IC-7300MK2 (B6h), each with its own
//! menu.
//!
//! Source: IC-7300 full manual, section 12 "SET MODE" and section 19
//! "CONTROL COMMAND" (CI-V command table, pp. 19-3 to 19-15).

use crate::dialect::icom::{self, CivSpec, item, AVERAGING, FILL, KEY_FRONT, MOD_INPUT, NOTCH_SW, OFF_ON, PERCENT, SPEED3, TBW, TONE, TOT, TX_DELAY, USB_LINE};
use crate::menu::{Kind, MenuItem};
use crate::model::{Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::types::{Band, Mode};

/// Icom's band stacking codes (`1A 01`) are the `BS` codes.
pub const HF_BANDS: &[Band] = &[
    Band { code: 1, label: "160", lo: 1_800_000, hi: 2_000_000 },
    Band { code: 2, label: "80", lo: 3_500_000, hi: 4_000_000 },
    Band { code: 3, label: "40", lo: 7_000_000, hi: 7_300_000 },
    Band { code: 4, label: "30", lo: 10_100_000, hi: 10_150_000 },
    Band { code: 5, label: "20", lo: 14_000_000, hi: 14_350_000 },
    Band { code: 6, label: "17", lo: 18_068_000, hi: 18_168_000 },
    Band { code: 7, label: "15", lo: 21_000_000, hi: 21_450_000 },
    Band { code: 8, label: "12", lo: 24_890_000, hi: 24_990_000 },
    Band { code: 9, label: "10", lo: 28_000_000, hi: 29_700_000 },
    Band { code: 10, label: "6", lo: 50_000_000, hi: 54_000_000 },
    Band { code: 11, label: "GEN", lo: 30_000, hi: 74_800_000 },
];

/// Modes of Icom HF radios: Icom code and DATA.
pub const HF_MODES: &[(Mode, u8, bool)] = &[
    (Mode::Lsb, 0x00, false),
    (Mode::Usb, 0x01, false),
    (Mode::Cw, 0x03, false),
    (Mode::CwR, 0x07, false),
    (Mode::Am, 0x02, false),
    (Mode::Fm, 0x05, false),
    (Mode::RttyLsb, 0x04, false),
    (Mode::RttyUsb, 0x08, false),
    (Mode::DataLsb, 0x00, true),
    (Mode::DataUsb, 0x01, true),
    (Mode::DataAm, 0x02, true),
    (Mode::DataFm, 0x05, true),
];

/// The same in front-panel order, for [`RadioModel::modes`].
pub const HF_MODE_LIST: &[Mode] = &[
    Mode::Lsb,
    Mode::Usb,
    Mode::Cw,
    Mode::CwR,
    Mode::Am,
    Mode::Fm,
    Mode::RttyLsb,
    Mode::RttyUsb,
    Mode::DataLsb,
    Mode::DataUsb,
    Mode::DataAm,
    Mode::DataFm,
];

/// RigHelm commands an Icom HF radio carries out.
pub const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AM", "BA", "BC", "BD", "BI", "BP00", "BS", "BU", "BY", "CO00", "CO02", "EX", "FA", "FB", "FR", "FT", "GT", "ID", "IF",
    "KM", "KP", "KR", "KS", "KY", "LK", "MA", "MC", "MD", "MG", "ML0", "ML1", "MR", "MW", "MX", "NB", "NL", "NR", "OI", "PA", "PB", "PC", "PL",
    "PR0", "PS", "RA", "RC", "RD", "RG", "RL", "RM", "RT", "RU", "SH", "SM", "SQ", "SV", "TX", "VD", "VG", "VS", "VX", "XT",
];

const TONE_GRP: &str = "SET > Tone Control/TBW";
const FUNC: &str = "SET > Function";
const CONN: &str = "SET > Connectors";
const DISP: &str = "SET > Display";
const TIME: &str = "SET > Time Set";
const SCOPE: &str = "SCOPE > Scope Set";
const AUDIO: &str = "AUDIO > Audio Scope Set";
const KEYER: &str = "KEYER > CW-KEY SET";
const DECODE: &str = "DECODE > RTTY Decode Set";
const SCAN: &str = "SCAN > Scan Set";
const VOICE: &str = "VOICE > Voice TX Set";
const REC: &str = "RECORDER > Recorder Set";
const NB: &str = "NB";
const VOX: &str = "VOX/BK-IN";
/// Front-panel settings with CI-V commands of their own.
pub const PANEL: &str = "FUNCTION";

const BAND_EDGE_BEEP: Kind = Kind::Choice(&["OFF", "ON (Default)", "ON (User)", "ON (User) & TX Limit"]);
const KEY_VOX: Kind = Kind::Choice(&["VOX/BK-IN", "PRESET", "Memory 1", "Memory 2", "Memory 3", "Memory 4"]);
const KEY_TUNE: Kind = Kind::Choice(&["AUTOTUNE", "PRESET", "Memory 1", "Memory 2", "Memory 3", "Memory 4"]);
const KEY_MIC: Kind = Kind::Choice(&[
    "No function",
    "UP",
    "DOWN",
    "UP (VFO: kHz)",
    "DOWN (VFO: kHz)",
    "XFC",
    "VFO/MEMO",
    "BAND UP",
    "BAND DOWN",
    "SPEECH",
    "MODE",
    "Memory 1",
    "Memory 2",
    "Memory 3",
    "Memory 4",
    "TS",
    "MPAD",
    "SPLIT",
    "A/B",
    "TUNER",
]);
pub const NUMBER_STYLE: Kind = Kind::Choice(&["Normal", "190→ANO", "190→ANT", "90→NO", "90→NT"]);
pub const COUNT_UP: Kind = Kind::Coded(&[(1, "M1"), (2, "M2"), (3, "M3"), (4, "M4"), (5, "M5"), (6, "M6"), (7, "M7"), (8, "M8")]);
pub const DOT_DASH: Kind = Kind::Num { min: 28, max: 45, step: 1, unit: "", signed: false, div: 10 };
pub const RISE_TIME: Kind = Kind::Choice(&["2 ms", "4 ms", "6 ms", "8 ms"]);
pub const KEY_TYPE: Kind = Kind::Choice(&["Straight", "Bug", "Paddle"]);
pub const NORMAL_REVERSE: Kind = Kind::Choice(&["Normal", "Reverse"]);
pub const PRE_REC: Kind = Kind::Choice(&["OFF", "5 sec", "10 sec", "15 sec"]);
pub const SKIP_TIME: Kind = Kind::Choice(&["3 sec", "5 sec", "10 sec", "30 sec"]);
pub const NB_DEPTH: Kind = Kind::Steps { max_code: 9, off_at_zero: false, first: 1, step: 1, unit: "" };
pub const VOX_DELAY: Kind = Kind::Num { min: 0, max: 20, step: 1, unit: " s", signed: false, div: 10 };
pub const VOX_VOICE_DELAY: Kind = Kind::Choice(&["OFF", "Short", "Mid", "Long"]);
pub const TUNING_STEP: Kind = Kind::Choice(&["OFF (1 Hz)", "100 Hz", "1 kHz", "5 kHz", "9 kHz", "10 kHz", "12.5 kHz", "20 kHz", "25 kHz"]);
pub const WIDTH3: Kind = Kind::Choice(&["WIDE", "MID", "NAR"]);
pub const DATE: Kind = Kind::Num { min: 20000101, max: 20991231, step: 1, unit: "", signed: false, div: 1 };
pub const CLOCK: Kind = Kind::Num { min: 0, max: 2359, step: 1, unit: "", signed: false, div: 1 };

const RAW: &[MenuItem] = &[
    // SET > Tone Control/TBW. The RX HPF/LPF items (00 01, 00 04, 00 07,
    // 00 10, 00 11) are two numbers in one and are left out.
    item(2, TONE_GRP, "SSB RX Bass", 2, TONE),
    item(3, TONE_GRP, "SSB RX Treble", 2, TONE),
    item(5, TONE_GRP, "AM RX Bass", 2, TONE),
    item(6, TONE_GRP, "AM RX Treble", 2, TONE),
    item(8, TONE_GRP, "FM RX Bass", 2, TONE),
    item(9, TONE_GRP, "FM RX Treble", 2, TONE),
    item(12, TONE_GRP, "SSB TX Bass", 2, TONE),
    item(13, TONE_GRP, "SSB TX Treble", 2, TONE),
    item(14, TONE_GRP, "SSB TBW (WIDE)", 2, TBW),
    item(15, TONE_GRP, "SSB TBW (MID)", 2, TBW),
    item(16, TONE_GRP, "SSB TBW (NAR)", 2, TBW),
    item(196, TONE_GRP, "SSB-D TBW", 2, TBW),
    item(17, TONE_GRP, "AM TX Bass", 2, TONE),
    item(18, TONE_GRP, "AM TX Treble", 2, TONE),
    item(19, TONE_GRP, "FM TX Bass", 2, TONE),
    item(20, TONE_GRP, "FM TX Treble", 2, TONE),
    // SET > Function. FM SPLIT Offset (HF/50M) (00 31, 00 32) are
    // frequencies and are left out.
    item(21, FUNC, "Beep Level", 4, PERCENT),
    item(22, FUNC, "Beep Level Limit", 2, OFF_ON),
    item(23, FUNC, "Beep (Confirmation)", 2, OFF_ON),
    item(24, FUNC, "Band Edge Beep", 2, BAND_EDGE_BEEP),
    item(25, FUNC, "RF/SQL Control", 2, Kind::Choice(&["Auto", "SQL", "RF+SQL"])),
    item(193, FUNC, "MF Band ATT", 2, OFF_ON),
    item(26, FUNC, "TX Delay HF", 2, TX_DELAY),
    item(27, FUNC, "TX Delay 50M", 2, TX_DELAY),
    item(28, FUNC, "TX Delay 70M", 2, TX_DELAY),
    item(29, FUNC, "Time-Out Timer (CI-V)", 2, TOT),
    item(30, FUNC, "Quick SPLIT", 2, OFF_ON),
    item(33, FUNC, "SPLIT LOCK", 2, OFF_ON),
    item(34, FUNC, "[TUNER] Switch", 2, Kind::Choice(&["Manual", "Auto"])),
    item(35, FUNC, "PTT Start", 2, OFF_ON),
    item(36, FUNC, "RTTY Mark Frequency", 2, Kind::Choice(&["1275 Hz", "1615 Hz", "2125 Hz"])),
    item(37, FUNC, "RTTY Shift Width", 2, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz"])),
    item(38, FUNC, "RTTY Keying Polarity", 2, NORMAL_REVERSE),
    item(39, FUNC, "SPEECH Language", 2, Kind::Choice(&["English", "Japanese"])),
    item(40, FUNC, "SPEECH Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(41, FUNC, "S-Level SPEECH", 2, OFF_ON),
    item(42, FUNC, "MODE SPEECH", 2, OFF_ON),
    item(43, FUNC, "SPEECH Level", 4, PERCENT),
    item(44, FUNC, "[SPEECH/LOCK] Switch", 2, Kind::Choice(&["SPEECH/LOCK", "LOCK/SPEECH"])),
    item(45, FUNC, "Lock Function", 2, Kind::Choice(&["MAIN DIAL", "PANEL"])),
    item(46, FUNC, "Memo Pad Quantity", 2, Kind::Choice(&["5", "10"])),
    item(47, FUNC, "MAIN DIAL Auto TS", 2, Kind::Choice(&["OFF", "Low", "High"])),
    item(48, FUNC, "MIC Up/Down Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(49, FUNC, "Quick RIT/∂TX Clear", 2, OFF_ON),
    item(50, FUNC, "[NOTCH] Switch (SSB)", 2, NOTCH_SW),
    item(51, FUNC, "[NOTCH] Switch (AM)", 2, NOTCH_SW),
    item(52, FUNC, "SSB/CW Synchronous Tuning", 2, OFF_ON),
    item(53, FUNC, "CW Normal Side", 2, Kind::Choice(&["LSB", "USB"])),
    item(198, FUNC, "Front Key Customize [VOX/BK-IN]", 2, KEY_VOX),
    item(199, FUNC, "Front Key Customize [AUTOTUNE]", 2, KEY_TUNE),
    item(200, FUNC, "Front Key Customize [▲]", 2, KEY_FRONT),
    item(201, FUNC, "Front Key Customize [▼]", 2, KEY_FRONT),
    item(202, FUNC, "MIC Key Customize [UP]", 2, KEY_MIC),
    item(203, FUNC, "MIC Key Customize [DN]", 2, KEY_MIC),
    item(54, FUNC, "Screen Capture [POWER] SW", 2, OFF_ON),
    item(55, FUNC, "Screen Capture File Type", 2, Kind::Choice(&["PNG", "BMP"])),
    item(56, FUNC, "Keyboard Type", 2, Kind::Choice(&["Ten-key", "Full Keyboard"])),
    item(194, FUNC, "Screen Full Keyboard Layout", 2, Kind::Choice(&["English", "German", "French"])),
    item(57, FUNC, "Calibration Marker", 2, OFF_ON),
    item(58, FUNC, "REF Adjust", 4, PERCENT),
    // SET > Connectors.
    item(59, CONN, "ACC/USB Output Select", 2, Kind::Choice(&["AF", "IF"])),
    item(60, CONN, "ACC/USB AF Output Level", 4, PERCENT),
    item(61, CONN, "ACC/USB AF SQL", 2, Kind::Choice(&["OFF (OPEN)", "ON"])),
    item(62, CONN, "ACC/USB AF Beep/Speech... Output", 2, OFF_ON),
    item(63, CONN, "ACC/USB IF Output Level", 4, PERCENT),
    item(64, CONN, "ACC MOD Level", 4, PERCENT),
    item(65, CONN, "USB MOD Level", 4, PERCENT),
    item(66, CONN, "DATA OFF MOD", 2, MOD_INPUT),
    item(67, CONN, "DATA MOD", 2, MOD_INPUT),
    item(68, CONN, "External Keypad VOICE", 2, OFF_ON),
    item(69, CONN, "External Keypad KEYER", 2, OFF_ON),
    item(70, CONN, "External Keypad RTTY", 2, OFF_ON),
    item(71, CONN, "CI-V Transceive", 2, OFF_ON),
    item(72, CONN, "CI-V USB→REMOTE Transceive Address", 4, Kind::Num { min: 0, max: 223, step: 1, unit: "", signed: false, div: 1 }),
    item(73, CONN, "CI-V Output (for ANT)", 2, OFF_ON),
    // Read only (the radio refuses a write).
    item(74, CONN, "CI-V USB Port", 2, Kind::Choice(&["Link to [REMOTE]", "Unlink from [REMOTE]"])),
    // The command table says 00 = ON, 01 = OFF.
    item(75, CONN, "CI-V USB Echo Back", 2, Kind::Choice(&["ON", "OFF"])),
    item(76, CONN, "USB Serial Function", 2, Kind::Choice(&["CI-V", "RTTY Decode"])),
    item(77, CONN, "RTTY Decode Baud Rate", 2, Kind::Choice(&["4800", "9600", "19200", "38400"])),
    item(78, CONN, "USB SEND", 2, USB_LINE),
    item(79, CONN, "USB Keying (CW)", 2, USB_LINE),
    item(80, CONN, "USB Keying (RTTY)", 2, USB_LINE),
    item(197, CONN, "Inhibit Timer at USB Connection", 2, OFF_ON),
    item(217, CONN, "PTT Port Function", 2, Kind::Choice(&["PTT Input", "PTT Input + SEND Output"])),
    // SET > Display.
    item(81, DISP, "LCD Backlight", 4, PERCENT),
    item(82, DISP, "Display Type", 2, Kind::Choice(&["A", "B"])),
    item(83, DISP, "Display Font", 2, Kind::Choice(&["Basic", "Round"])),
    item(84, DISP, "Meter Peak Hold", 2, OFF_ON),
    item(85, DISP, "Memory Name", 2, OFF_ON),
    item(86, DISP, "MN-Q Popup (MN OFF→ON)", 2, OFF_ON),
    item(87, DISP, "BW Popup (PBT)", 2, OFF_ON),
    item(88, DISP, "BW Popup (FIL)", 2, OFF_ON),
    item(89, DISP, "Screen Saver", 2, Kind::Choice(&["OFF", "15 min", "30 min", "60 min"])),
    item(90, DISP, "Opening Message", 2, OFF_ON),
    item(91, DISP, "My Call (opening message)", 10, Kind::Text { max: 10 }),
    item(92, DISP, "Power ON Check", 2, OFF_ON),
    item(93, DISP, "Display Language", 2, Kind::Choice(&["English", "Japanese"])),
    // SET > Time Set: date as YYYYMMDD, time as HHMM. UTC Offset (00 96)
    // is an offset and a sign and is left out.
    item(94, TIME, "Date (YYYYMMDD)", 8, DATE),
    item(95, TIME, "Time (HHMM)", 4, CLOCK),
    // Scope set screen. The colours (01 04-01 06) are three numbers and
    // the fixed edges (01 12-01 50, 02 04-02 16) frequencies: left out.
    item(97, SCOPE, "Scope during TX (CENTER TYPE)", 2, OFF_ON),
    item(98, SCOPE, "Max Hold", 2, Kind::Choice(&["OFF", "10s Hold", "ON"])),
    item(99, SCOPE, "CENTER Type Display", 2, Kind::Choice(&["Filter Center", "Carrier Point Center", "Carrier Point Center (Abs. Freq.)"])),
    item(100, SCOPE, "Marker Position (Fix Type/Scroll Type)", 2, Kind::Choice(&["Filter Center", "Carrier Point"])),
    item(101, SCOPE, "VBW", 2, Kind::Choice(&["Narrow", "Wide"])),
    item(102, SCOPE, "Averaging", 2, AVERAGING),
    item(103, SCOPE, "Waveform Type", 2, FILL),
    item(107, SCOPE, "Waterfall Display", 2, OFF_ON),
    item(108, SCOPE, "Waterfall Speed", 2, SPEED3),
    item(109, SCOPE, "Waterfall Size (Expand Screen)", 2, Kind::Choice(&["Small", "Mid", "Large"])),
    item(110, SCOPE, "Waterfall Peak Color Level", 2, Kind::Choice(&["Grid 1", "Grid 2", "Grid 3", "Grid 4", "Grid 5", "Grid 6", "Grid 7", "Grid 8"])),
    item(111, SCOPE, "Waterfall Marker Auto-hide", 2, OFF_ON),
    item(151, AUDIO, "FFT Scope Waveform Type", 2, FILL),
    item(153, AUDIO, "FFT Scope Waterfall Display", 2, OFF_ON),
    // Keyer set.
    item(155, KEYER, "Contest Number Style", 2, NUMBER_STYLE),
    item(156, KEYER, "Count Up Trigger Channel", 2, COUNT_UP),
    item(157, KEYER, "Present Number", 4, Kind::Num { min: 1, max: 9999, step: 1, unit: "", signed: false, div: 1 }),
    item(158, KEYER, "Side Tone Level", 4, PERCENT),
    item(159, KEYER, "Side Tone Level Limit", 2, OFF_ON),
    item(160, KEYER, "Keyer Repeat Time", 2, Kind::Num { min: 1, max: 60, step: 1, unit: "s", signed: false, div: 1 }),
    item(161, KEYER, "Dot/Dash Ratio (1:1:x)", 2, DOT_DASH),
    item(162, KEYER, "Rise Time", 2, RISE_TIME),
    item(163, KEYER, "Paddle Polarity", 2, NORMAL_REVERSE),
    item(164, KEYER, "Key Type", 2, KEY_TYPE),
    item(165, KEYER, "MIC Up/Down Keyer", 2, OFF_ON),
    // RTTY decode set. The colours (01 67, 01 71, 01 72) are left out.
    item(166, DECODE, "FFT Scope Averaging", 2, AVERAGING),
    item(168, DECODE, "Decode USOS", 2, OFF_ON),
    item(169, DECODE, "Decode New Line Code", 2, Kind::Choice(&["CR,LF,CR+LF", "CR+LF"])),
    item(170, DECODE, "TX USOS", 2, OFF_ON),
    item(173, DECODE, "Log", 2, OFF_ON),
    item(174, DECODE, "Log File Type", 2, Kind::Choice(&["Text", "HTML"])),
    item(175, DECODE, "Time Stamp", 2, OFF_ON),
    item(176, DECODE, "Time Stamp (Time)", 2, Kind::Choice(&["Local", "UTC"])),
    item(177, DECODE, "Time Stamp (Frequency)", 2, OFF_ON),
    item(178, SCAN, "Scan Speed", 2, Kind::Choice(&["Low", "High"])),
    item(179, SCAN, "Scan Resume", 2, OFF_ON),
    item(180, VOICE, "Auto Monitor", 2, OFF_ON),
    item(181, VOICE, "Repeat Time", 2, Kind::Num { min: 1, max: 15, step: 1, unit: "s", signed: false, div: 1 }),
    item(195, VOICE, "TX Level", 4, PERCENT),
    item(182, REC, "REC Mode", 2, Kind::Choice(&["TX&RX", "RX Only"])),
    item(183, REC, "TX REC Audio", 2, Kind::Choice(&["Microphone", "TX Monitor"])),
    item(184, REC, "RX REC Condition", 2, Kind::Choice(&["Always", "Squelch Auto"])),
    item(185, REC, "File Split", 2, OFF_ON),
    item(186, REC, "PTT Auto REC", 2, OFF_ON),
    item(187, REC, "PRE-REC for PTT Auto REC", 2, PRE_REC),
    item(188, REC, "QSO Player Skip Time", 2, SKIP_TIME),
    item(189, NB, "NB Depth", 2, NB_DEPTH),
    item(190, NB, "NB Width", 4, PERCENT),
    item(191, VOX, "VOX Delay", 2, VOX_DELAY),
    item(192, VOX, "VOX Voice Delay", 2, VOX_VOICE_DELAY),
    // Front-panel settings (their own CI-V commands, see SPEC.extra).
    item(9001, PANEL, "IP+", 2, OFF_ON),
    item(9002, PANEL, "Filter Shape (DSP)", 2, Kind::Choice(&["SHARP", "SOFT"])),
    item(9003, PANEL, "Manual Notch Width", 2, WIDTH3),
    item(9004, PANEL, "SSB TX Bandwidth", 2, WIDTH3),
    item(9005, PANEL, "Twin Peak Filter (RTTY)", 2, OFF_ON),
    item(9006, PANEL, "TX Inhibit", 2, OFF_ON),
    item(9007, PANEL, "Tuning Step", 2, TUNING_STEP),
    item(9008, PANEL, "AGC Time Constant (0 = OFF)", 2, Kind::Num { min: 0, max: 13, step: 1, unit: "", signed: false, div: 1 }),
    item(9009, PANEL, "Manual Notch Position", 4, PERCENT),
    item(9010, PANEL, "Twin PBT Inner", 4, PERCENT),
    item(9011, PANEL, "Twin PBT Outer", 4, PERCENT),
    item(9012, PANEL, "BK-IN Delay (2.0-13.0 dots)", 4, PERCENT),
    item(9013, PANEL, "Anti VOX", 4, PERCENT),
    item(9014, PANEL, "Display Brightness", 4, PERCENT),
];

pub static MENU: [MenuItem; RAW.len()] = icom::numbered::<{ RAW.len() }>(RAW);

/// Front-panel settings in the menu, and their commands.
pub const EXTRA: &[(u32, &[u8])] = &[
    (9001, &[0x1A, 0x07]),
    (9002, &[0x16, 0x56]),
    (9003, &[0x16, 0x57]),
    (9004, &[0x16, 0x58]),
    (9005, &[0x16, 0x4F]),
    (9006, &[0x16, 0x66]),
    (9007, &[0x10]),
    (9008, &[0x1A, 0x04]),
    (9009, &[0x14, 0x0D]),
    (9010, &[0x14, 0x07]),
    (9011, &[0x14, 0x08]),
    (9012, &[0x14, 0x0F]),
    (9013, &[0x14, 0x17]),
    (9014, &[0x14, 0x19]),
];

pub static SPEC: CivSpec = CivSpec {
    modes: HF_MODES,
    att: &[(0, 0x00), (1, 0x20)],
    max_watts: 100.0,
    s_cal: &icom::S_CAL,
    po_cal: &icom::PO_CAL,
    swr_cal: &icom::SWR_CAL,
    alc_cal: &icom::ALC_CAL,
    comp_cal: &icom::COMP_CAL,
    id_cal: &icom::ID_CAL,
    vd_cal: &icom::VD_CAL,
    transceive: Some(71),
    keyer_memories: 8,
    voice_memories: 8,
    antenna_switch: false,
    apf: true,
    lock: true,
    memory_name_len: 10,
    vox_delay: Some(191),
    keyer_type: Some(164),
    extra: EXTRA,
};

pub static IC7300: RadioModel = RadioModel {
    key: "ic7300",
    name: "IC-7300",
    maker: Maker::Icom,
    family: Family::Icom,
    proto: Proto::CiV(&SPEC),
    id: "",
    civ_addr: 0x94,
    hamlib: 3073,
    freq_min: 30_000,
    freq_max: 74_800_000,
    bands: HF_BANDS,
    modes: HF_MODE_LIST,
    menu: &MENU,
    roles: Roles {
        mic_sel: &[icom::num_of(&MENU, 66), icom::num_of(&MENU, 67)],
        mic_front: 0,
        mic_usb: 3,
        pc_keying: Some((icom::num_of(&MENU, 79), 0, 2)),
        cat_tot: None,
        cat_items: &[icom::num_of(&MENU, 71), icom::num_of(&MENU, 72), icom::num_of(&MENU, 74), icom::num_of(&MENU, 75), icom::num_of(&MENU, 76)],
        cw_bfo: Some((icom::num_of(&MENU, 53), ["01", "00", "--"])),
        cw_pitch_display: None,
        data_other: None,
        data_disp: None,
    },
    caps: Caps {
        commands: COMMANDS,
        widths: icom::widths,
        narrow: false,
        preamp: &[(0, "OFF"), (1, "P.AMP1"), (2, "P.AMP2")],
        att: &[(0, "OFF"), (1, "20 dB")],
        agc: &[(1, "FAST"), (2, "MID"), (3, "SLOW")],
        roofing: &[],
        roofing_answers: &[],
        nb: &[(0, "OFF"), (1, "NB")],
        nr_max: 15,
        nb_level_max: 100,
        antennas: 1,
        power: (2, 100),
        meter_select: &[],
        voice_memories: 8,
        keyer_memories: 8,
        keyer_memory_len: 70,
        memory: Memory { first: 1, last: 101, pms: Some(100) },
        shared_clarifier: true,
    },
    bauds: &[115200, 57600, 38400, 19200, 9600, 4800],
    stop_bits: 1,
    rts_cts: false,
    sim: &["SH028;", "GT02;"],
};

pub static IC7300MK2: RadioModel = RadioModel { key: "ic7300mk2", name: "IC-7300MK2", civ_addr: 0xB6, ..IC7300 };
