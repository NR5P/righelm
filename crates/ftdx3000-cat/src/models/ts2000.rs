//! Kenwood TS-2000 (and TS-2000X, TS-B2000), from the instruction manual's
//! "PC Control Command" tables (pages 113-141).
//!
//! The translator is the Kenwood one in [`super::ts480`]; the differences
//! are in [`KW`]. RigHelm's VFO-A and VFO-B are the main transceiver's VFO A
//! and VFO B (`FA`/`FB`, `FR`/`FT` for split), which is what HF split,
//! Hamlib and logging programs use. The sub-receiver (VHF/UHF only, its own
//! VFO `FC`, `SB`, `DC` for the control/TX band, satellite mode `SA`) has no
//! RigHelm counterpart: `AG`, `SQ`, `SM` and `BY` are the main receiver's,
//! the sub-receiver's answers are left out. The main transceiver covers HF,
//! 6 m, 2 m, 70 cm and (with the UT-20 option) 23 cm.
//!
//! Differences from the TS-480: RF gain 0-255, S-meter 0-30 dots (S9 at 15),
//! `RM` 0-30 answers only the meter selected (RigHelm `MS`), AGC is a time
//! constant 1-20 (FAST 5, MID 10, SLOW 20, as Hamlib), tones are the 38
//! standard ones numbered from 01, `IF` has a 5-digit RIT offset and the
//! repeater shift, `OS` repeater shift, CW pitch is menu 31, menus have
//! sub-items (`EX` P2 is the letter, A = 01: menu 06B is `EX0060200`), and
//! `TX;` keys the transmitter on the TX band (Hamlib sends the same); there
//! is no A=B or IF shift command (`IS` is the CW DSP shift).

use super::ts480::{self, HF_BANDS, Kw, MODES, OFF_9, OFF_ON, PITCH, WEIGHT, item, widths_from};
use crate::dialect::text::{Ctx, TextSpec, read_prefix};
use crate::menu::{Kind, MenuItem, num};
use crate::model::{Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::types::{Band, WidthClass};

pub static SPEC: TextSpec = TextSpec { down, up, answer_prefix: read_prefix };

fn down(cx: &mut Ctx, f: &str) -> Vec<String> {
    ts480::down_kw(&KW, cx, f)
}

fn up(cx: &mut Ctx, f: &str) -> Vec<String> {
    ts480::up_kw(&KW, cx, f)
}

/// The 38 tones of page 35, numbers 01-38 (`TN` 39 is 1750 Hz), 0.1 Hz.
const TONES_38: &[u16] = &[
    670, 719, 744, 770, 797, 825, 854, 885, 915, 948, 974, 1000, 1035, 1072, 1109, 1148, 1188, 1230, 1273, 1318, 1365,
    1413, 1462, 1514, 1567, 1622, 1679, 1738, 1799, 1862, 1928, 2035, 2107, 2181, 2257, 2336, 2418, 2503,
];

/// S-meter dots (0-30) -> dB relative to S9 (Hamlib's TS-2000 calibration).
const SM_CAL_30: &[(u8, f32)] = &[(0, -54.0), (3, -48.0), (6, -36.0), (9, -24.0), (12, -12.0), (15, 0.0), (30, 60.0)];

/// `SH` 00-11 in SSB/FM: the high cut, Hz.
const SSB_WIDTHS: &[u32] = &[1400, 1600, 1800, 2000, 2200, 2400, 2600, 2800, 3000, 3400, 4000, 5000];

static KW: Kw = Kw {
    tones: TONES_38,
    tone_first: 1,
    rg_max: 255,
    sm_cal: SM_CAL_30,
    sm_max: 30,
    rm_max: 30,
    po_watts: 100.0,
    if_p2: "0000",
    clar_digits: 5,
    if_shift: true,
    pitch_addr: 310000,
    gt: &[(0, 0), (1, 5), (2, 10), (3, 20)],
    ptt: "TX;",
    mox: "TX;",
    ssb_widths: SSB_WIDTHS,
    scan_from: 290,
    max_ch: 299,
    sc_answer_2: false,
    pb_answer_3: false,
    has_vv: false,
    has_is: false,
    has_os: true,
    rm_all: false,
};

fn widths(class: WidthClass, _narrow: bool) -> Vec<(u8, u32)> {
    widths_from(class, SSB_WIDTHS)
}

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
    Band { code: 12, label: "2", lo: 144_000_000, hi: 148_000_000 },
    Band { code: 13, label: "70", lo: 430_000_000, hi: 450_000_000 },
    Band { code: 14, label: "23", lo: 1_240_000_000, hi: 1_300_000_000 },
    Band { code: 11, label: "GEN", lo: 30_000, hi: 60_000_000 },
];

