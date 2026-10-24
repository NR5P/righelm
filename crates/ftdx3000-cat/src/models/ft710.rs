//! Yaesu FT-710: the FTDX101's CAT generation (see [`super::ftdx101`]) cut
//! down. RigHelm's commands map onto what it has: the clarifier is `CF`
//! (for `RT`/`XT`/`RC`/`RU`/`RD`), split is `FT0`/`FT1` (or `ST`), busy is
//! a flag in the one-frame `RI0;` answer, `KY` names text or message
//! memories, `AC` starts tuning with 3. It has no roofing filters, `AN`,
//! `FR`, `FS`, `RS`, `QS`, `EM`/`EN`, `ED`/`EU` or `LM`.

use super::ftdx10::{CH_STEP, CW_10, ENC_DEC, RPT, SSB_10};
use super::ftdx101::{self as dx, Flavor, mi};
use super::ftdx101::{
    AGC_DELAY, AMC_RELEASE, AM_CH_STEP, APF_WIDTH, BK_IN_TYPE, BRIGHTNESS, CONTEST_NUMBER, CONTOUR_LEVEL, CONTOUR_WIDTH,
    CW_AUTO, CW_MEM, CW_WEIGHT, DATA_SHIFT, EQ_LEVEL, EQ1_FREQ, EQ2_FREQ, EQ3_FREQ, FM_CH_STEP, FREQ_DISPLAY, HCUT,
    KEYBOARD, KEYER_TYPE, LCUT, LEVEL, MARK_FREQ, NOR_REV, NOTCH_WIDTH, NUMBER_STYLE, OFF_ON, PIXEL, POWER_25W, POWER_50W,
    POWER_100W, PSK_TONE, QMB_CH, QSK_DELAY, RBW, REF_FREQ_ADJ, REPEAT_INTERVAL, RPT_SHIFT_28, RPT_SHIFT_50, SCAN_RESUME,
    SCREEN_SAVER, SENSITIVITY, SHIFT_FREQ, SLOPE, TOT, TX_BPF, TX_TOT,
};
use crate::dialect::text::{Ctx, TextSpec, read_prefix};
use crate::menu::{Kind, MenuItem, num, signed};
use crate::model::{Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::types::{self, Band, WidthClass};

pub static SPEC: TextSpec = TextSpec { down, up, answer_prefix: read_prefix };

fn down(cx: &mut Ctx, f: &str) -> Vec<String> {
    dx::down_with(cx, f, &FLAVOR)
}

fn up(cx: &mut Ctx, f: &str) -> Vec<String> {
    dx::up_with(cx, f, &FLAVOR)
}

static FLAVOR: Flavor = Flavor {
    dual_meter: false,
    da_contrast: true,
    encoders: &[],
    ft_select: true,
    clar_cf: true,
    single_lock: true,
    packed_ri: true,
    tune_start: '3',
    ky_split: true,
    vc_tune: false,
};

/// Table 3: SSB as the FTDX10's; CW, DATA and PSK share the CW column
/// (AM/FM widths are fixed).
fn widths(class: WidthClass, _narrow: bool) -> Vec<(u8, u32)> {
    match class {
        WidthClass::Ssb => dx::table(&SSB_10),
        WidthClass::Cw | WidthClass::Rtty => dx::table(&CW_10),
        WidthClass::Fixed => Vec::new(),
    }
}

/// 00-10 like the FTDX3000's; 11 is "70 MHz/GEN".
const BANDS: &[Band] = &[
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
    Band { code: 11, label: "GEN", lo: 30_000, hi: 75_000_000 },
];

const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AM", "BA", "BC", "BD", "BI", "BP", "BS", "BU", "BY", "CH", "CN", "CO", "CS", "CT", "DA", "DN",
    "EX", "FA", "FB", "FT", "GT", "ID", "IF", "IS", "KM", "KP", "KR", "KS", "KY", "LK", "MA", "MC", "MD", "MG", "ML", "MR",
    "MS", "MW", "MX", "NA", "NB", "NL", "NR", "OI", "OS", "PA", "PB", "PC", "PL", "PR", "PS", "QI", "QR", "RA", "RC", "RD",
    "RG", "RI", "RL", "RM", "RT", "RU", "SC", "SD", "SH", "SM", "SQ", "SV", "TS", "TX", "UP", "VD", "VG", "VM", "VS", "VX",
    "XT",
];

pub static FT710: RadioModel = RadioModel {
    key: "ft710",
    name: "FT-710",
    maker: Maker::Yaesu,
    family: Family::Yaesu,
    proto: Proto::Text(&SPEC),
    id: "0800",
    civ_addr: 0,
    hamlib: 1049,
    freq_min: 30_000,
    freq_max: 75_000_000,
    bands: BANDS,
    modes: dx::MODES,
    menu: MENU,
    roles: Roles {
        // SSB/AM/FM/DATA MOD SOURCE: MIC, USB, REAR, AUTO.
        mic_sel: &[14, 33, 49, 71],
        mic_front: 0,
        mic_usb: 1,
        pc_keying: Some((109, 0, 1)),
        cat_tot: Some((131, "1")),
        // TUN/LIN PORT SELECT (CAT-3 is on that jack), CAT-1/2/3 rates,
        // time-outs and stop bits, also those in the five presets.
        cat_items: &[
            128, 130, 131, 132, 133, 134, 135, 136, 208, 209, 210, 226, 227, 228, 244, 245, 246, 262, 263, 264, 280, 281, 282,
        ],
        cw_bfo: None,
        cw_pitch_display: Some((112, "1")),
        // DATA-L/U use the CW/PSK widths (Table 3), as RigHelm does for a
        // DATA MODE that isn't OTHER; the FT-710 has no such item, so this
        // names one whose value is never "OTHER".
        data_other: Some((75, "OTHER")),
        data_disp: None,
    },
    caps: Caps {
        commands: COMMANDS,
        widths,
        narrow: false,
        preamp: &[(0, "IPO"), (1, "AMP1"), (2, "AMP2")],
        att: &[(0, "OFF"), (1, "6"), (2, "12"), (3, "18")],
        agc: types::AGC_SET,
        roofing: &[],
        roofing_answers: &[],
        nb: &[(0, "OFF"), (1, "ON")],
        nr_max: 15,
        nb_level_max: 10,
        antennas: 1,
        power: (5, 100),
        meter_select: &[(0, "PO"), (1, "COMP"), (2, "ALC"), (3, "VDD"), (4, "ID"), (5, "SWR")],
        voice_memories: 5,
        keyer_memories: 5,
        keyer_memory_len: 50,
        memory: Memory { first: 1, last: 117, pms: Some(100) },
        shared_clarifier: true,
    },
    // CAT-1 (the USB Enhanced COM port) at up to 115200, 1 stop bit by
    // default (menu CAT-1 CAT-3 STOP BIT), no handshake.
    bauds: &[115200, 38400, 19200, 9600, 4800],
    stop_bits: 1,
    rts_cts: false,
    sim: &["MS2;", "SH013;"],
};

