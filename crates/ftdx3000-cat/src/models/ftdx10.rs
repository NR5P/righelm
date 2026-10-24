//! Yaesu FTDX10: the FTDX101's CAT generation (see [`super::ftdx101`])
//! without a SUB receiver: no `AN`, `FR`, `FS`, `UL` or VC tune, one meter
//! selector, a main-dial-only lock, its own width table and roofing
//! filters, and TXW (`TS`) as on the FTDX3000.

use super::ftdx101::{self as dx, Flavor, mi};
use super::ftdx101::{
    AFC_RANGE, AGC_DELAY, AMC_RELEASE, AM_CH_STEP, APF_WIDTH, BAUDOT, BK_IN_TYPE, BRIGHTNESS, CONTEST_NUMBER, CONTOUR_LEVEL,
    CONTOUR_WIDTH, CW_AUTO, CW_MEM, CW_WEIGHT, DATA_SHIFT, DECODE_BW, DIAL_STEP, DIDDLE, ENABLE, EQ_BWTH, EQ_LEVEL, EQ1_FREQ,
    EQ2_FREQ, EQ3_FREQ, FM_CH_STEP, FREQ_DISPLAY, HCUT, KEYBOARD, KEYER_TYPE, LCUT, LEVEL, MAIN_STEPS, MARK_FREQ, MOD_SRC,
    MPVD_STEPS, MY_CALL_TIME, NB_REJECTION, NB_WIDTH, NEW_LINE, NOR_REV, NOTCH_WIDTH, NUMBER_STYLE, OFF_ON, PC_KEYING, PIXEL,
    POWER_25W, POWER_50W, POWER_100W, PSK_MODE, PSK_TONE, QMB_CH, QSK_DELAY, QUICK_SPLIT, RATE, RBW, REAR_SEL, REF_FREQ_ADJ,
    REPEAT_INTERVAL, RF_SQL, RPT_SHIFT_28, RPT_SHIFT_50, RPTT, SCAN_RESUME, SCOPE_CTR, SCREEN_SAVER, SENSITIVITY, SHIFT_FREQ,
    SHIFT_PORT, SLOPE, TOT, TUNER_SELECT, TX_BPF, TX_TOT, VOX_SELECT, WAVE_SHAPE,
};
use crate::dialect::text::{Ctx, TextSpec, read_prefix};
use crate::menu::{Kind, MenuItem, signed};
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
    // MAIN, MPVD, MAIN NOTCH, MAIN CONTOUR (no MIC/SPEED or PROC encoders).
    encoders: &[(0, 0), (1, 1), (6, 4), (7, 5)],
    ft_select: false,
    clar_cf: false,
    single_lock: true,
    packed_ri: false,
    tune_start: '2',
    ky_split: false,
    vc_tune: false,
};

/// Table 3 (FTDX10, also the FT-710's): SSB 01-23, CW/RTTY/PSK 01-21.
pub(super) const SSB_10: [u32; 23] = [
    300, 400, 600, 850, 1100, 1200, 1500, 1650, 1800, 1950, 2100, 2250, 2400, 2450, 2500, 2600, 2700, 2800, 2900, 3000, 3200,
    3500, 4000,
];
pub(super) const CW_10: [u32; 21] =
    [50, 100, 150, 200, 250, 300, 350, 400, 450, 500, 600, 800, 1200, 1400, 1700, 2000, 2400, 3000, 3200, 3500, 4000];

fn widths(class: WidthClass, _narrow: bool) -> Vec<(u8, u32)> {
    match class {
        WidthClass::Ssb => dx::table(&SSB_10),
        WidthClass::Cw | WidthClass::Rtty => dx::table(&CW_10),
        WidthClass::Fixed => Vec::new(),
    }
}

/// 00-10 like the FTDX3000's, 11 GEN, 12 MW (no 70 MHz key).
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
    Band { code: 12, label: "MW", lo: 522_000, hi: 1_710_000 },
    Band { code: 11, label: "GEN", lo: 30_000, hi: 75_000_000 },
];

