//! Value types and lookup tables used by the FTDX3000 CAT commands.

/// Operating mode as used by `MD`, `IF`, `OI`, `MR` and `MW`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Lsb,
    Usb,
    Cw,
    Fm,
    Am,
    RttyLsb,
    CwR,
    DataLsb,
    RttyUsb,
    DataFm,
    FmN,
    DataUsb,
    AmN,
}

impl Mode {
    pub const ALL: [Mode; 13] = [
        Mode::Lsb,
        Mode::Usb,
        Mode::Cw,
        Mode::CwR,
        Mode::Am,
        Mode::AmN,
        Mode::Fm,
        Mode::FmN,
        Mode::RttyLsb,
        Mode::RttyUsb,
        Mode::DataLsb,
        Mode::DataUsb,
        Mode::DataFm,
    ];

    pub fn from_code(c: char) -> Option<Mode> {
        Some(match c.to_ascii_uppercase() {
            '1' => Mode::Lsb,
            '2' => Mode::Usb,
            '3' => Mode::Cw,
            '4' => Mode::Fm,
            '5' => Mode::Am,
            '6' => Mode::RttyLsb,
            '7' => Mode::CwR,
            '8' => Mode::DataLsb,
            '9' => Mode::RttyUsb,
            'A' => Mode::DataFm,
            'B' => Mode::FmN,
            'C' => Mode::DataUsb,
            'D' => Mode::AmN,
            _ => return None,
        })
    }

    pub fn code(self) -> char {
        match self {
            Mode::Lsb => '1',
            Mode::Usb => '2',
            Mode::Cw => '3',
            Mode::Fm => '4',
            Mode::Am => '5',
            Mode::RttyLsb => '6',
            Mode::CwR => '7',
            Mode::DataLsb => '8',
            Mode::RttyUsb => '9',
            Mode::DataFm => 'A',
            Mode::FmN => 'B',
            Mode::DataUsb => 'C',
            Mode::AmN => 'D',
        }
    }

    /// Label as printed on the radio's front panel.
    pub fn label(self) -> &'static str {
        match self {
            Mode::Lsb => "LSB",
            Mode::Usb => "USB",
            Mode::Cw => "CW",
            Mode::CwR => "CW-R",
            Mode::Am => "AM",
            Mode::AmN => "AM-N",
            Mode::Fm => "FM",
            Mode::FmN => "FM-N",
            Mode::RttyLsb => "RTTY-L",
            Mode::RttyUsb => "RTTY-U",
            Mode::DataLsb => "DATA-L",
            Mode::DataUsb => "DATA-U",
            Mode::DataFm => "DATA-FM",
        }
    }

    /// Hamlib mode name (rigctl protocol).
    pub fn hamlib(self) -> &'static str {
        match self {
            Mode::Lsb => "LSB",
            Mode::Usb => "USB",
            Mode::Cw => "CW",
            Mode::CwR => "CWR",
            Mode::Am => "AM",
            Mode::AmN => "AMN",
            Mode::Fm => "FM",
            Mode::FmN => "FMN",
            Mode::RttyLsb => "RTTY",
            Mode::RttyUsb => "RTTYR",
            Mode::DataLsb => "PKTLSB",
            Mode::DataUsb => "PKTUSB",
            Mode::DataFm => "PKTFM",
        }
    }

    pub fn from_hamlib(s: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.hamlib().eq_ignore_ascii_case(s))
    }

    /// Which column of the width table (Table 5) applies.
    pub fn width_class(self) -> WidthClass {
        match self {
            Mode::Lsb | Mode::Usb | Mode::DataLsb | Mode::DataUsb => WidthClass::Ssb,
            Mode::Cw | Mode::CwR => WidthClass::Cw,
            Mode::RttyLsb | Mode::RttyUsb => WidthClass::Rtty,
            Mode::Am | Mode::AmN | Mode::Fm | Mode::FmN | Mode::DataFm => WidthClass::Fixed,
        }
    }

    pub fn is_cw(self) -> bool {
        matches!(self, Mode::Cw | Mode::CwR)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidthClass {
    Ssb,
    Cw,
    Rtty,
    /// AM/FM: no `SH` width; only narrow on/off.
    Fixed,
}

/// Table 5: `SH` index -> bandwidth in Hz, for a given mode class and `NA` state.
/// Index 0 is the default width for the narrow/wide setting.
pub fn width_hz(class: WidthClass, narrow: bool, index: u8) -> Option<u32> {
    const SSB_N: [u32; 10] = [1500, 200, 400, 600, 850, 1100, 1350, 1500, 1650, 1800];
    const SSB_W: [u32; 26] = [
        2400, 0, 0, 0, 0, 0, 0, 0, 0, 1800, 1950, 2100, 2200, 2300, 2400, 2500, 2600, 2700, 2800,
        2900, 3000, 3200, 3400, 3600, 3800, 4000,
    ];
    const CW_N: [u32; 11] = [500, 50, 100, 150, 200, 250, 300, 350, 400, 450, 500];
    const CW_W: [u32; 17] = [2400, 0, 0, 0, 0, 0, 0, 0, 0, 0, 500, 800, 1200, 1400, 1700, 2000, 2400];
    let table: &[u32] = match (class, narrow) {
        (WidthClass::Ssb, true) => &SSB_N,
        (WidthClass::Ssb, false) => &SSB_W,
        (WidthClass::Cw | WidthClass::Rtty, true) => &CW_N,
        (WidthClass::Cw | WidthClass::Rtty, false) => &CW_W,
        (WidthClass::Fixed, _) => return None,
    };
    table.get(index as usize).copied().filter(|&w| w != 0)
}

/// All valid `SH` indexes for a mode class and narrow setting, with widths.
pub fn width_choices(class: WidthClass, narrow: bool) -> Vec<(u8, u32)> {
    (0..=25).filter_map(|i| width_hz(class, narrow, i).map(|w| (i, w))).collect()
}

/// Band select codes for `BS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Band {
    pub code: u8,
    pub label: &'static str,
    /// Frequency range in Hz, used to tell which band a frequency is in.
    pub lo: u64,
    pub hi: u64,
}

