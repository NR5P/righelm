//! A typed model of the radio, updated from Answer (or auto-information) frames.

use std::collections::{BTreeMap, BTreeSet};

use crate::command::answer_key;
use crate::frame;
use crate::model::RadioModel;
use crate::types::{Meter, Mode, WidthClass};

/// The information block shared by `IF`, `OI` and `MR` answers (and `MW` sets).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChannelInfo {
    /// Memory channel number (P1).
    pub channel: u16,
    pub freq: u64,
    /// Clarifier offset in Hz (signed).
    pub clar_offset: i32,
    pub rx_clar: bool,
    pub tx_clar: bool,
    pub mode: Option<Mode>,
    /// 0 VFO, 1 memory, 2 memory tune, 3 QMB, 4 QMB-MT.
    pub mem_mode: u8,
    /// 0 off, 1 CTCSS enc/dec, 2 CTCSS enc.
    pub ctcss: u8,
    pub tone: u8,
    /// 0 simplex, 1 plus, 2 minus.
    pub shift: u8,
}

impl ChannelInfo {
    /// Parse the 24 parameter characters of an `IF`/`OI`/`MR` answer (25 or
    /// 26 when the frequency has nine or ten digits).
    pub fn parse(p: &str) -> Option<ChannelInfo> {
        if p.len() < 24 || !p.is_ascii() {
            return None;
        }
        // The clarifier sign follows the frequency (8 digits, 9 above
        // 99.999999 MHz, 10 above 999.999999 MHz).
        let Some(f) = (8..=10).find(|f| matches!(p.get(3 + f..4 + f), Some("+" | "-"))) else { return None };
        if p.len() < 16 + f {
            return None;
        }
        let p = &p[..];
        let o = f - 8;
        let clar_sign = if &p[11 + o..12 + o] == "-" { -1 } else { 1 };
        Some(ChannelInfo {
            channel: p[0..3].parse().ok()?,
            freq: p[3..3 + f].parse().ok()?,
            clar_offset: clar_sign * p[12 + o..16 + o].parse::<i32>().ok()?,
            rx_clar: &p[16 + o..17 + o] == "1",
            tx_clar: &p[17 + o..18 + o] == "1",
            mode: Mode::from_code(p[18 + o..19 + o].chars().next()?),
            mem_mode: p[19 + o..20 + o].parse().unwrap_or(0),
            ctcss: p[20 + o..21 + o].parse().unwrap_or(0),
            tone: p[21 + o..23 + o].parse().unwrap_or(0),
            shift: p[23 + o..24 + o].parse().unwrap_or(0),
        })
    }

    /// Format as an `MW` (memory write) Set command for channel `ch`.
    pub fn to_mw(&self, ch: u16) -> String {
        let sign = if self.clar_offset < 0 { '-' } else { '+' };
        format!(
            "MW{ch:03}{:08}{sign}{:04}{}{}{}0{}{:02}{};",
            self.freq,
            self.clar_offset.unsigned_abs().min(9999),
            self.rx_clar as u8,
            self.tx_clar as u8,
            self.mode.unwrap_or(Mode::Usb).code(),
            self.ctcss,
            self.tone,
            self.shift
        )
    }