const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AM", "BA", "BC", "BD", "BI", "BP", "BS", "BU", "BY", "CH", "CN", "CO", "CS", "CT", "DA", "DN",
    "ED", "EM", "EN", "EU", "EX", "FA", "FB", "FT", "GT", "ID", "IF", "IS", "KM", "KP", "KR", "KS", "KY", "LK", "LM", "MA",
    "MC", "MD", "MG", "ML", "MR", "MS", "MW", "MX", "NA", "NB", "NL", "NR", "OI", "OS", "PA", "PB", "PC", "PL", "PR", "PS",
    "QI", "QR", "QS", "RA", "RC", "RD", "RF", "RG", "RI", "RL", "RM", "RS", "RT", "RU", "SC", "SD", "SH", "SM", "SQ", "SV",
    "TS", "TX", "UP", "VD", "VG", "VM", "VS", "VX", "XT",
];

/// Set codes 1, 2, 4, 5; the radio answers 6, 7, 9, A (Set codes listed
/// too in case it echoes them).
const ROOFING_SET: &[(char, &str)] = &[('1', "12 kHz"), ('2', "3 kHz"), ('4', "500 Hz"), ('5', "300 Hz")];
const ROOFING_ANSWERS: &[(char, &str)] =
    &[('6', "12 kHz"), ('7', "3 kHz"), ('9', "500 Hz"), ('A', "300 Hz"), ('1', "12 kHz"), ('2', "3 kHz"), ('4', "500 Hz"), ('5', "300 Hz")];