const COMMANDS: &[&str] = &[
    "AC", "AG", "AI", "AN", "BC", "BD", "BI", "BU", "BY", "CH", "CN", "CT", "DN", "EX", "FA", "FB", "FR", "FT", "GT",
    "ID", "IF", "KP", "KS", "KY", "LK", "MC", "MD", "MG", "ML", "MR", "MS", "MW", "MX", "NB", "NL", "NR", "OI", "OS",
    "PA", "PB", "PC", "PL", "PR", "PS", "QI", "RA", "RC", "RD", "RG", "RL", "RM", "RT", "RU", "SC", "SD", "SH", "SM",
    "SQ", "TS", "TX", "UL", "UP", "VD", "VG", "VX", "XT",
];

pub static TS2000: RadioModel = RadioModel {
    key: "ts2000",
    name: "TS-2000",
    maker: Maker::Kenwood,
    family: Family::Kenwood,
    proto: Proto::Text(&SPEC),
    id: "019",
    civ_addr: 0,
    hamlib: 2014,
    freq_min: 30_000,
    freq_max: 1_300_000_000,
    bands: BANDS,
    modes: MODES,
    menu: MENU,
    roles: Roles {
        mic_sel: &[],
        mic_front: 0,
        mic_usb: 0,
        pc_keying: None,
        cat_tot: None,
        // 55 packet communication mode (the TNC takes the COM port), 56 COM speed.
        cat_items: &[75, 76],
        cw_bfo: None,
        cw_pitch_display: None,
        data_other: None,
        data_disp: None,
        data_psk: false,
    },
    caps: Caps {
        commands: COMMANDS,
        widths,
        narrow: false,
        preamp: &[(0, "OFF"), (1, "ON")],
        att: &[(0, "OFF"), (1, "ON")],
        agc: &[(0, "OFF"), (1, "FAST"), (2, "MID"), (3, "SLOW")],
        roofing: &[],
        roofing_answers: &[],
        nb: &[(0, "OFF"), (1, "ON")],
        nr_max: 9,
        nb_level_max: 10,
        antennas: 2,
        power: (5, 100),
        meter_select: &[(1, "SWR"), (2, "COMP"), (3, "ALC")],
        voice_memories: 3,
        keyer_memories: 3,
        keyer_memory_len: 0,
        memory: Memory { first: 0, last: 299, pms: None },
        shared_clarifier: true,
    },
    bauds: &[57600, 38400, 19200, 9600, 4800],
    stop_bits: 1,
    rts_cts: true,
    sim: &[],
};

// ------------------------------------------------------------ the menu

/// Menu `n` with sub-item `letter` (0 none, 1 = A): RigHelm number `num`,
/// address `nnn` `ll` 0 0.
const fn x(num: u16, n: u32, letter: u32, group: &'static str, name: &'static str, width: u8, kind: Kind) -> MenuItem {
    item(num, n * 10000 + letter * 100, group, name, width, kind)
}

const OPS: &str = "OPERATOR INTERFACE";
const TUNE: &str = "TUNING CONTROL";
const MEM: &str = "MEMORY CHANNEL";
const SCAN: &str = "SCAN";
const SOUND: &str = "MONITOR SOUND";
const SPK: &str = "SPEAKER / RX ANT";
const SQL: &str = "S-METER SQUELCH";
const DSP: &str = "DSP";
const TXG: &str = "TX";
const AT: &str = "ANTENNA TUNER";
const LIN: &str = "LINEAR AMPLIFIER";
const MSG: &str = "MESSAGE PLAYBACK";
const CW: &str = "CW";
const FSK: &str = "FSK";
const FM: &str = "FM";
const DTMF: &str = "DTMF";
const TNC: &str = "TNC";
const PF: &str = "PF KEYS";
const GEN: &str = "GENERAL";
const RC: &str = "RC-2000";
const RPT: &str = "REPEATER (K)";
const SKY: &str = "SKY COMMAND II+ (K)";

