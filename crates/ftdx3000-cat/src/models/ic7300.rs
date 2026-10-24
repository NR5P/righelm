//! Icom IC-7300 (CI-V address 94h) and IC-7300MK2 (B6h), each with its own
//! menu.
//!
//! Source: IC-7300 full manual, section 12 "SET MODE" and section 19
//! "CONTROL COMMAND" (CI-V command table, pp. 19-3 to 19-15).

use crate::dialect::icom::{self, CivSpec, MemLayout, item, AVERAGING, FILL, KEY_FRONT, MOD_INPUT, NOTCH_SW, OFF_ON, PERCENT, SPEED3, TBW, TONE, TOT, TX_DELAY, USB_LINE};
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
    "AB", "AC", "AG", "AI", "AM", "BA", "BC", "BD", "BI", "BP00", "BS", "BU", "BY", "CN", "CT", "EX", "FA", "FB", "FR", "FT", "GT", "ID", "IF",
    "KM", "KP", "KR", "KS", "KY", "LK", "MA", "MC", "MD", "MG", "ML0", "ML1", "MR", "MW", "MX", "NB", "NL", "NR", "OI", "PA", "PB", "PC", "PL",
    "PR0", "PS", "RA", "RC", "RD", "RG", "RL", "RM", "RT", "RU", "SH", "SM", "SQ", "SV", "TX", "VD", "VG", "VS", "VX", "XT",
];

pub const TONE_GRP: &str = "SET > Tone Control/TBW";
pub const FUNC: &str = "SET > Function";
pub const CONN: &str = "SET > Connectors";
pub const DISP: &str = "SET > Display";
pub const TIME: &str = "SET > Time Set";
pub const SCOPE: &str = "SCOPE > Scope Set";
pub const AUDIO: &str = "AUDIO > Audio Scope Set";
pub const KEYER: &str = "KEYER > CW-KEY SET";
pub const DECODE: &str = "DECODE > RTTY Decode Set";
pub const SCAN: &str = "SCAN > Scan Set";
pub const VOICE: &str = "VOICE > Voice TX Set";
pub const REC: &str = "RECORDER > Recorder Set";
pub const NB: &str = "NB";
pub const VOX: &str = "VOX/BK-IN";
/// Front-panel settings with CI-V commands of their own.
pub const PANEL: &str = "FUNCTION";

