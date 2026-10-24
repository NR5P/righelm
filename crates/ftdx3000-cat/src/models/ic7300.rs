//! Icom IC-7300 (being written: no menu yet).

use crate::dialect::icom::{self, CivSpec};
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

/// RigHelm commands an Icom HF radio carries out.
pub const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AM", "BA", "BC", "BD", "BI", "BP00", "BS", "BU", "BY", "CO00", "CO02", "EX", "FA", "FB", "FR", "FT", "GT", "ID", "IF",
    "KM", "KP", "KS", "KY", "LK", "MA", "MC", "MD", "MG", "ML0", "ML1", "MR", "MW", "MX", "NB", "NL", "NR", "OI", "PA", "PB", "PC", "PL", "PR0",
    "PS", "RA", "RC", "RD", "RG", "RL", "RM", "RT", "RU", "SH", "SM", "SQ", "SV", "TX", "VG", "VS", "VX", "XT",
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
    transceive: None,
    keyer_memories: 8,
    voice_memories: 8,
    antenna_switch: false,
    apf: true,
    lock: true,
    memory_name_len: 10,
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
    modes: &[
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
    ],
    menu: &[],
    roles: Roles::NONE,
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
    bauds: &[115200, 19200, 9600, 4800],
    stop_bits: 1,
    rts_cts: false,
    sim: &["SH028;", "GT02;"],
};

pub static IC7300MK2: RadioModel = RadioModel { key: "ic7300mk2", name: "IC-7300MK2", civ_addr: 0xB6, ..IC7300 };
