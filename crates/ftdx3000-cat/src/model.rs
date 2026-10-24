//! The radios RigHelm can control, and what each one has.
//!
//! Inside RigHelm everything speaks one language: the FTDX3000's CAT
//! commands (the table in [`crate::command`]), whatever radio is connected.
//! Each model's translator ([`crate::dialect`]) turns that into the radio's
//! own protocol on the wire, so the engine, the virtual ports, the rigctld
//! server and remote control work the same for every radio.
//!
//! What a parameter means is the FTDX3000's, with these exceptions, which are
//! the connected model's own:
//!
//! * `EXnnn` menu numbers: `nnn` is the item's place in the model's
//!   [`RadioModel::menu`] table, the value its raw text as the radio has it.
//! * `SH0nn` width index: the model's [`Caps::widths`] table.
//! * `PA0n` / `RA0n` / `RF0n` / `NB0n` / `AN0n` codes: the model's lists.
//! * `BSnn` band codes: the model's [`RadioModel::bands`].
//!
//! Meter readings (`SM0`, `RMn`) are always on the FTDX3000's scale, so one
//! calibration in [`crate::types`] fits every radio; translators convert.
//! The model is detected from the radio's answer to an identification
//! command, or chosen in the settings.

use crate::menu::{self, MenuItem};
use crate::types::{self, Band, Mode, WidthClass};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Maker {
    Yaesu,
    Icom,
    Kenwood,
    Elecraft,
}

impl Maker {
    pub fn name(self) -> &'static str {
        match self {
            Maker::Yaesu => "Yaesu",
            Maker::Icom => "Icom",
            Maker::Kenwood => "Kenwood",
            Maker::Elecraft => "Elecraft",
        }
    }
}

/// How the radio talks on its CAT port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    /// The FTDX3000's own CAT: RigHelm's language, no translation.
    Ftdx3000,
    /// Yaesu's other text CAT radios (FTDX101, FT-991A, FT-710, ...).
    Yaesu,
    /// Kenwood text commands (and radios that speak them, like Elecraft).
    Kenwood,
    /// Icom CI-V (binary frames).
    Icom,
}