pub const BAND_EDGE_BEEP: Kind = Kind::Choice(&["OFF", "ON (Default)", "ON (User)", "ON (User) & TX Limit"]);
pub const KEY_VOX: Kind = Kind::Choice(&["VOX/BK-IN", "PRESET", "Memory 1", "Memory 2", "Memory 3", "Memory 4"]);
pub const KEY_TUNE: Kind = Kind::Choice(&["AUTOTUNE", "PRESET", "Memory 1", "Memory 2", "Memory 3", "Memory 4"]);
pub const KEY_MIC: Kind = Kind::Choice(&[
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
    apf: false,
    apf_widths: false,
    apf_hz: 1270.0,
    lock: true,
    memory_name_len: 10,
    memory: MemLayout::Hf { split_copy: true },
    main_sub: false,
    antenna_rx: false,
    dual_band: false,
    tone_function: false,
    duplex: false,
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

// ------------------------------------------------------------ IC-7300MK2
//
// Source: IC-7300MK2 CI-V Reference Guide (command table pp. 4-15, command
// formats pp. 16-26). Its menu differs from the IC-7300's: other item
// numbers, two USB serial ports (A and B), LAN, NTP, external display.

/// RigHelm commands the IC-7300MK2 carries out (the IC-7300's and APF).
pub const MK2_COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AM", "BA", "BC", "BD", "BI", "BP00", "BS", "BU", "BY", "CN", "CO00", "CO02", "CT", "EX", "FA", "FB", "FR", "FT", "GT", "ID", "IF",
    "KM", "KP", "KR", "KS", "KY", "LK", "MA", "MC", "MD", "MG", "ML0", "ML1", "MR", "MW", "MX", "NB", "NL", "NR", "OI", "PA", "PB", "PC", "PL",
    "PR0", "PS", "RA", "RC", "RD", "RG", "RL", "RM", "RT", "RU", "SH", "SM", "SQ", "SV", "TX", "VD", "VG", "VS", "VX", "XT",
];

pub const MK2_TONE_RX: &str = "SET > Tone Control/TBW > RX";
pub const MK2_TONE_TX: &str = "SET > Tone Control/TBW > TX";
pub const MK2_FUNC: &str = "SET > Function";
pub const MK2_USB_OUT: &str = "SET > Connectors > USB AF/IF Output";
pub const MK2_ACC_OUT: &str = "SET > Connectors > ACC AF/IF Output";
pub const MK2_LAN_OUT: &str = "SET > Connectors > LAN AF/IF Output";
pub const MK2_MOD: &str = "SET > Connectors > MOD Input";
pub const MK2_KEYPAD: &str = "SET > Connectors > External Keypad";
pub const MK2_CIV: &str = "SET > Connectors > CI-V";
pub const MK2_CONN: &str = "SET > Connectors";
pub const MK2_USB_KEY: &str = "SET > Connectors > USB SEND/Keying";
pub const MK2_NET: &str = "SET > Network";
pub const MK2_REMOTE: &str = "SET > Network > Remote Settings";
pub const MK2_DISP: &str = "SET > Display";
pub const MK2_EXT_DISP: &str = "SET > Display > External Display";
pub const MK2_TIME: &str = "SET > Time Set";
pub const MK2_KEYER001: &str = "KEYER > EDIT/SET > 001 SET";
pub const MK2_CWKEY: &str = "SET > CW-KEY Set";
pub const MK2_CW_DECODE: &str = "KEYER > Decode Set";
pub const MK2_RTTY001: &str = "RTTY DECODE > TX MEM > 001 SET";
pub const MK2_RTTY_LOG: &str = "RTTY DECODE > Log Set";
pub const MK2_REC: &str = "RECORD > Recorder Set";
pub const MK2_PLAYER: &str = "RECORD > Player Set";
pub const MK2_MENU_SET: &str = "MENU SET";
pub const MK2_APF: &str = "APF";

/// SSB TBW: the low digit 0-5 = 100, 120, 150, 200, 300, 500 Hz.
pub const MK2_TBW: Kind = Kind::Coded(&[
    (0, "100-2500 Hz"),
    (1, "120-2500 Hz"),
    (2, "150-2500 Hz"),
    (3, "200-2500 Hz"),
    (4, "300-2500 Hz"),
    (5, "500-2500 Hz"),
    (10, "100-2700 Hz"),
    (11, "120-2700 Hz"),
    (12, "150-2700 Hz"),
    (13, "200-2700 Hz"),
    (14, "300-2700 Hz"),
    (15, "500-2700 Hz"),
    (20, "100-2800 Hz"),
    (21, "120-2800 Hz"),
    (22, "150-2800 Hz"),
    (23, "200-2800 Hz"),
    (24, "300-2800 Hz"),
    (25, "500-2800 Hz"),
    (30, "100-2900 Hz"),
    (31, "120-2900 Hz"),
    (32, "150-2900 Hz"),
    (33, "200-2900 Hz"),
    (34, "300-2900 Hz"),
    (35, "500-2900 Hz"),
]);
pub const MK2_KEY_VOX: Kind = Kind::Choice(&["VOX/BK-IN", "RX-ANT", "APF", "PRESET", "Memory 1", "Memory 2", "Memory 3", "Memory 4"]);
pub const MK2_KEY_TUNE: Kind = Kind::Choice(&["AUTOTUNE", "RX-ANT", "APF", "PRESET", "Memory 1", "Memory 2", "Memory 3", "Memory 4"]);
pub const MK2_KEY_UPDN: Kind = Kind::Choice(&["M-CH UP", "M-CH DOWN", "RX-ANT", "APF", "PRESET", "Memory 1", "Memory 2", "Memory 3", "Memory 4"]);
pub const MK2_MOD_INPUT: Kind = Kind::Choice(&["MIC", "USB", "ACC", "MIC,USB", "MIC,ACC", "LAN"]);
pub const MK2_USB_LINE: Kind = Kind::Choice(&["OFF", "USB (A) DTR", "USB (A) RTS", "USB (B) DTR", "USB (B) RTS"]);
pub const AF_IF: Kind = Kind::Choice(&["AF", "IF"]);
pub const AF_SQL: Kind = Kind::Choice(&["OFF (OPEN)", "ON"]);
pub const PORT: Kind = Kind::Num { min: 1, max: 65535, step: 1, unit: "", signed: false, div: 1 };
pub const MK2_DATE: Kind = Kind::Num { min: 20200101, max: 20991231, step: 1, unit: "", signed: false, div: 1 };

const RAW_MK2: &[MenuItem] = &[
    // The RX HPF/LPF items (00 01, 00 04, 00 07, 00 10, 00 11) are two
    // numbers in one and are left out.
    item(2, MK2_TONE_RX, "SSB RX Bass", 2, TONE),
    item(3, MK2_TONE_RX, "SSB RX Treble", 2, TONE),
    item(5, MK2_TONE_RX, "AM RX Bass", 2, TONE),
    item(6, MK2_TONE_RX, "AM RX Treble", 2, TONE),
    item(8, MK2_TONE_RX, "FM RX Bass", 2, TONE),
    item(9, MK2_TONE_RX, "FM RX Treble", 2, TONE),
    item(12, MK2_TONE_TX, "SSB TX Bass", 2, TONE),
    item(13, MK2_TONE_TX, "SSB TX Treble", 2, TONE),
    item(14, MK2_TONE_TX, "SSB TBW (WIDE)", 2, MK2_TBW),
    item(15, MK2_TONE_TX, "SSB TBW (MID)", 2, MK2_TBW),
    item(16, MK2_TONE_TX, "SSB TBW (NAR)", 2, MK2_TBW),
    item(17, MK2_TONE_TX, "SSB-D TBW", 2, MK2_TBW),
    item(18, MK2_TONE_TX, "AM TX Bass", 2, TONE),
    item(19, MK2_TONE_TX, "AM TX Treble", 2, TONE),
    item(20, MK2_TONE_TX, "FM TX Bass", 2, TONE),
    item(21, MK2_TONE_TX, "FM TX Treble", 2, TONE),
    // FM SPLIT Offset (HF/50M) (00 34, 00 35) are frequencies: left out.
    item(22, MK2_FUNC, "Beep Level", 4, PERCENT),
    item(23, MK2_FUNC, "Beep Level Limit", 2, OFF_ON),
    item(24, MK2_FUNC, "Beep (Confirmation)", 2, OFF_ON),
    item(25, MK2_FUNC, "Band Edge Beep", 2, BAND_EDGE_BEEP),
    item(26, MK2_FUNC, "RF/SQL Control", 2, Kind::Choice(&["Auto", "SQL", "RF+SQL"])),
    item(27, MK2_FUNC, "Cancel CI-V Remote Set Levels", 2, Kind::Choice(&["All Volume Levels", "Operated Volume Level"])),
    item(28, MK2_FUNC, "MF Band ATT", 2, OFF_ON),
    item(29, MK2_FUNC, "TX Delay HF", 2, TX_DELAY),
    item(30, MK2_FUNC, "TX Delay 50M", 2, TX_DELAY),
    item(31, MK2_FUNC, "TX Delay 70M", 2, TX_DELAY),
    item(32, MK2_FUNC, "Time-Out Timer (CI-V)", 2, TOT),
    item(33, MK2_FUNC, "Quick SPLIT", 2, OFF_ON),
    item(36, MK2_FUNC, "SPLIT LOCK", 2, OFF_ON),
    item(37, MK2_FUNC, "[TUNER] Switch", 2, Kind::Choice(&["Manual", "Auto"])),
    item(38, MK2_FUNC, "PTT Start", 2, OFF_ON),
    item(39, MK2_FUNC, "RTTY Mark Frequency", 2, Kind::Choice(&["1275 Hz", "1615 Hz", "2125 Hz"])),
    item(40, MK2_FUNC, "RTTY Shift Width", 2, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz"])),
    item(41, MK2_FUNC, "RTTY Keying Polarity", 2, NORMAL_REVERSE),
    item(42, MK2_FUNC, "SPEECH Language", 2, Kind::Choice(&["English", "Japanese"])),
    item(43, MK2_FUNC, "SPEECH Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(44, MK2_FUNC, "S-Level SPEECH", 2, OFF_ON),
    item(45, MK2_FUNC, "MODE SPEECH", 2, OFF_ON),
    item(46, MK2_FUNC, "SPEECH Level", 4, PERCENT),
    item(47, MK2_FUNC, "[SPEECH/LOCK] Switch", 2, Kind::Choice(&["SPEECH/LOCK", "LOCK/SPEECH"])),
    item(48, MK2_FUNC, "Lock Function", 2, Kind::Choice(&["MAIN DIAL", "PANEL"])),
    item(49, MK2_FUNC, "Memo Pad Quantity", 2, Kind::Choice(&["5", "10"])),
    item(50, MK2_FUNC, "MAIN DIAL Auto TS", 2, Kind::Choice(&["OFF", "Low", "High"])),
    item(51, MK2_FUNC, "MIC Up/Down Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(52, MK2_FUNC, "Quick RIT/∂TX Clear", 2, OFF_ON),
    item(53, MK2_FUNC, "[NOTCH] Switch (SSB)", 2, NOTCH_SW),
    item(54, MK2_FUNC, "[NOTCH] Switch (AM)", 2, NOTCH_SW),
    item(55, MK2_FUNC, "SSB/CW Synchronous Tuning", 2, OFF_ON),
    item(56, MK2_FUNC, "CW Normal Side", 2, Kind::Choice(&["LSB", "USB"])),
    item(57, MK2_FUNC, "Front Key Customize [VOX/BK-IN]", 2, MK2_KEY_VOX),
    item(58, MK2_FUNC, "Front Key Customize [AUTOTUNE]", 2, MK2_KEY_TUNE),
    item(59, MK2_FUNC, "Front Key Customize [▲]", 2, MK2_KEY_UPDN),
    item(60, MK2_FUNC, "Front Key Customize [▼]", 2, MK2_KEY_UPDN),
    item(61, MK2_FUNC, "MIC Key Customize [UP]", 2, KEY_MIC),
    item(62, MK2_FUNC, "MIC Key Customize [DN]", 2, KEY_MIC),
    item(63, MK2_FUNC, "Screen Capture [POWER] SW", 2, OFF_ON),
    item(64, MK2_FUNC, "Screen Capture File Type", 2, Kind::Choice(&["PNG", "BMP"])),
    item(65, MK2_FUNC, "Keyboard Type", 2, Kind::Choice(&["Ten-key", "Full Keyboard"])),
    item(66, MK2_FUNC, "Full Keyboard Layout", 2, Kind::Choice(&["English", "German", "French"])),
    item(67, MK2_FUNC, "Calibration Marker", 2, OFF_ON),
    item(68, MK2_FUNC, "REF Adjust", 4, PERCENT),
    item(69, MK2_USB_OUT, "Output Select", 2, AF_IF),
    item(70, MK2_USB_OUT, "AF Output Level", 4, PERCENT),
    item(71, MK2_USB_OUT, "AF SQL", 2, AF_SQL),
    item(72, MK2_USB_OUT, "AF Beep/Speech... Output", 2, OFF_ON),
    item(73, MK2_USB_OUT, "IF Output Level", 4, PERCENT),
    item(74, MK2_ACC_OUT, "Output Select", 2, AF_IF),
    item(75, MK2_ACC_OUT, "AF Output Level", 4, PERCENT),
    item(76, MK2_ACC_OUT, "AF SQL", 2, AF_SQL),
    item(77, MK2_ACC_OUT, "AF Beep/Speech... Output", 2, OFF_ON),
    item(78, MK2_ACC_OUT, "IF Output Level", 4, PERCENT),
    item(79, MK2_LAN_OUT, "Output Select", 2, AF_IF),
    item(80, MK2_LAN_OUT, "AF SQL", 2, AF_SQL),
    item(81, MK2_MOD, "USB MOD Level", 4, PERCENT),
    item(82, MK2_MOD, "ACC MOD Level", 4, PERCENT),
    item(83, MK2_MOD, "LAN MOD Level", 4, PERCENT),
    item(84, MK2_MOD, "DATA OFF MOD", 2, MK2_MOD_INPUT),
    item(85, MK2_MOD, "DATA MOD", 2, MK2_MOD_INPUT),
    item(86, MK2_KEYPAD, "VOICE", 2, OFF_ON),
    item(87, MK2_KEYPAD, "KEYER", 2, OFF_ON),
    item(88, MK2_KEYPAD, "RTTY", 2, OFF_ON),
    item(89, MK2_CIV, "CI-V Transceive", 2, OFF_ON),
    item(90, MK2_CIV, "USB/LAN→REMOTE Transceive Address", 4, Kind::Num { min: 0, max: 223, step: 1, unit: "", signed: false, div: 1 }),
    item(91, MK2_CIV, "CI-V Output (for ANT)", 2, OFF_ON),
    item(92, MK2_CIV, "CI-V USB (A) Echo Back", 2, OFF_ON),
    item(93, MK2_CIV, "CI-V USB (B) Echo Back", 2, OFF_ON),
    item(94, MK2_CONN, "USB (B) Function", 2, Kind::Choice(&["RTTY Decode", "CI-V"])),
    item(95, MK2_CONN, "SEND Relay Output", 2, OFF_ON),
    item(96, MK2_USB_KEY, "USB SEND", 2, MK2_USB_LINE),
    item(97, MK2_USB_KEY, "USB Keying (CW)", 2, MK2_USB_LINE),
    item(98, MK2_USB_KEY, "USB Keying (RTTY)", 2, MK2_USB_LINE),
    item(99, MK2_CONN, "PTT Port Function", 2, Kind::Choice(&["PTT Input", "PTT Input + SEND Output"])),
    // IP addresses (01 01, 01 02, 01 04-01 06) are four numbers: left out.
    item(100, MK2_NET, "DHCP (valid after restart)", 2, OFF_ON),
    item(103, MK2_NET, "Subnet Mask (bits, valid after restart)", 2, Kind::Num { min: 1, max: 30, step: 1, unit: "", signed: false, div: 1 }),
    item(107, MK2_NET, "Network Name", 15, Kind::Text { max: 15 }),
    item(108, MK2_REMOTE, "Network Control (valid after restart)", 2, OFF_ON),
    item(109, MK2_REMOTE, "Power OFF Setting (for Remote Control)", 2, Kind::Choice(&["Only Shutdown", "Standby/Shutdown"])),
    item(110, MK2_REMOTE, "Control Port (UDP)", 6, PORT),
    item(111, MK2_REMOTE, "Serial Port (UDP)", 6, PORT),
    item(112, MK2_REMOTE, "Audio Port (UDP)", 6, PORT),
    item(113, MK2_REMOTE, "Internet Access Line", 2, Kind::Choice(&["FTTH", "ADSL/CATV"])),
    item(114, MK2_REMOTE, "Network Radio Name", 16, Kind::Text { max: 16 }),
    item(115, MK2_DISP, "LCD Backlight", 4, PERCENT),
    item(116, MK2_DISP, "Display Type", 2, Kind::Choice(&["A (Black background)", "B (Blue background)"])),
    item(117, MK2_DISP, "Display Font", 2, Kind::Choice(&["Square", "Round"])),
    item(118, MK2_DISP, "Meter Peak Hold", 2, OFF_ON),
    item(119, MK2_DISP, "Memory Name", 2, OFF_ON),
    item(120, MK2_DISP, "MN-Q Popup (MN OFF→ON)", 2, OFF_ON),
    item(121, MK2_DISP, "BW Popup (PBT)", 2, OFF_ON),
    item(122, MK2_DISP, "BW Popup (FIL)", 2, OFF_ON),
    item(123, MK2_DISP, "Screen Saver", 2, Kind::Choice(&["OFF", "15 min", "30 min", "60 min"])),
    item(124, MK2_EXT_DISP, "External Display", 2, OFF_ON),
    item(125, MK2_EXT_DISP, "Resolution", 2, Kind::Choice(&["640x480", "1024x768", "1280x720"])),
    item(126, MK2_EXT_DISP, "Audio Output", 2, OFF_ON),
    item(127, MK2_DISP, "Opening Message", 2, OFF_ON),
    item(128, MK2_DISP, "My Call", 10, Kind::Text { max: 10 }),
    item(129, MK2_DISP, "Power ON Check", 2, OFF_ON),
    item(130, MK2_DISP, "Display Language", 2, Kind::Choice(&["English", "Japanese"])),
    item(131, MK2_DISP, "System Language", 2, Kind::Choice(&["English", "Japanese"])),
    // UTC Offset (01 36) is an offset and a sign: left out.
    item(132, MK2_TIME, "Date (YYYYMMDD)", 8, MK2_DATE),
    item(133, MK2_TIME, "Time (HHMM)", 4, CLOCK),
    item(134, MK2_TIME, "NTP Function", 2, OFF_ON),
    item(135, MK2_TIME, "NTP Server Address", 64, Kind::Text { max: 64 }),
    // Colours (01 44-01 46, 02 05, 02 07, 02 29, 02 33, 02 34, 02 49-02 52)
    // and the fixed edges (01 52-02 03) are left out.
    item(137, SCOPE, "Scope during TX (CENTER TYPE)", 2, OFF_ON),
    item(138, SCOPE, "Max Hold", 2, Kind::Choice(&["OFF", "10s Hold", "ON"])),
    item(139, SCOPE, "CENTER Type Display", 2, Kind::Choice(&["Filter Center", "Carrier Point Center", "Carrier Point Center (Abs. Freq.)"])),
    item(140, SCOPE, "Marker Position (Fix Type/Scroll Type)", 2, Kind::Choice(&["Filter Center", "Carrier Point"])),
    item(141, SCOPE, "VBW", 2, Kind::Choice(&["Narrow", "Wide"])),
    item(142, SCOPE, "Averaging", 2, AVERAGING),
    item(143, SCOPE, "Waveform Type", 2, FILL),
    item(147, SCOPE, "Waterfall Display", 2, OFF_ON),
    item(148, SCOPE, "Waterfall Speed", 2, SPEED3),
    item(149, SCOPE, "Waterfall Size (Expand Screen)", 2, Kind::Choice(&["Small", "Mid", "Large"])),
    item(150, SCOPE, "Waterfall Peak Color Level", 2, Kind::Choice(&["Grid 1", "Grid 2", "Grid 3", "Grid 4", "Grid 5", "Grid 6", "Grid 7", "Grid 8"])),
    item(151, SCOPE, "Waterfall Marker Auto-hide", 2, OFF_ON),
    item(204, AUDIO, "FFT Scope Waveform Type", 2, Kind::Choice(&["Line", "Fill"])),
    item(206, AUDIO, "FFT Scope Waterfall Display", 2, OFF_ON),
    item(208, MK2_KEYER001, "Number Style", 2, NUMBER_STYLE),
    item(209, MK2_KEYER001, "Count Up Trigger (M1)", 2, OFF_ON),
    item(210, MK2_KEYER001, "Count Up Trigger (M2)", 2, OFF_ON),
    item(211, MK2_KEYER001, "Count Up Trigger (M3)", 2, OFF_ON),
    item(212, MK2_KEYER001, "Count Up Trigger (M4)", 2, OFF_ON),
    item(213, MK2_KEYER001, "Count Up Trigger (M5)", 2, OFF_ON),
    item(214, MK2_KEYER001, "Count Up Trigger (M6)", 2, OFF_ON),
    item(215, MK2_KEYER001, "Count Up Trigger (M7)", 2, OFF_ON),
    item(216, MK2_KEYER001, "Count Up Trigger (M8)", 2, OFF_ON),
    item(217, MK2_KEYER001, "Present Number", 4, Kind::Num { min: 1, max: 9999, step: 1, unit: "", signed: false, div: 1 }),
    item(218, MK2_CWKEY, "Side Tone Level", 4, PERCENT),
    item(219, MK2_CWKEY, "Side Tone Level Limit", 2, OFF_ON),
    item(220, MK2_CWKEY, "Keyer Repeat Time", 2, Kind::Num { min: 1, max: 60, step: 1, unit: "s", signed: false, div: 1 }),
    item(221, MK2_CWKEY, "Dot/Dash Ratio (1:1:x)", 2, DOT_DASH),
    item(222, MK2_CWKEY, "Rise Time", 2, RISE_TIME),
    item(223, MK2_CWKEY, "Paddle Polarity", 2, NORMAL_REVERSE),
    item(224, MK2_CWKEY, "Key Type", 2, KEY_TYPE),
    item(225, MK2_CWKEY, "MIC Up/Down Keyer", 2, OFF_ON),
    item(226, MK2_CW_DECODE, "Decode Display", 2, OFF_ON),
    item(227, MK2_CW_DECODE, "Japanese Morse Decode", 2, OFF_ON),
    item(228, DECODE, "FFT Scope Averaging", 2, AVERAGING),
    item(230, DECODE, "Decode USOS", 2, OFF_ON),
    item(231, DECODE, "Decode New Line Code", 2, Kind::Choice(&["CR,LF,CR+LF", "CR+LF"])),
    item(232, DECODE, "TX USOS", 2, OFF_ON),
    item(235, MK2_RTTY001, "Count Up Trigger (RT1)", 2, OFF_ON),
    item(236, MK2_RTTY001, "Count Up Trigger (RT2)", 2, OFF_ON),
    item(237, MK2_RTTY001, "Count Up Trigger (RT3)", 2, OFF_ON),
    item(238, MK2_RTTY001, "Count Up Trigger (RT4)", 2, OFF_ON),
    item(239, MK2_RTTY001, "Count Up Trigger (RT5)", 2, OFF_ON),
    item(240, MK2_RTTY001, "Count Up Trigger (RT6)", 2, OFF_ON),
    item(241, MK2_RTTY001, "Count Up Trigger (RT7)", 2, OFF_ON),
    item(242, MK2_RTTY001, "Count Up Trigger (RT8)", 2, OFF_ON),
    item(243, MK2_RTTY001, "Present Number", 4, Kind::Num { min: 1, max: 9999, step: 1, unit: "", signed: false, div: 1 }),
    item(244, MK2_RTTY_LOG, "Decode Log", 2, OFF_ON),
    item(245, MK2_RTTY_LOG, "File Type", 2, Kind::Choice(&["Text", "HTML"])),
    item(246, MK2_RTTY_LOG, "Time Stamp", 2, OFF_ON),
    item(247, MK2_RTTY_LOG, "Time Stamp (Time)", 2, Kind::Choice(&["Local", "UTC"])),
    item(248, MK2_RTTY_LOG, "Time Stamp (Frequency)", 2, OFF_ON),
    item(253, SCAN, "SCAN Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(254, SCAN, "SCAN Resume", 2, OFF_ON),
    item(255, VOICE, "TX LEVEL", 4, PERCENT),
    item(256, VOICE, "Auto Monitor", 2, OFF_ON),
    item(257, VOICE, "Repeat Time", 2, Kind::Num { min: 1, max: 15, step: 1, unit: "s", signed: false, div: 1 }),
    item(258, MK2_REC, "TX REC Audio", 2, Kind::Choice(&["Direct", "Monitor"])),
    item(259, MK2_REC, "RX REC Condition", 2, Kind::Choice(&["Always", "Squelch Auto"])),
    item(260, MK2_REC, "File Split", 2, OFF_ON),
    item(261, MK2_REC, "PTT Auto REC", 2, OFF_ON),
    item(262, MK2_REC, "PRE-REC for PTT Auto REC", 2, PRE_REC),
    item(263, MK2_PLAYER, "Skip Time", 2, SKIP_TIME),
    item(264, MK2_MENU_SET, "CW [KEYER/DECODE] Key", 2, Kind::Choice(&["Select screen", "KEYER", "DECODE"])),
    item(265, NB, "NB Depth", 2, NB_DEPTH),
    item(266, NB, "NB Width", 4, PERCENT),
    item(267, VOX, "VOX Delay", 2, VOX_DELAY),
    item(268, VOX, "VOX Voice Delay", 2, VOX_VOICE_DELAY),
    item(269, MK2_APF, "APF Type", 2, Kind::Choice(&["SHARP", "SOFT"])),
    item(270, MK2_APF, "APF AF Level", 2, Kind::Num { min: 0, max: 6, step: 1, unit: "dB", signed: false, div: 1 }),
    // Front-panel settings (their own CI-V commands, see SPEC_MK2.extra).
    item(9001, PANEL, "IP+", 2, OFF_ON),
    item(9002, PANEL, "IF Filter Shape", 2, Kind::Choice(&["SHARP", "SOFT"])),
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
    item(9014, PANEL, "LCD Backlight Brightness", 4, PERCENT),
    item(9015, PANEL, "RX Antenna", 2, OFF_ON),
];

pub static MENU_MK2: [MenuItem; RAW_MK2.len()] = icom::numbered::<{ RAW_MK2.len() }>(RAW_MK2);

pub const EXTRA_MK2: &[(u32, &[u8])] = &[
    (9001, &[0x16, 0x65]),
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
    (9015, &[0x12, 0x00]),
];

/// COMP meter of the IC-7300MK2: 02 10 = 30 dB.
pub const MK2_COMP_CAL: [(u8, f32); 3] = [(0, 0.0), (130, 15.0), (210, 30.0)];

pub static SPEC_MK2: CivSpec = CivSpec {
    comp_cal: &MK2_COMP_CAL,
    transceive: Some(89),
    apf: true,
    apf_widths: true,
    apf_hz: 550.0,
    memory_name_len: 16,
    vox_delay: Some(267),
    keyer_type: Some(224),
    extra: EXTRA_MK2,
    ..SPEC
};

pub static IC7300MK2: RadioModel = RadioModel {
    key: "ic7300mk2",
    name: "IC-7300MK2",
    proto: Proto::CiV(&SPEC_MK2),
    civ_addr: 0xB6,
    menu: &MENU_MK2,
    roles: Roles {
        mic_sel: &[icom::num_of(&MENU_MK2, 84), icom::num_of(&MENU_MK2, 85)],
        mic_front: 0,
        mic_usb: 1,
        // USB (B) RTS.
        pc_keying: Some((icom::num_of(&MENU_MK2, 97), 0, 4)),
        cat_tot: None,
        cat_items: &[
            icom::num_of(&MENU_MK2, 89),
            icom::num_of(&MENU_MK2, 90),
            icom::num_of(&MENU_MK2, 92),
            icom::num_of(&MENU_MK2, 93),
            icom::num_of(&MENU_MK2, 94),
            icom::num_of(&MENU_MK2, 100),
            icom::num_of(&MENU_MK2, 103),
            icom::num_of(&MENU_MK2, 108),
            icom::num_of(&MENU_MK2, 110),
            icom::num_of(&MENU_MK2, 111),
            icom::num_of(&MENU_MK2, 112),
        ],
        cw_bfo: Some((icom::num_of(&MENU_MK2, 56), ["01", "00", "--"])),
        cw_pitch_display: None,
        data_other: None,
        data_disp: None,
    },
    caps: Caps { commands: MK2_COMMANDS, ..IC7300.caps },
    ..IC7300
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialect::icom::num_of;
    use crate::dialect::icom::testing::*;

    #[test]
    fn ic7300_literal_frames() {
        let mut r = radio(&IC7300);
        assert_eq!(down(&mut r, "FA14074000;"), ["FE FE 94 E0 25 00 00 40 07 14 00 FD"]);
        assert!(up(&mut r, "FE FE E0 94 FB FD").is_empty());
        assert_eq!(down(&mut r, "FB;"), ["FE FE 94 E0 25 01 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 94 25 01 00 60 07 14 00 FD"), ["FB14076000;"]);
        assert_eq!(down(&mut r, "MD0C;"), ["FE FE 94 E0 26 00 01 01 01 FD"]);
        up(&mut r, "FE FE E0 94 FB FD");
        assert_eq!(down(&mut r, "MD0;"), ["FE FE 94 E0 26 00 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 94 26 00 03 00 02 FD"), ["MD03;"]);
        assert_eq!(down(&mut r, "AG0128;"), ["FE FE 94 E0 14 01 01 28 FD"]);
        up(&mut r, "FE FE E0 94 FB FD");
        // 50 W of 100: 01 28 (half of 02 55).
        assert_eq!(down(&mut r, "PC050;"), ["FE FE 94 E0 14 0A 01 28 FD"]);
        up(&mut r, "FE FE E0 94 FB FD");
        // S9 = 01 20 on the IC-7300, 130 on RigHelm's scale.
        assert_eq!(down(&mut r, "SM0;"), ["FE FE 94 E0 15 02 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 94 15 02 01 20 FD"), ["SM0130;"]);
        assert_eq!(down(&mut r, "RA01;"), ["FE FE 94 E0 11 20 FD"]);
        up(&mut r, "FE FE E0 94 FB FD");
        assert_eq!(down(&mut r, "TX1;"), ["FE FE 94 E0 1C 00 01 FD"]);
        up(&mut r, "FE FE E0 94 FB FD");
        assert_eq!(down(&mut r, "BS05;"), ["FE FE 94 E0 1A 01 05 01 FD"]);
        // A refusal is "?;".
        assert_eq!(up(&mut r, "FE FE E0 94 FA FD"), ["?;"]);
        // CI-V Transceive broadcast of the frequency.
        assert_eq!(up(&mut r, "FE FE 00 94 00 00 40 07 07 00 FD"), ["FA07074000;"]);
    }

    #[test]
    fn ic7300_menu_vox_delay_keyer_and_extras() {
        let m = &IC7300;
        let mut r = radio(m);
        let n = num_of(m.menu, 71);
        assert_eq!(down(&mut r, &format!("EX{n:03};")), ["FE FE 94 E0 1A 05 00 71 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 94 1A 05 00 71 01 FD"), [format!("EX{n:03}01;")]);
        let beep = num_of(m.menu, 21);
        assert_eq!(down(&mut r, &format!("EX{beep:03}0128;")), ["FE FE 94 E0 1A 05 00 21 01 28 FD"]);
        up(&mut r, "FE FE E0 94 FB FD");
        let call = num_of(m.menu, 91);
        assert_eq!(down(&mut r, &format!("EX{call:03}W1AW      ;")), ["FE FE 94 E0 1A 05 00 91 57 31 41 57 FD"]);
        up(&mut r, "FE FE E0 94 FB FD");
        let ipp = num_of(m.menu, 9001);
        assert_eq!(down(&mut r, &format!("EX{ipp:03}01;")), ["FE FE 94 E0 1A 07 01 FD"]);
        up(&mut r, "FE FE E0 94 FB FD");
        assert_eq!(down(&mut r, &format!("EX{ipp:03};")), ["FE FE 94 E0 1A 07 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 94 1A 07 01 FD"), [format!("EX{ipp:03}01;")]);
        assert_eq!(down(&mut r, "VD0500;"), ["FE FE 94 E0 1A 05 01 91 05 FD"]);
        up(&mut r, "FE FE E0 94 FB FD");
        assert_eq!(down(&mut r, "VD;"), ["FE FE 94 E0 1A 05 01 91 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 94 1A 05 01 91 05 FD"), ["VD0500;"]);
        assert_eq!(down(&mut r, "KR1;"), ["FE FE 94 E0 1A 05 01 64 02 FD"]);
        up(&mut r, "FE FE E0 94 FB FD");
        assert_eq!(down(&mut r, "KR;"), ["FE FE 94 E0 1A 05 01 64 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 94 1A 05 01 64 01 FD"), ["KR1;"]);
    }

    #[test]
    fn ic7300_emulator_literal_frames() {
        let mut e = emulator(&IC7300);
        let req = e.request(&hex("FE FE 94 E0 25 00 FD"));
        assert_eq!(req.reads, ["FA;"]);
        assert_eq!(e.respond(&req, &[Some("FA07074000;".into())]), hex("FE FE E0 94 25 00 00 40 07 07 00 FD"));
        let req = e.request(&hex("FE FE 94 E0 26 00 01 01 02 FD"));
        assert_eq!(req.sets, ["MD0C;"]);
        let req = e.request(&hex("FE FE 94 E0 1A 05 00 71 00 FD"));
        assert_eq!(req.sets, [format!("EX{:03}00;", num_of(IC7300.menu, 71))]);
        let req = e.request(&hex("FE FE 94 E0 19 00 FD"));
        assert_eq!(req.immediate, Some(hex("FE FE E0 94 19 00 94 FD")));
    }

    #[test]
    fn ic7300_round_trips() {
        let m = &IC7300;
        for f in [
            "FA14074000;", "FB07074000;", "MD01;", "MD02;", "MD03;", "MD07;", "MD05;", "MD04;", "MD06;", "MD09;", "MD08;", "MD0C;", "MD0H;", "MD0A;",
            "AG0128;", "RG0200;", "SQ0010;", "PC050;", "MG040;", "KS022;", "PA01;", "PA02;", "NB01;", "NR01;", "RL008;", "BC01;", "PR01;", "VX1;",
            "BI1;", "RT1;", "XT0;", "SH010;", "GT01;", "GT03;", "RA01;", "RA00;", "TX1;", "TX0;", "AC001;", "LK1;", "PB01;",
            "MW00107074000+000000C00000;", "MW10014074000+000000402000;",
        ] {
            round_trip_set(m, f);
        }
        menu_round_trips(m);
        for (read, answer) in [
            ("FA;", "FA14074000;"),
            ("FB;", "FB14076000;"),
            ("MD0;", "MD0C;"),
            ("AG0;", "AG0128;"),
            ("PC;", "PC050;"),
            ("KS;", "KS022;"),
            ("GT0;", "GT02;"),
            ("RA0;", "RA01;"),
            ("SH0;", "SH028;"),
            ("TX;", "TX0;"),
            ("FT;", "FT1;"),
            ("MR001;", "MR00107074000+000000C10000;"),
            ("MR101;", "MR10114074000+000000411000;"),
        ] {
            round_trip_read(m, read, answer);
        }
    }

    #[test]
    fn ic7300_ctcss() {
        let mut r = radio(&IC7300);
        // Tone 08 = 88.5 Hz, for the repeater tone and the tone squelch.
        assert_eq!(down(&mut r, "CN008;"), ["FE FE 94 E0 1B 00 00 08 85 FD", "FE FE 94 E0 1B 01 00 08 85 FD"]);
        up(&mut r, "FE FE E0 94 FB FD");
        up(&mut r, "FE FE E0 94 FB FD");
        assert_eq!(down(&mut r, "CN0;"), ["FE FE 94 E0 1B 00 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 94 1B 00 00 10 00 FD"), ["CN012;"]);
        // Encode only: tone squelch off, repeater tone on.
        assert_eq!(down(&mut r, "CT02;"), ["FE FE 94 E0 16 43 00 FD", "FE FE 94 E0 16 42 01 FD"]);
        up(&mut r, "FE FE E0 94 FB FD");
        up(&mut r, "FE FE E0 94 FB FD");
        assert_eq!(down(&mut r, "CT0;"), ["FE FE 94 E0 16 42 FD", "FE FE 94 E0 16 43 FD"]);
        assert!(up(&mut r, "FE FE E0 94 16 42 00 FD").is_empty());
        assert_eq!(up(&mut r, "FE FE E0 94 16 43 01 FD"), ["CT01;"]);
        for (read, answer) in [("CN0;", "CN012;"), ("CT0;", "CT02;"), ("CT0;", "CT01;"), ("CT0;", "CT00;")] {
            round_trip_read(&IC7300, read, answer);
        }
    }

    #[test]
    fn ic7300mk2_literal_frames() {
        let m = &IC7300MK2;
        let mut r = radio(m);
        assert_eq!(down(&mut r, "FA07074000;"), ["FE FE B6 E0 25 00 00 40 07 07 00 FD"]);
        up(&mut r, "FE FE E0 B6 FB FD");
        let n = num_of(m.menu, 89);
        assert_eq!(down(&mut r, &format!("EX{n:03};")), ["FE FE B6 E0 1A 05 00 89 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 B6 1A 05 00 89 01 FD"), [format!("EX{n:03}01;")]);
        let ipp = num_of(m.menu, 9001);
        assert_eq!(down(&mut r, &format!("EX{ipp:03}01;")), ["FE FE B6 E0 16 65 01 FD"]);
        up(&mut r, "FE FE E0 B6 FB FD");
        let rxant = num_of(m.menu, 9015);
        assert_eq!(down(&mut r, &format!("EX{rxant:03};")), ["FE FE B6 E0 12 00 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 B6 12 00 01 FD"), [format!("EX{rxant:03}01;")]);
        let port = num_of(m.menu, 110);
        assert_eq!(down(&mut r, &format!("EX{port:03}050001;")), ["FE FE B6 E0 1A 05 01 10 05 00 01 FD"]);
        up(&mut r, "FE FE E0 B6 FB FD");
        // APF: the radio's NAR width is kept when RigHelm turns APF on.
        assert_eq!(down(&mut r, "CO00;"), ["FE FE B6 E0 16 32 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 B6 16 32 03 FD"), ["CO0002;"]);
        assert_eq!(down(&mut r, "CO0000;"), ["FE FE B6 E0 16 32 00 FD"]);
        up(&mut r, "FE FE E0 B6 FB FD");
        assert_eq!(down(&mut r, "CO0002;"), ["FE FE B6 E0 16 32 03 FD"]);
        up(&mut r, "FE FE E0 B6 FB FD");
        // +50 Hz of +-550 Hz around 01 28.
        assert_eq!(down(&mut r, "CO0230;"), ["FE FE B6 E0 14 05 01 40 FD"]);
        up(&mut r, "FE FE E0 B6 FB FD");
        assert_eq!(down(&mut r, "CO02;"), ["FE FE B6 E0 14 05 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 B6 14 05 01 40 FD"), ["CO0230;"]);
        assert_eq!(down(&mut r, "VD1000;"), ["FE FE B6 E0 1A 05 02 67 10 FD"]);
        up(&mut r, "FE FE E0 B6 FB FD");
        assert_eq!(down(&mut r, "KR0;"), ["FE FE B6 E0 1A 05 02 24 00 FD"]);
        up(&mut r, "FE FE E0 B6 FB FD");
        // COMP meter: 02 10 = 30 dB = full scale on RigHelm's.
        assert_eq!(down(&mut r, "RM3;"), ["FE FE B6 E0 15 14 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 B6 15 14 02 10 FD"), ["RM3255;"]);
    }

    #[test]
    fn ic7300mk2_round_trips() {
        let m = &IC7300MK2;
        for f in ["FA14074000;", "MD0C;", "MD03;", "PC050;", "CO0002;", "CO0000;", "CO0225;", "CO0250;", "CO0200;", "SH040;", "PA02;"] {
            round_trip_set(m, f);
        }
        menu_round_trips(m);
        for (read, answer) in [("FA;", "FA14074000;"), ("CO00;", "CO0002;"), ("CO02;", "CO0240;")] {
            round_trip_read(m, read, answer);
        }
        assert_eq!(m.menu.len(), MENU_MK2.len());
        assert_ne!(m.menu.len(), IC7300.menu.len());
    }
}