// ------------------------------------------------------------- menu

const AF_TONE: Kind = signed(-20, 10, 1, "");
const MOD_SRC: Kind = Kind::Choice(&["MIC", "USB", "REAR", "AUTO"]);
/// RPTT SELECT and PC KEYING.
const PORT_LINE: Kind = Kind::Choice(&["OFF", "RTS", "DTR", "DAKY"]);
const NAR_SSB: Kind = Kind::Choice(&[
    "300 Hz", "400 Hz", "600 Hz", "850 Hz", "1100 Hz", "1200 Hz", "1500 Hz", "1650 Hz", "1800 Hz", "1950 Hz", "2100 Hz",
    "2250 Hz", "2400 Hz", "2450 Hz", "2500 Hz", "2600 Hz", "2700 Hz", "2800 Hz", "2900 Hz", "3000 Hz", "3200 Hz", "3500 Hz",
    "4000 Hz",
]);
const NAR_CW: Kind = Kind::Choice(&[
    "50 Hz", "100 Hz", "150 Hz", "200 Hz", "250 Hz", "300 Hz", "350 Hz", "400 Hz", "450 Hz", "500 Hz", "600 Hz", "800 Hz",
    "1200 Hz", "1400 Hz", "1700 Hz", "2000 Hz", "2400 Hz", "3000 Hz", "3200 Hz", "3500 Hz", "4000 Hz",
]);
const WAVE_SHAPE: Kind = Kind::Choice(&["4 ms", "6 ms", "8 ms"]);
const RF_SQL: Kind = Kind::Choice(&["RF", "SQL", "SQL (FM only)"]);
const TUN_LIN_PORT: Kind = Kind::Choice(&["EXT-TUNER", "LINEAR", "CAT-3", "GPO"]);
const TUNER_TYPE: Kind = Kind::Choice(&["INT", "INT (FAST)", "EXT", "ATAS"]);
const RATE: Kind = Kind::Choice(&["4800 bps", "9600 bps", "19200 bps", "38400 bps", "115200 bps"]);
const STOP_BIT: Kind = Kind::Choice(&["1 bit", "2 bit"]);
const MIC_KEY: Kind = Kind::Choice(&[
    "LOCK", "QMB", "A/B", "V/M", "TUNER", "VOX/MOX", "MODE", "ZIN/SPOT", "SPLIT", "FINE", "NAR", "NB", "DNR", "FREQ UP",
    "FREQ DOWN", "BAND UP", "BAND DOWN", "ATT", "IPO", "DNF", "AGC",
]);
const NB_REJECTION: Kind = Kind::Choice(&["LOW", "MID", "HIGH"]);
const NB_WIDTH: Kind = Kind::Choice(&["NARROW", "MEDIUM", "WIDE"]);
const EQ_BWTH: Kind = num(0, 10, 1, "");
const VOX_SELECT: Kind = Kind::Choice(&["MIC", "USB", "REAR"]);
const METER_DETECTOR: Kind = Kind::Choice(&["AVERAGE", "PEAK"]);
const DIAL_STEP: Kind = Kind::Choice(&["5 Hz", "10 Hz", "20 Hz"]);
const MAIN_STEPS: Kind = Kind::Choice(&["50", "100", "200"]);
const MY_CALL_TIME: Kind = Kind::Choice(&["OFF", "1 s", "2 s", "3 s", "4 s", "5 s"]);
const POP_UP_TIME: Kind = Kind::Choice(&["FAST", "MID", "SLOW"]);
const SCOPE_CTR: Kind = Kind::Choice(&["FILTER", "CARRIER POINT"]);
const VMI_COLOR: Kind = Kind::Choice(&["BLUE", "GREEN", "WHITE", "NONE"]);
const VMI_CLAR: Kind = Kind::Choice(&["RED", "NONE"]);

