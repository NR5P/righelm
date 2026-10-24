//! Icom IC-9700 (CI-V address A2h): 2 m, 70 cm and 23 cm with MAIN and SUB
//! bands, satellite mode and D-STAR (DV).
//!
//! Source: IC-9700 CI-V Reference Guide (command table pp. 3-12, command
//! formats pp. 13-28).
//!
//! RigHelm's VFO-A/B are the MAIN band's VFO A and B: `25`/`26` work on
//! the MAIN band only (selected/unselected VFO), `07 00`/`07 01` select
//! VFO A/B. `07 B0` exchanges MAIN and SUB, not the VFOs, so there is no
//! `SV`. Memories are per band (RigHelm channel = band x 100 + channel).

use super::ic7300::{
    AF_SQL, BAND_EDGE_BEEP, CLOCK, COUNT_UP, DATE, DOT_DASH, KEY_TYPE, MK2_USB_LINE as USB_LINE_AB, NB_DEPTH, NORMAL_REVERSE, NUMBER_STYLE,
    PANEL, VOX_DELAY, VOX_VOICE_DELAY, WIDTH3,
};
use super::ic705::TONE_FUNCTION;
use crate::dialect::icom::{self, AVERAGING, CivSpec, FILL, MemLayout, NOTCH_SW, OFF_ON, PERCENT, SPEED3, TBW, TONE, TOT, TX_DELAY, item};
use crate::menu::{Kind, MenuItem};
use crate::model::{Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::types::{Band, Mode};

/// Band stacking codes (`1A 01`).
pub const BANDS: &[Band] = &[
    Band { code: 1, label: "2m", lo: 144_000_000, hi: 148_000_000 },
    Band { code: 2, label: "70cm", lo: 430_000_000, hi: 450_000_000 },
    Band { code: 3, label: "23cm", lo: 1_240_000_000, hi: 1_300_000_000 },
];

/// Icom mode code and DATA for each RigHelm mode. DD (22, 1.2 GHz digital
/// data) has no RigHelm mode.
pub const MODES: &[(Mode, u8, bool)] = &[
    (Mode::Lsb, 0x00, false),
    (Mode::Usb, 0x01, false),
    (Mode::Cw, 0x03, false),
    (Mode::CwR, 0x07, false),
    (Mode::RttyLsb, 0x04, false),
    (Mode::RttyUsb, 0x08, false),
    (Mode::Am, 0x02, false),
    (Mode::Fm, 0x05, false),
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
    Mode::Dv,
    Mode::DataLsb,
    Mode::DataUsb,
    Mode::DataAm,
    Mode::DataFm,
];

/// RigHelm commands the IC-9700 carries out (no SV, no memory select).
pub const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "BA", "BC", "BD", "BI", "BP00", "BS", "BU", "BY", "CN", "CT", "EX", "FA", "FB", "FR", "FT", "GT", "ID", "IF", "KM", "KP", "KR",
    "KS", "KY", "LK", "MD", "MG", "ML0", "ML1", "MR", "MW", "MX", "NB", "NL", "NR", "OI", "OS", "PA", "PB", "PC", "PL", "PR0", "PS", "RA", "RC", "RD",
    "RG", "RL", "RM", "RT", "RU", "SH", "SM", "SQ", "TX", "VD", "VG", "VS", "VX", "XT",
];

const CSV_SEP: Kind = Kind::Choice(&["Separator , / Decimal .", "Separator ; / Decimal .", "Separator ; / Decimal ,"]);
const MOD_9700: Kind = Kind::Choice(&["MIC", "ACC", "MIC,ACC", "USB", "MIC,USB", "LAN"]);
const BEEP_SOUND: Kind = Kind::Num { min: 50, max: 200, step: 1, unit: "x10 Hz", signed: false, div: 1 };
const PORT: Kind = Kind::Num { min: 1, max: 65535, step: 1, unit: "", signed: false, div: 1 };
const GRID: Kind = Kind::Choice(&["Grid 1", "Grid 2", "Grid 3", "Grid 4", "Grid 5", "Grid 6", "Grid 7", "Grid 8"]);
const PRESENT_NUMBER: Kind = Kind::Num { min: 1, max: 9999, step: 1, unit: "", signed: false, div: 1 };
const PAUSE_TIMER: Kind = Kind::Choice(&["2 sec", "4 sec", "6 sec", "8 sec", "10 sec", "12 sec", "14 sec", "16 sec", "18 sec", "20 sec", "HOLD"]);
const RESUME_TIMER: Kind = Kind::Choice(&["0 sec", "1 sec", "2 sec", "3 sec", "4 sec", "5 sec", "HOLD"]);
const COURSE: Kind = Kind::Num { min: 0, max: 360, step: 1, unit: "°", signed: false, div: 1 };
const SPEED_KMH: Kind = Kind::Num { min: 0, max: 1850, step: 1, unit: "km/h", signed: false, div: 1 };
const HEIGHT: Kind = Kind::Choice(&[
    "3 m (10 ft)",
    "6 m (20 ft)",
    "12 m (40 ft)",
    "24 m (80 ft)",
    "49 m (160 ft)",
    "98 m (320 ft)",
    "195 m (640 ft)",
    "390 m (1280 ft)",
    "780 m (2560 ft)",
    "1561 m (5120 ft)",
]);
const SSID: Kind = Kind::Choice(&[
    "---", "(-0)", "-1", "-2", "-3", "-4", "-5", "-6", "-7", "-8", "-9", "-10", "-11", "-12", "-13", "-14", "-15", "-A", "-B", "-C", "-D", "-E",
    "-F", "-G", "-H", "-I", "-J", "-K", "-L", "-M", "-N", "-O", "-P", "-Q", "-R", "-S", "-T", "-U", "-V", "-W", "-X", "-Y", "-Z",
]);
const KEY_VOX_9700: Kind = Kind::Choice(&["VOX/BK-IN", "CD", "PRESET", "Home CH", "Temporary Skip", "Memory 1", "Memory 2", "Memory 3", "Memory 4"]);
const KEY_AFC_9700: Kind = Kind::Choice(&[
    "AUTOTUNE/AFC",
    "AUTOTUNE/AFC/RX>CS",
    "TONE/RX>CS",
    "CD",
    "CD/RX>CS",
    "PRESET",
    "PRESET/RX>CS",
    "Home CH",
    "Home CH/RX>CS",
    "Temporary Skip",
    "Temporary Skip/RX>CS",
    "Memory 1",
    "Memory 2",
    "Memory 3",
    "Memory 4",
]);
const MIC_KEY_9700: Kind = Kind::Choice(&[
    "No function",
    "UP",
    "DOWN",
    "UP (VFO: kHz)",
    "DOWN (VFO: kHz)",
    "XFC",
    "CALL",
    "VFO/MEMO",
    "DR",
    "FROM/TO (DR)",
    "Home CH",
    "BAND UP",
    "BAND DOWN",
    "SCAN",
    "Temporary Skip",
    "SPEECH",
    "MAIN/DUAL",
    "MODE",
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
]);
const TUNING_STEP_9700: Kind = Kind::Choice(&[
    "OFF (10 Hz or 1 Hz)",
    "100 Hz",
    "500 Hz",
    "1 kHz",
    "5 kHz",
    "6.25 kHz",
    "10 kHz",
    "12.5 kHz",
    "20 kHz",
    "25 kHz",
    "50 kHz",
    "100 kHz",
]);