pub const BANDS: &[Band] = &[
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
    Band { code: 11, label: "GEN", lo: 30_000, hi: 60_000_000 },
];

pub fn band_of(freq: u64) -> Option<&'static Band> {
    BANDS.iter().filter(|b| b.code != 11).find(|b| (b.lo..=b.hi).contains(&freq))
}

/// FTDX3000 tunable range (Hz).
pub const FREQ_MIN: u64 = 30_000;
pub const FREQ_MAX: u64 = 60_000_000;

/// Table 1: CTCSS tone number -> tone in tenths of Hz.
pub const CTCSS_DHZ: [u16; 50] = [
    670, 693, 719, 744, 770, 797, 825, 854, 885, 915, 948, 974, 1000, 1035, 1072, 1109, 1148, 1188,
    1230, 1273, 1318, 1365, 1413, 1462, 1514, 1567, 1598, 1622, 1655, 1679, 1713, 1738, 1773, 1799,
    1835, 1862, 1899, 1928, 1966, 1995, 2035, 2065, 2107, 2181, 2257, 2291, 2336, 2418, 2503, 2541,
];

/// Roofing filter as reported by `RF0x;` answers.
pub fn roofing_label(answer_code: char) -> Option<&'static str> {
    Some(match answer_code.to_ascii_uppercase() {
        '1' => "15 kHz",
        '2' => "6 kHz",
        '3' => "3 kHz",
        '4' => "AUTO 15 kHz",
        '5' => "AUTO 6 kHz",
        '6' => "AUTO 3 kHz",
        '7' => "600 Hz",
        '8' => "300 Hz",
        '9' => "AUTO 600 Hz",
        'A' => "AUTO 300 Hz",
        _ => return None,
    })
}

/// Roofing filter Set codes (`RF0x;`).
pub const ROOFING_SET: &[(char, &str)] =
    &[('0', "AUTO"), ('1', "15 kHz"), ('2', "6 kHz"), ('3', "3 kHz"), ('4', "600 Hz"), ('5', "300 Hz")];

pub fn roofing_is_auto(answer_code: char) -> bool {
    matches!(answer_code.to_ascii_uppercase(), '4' | '5' | '6' | '9' | 'A')
}

/// AGC as reported by `GT0x;` answers (0-6). Set codes are 0-4.
pub fn agc_label(code: u8) -> &'static str {
    match code {
        0 => "OFF",
        1 => "FAST",
        2 => "MID",
        3 => "SLOW",
        4 => "AUTO-F",
        5 => "AUTO-M",
        6 => "AUTO-S",
        _ => "?",
    }
}

pub const AGC_SET: &[(u8, &str)] = &[(0, "OFF"), (1, "FAST"), (2, "MID"), (3, "SLOW"), (4, "AUTO")];

/// Meters readable with `RMx;`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Meter {
    S,
    Comp,
    Alc,
    Po,
    Swr,
    Id,
    Vdd,
}

impl Meter {
    pub const TX: [Meter; 6] = [Meter::Po, Meter::Swr, Meter::Alc, Meter::Comp, Meter::Id, Meter::Vdd];

    pub fn rm_code(self) -> char {
        match self {
            Meter::S => '1',
            Meter::Comp => '3',
            Meter::Alc => '4',
            Meter::Po => '5',
            Meter::Swr => '6',
            Meter::Id => '7',
            Meter::Vdd => '8',
        }
    }

