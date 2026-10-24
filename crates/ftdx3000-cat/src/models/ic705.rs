//! Icom IC-705 (CI-V address A4h): 10 W, HF to 70 cm, with WFM and AM/FM
//! broadcast and air band reception and D-STAR (DV).
//!
//! Source: IC-705 CI-V Reference Guide (command table pp. 3-16, command
//! formats pp. 17-29).

use super::ic7300::{
    AF_SQL, BAND_EDGE_BEEP, CLOCK, COUNT_UP, DOT_DASH, KEY_TYPE, MK2_USB_LINE as USB_LINE_AB, NB_DEPTH, NORMAL_REVERSE, NUMBER_STYLE,
    PANEL, VOX_DELAY, WIDTH3,
};
use crate::dialect::icom::{self, AVERAGING, CivSpec, MemLayout, FILL, NOTCH_SW, OFF_ON, PERCENT, SPEED3, TBW, TONE, TOT, TX_DELAY, item};
use crate::menu::{Kind, MenuItem};
use crate::model::{Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::types::{Band, Mode};

/// Band stacking codes (`1A 01`): the HF bands, 6 m, FM broadcast, air
/// band, 2 m and 70 cm.
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
    Band { code: 11, label: "FM BC", lo: 74_800_000, hi: 107_999_999 },
    Band { code: 12, label: "AIR", lo: 108_000_000, hi: 136_999_999 },
    Band { code: 13, label: "2m", lo: 144_000_000, hi: 148_000_000 },
    Band { code: 14, label: "70cm", lo: 420_000_000, hi: 450_000_000 },
    Band { code: 15, label: "GEN", lo: 30_000, hi: 470_000_000 },
];

/// Icom mode code and DATA for each RigHelm mode.
pub const MODES: &[(Mode, u8, bool)] = &[
    (Mode::Lsb, 0x00, false),
    (Mode::Usb, 0x01, false),
    (Mode::Cw, 0x03, false),
    (Mode::CwR, 0x07, false),
    (Mode::RttyLsb, 0x04, false),
    (Mode::RttyUsb, 0x08, false),
    (Mode::Am, 0x02, false),
    (Mode::Fm, 0x05, false),
    (Mode::Wfm, 0x06, false),
    (Mode::Dv, 0x17, false),
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
    Mode::Am,
    Mode::Fm,
    Mode::Wfm,
    Mode::Dv,
    Mode::DataLsb,
    Mode::DataUsb,
    Mode::DataAm,
    Mode::DataFm,
];

/// RigHelm commands the IC-705 carries out (no APF, no IP+).
pub const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AM", "BA", "BC", "BD", "BI", "BP00", "BS", "BU", "BY", "EX", "FA", "FB", "FR", "FT", "GT", "ID", "IF", "KM", "KP",
    "KR", "KS", "KY", "LK", "MA", "MC", "MD", "MG", "ML0", "ML1", "MR", "MW", "MX", "NB", "NL", "NR", "OI", "PA", "PB", "PC", "PL", "PR0", "PS", "RA", "RC", "RD", "RG", "RL",
    "RM", "RT", "RU", "SH", "SM", "SQ", "SV", "TX", "VD", "VG", "VS", "VX", "XT",
];

const CSV_SEP: Kind = Kind::Choice(&["Separator , / Decimal .", "Separator ; / Decimal .", "Separator ; / Decimal ,"]);
const MOD_705: Kind = Kind::Choice(&["MIC", "USB", "MIC,USB", "WLAN"]);
const DATE_2020: Kind = Kind::Num { min: 20200101, max: 20991231, step: 1, unit: "", signed: false, div: 1 };
const CENTER_TYPE: Kind = Kind::Choice(&["Filter Center", "Carrier Point Center", "Carrier Point Center (Abs. Freq.)"]);
const GRID: Kind = Kind::Choice(&["Grid 1", "Grid 2", "Grid 3", "Grid 4", "Grid 5", "Grid 6", "Grid 7", "Grid 8"]);
const PRESENT_NUMBER: Kind = Kind::Num { min: 1, max: 9999, step: 1, unit: "", signed: false, div: 1 };
const PAUSE_TIMER: Kind = Kind::Choice(&["2 sec", "4 sec", "6 sec", "8 sec", "10 sec", "12 sec", "14 sec", "16 sec", "18 sec", "20 sec", "HOLD"]);
const RESUME_TIMER: Kind = Kind::Choice(&["0 sec", "1 sec", "2 sec", "3 sec", "4 sec", "5 sec", "HOLD"]);
const COURSE: Kind = Kind::Num { min: 0, max: 360, step: 1, unit: "°", signed: false, div: 1 };
const SPEED_KMH: Kind = Kind::Num { min: 0, max: 1850, step: 1, unit: "km/h", signed: false, div: 1 };
const SSID: Kind = Kind::Choice(&[
    "---", "(-0)", "-1", "-2", "-3", "-4", "-5", "-6", "-7", "-8", "-9", "-10", "-11", "-12", "-13", "-14", "-15", "-A", "-B", "-C", "-D", "-E",
    "-F", "-G", "-H", "-I", "-J", "-K", "-L", "-M", "-N", "-O", "-P", "-Q", "-R", "-S", "-T", "-U", "-V", "-W", "-X", "-Y", "-Z",
]);
const KEY_VOX_705: Kind = Kind::Choice(&[
    "TRANSMIT",
    "TUNER",
    "VOX/BK-IN",
    "P.AMP/ATT",
    "NOTCH",
    "NB",
    "NR",
    "SPLIT",
    "A/B",
    "VFO/MEMO",
    "CD",
    "PRESET",
    "Home CH",
    "Temporary Skip",
    "Memory 1",
    "Memory 2",
    "Memory 3",
    "Memory 4",
]);
const MIC_KEY_705: Kind = Kind::Choice(&[
    "No function",
    "UP",
    "DOWN",
    "UP (VFO: kHz)",
    "DOWN (VFO: kHz)",
    "VOL UP",
    "VOL DOWN",
    "XFC",
    "CALL",
    "VFO/MEMO",
    "DR",
    "FROM/TO (DR)",
    "Home CH",
    "BAND/GROUP UP",
    "BAND/GROUP DOWN",
    "SCAN",
    "Temporary Skip",
    "SPEECH",
    "MODE",
    "RF Power",
    "Memory 1",
    "Memory 2",
    "Memory 3",
    "Memory 4",
    "T-CALL",
    "RX>CS",
    "TS",
    "MPAD",
    "SPLIT",
    "A/B",
    "TUNER",
]);
const TUNING_STEP_705: Kind = Kind::Choice(&[
    "OFF (10 Hz or 1 Hz)",
    "100 Hz",
    "500 Hz",
    "1 kHz",
    "5 kHz",
    "6.25 kHz",
    "8.33 kHz",
    "9 kHz",
    "10 kHz",
    "12.5 kHz",
    "20 kHz",
    "25 kHz",
    "50 kHz",
    "100 kHz",
]);
pub const TONE_FUNCTION: Kind = Kind::Coded(&[
    (0, "OFF"),
    (1, "TONE"),
    (2, "TSQL"),
    (3, "DTCS"),
    (6, "DTCS (T)"),
    (7, "TONE (T)/DTCS (R)"),
    (8, "DTCS (T)/TSQL (R)"),
    (9, "TONE (T)/TSQL (R)"),
]);