/// Table 2, the five EXTENSION SETTING presets included.
const MENU: &[MenuItem] = &[
    mi(1, 10101, "MODE SSB", "AF TREBLE GAIN", 3, AF_TONE),
    mi(2, 10102, "MODE SSB", "AF MIDDLE TONE GAIN", 3, AF_TONE),
    mi(3, 10103, "MODE SSB", "AF BASS GAIN", 3, AF_TONE),
    mi(4, 10104, "MODE SSB", "AGC FAST DELAY", 4, AGC_DELAY),
    mi(5, 10105, "MODE SSB", "AGC MID DELAY", 4, AGC_DELAY),
    mi(6, 10106, "MODE SSB", "AGC SLOW DELAY", 4, AGC_DELAY),
    mi(7, 10107, "MODE SSB", "LCUT FREQ", 2, LCUT),
    mi(8, 10108, "MODE SSB", "LCUT SLOPE", 1, SLOPE),
    mi(9, 10109, "MODE SSB", "HCUT FREQ", 2, HCUT),
    mi(10, 10110, "MODE SSB", "HCUT SLOPE", 1, SLOPE),
    mi(11, 10111, "MODE SSB", "USB OUT LEVEL", 3, LEVEL),
    mi(12, 10112, "MODE SSB", "REAR OUT LEVEL", 3, LEVEL),
    mi(13, 10113, "MODE SSB", "TX BPF SEL", 1, TX_BPF),
    mi(14, 10114, "MODE SSB", "MOD SOURCE", 1, MOD_SRC),
    mi(15, 10115, "MODE SSB", "USB MOD GAIN", 3, LEVEL),
    mi(16, 10116, "MODE SSB", "REAR MOD GAIN", 3, LEVEL),
    mi(17, 10117, "MODE SSB", "RPTT SELECT", 1, PORT_LINE),
    mi(18, 10118, "MODE SSB", "NAR WIDTH", 2, NAR_SSB),
    mi(19, 10119, "MODE SSB", "CW AUTO MODE", 1, CW_AUTO),
    mi(20, 10201, "MODE AM", "AF TREBLE GAIN", 3, AF_TONE),
    mi(21, 10202, "MODE AM", "AF MIDDLE TONE GAIN", 3, AF_TONE),
    mi(22, 10203, "MODE AM", "AF BASS GAIN", 3, AF_TONE),
    mi(23, 10204, "MODE AM", "AGC FAST DELAY", 4, AGC_DELAY),
    mi(24, 10205, "MODE AM", "AGC MID DELAY", 4, AGC_DELAY),
    mi(25, 10206, "MODE AM", "AGC SLOW DELAY", 4, AGC_DELAY),
    mi(26, 10207, "MODE AM", "LCUT FREQ", 2, LCUT),
    mi(27, 10208, "MODE AM", "LCUT SLOPE", 1, SLOPE),
    mi(28, 10209, "MODE AM", "HCUT FREQ", 2, HCUT),
    mi(29, 10210, "MODE AM", "HCUT SLOPE", 1, SLOPE),
    mi(30, 10211, "MODE AM", "USB OUT LEVEL", 3, LEVEL),
    mi(31, 10212, "MODE AM", "REAR OUT LEVEL", 3, LEVEL),
    mi(32, 10213, "MODE AM", "TX BPF SEL", 1, TX_BPF),
    mi(33, 10214, "MODE AM", "MOD SOURCE", 1, MOD_SRC),
    mi(34, 10215, "MODE AM", "USB MOD GAIN", 3, LEVEL),
    mi(35, 10216, "MODE AM", "REAR MOD GAIN", 3, LEVEL),
    mi(36, 10217, "MODE AM", "RPTT SELECT", 1, PORT_LINE),
    mi(37, 10301, "MODE FM", "AF TREBLE GAIN", 3, AF_TONE),
    mi(38, 10302, "MODE FM", "AF MIDDLE TONE GAIN", 3, AF_TONE),
    mi(39, 10303, "MODE FM", "AF BASS GAIN", 3, AF_TONE),
    mi(40, 10304, "MODE FM", "AGC FAST DELAY", 4, AGC_DELAY),
    mi(41, 10305, "MODE FM", "AGC MID DELAY", 4, AGC_DELAY),
    mi(42, 10306, "MODE FM", "AGC SLOW DELAY", 4, AGC_DELAY),
    mi(43, 10307, "MODE FM", "LCUT FREQ", 2, LCUT),
    mi(44, 10308, "MODE FM", "LCUT SLOPE", 1, SLOPE),
    mi(45, 10309, "MODE FM", "HCUT FREQ", 2, HCUT),
    mi(46, 10310, "MODE FM", "HCUT SLOPE", 1, SLOPE),
    mi(47, 10311, "MODE FM", "USB OUT LEVEL", 3, LEVEL),
    mi(48, 10312, "MODE FM", "REAR OUT LEVEL", 3, LEVEL),
    mi(49, 10313, "MODE FM", "MOD SOURCE", 1, MOD_SRC),
    mi(50, 10314, "MODE FM", "USB MOD GAIN", 3, LEVEL),
    mi(51, 10315, "MODE FM", "REAR MOD GAIN", 3, LEVEL),
    mi(52, 10316, "MODE FM", "RPTT SELECT", 1, PORT_LINE),
    mi(53, 10317, "MODE FM", "RPT SHIFT", 1, RPT),
    mi(54, 10318, "MODE FM", "RPT SHIFT (28 MHz)", 4, RPT_SHIFT_28),
    mi(55, 10319, "MODE FM", "RPT SHIFT (50 MHz)", 4, RPT_SHIFT_50),
    mi(56, 10320, "MODE FM", "ENC/DEC", 1, ENC_DEC),
    mi(57, 10321, "MODE FM", "TONE FREQ", 2, Kind::Tone),
    mi(58, 10401, "MODE PSK/DATA", "AF TREBLE GAIN", 3, AF_TONE),
    mi(59, 10402, "MODE PSK/DATA", "AF MIDDLE TONE GAIN", 3, AF_TONE),
    mi(60, 10403, "MODE PSK/DATA", "AF BASS GAIN", 3, AF_TONE),
    mi(61, 10404, "MODE PSK/DATA", "AGC FAST DELAY", 4, AGC_DELAY),
    mi(62, 10405, "MODE PSK/DATA", "AGC MID DELAY", 4, AGC_DELAY),
    mi(63, 10406, "MODE PSK/DATA", "AGC SLOW DELAY", 4, AGC_DELAY),
    mi(64, 10407, "MODE PSK/DATA", "LCUT FREQ", 2, LCUT),
    mi(65, 10408, "MODE PSK/DATA", "LCUT SLOPE", 1, SLOPE),
    mi(66, 10409, "MODE PSK/DATA", "HCUT FREQ", 2, HCUT),
    mi(67, 10410, "MODE PSK/DATA", "HCUT SLOPE", 1, SLOPE),
    mi(68, 10411, "MODE PSK/DATA", "USB OUT LEVEL", 3, LEVEL),
    mi(69, 10412, "MODE PSK/DATA", "REAR OUT LEVEL", 3, LEVEL),
    mi(70, 10413, "MODE PSK/DATA", "TX BPF SEL", 1, TX_BPF),
    mi(71, 10414, "MODE PSK/DATA", "MOD SOURCE", 1, MOD_SRC),
    mi(72, 10415, "MODE PSK/DATA", "USB MOD GAIN", 3, LEVEL),
    mi(73, 10416, "MODE PSK/DATA", "REAR MOD GAIN", 3, LEVEL),
    mi(74, 10417, "MODE PSK/DATA", "RPTT SELECT", 1, PORT_LINE),
    mi(75, 10418, "MODE PSK/DATA", "NAR WIDTH", 2, NAR_CW),
    mi(76, 10419, "MODE PSK/DATA", "PSK TONE", 1, PSK_TONE),
    mi(77, 10420, "MODE PSK/DATA", "DATA SHIFT (SSB)", 4, DATA_SHIFT),
    mi(78, 10501, "MODE RTTY", "AF TREBLE GAIN", 3, AF_TONE),
    mi(79, 10502, "MODE RTTY", "AF MIDDLE TONE GAIN", 3, AF_TONE),
    mi(80, 10503, "MODE RTTY", "AF BASS GAIN", 3, AF_TONE),
    mi(81, 10504, "MODE RTTY", "AGC FAST DELAY", 4, AGC_DELAY),
    mi(82, 10505, "MODE RTTY", "AGC MID DELAY", 4, AGC_DELAY),
    mi(83, 10506, "MODE RTTY", "AGC SLOW DELAY", 4, AGC_DELAY),
    mi(84, 10507, "MODE RTTY", "LCUT FREQ", 2, LCUT),
    mi(85, 10508, "MODE RTTY", "LCUT SLOPE", 1, SLOPE),
    mi(86, 10509, "MODE RTTY", "HCUT FREQ", 2, HCUT),
    mi(87, 10510, "MODE RTTY", "HCUT SLOPE", 1, SLOPE),
    mi(88, 10511, "MODE RTTY", "USB OUT LEVEL", 3, LEVEL),
    mi(89, 10512, "MODE RTTY", "REAR OUT LEVEL", 3, LEVEL),
    mi(90, 10513, "MODE RTTY", "RPTT SELECT", 1, PORT_LINE),
    mi(91, 10514, "MODE RTTY", "NAR WIDTH", 2, NAR_CW),
    mi(92, 10515, "MODE RTTY", "MARK FREQUENCY", 1, MARK_FREQ),
    mi(93, 10516, "MODE RTTY", "SHIFT FREQUENCY", 1, SHIFT_FREQ),
    mi(94, 10517, "MODE RTTY", "POLARITY-TX", 1, NOR_REV),
    mi(95, 20101, "MODE CW", "AF TREBLE GAIN", 3, AF_TONE),
    mi(96, 20102, "MODE CW", "AF MIDDLE TONE GAIN", 3, AF_TONE),
    mi(97, 20103, "MODE CW", "AF BASS GAIN", 3, AF_TONE),
    mi(98, 20104, "MODE CW", "AGC FAST DELAY", 4, AGC_DELAY),
    mi(99, 20105, "MODE CW", "AGC MID DELAY", 4, AGC_DELAY),
    mi(100, 20106, "MODE CW", "AGC SLOW DELAY", 4, AGC_DELAY),
    mi(101, 20107, "MODE CW", "LCUT FREQ", 2, LCUT),
    mi(102, 20108, "MODE CW", "LCUT SLOPE", 1, SLOPE),
    mi(103, 20109, "MODE CW", "HCUT FREQ", 2, HCUT),
    mi(104, 20110, "MODE CW", "HCUT SLOPE", 1, SLOPE),
    mi(105, 20111, "MODE CW", "USB OUT LEVEL", 3, LEVEL),
    mi(106, 20112, "MODE CW", "REAR OUT LEVEL", 3, LEVEL),
    mi(107, 20113, "MODE CW", "RPTT SELECT", 1, PORT_LINE),
    mi(108, 20114, "MODE CW", "NAR WIDTH", 2, NAR_CW),
    mi(109, 20115, "MODE CW", "PC KEYING", 1, PORT_LINE),
    mi(110, 20116, "MODE CW", "CW BK-IN TYPE", 1, BK_IN_TYPE),
    mi(111, 20117, "MODE CW", "CW WAVE SHAPE", 1, WAVE_SHAPE),
    mi(112, 20118, "MODE CW", "CW FREQ DISPLAY", 1, FREQ_DISPLAY),
    mi(113, 20119, "MODE CW", "QSK DELAY TIME", 1, QSK_DELAY),
    mi(114, 20120, "MODE CW", "CW INDICATOR", 1, OFF_ON),
    mi(115, 20201, "KEYER", "KEYER TYPE", 1, KEYER_TYPE),
    mi(116, 20202, "KEYER", "KEYER DOT/DASH", 1, NOR_REV),
    mi(117, 20203, "KEYER", "CW WEIGHT", 2, CW_WEIGHT),
    mi(118, 20204, "KEYER", "NUMBER STYLE", 1, NUMBER_STYLE),
    mi(119, 20205, "KEYER", "CONTEST NUMBER", 4, CONTEST_NUMBER),
    mi(120, 20206, "KEYER", "CW MEMORY 1", 1, CW_MEM),
    mi(121, 20207, "KEYER", "CW MEMORY 2", 1, CW_MEM),
    mi(122, 20208, "KEYER", "CW MEMORY 3", 1, CW_MEM),
    mi(123, 20209, "KEYER", "CW MEMORY 4", 1, CW_MEM),
    mi(124, 20210, "KEYER", "CW MEMORY 5", 1, CW_MEM),
    mi(125, 20211, "KEYER", "REPEAT INTERVAL", 2, REPEAT_INTERVAL),
    mi(126, 30101, "GENERAL", "BEEP LEVEL", 3, LEVEL),
    mi(127, 30102, "GENERAL", "RF/SQL VR", 1, RF_SQL),
    mi(128, 30103, "GENERAL", "TUN/LIN PORT SELECT", 1, TUN_LIN_PORT),
    mi(129, 30104, "GENERAL", "TUNER TYPE SELECT", 1, TUNER_TYPE),
    mi(130, 30105, "GENERAL", "CAT-1 RATE", 1, RATE),
    mi(131, 30106, "GENERAL", "CAT-1 TIME OUT TIMER", 1, TOT),
    mi(132, 30107, "GENERAL", "CAT-1 CAT-3 STOP BIT", 1, STOP_BIT),
    mi(133, 30108, "GENERAL", "CAT-2 RATE", 1, RATE),
    mi(134, 30109, "GENERAL", "CAT-2 TIME OUT TIMER", 1, TOT),
    mi(135, 30110, "GENERAL", "CAT-3 RATE", 1, RATE),
    mi(136, 30111, "GENERAL", "CAT-3 TIME OUT TIMER", 1, TOT),
    mi(137, 30112, "GENERAL", "QMB CH", 1, QMB_CH),
    mi(138, 30113, "GENERAL", "BAND STACK", 1, OFF_ON),
    mi(139, 30114, "GENERAL", "MEM GROUP", 1, OFF_ON),
    mi(140, 30115, "GENERAL", "TX TIME OUT TIMER", 2, TX_TOT),
    mi(141, 30116, "GENERAL", "MIC SCAN", 1, OFF_ON),
    mi(142, 30117, "GENERAL", "MIC SCAN RESUME", 1, SCAN_RESUME),
    mi(143, 30118, "GENERAL", "REF FREQ ADJ", 3, REF_FREQ_ADJ),
    mi(144, 30119, "GENERAL", "KEYBOARD LANGUAGE", 2, KEYBOARD),
    mi(145, 30120, "GENERAL", "MIC P1", 2, MIC_KEY),
    mi(146, 30121, "GENERAL", "MIC P2", 2, MIC_KEY),
    mi(147, 30122, "GENERAL", "MIC P3", 2, MIC_KEY),
    mi(148, 30123, "GENERAL", "MIC P4", 2, MIC_KEY),
    mi(149, 30124, "GENERAL", "MIC UP", 2, MIC_KEY),
    mi(150, 30125, "GENERAL", "MIC DOWN", 2, MIC_KEY),
    mi(151, 30126, "GENERAL", "SCU-LAN10", 1, OFF_ON),
    mi(152, 30201, "RX DSP", "IF NOTCH WIDTH", 1, NOTCH_WIDTH),
    mi(153, 30202, "RX DSP", "NB REJECTION", 1, NB_REJECTION),
    mi(154, 30203, "RX DSP", "NB WIDTH", 1, NB_WIDTH),
    mi(155, 30204, "RX DSP", "APF WIDTH", 1, APF_WIDTH),
    mi(156, 30205, "RX DSP", "CONTOUR LEVEL", 3, CONTOUR_LEVEL),
    mi(157, 30206, "RX DSP", "CONTOUR WIDTH", 2, CONTOUR_WIDTH),
    mi(158, 30301, "TX AUDIO", "AMC RELEASE TIME", 1, AMC_RELEASE),
    mi(159, 30302, "TX AUDIO", "PRMTRC EQ1 FREQ", 2, EQ1_FREQ),
    mi(160, 30303, "TX AUDIO", "PRMTRC EQ1 LEVEL", 3, EQ_LEVEL),
    mi(161, 30304, "TX AUDIO", "PRMTRC EQ1 BWTH", 2, EQ_BWTH),
    mi(162, 30305, "TX AUDIO", "PRMTRC EQ2 FREQ", 2, EQ2_FREQ),
    mi(163, 30306, "TX AUDIO", "PRMTRC EQ2 LEVEL", 3, EQ_LEVEL),
    mi(164, 30307, "TX AUDIO", "PRMTRC EQ2 BWTH", 2, EQ_BWTH),
    mi(165, 30308, "TX AUDIO", "PRMTRC EQ3 FREQ", 2, EQ3_FREQ),
    mi(166, 30309, "TX AUDIO", "PRMTRC EQ3 LEVEL", 3, EQ_LEVEL),
    mi(167, 30310, "TX AUDIO", "PRMTRC EQ3 BWTH", 2, EQ_BWTH),
    mi(168, 30311, "TX AUDIO", "P PRMTRC EQ1 FREQ", 2, EQ1_FREQ),
    mi(169, 30312, "TX AUDIO", "P PRMTRC EQ1 LEVEL", 3, EQ_LEVEL),
    mi(170, 30313, "TX AUDIO", "P PRMTRC EQ1 BWTH", 2, EQ_BWTH),
    mi(171, 30314, "TX AUDIO", "P PRMTRC EQ2 FREQ", 2, EQ2_FREQ),
    mi(172, 30315, "TX AUDIO", "P PRMTRC EQ2 LEVEL", 3, EQ_LEVEL),
    mi(173, 30316, "TX AUDIO", "P PRMTRC EQ2 BWTH", 2, EQ_BWTH),
    mi(174, 30317, "TX AUDIO", "P PRMTRC EQ3 FREQ", 2, EQ3_FREQ),
    mi(175, 30318, "TX AUDIO", "P PRMTRC EQ3 LEVEL", 3, EQ_LEVEL),
    mi(176, 30319, "TX AUDIO", "P PRMTRC EQ3 BWTH", 2, EQ_BWTH),
    mi(177, 30401, "TX GENERAL", "HF MAX POWER", 3, POWER_100W),
    mi(178, 30402, "TX GENERAL", "50M MAX POWER", 3, POWER_100W),
    mi(179, 30403, "TX GENERAL", "70M MAX POWER", 3, POWER_50W),
    mi(180, 30404, "TX GENERAL", "AM MAX POWER", 3, POWER_25W),
    mi(181, 30405, "TX GENERAL", "VOX SELECT", 1, VOX_SELECT),
    mi(182, 30406, "TX GENERAL", "EMERGENCY FREQ TX", 1, OFF_ON),
    mi(183, 30407, "TX GENERAL", "TX INHIBIT", 1, OFF_ON),
    mi(184, 30408, "TX GENERAL", "METER DETECTOR", 1, METER_DETECTOR),
    mi(185, 30501, "TUNING", "SSB/CW DIAL STEP", 1, DIAL_STEP),
    mi(186, 30502, "TUNING", "RTTY/PSK DIAL STEP", 1, DIAL_STEP),
    mi(187, 30503, "TUNING", "CH STEP", 1, CH_STEP),
    mi(188, 30504, "TUNING", "AM CH STEP", 1, AM_CH_STEP),
    mi(189, 30505, "TUNING", "FM CH STEP", 1, FM_CH_STEP),
    mi(190, 30506, "TUNING", "MAIN STEPS PER REV.", 1, MAIN_STEPS),
    mi(191, 40101, "DISPLAY", "MY CALL", 12, Kind::Text { max: 12 }),
    mi(192, 40102, "DISPLAY", "MY CALL TIME", 1, MY_CALL_TIME),
    mi(193, 40103, "DISPLAY", "POP-UP TIME", 1, POP_UP_TIME),
    mi(194, 40104, "DISPLAY", "SCREEN SAVER", 1, SCREEN_SAVER),
    mi(195, 40105, "DISPLAY", "DIMMER LED", 2, BRIGHTNESS),
    mi(196, 40106, "DISPLAY", "MOUSE POINTER SPEED", 2, BRIGHTNESS),
    mi(197, 40201, "SCOPE", "RBW", 1, RBW),
    mi(198, 40202, "SCOPE", "SCOPE CTR", 1, SCOPE_CTR),
    mi(199, 40203, "SCOPE", "2D DISP SENSITIVITY", 1, SENSITIVITY),
    mi(200, 40204, "SCOPE", "3DSS DISP SENSITIVITY", 1, SENSITIVITY),
    mi(201, 40301, "VFO IND COLOR", "VMI COLOR VFO-A", 1, VMI_COLOR),
    mi(202, 40302, "VFO IND COLOR", "VMI COLOR VFO-B", 1, VMI_COLOR),
    mi(203, 40303, "VFO IND COLOR", "VMI COLOR MEMORY", 1, VMI_COLOR),
    mi(204, 40304, "VFO IND COLOR", "VMI COLOR CLAR", 1, VMI_CLAR),
    mi(205, 40401, "EXT MONITOR", "EXT DISPLAY", 1, OFF_ON),
    mi(206, 40402, "EXT MONITOR", "PIXEL", 1, PIXEL),
    mi(207, 60101, "PRESET 1", "PRESET NAME", 12, Kind::Text { max: 12 }),
    mi(208, 60102, "PRESET 1", "CAT-1 RATE", 1, RATE),
    mi(209, 60103, "PRESET 1", "CAT-1 TIME OUT TIMER", 1, TOT),
    mi(210, 60104, "PRESET 1", "CAT-1 CAT-3 STOP BIT", 1, STOP_BIT),
    mi(211, 60105, "PRESET 1", "AGC FAST DELAY", 4, AGC_DELAY),
    mi(212, 60106, "PRESET 1", "AGC MID DELAY", 4, AGC_DELAY),
    mi(213, 60107, "PRESET 1", "AGC SLOW DELAY", 4, AGC_DELAY),
    mi(214, 60108, "PRESET 1", "LCUT FREQ", 2, LCUT),
    mi(215, 60109, "PRESET 1", "LCUT SLOPE", 1, SLOPE),
    mi(216, 60110, "PRESET 1", "HCUT FREQ", 2, HCUT),
    mi(217, 60111, "PRESET 1", "HCUT SLOPE", 1, SLOPE),
    mi(218, 60112, "PRESET 1", "USB OUT LEVEL", 3, LEVEL),
    mi(219, 60113, "PRESET 1", "REAR OUT LEVEL", 3, LEVEL),
    mi(220, 60114, "PRESET 1", "TX BPF SEL", 1, TX_BPF),
    mi(221, 60115, "PRESET 1", "MOD SOURCE", 1, MOD_SRC),
    mi(222, 60116, "PRESET 1", "USB MOD GAIN", 3, LEVEL),
    mi(223, 60117, "PRESET 1", "REAR MOD GAIN", 3, LEVEL),
    mi(224, 60118, "PRESET 1", "RPTT SELECT", 1, PORT_LINE),
    mi(225, 60201, "PRESET 2", "PRESET NAME", 12, Kind::Text { max: 12 }),
    mi(226, 60202, "PRESET 2", "CAT-1 RATE", 1, RATE),
    mi(227, 60203, "PRESET 2", "CAT-1 TIME OUT TIMER", 1, TOT),
    mi(228, 60204, "PRESET 2", "CAT-1 CAT-3 STOP BIT", 1, STOP_BIT),
    mi(229, 60205, "PRESET 2", "AGC FAST DELAY", 4, AGC_DELAY),
    mi(230, 60206, "PRESET 2", "AGC MID DELAY", 4, AGC_DELAY),
    mi(231, 60207, "PRESET 2", "AGC SLOW DELAY", 4, AGC_DELAY),
    mi(232, 60208, "PRESET 2", "LCUT FREQ", 2, LCUT),
    mi(233, 60209, "PRESET 2", "LCUT SLOPE", 1, SLOPE),
    mi(234, 60210, "PRESET 2", "HCUT FREQ", 2, HCUT),
    mi(235, 60211, "PRESET 2", "HCUT SLOPE", 1, SLOPE),
    mi(236, 60212, "PRESET 2", "USB OUT LEVEL", 3, LEVEL),
    mi(237, 60213, "PRESET 2", "REAR OUT LEVEL", 3, LEVEL),
    mi(238, 60214, "PRESET 2", "TX BPF SEL", 1, TX_BPF),
    mi(239, 60215, "PRESET 2", "MOD SOURCE", 1, MOD_SRC),
    mi(240, 60216, "PRESET 2", "USB MOD GAIN", 3, LEVEL),
    mi(241, 60217, "PRESET 2", "REAR MOD GAIN", 3, LEVEL),
    mi(242, 60218, "PRESET 2", "RPTT SELECT", 1, PORT_LINE),
    mi(243, 60301, "PRESET 3", "PRESET NAME", 12, Kind::Text { max: 12 }),
    mi(244, 60302, "PRESET 3", "CAT-1 RATE", 1, RATE),
    mi(245, 60303, "PRESET 3", "CAT-1 TIME OUT TIMER", 1, TOT),
    mi(246, 60304, "PRESET 3", "CAT-1 CAT-3 STOP BIT", 1, STOP_BIT),
    mi(247, 60305, "PRESET 3", "AGC FAST DELAY", 4, AGC_DELAY),
    mi(248, 60306, "PRESET 3", "AGC MID DELAY", 4, AGC_DELAY),
    mi(249, 60307, "PRESET 3", "AGC SLOW DELAY", 4, AGC_DELAY),
    mi(250, 60308, "PRESET 3", "LCUT FREQ", 2, LCUT),
    mi(251, 60309, "PRESET 3", "LCUT SLOPE", 1, SLOPE),
    mi(252, 60310, "PRESET 3", "HCUT FREQ", 2, HCUT),
    mi(253, 60311, "PRESET 3", "HCUT SLOPE", 1, SLOPE),
    mi(254, 60312, "PRESET 3", "USB OUT LEVEL", 3, LEVEL),
    mi(255, 60313, "PRESET 3", "REAR OUT LEVEL", 3, LEVEL),
    mi(256, 60314, "PRESET 3", "TX BPF SEL", 1, TX_BPF),
    mi(257, 60315, "PRESET 3", "MOD SOURCE", 1, MOD_SRC),
    mi(258, 60316, "PRESET 3", "USB MOD GAIN", 3, LEVEL),
    mi(259, 60317, "PRESET 3", "REAR MOD GAIN", 3, LEVEL),
    mi(260, 60318, "PRESET 3", "RPTT SELECT", 1, PORT_LINE),
    mi(261, 60401, "PRESET 4", "PRESET NAME", 12, Kind::Text { max: 12 }),
    mi(262, 60402, "PRESET 4", "CAT-1 RATE", 1, RATE),
    mi(263, 60403, "PRESET 4", "CAT-1 TIME OUT TIMER", 1, TOT),
    mi(264, 60404, "PRESET 4", "CAT-1 CAT-3 STOP BIT", 1, STOP_BIT),
    mi(265, 60405, "PRESET 4", "AGC FAST DELAY", 4, AGC_DELAY),
    mi(266, 60406, "PRESET 4", "AGC MID DELAY", 4, AGC_DELAY),
    mi(267, 60407, "PRESET 4", "AGC SLOW DELAY", 4, AGC_DELAY),
    mi(268, 60408, "PRESET 4", "LCUT FREQ", 2, LCUT),
    mi(269, 60409, "PRESET 4", "LCUT SLOPE", 1, SLOPE),
    mi(270, 60410, "PRESET 4", "HCUT FREQ", 2, HCUT),
    mi(271, 60411, "PRESET 4", "HCUT SLOPE", 1, SLOPE),
    mi(272, 60412, "PRESET 4", "USB OUT LEVEL", 3, LEVEL),
    mi(273, 60413, "PRESET 4", "REAR OUT LEVEL", 3, LEVEL),
    mi(274, 60414, "PRESET 4", "TX BPF SEL", 1, TX_BPF),
    mi(275, 60415, "PRESET 4", "MOD SOURCE", 1, MOD_SRC),
    mi(276, 60416, "PRESET 4", "USB MOD GAIN", 3, LEVEL),
    mi(277, 60417, "PRESET 4", "REAR MOD GAIN", 3, LEVEL),
    mi(278, 60418, "PRESET 4", "RPTT SELECT", 1, PORT_LINE),
    mi(279, 60501, "PRESET 5", "PRESET NAME", 12, Kind::Text { max: 12 }),
    mi(280, 60502, "PRESET 5", "CAT-1 RATE", 1, RATE),
    mi(281, 60503, "PRESET 5", "CAT-1 TIME OUT TIMER", 1, TOT),
    mi(282, 60504, "PRESET 5", "CAT-1 CAT-3 STOP BIT", 1, STOP_BIT),
    mi(283, 60505, "PRESET 5", "AGC FAST DELAY", 4, AGC_DELAY),
    mi(284, 60506, "PRESET 5", "AGC MID DELAY", 4, AGC_DELAY),
    mi(285, 60507, "PRESET 5", "AGC SLOW DELAY", 4, AGC_DELAY),
    mi(286, 60508, "PRESET 5", "LCUT FREQ", 2, LCUT),
    mi(287, 60509, "PRESET 5", "LCUT SLOPE", 1, SLOPE),
    mi(288, 60510, "PRESET 5", "HCUT FREQ", 2, HCUT),
    mi(289, 60511, "PRESET 5", "HCUT SLOPE", 1, SLOPE),
    mi(290, 60512, "PRESET 5", "USB OUT LEVEL", 3, LEVEL),
    mi(291, 60513, "PRESET 5", "REAR OUT LEVEL", 3, LEVEL),
    mi(292, 60514, "PRESET 5", "TX BPF SEL", 1, TX_BPF),
    mi(293, 60515, "PRESET 5", "MOD SOURCE", 1, MOD_SRC),
    mi(294, 60516, "PRESET 5", "USB MOD GAIN", 3, LEVEL),
    mi(295, 60517, "PRESET 5", "REAR MOD GAIN", 3, LEVEL),
    mi(296, 60518, "PRESET 5", "RPTT SELECT", 1, PORT_LINE),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ftdx101::tests::{d, round_trip, ua, up_prog};

    fn dd(f: &str) -> Vec<String> {
        d(&FT710, &SPEC, f)
    }

    fn uu(f: &str) -> Vec<String> {
        ua(&FT710, &SPEC, f)
    }

    fn prog(f: &str) -> Vec<String> {
        up_prog(&FT710, &SPEC, f)
    }

    #[test]
    fn menu_table() {
        assert_eq!(MENU.len(), 296);
        assert_eq!(FT710.menu_item(130).map(|m| (m.name, m.addr)), Some(("CAT-1 RATE", 30105)));
        assert_eq!(FT710.menu_item(109).unwrap().name, "PC KEYING");
        assert_eq!(FT710.menu_item(112).unwrap().name, "CW FREQ DISPLAY");
        assert_eq!(FT710.menu_item(75).map(|m| (m.name, m.group)), Some(("NAR WIDTH", "MODE PSK/DATA")));
        for i in [14, 33, 49, 71] {
            assert_eq!(FT710.menu_item(i).unwrap().name, "MOD SOURCE");
        }
        for &i in FT710.roles.cat_items {
            let n = FT710.menu_item(i).unwrap().name;
            assert!(n.starts_with("CAT-") || n.starts_with("TUN/LIN"), "{i} {n}");
        }
        assert_eq!(MENU.last().map(|m| (m.addr, m.name)), Some((60518, "RPTT SELECT")));
    }

    #[test]
    fn data_uses_the_cw_widths() {
        let s = crate::RadioState::for_model(&FT710);
        assert_eq!(s.width_class(crate::Mode::DataUsb), WidthClass::Rtty);
        assert_eq!(s.width_class(crate::Mode::Usb), WidthClass::Ssb);
    }

    #[test]
    fn clarifier_is_cf() {
        assert_eq!(dd("RT;"), ["CF000;"]);
        assert_eq!(dd("XT;"), ["CF000;"]);
        assert_eq!(dd("RT1;"), ["CF00010000;"]);
        assert_eq!(dd("RC;"), ["CF001+0000;"]);
        assert_eq!(dd("RU0150;"), ["CF001+0150;"]);
        assert_eq!(dd("RD0150;"), ["CF001-0150;"]);
        assert_eq!(uu("CF00011000;"), ["RT1;", "XT1;"]);
        assert!(uu("CF001+0150;").is_empty(), "the offset comes from IF");
        assert_eq!(prog("CF001+0150;"), ["RC;", "RU0150;"]);
        assert_eq!(prog("CF001-0020;"), ["RC;", "RD0020;"]);
        assert_eq!(prog("CF00010000;"), ["RT1;", "XT0;"]);
        // The offset steps from what IF said.
        let mut cx = Ctx::new(&FT710);
        down(&mut cx, "ID;");
        up(&mut cx, "IF001014074000+012010200000;");
        assert_eq!(down(&mut cx, "RU0100;"), ["CF001+0220;"]);
        assert_eq!(down(&mut cx, "RD0300;"), ["CF001-0080;"]);
        assert_eq!(down(&mut cx, "XT1;"), ["CF00011000;"]);
        // A program reading CF gets it from RigHelm's IF.
        let mut cx = Ctx::new(&FT710);
        assert_eq!(up(&mut cx, "CF000;"), ["IF;"]);
        assert_eq!(down(&mut cx, "IF00114074000+012010200000;"), ["CF00010000;"]);
        assert_eq!(up(&mut cx, "CF001;"), ["IF;"]);
        assert_eq!(down(&mut cx, "IF00114074000-012010200000;"), ["CF001-0120;"]);
        assert_eq!(down(&mut cx, "IF00114074000-012010200000;"), ["IF001014074000-012010200000;"]);
    }

    #[test]
    fn split_and_tx() {
        assert_eq!(dd("FT2;"), ["FT0;"]);
        assert_eq!(dd("FT3;"), ["FT1;"]);
        assert_eq!(dd("FT;"), ["FT;"]);
        assert_eq!(uu("FT1;"), ["FT1;"]);
        assert_eq!(prog("FT1;"), ["FT3;"]);
        assert_eq!(prog("FT0;"), ["FT2;"]);
        assert_eq!(prog("ST1;"), ["FT3;"]);
        assert_eq!(uu("ST1;"), ["FT1;"]);
    }

    #[test]
    fn radio_information() {
        assert_eq!(dd("RI0;"), ["RI0;"]);
        assert_eq!(dd("RI3;"), ["RI0;"]);
        assert_eq!(dd("BY;"), ["RI0;"]);
        assert!(dd("RI5;").is_empty());
        assert_eq!(uu("RI01200001;"), ["RI01;", "RI30;", "RI41;", "BY10;"]);
        assert_eq!(uu("RI00100000;"), ["RI00;", "RI31;", "RI40;", "BY00;"]);
        // A program asking RI0 gets every flag RigHelm knows.
        let mut cx = Ctx::new(&FT710);
        assert_eq!(up(&mut cx, "RI0;"), ["RI0;"]);
        assert_eq!(down(&mut cx, "RI01;"), ["RI01000000;"]);
        assert_eq!(down(&mut cx, "BY10;"), ["RI01000001;"]);
    }

    #[test]
    fn other_commands() {
        assert_eq!(dd("AC002;"), ["AC003;"]);
        assert_eq!(dd("AC001;"), ["AC001;"]);
        assert_eq!(uu("AC003;"), ["AC002;"]);
        assert!(uu("AC020;").is_empty(), "ATAS");
        assert_eq!(dd("KY1;"), ["KY01;"]);
        assert_eq!(dd("KY5;"), ["KY05;"]);
        assert_eq!(dd("KY6;"), ["KY11;"]);
        assert_eq!(dd("KYA;"), ["KY15;"]);
        assert_eq!(uu("KY15;"), ["KYA;"]);
        assert!(uu("KY00;").is_empty());
        assert_eq!(dd("LK5;"), ["LK1;"]);
        assert_eq!(uu("LK1;"), ["LK5;"]);
        assert_eq!(dd("MS5;"), ["MS50;"]);
        assert_eq!(dd("DA151515;"), ["DA00202020;"]);
        assert_eq!(dd("SH021;"), ["SH0021;"]);
        assert_eq!(dd("EX130;"), ["EX030105;"]);
        assert_eq!(uu("EX0301054;"), ["EX1304;"]);
        assert_eq!(dd("EX296;"), ["EX060518;"]);
        assert!(dd("RF01;").is_empty(), "no roofing filters");
        assert!(dd("FR4;").is_empty());
        assert!(dd("ED005;").is_empty());
        assert!(dd("LM01;").is_empty());
        assert_eq!(dd("TS1;"), ["TS1;"]);
        assert!(uu("VE1234;").is_empty());
        assert!(uu("GP0000;").is_empty());
    }

    #[test]
    fn round_trips() {
        let frames = [
            "FA14074000;", "FB70100000;", "MD0I;", "MD0J;", "MD07;", "IF00114074000+012010200000;",
            "OI10050313000-001000I10000;", "MR117;", "MC100;", "SH0;", "SH023;", "IS0-0500;", "CN049;", "CO0001;",
            "CO0002;", "CO0225;", "SD0100;", "SQ0255;", "DA151515;", "MS0;", "MS4;", "RM1100;", "RM8200;", "LK4;", "LK5;",
            // (RT/XT, RI and BY become frames that answer several of
            // RigHelm's commands at once; tested above.)
            "FT;", "FT0;", "FT1;", "AC;", "AC000;", "AC001;", "AC002;", "KY1;", "KY5;", "KY6;", "KYA;", "XT1;", "RI0;",
            "TS0;", "EX001;", "EX001-20;",
            "EX191W1AW        ;", "EX207PRESET 1     ;", "PC050;", "PL000;", "BS11;",
        ];
        round_trip(&FT710, &SPEC, &frames);
    }

    #[test]
    fn every_menu_item_maps_both_ways() {
        for item in FT710.menu {
            assert_eq!(d(&FT710, &SPEC, &format!("EX{:03};", item.num)), [format!("EX{:06};", item.addr)]);
            assert_eq!(ua(&FT710, &SPEC, &format!("EX{:06}0;", item.addr)), [format!("EX{:03}0;", item.num)]);
        }
    }
}