const DELAY: Kind = Kind::Choice(&["OFF", "1", "2"]);
const EQ: Kind = Kind::Choice(&["OFF", "H BOOST", "F PASS", "B BOOST", "CONVEN", "USER"]);
const PFK: Kind = num(0, 99, 1, "");
const MAIN_SUB: Kind = Kind::Choice(&["MAIN", "SUB"]);
const SPEED: Kind = Kind::Choice(&["1200", "9600"]);
const LEVEL9: Kind = num(0, 9, 1, "");
const TONE: Kind = Kind::Coded(&[
    (1, "67.0 Hz"),
    (2, "71.9 Hz"),
    (3, "74.4 Hz"),
    (4, "77.0 Hz"),
    (5, "79.7 Hz"),
    (6, "82.5 Hz"),
    (7, "85.4 Hz"),
    (8, "88.5 Hz"),
    (9, "91.5 Hz"),
    (10, "94.8 Hz"),
    (11, "97.4 Hz"),
    (12, "100.0 Hz"),
    (13, "103.5 Hz"),
    (14, "107.2 Hz"),
    (15, "110.9 Hz"),
    (16, "114.8 Hz"),
    (17, "118.8 Hz"),
    (18, "123.0 Hz"),
    (19, "127.3 Hz"),
    (20, "131.8 Hz"),
    (21, "136.5 Hz"),
    (22, "141.3 Hz"),
    (23, "146.2 Hz"),
    (24, "151.4 Hz"),
    (25, "156.7 Hz"),
    (26, "162.2 Hz"),
    (27, "167.9 Hz"),
    (28, "173.8 Hz"),
    (29, "179.9 Hz"),
    (30, "186.2 Hz"),
    (31, "192.8 Hz"),
    (32, "203.5 Hz"),
    (33, "210.7 Hz"),
    (34, "218.1 Hz"),
    (35, "225.7 Hz"),
    (36, "233.6 Hz"),
    (37, "241.8 Hz"),
    (38, "250.3 Hz"),
]);