/// How RigHelm talks to it.
#[derive(Debug, Clone, Copy)]
pub enum Proto {
    /// The FTDX3000's own CAT: no translation.
    Native,
    /// Text commands, translated by these functions.
    Text(&'static crate::dialect::text::TextSpec),
    /// Icom CI-V.
    CiV(&'static crate::dialect::icom::CivSpec),
}

/// Menu items that features of RigHelm work through.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Roles {
    /// MIC SEL items (one per mode group) that pick the radio's own
    /// microphone or its USB audio input, and those two codes.
    pub mic_sel: &'static [u16],
    pub mic_front: i32,
    pub mic_usb: i32,
    /// CW keying on the second (Standard) USB port's RTS line: the item and
    /// its OFF and RTS codes.
    pub pc_keying: Option<(u16, i32, i32)>,
    /// Set on connect: CAT time-out timer item and the raw value (100 ms).
    pub cat_tot: Option<(u16, &'static str)>,
    /// Items that change how RigHelm talks to the radio; a menu restore
    /// skips them unless asked.
    pub cat_items: &'static [u16],
    /// CW BFO side: item and the raw codes for USB, LSB and AUTO (LSB below
    /// 10 MHz).
    pub cw_bfo: Option<(u16, [&'static str; 3])>,
    /// CW FREQ DISPLAY: item and the code for "pitch offset" (the dial shows
    /// a signal heard at the pitch).
    pub cw_pitch_display: Option<(u16, &'static str)>,
    /// DATA MODE item and the code for OTHER (DATA uses the SSB widths and
    /// the display offset below); otherwise DATA uses the PSK widths.
    pub data_other: Option<(u16, &'static str)>,
    /// OTHER DISP (SSB) item: the dial shift in DATA, in Hz.
    pub data_disp: Option<u16>,
}

impl Roles {
    pub const NONE: Roles = Roles {
        mic_sel: &[],
        mic_front: 0,
        mic_usb: 0,
        pc_keying: None,
        cat_tot: None,
        cat_items: &[],
        cw_bfo: None,
        cw_pitch_display: None,
        data_other: None,
        data_disp: None,
    };
}

/// Memory channels: `first..=last`, the ones from `pms` on are PMS pairs
/// (P-1L, P-1U, ...), shown that way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    pub first: u16,
    pub last: u16,
    pub pms: Option<u16>,
}

impl Memory {
    pub fn name(&self, ch: u16) -> String {
        match self.pms {
            Some(p) if ch >= p => {
                let pair = (ch - p) / 2 + 1;
                format!("P-{pair}{}", if (ch - p) % 2 == 0 { 'L' } else { 'U' })
            }
            _ => format!("{ch:03}"),
        }
    }

    pub fn channels(&self) -> std::ops::RangeInclusive<u16> {
        self.first..=self.last
    }
}

/// What the radio has, beyond the commands themselves.
#[derive(Debug, Clone, Copy)]
pub struct Caps {
    /// The RigHelm (FTDX3000) commands this radio can carry out. Anything
    /// else is answered `?;` without bothering the radio, and the controls
    /// for it are hidden.
    pub commands: &'static [&'static str],
    /// Width tables: `(SH index, Hz)` for a mode class and the narrow switch.
    pub widths: fn(WidthClass, bool) -> Vec<(u8, u32)>,
    /// A separate narrow switch (`NA`) with its own width table.
    pub narrow: bool,
    /// `PA0n`: front end (IPO / preamps).
    pub preamp: &'static [(u8, &'static str)],
    /// `RA0n`: attenuator.
    pub att: &'static [(u8, &'static str)],
    /// `GT0n` codes that can be set (FTDX3000 meaning: 0 OFF, 1 FAST,
    /// 2 MID, 3 SLOW, 4 AUTO).
    pub agc: &'static [(u8, &'static str)],
    /// `RF0x` roofing filter: Set codes, and the labels of answer codes.
    pub roofing: &'static [(char, &'static str)],
    pub roofing_answers: &'static [(char, &'static str)],
    /// `NB0n`: noise blanker settings.
    pub nb: &'static [(u8, &'static str)],
    /// Highest `RL0` noise reduction level.
    pub nr_max: u8,
    /// Highest `NL0` noise blanker level.
    pub nb_level_max: u16,
    /// Number of antenna jacks (`AN0n`), 1 = no antenna switch.
    pub antennas: u8,
    /// Transmit power range in watts (`PC`).
    pub power: (u8, u8),
    /// `MS` meter selection codes and labels.
    pub meter_select: &'static [(u8, &'static str)],
    /// Voice (DVS) memory channels (`PB0n`).
    pub voice_memories: u8,
    /// CW keyer memories (`KM`/`KY`) and their length.
    pub keyer_memories: u8,
    pub keyer_memory_len: u8,
    pub memory: Memory,
    /// The clarifier offset is shared between RX and TX clarifier.
    pub shared_clarifier: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct RadioModel {
    /// Stable name used in the config file ("ftdx3000", "ic7300").
    pub key: &'static str,
    /// As printed on the radio ("FTDX3000", "IC-7300").
    pub name: &'static str,
    pub maker: Maker,
    pub family: Family,
    pub proto: Proto,
    /// Answer to `ID;` without the command (Yaesu "0462", Kenwood "023");
    /// empty for CI-V radios.
    pub id: &'static str,
    /// CI-V default address (Icom), else 0.
    pub civ_addr: u8,
    /// Hamlib's model number (for documentation and cross-checks).
    pub hamlib: u32,
    pub freq_min: u64,
    pub freq_max: u64,
    /// Band keys (`BSnn` codes) in front-panel order; code 11 is GEN.
    pub bands: &'static [Band],
    /// Modes, in front-panel order.
    pub modes: &'static [Mode],
    pub menu: &'static [MenuItem],
    pub roles: Roles,
    pub caps: Caps,
    /// Serial rates to try, fastest first, stop bits, and whether the radio
    /// uses RTS/CTS handshake by default.
    pub bauds: &'static [u32],
    pub stop_bits: u8,
    pub rts_cts: bool,
    /// RigHelm answers the simulator starts with, over the FTDX3000's
    /// defaults (values that would be out of range on this radio).
    pub sim: &'static [&'static str],
}

impl RadioModel {
    pub fn has(&self, code: &str) -> bool {
        self.caps.commands.iter().any(|c| c.eq_ignore_ascii_case(code))
    }