const RAW: &[MenuItem] = &[
    // RX HPF/LPF (00 01, 00 04, 00 07, 00 10, 00 15, 00 16) are two numbers
    // in one; SPLIT Offset (00 46) a frequency; UTC Offset (01 70) an offset
    // and a sign; the colours, the scope's fixed edges (01 88-02 38,
    // 03 69-03 85), the manual positions (02 86, 03 12, 03 26) and the
    // alarm area (03 46) other multi-part values: all left out.
    item(2, "SET > Tone Control/TBW > RX > SSB", "RX Tone (Bass)", 2, TONE),
    item(3, "SET > Tone Control/TBW > RX > SSB", "RX Tone (Treble)", 2, TONE),
    item(5, "SET > Tone Control/TBW > RX > AM", "RX Tone (Bass)", 2, TONE),
    item(6, "SET > Tone Control/TBW > RX > AM", "RX Tone (Treble)", 2, TONE),
    item(8, "SET > Tone Control/TBW > RX > FM", "RX Tone (Bass)", 2, TONE),
    item(9, "SET > Tone Control/TBW > RX > FM", "RX Tone (Treble)", 2, TONE),
    item(11, "SET > Tone Control/TBW > RX > DV", "RX Tone (Bass)", 2, TONE),
    item(12, "SET > Tone Control/TBW > RX > DV", "RX Tone (Treble)", 2, TONE),
    item(13, "SET > Tone Control/TBW > RX > WFM", "RX Tone (Bass)", 2, TONE),
    item(14, "SET > Tone Control/TBW > RX > WFM", "RX Tone (Treble)", 2, TONE),
    item(17, "SET > Tone Control/TBW > TX > SSB", "TX Tone (Bass)", 2, TONE),
    item(18, "SET > Tone Control/TBW > TX > SSB", "TX Tone (Treble)", 2, TONE),
    item(19, "SET > Tone Control/TBW > TX > SSB", "SSB TBW (WIDE)", 2, TBW),
    item(20, "SET > Tone Control/TBW > TX > SSB", "SSB TBW (MID)", 2, TBW),
    item(21, "SET > Tone Control/TBW > TX > SSB", "SSB TBW (NAR)", 2, TBW),
    item(22, "SET > Tone Control/TBW > TX > SSB", "SSB-D TBW", 2, TBW),
    item(23, "SET > Tone Control/TBW > TX > AM", "TX Tone (Bass)", 2, TONE),
    item(24, "SET > Tone Control/TBW > TX > AM", "TX Tone (Treble)", 2, TONE),
    item(25, "SET > Tone Control/TBW > TX > FM", "TX Tone (Bass)", 2, TONE),
    item(26, "SET > Tone Control/TBW > TX > FM", "TX Tone (Treble)", 2, TONE),
    item(27, "SET > Tone Control/TBW > TX > DV", "TX Tone (Bass)", 2, TONE),
    item(28, "SET > Tone Control/TBW > TX > DV", "TX Tone (Treble)", 2, TONE),
    item(29, "SET > Function", "Beep Level", 4, PERCENT),
    item(30, "SET > Function", "Beep Level Limit", 2, OFF_ON),
    item(31, "SET > Function", "Beep (Confirmation)", 2, OFF_ON),
    item(32, "SET > Function", "Home CH Beep", 2, OFF_ON),
    item(33, "SET > Function", "Band Edge Beep", 2, BAND_EDGE_BEEP),
    item(34, "SET > Function", "Auto Power OFF", 2, Kind::Choice(&["OFF", "30 min", "60 min", "90 min", "120 min"])),
    item(35, "SET > Function", "Power Save", 2, Kind::Choice(&["OFF", "Auto (Short)", "Auto (Middle)", "Auto (Long)"])),
    item(36, "SET > Function", "Max TX Power (Battery Pack)", 2, Kind::Choice(&["0.5 W", "1 W", "2.5 W", "5 W"])),
    item(37, "SET > Function", "Max TX Power (DC 13.8V)", 2, Kind::Choice(&["0.5 W", "1 W", "2.5 W", "5 W", "10 W"])),
    item(38, "SET > Function", "TX Delay (HF)", 2, TX_DELAY),
    item(39, "SET > Function", "TX Delay (50 MHz)", 2, TX_DELAY),
    item(41, "SET > Function", "TX Delay (144 MHz)", 2, TX_DELAY),
    item(42, "SET > Function", "TX Delay (430 MHz)", 2, TX_DELAY),
    item(43, "SET > Function", "Time-Out Timer", 2, TOT),
    item(44, "SET > Function", "PTT Lock", 2, OFF_ON),
    item(45, "SET > Function > SPLIT", "Quick SPLIT", 2, OFF_ON),
    item(47, "SET > Function > SPLIT", "SPLIT LOCK", 2, OFF_ON),
    item(48, "SET > Function > Tuner", "PTT Start", 2, OFF_ON),
    item(49, "SET > Function", "Auto Repeater", 2, Kind::Choice(&["OFF", "ON (DUP)", "ON (DUP,TONE)"])),
    item(50, "SET > Function", "RTTY Mark Frequency", 2, Kind::Choice(&["1275 Hz", "1615 Hz", "2125 Hz"])),
    item(51, "SET > Function", "RTTY Shift Width", 2, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz"])),
    item(52, "SET > Function", "RTTY Keying Polarity", 2, NORMAL_REVERSE),
    item(53, "SET > Function > SPEECH", "SPEECH Language", 2, Kind::Choice(&["Japanese", "English"])),
    item(54, "SET > Function > SPEECH", "Alphabet", 2, Kind::Choice(&["Normal", "Phonetic Code"])),
    item(55, "SET > Function > SPEECH", "SPEECH Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(56, "SET > Function > SPEECH", "RX Call Sign SPEECH", 2, Kind::Choice(&["OFF", "ON (Kerchunk)", "ON (All)"])),
    item(57, "SET > Function > SPEECH", "RX>CS SPEECH", 2, OFF_ON),
    item(58, "SET > Function > SPEECH", "MIC Up/Down SPEECH", 2, OFF_ON),
    item(59, "SET > Function > SPEECH", "S-Level SPEECH", 2, OFF_ON),
    item(60, "SET > Function > SPEECH", "MODE SPEECH", 2, OFF_ON),
    item(61, "SET > Function > SPEECH", "SPEECH Level", 4, PERCENT),
    item(62, "SET > Function", "[SPEECH/LOCK] Switch", 2, Kind::Choice(&["SPEECH/LOCK", "LOCK/SPEECH"])),
    item(63, "SET > Function", "Lock Function", 2, Kind::Choice(&["MAIN DIAL", "PANEL"])),
    item(64, "SET > Function", "Memo Pad Quantity", 2, Kind::Choice(&["5 ch", "10 ch"])),
    item(65, "SET > Function", "MAIN DIALAuto TS", 2, Kind::Choice(&["OFF", "Low", "High"])),
    item(66, "SET > Function", "MIC Up/Down Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(67, "SET > Function", "[NOTCH] Switch (SSB)", 2, NOTCH_SW),
    item(68, "SET > Function", "[NOTCH] Switch (AM)", 2, NOTCH_SW),
    item(69, "SET > Function", "SSB/CW Synchronous Tuning", 2, OFF_ON),
    item(70, "SET > Function", "CW Normal Side", 2, Kind::Choice(&["LSB", "USB"])),
    item(71, "SET > Function", "Charging (Power ON)", 2, OFF_ON),
    item(72, "SET > Function", "USB Power Input (Phone, Tablet, PC)", 2, OFF_ON),
    item(73, "SET > Function", "Power OFF Setting (for Remote Control)", 2, Kind::Choice(&["Shutdown only", "Standby/Shutdown"])),
    item(74, "SET > Function > Remote MIC Key", "[A]", 2, MIC_KEY_705),
    item(75, "SET > Function > Remote MIC Key", "[B]", 2, MIC_KEY_705),
    item(76, "SET > Function > Remote MIC Key", "[▲]", 2, MIC_KEY_705),
    item(77, "SET > Function > Remote MIC Key", "[▼]", 2, MIC_KEY_705),
    item(78, "SET > Function > Remote MIC Key", "Mode Select (SSB)", 2, OFF_ON),
    item(79, "SET > Function > Remote MIC Key", "Mode Select (CW)", 2, OFF_ON),
    item(80, "SET > Function > Remote MIC Key", "Mode Select (RTTY)", 2, OFF_ON),
    item(81, "SET > Function > Remote MIC Key", "Mode Select (AM)", 2, OFF_ON),
    item(82, "SET > Function > Remote MIC Key", "Mode Select (FM)", 2, OFF_ON),
    item(83, "SET > Function > Remote MIC Key", "Mode Select (DV)", 2, OFF_ON),
    item(84, "SET > Function > Remote MIC Key", "Mode Select (WFM)", 2, OFF_ON),
    item(85, "SET > Function", "Keyboard Type", 2, Kind::Choice(&["Ten-key", "Full Keyboard"])),
    item(86, "SET > Function", "Full Keyboard Layout", 2, Kind::Choice(&["English", "German", "French"])),
    item(87, "SET > Function", "Screen Capture [POWER] Switch", 2, OFF_ON),
    item(88, "SET > Function", "Screen Capture File Type", 2, Kind::Choice(&["PNG", "BMP"])),
    item(89, "SET > Function", "REF Adjust", 4, Kind::Percent { max: 511 }),
    item(90, "SET > DV Set", "Standby Beep", 2, Kind::Choice(&["OFF", "ON", "ON (to me: High Tone)", "ON (to me: Alarm/High Tone)"])),
    item(91, "SET > DV Set", "Auto Reply", 2, Kind::Choice(&["OFF", "ON", "Voice", "Position"])),
    item(92, "SET > DV Set", "DV Data TX", 2, Kind::Choice(&["PTT", "Auto"])),
    item(93, "SET > DV Set > DV Fast Data", "Fast Data", 2, OFF_ON),
    item(94, "SET > DV Set > DV Fast Data", "GPS Data Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(95, "SET > DV Set > DV Fast Data", "TX Delay (PTT)", 2, Kind::Steps { max_code: 10, off_at_zero: true, first: 1, step: 1, unit: "s" }),
    item(96, "SET > DV Set", "Digital Monitor", 2, Kind::Choice(&["Auto", "Digital", "Analog"])),
    item(97, "SET > DV Set", "Digital Repeater Set", 2, OFF_ON),
    item(98, "SET > DV Set", "DV Auto Detect", 2, OFF_ON),
    item(99, "SET > DV Set", "RX Record (RPT)", 2, Kind::Choice(&["ALL", "Latest Only"])),
    item(100, "SET > DV Set", "BK", 2, OFF_ON),
    item(101, "SET > DV Set", "EMR", 2, OFF_ON),
    item(102, "SET > DV Set", "EMR AF Level", 4, PERCENT),
    item(103, "SET > QSO/RX Log", "QSO Log", 2, OFF_ON),
    item(104, "SET > QSO/RX Log", "RX History Log", 2, OFF_ON),
    item(105, "SET > QSO/RX Log > CSV Format", "Separator/Decimal", 2, CSV_SEP),
    item(106, "SET > QSO/RX Log > CSV Format", "Date", 2, Kind::Choice(&["yyyy/mm/dd", "mm/dd/yyyy", "dd/mm/yyyy"])),
    item(107, "SET > Connectors", "SP Jack Function", 2, Kind::Choice(&["Speaker", "Phone", "Phone (L+R)"])),
    item(108, "SET > Connectors", "Phones Level", 2, Kind::Steps { max_code: 30, off_at_zero: false, first: -15, step: 1, unit: "" }),
    item(109, "SET > Connectors > USB AF/IF Output", "Output Select", 2, Kind::Choice(&["AF", "IF"])),
    item(110, "SET > Connectors > USB AF/IF Output", "AF Output Level", 4, PERCENT),
    item(111, "SET > Connectors > USB AF/IF Output", "AF SQL", 2, AF_SQL),
    item(112, "SET > Connectors > USB AF/IF Output", "AF Beep/Speech... Output", 2, OFF_ON),
    item(113, "SET > Connectors > USB AF/IF Output", "IF Output Level", 4, PERCENT),
    item(114, "SET > Connectors > WLAN AF/IF Output", "Output Select", 2, Kind::Choice(&["AF", "IF"])),
    item(115, "SET > Connectors > WLAN AF/IF Output", "AF SQL", 2, AF_SQL),
    item(116, "SET > Connectors > MOD Input", "USB MOD Level", 4, PERCENT),
    item(117, "SET > Connectors > MOD Input", "WLAN MOD Level", 4, PERCENT),
    item(118, "SET > Connectors > MOD Input", "DATA OFF MOD", 2, MOD_705),
    item(119, "SET > Connectors > MOD Input", "DATA MOD", 2, MOD_705),
    item(120, "SET > Connectors > SEND Output", "HF", 2, OFF_ON),
    item(121, "SET > Connectors > SEND Output", "50M", 2, OFF_ON),
    item(123, "SET > Connectors > SEND Output", "144M", 2, OFF_ON),
    item(124, "SET > Connectors > SEND Output", "430M", 2, OFF_ON),
    item(125, "SET > Connectors > USB SEND/Keying", "USB SEND", 2, USB_LINE_AB),
    item(126, "SET > Connectors > USB SEND/Keying", "USB Keying (CW)", 2, USB_LINE_AB),
    item(127, "SET > Connectors > USB SEND/Keying", "USB Keying (RTTY)", 2, USB_LINE_AB),
    item(128, "SET > Connectors > External Keypad", "VOICE", 2, OFF_ON),
    item(129, "SET > Connectors > External Keypad", "KEYER", 2, OFF_ON),
    item(130, "SET > Connectors > External Keypad", "RTTY", 2, OFF_ON),
    item(131, "SET > Connectors > CI-V", "CI-V Transceive", 2, OFF_ON),
    item(132, "SET > Connectors > CI-V", "CI-V USB Echo Back", 2, OFF_ON),
    item(133, "SET > Connectors > USB (B) Function", "USB (B) Function", 2, Kind::Choice(&["OFF", "RTTY Decode", "DV Data", "Weather"])),
    item(134, "SET > Connectors > USB (B) Function", "GPS Out", 2, OFF_ON),
    item(135, "SET > Connectors", "MIC Jack 8V Output", 2, OFF_ON),
    item(136, "SET > Display", "LCD Backlight", 4, PERCENT),
    item(137, "SET > Display", "LCD Backlight Auto Adjust", 2, OFF_ON),
    item(138, "SET > Display", "Screen Saver (Battery Pack)", 2, Kind::Choice(&["OFF", "1min", "2min", "5min", "15min", "30min", "60min"])),
    item(139, "SET > Display", "Screen Saver (DC 13.8 V)", 2, Kind::Choice(&["OFF", "1min", "2min", "5min", "15min", "30min", "60min"])),
    item(140, "SET > Display", "Screen OFF [POWER] Switch", 2, OFF_ON),
    item(141, "SET > Display", "RX LED", 2, OFF_ON),
    item(142, "SET > Display", "Meter Peak Hold", 2, OFF_ON),
    item(143, "SET > Display", "Memory Name", 2, OFF_ON),
    item(144, "SET > Display", "Group Name Popup", 2, OFF_ON),
    item(145, "SET > Display", "RX Call Sign Display", 2, Kind::Choice(&["OFF", "Normal", "RX Hold", "Hold"])),
    item(146, "SET > Display", "RX Position Indicator", 2, OFF_ON),
    item(147, "SET > Display", "RX Position Display", 2, OFF_ON),
    item(148, "SET > Display", "RX Position Display Timer", 2, Kind::Choice(&["5sec", "10sec", "15sec", "30sec", "Hold"])),
    item(149, "SET > Display", "Reply Position Display", 2, OFF_ON),
    item(150, "SET > Display", "RX Picture Indicator", 2, OFF_ON),
    item(151, "SET > Display", "DV RX Backlight", 2, OFF_ON),
    item(152, "SET > Display", "TX Call Sign Display", 2, Kind::Choice(&["OFF", "Your Call Sign", "My Call Sign"])),
    item(153, "SET > Display", "Scroll Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(154, "SET > Display", "Opening Message", 2, OFF_ON),
    item(155, "SET > Display", "Power ON Check", 2, OFF_ON),
    item(156, "SET > Display > Display Unit", "Latitude/Longitude", 2, Kind::Choice(&["ddd°mm.mm′", "ddd°mm′ss″", "ddd.dddd°"])),
    item(157, "SET > Display > Display Unit", "Altitude/Distance", 2, Kind::Choice(&["m", "ft/mi"])),
    item(158, "SET > Display > Display Unit", "Speed", 2, Kind::Choice(&["km/h", "mph", "knots"])),
    item(159, "SET > Display > Display Unit", "Temperature", 2, Kind::Choice(&["°C", "°F"])),
    item(160, "SET > Display > Display Unit", "Barometric", 2, Kind::Choice(&["hPa", "mb", "mmHg", "inHg"])),
    item(161, "SET > Display > Display Unit", "Rainfall", 2, Kind::Choice(&["mm", "inch"])),
    item(162, "SET > Display > Display Unit", "Wind Speed", 2, Kind::Choice(&["m/s", "km/h", "mph", "knots"])),
    item(163, "SET > Display", "Display Language", 2, Kind::Choice(&["English", "Japanese"])),
    item(164, "SET > Display", "System Language", 2, Kind::Choice(&["English", "Japanese"])),
    item(165, "SET > Time Set > Date/Time", "Date (YYYYMMDD)", 8, DATE_2020),
    item(166, "SET > Time Set > Date/Time", "Time (HHMM)", 4, CLOCK),
    item(167, "SET > Time Set > Date/Time", "NTP Function", 2, OFF_ON),
    item(168, "SET > Time Set > Date/Time", "NTP Server Address", 64, Kind::Text { max: 64 }),
    item(169, "SET > Time Set > Date/Time", "GPS Time Correct", 2, Kind::Choice(&["OFF", "Auto"])),
    item(171, "SET > SD Card > Import/Export > CSV Format", "Separator/Decimal", 2, CSV_SEP),
    item(172, "SET > SD Card > Import/Export > CSV Format", "Date", 2, Kind::Choice(&["yyyy/mm/dd", "mm/dd/yyyy", "dd/mm/yyyy"])),
    item(173, "SCOPE > SCOPE SET", "Scope during Tx (CENTER Type)", 2, OFF_ON),
    item(174, "SCOPE > SCOPE SET", "Max Hold", 2, Kind::Choice(&["OFF", "10s Hold", "ON"])),
    item(175, "SCOPE > SCOPE SET", "CENTER Type Display", 2, CENTER_TYPE),
    item(176, "SCOPE > SCOPE SET", "Marker Position (FIX Type/SCROLL Type)", 2, Kind::Choice(&["Filter Center", "Carrier Point"])),
    item(177, "SCOPE > SCOPE SET", "VBW", 2, Kind::Choice(&["Narrow", "Wide"])),
    item(178, "SCOPE > SCOPE SET", "Averaging", 2, AVERAGING),
    item(179, "SCOPE > SCOPE SET", "Waveform Type", 2, FILL),
    item(183, "SCOPE > SCOPE SET", "Waterfall Display", 2, OFF_ON),
    item(184, "SCOPE > SCOPE SET", "Waterfall Speed", 2, SPEED3),
    item(185, "SCOPE > SCOPE SET", "Waterfall Size (Expand Screen)", 2, Kind::Choice(&["Small", "Mid", "Large"])),
    item(186, "SCOPE > SCOPE SET", "Waterfall Peak Color Level", 2, GRID),
    item(187, "SCOPE > SCOPE SET", "Waterfall Marker Auto-hide", 2, OFF_ON),
    item(239, "AUDIO > AUDIO SCOPE SET", "FFT Scope Waveform Type", 2, Kind::Choice(&["Line", "Fill"])),
    item(241, "AUDIO > AUDIO SCOPE SET", "FFT Scope Waterfall Display", 2, OFF_ON),
    item(243, "VOICE", "TX LEVEL", 4, PERCENT),
    item(244, "VOICE > VOICE TX SET", "Auto Monitor", 2, OFF_ON),
    item(245, "VOICE > VOICE TX SET", "Repeat Time", 2, Kind::Num { min: 1, max: 15, step: 1, unit: "s", signed: false, div: 1 }),
    item(246, "KEYER > KEYER 001", "Number Style", 2, NUMBER_STYLE),
    item(247, "KEYER > KEYER 001", "Count Up Trigger", 2, COUNT_UP),
    item(248, "KEYER > KEYER 001", "Present Number", 4, PRESENT_NUMBER),
    item(249, "KEYER > CW-KEY SET", "Side Tone Level", 4, PERCENT),
    item(250, "KEYER > CW-KEY SET", "Side Tone Level Limit", 2, OFF_ON),
    item(251, "KEYER > CW-KEY SET", "Keyer Repeat Time", 2, Kind::Num { min: 1, max: 60, step: 1, unit: "s", signed: false, div: 1 }),
    item(252, "KEYER > CW-KEY SET", "Dot/Dash Ratio (1:1:x)", 2, DOT_DASH),
    item(253, "KEYER > CW-KEY SET", "Rise Time", 2, Kind::Choice(&["2ms", "4ms", "6ms", "8ms"])),
    item(254, "KEYER > CW-KEY SET", "Paddle Polarity", 2, NORMAL_REVERSE),
    item(255, "KEYER > CW-KEY SET", "Key Type", 2, KEY_TYPE),
    item(256, "KEYER > CW-KEY SET", "MIC Up/Down Keyer", 2, Kind::Choice(&["OFF", "ON (UP/DOWN)", "ON (A/B)"])),
    item(257, "DECODE > RTTY DECODE SET", "FFT Scope Averaging", 2, AVERAGING),
    item(259, "DECODE > RTTY DECODE SET", "Decode USOS", 2, OFF_ON),
    item(260, "DECODE > RTTY DECODE SET", "Decode New Line Code", 2, Kind::Choice(&["CR,LF,CR+LF", "CR+LF"])),
    item(261, "DECODE > RTTY DECODE SET", "TX USOS", 2, OFF_ON),
    item(264, "DECODE > RTTY DECODE LOG", "Decode Log", 2, OFF_ON),
    item(265, "DECODE > RTTY DECODE LOG > Log Set", "File Type", 2, Kind::Choice(&["Text", "HTML"])),
    item(266, "DECODE > RTTY DECODE LOG > Log Set", "Time Stamp", 2, OFF_ON),
    item(267, "DECODE > RTTY DECODE LOG > Log Set", "Time Stamp (Time)", 2, Kind::Choice(&["Local", "UTC"])),
    item(268, "DECODE > RTTY DECODE LOG > Log Set", "Time Stamp (Frequency)", 2, OFF_ON),
    item(269, "RECORD > Recorder Set", "TX REC Audio", 2, Kind::Choice(&["Direct", "Monitor"])),
    item(270, "RECORD > Recorder Set", "RX REC Condition", 2, Kind::Choice(&["Always", "Squelch Auto"])),
    item(271, "RECORD > Recorder Set", "File Split", 2, OFF_ON),
    item(272, "RECORD > Recorder Set", "PTT Auto REC", 2, OFF_ON),
    item(273, "RECORD > Recorder Set", "PRE-REC for PTT Auto REC", 2, Kind::Choice(&["OFF", "5sec", "10sec", "15sec"])),
    item(274, "RECORD > Player Set", "Skip Time", 2, Kind::Choice(&["3sec", "5sec", "10sec", "30sec"])),
    item(275, "SCAN > SCAN SET", "SCAN Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(276, "SCAN > SCAN SET", "SCAN Resume", 2, OFF_ON),
    item(277, "SCAN > SCAN SET", "Pause Timer", 2, PAUSE_TIMER),
    item(278, "SCAN > SCAN SET", "Resume Timer", 2, RESUME_TIMER),
    item(279, "SCAN > SCAN SET", "Temporary Skip Timer", 2, Kind::Choice(&["5min", "10min", "15min", "While Scanning", "While Powered ON"])),
    item(280, "SCAN > SCAN SET", "MAIN DIAL Operation (SCAN)", 2, Kind::Choice(&["OFF", "Up/Down"])),
    item(281, "GPS > GPS Set", "GPS Select", 2, Kind::Choice(&["OFF", "ON", "Manual"])),
    item(282, "GPS > GPS Set > GPS Option", "SBAS", 2, OFF_ON),
    item(283, "GPS > GPS Set > GPS Option", "GLONASS", 2, OFF_ON),
    item(284, "GPS > GPS Set > GPS Option", "Power Save", 2, Kind::Choice(&["OFF", "1min", "2min", "4min", "8min", "Auto"])),
    item(285, "GPS > GPS Set > GPS Option", "Satellite Information Out", 2, Kind::Choice(&["GPS/QZSS/GLONASS", "GPS Only"])),
    item(287, "GPS", "GPS TX Mode", 2, Kind::Choice(&["OFF", "D-PRS", "NMEA"])),
    item(288, "GPS > GPS TX Mode > D-PRS", "Unproto Address", 56, Kind::Text { max: 56 }),
    item(289, "GPS > GPS TX Mode > D-PRS", "TX Format", 2, Kind::Choice(&["Position", "Object", "Item", "Weather"])),
    item(290, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Symbol", 2, Kind::Choice(&["No.1", "No.2", "No.3", "No.4"])),
    item(291, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Symbol No.1", 2, Kind::Text { max: 2 }),
    item(292, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Symbol No.2", 2, Kind::Text { max: 2 }),
    item(293, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Symbol No.3", 2, Kind::Text { max: 2 }),
    item(294, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Symbol No.4", 2, Kind::Text { max: 2 }),
    item(295, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "SSID", 2, SSID),
    item(296, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Comment", 2, Kind::Choice(&["No.1", "No.2", "No.3", "No.4"])),
    item(297, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Comment No.1", 43, Kind::Text { max: 43 }),
    item(298, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Comment No.2", 43, Kind::Text { max: 43 }),
    item(299, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Comment No.3", 43, Kind::Text { max: 43 }),
    item(300, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Comment No.4", 43, Kind::Text { max: 43 }),
    item(301, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Time Stamp", 2, Kind::Choice(&["OFF", "DHM", "HMS"])),
    item(302, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Altitude", 2, OFF_ON),
    item(303, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Data Extension", 2, Kind::Choice(&["OFF", "Course/Speed", "Power/Height/Gain/Directivity"])),
    item(304, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Power", 2, Kind::Choice(&["0W", "1W", "4W", "9W", "16W", "25W", "36W", "49W", "64W", "81W"])),
    item(305, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Height", 2, Kind::Choice(&["3m", "6m", "12m", "24m", "49m", "98m", "195m", "390m", "780m", "1561m"])),
    item(306, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Gain", 2, Kind::Num { min: 0, max: 9, step: 1, unit: "dB", signed: false, div: 1 }),
    item(307, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Directivity", 2, Kind::Choice(&["Omni", "45ºNE", "90ºE", "135ºSE", "180ºS", "225ºSW", "270ºW", "315ºNW", "360ºN"])),
    item(308, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Object Name", 9, Kind::Text { max: 9 }),
    item(309, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Data Type", 2, Kind::Choice(&["Live Object", "Kill Object"])),
    item(310, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Symbol", 2, Kind::Text { max: 2 }),
    item(311, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Comment", 43, Kind::Text { max: 43 }),
    item(313, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Data Extension", 2, Kind::Choice(&["OFF", "Course/Speed", "Power/Height/Gain/Directivity"])),
    item(314, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Course", 4, COURSE),
    item(315, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Speed (km/h)", 4, SPEED_KMH),
    item(316, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Power", 2, Kind::Choice(&["0W", "1W", "4W", "9W", "16W", "25W", "36W", "49W", "64W", "81W"])),
    item(317, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Height", 2, Kind::Choice(&["3m", "6m", "12m", "24m", "49m", "98m", "195m", "390m", "780m", "1561m"])),
    item(318, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Gain", 2, Kind::Num { min: 0, max: 9, step: 1, unit: "dB", signed: false, div: 1 }),
    item(319, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Directivity", 2, Kind::Choice(&["Omni", "45ºNE", "90ºE", "135ºSE", "180ºS", "225ºSW", "270ºW", "315ºNW", "360ºN"])),
    item(320, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "SSID", 2, SSID),
    item(321, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Time Stamp", 2, Kind::Choice(&["DHM", "HMS"])),
    item(322, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Item Name", 9, Kind::Text { max: 9 }),
    item(323, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Data Type", 2, Kind::Choice(&["Live Item", "Killed Item"])),
    item(324, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Symbol", 2, Kind::Text { max: 2 }),
    item(325, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Comment", 43, Kind::Text { max: 43 }),
    item(327, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Data Extension", 2, Kind::Choice(&["OFF", "Course/Speed", "Power/Height/Gain/Directivity"])),
    item(328, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Course", 4, COURSE),
    item(329, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Speed (km/h)", 4, SPEED_KMH),
    item(330, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Power", 2, Kind::Choice(&["0W", "1W", "4W", "9W", "16W", "25W", "36W", "49W", "64W", "81W"])),
    item(331, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Height", 2, Kind::Choice(&["3m", "6m", "12m", "24m", "49m", "98m", "195m", "390m", "780m", "1561m"])),
    item(332, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Gain", 2, Kind::Num { min: 0, max: 9, step: 1, unit: "dB", signed: false, div: 1 }),
    item(333, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Directivity", 2, Kind::Choice(&["Omni", "45ºNE", "90ºE", "135ºSE", "180ºS", "225ºSW", "270ºW", "315ºNW", "360ºN"])),
    item(334, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "SSID", 2, SSID),
    item(335, "GPS > GPS TX Mode > D-PRS > TX Format > Weather", "Symbol", 2, Kind::Text { max: 2 }),
    item(336, "GPS > GPS TX Mode > D-PRS > TX Format > Weather", "SSID", 2, SSID),
    item(337, "GPS > GPS TX Mode > D-PRS > TX Format > Weather", "Comment", 43, Kind::Text { max: 43 }),
    item(338, "GPS > GPS TX Mode > D-PRS > TX Format > Weather", "Time Stamp", 2, Kind::Choice(&["OFF", "DHM", "HMS"])),
    item(339, "GPS > GPS TX Mode > NMEA > GPS Sentence", "RMC", 2, OFF_ON),
    item(340, "GPS > GPS TX Mode > NMEA > GPS Sentence", "GGA", 2, OFF_ON),
    item(341, "GPS > GPS TX Mode > NMEA > GPS Sentence", "GLL", 2, OFF_ON),
    item(342, "GPS > GPS TX Mode > NMEA > GPS Sentence", "GSA", 2, OFF_ON),
    item(343, "GPS > GPS TX Mode > NMEA > GPS Sentence", "VTG", 2, OFF_ON),
    item(344, "GPS > GPS TX Mode > NMEA > GPS Sentence", "GSV", 2, OFF_ON),
    item(345, "GPS > GPS TX Mode > NMEA", "GPS Message", 20, Kind::Text { max: 20 }),
    item(347, "GPS > GPS Alarm", "Alarm Area (RX/ Memory)", 2, Kind::Choice(&["Limited", "Extended", "Both"])),
    item(348, "GPS > GPS Logger", "GPS Logger", 2, OFF_ON),
    item(349, "GPS > GPS Logger", "Record Interval", 2, Kind::Choice(&["1sec", "5sec", "10sec", "30sec", "1min", "5min", "10min"])),
    item(350, "GPS > GPS Logger > Record Sentence", "RMC", 2, OFF_ON),
    item(351, "GPS > GPS Logger > Record Sentence", "GGA", 2, OFF_ON),
    item(352, "GPS > GPS Logger > Record Sentence", "VTG", 2, OFF_ON),
    item(353, "GPS > GPS Logger > Record Sentence", "GSA", 2, OFF_ON),
    item(354, "GPS", "GPS Auto TX", 2, Kind::Choice(&["OFF", "30sec", "1min", "3min", "5min", "10min", "30min"])),
    item(355, "DTMF > DTMF SET", "DTMF Speed", 2, Kind::Choice(&["100ms", "200ms", "300ms", "500ms"])),
    item(356, "NB", "NB LEVEL", 4, PERCENT),
    item(357, "NB", "NB DEPTH", 2, NB_DEPTH),
    item(358, "NB", "NB WIDTH", 4, PERCENT),
    item(359, "VOX", "VOX DELAY", 2, VOX_DELAY),
    item(360, "VOX", "VOICE DELAY", 2, Kind::Choice(&["OFF", "SHORT", "MID", "LONG"])),
    item(361, "CD", "Call Sign Display/ Name Display", 2, Kind::Choice(&["Call Sign Display", "Name Display"])),
    item(362, "GPS Position", "Compass Direction", 2, Kind::Choice(&["Heading Up", "North Up", "South Up"])),
    item(363, "SET > Function", "Battery Pack Select", 2, Kind::Choice(&["BP-272", "BP-307"])),
    item(364, "SET > Function", "Battery Pack Confirmation", 2, OFF_ON),
    item(365, "SET > Function > Tuner", "Tuner Select", 2, Kind::Choice(&["AH-705", "Others"])),
    item(366, "SET > Function > Tuner > AH-705", "Tune Memory", 2, OFF_ON),
    item(367, "SET > Function > Front Key Customize", "[VOX/BK-IN]", 2, KEY_VOX_705),
    item(368, "SET > Function > Front Key Customize", "[AUTOTUNE/RX>CS]", 2, Kind::Choice(&["AUTOTUNE/RX>CS", "CD/RX>CS", "PRESET/RX>CS", "Home CH/RX>CS", "Temporary Skip/RX>CS"])),
    // Front-panel settings (their own CI-V commands, see EXTRA).
    item(9002, PANEL, "IF Filter Shape", 2, Kind::Choice(&["SHARP", "SOFT"])),
    item(9003, PANEL, "Manual Notch Width", 2, WIDTH3),
    item(9004, PANEL, "SSB TX Bandwidth", 2, WIDTH3),
    item(9005, PANEL, "Twin Peak Filter (RTTY)", 2, OFF_ON),
    item(9007, PANEL, "Tuning Step", 2, TUNING_STEP_705),
    item(9008, PANEL, "AGC Time Constant (0 = OFF)", 2, Kind::Num { min: 0, max: 13, step: 1, unit: "", signed: false, div: 1 }),
    item(9009, PANEL, "Manual Notch Position", 4, PERCENT),
    item(9010, PANEL, "Twin PBT Inner", 4, PERCENT),
    item(9011, PANEL, "Twin PBT Outer", 4, PERCENT),
    item(9012, PANEL, "BK-IN Delay (2.0-13.0 dots)", 4, PERCENT),
    item(9013, PANEL, "Anti VOX", 4, PERCENT),
    item(9014, PANEL, "LCD Backlight Brightness", 4, PERCENT),
    item(9016, PANEL, "Tone Squelch Function", 2, TONE_FUNCTION),
    item(9017, PANEL, "DSQL/CSQL (DV)", 2, Kind::Choice(&["OFF", "DSQL", "CSQL"])),
    item(9018, PANEL, "Repeater Tone", 2, OFF_ON),
    item(9019, PANEL, "Tone Squelch", 2, OFF_ON),
];

pub static MENU: [MenuItem; RAW.len()] = icom::numbered::<{ RAW.len() }>(RAW);

pub const EXTRA: &[(u32, &[u8])] = &[
    (9002, &[0x16, 0x56]),
    (9003, &[0x16, 0x57]),
    (9004, &[0x16, 0x58]),
    (9005, &[0x16, 0x4F]),
    (9007, &[0x10]),
    (9008, &[0x1A, 0x04]),
    (9009, &[0x14, 0x0D]),
    (9010, &[0x14, 0x07]),
    (9011, &[0x14, 0x08]),
    (9012, &[0x14, 0x0F]),
    (9013, &[0x14, 0x17]),
    (9014, &[0x14, 0x19]),
    (9016, &[0x16, 0x5D]),
    (9017, &[0x16, 0x5B]),
    (9018, &[0x16, 0x42]),
    (9019, &[0x16, 0x43]),
];

/// The IC-705's meters (command table, `15 xx`).
pub const COMP_CAL: [(u8, f32); 3] = [(0, 0.0), (130, 15.0), (210, 25.5)];
pub const VD_CAL: [(u8, f32); 3] = [(0, 0.0), (75, 5.0), (241, 16.0)];
pub const ID_CAL: [(u8, f32); 3] = [(0, 0.0), (121, 2.0), (241, 4.0)];

pub static SPEC: CivSpec = CivSpec {
    modes: MODES,
    att: &[(0, 0x00), (1, 0x20)],
    max_watts: 10.0,
    s_cal: &icom::S_CAL,
    po_cal: &icom::PO_CAL,
    swr_cal: &icom::SWR_CAL,
    alc_cal: &icom::ALC_CAL,
    comp_cal: &COMP_CAL,
    id_cal: &ID_CAL,
    vd_cal: &VD_CAL,
    transceive: Some(131),
    keyer_memories: 8,
    voice_memories: 8,
    antenna_switch: false,
    apf: false,
    apf_widths: false,
    apf_hz: 1270.0,
    lock: true,
    memory_name_len: 16,
    memory: MemLayout::Group,
    main_sub: false,
    antenna_rx: false,
    vox_delay: Some(359),
    keyer_type: Some(255),
    extra: EXTRA,
};

pub static IC705: RadioModel = RadioModel {
    key: "ic705",
    name: "IC-705",
    maker: Maker::Icom,
    family: Family::Icom,
    proto: Proto::CiV(&SPEC),
    id: "",
    civ_addr: 0xA4,
    hamlib: 3085,
    freq_min: 30_000,
    freq_max: 470_000_000,
    bands: BANDS,
    modes: MODE_LIST,
    menu: &MENU,
    roles: Roles {
        mic_sel: &[icom::num_of(&MENU, 118), icom::num_of(&MENU, 119)],
        mic_front: 0,
        mic_usb: 1,
        // USB (B) RTS.
        pc_keying: Some((icom::num_of(&MENU, 126), 0, 4)),
        cat_tot: None,
        cat_items: &[icom::num_of(&MENU, 131), icom::num_of(&MENU, 132), icom::num_of(&MENU, 133)],
        cw_bfo: Some((icom::num_of(&MENU, 70), ["01", "00", "--"])),
        cw_pitch_display: None,
        data_other: None,
        data_disp: None,
    },
    caps: Caps {
        commands: COMMANDS,
        widths: icom::widths,
        narrow: false,
        // On 2 m and 70 cm 01 is the one preamp.
        preamp: &[(0, "OFF"), (1, "P.AMP1"), (2, "P.AMP2")],
        att: &[(0, "OFF"), (1, "20 dB")],
        agc: &[(1, "FAST"), (2, "MID"), (3, "SLOW")],
        roofing: &[],
        roofing_answers: &[],
        nb: &[(0, "OFF"), (1, "NB")],
        nr_max: 15,
        nb_level_max: 100,
        antennas: 1,
        power: (1, 10),
        meter_select: &[],
        voice_memories: 8,
        keyer_memories: 8,
        keyer_memory_len: 70,
        memory: Memory { first: 0, last: 99, pms: None },
        shared_clarifier: true,
    },
    bauds: &[115200, 57600, 38400, 19200, 9600, 4800],
    stop_bits: 1,
    rts_cts: false,
    sim: &["SH028;", "GT02;", "PC010;"],
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialect::icom::num_of;
    use crate::dialect::icom::testing::*;

    #[test]
    fn ic705_literal_frames() {
        let m = &IC705;
        let mut r = radio(m);
        // 145.500 MHz: nine digits in RigHelm, five BCD bytes on CI-V.
        assert_eq!(down(&mut r, "FA145500000;"), ["FE FE A4 E0 25 00 00 00 50 45 01 FD"]);
        up(&mut r, "FE FE E0 A4 FB FD");
        assert_eq!(down(&mut r, "FA;"), ["FE FE A4 E0 25 00 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 A4 25 00 00 00 50 45 01 FD"), ["FA145500000;"]);
        // DV is 17, WFM 06.
        assert_eq!(down(&mut r, "MD0F;"), ["FE FE A4 E0 26 00 17 00 01 FD"]);
        up(&mut r, "FE FE E0 A4 FB FD");
        assert_eq!(down(&mut r, "MD0;"), ["FE FE A4 E0 26 00 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 A4 26 00 06 00 01 FD"), ["MD0G;"]);
        // 10 W: 5 W is half of 02 55.
        assert_eq!(down(&mut r, "PC005;"), ["FE FE A4 E0 14 0A 01 28 FD"]);
        up(&mut r, "FE FE E0 A4 FB FD");
        // Band stacking: 2 m is 13, 70 cm 14; DATA has its own byte.
        assert_eq!(down(&mut r, "BS13;"), ["FE FE A4 E0 1A 01 13 01 FD"]);
        let b = r.answer(&hex(
            "FE FE E0 A4 1A 01 13 01 00 00 52 44 01 01 01 01 00 00 00 08 85 00 08 85 00 00 23 00 00 00 00 \
             43 51 43 51 43 51 20 20 20 20 20 20 20 20 20 20 20 20 20 20 20 20 20 20 FD",
        ));
        assert_eq!(b.then.len(), 2);
        let tune: Vec<String> = b.then.iter().map(|o| match o { crate::dialect::Out::Radio { bytes, .. } => bytes.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" "), _ => String::new() }).collect();
        assert_eq!(tune, ["FE FE A4 E0 25 00 00 00 52 44 01 FD", "FE FE A4 E0 26 00 01 01 01 FD"]);
        let n = num_of(m.menu, 131);
        assert_eq!(down(&mut r, &format!("EX{n:03};")), ["FE FE A4 E0 1A 05 01 31 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 A4 1A 05 01 31 01 FD"), [format!("EX{n:03}01;")]);
        let refadj = num_of(m.menu, 89);
        assert_eq!(down(&mut r, &format!("EX{refadj:03}0511;")), ["FE FE A4 E0 1A 05 00 89 05 11 FD"]);
        up(&mut r, "FE FE E0 A4 FB FD");
        assert_eq!(down(&mut r, "VD0500;"), ["FE FE A4 E0 1A 05 03 59 05 FD"]);
        up(&mut r, "FE FE E0 A4 FB FD");
        // Memory 5 of group 0.
        assert_eq!(down(&mut r, "MC005;"), ["FE FE A4 E0 08 A0 00 00 FD", "FE FE A4 E0 08 00 05 FD"]);
        // Vd: 00 75 = 5 V; Id: 01 21 = 2 A.
        assert_eq!(down(&mut r, "RM8;"), ["FE FE A4 E0 15 15 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 A4 15 15 00 75 FD"), ["RM8085;"]);
    }

    #[test]
    fn ic705_round_trips() {
        let m = &IC705;
        for f in [
            "FA145500000;", "FB433500000;", "FA07074000;", "MD0F;", "MD0G;", "MD0C;", "MD0A;", "MD04;", "PC005;", "PC010;", "PA01;", "RA01;",
            "MW00514074000+000000C00000;", "MW09914574000+000000402000;", "MC005;",
        ] {
            if f.starts_with("MC") {
                continue;
            }
            round_trip_set(m, f);
        }
        menu_round_trips(m);
        for (read, answer) in [("FA;", "FA145500000;"), ("MD0;", "MD0F;"), ("PC;", "PC005;"), ("MR005;", "MR00514574000+000000411000;")] {
            round_trip_read(m, read, answer);
        }
    }
}
