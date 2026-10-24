//! Icom IC-7610 (CI-V address 98h): HF/50 MHz with two receivers (MAIN and
//! SUB), two antenna connectors and an RX antenna input.
//!
//! Source: IC-7610 CI-V Reference Guide (command table pp. 3-9, command
//! formats pp. 10-15).
//!
//! The IC-7610 has no VFO A/B: its two "VFOs" are the MAIN and SUB
//! receivers (`25`/`26` 00 = MAIN, 01 = SUB, `07 D0`/`07 D1` select one),
//! so RigHelm's VFO-A is MAIN and VFO-B is SUB, and split transmits on SUB.

use super::ic7300::{
    AF_IF, AF_SQL, BAND_EDGE_BEEP, CLOCK, COUNT_UP, DATE, DOT_DASH, KEY_TYPE, MK2_USB_LINE, NB_DEPTH, NORMAL_REVERSE, NUMBER_STYLE, PANEL, PRE_REC,
    SKIP_TIME, TUNING_STEP, VOX_DELAY, VOX_VOICE_DELAY, WIDTH3,
};
use crate::dialect::icom::{self, AVERAGING, CivSpec, FILL, MemLayout, NOTCH_SW, OFF_ON, PERCENT, SPEED3, TBW, TONE, TOT, TX_DELAY, item};
use crate::menu::{Kind, MenuItem};
use crate::model::{Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::types::{Band, Mode};

/// Band stacking codes (`1A 01`), as the IC-7300's but GEN ends at 60 MHz.
pub const BANDS: &[Band] = &[
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
    Band { code: 11, label: "GEN", lo: 30_000, hi: 60_000_000 },
];

/// Icom mode code and DATA (DATA1; the radio's DATA2/DATA3 read as DATA
/// too). PSK-R (13) has no RigHelm mode.
pub const MODES: &[(Mode, u8, bool)] = &[
    (Mode::Lsb, 0x00, false),
    (Mode::Usb, 0x01, false),
    (Mode::Cw, 0x03, false),
    (Mode::CwR, 0x07, false),
    (Mode::RttyLsb, 0x04, false),
    (Mode::RttyUsb, 0x08, false),
    (Mode::Psk, 0x12, false),
    (Mode::Am, 0x02, false),
    (Mode::Fm, 0x05, false),
    (Mode::DataLsb, 0x00, true),
    (Mode::DataUsb, 0x01, true),
    (Mode::DataAm, 0x02, true),
    (Mode::DataFm, 0x05, true),
];

pub const MODE_LIST: &[Mode] = &[
    Mode::Lsb,
    Mode::Usb,
    Mode::Cw,
    Mode::CwR,
    Mode::RttyLsb,
    Mode::RttyUsb,
    Mode::Psk,
    Mode::Am,
    Mode::Fm,
    Mode::DataLsb,
    Mode::DataUsb,
    Mode::DataAm,
    Mode::DataFm,
];

/// RigHelm commands the IC-7610 carries out.
pub const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AM", "AN", "BA", "BC", "BD", "BI", "BP00", "BS", "BU", "BY", "CN", "CO00", "CO02", "CT", "EX", "FA", "FB", "FR", "FT", "GT", "ID",
    "IF", "KM", "KP", "KR", "KS", "KY", "LK", "MA", "MC", "MD", "MG", "ML0", "ML1", "MR", "MW", "MX", "NB", "NL", "NR", "OI", "PA", "PB", "PC",
    "PL", "PR0", "PS", "RA", "RC", "RD", "RG", "RL", "RM", "RT", "RU", "SH", "SM", "SQ", "SV", "TX", "VD", "VG", "VS", "VX", "XT",
];

const TONE_RX: &str = "SET > Tone Control/TBW > RX";
const TONE_TX: &str = "SET > Tone Control/TBW > TX";
const FUNC: &str = "SET > Function";
const SPLIT: &str = "SET > Function > SPLIT";
const SPEECH: &str = "SET > Function > SPEECH";
const PHONES: &str = "SET > Connectors > Phones";
const ACC_OUT: &str = "SET > Connectors > ACC AF/IF Output";
const USB_OUT: &str = "SET > Connectors > USB AF/IF Output";
const LAN_OUT: &str = "SET > Connectors > LAN AF/IF Output";
const MOD: &str = "SET > Connectors > MOD Input";
const USB_KEY: &str = "SET > Connectors > USB SEND/Keying";
const KEYPAD: &str = "SET > Connectors > External Keypad";
const KBD: &str = "SET > Connectors > Keyboard/Mouse";
const USB_DIAL: &str = "SET > Connectors > USB DIAL";
const CIV: &str = "SET > Connectors > CI-V";
const METER: &str = "SET > Connectors > External Meter";
const CONN: &str = "SET > Connectors";
const NET: &str = "SET > Network";
const DISP: &str = "SET > Display";
const TIME: &str = "SET > Time Set";
const SCOPE: &str = "SCOPE SET";
const AUDIO: &str = "AUDIO SCOPE SET";
const KEYER001: &str = "KEYER > KEYER 001";
const CWKEY: &str = "KEYER > CW-KEY SET";
const RTTY: &str = "RTTY DECODE SET";
const RTTY_LOG: &str = "RTTY DECODE LOG";
const PSK: &str = "PSK DECODE SET";
const PSK_LOG: &str = "PSK DECODE LOG";
const SCAN: &str = "SCAN SET";
const VOICE: &str = "VOICE TX";
const REC: &str = "RECORDER > Recorder Set";
const PLAYER: &str = "RECORDER > Player Set";
const REPLAY: &str = "RECORDER > Instant Replay Set";
const ANT: &str = "ANT";
const NB: &str = "NB";
const VOX: &str = "VOX";
const APF: &str = "APF";