    /// Display name with the maker: "Yaesu FTDX3000".
    pub fn full_name(&self) -> String {
        format!("{} {}", self.maker.name(), self.name)
    }

    pub fn menu_item(&self, num: u16) -> Option<&'static MenuItem> {
        let m = self.menu.get(num.checked_sub(1)? as usize)?;
        (m.num == num).then_some(m).or_else(|| self.menu.iter().find(|m| m.num == num))
    }

    /// Menu groups in front-panel order.
    pub fn menu_groups(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        for m in self.menu {
            if !out.contains(&m.group) {
                out.push(m.group);
            }
        }
        out
    }

    pub fn clamp_freq(&self, hz: u64) -> u64 {
        hz.clamp(self.freq_min, self.freq_max)
    }

    pub fn in_range(&self, hz: u64) -> bool {
        (self.freq_min..=self.freq_max).contains(&hz)
    }

    /// The amateur band (not GEN) a frequency is in.
    pub fn band_of(&self, hz: u64) -> Option<&'static Band> {
        self.bands.iter().filter(|b| b.label != "GEN").find(|b| (b.lo..=b.hi).contains(&hz))
    }

    pub fn width_hz(&self, class: WidthClass, narrow: bool, index: u8) -> Option<u32> {
        if class == WidthClass::Fixed {
            return None;
        }
        (self.caps.widths)(class, narrow).into_iter().find(|(i, _)| *i == index).map(|(_, w)| w).filter(|&w| w != 0)
    }

    pub fn roofing_label(&self, code: char) -> Option<&'static str> {
        self.caps.roofing_answers.iter().find(|(c, _)| c.eq_ignore_ascii_case(&code)).map(|(_, l)| *l)
    }

    pub fn is_ftdx3000(&self) -> bool {
        self.family == Family::Ftdx3000
    }
}

/// Every RigHelm command, for radios that have them all.
pub const ALL_COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AM", "AN", "BA", "BC", "BD", "BI", "BP", "BS", "BU", "BY", "CH", "CN", "CO", "CS", "CT",
    "DA", "DN", "ED", "EK", "EM", "EN", "EU", "EX", "FA", "FB", "FR", "FS", "FT", "GT", "ID", "IF", "IS", "KM", "KP",
    "KR", "KS", "KY", "LK", "LM", "MA", "MC", "MD", "MG", "ML", "MR", "MS", "MW", "MX", "NA", "NB", "NL", "NR", "OI",
    "OS", "PA", "PB", "PC", "PL", "PR", "PS", "QI", "QR", "QS", "RA", "RC", "RD", "RF", "RG", "RI", "RL", "RM", "RO",
    "RS", "RT", "RU", "SC", "SD", "SF", "SH", "SM", "SQ", "SV", "TS", "TX", "UL", "UP", "VD", "VF", "VG", "VM", "VS",
    "VX", "XT",
];

fn ftdx3000_widths(class: WidthClass, narrow: bool) -> Vec<(u8, u32)> {
    types::width_choices(class, narrow)
}