/// All menu items but 45A (DTMF number memory: channel and code are in P3
/// and P4, not a setting); 90 items.
pub const MENU: &[MenuItem] = &[
    x(1, 0, 0, OPS, "DISPLAY BRIGHTNESS", 1, Kind::Choice(&["OFF", "1", "2", "3", "4"])),
    x(2, 1, 0, OPS, "KEY ILLUMINATION", 1, OFF_ON),
    x(3, 2, 0, TUNE, "TUNING CONTROL PER REVOLUTION", 1, Kind::Choice(&["500", "1000"])),
    x(4, 3, 0, TUNE, "TUNING WITH MULTI/CH CONTROL", 1, OFF_ON),
    x(5, 4, 0, TUNE, "MULTI/CH ROUNDS OFF FREQUENCY", 1, OFF_ON),
    x(6, 5, 0, TUNE, "9 kHz MULTI/CH STEP ON AM BC BAND", 1, OFF_ON),
    x(7, 6, 1, MEM, "MEMORY-VFO SPLIT", 1, OFF_ON),
    x(8, 6, 2, MEM, "TUNABLE MEMORY CHANNELS", 1, OFF_ON),
    x(9, 7, 0, SCAN, "PROGRAM SCAN PARTIALLY SLOWED", 1, OFF_ON),
    x(10, 8, 0, SCAN, "PROGRAM SCAN SLOW-DOWN RANGE", 1, Kind::Steps { max_code: 4, off_at_zero: false, first: 100, step: 100, unit: "Hz" }),
    x(11, 9, 0, SCAN, "PROGRAM SCAN HOLD", 1, OFF_ON),
    x(12, 10, 0, SCAN, "SCAN RESUME", 1, Kind::Choice(&["TO", "CO"])),
    x(13, 11, 0, SCAN, "VISUAL SCAN RANGE", 1, Kind::Choice(&["31 ch", "61 ch", "91 ch", "181 ch"])),
    x(14, 12, 0, SOUND, "BEEP LEVEL", 1, OFF_9),
    x(15, 13, 0, SOUND, "TX SIDETONE VOLUME", 1, OFF_9),
    x(16, 14, 0, SOUND, "DRU-3A PLAYBACK VOLUME", 1, OFF_9),
    x(17, 15, 0, SOUND, "VS-3 PLAYBACK VOLUME", 1, OFF_9),
    x(18, 16, 0, SPK, "EXT.SP2 / HEADPHONE OUTPUT", 1, Kind::Choice(&["0: MIX/MIX", "1: MAIN/SUB", "2: MAIN+SUB/4"])),
    x(19, 17, 0, SPK, "REVERSE SP1/SP2", 1, OFF_ON),
    x(20, 18, 0, SPK, "HF RX ANT INPUT", 1, OFF_ON),
    x(21, 19, 1, SQL, "S-METER SQUELCH", 1, OFF_ON),
    x(22, 19, 2, SQL, "S-METER SQUELCH HANG TIME", 1, Kind::Choice(&["OFF", "125 ms", "250 ms", "500 ms"])),
    x(23, 20, 0, DSP, "DSP RX EQUALIZER", 1, EQ),
    x(24, 21, 0, DSP, "DSP TX EQUALIZER", 1, EQ),
    x(25, 22, 0, DSP, "DSP TX BANDWIDTH SSB/AM", 1, Kind::Choice(&["2.0 kHz", "2.2 kHz", "2.4 kHz", "2.6 kHz", "2.8 kHz", "3.0 kHz"])),
    x(26, 23, 0, TXG, "FINE TX POWER", 1, OFF_ON),
    x(27, 24, 0, TXG, "TIME-OUT TIMER", 1, Kind::Choice(&["OFF", "3 min", "5 min", "10 min", "20 min", "30 min"])),
    x(28, 25, 0, TXG, "TRANSVERTER DISPLAY", 1, OFF_ON),
    x(29, 26, 0, AT, "TX HOLD AFTER AT TUNING", 1, OFF_ON),
    x(30, 27, 0, AT, "IN-LINE AT WHILE RECEIVING", 1, OFF_ON),
    x(31, 28, 1, LIN, "LINEAR DELAY HF", 1, DELAY),
    x(32, 28, 2, LIN, "LINEAR DELAY 50 MHz", 1, DELAY),
    x(33, 28, 3, LIN, "LINEAR DELAY 144 MHz", 1, DELAY),
    x(34, 28, 4, LIN, "LINEAR DELAY 430 MHz", 1, DELAY),
    x(35, 28, 5, LIN, "LINEAR DELAY 1.2 GHz", 1, DELAY),
    x(36, 29, 1, MSG, "REPEAT PLAYBACK", 1, OFF_ON),
    x(37, 29, 2, MSG, "PLAYBACK REPEAT INTERVAL", 2, num(0, 60, 1, "s")),
    x(38, 30, 0, MSG, "KEYING PRIORITY OVER PLAYBACK", 1, OFF_ON),
    x(39, 31, 0, CW, "CW PITCH / SIDETONE", 2, PITCH),
    x(40, 32, 0, CW, "CW RISE TIME", 1, Kind::Choice(&["1 ms", "2 ms", "4 ms", "6 ms"])),
    x(41, 33, 0, CW, "CW WEIGHT", 2, WEIGHT),
    x(42, 34, 0, CW, "REVERSE AUTO WEIGHT", 1, OFF_ON),
    x(43, 35, 0, CW, "BUG KEY", 1, OFF_ON),
    x(44, 36, 0, CW, "AUTO CW TX IN SSB", 1, OFF_ON),
    x(45, 37, 0, CW, "SSB TO CW FREQUENCY CORRECTION", 1, OFF_ON),
    x(46, 38, 0, FSK, "FSK SHIFT", 1, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz", "850 Hz"])),
    x(47, 39, 0, FSK, "FSK KEYING POLARITY", 1, Kind::Choice(&["NORMAL", "INVERSE"])),
    x(48, 40, 0, FSK, "FSK TONE", 1, Kind::Choice(&["1275 Hz", "2125 Hz"])),
    x(49, 41, 0, FM, "FM MIC GAIN", 1, Kind::Choice(&["LOW", "MID", "HIGH"])),
    x(50, 42, 0, FM, "SUB-TONE MODE", 1, Kind::Choice(&["BURST", "CONT"])),
    x(51, 43, 0, FM, "AUTO REPEATER OFFSET", 1, OFF_ON),
    x(52, 44, 0, FM, "TX HOLD 1750 Hz TONE", 1, OFF_ON),
    x(53, 45, 2, DTMF, "STORED DTMF TX SPEED", 1, Kind::Choice(&["SLOW", "FAST"])),
    x(54, 45, 3, DTMF, "STORED DTMF PAUSE", 1, Kind::Choice(&["100 ms", "250 ms", "500 ms", "750 ms", "1000 ms", "1500 ms", "2000 ms"])),
    x(55, 45, 4, DTMF, "MIC REMOTE CONTROL", 1, OFF_ON),
    x(56, 46, 0, TNC, "INTERNAL TNC BAND", 1, MAIN_SUB),
    x(57, 47, 0, TNC, "INTERNAL TNC SPEED", 1, SPEED),
    x(58, 48, 0, TNC, "DCD SENSING BAND", 1, Kind::Choice(&["TNC BAND", "MAIN & SUB"])),
    x(59, 49, 1, TNC, "PACKET CLUSTER TUNE MODE", 1, Kind::Choice(&["AUTO", "MANUAL"])),
    x(60, 49, 2, TNC, "PACKET CLUSTER RX TONE", 1, Kind::Choice(&["OFF", "MORSE", "VOICE"])),
    x(61, 50, 1, TNC, "PACKET FILTER BANDWIDTH", 1, OFF_ON),
    x(62, 50, 2, TNC, "PACKET AF INPUT LEVEL", 1, LEVEL9),
    x(63, 50, 3, TNC, "PACKET MAIN AF OUTPUT LEVEL", 1, LEVEL9),
    x(64, 50, 4, TNC, "PACKET SUB AF OUTPUT LEVEL", 1, LEVEL9),
    x(65, 50, 5, TNC, "EXTERNAL TNC BAND", 1, MAIN_SUB),
    x(66, 50, 6, TNC, "EXTERNAL TNC SPEED", 1, SPEED),
    x(67, 51, 1, PF, "FRONT PANEL PF KEY", 2, PFK),
    x(68, 51, 2, PF, "MIC PF1 (CALL) KEY", 2, PFK),
    x(69, 51, 3, PF, "MIC PF2 (VFO) KEY", 2, PFK),
    x(70, 51, 4, PF, "MIC PF3 (MR) KEY", 2, PFK),
    x(71, 51, 5, PF, "MIC PF4 (PF) KEY", 2, PFK),
    x(72, 52, 0, GEN, "SPLIT TRANSFER (MASTER/SLAVE)", 1, OFF_ON),
    x(73, 53, 0, GEN, "WRITE TRANSFERRED SPLIT TO VFOS", 1, OFF_ON),
    x(74, 54, 0, GEN, "TX INHIBIT", 1, OFF_ON),
    x(75, 55, 0, GEN, "PACKET COMMUNICATION MODE", 1, OFF_ON),
    x(76, 56, 0, GEN, "COM PORT SPEED", 1, Kind::Choice(&["4800", "9600", "19200", "38400", "57600"])),
    x(77, 57, 0, GEN, "AUTO POWER OFF", 1, Kind::Choice(&["OFF", "60 min", "120 min", "180 min"])),
    x(78, 58, 0, RC, "RC-2000 FONT", 1, Kind::Choice(&["FONT1", "FONT2"])),
    x(79, 59, 0, RC, "DISPLAY CONTRAST", 2, Kind::Steps { max_code: 15, off_at_zero: false, first: 1, step: 1, unit: "" }),
    x(80, 60, 0, RC, "RC-2000 DISPLAY MODE", 1, Kind::Choice(&["NEGATIVE", "POSITIVE"])),
    x(81, 61, 1, RPT, "REPEATER MODE", 1, Kind::Choice(&["OFF", "LOCKED", "CROSS"])),
    x(82, 61, 2, RPT, "REPEATER TX HOLD", 1, OFF_ON),
    x(83, 61, 3, RPT, "REMOTE CONTROL ID CODE", 3, num(0, 999, 1, "")),
    x(84, 61, 4, RPT, "REMOTE ACKNOWLEDGEMENT", 1, OFF_ON),
    x(85, 61, 5, RPT, "EXTERNAL REMOTE CONTROL", 1, OFF_ON),
    x(86, 62, 1, SKY, "COMMANDER CALLSIGN", 8, Kind::Text { max: 8 }),
    x(87, 62, 2, SKY, "TRANSPORTER CALLSIGN", 8, Kind::Text { max: 8 }),
    x(88, 62, 3, SKY, "SKY COMMAND TONE", 2, TONE),
    x(89, 62, 4, SKY, "SKY COMMAND SPEED", 1, SPEED),
    x(90, 62, 5, SKY, "SKY COMMAND MODE", 1, Kind::Choice(&["OFF", "CLIENT", "COMMANDER", "TRANSPORTER"])),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn cx() -> Ctx {
        Ctx::new(&TS2000)
    }

    /// A TS-2000 `IF` answer from its fields.
    #[allow(clippy::too_many_arguments)]
    fn kif(freq: &str, clar: &str, rit_xit: &str, ch: &str, tx: &str, mode: &str, func: &str, split: &str, tone: &str, tn: &str, shift: &str) -> String {
        format!("IF{freq}0000{clar}{rit_xit}{ch}{tx}{mode}{func}0{split}{tone}{tn}{shift};")
    }

    #[test]
    fn menu_items() {
        assert_eq!(MENU.len(), 90);
        let m = |n: u16| TS2000.menu_item(n).unwrap();
        assert_eq!(m(75).name, "PACKET COMMUNICATION MODE");
        assert_eq!(m(76).name, "COM PORT SPEED");
        assert_eq!(m(76).addr, 560000);
        assert_eq!(m(8).addr, 60200);
        for r in TS2000.roles.cat_items {
            assert!(matches!(m(*r).name, "PACKET COMMUNICATION MODE" | "COM PORT SPEED"), "{r}");
        }
    }

    #[test]
    fn frames_from_the_manual() {
        let mut c = cx();
        assert_eq!(down(&mut c, "FA144195000;"), vec!["FA00144195000;"]);
        assert_eq!(up(&mut c, "FA00007000000;"), vec!["FA07000000;"]);
        assert_eq!(up(&mut c, "FB00432100000;")[0], "FB432100000;");
        assert_eq!(down(&mut c, "MD07;"), vec!["MD7;"]);
        // Menu 00 (display brightness), 06B, 31 CW pitch (600 Hz = 04, as Hamlib).
        assert_eq!(down(&mut c, "EX0013;"), vec!["EX00000003;"]);
        assert_eq!(up(&mut c, "EX00000000;"), vec!["EX0010;"]);
        assert_eq!(down(&mut c, "EX008;"), vec!["EX0060200;"]);
        assert_eq!(down(&mut c, "KP30;"), vec!["EX031000004;"]);
        assert_eq!(up(&mut c, "EX031000004;"), vec!["EX03904;", "KP30;"]);
        // AGC: Hamlib's FAST 005, MEDIUM 010, SLOW 020.
        assert_eq!(down(&mut c, "GT01;"), vec!["GT005;"]);
        assert_eq!(down(&mut c, "GT02;"), vec!["GT010;"]);
        assert_eq!(down(&mut c, "GT03;"), vec!["GT020;"]);
        assert_eq!(up(&mut c, "GT001;"), vec!["GT01;"]);
        assert_eq!(up(&mut c, "GT016;"), vec!["GT03;"]);
        assert_eq!(up(&mut c, "GT000;"), vec!["GT00;"]);
        // RF gain 0-255, no scaling.
        assert_eq!(down(&mut c, "RG0200;"), vec!["RG200;"]);
        assert_eq!(up(&mut c, "RG127;"), vec!["RG0127;"]);
        // PTT on the TX band.
        assert_eq!(down(&mut c, "TX1;"), vec!["TX;"]);
        assert_eq!(down(&mut c, "TX0;"), vec!["RX;"]);
        assert_eq!(up(&mut c, "TX0;"), vec!["TX1;"]);
        assert_eq!(up(&mut c, "RX0;"), vec!["TX0;"]);
        // Tones: 88.5 Hz is FTDX3000 08, TS-2000 08; 69.3 (FTDX3000 01) -> 67.0 (01).
        assert_eq!(down(&mut c, "CN008;"), vec!["TN08;", "CN08;"]);
        assert_eq!(down(&mut c, "CN001;"), vec!["TN01;", "CN01;"]);
        assert_eq!(up(&mut c, "CN32;"), vec!["CN040;"]);
        // Repeater shift; the sub-receiver's AF gain and S-meter are not ours.
        assert_eq!(down(&mut c, "OS02;"), vec!["OS2;"]);
        assert_eq!(up(&mut c, "OS3;"), vec!["OS02;"]);
        assert!(up(&mut c, "AG1100;").is_empty());
        assert!(up(&mut c, "SM10010;").is_empty());
        assert!(down(&mut c, "AB;").is_empty());
        // Memory channels 000-299 (bank + 2 digits).
        assert_eq!(down(&mut c, "MC250;"), vec!["MC250;"]);
        assert_eq!(up(&mut c, "MC 05;"), vec!["MC005;"]);
        assert_eq!(down(&mut c, "MR123;"), vec!["MR0123;"]);
    }

    #[test]
    fn if_and_meters() {
        let mut c = cx();
        // 145.000 MHz FM, RIT -00120, CTCSS 100.0 Hz (TS-2000 12), shift minus.
        let f = kif("00145000000", "-00120", "10", "000", "0", "4", "0", "0", "2", "12", "2");
        assert_eq!(f.len(), 38);
        let out = up(&mut c, &f);
        assert_eq!(out[0], "IF000145000000-012010401122;");
        assert!(out.contains(&"CT01;".to_string()));
        let mut p = cx();
        p.remember("dir", "emu");
        assert_eq!(down(&mut p, "IF000145000000-012010401122;"), vec![f]);
        // S-meter 0-30, S9 at 15.
        assert_eq!(up(&mut c, "SM00015;")[0], "SM0130;");
        assert_eq!(up(&mut c, "SM00030;")[0], "SM0255;");
        // RM answers only the selected meter.
        let rm = up(&mut c, "RM30015;");
        assert_eq!(rm[0], "MS3;");
        assert!(rm.contains(&"RM4128;".to_string()));
        assert_eq!(down(&mut c, "MS1;"), vec!["RM1;"]);
        assert_eq!(down(&mut c, "MS;"), vec!["RM;"]);
        // Widths: SSB SH 00-11.
        assert_eq!(down(&mut c, "SH005;"), vec!["SH05;"]);
        assert_eq!(TS2000.width_hz(WidthClass::Ssb, false, 5), Some(2400));
        assert_eq!(TS2000.width_hz(WidthClass::Cw, false, 7), Some(500));
    }

    #[test]
    fn round_trips() {
        for f in [
            "FA14074000;", "FB144174000;", "FA432100000;", "MD01;", "MD02;", "MD03;", "MD04;", "MD05;", "MD06;", "MD07;",
            "MD09;", "AG0100;", "SQ0010;", "RG0200;", "PC050;", "MG030;", "KS025;", "PA01;", "RA01;", "NB01;", "NL0005;",
            "NR01;", "RL005;", "BC01;", "PR01;", "VX1;", "VG100;", "VD0300;", "SD0050;", "GT00;", "GT01;", "GT02;",
            "GT03;", "RT1;", "XT1;", "FT1;", "FR4;", "AN02;", "AC001;", "MC250;", "EX0013;", "EX03906;", "EX0763;",
            "EX08612;", "EX083123;", "LK7;", "PB02;", "AI1;", "PS1;", "TS1;", "CN008;", "OS01;", "SC1;", "KP40;",
            "ML1050;", "PL050;", "MS2;",
        ] {
            let mut c = cx();
            let native = down(&mut c, f);
            assert!(!native.is_empty(), "{f}");
            let back: Vec<String> = native.iter().flat_map(|n| up(&mut c, n)).collect();
            assert!(back.contains(&f.to_string()), "{f} -> {native:?} -> {back:?}");
        }
    }
}