const RAW: &[MenuItem] = &[
    // RX HPF/LPF (00 01, 00 04, 00 07, 00 10, 00 13, 00 14) are two numbers
    // in one; SPLIT Offset (00 44) a frequency; IP addresses (01 38, 01 39,
    // 01 41-01 43) four numbers; UTC Offset (01 84) an offset and a sign;
    // VBW (01 91) two values; the colours, the scope's fixed edges, the
    // manual positions (02 57, 02 83, 02 97) and the alarm area (03 17)
    // other multi-part values: all left out.
    item(2, "SET > Tone Control/TBW > RX", "SSB RX Tone (Bass)", 2, TONE),
    item(3, "SET > Tone Control/TBW > RX", "SSB RX Tone (Treble)", 2, TONE),
    item(5, "SET > Tone Control/TBW > RX", "AM RX Tone (Bass)", 2, TONE),
    item(6, "SET > Tone Control/TBW > RX", "AM RX Tone (Treble)", 2, TONE),
    item(8, "SET > Tone Control/TBW > RX", "FM RX Tone (Bass)", 2, TONE),
    item(9, "SET > Tone Control/TBW > RX", "FM RX Tone (Treble)", 2, TONE),
    item(11, "SET > Tone Control/TBW > RX", "DV RX Tone (Bass)", 2, TONE),
    item(12, "SET > Tone Control/TBW > RX", "DV RX Tone (Treble)", 2, TONE),
    item(15, "SET > Tone Control/TBW > TX", "SSB TX Tone (Bass)", 2, TONE),
    item(16, "SET > Tone Control/TBW > TX", "SSB TX Tone (Treble)", 2, TONE),
    item(17, "SET > Tone Control/TBW > TX", "SSB TBW (WIDE)", 2, TBW),
    item(18, "SET > Tone Control/TBW > TX", "SSB TBW (MID)", 2, TBW),
    item(19, "SET > Tone Control/TBW > TX", "SSB TBW (NAR)", 2, TBW),
    item(20, "SET > Tone Control/TBW > TX", "SSB-D TBW", 2, TBW),
    item(21, "SET > Tone Control/TBW > TX", "AM TX Tone (Bass)", 2, TONE),
    item(22, "SET > Tone Control/TBW > TX", "AM TX Tone (Treble)", 2, TONE),
    item(23, "SET > Tone Control/TBW > TX", "FM TX Tone (Bass)", 2, TONE),
    item(24, "SET > Tone Control/TBW > TX", "FM TX Tone (Treble)", 2, TONE),
    item(25, "SET > Tone Control/TBW > TX", "DV TX Tone (Bass)", 2, TONE),
    item(26, "SET > Tone Control/TBW > TX", "DV TX Tone (Treble)", 2, TONE),
    item(27, "SET > Function", "Beep Level", 4, PERCENT),
    item(28, "SET > Function", "Beep Level Limit", 2, OFF_ON),
    item(29, "SET > Function", "Beep (Confirmation)", 2, OFF_ON),
    item(30, "SET > Function", "Band Edge Beep", 2, BAND_EDGE_BEEP),
    item(31, "SET > Function", "Beep Sound (MAIN)", 4, BEEP_SOUND),
    item(32, "SET > Function", "Beep Sound (SUB)", 4, BEEP_SOUND),
    item(33, "SET > Function > Sub Band Mute (TX)", "Speaker/Phones", 2, OFF_ON),
    item(34, "SET > Function > Sub Band Mute (TX)", "USB", 2, OFF_ON),
    item(35, "SET > Function > Sub Band Mute (TX)", "LAN", 2, OFF_ON),
    item(36, "SET > Function", "RF/SQL Control", 2, Kind::Choice(&["Auto", "SQL", "RF+SQL"])),
    item(37, "SET > Function", "FM/DV Center Error function", 2, OFF_ON),
    item(38, "SET > Function", "TX Delay (144M)", 2, TX_DELAY),
    item(39, "SET > Function", "TX Delay (430M)", 2, TX_DELAY),
    item(40, "SET > Function", "TX Delay (1200M)", 2, TX_DELAY),
    item(41, "SET > Function", "Time-Out Timer", 2, TOT),
    item(42, "SET > Function", "PTT Lock", 2, OFF_ON),
    item(43, "SET > Function > SPLIT", "Quick SPLIT", 2, OFF_ON),
    item(45, "SET > Function > SPLIT", "SPLIT LOCK", 2, OFF_ON),
    item(46, "SET > Function", "Auto Repeater", 2, Kind::Choice(&["OFF", "ON (DUP,TONE)"])),
    item(47, "SET > Function", "RTTY Mark Frequency", 2, Kind::Choice(&["1275 Hz", "1615 Hz", "2125 Hz"])),
    item(48, "SET > Function", "RTTY Shift Width", 2, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz"])),
    item(49, "SET > Function", "RTTY Keying Polarity", 2, NORMAL_REVERSE),
    item(50, "SET > Function > SPEECH", "SPEECH Language", 2, Kind::Choice(&["English", "Japanese"])),
    item(51, "SET > Function > SPEECH", "Alphabet", 2, Kind::Choice(&["Normal", "Phonetic Code"])),
    item(52, "SET > Function > SPEECH", "SPEECH Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(53, "SET > Function > SPEECH", "RX Call Sign SPEECH", 2, Kind::Choice(&["OFF", "ON (Kerchunk)", "ON (All)"])),
    item(54, "SET > Function > SPEECH", "RX>CS SPEECH", 2, OFF_ON),
    item(55, "SET > Function > SPEECH", "S-Level SPEECH", 2, OFF_ON),
    item(56, "SET > Function > SPEECH", "MODE SPEECH", 2, OFF_ON),
    item(57, "SET > Function > SPEECH", "SPEECH Level", 4, PERCENT),
    item(58, "SET > Function", "[SPEECH/LOCK] Switch", 2, Kind::Choice(&["SPEECH/LOCK", "LOCK/SPEECH"])),
    item(59, "SET > Function", "Lock Function", 2, Kind::Choice(&["MAIN DIAL", "PANEL"])),
    item(60, "SET > Function", "Memo Pad Quantity", 2, Kind::Choice(&["5 ch", "10 ch"])),
    item(61, "SET > Function", "MAIN DIALAuto TS", 2, Kind::Choice(&["OFF", "Low", "High"])),
    item(62, "SET > Function", "MIC Up/Down Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(63, "SET > Function", "AFC Limit", 2, OFF_ON),
    item(64, "SET > Function", "[NOTCH] Switch (SSB)", 2, NOTCH_SW),
    item(65, "SET > Function", "[NOTCH] Switch (AM)", 2, NOTCH_SW),
    item(66, "SET > Function", "SSB/CW Synchronous Tuning", 2, OFF_ON),
    item(67, "SET > Function", "CW Normal Side", 2, Kind::Choice(&["LSB", "USB"])),
    item(68, "SET > Function", "Screen Keyboard Type", 2, Kind::Choice(&["Ten-key", "Full Keyboard"])),
    item(69, "SET > Function", "Screen Full Keyboard Layout", 2, Kind::Choice(&["English", "German", "French"])),
    item(70, "SET > Function", "Screen Capture [POWER] Switch", 2, OFF_ON),
    item(71, "SET > Function", "Screen Capture File Type", 2, Kind::Choice(&["PNG", "BMP"])),
    item(72, "SET > Function", "REF Adjust", 4, PERCENT),
    item(73, "SET > Function", "REF Adjust (FINE)", 4, PERCENT),
    item(74, "SET > DV/DD Set", "Standby Beep", 2, Kind::Choice(&["OFF", "ON", "ON (to me: High Tone)", "ON (to me: Alarm/High Tone)"])),
    item(75, "SET > DV/DD Set", "Auto Reply", 2, Kind::Choice(&["OFF", "ON", "Voice"])),
    item(76, "SET > DV/DD Set", "DV Data TX", 2, Kind::Choice(&["PTT", "Auto"])),
    item(77, "SET > DV/DD Set > DV Fast Data", "Fast Data", 2, OFF_ON),
    item(78, "SET > DV/DD Set > DV Fast Data", "GPS Data Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(79, "SET > DV/DD Set > DV Fast Data", "TX Delay (PTT)", 2, Kind::Steps { max_code: 10, off_at_zero: true, first: 1, step: 1, unit: "s" }),
    item(80, "SET > DV/DD Set", "Digital Monitor", 2, Kind::Choice(&["Auto", "Digital", "Analog"])),
    item(81, "SET > DV/DD Set", "Digital Repeater Set", 2, OFF_ON),
    item(82, "SET > DV/DD Set", "DV Auto Detect", 2, OFF_ON),
    item(83, "SET > DV/DD Set", "RX Record (RPT)", 2, Kind::Choice(&["ALL", "Latest Only"])),
    item(84, "SET > DV/DD Set", "BK", 2, OFF_ON),
    item(85, "SET > DV/DD Set", "EMR", 2, OFF_ON),
    item(86, "SET > DV/DD Set", "EMR AF Level", 4, PERCENT),
    item(87, "SET > DV/DD Set", "DD TX Inhibit (Power ON)", 2, OFF_ON),
    item(88, "SET > DV/DD Set", "DD Packet Output", 2, Kind::Choice(&["Normal", "All"])),
    item(89, "SET > QSO/RX Log", "QSO Log", 2, OFF_ON),
    item(90, "SET > QSO/RX Log", "RX History Log", 2, OFF_ON),
    item(91, "SET > QSO/RX Log > CSV Format", "Separator/Decimal", 2, CSV_SEP),
    item(92, "SET > QSO/RX Log > CSV Format", "Date", 2, Kind::Choice(&["yyyy/mm/dd", "mm/dd/yyyy", "dd/mm/yyyy"])),
    item(93, "SET > Connectors > External P.AMP", "144M", 2, OFF_ON),
    item(94, "SET > Connectors > External P.AMP", "430M", 2, OFF_ON),
    item(95, "SET > Connectors > External P.AMP", "1200M", 2, OFF_ON),
    item(96, "SET > Connectors", "External Speaker Separate", 2, Kind::Choice(&["Separate", "Mix"])),
    item(97, "SET > Connectors > Phones", "Level", 2, Kind::Steps { max_code: 30, off_at_zero: false, first: -15, step: 1, unit: "dB" }),
    item(98, "SET > Connectors > Phones", "L/R Mix", 2, Kind::Choice(&["Separate", "Mix", "Auto"])),
    item(99, "SET > Connectors > ACC AF/IF Output", "AF/SQL Output Select", 2, Kind::Choice(&["MAIN", "SUB"])),
    item(100, "SET > Connectors > ACC AF/IF Output", "Output Select", 2, Kind::Choice(&["AF", "IF"])),
    item(101, "SET > Connectors > ACC AF/IF Output", "AF Output Level", 4, PERCENT),
    item(102, "SET > Connectors > ACC AF/IF Output", "AF SQL", 2, AF_SQL),
    item(103, "SET > Connectors > ACC AF/IF Output", "AF Beep/Speech... Output", 2, OFF_ON),
    item(104, "SET > Connectors > ACC AF/IF Output", "ACC IF Output Level", 4, PERCENT),
    item(105, "SET > Connectors > USB AF/IF Output", "Output Select", 2, Kind::Choice(&["AF", "IF"])),
    item(106, "SET > Connectors > USB AF/IF Output", "AF Output Level", 4, PERCENT),
    item(107, "SET > Connectors > USB AF/IF Output", "AF SQL", 2, AF_SQL),
    item(108, "SET > Connectors > USB AF/IF Output", "AF Beep/Speech... Output", 2, OFF_ON),
    item(109, "SET > Connectors > USB AF/IF Output", "IF Output Level", 4, PERCENT),
    item(110, "SET > Connectors > LAN AF/IF Output", "Output Select", 2, Kind::Choice(&["AF", "IF"])),
    item(111, "SET > Connectors > LAN AF/IF Output", "AF SQL", 2, AF_SQL),
    item(112, "SET > Connectors > MOD Input", "ACC MOD Level", 4, PERCENT),
    item(113, "SET > Connectors > MOD Input", "USB MOD Level", 4, PERCENT),
    item(114, "SET > Connectors > MOD Input", "LAN MOD Level", 4, PERCENT),
    item(115, "SET > Connectors > MOD Input", "DATA OFF MOD", 2, MOD_9700),
    item(116, "SET > Connectors > MOD Input", "DATA MOD", 2, MOD_9700),
    item(117, "SET > Connectors > ACC SEND Output", "144M", 2, OFF_ON),
    item(118, "SET > Connectors > ACC SEND Output", "430M", 2, OFF_ON),
    item(119, "SET > Connectors > ACC SEND Output", "1200M", 2, OFF_ON),
    item(120, "SET > Connectors > USB SEND/Keying", "USB SEND", 2, USB_LINE_AB),
    item(121, "SET > Connectors > USB SEND/Keying", "USB Keying (CW)", 2, USB_LINE_AB),
    item(122, "SET > Connectors > USB SEND/Keying", "USB Keying (RTTY)", 2, USB_LINE_AB),
    item(123, "SET > Connectors > USB SEND/Keying", "Inhibit Timer at USB connection", 2, OFF_ON),
    item(124, "SET > Connectors > External Keypad", "VOICE", 2, OFF_ON),
    item(125, "SET > Connectors > External Keypad", "KEYER", 2, OFF_ON),
    item(126, "SET > Connectors > External Keypad", "RTTY", 2, OFF_ON),
    item(127, "SET > Connectors > CI-V", "CI-V Transceive", 2, OFF_ON),
    item(128, "SET > Connectors > CI-V", "CI-V USB/LAN→REMOTE Transceive Address", 4, Kind::Num { min: 0, max: 223, step: 1, unit: "", signed: false, div: 1 }),
    item(129, "SET > Connectors > CI-V", "CI-V USB Port", 2, Kind::Choice(&["Link to [REMOTE]", "Unlink from [REMOTE]"])),
    item(130, "SET > Connectors > CI-V", "CI-V USB Echo Back", 2, OFF_ON),
    item(131, "SET > Connectors > CI-V", "CI-V DATA Echo Back", 2, OFF_ON),
    item(132, "SET > Connectors > CI-V > USB (B)/DATA Function", "USB (B) Function", 2, Kind::Choice(&["OFF", "RTTY Decode", "DV Data"])),
    item(133, "SET > Connectors > CI-V > USB (B)/DATA Function", "DATA Function", 2, Kind::Choice(&["OFF", "RTTY Decode", "DV Data", "GPS/Weather", "CI-V"])),
    item(134, "SET > Connectors > CI-V > USB (B)/DATA Function", "GPS Out", 2, Kind::Choice(&["OFF", "DATA→USB (B)"])),
    item(135, "SET > Connectors > CI-V > USB (B)/DATA Function", "DV Data/GPS Out Baud Rate", 2, Kind::Choice(&["4800bps", "9600bps"])),
    item(136, "SET > Connectors > CI-V > USB (B)/DATA Function", "RTTY Decode Baud Rate", 2, Kind::Choice(&["4800bps", "9600bps", "19200bps", "38400bps"])),
    item(137, "SET > Network", "DHCP (Valid after Restart)", 2, OFF_ON),
    item(140, "SET > Network", "Subnet Mask (bits, valid after restart)", 2, Kind::Num { min: 1, max: 30, step: 1, unit: "", signed: false, div: 1 }),
    item(144, "SET > Network", "Network Name", 15, Kind::Text { max: 15 }),
    item(145, "SET > Network", "Network Control (Valid after Restart)", 2, OFF_ON),
    item(146, "SET > Network", "Power OFF Setting (for Remote Control)", 2, Kind::Choice(&["Shutdown only", "Standby/Shutdown"])),
    item(147, "SET > Network", "Control Port (UDP)", 6, PORT),
    item(148, "SET > Network", "Serial Port (UDP)", 6, PORT),
    item(149, "SET > Network", "Audio Port (UDP)", 6, PORT),
    item(150, "SET > Network", "Internet Access Line", 2, Kind::Choice(&["FTTH", "ADSL/CATV"])),
    item(151, "SET > Network", "Network Radio Name", 16, Kind::Text { max: 16 }),
    item(152, "SET > Display", "LCD Backlight", 4, PERCENT),
    item(153, "SET > Display", "Display Type", 2, Kind::Choice(&["A", "B"])),
    item(154, "SET > Display", "Display Font", 2, Kind::Choice(&["Basic", "Round"])),
    item(155, "SET > Display", "Meter Peak Hold (Bar)", 2, OFF_ON),
    item(156, "SET > Display", "Memory Name", 2, OFF_ON),
    item(157, "SET > Display", "MN-Q Popup (MN OFF→ON)", 2, OFF_ON),
    item(158, "SET > Display", "BW Popup (PBT)", 2, OFF_ON),
    item(159, "SET > Display", "BW Popup (FIL)", 2, OFF_ON),
    item(160, "SET > Display", "RX Call Sign Display", 2, Kind::Choice(&["OFF", "Normal", "RX Hold", "Hold"])),
    item(161, "SET > Display", "RX Position Indicator", 2, OFF_ON),
    item(162, "SET > Display", "RX Position Display", 2, Kind::Choice(&["OFF", "ON (Main/Sub)", "ON (Main Only)"])),
    item(163, "SET > Display", "RX Position Display Timer", 2, Kind::Choice(&["5 sec", "10 sec", "15 sec", "30 sec", "Hold"])),
    item(164, "SET > Display", "Reply Position Display", 2, OFF_ON),
    item(165, "SET > Display", "TX Call Sign Display", 2, Kind::Choice(&["OFF", "Your Call Sign", "My Call Sign"])),
    item(166, "SET > Display", "Scroll Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(167, "SET > Display", "Screen Saver", 2, Kind::Choice(&["OFF", "15 min.", "30 min.", "60 min."])),
    item(168, "SET > Display", "Opening Message", 2, OFF_ON),
    item(169, "SET > Display", "Power ON Check", 2, OFF_ON),
    item(170, "SET > Display > Display Unit", "Latitude/Longitude", 2, Kind::Choice(&["ddd°mm.mm'", "ddd°mm'ss", "ddd.dddd°"])),
    item(171, "SET > Display > Display Unit", "Altitude/Distance", 2, Kind::Choice(&["m", "ft/mi"])),
    item(172, "SET > Display > Display Unit", "Speed", 2, Kind::Choice(&["km/h", "mph", "knots"])),
    item(173, "SET > Display > Display Unit", "Temperature", 2, Kind::Choice(&["ºC", "ºF"])),
    item(174, "SET > Display > Display Unit", "Barometric", 2, Kind::Choice(&["hPa", "mb", "mmHg", "inHg"])),
    item(175, "SET > Display > Display Unit", "Rainfall", 2, Kind::Choice(&["mm", "inch"])),
    item(176, "SET > Display > Display Unit", "Wind Speed", 2, Kind::Choice(&["m/s", "km/h", "mph", "knots"])),
    item(177, "SET > Display", "Display Language", 2, Kind::Choice(&["English", "Japanese"])),
    item(178, "SET > Display", "System Language", 2, Kind::Choice(&["English", "Japanese"])),
    item(179, "SET > Time Set > Date/Time", "Date (YYYYMMDD)", 8, DATE),
    item(180, "SET > Time Set > Date/Time", "Time (HHMM)", 4, CLOCK),
    item(181, "SET > Time Set > Date/Time", "NTP Function", 2, OFF_ON),
    item(182, "SET > Time Set > Date/Time", "NTP Server Address", 64, Kind::Text { max: 64 }),
    item(183, "SET > Time Set > Date/Time", "GPS Time Correct", 2, Kind::Choice(&["OFF", "Auto"])),
    item(185, "SET > SD Card > Import/Export > CSV Format", "Separator/Decimal", 2, CSV_SEP),
    item(186, "SET > SD Card > Import/Export > CSV Format", "Date", 2, Kind::Choice(&["yyyy/mm/dd", "mm/dd/yyyy", "dd/mm/yyyy"])),
    item(187, "SCOPE", "Scope during Tx (CENTER TYPE)", 2, OFF_ON),
    item(188, "SCOPE", "Max Hold", 2, Kind::Choice(&["OFF", "10s Hold", "ON"])),
    item(189, "SCOPE", "CENTER Type Display", 2, Kind::Choice(&["Filter center", "Carrier point center", "Carrier point center (Abs. Freq.)"])),
    item(190, "SCOPE", "Marker Position (Fix Type/SCROLL Type)", 2, Kind::Choice(&["Filter center", "Carrier point"])),
    item(192, "SCOPE", "Averaging", 2, AVERAGING),
    item(193, "SCOPE", "Waveform Type", 2, FILL),
    item(197, "SCOPE", "Waterfall Display", 2, OFF_ON),
    item(198, "SCOPE", "Waterfall Speed", 2, SPEED3),
    item(199, "SCOPE", "Waterfall Size (Expand Screen)", 2, Kind::Choice(&["Small", "Mid", "Large"])),
    item(200, "SCOPE", "Waterfall Peak Color Level", 2, GRID),
    item(201, "SCOPE", "Waterfall Marker Auto-hide", 2, OFF_ON),
    item(211, "AUDIO SCOPE SET", "FFT Scope Waveform Type", 2, Kind::Choice(&["Line", "Fill"])),
    item(213, "AUDIO SCOPE SET", "FFT Scope Waterfall Display", 2, OFF_ON),
    item(215, "VOICE TX", "TX LEVEL", 4, PERCENT),
    item(216, "VOICE TX > VOICE TX SET", "Auto Monitor", 2, OFF_ON),
    item(217, "VOICE TX > VOICE TX SET", "Repeat Time", 2, Kind::Num { min: 1, max: 15, step: 1, unit: "s", signed: false, div: 1 }),
    item(218, "KEYER > KEYER 001", "Number Style", 2, NUMBER_STYLE),
    item(219, "KEYER > KEYER 001", "Count Up Trigger", 2, COUNT_UP),
    item(220, "KEYER > KEYER 001", "Present Number", 4, PRESENT_NUMBER),
    item(221, "KEYER > CW-KEY SET", "Side Tone Level", 4, PERCENT),
    item(222, "KEYER > CW-KEY SET", "Side Tone Level Limit", 2, OFF_ON),
    item(223, "KEYER > CW-KEY SET", "Keyer Repeat Time", 2, Kind::Num { min: 1, max: 60, step: 1, unit: "s", signed: false, div: 1 }),
    item(224, "KEYER > CW-KEY SET", "Dot/Dash Ratio (1:1:x)", 2, DOT_DASH),
    item(225, "KEYER > CW-KEY SET", "Rise Time", 2, Kind::Choice(&["2 msec.", "4 msec.", "6 msec.", "8 msec."])),
    item(226, "KEYER > CW-KEY SET", "Paddle Polarity", 2, NORMAL_REVERSE),
    item(227, "KEYER > CW-KEY SET", "Key Type", 2, KEY_TYPE),
    item(228, "KEYER > CW-KEY SET", "MIC Up/Down Keyer", 2, OFF_ON),
    item(229, "RTTY DECODE SET", "FFT Scope Averaging", 2, AVERAGING),
    item(231, "RTTY DECODE SET", "Decode USOS", 2, OFF_ON),
    item(232, "RTTY DECODE SET", "Decode New Line Code", 2, Kind::Choice(&["CR,LF,CR+LF", "CR+LF"])),
    item(233, "RTTY DECODE SET", "TX USOS", 2, OFF_ON),
    item(234, "RTTY DECODE SET", "Displayed Characters during Tx (Satellite)", 2, Kind::Choice(&["Displayed Characters during RX", "Displayed Characters during TX"])),
    item(237, "RTTY DECODE LOG", "Decode Log", 2, OFF_ON),
    item(238, "RTTY DECODE LOG > Log Set", "File Type", 2, Kind::Choice(&["Text", "HTML"])),
    item(239, "RTTY DECODE LOG > Log Set", "Time Stamp", 2, OFF_ON),
    item(240, "RTTY DECODE LOG > Log Set", "Time Stamp (Time)", 2, Kind::Choice(&["Local", "UTC"])),
    item(241, "RTTY DECODE LOG > Log Set", "Time Stamp (Frequency)", 2, OFF_ON),
    item(242, "QSO RECORDER > Recorder Set", "TX REC Audio", 2, Kind::Choice(&["Direct", "Monitor"])),
    item(243, "QSO RECORDER > Recorder Set", "RX REC Condition", 2, Kind::Choice(&["Always", "Squelch Auto"])),
    item(244, "QSO RECORDER > Recorder Set", "File Split", 2, OFF_ON),
    item(245, "QSO RECORDER > Recorder Set", "REC Operation", 2, Kind::Choice(&["MAIN/SUB Separate", "MAIN/SUB Link"])),
    item(246, "QSO RECORDER > Recorder Set", "PTT Auto REC", 2, OFF_ON),
    item(247, "QSO RECORDER > Recorder Set", "PRE-REC for PTT Auto REC", 2, Kind::Choice(&["OFF", "5 sec.", "10 sec.", "15 sec."])),
    item(248, "QSO RECORDER > Player Set", "Skip Time", 2, Kind::Choice(&["3 sec.", "5 sec.", "10 sec.", "30 sec."])),
    item(249, "SCAN SET", "SCAN Speed", 2, Kind::Choice(&["Slow", "Fast"])),
    item(250, "SCAN SET", "SCAN Resume", 2, OFF_ON),
    item(251, "SCAN SET", "Pause Timer", 2, PAUSE_TIMER),
    item(252, "SCAN SET", "Resume Timer", 2, RESUME_TIMER),
    item(253, "SCAN SET", "Temporary Skip Timer", 2, Kind::Choice(&["5 min.", "10 min.", "15 min.", "While Scanning", "While Powered ON"])),
    item(254, "SCAN SET", "MAIN DIAL Operation (SCAN)", 2, Kind::Choice(&["OFF", "Up/Down"])),
    item(255, "GPS > GPS Set", "GPS Select", 2, Kind::Choice(&["OFF", "External GPS", "Manual"])),
    item(256, "GPS > GPS Set", "GPS Receiver Baud Rate", 2, Kind::Choice(&["4800bps", "9600bps"])),
    item(258, "GPS", "GPS TX Mode", 2, Kind::Choice(&["OFF", "D-PRS", "NMEA"])),
    item(259, "GPS > GPS TX Mode > D-PRS", "Unproto Address", 56, Kind::Text { max: 56 }),
    item(260, "GPS > GPS TX Mode > D-PRS", "TX Format", 2, Kind::Choice(&["Position", "Object", "Item", "Weather"])),
    item(261, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Symbol", 2, Kind::Choice(&["No.1", "No.2", "No.3", "No.4"])),
    item(262, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Symbol No.1", 2, Kind::Text { max: 2 }),
    item(263, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Symbol No.2", 2, Kind::Text { max: 2 }),
    item(264, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Symbol No.3", 2, Kind::Text { max: 2 }),
    item(265, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Symbol No.4", 2, Kind::Text { max: 2 }),
    item(266, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "SSID", 2, SSID),
    item(267, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Comment", 2, Kind::Choice(&["No.1", "No.2", "No.3", "No.4"])),
    item(268, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Comment 1", 43, Kind::Text { max: 43 }),
    item(269, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Comment 2", 43, Kind::Text { max: 43 }),
    item(270, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Comment 3", 43, Kind::Text { max: 43 }),
    item(271, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Comment 4", 43, Kind::Text { max: 43 }),
    item(272, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Time Stamp", 2, Kind::Choice(&["OFF", "DHM", "HMS"])),
    item(273, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Altitude", 2, OFF_ON),
    item(274, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Data Extension", 2, Kind::Choice(&["OFF", "Course/Speed", "Power/Height/Gain/Directivity"])),
    item(275, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Power", 2, Kind::Choice(&["0W", "1W", "4W", "9W", "16W", "25W", "36W", "49W", "64W", "81W"])),
    item(276, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Height", 2, HEIGHT),
    item(277, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Gain", 2, Kind::Num { min: 0, max: 9, step: 1, unit: "dB", signed: false, div: 1 }),
    item(278, "GPS > GPS TX Mode > D-PRS > TX Format > Position", "Directivity", 2, Kind::Choice(&["Omni", "45ºNE", "90ºE", "135ºSE", "180ºS", "225ºSW", "270ºW", "315ºNW", "360ºN"])),
    item(279, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Object Name", 9, Kind::Text { max: 9 }),
    item(280, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Data Type", 2, Kind::Choice(&["Live Object", "Kill Object"])),
    item(281, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Symbol", 2, Kind::Text { max: 2 }),
    item(282, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Comment", 43, Kind::Text { max: 43 }),
    item(284, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Data Extension", 2, Kind::Choice(&["OFF", "Course/Speed", "Power/Height/Gain/Directivity"])),
    item(285, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Course", 4, COURSE),
    item(286, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Speed (km/h)", 4, SPEED_KMH),
    item(287, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Power", 2, Kind::Choice(&["0W", "1W", "4W", "9W", "16W", "25W", "36W", "49W", "64W", "81W"])),
    item(288, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Height", 2, HEIGHT),
    item(289, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Gain", 2, Kind::Num { min: 0, max: 9, step: 1, unit: "dB", signed: false, div: 1 }),
    item(290, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Directivity", 2, Kind::Choice(&["Omni", "45ºNE", "90ºE", "135ºSE", "180ºS", "225ºSW", "270ºW", "315ºNW", "360ºN"])),
    item(291, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "SSID", 2, SSID),
    item(292, "GPS > GPS TX Mode > D-PRS > TX Format > Object", "Time Stamp", 2, Kind::Choice(&["DHM", "HMS"])),
    item(293, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Item Name", 9, Kind::Text { max: 9 }),
    item(294, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Data Type", 2, Kind::Choice(&["Live Item", "Killed Item"])),
    item(295, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Symbol", 2, Kind::Text { max: 2 }),
    item(296, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Comment", 43, Kind::Text { max: 43 }),
    item(298, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Data Extension", 2, Kind::Choice(&["OFF", "Course/Speed", "Power/Height/Gain/Directivity"])),
    item(299, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Course", 4, COURSE),
    item(300, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Speed (km/h)", 4, SPEED_KMH),
    item(301, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Power", 2, Kind::Choice(&["0W", "1W", "4W", "9W", "16W", "25W", "36W", "49W", "64W", "81W"])),
    item(302, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Height", 2, HEIGHT),
    item(303, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Gain", 2, Kind::Num { min: 0, max: 9, step: 1, unit: "dB", signed: false, div: 1 }),
    item(304, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "Directivity", 2, Kind::Choice(&["Omni", "45ºNE", "90ºE", "135ºSE", "180ºS", "225ºSW", "270ºW", "315ºNW", "360ºN"])),
    item(305, "GPS > GPS TX Mode > D-PRS > TX Format > Item", "SSID", 2, SSID),
    item(306, "GPS > GPS TX Mode > D-PRS > TX Format > Weather", "Symbol", 2, Kind::Text { max: 2 }),
    item(307, "GPS > GPS TX Mode > D-PRS > TX Format > Weather", "SSID", 2, SSID),
    item(308, "GPS > GPS TX Mode > D-PRS > TX Format > Weather", "Comment", 43, Kind::Text { max: 43 }),
    item(309, "GPS > GPS TX Mode > D-PRS > TX Format > Weather", "Time Stamp", 2, Kind::Choice(&["OFF", "DHM", "HMS"])),
    item(310, "GPS > GPS TX Mode > NMEA > GPS Sentence", "RMC", 2, OFF_ON),
    item(311, "GPS > GPS TX Mode > NMEA > GPS Sentence", "GGA", 2, OFF_ON),
    item(312, "GPS > GPS TX Mode > NMEA > GPS Sentence", "GLL", 2, OFF_ON),
    item(313, "GPS > GPS TX Mode > NMEA > GPS Sentence", "GSA", 2, OFF_ON),
    item(314, "GPS > GPS TX Mode > NMEA > GPS Sentence", "VTG", 2, OFF_ON),
    item(315, "GPS > GPS TX Mode > NMEA > GPS Sentence", "GSV", 2, OFF_ON),
    item(316, "GPS > GPS TX Mode > NMEA", "GPS Message", 20, Kind::Text { max: 20 }),
    item(318, "GPS > GPS Alarm", "Alarm Area (RX/Memory)", 2, Kind::Choice(&["Limited", "Extended", "Both"])),
    item(319, "GPS", "GPS Auto TX", 2, Kind::Choice(&["OFF", "5 sec", "10 sec", "30 sec", "1 min", "3 min", "5 min", "10 min", "30 min"])),
    item(320, "DTMF SET", "DTMF Speed", 2, Kind::Choice(&["100ms", "200ms", "300ms", "500 ms"])),
    item(321, "NB", "NB LEVEL (144 MHz)", 4, PERCENT),
    item(322, "NB", "NB DEPTH (144 MHz)", 2, NB_DEPTH),
    item(323, "NB", "NB WIDTH (144 MHz)", 4, PERCENT),
    item(324, "NB", "NB LEVEL (430 MHz)", 4, PERCENT),
    item(325, "NB", "NB DEPTH (430 MHz)", 2, NB_DEPTH),
    item(326, "NB", "NB WIDTH (430 MHz)", 4, PERCENT),
    item(327, "NB", "NB LEVEL (1200 MHz)", 4, PERCENT),
    item(328, "NB", "NB DEPTH (1200 MHz)", 2, NB_DEPTH),
    item(329, "NB", "NB WIDTH (1200 MHz)", 4, PERCENT),
    item(330, "VOX", "VOX Delay", 2, VOX_DELAY),
    item(331, "VOX", "VOX Voice Delay", 2, VOX_VOICE_DELAY),
    item(332, "TX PWR LIMIT", "TX PWR LIMIT (144M)", 2, OFF_ON),
    item(333, "TX PWR LIMIT", "TX PWR LIMIT Level (144M)", 4, PERCENT),
    item(334, "TX PWR LIMIT", "TX PWR LIMIT (430M)", 2, OFF_ON),
    item(335, "TX PWR LIMIT", "TX PWR LIMIT Level (430M)", 4, PERCENT),
    item(336, "TX PWR LIMIT", "TX PWR LIMIT (1200M)", 2, OFF_ON),
    item(337, "TX PWR LIMIT", "TX PWR LIMIT Level (1200M)", 4, PERCENT),
    item(338, "CD", "Received Call Sign Display", 2, Kind::Choice(&["Call Sign", "Name"])),
    item(339, "GPS Position", "Compass Direction", 2, Kind::Choice(&["Heading Up", "North Up", "South Up"])),
    item(340, "SET > Function", "Home CH Beep", 2, OFF_ON),
    item(341, "SET > Connectors", "PTT Port Function", 2, Kind::Choice(&["PTT Input", "PTT Input + SEND Output"])),
    item(342, "SET > Display", "RX Picture Indicator", 2, OFF_ON),
    item(343, "SET > Function > Front Key Customize", "[VOX/BK-IN]", 2, KEY_VOX_9700),
    item(344, "SET > Function > Front Key Customize", "[AUTOTUNE/AFC]", 2, KEY_AFC_9700),
    item(345, "SET > Function > Front Key Customize", "[TONE/RX>CS]", 2, Kind::Choice(&["AUTOTUNE/AFC", "TONE/RX>CS", "CD/RX>CS", "PRESET/RX>CS", "Home CH/RX>CS", "Temporary Skip/RX>CS"])),
    item(346, "SET > Function > MIC Key Customize", "[UP]", 2, MIC_KEY_9700),
    item(347, "SET > Function > MIC Key Customize", "[DN]", 2, MIC_KEY_9700),
    // Front-panel settings (their own CI-V commands, see EXTRA).
    item(9001, PANEL, "IP+", 2, OFF_ON),
    item(9002, PANEL, "IF Filter Shape", 2, Kind::Choice(&["SHARP", "SOFT"])),
    item(9003, PANEL, "Manual Notch Width", 2, WIDTH3),
    item(9004, PANEL, "SSB TX Bandwidth", 2, WIDTH3),
    item(9005, PANEL, "Twin Peak Filter (RTTY)", 2, OFF_ON),
    item(9007, PANEL, "Tuning Step", 2, TUNING_STEP_9700),
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
    item(9027, PANEL, "AFC", 2, OFF_ON),
    item(9028, PANEL, "Dualwatch (SUB band)", 2, OFF_ON),
    item(9029, PANEL, "Satellite Mode", 2, OFF_ON),
];

pub static MENU: [MenuItem; RAW.len()] = icom::numbered::<{ RAW.len() }>(RAW);

pub const EXTRA: &[(u32, &[u8])] = &[
    (9001, &[0x16, 0x65]),
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
    (9027, &[0x16, 0x4A]),
    (9028, &[0x16, 0x59]),
    (9029, &[0x16, 0x5A]),
];

/// The IC-9700's meters (command table, `15 xx`).
pub const ID_CAL: [(u8, f32); 3] = [(0, 0.0), (121, 10.0), (241, 20.0)];

pub static SPEC: CivSpec = CivSpec {
    modes: MODES,
    att: &[(0, 0x00), (1, 0x10)],
    max_watts: 100.0,
    s_cal: &icom::S_CAL,
    po_cal: &icom::PO_CAL,
    swr_cal: &icom::SWR_CAL,
    alc_cal: &icom::ALC_CAL,
    comp_cal: &super::ic705::COMP_CAL,
    id_cal: &ID_CAL,
    vd_cal: &icom::VD_CAL,
    transceive: Some(127),
    keyer_memories: 8,
    voice_memories: 8,
    antenna_switch: false,
    apf: false,
    apf_widths: false,
    apf_hz: 1270.0,
    lock: true,
    memory_name_len: 16,
    memory: MemLayout::Band,
    main_sub: false,
    antenna_rx: false,
    dual_band: true,
    tone_function: true,
    duplex: true,
    vox_delay: Some(330),
    keyer_type: Some(227),
    extra: EXTRA,
};

pub static IC9700: RadioModel = RadioModel {
    key: "ic9700",
    name: "IC-9700",
    maker: Maker::Icom,
    family: Family::Icom,
    proto: Proto::CiV(&SPEC),
    id: "",
    civ_addr: 0xA2,
    hamlib: 3081,
    freq_min: 144_000_000,
    freq_max: 1_300_000_000,
    bands: BANDS,
    modes: MODE_LIST,
    menu: &MENU,
    roles: Roles {
        mic_sel: &[icom::num_of(&MENU, 115), icom::num_of(&MENU, 116)],
        mic_front: 0,
        mic_usb: 3,
        // USB (B) RTS.
        pc_keying: Some((icom::num_of(&MENU, 121), 0, 4)),
        cat_tot: None,
        cat_items: &[
            icom::num_of(&MENU, 127),
            icom::num_of(&MENU, 128),
            icom::num_of(&MENU, 129),
            icom::num_of(&MENU, 130),
            icom::num_of(&MENU, 131),
            icom::num_of(&MENU, 132),
            icom::num_of(&MENU, 133),
        ],
        cw_bfo: Some((icom::num_of(&MENU, 67), ["01", "00", "--"])),
        cw_pitch_display: None,
        data_other: None,
        data_disp: None,
    },
    caps: Caps {
        commands: COMMANDS,
        widths: icom::widths,
        narrow: false,
        preamp: &[(0, "OFF"), (1, "P.AMP"), (2, "EXT-P.AMP"), (3, "P.AMP + EXT-P.AMP")],
        att: &[(0, "OFF"), (1, "10 dB")],
        agc: &[(1, "FAST"), (2, "MID"), (3, "SLOW")],
        roofing: &[],
        roofing_answers: &[],
        nb: &[(0, "OFF"), (1, "NB")],
        nr_max: 15,
        nb_level_max: 100,
        antennas: 1,
        // 100 W on 2 m, 75 W on 70 cm, 10 W on 23 cm.
        power: (1, 100),
        meter_select: &[],
        voice_memories: 8,
        keyer_memories: 8,
        keyer_memory_len: 70,
        memory: Memory { first: 101, last: 399, pms: None },
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
    fn ic9700_literal_frames() {
        let m = &IC9700;
        let mut r = radio(m);
        // 1296.1 MHz: ten digits in RigHelm.
        assert_eq!(down(&mut r, "FA1296100000;"), ["FE FE A2 E0 25 00 00 00 10 96 12 FD"]);
        up(&mut r, "FE FE E0 A2 FB FD");
        assert_eq!(down(&mut r, "FA;"), ["FE FE A2 E0 25 00 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 A2 25 00 00 00 10 96 12 FD"), ["FA1296100000;"]);
        assert_eq!(down(&mut r, "VS1;"), ["FE FE A2 E0 07 01 FD"]);
        up(&mut r, "FE FE E0 A2 FB FD");
        assert_eq!(down(&mut r, "MD0F;"), ["FE FE A2 E0 26 01 17 00 01 FD"]);
        up(&mut r, "FE FE E0 A2 FB FD");
        assert_eq!(down(&mut r, "RA01;"), ["FE FE A2 E0 11 10 FD"]);
        up(&mut r, "FE FE E0 A2 FB FD");
        assert_eq!(down(&mut r, "PA03;"), ["FE FE A2 E0 16 02 03 FD"]);
        up(&mut r, "FE FE E0 A2 FB FD");
        assert_eq!(down(&mut r, "BS03;"), ["FE FE A2 E0 1A 01 03 01 FD"]);
        up(&mut r, "FE FE E0 A2 FA FD");
        // Memory 5 of the 70 cm band.
        assert_eq!(down(&mut r, "MR205;"), ["FE FE A2 E0 1A 00 02 00 05 FD"]);
        assert_eq!(
            up(
                &mut r,
                "FE FE E0 A2 1A 00 02 00 05 00 00 00 50 33 04 05 01 00 01 00 00 08 85 00 08 85 00 00 23 00 00 00 00 \
                 43 51 43 51 43 51 20 20 20 20 20 20 20 20 20 20 20 20 20 20 20 20 20 20 FD"
            ),
            ["MR205433500000+000000412000;"]
        );
        let sat = num_of(m.menu, 9029);
        assert_eq!(down(&mut r, &format!("EX{sat:03}01;")), ["FE FE A2 E0 16 5A 01 FD"]);
        up(&mut r, "FE FE E0 A2 FB FD");
        assert_eq!(down(&mut r, "VD0100;"), ["FE FE A2 E0 1A 05 03 30 01 FD"]);
        up(&mut r, "FE FE E0 A2 FB FD");
        // Id: 01 21 = 10 A.
        assert_eq!(down(&mut r, "RM7;"), ["FE FE A2 E0 15 16 FD"]);
        assert_eq!(up(&mut r, "FE FE E0 A2 15 16 01 21 FD"), ["RM7102;"]);
        // No VFO swap (07 B0 swaps MAIN and SUB) and no memory select.
        assert_eq!(down(&mut r, "SV;"), ["?;"]);
        assert_eq!(down(&mut r, "MC101;"), ["?;"]);
    }

    #[test]
    fn ic9700_round_trips() {
        let m = &IC9700;
        for f in [
            "FA145500000;", "FB435000000;", "FA1296100000;", "MD0F;", "MD0C;", "MD04;", "PA02;", "PA03;", "RA01;", "PC050;",
            "MW10114552000+000000400000;", "MW30212961000+000000C00000;",
        ] {
            round_trip_set(m, f);
        }
        menu_round_trips(m);
        for (read, answer) in [("FA;", "FA1296100000;"), ("MD0;", "MD0F;"), ("MR101;", "MR101145520000+000000412000;")] {
            round_trip_read(m, read, answer);
        }
    }
}