pub static FTDX3000: RadioModel = RadioModel {
    key: "ftdx3000",
    name: "FTDX3000",
    maker: Maker::Yaesu,
    family: Family::Ftdx3000,
    proto: Proto::Native,
    id: "0462",
    civ_addr: 0,
    hamlib: 1037,
    freq_min: types::FREQ_MIN,
    freq_max: types::FREQ_MAX,
    bands: types::BANDS,
    modes: &Mode::ALL,
    menu: menu::MENU,
    roles: Roles {
        mic_sel: &[53, 85, 103],
        mic_front: 0,
        mic_usb: 2,
        pc_keying: Some((65, 0, 2)),
        cat_tot: Some((39, "1")),
        cat_items: &[37, 38, 39, 40],
        cw_bfo: Some((60, ["0", "1", "2"])),
        cw_pitch_display: Some((64, "1")),
        data_other: Some((67, "1")),
        data_disp: Some(69),
    },
    caps: Caps {
        commands: ALL_COMMANDS,
        widths: ftdx3000_widths,
        narrow: true,
        preamp: &[(0, "IPO"), (1, "AMP1"), (2, "AMP2")],
        att: &[(0, "OFF"), (1, "6"), (2, "12"), (3, "18")],
        agc: types::AGC_SET,
        roofing: types::ROOFING_SET,
        roofing_answers: &[
            ('1', "15 kHz"),
            ('2', "6 kHz"),
            ('3', "3 kHz"),
            ('4', "AUTO 15 kHz"),
            ('5', "AUTO 6 kHz"),
            ('6', "AUTO 3 kHz"),
            ('7', "600 Hz"),
            ('8', "300 Hz"),
            ('9', "AUTO 600 Hz"),
            ('A', "AUTO 300 Hz"),
        ],
        nb: &[(0, "OFF"), (1, "NB"), (2, "WIDE")],
        nr_max: 15,
        nb_level_max: 100,
        antennas: 3,
        power: (5, 100),
        meter_select: &[(0, "COMP"), (1, "ALC"), (2, "PO"), (3, "SWR"), (4, "ID"), (5, "VDD")],
        voice_memories: 5,
        keyer_memories: 5,
        keyer_memory_len: 50,
        memory: Memory { first: 1, last: 117, pms: Some(100) },
        shared_clarifier: true,
    },
    bauds: &[38400, 19200, 9600, 4800],
    stop_bits: 2,
    rts_cts: true,
    sim: &[],
};

/// Every supported radio, FTDX3000 first.
pub fn all() -> &'static [&'static RadioModel] {
    crate::models::ALL
}

/// The model stored in a config file.
pub fn by_key(key: &str) -> Option<&'static RadioModel> {
    all().iter().copied().find(|m| m.key.eq_ignore_ascii_case(key))
}

/// The text radio whose `ID;` answer this is (`ID0462;`, `ID023;`).
/// Several models can share an answer; the first listed wins.
pub fn by_id_answer(answer: &str) -> Option<&'static RadioModel> {
    let id = answer.trim().strip_prefix("ID")?.trim_end_matches(';');
    all().iter().copied().find(|m| m.family != Family::Icom && m.id == id)
}

/// The Icom radio at a CI-V address.
pub fn by_civ_addr(addr: u8) -> Option<&'static RadioModel> {
    all().iter().copied().find(|m| m.family == Family::Icom && m.civ_addr == addr)
}

impl Default for &'static RadioModel {
    fn default() -> Self {
        &FTDX3000
    }
}

impl PartialEq for RadioModel {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for RadioModel {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ftdx3000_has_every_command_in_the_table() {
        for c in crate::command::COMMANDS {
            assert!(FTDX3000.has(c.code), "{}", c.code);
        }
        assert_eq!(ALL_COMMANDS.len(), crate::command::COMMANDS.len());
    }

    #[test]
    fn registry_is_consistent() {
        let mut keys = std::collections::HashSet::new();
        for m in all() {
            assert!(keys.insert(m.key), "duplicate key {}", m.key);
            assert!(m.freq_min < m.freq_max, "{}", m.key);
            assert!(!m.modes.is_empty() && !m.bands.is_empty(), "{}", m.key);
            assert!(m.caps.memory.first <= m.caps.memory.last, "{}", m.key);
            for (i, item) in m.menu.iter().enumerate() {
                assert_eq!(item.num as usize, i + 1, "{} menu {}", m.key, item.name);
            }
            for r in m.roles.mic_sel.iter().chain(m.roles.cat_items) {
                assert!(m.menu_item(*r).is_some(), "{} role item {r}", m.key);
            }
            for c in m.caps.commands {
                assert!(crate::command::lookup(c).is_some(), "{} command {c}", m.key);
            }
            match m.family {
                Family::Icom => assert!(m.civ_addr != 0, "{}", m.key),
                _ => assert!(!m.id.is_empty(), "{}", m.key),
            }
        }
        assert_eq!(by_id_answer("ID0462;").map(|m| m.key), Some("ftdx3000"));
        assert_eq!(by_key("FTDX3000").map(|m| m.name), Some("FTDX3000"));
    }

    #[test]
    fn pms_names() {
        let m = FTDX3000.caps.memory;
        assert_eq!(m.name(5), "005");
        assert_eq!(m.name(100), "P-1L");
        assert_eq!(m.name(101), "P-1U");
        assert_eq!(m.name(117), "P-9U");
    }
}