const MOD_7610: Kind = Kind::Choice(&["MIC", "ACC", "MIC,ACC", "USB", "MIC,USB", "LAN"]);
const MAIN_SUB: Kind = Kind::Choice(&["MAIN", "SUB"]);
const EXT_METER_MAIN: Kind = Kind::Choice(&["Auto", "S (main)", "Po", "SWR", "ALC", "COMP", "Vd", "Id"]);
const EXT_METER_SUB: Kind = Kind::Choice(&["Auto", "S (sub)", "Po", "SWR", "ALC", "COMP", "Vd", "Id"]);
const BEEP_SOUND: Kind = Kind::Num { min: 50, max: 200, step: 1, unit: "x10 Hz", signed: false, div: 1 };
const PORT: Kind = Kind::Num { min: 1, max: 65535, step: 1, unit: "", signed: false, div: 1 };
const ANT_MEMORY: Kind = Kind::Coded(&[(0, "ANT1"), (1, "ANT1 + RX-ANT"), (100, "ANT2"), (101, "ANT2 + RX-ANT")]);
const GRID10: Kind = Kind::Choice(&["Grid 1", "Grid 2", "Grid 3", "Grid 4", "Grid 5", "Grid 6", "Grid 7", "Grid 8", "Grid 9", "Grid 10"]);
const KEYBOARD_LANG: Kind = Kind::Choice(&[
    "English",
    "Japanese",
    "United Kingdom",
    "French",
    "French (Canadian)",
    "German",
    "Portuguese",
    "Portuguese (Brazilian)",
    "Spanish",
    "Spanish (Latin American)",
    "Italian",
]);
const STAMP_TIME: Kind = Kind::Choice(&["Local", "CLOCK2"]);