    /// Format as an answer body (for emulating the radio to other programs).
    pub fn to_answer(&self, cmd: &str) -> String {
        let sign = if self.clar_offset < 0 { '-' } else { '+' };
        format!(
            "{cmd}{:03}{:08}{sign}{:04}{}{}{}{}{}{:02}{};",
            self.channel,
            self.freq,
            self.clar_offset.unsigned_abs().min(9999),
            self.rx_clar as u8,
            self.tx_clar as u8,
            self.mode.unwrap_or(Mode::Usb).code(),
            self.mem_mode,
            self.ctcss,
            self.tone,
            self.shift
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct RadioState {
    /// The radio this is (FTDX3000 until another is known).
    pub model: &'static RadioModel,
    /// Answer keys seen at least once (e.g. "FA", "AG0", "EX038").
    pub seen: BTreeSet<String>,

    pub id: Option<u16>,
    pub power_on: Option<bool>,
    pub auto_info: bool,

    pub freq_a: u64,
    pub freq_b: u64,
    pub mode_a: Option<Mode>,
    pub mode_b: Option<Mode>,
    pub info_a: ChannelInfo,
    pub info_b: ChannelInfo,
    /// VS: 0 VFO-A, 1 VFO-B.
    pub vfo_select: u8,
    /// FR: 0 A rx, 1 A mute, 4 B rx, 5 B mute.
    pub rx_function: u8,
    /// FT answer: 0 VFO-A transmitter, 1 VFO-B transmitter.
    pub tx_vfo: u8,
    /// TX answer: 0 off, 1 CAT TX, 2 radio TX (PTT/MOX/VOX).
    pub tx: u8,
    pub mox: bool,
    pub busy: bool,

    pub af_gain: u8,
    pub rf_gain: u8,
    pub squelch: u8,
    pub power: u8,
    pub mic_gain: u8,
    pub proc_level: u8,
    /// PR0: speech processor on.
    pub processor: bool,
    /// PR1: parametric mic EQ on.
    pub mic_eq: bool,
    pub vox: bool,
    pub vox_gain: u8,
    pub vox_delay: u16,
    pub monitor: bool,
    pub monitor_level: u8,

    /// NB: 0 off, 1 on, 2 wide.
    pub nb: u8,
    pub nb_level: u8,
    pub nr: bool,
    pub nr_level: u8,
    pub auto_notch: bool,
    pub manual_notch: bool,
    /// Manual notch frequency in units of 10 Hz (1-400).
    pub notch_freq: u16,
    /// CO00: 0 off, 1 contour, 2 APF.
    pub contour_mode: u8,
    /// CO01: 1-40 (x100 Hz).
    pub contour_freq: u8,
    /// CO02: 0-50 (APF -250..+250 Hz in 10 Hz steps, 25 = centre).
    pub apf_freq: u8,
    pub width_index: u8,
    pub narrow: bool,
    pub if_shift: i32,
    /// RF answer code (see `types::roofing_label`).
    pub roofing: char,
    /// RA: 0 off, 1 6 dB, 2 12 dB, 3 18 dB.
    pub att: u8,
    /// PA: 0 IPO, 1 AMP1, 2 AMP2.
    pub preamp: u8,
    /// GT answer code 0-6.
    pub agc: u8,
    /// VF answer: on/off, value.
    pub vrf_on: bool,
    pub vrf_value: u16,

    pub antenna: u8,
    /// AC P3: 0 off, 1 on, 2 tuning.
    pub tuner: u8,

    pub keyer: bool,
    pub key_speed: u8,
    /// KP 0-75 -> 300 + 10*n Hz.
    pub key_pitch: u8,
    pub break_in: bool,
    pub bk_delay: u16,
    pub cw_spot: bool,

    pub rx_clar: bool,
    pub tx_clar: bool,

    /// LK answer 4-7.
    pub lock: u8,
    /// FS answer 4-7.
    pub fast: u8,
    pub txw: bool,
    pub scan: u8,
    pub sub_dial: u8,
    pub ctcss_mode: u8,
    pub ctcss_tone: u8,
    pub rpt_shift: u8,
    pub dimmer: (u8, u8, u8),
    pub memory_channel: u16,
    pub menu_mode: bool,
    pub pll_unlock: bool,
    pub meter_sw: u8,
    pub rotator: Option<(u8, u16, u8)>,

    /// Raw meter readings 0-255.
    pub s_meter: u8,
    pub meters: BTreeMap<MeterKey, u8>,
    /// RI flags by P1 code.
    pub radio_info: BTreeMap<u8, bool>,

    /// EX menu raw parameter text by item number.
    pub menu: BTreeMap<u16, String>,
    pub memories: BTreeMap<u16, ChannelInfo>,
    pub keyer_memory: [Option<String>; 5],
}

/// `Meter` is not `Ord`; keep a sortable key for the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MeterKey(pub char);

impl From<Meter> for MeterKey {
    fn from(m: Meter) -> Self {
        MeterKey(m.rm_code())
    }
}

fn n<T: std::str::FromStr>(s: &str) -> Option<T> {
    s.trim_start_matches('+').parse().ok()
}

fn flag(s: &str) -> bool {
    s == "1"
}

impl RadioState {
    pub fn new() -> Self {
        Self { roofing: '0', ..Default::default() }
    }

    pub fn for_model(model: &'static RadioModel) -> Self {
        Self { model, ..Self::new() }
    }

    /// The raw value of menu item `num`.
    pub fn menu_raw(&self, num: u16) -> Option<&str> {
        self.menu.get(&num).map(String::as_str)
    }

    /// Is a CW signal heard on the upper side of the carrier (like USB)?
    /// CW BFO: USB, LSB or AUTO (LSB below 10 MHz); CW-R flips it.
    pub fn cw_upper(&self, mode: Mode, dial: u64) -> bool {
        let normal = match self.model.roles.cw_bfo {
            Some((item, [_, lsb, auto])) => match self.menu_raw(item) {
                Some(v) if v == lsb => false,
                Some(v) if v == auto => dial >= 10_000_000,
                _ => true,
            },
            None => true,
        };
        if mode == Mode::CwR { !normal } else { normal }
    }

    /// The dial shows the frequency of a CW signal heard at the pitch (CW
    /// FREQ DISPLAY = PITCH OFFSET), not the carrier.
    pub fn cw_dial_is_pitch_offset(&self) -> bool {
        self.model.roles.cw_pitch_display.is_some_and(|(item, v)| self.menu_raw(item) == Some(v))
    }

    /// DATA MODE = OTHER: DATA uses the SSB widths and the display offset.
    pub fn data_is_other(&self) -> bool {
        self.model.roles.data_other.is_some_and(|(item, v)| self.menu_raw(item) == Some(v))
    }

    /// How far the dial is shifted in DATA (OTHER DISP), in Hz.
    pub fn data_display_offset(&self) -> f32 {
        if !self.data_is_other() {
            return 0.0;
        }
        self.model.roles.data_disp.and_then(|i| self.menu_raw(i)).and_then(|v| v.trim_start_matches('+').parse().ok()).unwrap_or(0.0)
    }

    pub fn has(&self, key: &str) -> bool {
        self.seen.contains(key)
    }

    pub fn meter(&self, m: Meter) -> Option<u8> {
        if m == Meter::S {
            return Some(self.s_meter);
        }
        self.meters.get(&MeterKey::from(m)).copied()
    }

    pub fn transmitting(&self) -> bool {
        self.tx != 0 || self.mox
    }

    /// Receiving on VFO-B (FR4/FR5).
    pub fn rx_on_b(&self) -> bool {
        self.rx_function >= 4
    }

    pub fn split(&self) -> bool {
        (self.tx_vfo == 1) != self.rx_on_b()
    }

    pub fn rx_freq(&self) -> u64 {
        if self.rx_on_b() { self.freq_b } else { self.freq_a }
    }

    pub fn rx_mode(&self) -> Option<Mode> {
        if self.rx_on_b() { self.mode_b } else { self.mode_a }
    }

    pub fn tx_mode(&self) -> Option<Mode> {
        if self.tx_vfo == 1 { self.mode_b } else { self.mode_a }
    }

    pub fn tx_freq(&self) -> u64 {
        if self.tx_vfo == 1 { self.freq_b } else { self.freq_a }
    }

    /// Which width table applies to `mode`. On the FTDX3000 DATA modes use
    /// the RTTY/PSK widths when menu 067 DATA MODE is PSK (the default) and
    /// the SSB widths when it is OTHER; radios without that menu use the
    /// SSB widths.
    pub fn width_class(&self, mode: Mode) -> WidthClass {
        match mode {
            Mode::DataLsb | Mode::DataUsb => {
                if self.model.roles.data_psk {
                    WidthClass::Rtty
                } else if self.model.roles.data_other.is_none() || self.data_is_other() {
                    WidthClass::Ssb
                } else {
                    WidthClass::Rtty
                }
            }
            m => m.width_class(),
        }
    }

    /// Bandwidth in Hz from width index, narrow flag and mode.
    pub fn width_hz(&self) -> Option<u32> {
        self.model.width_hz(self.width_class(self.mode_a?), self.narrow, self.width_index)
    }

    /// Apply one frame from the radio. Returns false if it was not understood.
    pub fn apply(&mut self, frame: &str) -> bool {
        let Some((code, p)) = frame::split(frame) else {
            return false;
        };
        let ok = self.apply_inner(&code, p).is_some();
        if ok {
            if let Some(k) = answer_key(frame) {
                self.seen.insert(k);
            }
        }
        ok
    }

    fn apply_inner(&mut self, code: &str, p: &str) -> Option<()> {
        let at = |i: usize| p.get(i..i + 1).unwrap_or("");
        let from = |i: usize| p.get(i..).unwrap_or("");
        match code {
            "AC" => self.tuner = n(at(2))?,
            "AG" => self.af_gain = n(from(1))?,
            "AI" => self.auto_info = flag(p),
            "AN" => self.antenna = n(at(1))?,
            "BC" => self.auto_notch = flag(at(1)),
            "BI" => self.break_in = flag(p),
            "BP" => match at(1) {
                "0" => self.manual_notch = n::<u16>(from(2))? != 0,
                "1" => self.notch_freq = n(from(2))?,
                _ => return None,
            },
            "BY" => self.busy = flag(at(0)),
            "CN" => self.ctcss_tone = n(from(1))?,
            "CO" => match at(1) {
                "0" => self.contour_mode = n(from(2))?,
                "1" => self.contour_freq = n(from(2))?,
                "2" => self.apf_freq = n(from(2))?,
                _ => return None,
            },
            "CS" => self.cw_spot = flag(p),
            "CT" => self.ctcss_mode = n(from(1))?,
            "DA" if p.len() >= 6 => self.dimmer = (n(&p[0..2])?, n(&p[2..4])?, n(&p[4..6])?),
            "EX" if p.len() >= 3 => {
                let num: u16 = n(&p[0..3])?;
                self.menu.insert(num, p[3..].to_string());
            }
            "FA" => {
                self.freq_a = n(p)?;
                self.info_a.freq = self.freq_a;
            }
            "FB" => {
                self.freq_b = n(p)?;
                self.info_b.freq = self.freq_b;
            }
            "FR" => self.rx_function = n(p)?,
            "FS" => self.fast = n(p)?,
            "FT" => self.tx_vfo = n(p)?,
            "GT" => self.agc = n(at(1))?,
            "ID" => self.id = Some(n(p)?),
            "IF" => {
                let mut info = ChannelInfo::parse(p)?;
                // On its 60 m memories an FTDX3000 shows (and `FA` reads)
                // the channel centre, but `IF` gives it up to 192 Hz low
                // (5331936 for 5332000): go by what the radio shows. Only
                // on the memory itself (the dial doesn't move it), so a
                // tuned frequency always reads as it is.
                let sixty = self.model.sixty_m_memories().is_some_and(|(usb, cw)| info.mem_mode == 1 && (usb..cw + 5).contains(&info.channel));
                if sixty {
                    info.freq = (info.freq + 250) / 500 * 500;
                }
                self.freq_a = info.freq;
                self.mode_a = info.mode.or(self.mode_a);
                self.rx_clar = info.rx_clar;
                self.tx_clar = info.tx_clar;
                self.info_a = info;
            }
            "IS" => self.if_shift = n(from(1))?,
            "KM" => {
                let ch: usize = n(at(0))?;
                // The radio marks the end of the text with '}' ("KM1};" = empty).
                let text = from(1).split('}').next().unwrap_or("").trim_end().to_string();
                *self.keyer_memory.get_mut(ch.checked_sub(1)?)? = Some(text);
            }
            "KP" => self.key_pitch = n(p)?,
            "KR" => self.keyer = flag(p),
            "KS" => self.key_speed = n(p)?,
            "LK" => self.lock = n(p)?,
            "MC" => self.memory_channel = n(p)?,
            "MD" => {
                let m = Mode::from_code(at(1).chars().next()?)?;
                self.mode_a = Some(m);
                self.info_a.mode = Some(m);
            }
            "MG" => self.mic_gain = n(p)?,
            "ML" => match at(0) {
                "0" => self.monitor = n::<u16>(from(1))? != 0,
                "1" => self.monitor_level = n(from(1))?,
                _ => return None,
            },
            "MR" => {
                let info = ChannelInfo::parse(p)?;
                self.memories.insert(info.channel, info);
            }
            "MS" => self.meter_sw = n(p)?,
            "MX" => self.mox = flag(p),
            "NA" => self.narrow = flag(at(1)),
            "NB" => self.nb = n(at(1))?,
            "NL" => self.nb_level = n(from(1))?,
            "NR" => self.nr = flag(at(1)),
            "OI" => {
                let info = ChannelInfo::parse(p)?;
                self.freq_b = info.freq;
                self.mode_b = info.mode.or(self.mode_b);
                self.info_b = info;
            }
            "OS" => self.rpt_shift = n(at(1))?,
            "PA" => self.preamp = n(at(1))?,
            "PC" => self.power = n(p)?,
            "PL" => self.proc_level = n(p)?,
            "PR" => match at(0) {
                // The reference book says P2 1 = OFF, 2 = ON, but the radio
                // (and Hamlib) use 0 = OFF, 1 = ON.
                "0" => self.processor = at(1) == "1",
                "1" => self.mic_eq = at(1) == "1",
                _ => return None,
            },
            "PS" => self.power_on = Some(flag(p)),
            "RA" => self.att = n(at(1))?,
            "RF" => self.roofing = at(1).chars().next()?,
            "RG" => self.rf_gain = n(from(1))?,
            "RI" => {
                self.radio_info.insert(n(at(0))?, flag(at(1)));
            }
            "RL" => self.nr_level = n(from(1))?,
            "RM" => {
                let c = at(0).chars().next()?;
                let v: u8 = n(from(1))?;
                if c == '1' {
                    self.s_meter = v;
                }
                self.meters.insert(MeterKey(c), v);
            }
            "RO" if p.len() >= 7 => self.rotator = Some((n(&p[0..1])?, n(&p[1..4])?, n(&p[4..7])?)),
            "RS" => self.menu_mode = flag(p),
            "RT" => self.rx_clar = flag(p),
            "SC" => self.scan = n(p)?,
            "SD" => self.bk_delay = n(p)?,
            "SF" => self.sub_dial = n(p)?,
            "SH" => self.width_index = n(from(1))?,
            "SM" => self.s_meter = n(from(1))?,
            "SQ" => self.squelch = n(from(1))?,
            "TS" => self.txw = flag(p),
            "TX" => self.tx = n(p)?,
            "UL" => self.pll_unlock = flag(p),
            "VD" => self.vox_delay = n(p)?,
            "VF" if p.len() >= 5 => {
                self.vrf_on = flag(&p[1..2]);
                self.vrf_value = n(&p[2..5])?;
            }
            "VG" => self.vox_gain = n(p)?,
            "VS" => self.vfo_select = n(p)?,
            "VX" => self.vox = flag(p),
            "XT" => self.tx_clar = flag(p),
            _ => return None,
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IF_FRAME: &str = "IF00114074000+012010200000;";

    #[test]
    fn parses_if() {
        let mut s = RadioState::new();
        assert!(s.apply(IF_FRAME));
        assert_eq!(s.freq_a, 14_074_000);
        assert_eq!(s.info_a.clar_offset, 120);
        assert!(s.rx_clar);
        assert!(!s.tx_clar);
        assert_eq!(s.mode_a, Some(Mode::Usb));
        assert_eq!(s.info_a.channel, 1);
        assert!(s.has("IF"));
    }

    #[test]
    fn sixty_m_memories_read_as_shown() {
        // What an FTDX3000 answered on each of its 60 m memories, and what
        // its display and `FA` said.
        let mut s = RadioState::new();
        for (frame, shown) in [
            ("IF11805331936+000000210000;", 5_332_000),
            ("IF11905347808+000000210000;", 5_348_000),
            ("IF12005358308+000000210000;", 5_358_500),
            ("IF12105372904+000000210000;", 5_373_000),
            ("IF12205404904+000000210000;", 5_405_000),
            ("IF12305331968+000000310000;", 5_332_000),
            ("IF12705404936+000000310000;", 5_405_000),
        ] {
            assert!(s.apply(frame));
            assert_eq!(s.freq_a, shown, "{frame}");
        }
        // Ordinary memories, memory tune and the VFO keep every hertz.
        for (frame, hz) in [
            ("IF05007063510-090000C10000;", 7_063_510),
            ("IF11805331936+000000220000;", 5_331_936),
            ("IF11805331936+000000200000;", 5_331_936),
        ] {
            assert!(s.apply(frame));
            assert_eq!(s.freq_a, hz, "{frame}");
        }
    }

    #[test]
    fn long_frequencies_in_channel_info() {
        for hz in [144_174_000u64, 1_296_200_000] {
            let info = ChannelInfo { channel: 1, freq: hz, mode: Some(Mode::Usb), ..Default::default() };
            let a = info.to_answer("IF");
            assert_eq!(ChannelInfo::parse(&a[2..a.len() - 1]).map(|i| i.freq), Some(hz), "{a}");
        }
    }

    #[test]
    fn channel_info_round_trip() {
        let info = ChannelInfo::parse(&IF_FRAME[2..IF_FRAME.len() - 1]).unwrap();
        assert_eq!(info.to_answer("IF"), IF_FRAME);
        assert_eq!(info.to_mw(5), "MW00514074000+012010200000;");
        // 9 and 10 frequency digits (IC-705 2 m, IC-9700 23 cm).
        for f in ["IF001145500000+000000410000;", "IF0011296100000-001010400000;"] {
            let info = ChannelInfo::parse(&f[2..f.len() - 1]).unwrap();
            assert_eq!(info.to_answer("IF"), f);
        }
    }

    #[test]
    fn applies_common_answers() {
        let mut s = RadioState::new();
        for f in ["FA07074000;", "FB14074000;", "MD03;", "AG0128;", "SM0130;", "RM5161;", "TX1;", "FT1;", "FR0;", "EX0383;", "NA01;", "SH010;"] {
            assert!(s.apply(f), "{f}");
        }
        assert_eq!(s.freq_a, 7_074_000);
        assert_eq!(s.freq_b, 14_074_000);
        assert_eq!(s.mode_a, Some(Mode::Cw));
        assert_eq!(s.af_gain, 128);
        assert_eq!(s.s_meter, 130);
        assert_eq!(s.meter(Meter::Po), Some(161));
        assert!(s.transmitting());
        assert!(s.split());
        assert_eq!(s.menu.get(&38).map(String::as_str), Some("3"));
        assert_eq!(s.width_hz(), Some(500));
        assert!(s.apply("KM1};"));
        assert_eq!(s.keyer_memory[0].as_deref(), Some(""));
        assert!(s.apply("KM2CQ TEST}"));
        assert_eq!(s.keyer_memory[1].as_deref(), Some("CQ TEST"));
        assert!(s.apply("MD0C;"));
        assert_eq!(s.width_class(Mode::DataUsb), WidthClass::Rtty);
        assert!(s.apply("EX0671;"));
        assert_eq!(s.width_class(Mode::DataUsb), WidthClass::Ssb);
        assert!(!s.apply("?;"));
        assert!(!s.apply("ZZ1;"));
    }
}