pub static FTDX10: RadioModel = RadioModel {
    key: "ftdx10",
    name: "FTDX10",
    maker: Maker::Yaesu,
    family: Family::Yaesu,
    proto: Proto::Text(&SPEC),
    id: "0761",
    civ_addr: 0,
    hamlib: 1042,
    freq_min: 30_000,
    freq_max: 75_000_000,
    bands: BANDS,
    modes: dx::MODES,
    menu: MENU,
    roles: Roles {
        // SSB/AM/FM/DATA MOD SOURCE: MIC or REAR (REAR SELECT then picks
        // DATA or USB).
        mic_sel: &[13, 29, 45, 69],
        mic_front: 0,
        mic_usb: 1,
        pc_keying: Some((115, 0, 2)),
        cat_tot: Some((138, "1")),
        cat_items: &[135, 136, 137, 138, 139],
        cw_bfo: None,
        cw_pitch_display: Some((114, "1")),
        data_other: None,
        data_disp: None,
    },
    caps: Caps {
        commands: COMMANDS,
        widths,
        narrow: false,
        preamp: &[(0, "IPO"), (1, "AMP1"), (2, "AMP2")],
        att: &[(0, "OFF"), (1, "6"), (2, "12"), (3, "18")],
        agc: types::AGC_SET,
        roofing: ROOFING_SET,
        roofing_answers: ROOFING_ANSWERS,
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
    bauds: &[38400, 19200, 9600, 4800],
    stop_bits: 2,
    rts_cts: true,
    sim: &["MS2;", "SH013;"],
};

// ------------------------------------------------------------- menu

pub(super) const AF_TONE: Kind = signed(-10, 10, 1, "");
pub(super) const RPT: Kind = Kind::Choice(&["-", "SIMPLEX", "+"]);
pub(super) const ENC_DEC: Kind = Kind::Choice(&["OFF", "ENC", "TSQ"]);
pub(super) const SPLIT_DISPLAY: Kind = Kind::Choice(&["FREQ", "DELTA"]);
pub(super) const CH_STEP: Kind = Kind::Choice(&["1 kHz", "2.5 kHz", "5 kHz", "10 kHz"]);

/// Table 2.
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
    mi(11, 10111, "MODE SSB", "SSB OUT LEVEL", 3, LEVEL),
    mi(12, 10112, "MODE SSB", "TX BPF SEL", 1, TX_BPF),
    mi(13, 10113, "MODE SSB", "SSB MOD SOURCE", 1, MOD_SRC),
    mi(14, 10114, "MODE SSB", "REAR SELECT", 1, REAR_SEL),
    mi(15, 10115, "MODE SSB", "RPORT GAIN", 3, LEVEL),
    mi(16, 10116, "MODE SSB", "RPTT SELECT", 1, RPTT),
    mi(17, 10201, "MODE AM", "AF TREBLE GAIN", 3, AF_TONE),
    mi(18, 10202, "MODE AM", "AF MIDDLE TONE GAIN", 3, AF_TONE),
    mi(19, 10203, "MODE AM", "AF BASS GAIN", 3, AF_TONE),
    mi(20, 10204, "MODE AM", "AGC FAST DELAY", 4, AGC_DELAY),
    mi(21, 10205, "MODE AM", "AGC MID DELAY", 4, AGC_DELAY),
    mi(22, 10206, "MODE AM", "AGC SLOW DELAY", 4, AGC_DELAY),
    mi(23, 10207, "MODE AM", "LCUT FREQ", 2, LCUT),
    mi(24, 10208, "MODE AM", "LCUT SLOPE", 1, SLOPE),
    mi(25, 10209, "MODE AM", "HCUT FREQ", 2, HCUT),
    mi(26, 10210, "MODE AM", "HCUT SLOPE", 1, SLOPE),
    mi(27, 10211, "MODE AM", "AM OUT LEVEL", 3, LEVEL),
    mi(28, 10212, "MODE AM", "TX BPF SEL", 1, TX_BPF),
    mi(29, 10213, "MODE AM", "AM MOD SOURCE", 1, MOD_SRC),
    mi(30, 10214, "MODE AM", "MIC GAIN", 4, Kind::MicGain),
    mi(31, 10215, "MODE AM", "REAR SELECT", 1, REAR_SEL),
    mi(32, 10216, "MODE AM", "RPORT GAIN", 3, LEVEL),
    mi(33, 10217, "MODE AM", "RPTT SELECT", 1, RPTT),
    mi(34, 10301, "MODE FM", "AF TREBLE GAIN", 3, AF_TONE),
    mi(35, 10302, "MODE FM", "AF MIDDLE TONE GAIN", 3, AF_TONE),
    mi(36, 10303, "MODE FM", "AF BASS GAIN", 3, AF_TONE),
    mi(37, 10304, "MODE FM", "AGC FAST DELAY", 4, AGC_DELAY),
    mi(38, 10305, "MODE FM", "AGC MID DELAY", 4, AGC_DELAY),
    mi(39, 10306, "MODE FM", "AGC SLOW DELAY", 4, AGC_DELAY),
    mi(40, 10307, "MODE FM", "LCUT FREQ", 2, LCUT),
    mi(41, 10308, "MODE FM", "LCUT SLOPE", 1, SLOPE),
    mi(42, 10309, "MODE FM", "HCUT FREQ", 2, HCUT),
    mi(43, 10310, "MODE FM", "HCUT SLOPE", 1, SLOPE),
    mi(44, 10311, "MODE FM", "FM OUT LEVEL", 3, LEVEL),
    mi(45, 10312, "MODE FM", "FM MOD SOURCE", 1, MOD_SRC),
    mi(46, 10313, "MODE FM", "MIC GAIN", 4, Kind::MicGain),
    mi(47, 10314, "MODE FM", "REAR SELECT", 1, REAR_SEL),
    mi(48, 10315, "MODE FM", "RPORT GAIN", 3, LEVEL),
    mi(49, 10316, "MODE FM", "RPTT SELECT", 1, RPTT),
    mi(50, 10317, "MODE FM", "RPT SHIFT (28 MHz)", 4, RPT_SHIFT_28),
    mi(51, 10318, "MODE FM", "RPT SHIFT (50 MHz)", 4, RPT_SHIFT_50),
    mi(52, 10319, "MODE FM", "RPT", 1, RPT),
    mi(53, 10320, "MODE FM", "TONE FREQ", 2, Kind::Tone),
    mi(54, 10321, "MODE FM", "ENC/DEC", 1, ENC_DEC),
    mi(55, 10401, "MODE PSK/DATA", "AF TREBLE GAIN", 3, AF_TONE),
    mi(56, 10402, "MODE PSK/DATA", "AF MIDDLE TONE GAIN", 3, AF_TONE),
    mi(57, 10403, "MODE PSK/DATA", "AF BASS GAIN", 3, AF_TONE),
    mi(58, 10404, "MODE PSK/DATA", "AGC FAST DELAY", 4, AGC_DELAY),
    mi(59, 10405, "MODE PSK/DATA", "AGC MID DELAY", 4, AGC_DELAY),
    mi(60, 10406, "MODE PSK/DATA", "AGC SLOW DELAY", 4, AGC_DELAY),
    mi(61, 10407, "MODE PSK/DATA", "PSK TONE", 1, PSK_TONE),
    mi(62, 10408, "MODE PSK/DATA", "DATA SHIFT (SSB)", 4, DATA_SHIFT),
    mi(63, 10409, "MODE PSK/DATA", "LCUT FREQ", 2, LCUT),
    mi(64, 10410, "MODE PSK/DATA", "LCUT SLOPE", 1, SLOPE),
    mi(65, 10411, "MODE PSK/DATA", "HCUT FREQ", 2, HCUT),
    mi(66, 10412, "MODE PSK/DATA", "HCUT SLOPE", 1, SLOPE),
    mi(67, 10413, "MODE PSK/DATA", "DATA OUT LEVEL", 3, LEVEL),
    mi(68, 10414, "MODE PSK/DATA", "TX BPF SEL", 1, TX_BPF),
    mi(69, 10415, "MODE PSK/DATA", "DATA MOD SOURCE", 1, MOD_SRC),
    mi(70, 10416, "MODE PSK/DATA", "REAR SELECT", 1, REAR_SEL),
    mi(71, 10417, "MODE PSK/DATA", "RPORT GAIN", 3, LEVEL),
    mi(72, 10418, "MODE PSK/DATA", "RPTT SELECT", 1, RPTT),
    mi(73, 10501, "MODE RTTY", "AF TREBLE GAIN", 3, AF_TONE),
    mi(74, 10502, "MODE RTTY", "AF MIDDLE TONE GAIN", 3, AF_TONE),
    mi(75, 10503, "MODE RTTY", "AF BASS GAIN", 3, AF_TONE),
    mi(76, 10504, "MODE RTTY", "AGC FAST DELAY", 4, AGC_DELAY),
    mi(77, 10505, "MODE RTTY", "AGC MID DELAY", 4, AGC_DELAY),
    mi(78, 10506, "MODE RTTY", "AGC SLOW DELAY", 4, AGC_DELAY),
    mi(79, 10507, "MODE RTTY", "POLARITY-RX", 1, NOR_REV),
    mi(80, 10508, "MODE RTTY", "POLARITY-TX", 1, NOR_REV),
    mi(81, 10509, "MODE RTTY", "LCUT FREQ", 2, LCUT),
    mi(82, 10510, "MODE RTTY", "LCUT SLOPE", 1, SLOPE),
    mi(83, 10511, "MODE RTTY", "HCUT FREQ", 2, HCUT),
    mi(84, 10512, "MODE RTTY", "HCUT SLOPE", 1, SLOPE),
    mi(85, 10513, "MODE RTTY", "RTTY OUT LEVEL", 3, LEVEL),
    mi(86, 10514, "MODE RTTY", "SHIFT PORT", 1, SHIFT_PORT),
    mi(87, 10515, "MODE RTTY", "MARK FREQUENCY", 1, MARK_FREQ),
    mi(88, 10516, "MODE RTTY", "SHIFT FREQUENCY", 1, SHIFT_FREQ),
    mi(89, 10601, "ENC/DEC PSK", "PSK MODE", 1, PSK_MODE),
    mi(90, 10602, "ENC/DEC PSK", "DECODE AFC RANGE", 1, AFC_RANGE),
    mi(91, 10603, "ENC/DEC PSK", "QPSK POLARITY RX", 1, NOR_REV),
    mi(92, 10604, "ENC/DEC PSK", "QPSK POLARITY TX", 1, NOR_REV),
    mi(93, 10605, "ENC/DEC PSK", "PSK TX LEVEL", 3, LEVEL),
    mi(94, 10701, "ENC/DEC RTTY", "RX USOS", 1, ENABLE),
    mi(95, 10702, "ENC/DEC RTTY", "TX USOS", 1, ENABLE),
    mi(96, 10703, "ENC/DEC RTTY", "RX NEW LINE CODE", 1, NEW_LINE),
    mi(97, 10704, "ENC/DEC RTTY", "TX AUTO CR+LF", 1, ENABLE),
    mi(98, 10705, "ENC/DEC RTTY", "TX DIDDLE", 1, DIDDLE),
    mi(99, 10706, "ENC/DEC RTTY", "BAUDOT CODE", 1, BAUDOT),
    mi(100, 20101, "MODE CW", "AF TREBLE GAIN", 3, AF_TONE),
    mi(101, 20102, "MODE CW", "AF MIDDLE TONE GAIN", 3, AF_TONE),
    mi(102, 20103, "MODE CW", "AF BASS GAIN", 3, AF_TONE),
    mi(103, 20104, "MODE CW", "AGC FAST DELAY", 4, AGC_DELAY),
    mi(104, 20105, "MODE CW", "AGC MID DELAY", 4, AGC_DELAY),
    mi(105, 20106, "MODE CW", "AGC SLOW DELAY", 4, AGC_DELAY),
    mi(106, 20107, "MODE CW", "LCUT FREQ", 2, LCUT),
    mi(107, 20108, "MODE CW", "LCUT SLOPE", 1, SLOPE),
    mi(108, 20109, "MODE CW", "HCUT FREQ", 2, HCUT),
    mi(109, 20110, "MODE CW", "HCUT SLOPE", 1, SLOPE),
    mi(110, 20111, "MODE CW", "CW OUT LEVEL", 3, LEVEL),
    mi(111, 20112, "MODE CW", "CW AUTO MODE", 1, CW_AUTO),
    mi(112, 20113, "MODE CW", "CW BK-IN TYPE", 1, BK_IN_TYPE),
    mi(113, 20114, "MODE CW", "CW WAVE SHAPE", 1, WAVE_SHAPE),
    mi(114, 20115, "MODE CW", "CW FREQ DISPLAY", 1, FREQ_DISPLAY),
    mi(115, 20116, "MODE CW", "PC KEYING", 1, PC_KEYING),
    mi(116, 20117, "MODE CW", "QSK DELAY TIME", 1, QSK_DELAY),
    mi(117, 20118, "MODE CW", "CW INDICATOR", 1, OFF_ON),
    mi(118, 20201, "KEYER", "KEYER TYPE", 1, KEYER_TYPE),
    mi(119, 20202, "KEYER", "KEYER DOT/DASH", 1, NOR_REV),
    mi(120, 20203, "KEYER", "CW WEIGHT", 2, CW_WEIGHT),
    mi(121, 20204, "KEYER", "NUMBER STYLE", 1, NUMBER_STYLE),
    mi(122, 20205, "KEYER", "CONTEST NUMBER", 4, CONTEST_NUMBER),
    mi(123, 20206, "KEYER", "CW MEMORY 1", 1, CW_MEM),
    mi(124, 20207, "KEYER", "CW MEMORY 2", 1, CW_MEM),
    mi(125, 20208, "KEYER", "CW MEMORY 3", 1, CW_MEM),
    mi(126, 20209, "KEYER", "CW MEMORY 4", 1, CW_MEM),
    mi(127, 20210, "KEYER", "CW MEMORY 5", 1, CW_MEM),
    mi(128, 20211, "KEYER", "REPEAT INTERVAL", 2, REPEAT_INTERVAL),
    mi(129, 20301, "DECODE CW", "CW DECODE BW", 1, DECODE_BW),
    mi(130, 30101, "GENERAL", "NB WIDTH", 1, NB_WIDTH),
    mi(131, 30102, "GENERAL", "NB REJECTION", 1, NB_REJECTION),
    mi(132, 30103, "GENERAL", "BEEP LEVEL", 3, LEVEL),
    mi(133, 30104, "GENERAL", "RF/SQL VR", 1, RF_SQL),
    mi(134, 30105, "GENERAL", "TUNER SELECT", 1, TUNER_SELECT),
    mi(135, 30106, "GENERAL", "232C RATE", 1, RATE),
    mi(136, 30107, "GENERAL", "232C TIME OUT TIMER", 1, TOT),
    mi(137, 30108, "GENERAL", "CAT RATE", 1, RATE),
    mi(138, 30109, "GENERAL", "CAT TIME OUT TIMER", 1, TOT),
    mi(139, 30110, "GENERAL", "CAT RTS", 1, ENABLE),
    mi(140, 30111, "GENERAL", "QMB CH", 1, QMB_CH),
    mi(141, 30112, "GENERAL", "MEM GROUP", 1, ENABLE),
    mi(142, 30113, "GENERAL", "QUICK SPLIT INPUT", 1, ENABLE),
    mi(143, 30114, "GENERAL", "QUICK SPLIT FREQ", 3, QUICK_SPLIT),
    mi(144, 30115, "GENERAL", "SPLIT FREQ DISPLAY", 1, SPLIT_DISPLAY),
    mi(145, 30116, "GENERAL", "TX TIME OUT TIMER", 2, TX_TOT),
    mi(146, 30117, "GENERAL", "MIC SCAN", 1, ENABLE),
    mi(147, 30118, "GENERAL", "MIC SCAN RESUME", 1, SCAN_RESUME),
    mi(148, 30119, "GENERAL", "REF FREQ ADJ", 3, REF_FREQ_ADJ),
    mi(149, 30120, "GENERAL", "KEYBOARD LANGUAGE", 2, KEYBOARD),
    mi(150, 30201, "RX DSP", "APF WIDTH", 1, APF_WIDTH),
    mi(151, 30202, "RX DSP", "CONTOUR LEVEL", 3, CONTOUR_LEVEL),
    mi(152, 30203, "RX DSP", "CONTOUR WIDTH", 2, CONTOUR_WIDTH),
    mi(153, 30204, "RX DSP", "IF NOTCH WIDTH", 1, NOTCH_WIDTH),
    mi(154, 30301, "TX AUDIO", "AMC RELEASE TIME", 1, AMC_RELEASE),
    mi(155, 30302, "TX AUDIO", "PRMTRC EQ1 FREQ", 2, EQ1_FREQ),
    mi(156, 30303, "TX AUDIO", "PRMTRC EQ1 LEVEL", 3, EQ_LEVEL),
    mi(157, 30304, "TX AUDIO", "PRMTRC EQ1 BWTH", 2, EQ_BWTH),
    mi(158, 30305, "TX AUDIO", "PRMTRC EQ2 FREQ", 2, EQ2_FREQ),
    mi(159, 30306, "TX AUDIO", "PRMTRC EQ2 LEVEL", 3, EQ_LEVEL),
    mi(160, 30307, "TX AUDIO", "PRMTRC EQ2 BWTH", 2, EQ_BWTH),
    mi(161, 30308, "TX AUDIO", "PRMTRC EQ3 FREQ", 2, EQ3_FREQ),
    mi(162, 30309, "TX AUDIO", "PRMTRC EQ3 LEVEL", 3, EQ_LEVEL),
    mi(163, 30310, "TX AUDIO", "PRMTRC EQ3 BWTH", 2, EQ_BWTH),
    mi(164, 30311, "TX AUDIO", "P PRMTRC EQ1 FREQ", 2, EQ1_FREQ),
    mi(165, 30312, "TX AUDIO", "P PRMTRC EQ1 LEVEL", 3, EQ_LEVEL),
    mi(166, 30313, "TX AUDIO", "P PRMTRC EQ1 BWTH", 2, EQ_BWTH),
    mi(167, 30314, "TX AUDIO", "P PRMTRC EQ2 FREQ", 2, EQ2_FREQ),
    mi(168, 30315, "TX AUDIO", "P PRMTRC EQ2 LEVEL", 3, EQ_LEVEL),
    mi(169, 30316, "TX AUDIO", "P PRMTRC EQ2 BWTH", 2, EQ_BWTH),
    mi(170, 30317, "TX AUDIO", "P PRMTRC EQ3 FREQ", 2, EQ3_FREQ),
    mi(171, 30318, "TX AUDIO", "P PRMTRC EQ3 LEVEL", 3, EQ_LEVEL),
    mi(172, 30319, "TX AUDIO", "P PRMTRC EQ3 BWTH", 2, EQ_BWTH),
    mi(173, 30401, "TX GENERAL", "HF MAX POWER", 3, POWER_100W),
    mi(174, 30402, "TX GENERAL", "50M MAX POWER", 3, POWER_100W),
    mi(175, 30403, "TX GENERAL", "70M MAX POWER", 3, POWER_50W),
    mi(176, 30404, "TX GENERAL", "AM MAX POWER", 3, POWER_25W),
    mi(177, 30405, "TX GENERAL", "VOX SELECT", 1, VOX_SELECT),
    mi(178, 30406, "TX GENERAL", "DATA VOX GAIN", 3, LEVEL),
    mi(179, 30407, "TX GENERAL", "EMERGENCY FREQ TX", 1, ENABLE),
    mi(180, 30501, "TUNING", "SSB/CW DIAL STEP", 1, DIAL_STEP),
    mi(181, 30502, "TUNING", "RTTY/PSK DIAL STEP", 1, DIAL_STEP),
    mi(182, 30503, "TUNING", "CH STEP", 1, CH_STEP),
    mi(183, 30504, "TUNING", "AM CH STEP", 1, AM_CH_STEP),
    mi(184, 30505, "TUNING", "FM CH STEP", 1, FM_CH_STEP),
    mi(185, 30506, "TUNING", "MAIN STEPS PER REV.", 1, MAIN_STEPS),
    mi(186, 30507, "TUNING", "MPVD STEPS PER REV.", 1, MPVD_STEPS),
    mi(187, 40101, "DISPLAY", "MY CALL", 12, Kind::Text { max: 12 }),
    mi(188, 40102, "DISPLAY", "MY CALL TIME", 1, MY_CALL_TIME),
    mi(189, 40103, "DISPLAY", "SCREEN SAVER", 1, SCREEN_SAVER),
    mi(190, 40104, "DISPLAY", "DIMMER LED", 2, BRIGHTNESS),
    mi(191, 40105, "DISPLAY", "MOUSE POINTER SPEED", 2, BRIGHTNESS),
    mi(192, 40201, "SCOPE", "RBW", 1, RBW),
    mi(193, 40202, "SCOPE", "SCOPE CTR", 1, SCOPE_CTR),
    mi(194, 40203, "SCOPE", "2D DISP SENSITIVITY", 1, SENSITIVITY),
    mi(195, 40204, "SCOPE", "3DSS DISP SENSITIVITY", 1, SENSITIVITY),
    mi(196, 40301, "EXT MONITOR", "EXT DISPLAY", 1, ENABLE),
    mi(197, 40302, "EXT MONITOR", "PIXEL", 1, PIXEL),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ftdx101::tests::{d, round_trip, ua, up_prog};

    fn dd(f: &str) -> Vec<String> {
        d(&FTDX10, &SPEC, f)
    }

    fn uu(f: &str) -> Vec<String> {
        ua(&FTDX10, &SPEC, f)
    }

    #[test]
    fn menu_table() {
        assert_eq!(MENU.len(), 197);
        assert_eq!(FTDX10.menu_item(137).map(|m| (m.name, m.addr)), Some(("CAT RATE", 30108)));
        assert_eq!(FTDX10.menu_item(115).unwrap().name, "PC KEYING");
        assert_eq!(FTDX10.menu_item(114).unwrap().name, "CW FREQ DISPLAY");
        assert_eq!(FTDX10.menu_item(138).unwrap().name, "CAT TIME OUT TIMER");
        for i in [13, 29, 45, 69] {
            assert!(FTDX10.menu_item(i).unwrap().name.ends_with("MOD SOURCE"));
        }
        assert_eq!(MENU.last().unwrap().addr, 40302);
    }

    #[test]
    fn manual_frames() {
        assert_eq!(dd("FA14250000;"), ["FA014250000;"]);
        assert_eq!(dd("EX001;"), ["EX010101;"]);
        assert_eq!(dd("EX053;"), ["EX010320;"]);
        assert_eq!(uu("EX01032012;"), ["EX05312;"]);
        assert_eq!(dd("MS2;"), ["MS20;"]);
        assert_eq!(uu("MS50;"), ["MS5;"]);
        assert_eq!(dd("DA101015;"), ["DA00132013;"]);
        assert_eq!(uu("DA00132013;"), ["DA101015;"]);
        assert_eq!(dd("LK1;"), ["LK1;"]);
        assert_eq!(dd("LK5;"), ["LK1;"]);
        assert!(dd("LK2;").is_empty(), "no VFO-B lock");
        assert_eq!(uu("LK1;"), ["LK5;"]);
        assert_eq!(uu("LK0;"), ["LK4;"]);
        assert_eq!(dd("ED605;"), ["ED405;"]);
        assert!(dd("ED405;").is_empty(), "no MIC/SPEED encoder");
        assert_eq!(dd("SH023;"), ["SH0023;"]);
        assert_eq!(uu("RF09;"), ["RF09;"]);
        assert_eq!(dd("TS1;"), ["TS1;"]);
        assert_eq!(dd("RT1;"), ["RT1;"]);
        assert!(dd("FR4;").is_empty());
        assert!(dd("AN01;").is_empty());
        assert!(dd("VF0;").is_empty());
        assert!(uu("VT001281;").is_empty());
        assert_eq!(up_prog(&FTDX10, &SPEC, "ST1;"), ["FT3;"]);
        assert_eq!(uu("ST1;"), ["FT1;"]);
        assert_eq!(dd("MD0I;"), ["MD0E;"]);
        assert_eq!(dd("IF00114074000+012010200000;"), ["IF001014074000+012010200000;"]);
    }

    #[test]
    fn round_trips() {
        let frames = [
            "FA14074000;", "FB50313000;", "MD0I;", "MD0J;", "IF00114074000+012010200000;", "OI10050313000-001000I10000;",
            "MR001;", "MC100;", "SH0;", "SH023;", "IS0+1000;", "CN012;", "CO0001;", "CO0002;", "CO0132;", "CO0225;",
            "SD0050;", "SQ0255;", "DA151515;", "DA000700;", "MS0;", "MS5;", "RM5161;", "LK4;", "LK5;", "FT2;", "FT3;",
            "FT;", "EX053;", "EX05349;", "EX187W1AW        ;", "RF01;", "RF0A;", "ED705;", "EU001;", "TS1;", "RI00;",
            "RI41;", "BY10;", "RT1;", "XT1;", "RC;", "RU0100;", "BS12;",
        ];
        round_trip(&FTDX10, &SPEC, &frames);
    }

    #[test]
    fn every_menu_item_maps_both_ways() {
        for item in FTDX10.menu {
            assert_eq!(d(&FTDX10, &SPEC, &format!("EX{:03};", item.num)), [format!("EX{:06};", item.addr)]);
            assert_eq!(ua(&FTDX10, &SPEC, &format!("EX{:06}0;", item.addr)), [format!("EX{:03}0;", item.num)]);
        }
    }
}