const RAW: &[MenuItem] = &[
    // RX HPF/LPF (00 01, 00 04, 00 07, 00 10-00 12) are two numbers in
    // one; FM SPLIT/transverter offsets (00 35, 00 36, 00 40) frequencies;
    // IP addresses (01 27, 01 28, 01 30-01 32) four numbers; the UTC
    // offsets (01 62, 01 64) an offset and a sign; the colours and the
    // scope's fixed edges: all left out.
    item(2, TONE_RX, "SSB RX Tone (Bass)", 2, TONE),
    item(3, TONE_RX, "SSB RX Tone (Treble)", 2, TONE),
    item(5, TONE_RX, "AM RX Tone (Bass)", 2, TONE),
    item(6, TONE_RX, "AM RX Tone (Treble)", 2, TONE),
    item(8, TONE_RX, "FM RX Tone (Bass)", 2, TONE),
    item(9, TONE_RX, "FM RX Tone (Treble)", 2, TONE),
    item(13, TONE_TX, "SSB TX Tone (Bass)", 2, TONE),
    item(14, TONE_TX, "SSB TX Tone (Treble)", 2, TONE),
    item(15, TONE_TX, "SSB TBW (WIDE)", 2, TBW),
    item(16, TONE_TX, "SSB TBW (MID)", 2, TBW),
    item(17, TONE_TX, "SSB TBW (NAR)", 2, TBW),
    item(296, TONE_TX, "SSB-D TBW", 2, TBW),
    item(18, TONE_TX, "AM TX Tone (Bass)", 2, TONE),
    item(19, TONE_TX, "AM TX Tone (Treble)", 2, TONE),
    item(20, TONE_TX, "FM TX Tone (Bass)", 2, TONE),
    item(21, TONE_TX, "FM TX Tone (Treble)", 2, TONE),
    item(22, FUNC, "Beep Level", 4, PERCENT),
    item(23, FUNC, "Beep Level Limit", 2, OFF_ON),
    item(24, FUNC, "Beep (Confirmation)", 2, OFF_ON),
    item(25, FUNC, "Band Edge Beep", 2, BAND_EDGE_BEEP),
    item(26, FUNC, "Beep Sound (MAIN)", 4, BEEP_SOUND),
    item(27, FUNC, "Beep Sound (SUB)", 4, BEEP_SOUND),
    item(28, FUNC, "RF/SQL Control", 2, Kind::Choice(&["Auto", "SQL", "RF+SQL"])),
    item(29, FUNC, "TX Delay (HF)", 2, TX_DELAY),
    item(30, FUNC, "TX Delay (50M)", 2, TX_DELAY),
    item(31, FUNC, "Time-Out Timer (CI-V)", 2, TOT),
    item(32, FUNC, "Quick Dualwatch", 2, OFF_ON),
    item(33, SPLIT, "Quick SPLIT", 2, OFF_ON),
    item(34, SPLIT, "Display Keypad on Quick SPLIT", 2, OFF_ON),
    item(37, SPLIT, "SPLIT LOCK", 2, OFF_ON),
    item(38, "SET > Function > Tuner", "PTT Start", 2, OFF_ON),
    item(39, FUNC, "Transverter Function", 2, Kind::Choice(&["Auto", "ON"])),
    item(41, FUNC, "RTTY Mark Frequency", 2, Kind::Choice(&["1275 Hz", "1615 Hz", "2125 Hz"])),
    item(42, FUNC, "RTTY Shift Width", 2, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz"])),
    item(43, FUNC, "RTTY Keying Polarity", 2, NORMAL_REVERSE),
    item(44, FUNC, "PSK Tone Frequency", 2, Kind::Choice(&["1000 Hz", "1500 Hz", "2000 Hz"])),
    item(45, SPEECH, "SPEECH Language", 2, Kind::Choice(&["English", "Japanese"])),
    item(46, SPEECH, "SPEECH Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(47, SPEECH, "S-Level SPEECH", 2, OFF_ON),
    item(48, SPEECH, "MODE SPEECH", 2, OFF_ON),
    item(49, SPEECH, "SPEECH Level", 4, PERCENT),
    item(50, FUNC, "[SPEECH/LOCK] Switch", 2, Kind::Choice(&["SPEECH/LOCK", "LOCK/SPEECH"])),
    item(51, FUNC, "Lock Function", 2, Kind::Choice(&["MAIN DIAL", "PANEL"])),
    item(52, FUNC, "Memo Pad Quantity", 2, Kind::Choice(&["5 ch", "10 ch"])),
    item(53, FUNC, "MAIN DIAL Auto TS", 2, Kind::Choice(&["OFF", "Low", "High"])),
    item(54, FUNC, "MAIN DIAL Select (USB DIAL-SUB Only)", 2, Kind::Choice(&["Main only", "Main/Sub"])),
    item(55, FUNC, "MAIN/SUB Tracking [MAIN/SUB] Switch", 2, OFF_ON),
    item(56, FUNC, "MIC Up/Down Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(57, FUNC, "Quick RIT/∂TX Clear", 2, OFF_ON),
    item(58, FUNC, "[NOTCH] Switch (SSB)", 2, NOTCH_SW),
    item(59, FUNC, "[NOTCH] Switch (AM)", 2, NOTCH_SW),
    item(60, FUNC, "FILTER Screen MAIN/SUB Select", 2, Kind::Choice(&["Fix", "Auto (by FILTER, PBT Operation)"])),
    item(61, FUNC, "SSB/CW Synchronous Tuning", 2, OFF_ON),
    item(62, FUNC, "CW Normal Side", 2, Kind::Choice(&["LSB", "USB"])),
    item(63, FUNC, "Screen Keyboard Type", 2, Kind::Choice(&["Ten-key", "Full Keyboard"])),
    item(64, FUNC, "Screen Full Keyboard Layout", 2, Kind::Choice(&["English", "German", "French"])),
    item(65, FUNC, "Screen Capture [POWER] Switch", 2, OFF_ON),
    item(66, FUNC, "Screen Capture Keyboard [Print Screen]", 2, OFF_ON),
    item(67, FUNC, "Screen Capture Storage Media", 2, Kind::Choice(&["SD Card", "USB flash drive"])),
    item(68, FUNC, "Screen Capture File Type", 2, Kind::Choice(&["PNG", "BMP"])),
    item(69, FUNC, "Calibration Marker", 2, OFF_ON),
    item(70, FUNC, "REF Adjust", 4, Kind::Percent { max: 511 }),
    item(71, PHONES, "Level", 2, Kind::Steps { max_code: 30, off_at_zero: false, first: -15, step: 1, unit: "dB" }),
    item(72, PHONES, "L/R Mix", 2, OFF_ON),
    item(73, ACC_OUT, "AF/SQL Output Select", 2, MAIN_SUB),
    item(74, ACC_OUT, "Output Select", 2, AF_IF),
    item(75, ACC_OUT, "AF/IF XFC Output (SPLIT ON)", 2, MAIN_SUB),
    item(76, ACC_OUT, "AF Output Level", 4, PERCENT),
    item(77, ACC_OUT, "AF SQL", 2, AF_SQL),
    item(78, ACC_OUT, "AF Beep/Speech... Output", 2, OFF_ON),
    item(79, ACC_OUT, "IF Output Level", 4, PERCENT),
    item(80, USB_OUT, "Output Select", 2, AF_IF),
    item(81, USB_OUT, "AF/IF XFC Output (SPLIT ON)", 2, MAIN_SUB),
    item(82, USB_OUT, "AF Output Level", 4, PERCENT),
    item(83, USB_OUT, "AF SQL", 2, AF_SQL),
    item(84, USB_OUT, "AF Beep/Speech... Output", 2, OFF_ON),
    item(85, USB_OUT, "IF Output Level", 4, PERCENT),
    item(86, LAN_OUT, "Output Select", 2, AF_IF),
    item(87, LAN_OUT, "AF SQL", 2, AF_SQL),
    item(88, MOD, "ACC MOD Level", 4, PERCENT),
    item(89, MOD, "USB MOD Level", 4, PERCENT),
    item(90, MOD, "LAN MOD Level", 4, PERCENT),
    item(91, MOD, "DATA OFF MOD", 2, MOD_7610),
    item(92, MOD, "DATA1 MOD", 2, MOD_7610),
    item(93, MOD, "DATA2 MOD", 2, MOD_7610),
    item(94, MOD, "DATA3 MOD", 2, MOD_7610),
    item(95, USB_KEY, "USB SEND", 2, MK2_USB_LINE),
    item(96, USB_KEY, "USB Keying (CW)", 2, MK2_USB_LINE),
    item(97, USB_KEY, "USB Keying (RTTY)", 2, MK2_USB_LINE),
    item(297, USB_KEY, "Inhibit Timer at USB connection", 2, OFF_ON),
    item(98, KEYPAD, "VOICE", 2, OFF_ON),
    item(99, KEYPAD, "KEYER", 2, OFF_ON),
    item(100, KEYPAD, "RTTY", 2, OFF_ON),
    item(101, KEYPAD, "PSK", 2, OFF_ON),
    item(102, KBD, "Keyboard [F1] ~ [F8] (VOICE)", 2, OFF_ON),
    item(103, KBD, "Keyboard [F1] ~ [F8] (KEYER)", 2, OFF_ON),
    item(104, KBD, "Keyboard Type", 2, KEYBOARD_LANG),
    item(105, KBD, "Keyboard Repeat Delay", 4, Kind::Num { min: 10, max: 100, step: 5, unit: "x10 ms", signed: false, div: 1 }),
    item(106, KBD, "Keyboard Repeat Rate (0 = 2.0 cps, 31 = 30.0 cps)", 2, Kind::Num { min: 0, max: 31, step: 1, unit: "", signed: false, div: 1 }),
    item(107, KBD, "Mouse Pointer Speed", 2, SPEED3),
    item(108, KBD, "Mouse Pointer Acceleration", 2, OFF_ON),
    item(109, USB_DIAL, "USB DIAL Select", 2, Kind::Choice(&["Sub only", "Main/Sub"])),
    item(110, USB_DIAL, "USB DIAL Auto TS", 2, Kind::Choice(&["OFF", "Low", "High"])),
    item(111, USB_DIAL, "USB DIAL [TRANSMIT] Switch", 2, Kind::Choice(&["Push to toggle", "Hold down to transmit"])),
    item(112, CIV, "CI-V Transceive", 2, OFF_ON),
    item(113, CIV, "CI-V USB/LAN→REMOTE Transceive Address", 4, Kind::Num { min: 0, max: 223, step: 1, unit: "", signed: false, div: 1 }),
    item(114, CIV, "CI-V Output (for ANT)", 2, OFF_ON),
    item(115, CIV, "CI-V USB Port", 2, Kind::Choice(&["Link to [REMOTE]", "Unlink from [REMOTE]"])),
    item(116, CIV, "CI-V USB Echo Back", 2, OFF_ON),
    item(117, METER, "External Meter Output (MAIN)", 2, EXT_METER_MAIN),
    item(118, METER, "External Meter Output (SUB)", 2, EXT_METER_SUB),
    item(119, METER, "External Meter Level (MAIN)", 4, PERCENT),
    item(120, METER, "External Meter Level (SUB)", 4, PERCENT),
    item(121, CONN, "Decode Baud Rate", 2, Kind::Choice(&["4800 bps", "9600 bps", "19200 bps", "38400 bps"])),
    item(122, CONN, "SEND Relay Type", 2, Kind::Choice(&["Reed", "MOS-FET"])),
    item(123, CONN, "ACC BAND Voltage Output", 2, Kind::Choice(&["MAIN", "SUB", "TX"])),
    item(124, CONN, "MIC Input DC Bias", 2, OFF_ON),
    item(125, CONN, "REF IN", 2, Kind::Choice(&["IN", "OFF"])),
    item(126, NET, "DHCP (valid after restart)", 2, OFF_ON),
    item(129, NET, "Subnet Mask (bits, valid after restart)", 2, Kind::Num { min: 1, max: 30, step: 1, unit: "", signed: false, div: 1 }),
    item(133, NET, "Network Name", 15, Kind::Text { max: 15 }),
    item(134, NET, "Network Control (valid after restart)", 2, OFF_ON),
    item(135, NET, "Power OFF Setting (for Remote Control)", 2, Kind::Choice(&["Shutdown only", "Standby/Shutdown"])),
    item(136, NET, "Control Port (UDP)", 6, PORT),
    item(137, NET, "Serial Port (UDP)", 6, PORT),
    item(138, NET, "Audio Port (UDP)", 6, PORT),
    item(139, NET, "Internet Access Line", 2, Kind::Choice(&["FTTH", "ADSL/CATV"])),
    item(140, NET, "Network Radio Name", 16, Kind::Text { max: 16 }),
    item(141, DISP, "LCD Backlight", 4, PERCENT),
    item(142, DISP, "LED Bright", 4, PERCENT),
    item(143, DISP, "Display Type", 2, Kind::Choice(&["A", "B"])),
    item(144, DISP, "Display Font", 2, Kind::Choice(&["Basic", "Round"])),
    item(145, DISP, "Meter Response (Standard, Edgewise)", 2, SPEED3),
    item(146, DISP, "Meter Type (Normal Screen)", 2, Kind::Choice(&["Standard", "Edgewise", "Bar"])),
    item(147, DISP, "Meter Type (Expand Screen)", 2, Kind::Choice(&["Edgewise", "Bar"])),
    item(148, DISP, "Meter Peak Hold (Bar)", 2, OFF_ON),
    item(149, DISP, "Memory Name", 2, OFF_ON),
    item(150, DISP, "APF-Width Popup (APF OFF→ON)", 2, OFF_ON),
    item(151, DISP, "Screen Saver", 2, Kind::Choice(&["OFF", "15 min", "30 min", "60 min"])),
    item(152, DISP, "External Display", 2, OFF_ON),
    item(153, DISP, "External Display Resolution", 2, Kind::Choice(&["800x480", "800x600"])),
    item(154, DISP, "Opening Message", 2, OFF_ON),
    item(155, DISP, "My Call", 10, Kind::Text { max: 10 }),
    item(156, DISP, "Power ON Check", 2, OFF_ON),
    item(157, DISP, "Display Language", 2, Kind::Choice(&["English", "Japanese"])),
    item(158, TIME, "Date (YYYYMMDD)", 8, DATE),
    item(159, TIME, "Time (HHMM)", 4, CLOCK),
    item(160, TIME, "NTP Function", 2, OFF_ON),
    item(161, TIME, "NTP Server Address", 64, Kind::Text { max: 64 }),
    item(163, TIME, "CLOCK2 Function", 2, OFF_ON),
    item(165, TIME, "CLOCK2 Name", 3, Kind::Text { max: 3 }),
    item(166, SCOPE, "Scope during Tx (CENTER TYPE)", 2, OFF_ON),
    item(167, SCOPE, "Max Hold", 2, Kind::Choice(&["OFF", "10s Hold", "ON"])),
    item(168, SCOPE, "CENTER Type Display", 2, Kind::Choice(&["Filter center", "Carrier point center", "Carrier point center (Abs. Freq.)"])),
    item(169, SCOPE, "Marker Position (Fix Type/SCROLL Type)", 2, Kind::Choice(&["Filter center", "Carrier point"])),
    item(170, SCOPE, "Averaging", 2, AVERAGING),
    item(171, SCOPE, "Waveform Type", 2, FILL),
    item(175, SCOPE, "Waterfall Display", 2, OFF_ON),
    item(176, SCOPE, "Waterfall Speed", 2, SPEED3),
    item(177, SCOPE, "Waterfall Size (Expand Screen)", 2, Kind::Choice(&["Small", "Mid", "Large"])),
    item(178, SCOPE, "Waterfall Peak Color Level", 2, GRID10),
    item(179, SCOPE, "Waterfall Marker Auto-hide", 2, OFF_ON),
    item(180, SCOPE, "Dual Scope Type", 2, Kind::Choice(&["Over/Under", "Side by Side"])),
    item(181, SCOPE, "Dual Scope Auto Select", 2, OFF_ON),
    item(218, AUDIO, "FFT Scope Waveform Type", 2, Kind::Choice(&["Line", "Fill"])),
    item(220, AUDIO, "FFT Scope Waterfall Display", 2, OFF_ON),
    item(222, KEYER001, "Number Style", 2, NUMBER_STYLE),
    item(223, KEYER001, "Count Up Trigger", 2, COUNT_UP),
    item(224, KEYER001, "Present Number", 4, Kind::Num { min: 1, max: 9999, step: 1, unit: "", signed: false, div: 1 }),
    item(225, CWKEY, "Side Tone Level", 4, PERCENT),
    item(226, CWKEY, "Side Tone Level Limit", 2, OFF_ON),
    item(227, CWKEY, "Keyer Repeat Time", 2, Kind::Num { min: 1, max: 60, step: 1, unit: "s", signed: false, div: 1 }),
    item(228, CWKEY, "Dot/Dash Ratio (1:1:x)", 2, DOT_DASH),
    item(229, CWKEY, "Rise Time", 2, Kind::Choice(&["2 ms", "4 ms", "6 ms", "8 ms"])),
    item(230, CWKEY, "Paddle Polarity", 2, NORMAL_REVERSE),
    item(231, CWKEY, "Key Type", 2, KEY_TYPE),
    item(232, CWKEY, "MIC Up/Down Keyer", 2, OFF_ON),
    item(233, RTTY, "FFT Scope Averaging", 2, AVERAGING),
    item(235, RTTY, "Decode USOS", 2, OFF_ON),
    item(236, RTTY, "Decode New Line Code", 2, Kind::Choice(&["CR,LF,CR+LF", "CR+LF"])),
    item(237, RTTY, "Diddle", 2, Kind::Choice(&["OFF", "BLANK", "LTRS"])),
    item(238, RTTY, "TX USOS", 2, OFF_ON),
    item(239, RTTY, "Auto CR+LF by TX", 2, OFF_ON),
    item(240, RTTY, "Time Stamp", 2, OFF_ON),
    item(241, RTTY, "Time Stamp (Time)", 2, STAMP_TIME),
    item(242, RTTY, "Time Stamp (Frequency)", 2, OFF_ON),
    item(247, RTTY_LOG, "Decode Log", 2, OFF_ON),
    item(248, RTTY_LOG, "Log Set > File Type", 2, Kind::Choice(&["Text", "HTML"])),
    item(249, PSK, "FFT Scope Averaging", 2, AVERAGING),
    item(251, PSK, "AFC Range", 2, Kind::Choice(&["±8 Hz", "±15 Hz"])),
    item(252, PSK, "Time Stamp", 2, OFF_ON),
    item(253, PSK, "Time Stamp (Time)", 2, STAMP_TIME),
    item(254, PSK, "Time Stamp (Frequency)", 2, OFF_ON),
    item(259, PSK_LOG, "Decode Log", 2, OFF_ON),
    item(260, PSK_LOG, "Log Set > File Type", 2, Kind::Choice(&["Text", "HTML"])),
    item(261, SCAN, "SCAN Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(262, SCAN, "SCAN Resume", 2, OFF_ON),
    item(263, VOICE, "TX LEVEL", 4, PERCENT),
    item(264, VOICE, "VOICE TX SET > Auto Monitor", 2, OFF_ON),
    item(265, VOICE, "VOICE TX SET > Repeat Time", 2, Kind::Num { min: 1, max: 15, step: 1, unit: "s", signed: false, div: 1 }),
    item(266, REC, "REC Mode", 2, Kind::Choice(&["TX&RX", "RX Only"])),
    item(267, REC, "TX REC Audio", 2, Kind::Choice(&["Direct", "Monitor"])),
    item(268, REC, "RX REC Condition", 2, Kind::Choice(&["Always", "Squelch Auto"])),
    item(269, REC, "File Split", 2, OFF_ON),
    item(270, REC, "PTT Auto REC", 2, OFF_ON),
    item(271, REC, "PRE-REC for PTT Auto REC", 2, PRE_REC),
    item(272, PLAYER, "Skip Time", 2, SKIP_TIME),
    item(273, REPLAY, "REC Time", 2, Kind::Num { min: 5, max: 30, step: 1, unit: "s", signed: false, div: 1 }),
    item(274, REPLAY, "Play Time", 2, Kind::Num { min: 3, max: 10, step: 1, unit: "s", signed: false, div: 1 }),
    item(275, ANT, "TYPE SET > RX-ANT Connectors", 2, Kind::Choice(&["Connect a receive antenna", "Connect an external device"])),
    item(276, ANT, "ANTENNA MEMORY (0.03-1.60 MHz)", 4, ANT_MEMORY),
    item(277, ANT, "ANTENNA MEMORY (1.60-2.00 MHz)", 4, ANT_MEMORY),
    item(278, ANT, "ANTENNA MEMORY (2.00-6.00 MHz)", 4, ANT_MEMORY),
    item(279, ANT, "ANTENNA MEMORY (6.00-8.00 MHz)", 4, ANT_MEMORY),
    item(280, ANT, "ANTENNA MEMORY (8.00-11.00 MHz)", 4, ANT_MEMORY),
    item(281, ANT, "ANTENNA MEMORY (11.00-15.00 MHz)", 4, ANT_MEMORY),
    item(282, ANT, "ANTENNA MEMORY (15.00-20.00 MHz)", 4, ANT_MEMORY),
    item(283, ANT, "ANTENNA MEMORY (20.00-22.00 MHz)", 4, ANT_MEMORY),
    item(284, ANT, "ANTENNA MEMORY (22.00-26.00 MHz)", 4, ANT_MEMORY),
    item(285, ANT, "ANTENNA MEMORY (26.00-30.00 MHz)", 4, ANT_MEMORY),
    item(286, ANT, "ANTENNA MEMORY (30.00-45.00 MHz)", 4, ANT_MEMORY),
    item(287, ANT, "ANTENNA MEMORY (45.00-60.00 MHz)", 4, ANT_MEMORY),
    item(288, ANT, "Temporary memory (TEMP-M)", 2, OFF_ON),
    item(289, ANT, "Antenna selection mode ([ANT] SW)", 2, Kind::Choice(&["OFF", "Manual", "Auto"])),
    item(290, NB, "NB Depth", 2, NB_DEPTH),
    item(291, NB, "NB Width", 4, PERCENT),
    item(292, VOX, "VOX Delay", 2, VOX_DELAY),
    item(293, VOX, "VOX Voice Delay", 2, VOX_VOICE_DELAY),
    item(294, APF, "APF Type", 2, Kind::Choice(&["SHARP", "SOFT"])),
    item(295, APF, "APF AF Level", 2, Kind::Num { min: 0, max: 6, step: 1, unit: "dB", signed: false, div: 1 }),
    item(310, CONN, "IC-PW2 Dual Connection Mode", 2, OFF_ON),
    // Front-panel settings (their own CI-V commands, see EXTRA).
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
    item(9020, PANEL, "DIGI-SEL", 2, OFF_ON),
    item(9021, PANEL, "DIGI-SEL Shift", 4, PERCENT),
    item(9022, PANEL, "DRIVE Gain", 4, PERCENT),
    item(9023, PANEL, "MAIN/SUB Tracking", 2, OFF_ON),
    item(9024, PANEL, "DPD", 2, OFF_ON),
    item(9025, PANEL, "Dualwatch", 2, OFF_ON),
    item(9026, PANEL, "ANT-RX I/O", 2, OFF_ON),
];

pub static MENU: [MenuItem; RAW.len()] = icom::numbered::<{ RAW.len() }>(RAW);

pub const EXTRA: &[(u32, &[u8])] = &[
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
    (9020, &[0x16, 0x4E]),
    (9021, &[0x14, 0x13]),
    (9022, &[0x14, 0x14]),
    (9023, &[0x16, 0x5E]),
    (9024, &[0x16, 0x67]),
    (9025, &[0x07, 0xC2]),
    (9026, &[0x16, 0x53]),
];

/// The IC-7610's meters (command table, `15 xx`).
pub const PO_CAL: [(u8, f32); 3] = [(0, 0.0), (143, 50.0), (212, 100.0)];
pub const VD_CAL: [(u8, f32); 3] = [(0, 0.0), (151, 10.0), (211, 16.0)];
pub const ID_CAL: [(u8, f32); 4] = [(0, 0.0), (77, 10.0), (165, 20.0), (241, 30.0)];

pub static SPEC: CivSpec = CivSpec {
    modes: MODES,
    // The front panel offers 6, 12 and 18 dB (the CI-V guide lists 3-45 dB
    // in 3 dB steps).
    att: &[(0, 0x00), (1, 0x06), (2, 0x12), (3, 0x18)],
    max_watts: 100.0,
    s_cal: &icom::S_CAL,
    po_cal: &PO_CAL,
    swr_cal: &icom::SWR_CAL,
    alc_cal: &icom::ALC_CAL,
    comp_cal: &icom::COMP_CAL,
    id_cal: &ID_CAL,
    vd_cal: &VD_CAL,
    transceive: Some(112),
    keyer_memories: 8,
    voice_memories: 8,
    antenna_switch: true,
    apf: true,
    apf_widths: true,
    apf_hz: 550.0,
    lock: true,
    memory_name_len: 10,
    memory: MemLayout::Hf { split_copy: false },
    main_sub: true,
    antenna_rx: true,
    dual_band: false,
    tone_function: false,
    duplex: false,
    vox_delay: Some(292),
    keyer_type: Some(231),
    extra: EXTRA,
};

pub static IC7610: RadioModel = RadioModel {
    key: "ic7610",
    name: "IC-7610",
    maker: Maker::Icom,
    family: Family::Icom,
    proto: Proto::CiV(&SPEC),
    id: "",
    civ_addr: 0x98,
    hamlib: 3078,
    freq_min: 30_000,
    freq_max: 60_000_000,
    bands: BANDS,
    modes: MODE_LIST,
    menu: &MENU,
    roles: Roles {
        mic_sel: &[icom::num_of(&MENU, 91), icom::num_of(&MENU, 92), icom::num_of(&MENU, 93), icom::num_of(&MENU, 94)],
        mic_front: 0,
        mic_usb: 3,
        // USB1 (B) RTS.
        pc_keying: Some((icom::num_of(&MENU, 96), 0, 4)),
        cat_tot: None,
        cat_items: &[icom::num_of(&MENU, 112), icom::num_of(&MENU, 113), icom::num_of(&MENU, 115), icom::num_of(&MENU, 116)],
        cw_bfo: Some((icom::num_of(&MENU, 62), ["01", "00", "--"])),
        cw_pitch_display: None,
        data_other: None,
        data_disp: None,
        data_psk: false,
    },
    caps: Caps {
        commands: COMMANDS,
        widths: icom::widths,
        narrow: false,
        preamp: &[(0, "OFF"), (1, "P.AMP1"), (2, "P.AMP2")],
        att: &[(0, "OFF"), (1, "6 dB"), (2, "12 dB"), (3, "18 dB")],
        agc: &[(1, "FAST"), (2, "MID"), (3, "SLOW")],
        roofing: &[],
        roofing_answers: &[],
        nb: &[(0, "OFF"), (1, "NB")],
        nr_max: 15,
        nb_level_max: 100,
        antennas: 2,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialect::icom::num_of;
    use crate::dialect::icom::testing::*;

    #[test]
    fn ic7610_literal_frames() {
        let m = &IC7610;
        let mut r = radio(m);
        // VFO-A is MAIN (25 00), VFO-B is SUB (25 01), whichever is selected.
        assert_eq!(down(&mut r, "FA14074000;"), ["FE FE 98 E0 25 00 00 40 07 14 00 FD"]);
        up(&mut r, "FE FE E0 98 FB FD");
        assert_eq!(down(&mut r, "VS1;"), ["FE FE 98 E0 07 D1 FD"]);
        up(&mut r, "FE FE E0 98 FB FD");
        assert_eq!(down(&mut r, "FB07074000;"), ["FE FE 98 E0 25 01 00 40 07 07 00 FD"]);
        up(&mut r, "FE FE E0 98 FB FD");
        assert_eq!(down(&mut r, "FA;"), ["FE FE 98 E0 25 00 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 98 25 00 00 40 07 14 00 FD"), ["FA14074000;"]);
        assert_eq!(down(&mut r, "VS0;"), ["FE FE 98 E0 07 D0 FD"]);
        up(&mut r, "FE FE E0 98 FB FD");
        assert_eq!(down(&mut r, "AB;"), ["FE FE 98 E0 07 B1 FD"]);
        up(&mut r, "FE FE E0 98 FB FD");
        assert_eq!(down(&mut r, "SV;"), ["FE FE 98 E0 07 B0 FD"]);
        up(&mut r, "FE FE E0 98 FB FD");
        // DATA2 reads as DATA; PSK is 12.
        assert_eq!(down(&mut r, "MD0;"), ["FE FE 98 E0 26 00 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 98 26 00 01 02 01 FD"), ["MD0C;"]);
        assert_eq!(down(&mut r, "MD0I;"), ["FE FE 98 E0 26 00 12 00 01 FD"]);
        up(&mut r, "FE FE E0 98 FB FD");
        // Attenuator 6/12/18 dB.
        assert_eq!(down(&mut r, "RA03;"), ["FE FE 98 E0 11 18 FD"]);
        up(&mut r, "FE FE E0 98 FB FD");
        assert_eq!(down(&mut r, "RA0;"), ["FE FE 98 E0 11 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 98 11 12 FD"), ["RA02;"]);
        // Antennas: 12 0n with the RX antenna after it.
        assert_eq!(down(&mut r, "AN0;"), ["FE FE 98 E0 12 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 98 12 01 01 FD"), ["AN020;"]);
        assert_eq!(down(&mut r, "AN01;"), ["FE FE 98 E0 12 00 01 FD"]);
        up(&mut r, "FE FE E0 98 FB FD");
        let ipp = num_of(m.menu, 9001);
        assert_eq!(down(&mut r, &format!("EX{ipp:03}01;")), ["FE FE 98 E0 16 65 01 FD"]);
        up(&mut r, "FE FE E0 98 FB FD");
        let dw = num_of(m.menu, 9025);
        assert_eq!(down(&mut r, &format!("EX{dw:03};")), ["FE FE 98 E0 07 C2 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 98 07 C2 01 FD"), [format!("EX{dw:03}01;")]);
        let antm = num_of(m.menu, 276);
        assert_eq!(down(&mut r, &format!("EX{antm:03}0101;")), ["FE FE 98 E0 1A 05 02 76 01 01 FD"]);
        up(&mut r, "FE FE E0 98 FB FD");
        assert_eq!(down(&mut r, "VD0300;"), ["FE FE 98 E0 1A 05 02 92 03 FD"]);
        up(&mut r, "FE FE E0 98 FB FD");
        // PO: 02 12 = 100 W, 161 on RigHelm's scale.
        assert_eq!(down(&mut r, "RM5;"), ["FE FE 98 E0 15 11 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 98 15 11 02 12 FD"), ["RM5161;"]);
        // A memory without the transmit copy.
        assert_eq!(
            down(&mut r, "MW00107074000+000000C00000;"),
            ["FE FE 98 E0 1A 00 00 01 00 00 40 07 07 00 01 01 10 00 08 85 00 08 85 20 20 20 20 20 20 20 20 20 20 FD"]
        );
    }

    #[test]
    fn ic7610_round_trips() {
        let m = &IC7610;
        for f in [
            "FA14074000;", "FB07074000;", "MD0I;", "MD0C;", "MD03;", "RA01;", "RA03;", "AN01;", "AN02;", "CO0002;", "CO0230;", "VS1;", "VS0;", "AB;",
            "SV;", "MW00107074000+000000C00000;", "MW10014074000+000000402000;",
        ] {
            round_trip_set(m, f);
        }
        menu_round_trips(m);
        for (read, answer) in [("FB;", "FB07074000;"), ("RA0;", "RA02;"), ("AN0;", "AN020;"), ("MR001;", "MR00107074000+000000C10000;")] {
            round_trip_read(m, read, answer);
        }
    }
}