    pub fn from_rm_code(c: char) -> Option<Meter> {
        Some(match c {
            '1' => Meter::S,
            '3' => Meter::Comp,
            '4' => Meter::Alc,
            '5' => Meter::Po,
            '6' => Meter::Swr,
            '7' => Meter::Id,
            '8' => Meter::Vdd,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Meter::S => "S",
            Meter::Comp => "COMP",
            Meter::Alc => "ALC",
            Meter::Po => "PO",
            Meter::Swr => "SWR",
            Meter::Id => "ID",
            Meter::Vdd => "VDD",
        }
    }

    /// `MS` front-panel meter selection code, for meters that have one.
    pub fn ms_code(self) -> Option<char> {
        match self {
            Meter::Comp => Some('0'),
            Meter::Alc => Some('1'),
            Meter::Swr => Some('3'),
            Meter::Id => Some('4'),
            Meter::Vdd => Some('5'),
            _ => None,
        }
    }
}

/// Piecewise-linear interpolation through calibration points.
pub fn interpolate(table: &[(u8, f32)], raw: u8) -> f32 {
    if table.is_empty() {
        return raw as f32;
    }
    if raw <= table[0].0 {
        return table[0].1;
    }
    for w in table.windows(2) {
        let (r0, v0) = w[0];
        let (r1, v1) = w[1];
        if raw <= r1 {
            let t = (raw - r0) as f32 / (r1 - r0).max(1) as f32;
            return v0 + t * (v1 - v0);
        }
    }
    table[table.len() - 1].1
}

/// S-meter raw (0-255) -> dB relative to S9 (Hamlib FTDX5000/3000 calibration).
pub const S_METER_CAL: [(u8, f32); 16] = [
    (0, -54.0),
    (12, -48.0),
    (27, -42.0),
    (40, -36.0),
    (55, -30.0),
    (65, -24.0),
    (80, -18.0),
    (95, -12.0),
    (112, -6.0),
    (130, 0.0),
    (150, 10.0),
    (172, 20.0),
    (190, 30.0),
    (220, 40.0),
    (240, 50.0),
    (255, 60.0),
];

/// PO raw -> watts (FLRig calibration for the FTDX3000/5000 family).
pub const PO_CAL: [(u8, f32); 15] = [
    (0, 0.0),
    (55, 10.0),
    (75, 20.0),
    (101, 40.0),
    (125, 60.0),
    (144, 80.0),
    (161, 100.0),
    (177, 120.0),
    (190, 140.0),
    (202, 160.0),
    (215, 180.0),
    (225, 200.0),
    (237, 220.0),
    (242, 240.0),
    (255, 250.0),
];

/// SWR raw -> ratio. Approximate; refined against the real radio later.
pub const SWR_CAL: [(u8, f32); 8] =
    [(0, 1.0), (26, 1.2), (52, 1.5), (89, 2.0), (126, 3.0), (173, 4.0), (236, 5.0), (255, 10.0)];

/// Drain current raw -> amps. Approximate (full scale = 25 A).
pub const ID_CAL: [(u8, f32); 2] = [(0, 0.0), (255, 25.0)];

/// Supply voltage raw -> volts. Approximate (full scale = 15 V on the meter face).
pub const VDD_CAL: [(u8, f32); 2] = [(0, 0.0), (255, 15.0)];

/// Human-readable S-meter text ("S7", "S9+20").
pub fn s_meter_text(raw: u8) -> String {
    let db = interpolate(&S_METER_CAL, raw);
    if db <= 0.0 {
        let s = ((db + 54.0) / 6.0).round().clamp(0.0, 9.0) as i32;
        format!("S{s}")
    } else {
        format!("S9+{:.0}", (db / 10.0).round() * 10.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_round_trip() {
        for m in Mode::ALL {
            assert_eq!(Mode::from_code(m.code()), Some(m));
            assert_eq!(Mode::from_hamlib(m.hamlib()), Some(m));
        }
    }

    #[test]
    fn widths_match_table_5() {
        assert_eq!(width_hz(WidthClass::Ssb, false, 0), Some(2400));
        assert_eq!(width_hz(WidthClass::Ssb, false, 1), None);
        assert_eq!(width_hz(WidthClass::Ssb, false, 25), Some(4000));
        assert_eq!(width_hz(WidthClass::Ssb, true, 4), Some(850));
        assert_eq!(width_hz(WidthClass::Cw, true, 1), Some(50));
        assert_eq!(width_hz(WidthClass::Cw, false, 16), Some(2400));
        assert_eq!(width_hz(WidthClass::Fixed, false, 0), None);
    }

    #[test]
    fn meter_text() {
        assert_eq!(s_meter_text(0), "S0");
        assert_eq!(s_meter_text(130), "S9");
        assert_eq!(s_meter_text(172), "S9+20");
        assert!((interpolate(&PO_CAL, 161) - 100.0).abs() < 0.01);
    }

    #[test]
    fn band_lookup() {
        assert_eq!(band_of(14_074_000).map(|b| b.label), Some("20"));
        assert_eq!(band_of(9_000_000), None);
    }
}
