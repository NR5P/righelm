//! Kenwood TS-590S and TS-590SG ("PC Control Command Reference Guide",
//! January 2019, which covers both), and the Kenwood helpers the TS-890S
//! and TS-990S translators share.
//!
//! How RigHelm's (FTDX3000) commands land on the TS-590:
//!
//! * `FA`/`FB`: 11 digits. `MD0x`: `MD` + `DA` (the DATA modes are
//!   USB/LSB/FM/AM with `DA1`); FSK is RTTY-L, FSK-R RTTY-U.
//! * `IF`: field by field (the radio's has no DATA flag: the last `DA` is
//!   used); it also gives RigHelm `TX` (the radio has no TX read) and `CT0`.
//! * Split: RigHelm `FR` (receiver) and `VS` are the radio's `FR` (which
//!   also makes it simplex), RigHelm `FT` is the radio's `FT`.
//! * `TX1` (CAT PTT) is `TX0;` (microphone) or `TX1;` (DATA SEND, the rear
//!   input) in DATA modes or, on the SG, when menu 070 says REAR; `TX0` is
//!   `RX;`.
//! * `SH0nn`: in CW/FSK the radio's `FW` width (Hz, table [`WIDTH`]); in
//!   SSB its `SH` high cut ([`HICUT`]); in SSB-DATA its `SL` width (or `SH`
//!   when the SG's menu 029 is HI/LO).
//! * `BC` is Beat Cancel, `BP00` the manual notch (`NT2`), `BP01` its
//!   frequency (the radio's 0-127 spread over RigHelm's 10-4000 Hz).
//! * `GT0` is `GC` (OFF/SLOW/FAST), `KP` the pitch menu item, `VG` 0-9,
//!   `ML` 0-20 (SG) / 0-9 (S), `PL` the processor input level.
//! * Meters: `SM0` (0-30 dots) is the S meter on receive and the power
//!   meter on transmit; `RM;` answers SWR, COMP and ALC.
//! * `EXnnn`: the radio's `EXaaa0000v` (aaa = menu number).
//!
//! The radio's Set and Answer have different layouts for a few commands
//! (`PA1;` / `PA10;`); the translator knows it is playing the radio for a
//! program (virtual port, simulator) once the program has read something.

use crate::dialect::text::{Ctx, TextSpec, freq_digits, is_read, parts, read_prefix};
use crate::dialect::{num as pnum, rescale};
use crate::menu::{Kind, MenuItem, num};
use crate::model::{Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::state::ChannelInfo;
use crate::types::{Band, CTCSS_DHZ, Mode, PO_CAL, S_METER_CAL, SWR_CAL, WidthClass};

// ------------------------------------------------------------- helpers
// Shared with the TS-890S and TS-990S.

/// A menu item numbered `num` in RigHelm and `addr` on the radio.
pub const fn item(num: u16, addr: u32, group: &'static str, name: &'static str, width: u8, kind: Kind) -> MenuItem {
    MenuItem { num, addr, group, name, width, kind }
}

pub fn one(s: impl Into<String>) -> Vec<String> {
    vec![s.into()]
}

/// The translator plays the radio for a program (it has seen a Read
/// command, which only programs send).
pub fn emulating(cx: &Ctx) -> bool {
    cx.recall("emu").is_some()
}

/// The RigHelm frames for a program's Read command.
pub fn rd(cx: &mut Ctx, frames: &[&str]) -> Vec<String> {
    cx.remember("emu", "1");
    frames.iter().map(|s| s.to_string()).collect()
}

/// Character `i` of `p` (or NUL).
pub fn at(p: &str, i: usize) -> char {
    p.as_bytes().get(i).map(|&b| b as char).unwrap_or('\0')
}

pub fn recall_char(cx: &Ctx, key: &str) -> Option<char> {
    cx.recall(key).and_then(|v| v.chars().next())
}

/// `v` on a 0..=`from` scale as 0..=`to`, rounded.
pub fn scale(v: u32, from: u32, to: u32) -> u32 {
    (v * to + from / 2) / from.max(1)
}

/// Kenwood mode digit (`MD`) and DATA flag as a RigHelm mode.
pub fn rh_mode(md: char, data: bool) -> Option<Mode> {
    Some(match (md, data) {
        ('1', false) => Mode::Lsb,
        ('1', true) => Mode::DataLsb,
        ('2', false) => Mode::Usb,
        ('2', true) => Mode::DataUsb,
        ('3', _) => Mode::Cw,
        ('4', false) => Mode::Fm,
        ('4', true) => Mode::DataFm,
        ('5', false) => Mode::Am,
        ('5', true) => Mode::DataAm,
        ('6', _) => Mode::RttyLsb,
        ('7', _) => Mode::CwR,
        ('9', _) => Mode::RttyUsb,
        _ => return None,
    })
}

/// A RigHelm mode as the Kenwood mode digit and DATA flag (`None`: the
/// mode has no DATA variant).
pub fn kw_mode(m: Mode) -> Option<(char, Option<bool>)> {
    Some(match m {
        Mode::Lsb => ('1', Some(false)),
        Mode::Usb => ('2', Some(false)),
        Mode::Cw => ('3', None),
        Mode::Fm => ('4', Some(false)),
        Mode::Am => ('5', Some(false)),
        Mode::RttyLsb => ('6', None),
        Mode::CwR => ('7', None),
        Mode::RttyUsb => ('9', None),
        Mode::DataLsb => ('1', Some(true)),
        Mode::DataUsb => ('2', Some(true)),
        Mode::DataFm => ('4', Some(true)),
        Mode::DataAm => ('5', Some(true)),
        _ => return None,
    })
}

/// The Kenwood mode digit has a DATA variant (LSB, USB, FM, AM).
pub fn has_data(md: char) -> bool {
    matches!(md, '1' | '2' | '4' | '5')
}

pub fn is_data(m: Mode) -> bool {
    matches!(m, Mode::DataLsb | Mode::DataUsb | Mode::DataFm | Mode::DataAm)
}

/// The current mode, from the last `MD`/`DA`/`IF` seen.
pub fn cur_mode(cx: &Ctx) -> Option<Mode> {
    rh_mode(recall_char(cx, "md")?, cx.recall("da") == Some("1"))
}

/// Kenwood tone/CTCSS numbers (`TN`/`CN` 00-41, `TN` 42 = 1750 Hz) in 0.1 Hz.
pub const KW_TONES: [u16; 43] = [
    670, 693, 719, 744, 770, 797, 825, 854, 885, 915, 948, 974, 1000, 1035, 1072, 1109, 1148, 1188, 1230, 1273, 1318,
    1365, 1413, 1462, 1514, 1567, 1622, 1679, 1738, 1799, 1862, 1928, 2035, 2065, 2107, 2181, 2257, 2291, 2336, 2418,
    2503, 2541, 17500,
];

fn nearest(table: &[u16], dhz: u16) -> u8 {
    table.iter().enumerate().min_by_key(|(_, t)| (**t as i32 - dhz as i32).abs()).map(|(i, _)| i as u8).unwrap_or(0)
}

/// RigHelm (FTDX3000) tone number as the Kenwood one.
pub fn kw_tone(rh: u8) -> u8 {
    nearest(&KW_TONES[..42], CTCSS_DHZ.get(rh as usize).copied().unwrap_or(670))
}

/// Kenwood tone number as RigHelm's.
pub fn rh_tone(kw: u8) -> u8 {
    nearest(&CTCSS_DHZ, KW_TONES.get(kw as usize).copied().unwrap_or(670))
}

/// Kenwood tone mode (0 off, 1 tone, 2 CTCSS, 3 cross tone) as RigHelm's
/// CTCSS mode (0 off, 1 ENC/DEC, 2 ENC) and back.
pub fn rh_tone_mode(kw: char) -> u8 {
    match kw {
        '1' => 2,
        '2' | '3' => 1,
        _ => 0,
    }
}

pub fn kw_tone_mode(rh: u8) -> char {
    match rh {
        1 => '2',
        2 => '1',
        _ => '0',
    }
}

/// An `IF`/`OI`/`MR` answer in RigHelm's (FTDX3000) layout.
#[allow(clippy::too_many_arguments)]
pub fn rh_info(cmd: &str, ch: u16, freq: u64, clar: i32, rx: bool, tx: bool, mode: char, mem: u8, ctcss: u8, tone: u8) -> String {
    let sign = if clar < 0 { '-' } else { '+' };
    format!("{cmd}{ch:03}{freq:08}{sign}{:04}{}{}{mode}{mem}{ctcss}{tone:02}0;", clar.unsigned_abs().min(9999), rx as u8, tx as u8)
}

/// Kenwood meter dots (`SM0`, `RM`: 0-30) -> dB over S9, watts, SWR.
/// Approximate: read off the TS-590's bar meter scales.
pub const KW_S_CAL: [(u8, f32); 5] = [(0, -54.0), (15, 0.0), (20, 20.0), (25, 40.0), (30, 60.0)];
pub const KW_PO_CAL: [(u8, f32); 5] = [(0, 0.0), (5, 10.0), (10, 25.0), (15, 50.0), (30, 100.0)];
pub const KW_SWR_CAL: [(u8, f32); 5] = [(0, 1.0), (9, 1.5), (12, 2.0), (15, 3.0), (30, 10.0)];
pub const KW_LINEAR_CAL: [(u8, f32); 2] = [(0, 0.0), (30, 255.0)];
pub const RAW_LINEAR_CAL: [(u8, f32); 2] = [(0, 0.0), (255, 255.0)];

/// The TX state as RigHelm's `TX` answer: 1 if RigHelm keyed it by CAT, 2
/// if the radio's own PTT did.
pub fn rh_tx(cx: &Ctx, on: bool) -> String {
    if !on {
        "TX0;".into()
    } else if cx.recall("cattx") == Some("1") {
        "TX1;".into()
    } else {
        "TX2;".into()
    }
}

/// Split from the last `FR`/`FT` seen.
pub fn kw_split(cx: &Ctx) -> bool {
    let rx = recall_char(cx, "vfo").unwrap_or('0');
    let tx = recall_char(cx, "ft").unwrap_or(rx);
    rx != '2' && tx != rx
}

// ------------------------------------------------------------- TS-590

pub static SPEC: TextSpec = TextSpec { down, up, answer_prefix: read_prefix };

/// FW widths in CW and FSK, and SL widths in SSB-DATA (Hz).
pub const WIDTH: [u32; 14] = [50, 80, 100, 150, 200, 250, 300, 400, 500, 600, 1000, 1500, 2000, 2500];
/// SH high cut in SSB (Hz).
pub const HICUT: [u32; 14] = [1000, 1200, 1400, 1600, 1800, 2000, 2200, 2400, 2600, 2800, 3000, 3400, 4000, 5000];

fn widths(class: WidthClass, _narrow: bool) -> Vec<(u8, u32)> {
    let t: &[u32] = match class {
        WidthClass::Ssb => &HICUT,
        WidthClass::Cw | WidthClass::Rtty => &WIDTH,
        WidthClass::Fixed => &[],
    };
    t.iter().enumerate().map(|(i, &w)| (i as u8, w)).collect()
}

fn sg(cx: &Ctx) -> bool {
    cx.model.key == "ts590sg"
}

/// Menu number of the side tone / pitch item.
fn pitch_addr(cx: &Ctx) -> u32 {
    if sg(cx) { 40 } else { 34 }
}

fn ml_max(cx: &Ctx) -> u32 {
    if sg(cx) { 20 } else { 9 }
}

/// The radio's command for the receive width in the current mode.
fn width_cmd(cx: &Ctx) -> Option<&'static str> {
    match cur_mode(cx) {
        Some(Mode::Cw | Mode::CwR | Mode::RttyLsb | Mode::RttyUsb) => Some("FW"),
        // SG menu 029 (SSB-DATA filter control): 0 HI/LO, 1 WIDTH/SHIFT.
        Some(Mode::DataLsb | Mode::DataUsb) if !sg(cx) || cx.recall("ex29") != Some("0") => Some("SL"),
        Some(Mode::Lsb | Mode::Usb | Mode::DataLsb | Mode::DataUsb) | None => Some("SH"),
        _ => None,
    }
}

fn down(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !p.is_ascii() {
        return Vec::new();
    }
    let read = is_read(f);
    let emu = emulating(cx);
    let tail = p.get(1..).unwrap_or("");
    // Answers for a program's IF read, before RigHelm's IF itself.
    if emu && !read && cx.recall("ifq").is_some() {
        match (code.as_str(), at(p, 0)) {
            ("FR", c) => {
                cx.remember("vfo", if c >= '4' { "1" } else { "0" });
                return Vec::new();
            }
            ("FT", c) => {
                cx.remember("ft", if c == '1' { "1" } else { "0" });
                return Vec::new();
            }
            ("TX", c) => {
                cx.remember("tx", if c == '0' { "0" } else { "1" });
                return Vec::new();
            }
            ("IF", _) => cx.memo.remove("ifq").map(|_| ()).unwrap_or(()),
            _ => {}
        }
    }
    match code.as_str() {
        "AG" | "BY" | "CH" | "DN" | "UP" | "ID" | "KS" | "MG" | "PC" | "PS" | "QI" | "RC" | "RS" | "RT" | "XT" | "TS" | "SD" | "SQ"
        | "VD" | "VX" => one(f),
        "AB" => one("VV;"),
        "MA" => one("SV;"),
        "AC" => match p {
            "" => one("AC;"),
            "000" => one("AC000;"),
            "001" => one("AC010;"),
            "002" => one(if emu { "AC011;" } else { "AC111;" }),
            _ => Vec::new(),
        },
        "AI" => match p {
            "" => one("AI;"),
            "0" => one("AI0;"),
            "1" => one("AI2;"),
            _ => Vec::new(),
        },
        "AN" => match (read, at(p, 1)) {
            (true, _) => one("AN;"),
            (false, a @ ('1' | '2')) if p.len() == 2 => one(format!("AN{a}99;")),
            (false, a @ ('1' | '2')) if p.len() == 3 => one(format!("AN{a}00;")),
            _ => Vec::new(),
        },
        "BC" if read => one("BC;"),
        "BC" => one(format!("BC{tail};")),
        "BP" => match (p.get(..2), p.get(2..).and_then(pnum::<u32>)) {
            (Some("00"), None) => one("NT;"),
            (Some("01"), None) => one("BP;"),
            (Some("00"), Some(v)) => one(if v > 0 { "NT20;" } else { "NT00;" }),
            (Some("01"), Some(v)) => one(format!("BP{:03};", ((v.clamp(1, 400) - 1) * 127 + 199) / 399)),
            _ => Vec::new(),
        },
        "BS" if p.len() == 2 => one(format!("BU{p};")),
        "CN" if read => one("CN;"),
        "CN" => match pnum::<u8>(tail) {
            Some(t) => {
                let k = kw_tone(t);
                vec![format!("CN{k:02};"), format!("TN{k:02};")]
            }
            None => Vec::new(),
        },
        "CT" if read => one("IF;"),
        "CT" => match tail {
            "0" => vec!["TO0;".into(), "CT0;".into()],
            "1" => vec!["TO0;".into(), "CT1;".into()],
            "2" => vec!["CT0;".into(), "TO1;".into()],
            _ => Vec::new(),
        },
        "EX" => ex_down(cx, p),
        "FB" if !read && cx.recall("sf") == Some("1") => {
            cx.remember("sf", "");
            let m = sf_mode(cur_mode(cx).unwrap_or(Mode::Usb));
            one(format!("SF1{:011}{m};", pnum::<u64>(p).unwrap_or(0)))
        }
        "FA" | "FB" => {
            if !read {
                cx.remember(&code.to_ascii_lowercase(), p);
            }
            freq_digits(f, 11)
        }
        "FR" | "VS" => match (code.as_str(), p) {
            (_, "") => one("FR;"),
            ("FR", "0" | "1") | ("VS", "0") => {
                cx.remember("vfo", "0");
                cx.remember("ft", "0");
                one("FR0;")
            }
            ("FR", "4" | "5") | ("VS", "1") => {
                cx.remember("vfo", "1");
                cx.remember("ft", "1");
                one("FR1;")
            }
            _ => Vec::new(),
        },
        "FT" => match p {
            "" => one("FT;"),
            "0" | "2" => {
                cx.remember("ft", "0");
                one("FT0;")
            }
            "1" | "3" => {
                cx.remember("ft", "1");
                one("FT1;")
            }
            _ => Vec::new(),
        },
        "GT" if read => one("GC;"),
        "GT" => match tail {
            "0" => one("GC0;"),
            "1" | "2" | "4" | "5" => one("GC2;"),
            "3" | "6" => one("GC1;"),
            _ => Vec::new(),
        },
        "IF" if read => one("IF;"),
        "IF" if cx.recall("sf") == Some("0") => {
            cx.remember("sf", "");
            let Some(info) = ChannelInfo::parse(p) else { return Vec::new() };
            one(format!("SF0{:011}{};", info.freq, sf_mode(info.mode.unwrap_or(Mode::Usb))))
        }
        "IF" => info_down(cx, p),
        "KP" => {
            let a = pitch_addr(cx);
            match pnum::<u32>(p) {
                None => one(format!("EX{a:03}0000;")),
                Some(n) => {
                    let c = ((n + 2) / 5).min(14);
                    cx.remember(&format!("ex{a}"), format!("{c:02}"));
                    one(format!("EX{a:03}0000{c:02};"))
                }
            }
        }
        "LK" => match pnum::<u8>(p) {
            None => one("LK;"),
            Some(v @ (0 | 1)) => one(format!("LK{v}0;")),
            Some(v @ 4..=7) => one(format!("LK{}0;", v & 1)),
            _ => Vec::new(),
        },
        "MC" if read => one("MC;"),
        "MC" => match pnum::<u16>(p) {
            Some(ch) if emu && ch < 100 => one(format!("MC {ch:02};")),
            Some(ch) => one(format!("MC{ch:03};")),
            None => Vec::new(),
        },
        "MD" => mode_down(cx, read, p),
        "ML" => ml_down(cx, read, p),
        "MR" if read => one(format!("MR0{p};")),
        "MR" | "MW" => mem_down(cx, &code, p),
        "NB" | "NR" if read => one(format!("{code};")),
        "NB" | "NR" | "NL" | "RG" | "RL" if !read => one(format!("{code}{tail};")),
        "NL" | "RG" | "RL" => one(format!("{code};")),
        "PA" if read => one("PA;"),
        "PA" => one(if emu { format!("PA{tail}0;") } else { format!("PA{tail};") }),
        "PB" if read => one("PB;"),
        "PB" => one(if emu { format!("PB{tail}000;") } else { format!("PB{tail};") }),
        "PL" if read => one("PL;"),
        "PL" => {
            let out = cx.recall("plout").unwrap_or(p).to_string();
            one(format!("PL{p}{out};"))
        }
        "PR" => match p {
            "0" => one("PR;"),
            "00" | "01" => one(format!("PR{tail};")),
            _ => Vec::new(),
        },
        "RA" if read => one("RA;"),
        "RA" => one(if emu { format!("RA0{tail}00;") } else { format!("RA0{tail};") }),
        "RU" | "RD" => match pnum::<u32>(p) {
            Some(v) if p.len() == 4 => one(format!("{code}{v:05};")),
            _ => Vec::new(),
        },
        "RM" => match (p.len(), p.get(..1)) {
            (1, Some("1" | "5")) => one("SM0;"),
            (1, Some("3" | "4" | "6")) => one("RM;"),
            (4, Some(m)) => {
                let Some(v) = pnum::<u8>(&p[1..]) else { return Vec::new() };
                match m {
                    "6" => one(format!("RM1{:04};", rescale(v, &SWR_CAL, &KW_SWR_CAL))),
                    "3" => one(format!("RM2{:04};", rescale(v, &RAW_LINEAR_CAL, &KW_LINEAR_CAL))),
                    "4" => one(format!("RM3{:04};", rescale(v, &RAW_LINEAR_CAL, &KW_LINEAR_CAL))),
                    "5" => one(format!("SM0{:04};", rescale(v, &PO_CAL, &KW_PO_CAL))),
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        },
        "SC" if read => one("SC;"),
        "SC" => match pnum::<u8>(p) {
            Some(n) if emu => one(format!("SC{}0;", n.min(1))),
            Some(n) => one(format!("SC{};", n.min(1))),
            None => Vec::new(),
        },
        "SH" => width_down(cx, read, tail),
        "SM" if read => one("SM0;"),
        "SM" => match pnum::<u8>(tail) {
            Some(v) => one(format!("SM0{:04};", rescale(v, &S_METER_CAL, &KW_S_CAL))),
            None => Vec::new(),
        },
        "TX" => match p {
            "" => one("IF;"),
            "0" => {
                cx.remember("tx", "0");
                cx.remember("cattx", "0");
                one("RX;")
            }
            "1" | "2" => {
                cx.remember("tx", "1");
                if emu {
                    return one("TX0;");
                }
                cx.remember("cattx", "1");
                let rear = cur_mode(cx).is_some_and(is_data) || (sg(cx) && cx.recall("ex70") == Some("1"));
                one(if rear { "TX1;" } else { "TX0;" })
            }
            _ => Vec::new(),
        },
        "VG" => match pnum::<u32>(p) {
            None => one("VG;"),
            Some(v) => one(format!("VG{:03};", scale(v.min(100), 100, 9))),
        },
        _ => Vec::new(),
    }
}

fn up(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !p.is_ascii() {
        return Vec::new();
    }
    let c0 = at(p, 0);
    match code.as_str() {
        "AG" | "SQ" if p.len() == 1 => rd(cx, &[f]),
        "AG" | "SQ" | "CH" | "QI" | "RC" => one(f),
        // Like the radio: out of range is the nearest end.
        "PC" if p.len() == 3 => {
            let (lo, hi) = cx.model.caps.power;
            one(format!("PC{:03};", pnum::<u8>(p).unwrap_or(hi).clamp(lo, hi)))
        }
        "BY" | "ID" | "KS" | "MG" | "PC" | "PS" | "RS" | "RT" | "XT" | "TS" | "SD" | "VD" | "VX" => {
            if p.is_empty() {
                rd(cx, &[f])
            } else {
                one(f)
            }
        }
        "DN" | "UP" => one(format!("{code};")),
        "VV" => one("AB;"),
        "SV" => one("MA;"),
        "AC" if p.is_empty() => rd(cx, &["AC;"]),
        "AC" if p.len() == 3 => one(match (at(p, 1), at(p, 2)) {
            (_, '1') => "AC002;",
            ('1', _) => "AC001;",
            _ => "AC000;",
        }),
        "AI" => match p {
            "" => rd(cx, &["AI;"]),
            "0" => one("AI0;"),
            "2" | "4" => one("AI1;"),
            _ => Vec::new(),
        },
        "AN" if p.is_empty() => rd(cx, &["AN0;"]),
        "AN" if p.len() == 3 && matches!(c0, '1' | '2') => one(if emulating(cx) { format!("AN0{c0};") } else { format!("AN0{c0}0;") }),
        "BC" if p.is_empty() => rd(cx, &["BC0;"]),
        "BC" => one(format!("BC0{};", (c0 != '0') as u8)),
        "BD" | "BU" if p.len() == 2 => one(format!("BS{p};")),
        "BP" if p.is_empty() => rd(cx, &["BP01;"]),
        "BP" => match pnum::<u32>(p) {
            Some(k) => one(format!("BP01{:03};", 1 + (k.min(127) * 399 + 63) / 127)),
            None => Vec::new(),
        },
        "NT" if p.is_empty() => rd(cx, &["BP00;"]),
        "NT" => one(if c0 == '2' { "BP00001;" } else { "BP00000;" }),
        "CN" | "TN" if p.is_empty() => rd(cx, &["CN0;"]),
        "CN" | "TN" => match pnum::<u8>(p) {
            Some(k) => one(format!("CN0{:02};", rh_tone(k))),
            None => Vec::new(),
        },
        "CT" | "TO" if p.is_empty() => rd(cx, &["CT0;"]),
        "CT" | "TO" => {
            cx.remember(&code.to_ascii_lowercase(), p);
            let ct = cx.recall("ct").is_some_and(|v| v != "0");
            let to = cx.recall("to") == Some("1");
            let rh = match (code.as_str(), c0) {
                ("CT", '1' | '2') | ("TO", '0') if ct => 1,
                ("TO", '1') | ("CT", '0') if to => 2,
                _ => 0,
            };
            one(format!("CT0{rh};"))
        }
        "DA" => da_up(cx, p),
        "EX" => ex_up(cx, p),
        "FA" | "FB" if p.is_empty() => rd(cx, &[f]),
        "FA" | "FB" => {
            cx.remember(&code.to_ascii_lowercase(), p.trim_start_matches('0'));
            freq_digits(f, 8)
        }
        "FR" if p.is_empty() => rd(cx, &["FR;"]),
        "FR" => {
            cx.remember("vfo", p);
            if emulating(cx) {
                cx.remember("ft", if c0 == '1' { "1" } else { "0" });
            }
            if c0 == '1' { vec!["FR4;".into(), "VS1;".into()] } else { vec!["FR0;".into(), "VS0;".into()] }
        }
        "FT" if p.is_empty() => rd(cx, &["FT;"]),
        "FT" if matches!(c0, '0' | '1') => {
            cx.remember("ft", p);
            let n = c0 as u8 - b'0';
            one(if emulating(cx) { format!("FT{};", n + 2) } else { format!("FT{n};") })
        }
        "FW" | "SH" | "SL" => width_up(cx, &code, p),
        "SF" => sf_up(cx, p),
        "GC" => match c0 {
            '\0' => rd(cx, &["GT0;"]),
            '0' => one("GT00;"),
            '1' => one("GT03;"),
            '2' | '3' => one("GT01;"),
            _ => Vec::new(),
        },
        // RigHelm's IF has no TX state, receiver or split: ask for those first.
        "IF" if p.is_empty() => {
            cx.remember("ifq", "1");
            rd(cx, &["FR;", "FT;", "TX;", "IF;"])
        }
        "IF" => info_up(cx, p),
        "LK" if p.is_empty() => rd(cx, &["LK;"]),
        "LK" => one(format!("LK{};", 4 + (c0 == '1') as u8)),
        "MC" if p.is_empty() => rd(cx, &["MC;"]),
        "MC" => match p.replace(' ', "0").parse::<u16>() {
            Ok(ch) => one(format!("MC{ch:03};")),
            Err(_) => Vec::new(),
        },
        "MD" => md_up(cx, p),
        "ML" => ml_up(cx, p),
        "MR" if p.len() == 4 => {
            let ch = p[1..].replace(' ', "0");
            cx.remember("mr", ch.as_str());
            rd(cx, &[&format!("MR{ch};")])
        }
        "MR" | "MW" => mem_up(cx, &code, p),
        "NB" | "NR" | "PA" | "PB" | "PR" | "RA" if p.is_empty() => rd(cx, &[&format!("{code}0;")]),
        "NB" | "PA" | "PB" | "PR" => one(format!("{code}0{c0};")),
        "NR" => one(format!("NR0{};", (c0 != '0') as u8)),
        "RA" => one(format!("RA0{};", (p.get(..2) != Some("00")) as u8)),
        "NL" | "RG" | "RL" if p.is_empty() => rd(cx, &[&format!("{code}0;")]),
        "NL" | "RG" | "RL" => one(format!("{code}0{p};")),
        "PL" if p.is_empty() => rd(cx, &["PL;"]),
        "PL" if p.len() == 6 => {
            cx.remember("plout", &p[3..]);
            one(format!("PL{};", &p[..3]))
        }
        "RU" | "RD" => match pnum::<u32>(p) {
            Some(v) if p.len() == 5 => one(format!("{code}{:04};", v.min(9999))),
            _ => Vec::new(),
        },
        "RM" if p.is_empty() => rd(cx, &["RM6;", "RM3;", "RM4;"]),
        "RM" if p.len() == 5 => {
            let Some(v) = pnum::<u8>(&p[1..]) else { return Vec::new() };
            let v = v.min(30);
            match c0 {
                '1' => one(format!("RM6{:03};", rescale(v, &KW_SWR_CAL, &SWR_CAL))),
                '2' => one(format!("RM3{:03};", rescale(v, &KW_LINEAR_CAL, &RAW_LINEAR_CAL))),
                '3' => one(format!("RM4{:03};", rescale(v, &KW_LINEAR_CAL, &RAW_LINEAR_CAL))),
                _ => Vec::new(),
            }
        }
        "RX" => {
            cx.remember("tx", "0");
            one("TX0;")
        }
        "TX" => {
            cx.remember("tx", "1");
            if emulating(cx) { one("TX1;") } else { one(rh_tx(cx, true)) }
        }
        "SC" if p.is_empty() => rd(cx, &["SC;"]),
        "SC" => one(format!("SC{};", (c0 != '0') as u8)),
        "SM" if p == "0" => {
            let tx = cx.recall("tx") == Some("1");
            rd(cx, &[if tx { "RM5;" } else { "SM0;" }])
        }
        "SM" if p.len() == 5 => {
            let Some(v) = pnum::<u8>(&p[1..]) else { return Vec::new() };
            let v = v.min(30);
            if cx.recall("tx") == Some("1") {
                vec!["SM0000;".into(), "RM1000;".into(), format!("RM5{:03};", rescale(v, &KW_PO_CAL, &PO_CAL))]
            } else {
                let s = rescale(v, &KW_S_CAL, &S_METER_CAL);
                vec![format!("SM0{s:03};"), format!("RM1{s:03};"), "RM5000;".into()]
            }
        }
        "VG" if p.is_empty() => rd(cx, &["VG;"]),
        "VG" => match pnum::<u32>(p) {
            Some(k) => one(format!("VG{:03};", scale(k.min(9), 9, 100))),
            None => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// A mode as the `SF` command's code (DATA modes C-F).
pub fn sf_mode(m: Mode) -> char {
    match kw_mode(m) {
        Some(('1', Some(true))) => 'C',
        Some(('2', Some(true))) => 'D',
        Some(('4', Some(true))) => 'E',
        Some(('5', Some(true))) => 'F',
        Some((k, _)) => k,
        None => '2',
    }
}

/// The radio's `SF` (a VFO's frequency and mode, from a program): reads
/// are answered from RigHelm's `IF` (VFO A) or `FB` (VFO B).
pub fn sf_up(cx: &mut Ctx, p: &str) -> Vec<String> {
    let vfo = at(p, 0);
    if !matches!(vfo, '0' | '1') {
        return Vec::new();
    }
    if p.len() == 1 {
        cx.remember("sf", vfo.to_string());
        return rd(cx, &[if vfo == '0' { "IF;" } else { "FB;" }]);
    }
    let (Some(hz), Some(c)) = (p.get(1..12).and_then(pnum::<u64>), p.as_bytes().get(12).map(|&b| b as char)) else { return Vec::new() };
    let (md, data) = match c {
        'C' => ('1', true),
        'D' => ('2', true),
        'E' => ('4', true),
        'F' => ('5', true),
        k => (k, false),
    };
    let Some(m) = rh_mode(md, data) else { return Vec::new() };
    let mut out = vec![format!("F{}{hz:08};", if vfo == '0' { 'A' } else { 'B' })];
    // RigHelm sets the mode of the VFO in use.
    if recall_char(cx, "vfo").unwrap_or('0') == vfo {
        cx.remember("md", md.to_string());
        if has_data(md) {
            cx.remember("da", if data { "1" } else { "0" });
        }
        out.push(format!("MD0{};", m.code()));
    }
    out
}

/// RigHelm `MD0x` Set/Answer or `MD0;` Read.
pub fn mode_down(cx: &mut Ctx, read: bool, p: &str) -> Vec<String> {
    if read {
        return vec!["MD;".into(), "DA;".into()];
    }
    let Some(m) = Mode::from_code(at(p, 1)) else { return Vec::new() };
    let Some((k, d)) = kw_mode(m).filter(|_| cx.model.modes.contains(&m)) else { return Vec::new() };
    cx.remember("md", k.to_string());
    match d {
        Some(d) => {
            cx.remember("da", if d { "1" } else { "0" });
            vec![format!("MD{k};"), format!("DA{};", d as u8)]
        }
        // The radio answers DA0 outside the DATA-capable modes.
        None if emulating(cx) => vec![format!("MD{k};"), "DA0;".into()],
        None => one(format!("MD{k};")),
    }
}

/// The radio's `MD`.
pub fn md_up(cx: &mut Ctx, p: &str) -> Vec<String> {
    if p.is_empty() {
        return rd(cx, &["MD0;"]);
    }
    let k = at(p, 0);
    cx.remember("md", k.to_string());
    let data = cx.recall("da") == Some("1") && has_data(k);
    rh_mode(k, data).map(|m| one(format!("MD0{};", m.code()))).unwrap_or_default()
}

/// The radio's `DA`: RigHelm's mode with the last `MD`. An answer that
/// changes nothing gives nothing (the `MD` answer before it said it all).
pub fn da_up(cx: &mut Ctx, p: &str) -> Vec<String> {
    if p.is_empty() {
        return rd(cx, &["MD0;"]);
    }
    // Not in CW/FSK (the radio refuses DA1 there and answers DA0).
    let Some(k) = recall_char(cx, "md").filter(|&k| has_data(k)) else { return Vec::new() };
    let changed = cx.recall("da") != Some(p);
    cx.remember("da", p);
    if !(changed || emulating(cx)) {
        return Vec::new();
    }
    rh_mode(k, p == "1").map(|m| one(format!("MD0{};", m.code()))).unwrap_or_default()
}

fn width_down(cx: &mut Ctx, read: bool, idx: &str) -> Vec<String> {
    if !read {
        if let Some(q) = cx.memo.remove("wq") {
            // RigHelm knows one width; the radio's other edge is shown as
            // its default (SH shift 1800/1750 Hz, SL low cut 0, FW 0).
            return one(match q.as_str() {
                "SH" => "SH08;",
                "SL" => "SL00;",
                _ => "FW0000;",
            });
        }
    }
    let Some(c) = width_cmd(cx) else { return Vec::new() };
    if read {
        return one(format!("{c};"));
    }
    match pnum::<usize>(idx).filter(|&i| i < WIDTH.len()) {
        Some(i) if c == "FW" => one(format!("FW{:04};", WIDTH[i])),
        Some(i) => one(format!("{c}{i:02};")),
        None => Vec::new(),
    }
}

fn width_up(cx: &mut Ctx, code: &str, p: &str) -> Vec<String> {
    if p.is_empty() {
        cx.remember("emu", "1");
        // A program reading the radio's other filter edge (Hamlib reads SH
        // and SL in every mode): answered with a fixed value, see width_down.
        if width_cmd(cx) != Some(code) {
            cx.remember("wq", code);
        }
        return one("SH0;");
    }
    if width_cmd(cx) != Some(code) {
        return Vec::new();
    }
    let Some(v) = pnum::<u32>(p) else { return Vec::new() };
    let i = if code == "FW" { WIDTH.iter().rposition(|&w| w <= v).unwrap_or(0) as u32 } else { v.min(13) };
    one(format!("SH0{i:02};"))
}

fn ml_down(cx: &mut Ctx, read: bool, p: &str) -> Vec<String> {
    if read {
        return one("ML;");
    }
    let Some(v) = pnum::<u32>(&p[1..]) else { return Vec::new() };
    let max = ml_max(cx);
    if p.starts_with('0') {
        cx.remember("ml0", if v > 0 { "1" } else { "0" });
        if v == 0 {
            return one("ML000;");
        }
        let k = cx.recall("ml").and_then(|k| k.parse().ok()).unwrap_or(max / 2);
        return one(format!("ML{k:03};"));
    }
    let k = scale(v.min(100), 100, max).clamp(1, max);
    cx.remember("ml", k.to_string());
    match cx.recall("ml0") {
        // Setting a level would turn the monitor on.
        Some("0") if emulating(cx) => one("ML000;"),
        Some("0") => Vec::new(),
        _ => one(format!("ML{k:03};")),
    }
}

fn ml_up(cx: &mut Ctx, p: &str) -> Vec<String> {
    if p.is_empty() {
        return rd(cx, &["ML1;"]);
    }
    let Some(k) = pnum::<u32>(p) else { return Vec::new() };
    let max = ml_max(cx);
    if k == 0 {
        cx.remember("ml0", "0");
        let k = cx.recall("ml").and_then(|k| k.parse().ok()).unwrap_or(max / 2);
        return vec!["ML0000;".into(), format!("ML1{:03};", scale(k, max, 100).max(1))];
    }
    cx.remember("ml0", "1");
    cx.remember("ml", k.to_string());
    vec!["ML0001;".into(), format!("ML1{:03};", scale(k.min(max), max, 100).max(1))]
}

fn ex_down(cx: &mut Ctx, p: &str) -> Vec<String> {
    let Some(item) = p.get(..3).and_then(|n| n.parse::<u16>().ok()).and_then(|n| cx.model.menu_item(n)) else { return Vec::new() };
    let raw = &p[3..];
    if raw.is_empty() {
        return one(format!("EX{:03}0000;", item.addr));
    }
    cx.remember(&format!("ex{}", item.addr), raw);
    one(format!("EX{:03}0000{raw};", item.addr))
}

fn ex_up(cx: &mut Ctx, p: &str) -> Vec<String> {
    let Some(addr) = p.get(..3).and_then(|a| a.parse::<u32>().ok()) else { return Vec::new() };
    let Some(item) = cx.model.menu.iter().find(|m| m.addr == addr) else { return Vec::new() };
    let raw = p.get(7..).unwrap_or("");
    if raw.is_empty() {
        return rd(cx, &[&format!("EX{:03};", item.num)]);
    }
    cx.remember(&format!("ex{addr}"), raw);
    let mut out = one(format!("EX{:03}{raw};", item.num));
    if addr == pitch_addr(cx) {
        if let Some(c) = pnum::<u32>(raw) {
            out.push(format!("KP{:02};", c.min(14) * 5));
        }
    }
    out
}

/// The radio's `IF` answer (35 characters) as RigHelm's `IF` (or `OI`
/// when VFO B is in use), `TX` and `CT0`.
fn info_up(cx: &mut Ctx, p: &str) -> Vec<String> {
    if p.len() < 35 {
        return Vec::new();
    }
    let (Some(freq), Some(clar)) = (pnum::<u64>(&p[0..11]), pnum::<i32>(&p[16..21])) else { return Vec::new() };
    let rit = at(p, 21) == '1';
    let xit = at(p, 22) == '1';
    let ch: u16 = p[23..26].replace(' ', "0").parse().unwrap_or(0);
    let tx = at(p, 26) == '1';
    let md = at(p, 27);
    let func = at(p, 28);
    cx.remember("md", md.to_string());
    cx.remember("vfo", func.to_string());
    cx.remember("tx", if tx { "1" } else { "0" });
    if at(p, 30) == '0' && func != '2' {
        cx.remember("ft", func.to_string());
    }
    let mode = rh_mode(md, cx.recall("da") == Some("1") && has_data(md)).map(|m| m.code()).unwrap_or('0');
    let ctcss = rh_tone_mode(at(p, 31));
    let tone = rh_tone(p[32..34].parse().unwrap_or(0));
    let mem = (func == '2') as u8;
    let mut out = Vec::new();
    if func == '1' {
        // VFO B's: RigHelm's IF is VFO A.
        out.push(rh_info("OI", ch, freq, clar, rit, xit, mode, mem, ctcss, tone));
        if let Some(fa) = cx.recall("fa").and_then(|f| f.parse::<u64>().ok()) {
            out.push(rh_info("IF", ch, fa, clar, rit, xit, '0', 0, ctcss, tone));
        }
    } else {
        if func == '0' {
            cx.remember("fa", freq.to_string());
        }
        out.push(rh_info("IF", ch, freq, clar, rit, xit, mode, mem, ctcss, tone));
    }
    out.push(rh_tx(cx, tx));
    out.push(format!("CT0{ctcss};"));
    out
}

/// RigHelm's `IF` answer as the radio's (for a program).
fn info_down(cx: &mut Ctx, p: &str) -> Vec<String> {
    let Some(info) = ChannelInfo::parse(p) else { return Vec::new() };
    let (md, data) = info.mode.and_then(kw_mode).unwrap_or(('2', None));
    cx.remember("md", md.to_string());
    if let Some(d) = data {
        cx.remember("da", if d { "1" } else { "0" });
    }
    let tx = cx.recall("tx") == Some("1");
    let func = if info.mem_mode != 0 { '2' } else { recall_char(cx, "vfo").unwrap_or('0') };
    let sign = if info.clar_offset < 0 { '-' } else { '+' };
    one(format!(
        "IF{:011}     {sign}{:04}{}{}{:03}{}{md}{func}0{}{}{:02}0;",
        info.freq,
        info.clar_offset.unsigned_abs().min(9999),
        info.rx_clar as u8,
        info.tx_clar as u8,
        info.channel,
        tx as u8,
        kw_split(cx) as u8,
        kw_tone_mode(info.ctcss),
        kw_tone(info.tone)
    ))
}

/// The radio's `MR` answer or `MW` set as RigHelm's.
fn mem_up(_cx: &mut Ctx, code: &str, p: &str) -> Vec<String> {
    if p.len() < 39 || at(p, 0) != '0' {
        return Vec::new();
    }
    let (Ok(ch), Some(freq)) = (p[1..4].replace(' ', "0").parse::<u16>(), pnum::<u64>(&p[4..15])) else { return Vec::new() };
    if freq == 0 {
        // An empty channel.
        return if code == "MR" { one("?;") } else { Vec::new() };
    }
    let Some(mode) = rh_mode(at(p, 15), at(p, 16) == '1') else { return Vec::new() };
    let tmode = at(p, 17);
    let tone = rh_tone(p[if tmode == '1' { 18..20 } else { 20..22 }].parse().unwrap_or(0));
    let info = ChannelInfo { channel: ch, freq, mode: Some(mode), mem_mode: 1, ctcss: rh_tone_mode(tmode), tone, ..Default::default() };
    if code == "MW" { one(info.to_mw(ch)) } else { one(info.to_answer("MR")) }
}

/// RigHelm's `MR` answer or `MW` set as the radio's.
fn mem_down(cx: &mut Ctx, code: &str, p: &str) -> Vec<String> {
    let Some(info) = ChannelInfo::parse(p) else { return Vec::new() };
    let ch = match code {
        "MR" => cx.recall("mr").and_then(|c| c.parse().ok()).unwrap_or(info.channel),
        _ => info.channel,
    };
    let Some((md, data)) = info.mode.and_then(kw_mode) else { return Vec::new() };
    let chs = if code == "MR" && ch < 100 { format!(" {ch:02}") } else { format!("{ch:03}") };
    let k = kw_tone(info.tone);
    one(format!("{code}0{chs}{:011}{md}{}{}{k:02}{k:02}{};", info.freq, data.unwrap_or(false) as u8, kw_tone_mode(info.ctcss), "0".repeat(17)))
}

// ------------------------------------------------------------- tables

const OFFON: Kind = Kind::Choice(&["OFF", "ON"]);
const LVL9: Kind = num(0, 9, 1, "");
const VOL20: Kind = Kind::Steps { max_code: 20, off_at_zero: true, first: 1, step: 1, unit: "" };
const VOL9: Kind = Kind::Steps { max_code: 9, off_at_zero: true, first: 1, step: 1, unit: "" };
const BAUD: Kind = Kind::Choice(&["4800 bps", "9600 bps", "19200 bps", "38400 bps", "57600 bps", "115200 bps"]);
const PF: Kind = num(0, 255, 1, "");
const TX_LO: Kind = Kind::Choice(&["10 Hz", "100 Hz", "200 Hz", "300 Hz", "400 Hz", "500 Hz"]);
const TX_HI: Kind = Kind::Choice(&["2500 Hz", "2600 Hz", "2700 Hz", "2800 Hz", "2900 Hz", "3000 Hz"]);
const TX_EQ: Kind = Kind::Choice(&["OFF", "HB1", "HB2", "FP", "BB1", "BB2", "C", "U"]);
const RX_EQ: Kind = Kind::Choice(&["OFF", "HB1", "HB2", "FP", "BB1", "BB2", "FLAT", "U"]);
const PITCH: Kind = Kind::Steps { max_code: 14, off_at_zero: false, first: 300, step: 50, unit: "Hz" };
const WEIGHT: Kind = Kind::Coded(&[
    (0, "AUTO"),
    (1, "2.5"),
    (2, "2.6"),
    (3, "2.7"),
    (4, "2.8"),
    (5, "2.9"),
    (6, "3.0"),
    (7, "3.1"),
    (8, "3.2"),
    (9, "3.3"),
    (10, "3.4"),
    (11, "3.5"),
    (12, "3.6"),
    (13, "3.7"),
    (14, "3.8"),
    (15, "3.9"),
    (16, "4.0"),
]);
const VOX_DELAY: Kind = Kind::Steps { max_code: 20, off_at_zero: false, first: 0, step: 5, unit: "" };
const AM_FM_STEP: Kind = Kind::Choice(&["5 kHz", "6.25 kHz", "10 kHz", "12.5 kHz", "15 kHz", "20 kHz", "25 kHz", "30 kHz", "50 kHz", "100 kHz"]);
const AM_FM_STEP_SG: Kind =
    Kind::Choice(&["OFF", "5 kHz", "6.25 kHz", "10 kHz", "12.5 kHz", "15 kHz", "20 kHz", "25 kHz", "30 kHz", "50 kHz", "100 kHz"]);
// The reference lists 0.5 twice.
const SSB_STEP_SG: Kind = Kind::Choice(&["OFF", "0.5 kHz", "0.5 kHz (2)", "1 kHz", "2.5 kHz", "5 kHz", "10 kHz"]);
const FILTER_CTRL: Kind = Kind::Choice(&["1 (HI/LO)", "2 (WIDTH/SHIFT)"]);
const PSQ_OUT: Kind = Kind::Choice(&["OFF", "BSY", "SQL", "SND", "BSY-SND", "SQL-SND"]);
const APO: Kind = Kind::Choice(&["OFF", "60 min", "120 min", "180 min"]);
const TOT: Kind = Kind::Choice(&["OFF", "3 min", "5 min", "10 min", "20 min", "30 min"]);

const PANEL: &str = "DISPLAY/PANEL";
const AUDIO: &str = "AUDIO/VOICE";
const TUNING: &str = "TUNING";
const MEMORY: &str = "MEMORY";
const SCAN: &str = "SCAN";
const DSP: &str = "FILTER/DSP";
const CW: &str = "CW";
const FSK: &str = "FSK";
const TX: &str = "TX";
const MSG: &str = "VOICE/MESSAGE";
const SPLIT: &str = "SPLIT";
const COM: &str = "COM/USB";
const DATA: &str = "DATA/ACC2";
const IFACE: &str = "INTERFACE";
const PFK: &str = "PF KEYS";

/// TS-590SG menu 000-099 (RigHelm number = menu number + 1).
pub static MENU_SG: &[MenuItem] = &[
    item(1, 0, PANEL, "Firmware version (read only)", 4, Kind::Text { max: 4 }),
    item(2, 1, PANEL, "Power on message", 8, Kind::Text { max: 8 }),
    item(3, 2, PANEL, "Display brightness", 1, Kind::Choice(&["OFF", "1", "2", "3", "4", "5", "6"])),
    item(4, 3, PANEL, "Back light color", 1, Kind::Choice(&["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"])),
    item(5, 4, PANEL, "Panel key response for double function", 1, Kind::Choice(&["1", "2", "3"])),
    item(6, 5, AUDIO, "Beep volume", 2, VOL20),
    item(7, 6, AUDIO, "Sidetone volume", 2, VOL20),
    item(8, 7, AUDIO, "Message playback volume", 2, VOL20),
    item(9, 8, AUDIO, "Voice guide volume", 2, VOL20),
    item(10, 9, AUDIO, "Voice guide speed", 1, Kind::Choice(&["0", "1", "2", "3", "4"])),
    item(11, 10, AUDIO, "Voice guide language", 1, Kind::Choice(&["EN", "JP"])),
    item(12, 11, AUDIO, "Auto announcement", 1, Kind::Choice(&["OFF", "1", "2"])),
    item(13, 12, TUNING, "MHz step", 1, Kind::Choice(&["0.1 MHz", "0.5 MHz", "1 MHz"])),
    item(14, 13, TUNING, "Tuning control adjustment rate", 1, Kind::Choice(&["250 Hz", "500 Hz", "1000 Hz"])),
    item(15, 14, TUNING, "MULTI/CH control rounding off", 1, OFFON),
    item(16, 15, TUNING, "Dedicated step inside the BC band (AM)", 1, OFFON),
    item(17, 16, TUNING, "MULTI/CH step for SSB", 1, SSB_STEP_SG),
    item(18, 17, TUNING, "MULTI/CH step for CW/FSK", 1, SSB_STEP_SG),
    item(19, 18, TUNING, "MULTI/CH step for AM", 2, AM_FM_STEP_SG),
    item(20, 19, TUNING, "MULTI/CH step for FM", 2, AM_FM_STEP_SG),
    item(21, 20, SPLIT, "Shiftable RX frequency during split TX", 1, OFFON),
    item(22, 21, MEMORY, "Maximum number of Quick Memory channels", 1, Kind::Choice(&["3", "5", "10"])),
    item(23, 22, MEMORY, "Temporary change of memory frequency", 1, OFFON),
    item(24, 23, SCAN, "Program Scan slow down", 1, OFFON),
    item(25, 24, SCAN, "Program Scan slow down range", 1, Kind::Choice(&["100 Hz", "200 Hz", "300 Hz", "400 Hz", "500 Hz"])),
    item(26, 25, SCAN, "Program Scan hold", 1, OFFON),
    item(27, 26, SCAN, "Scan resume method", 1, Kind::Choice(&["TO", "CO"])),
    item(28, 27, TUNING, "Auto mode change", 1, OFFON),
    item(29, 28, DSP, "Low/High cut or Width/Shift (SSB)", 1, FILTER_CTRL),
    item(30, 29, DSP, "Low/High cut or Width/Shift (SSB-DATA)", 1, FILTER_CTRL),
    item(31, 30, DSP, "AUTO NOTCH following speed", 1, Kind::Choice(&["0", "1", "2", "3", "4"])),
    item(32, 31, DSP, "SSB/AM TX low cut", 1, TX_LO),
    item(33, 32, DSP, "SSB/AM TX high cut", 1, TX_HI),
    item(34, 33, DSP, "SSB-DATA TX low cut", 1, TX_LO),
    item(35, 34, DSP, "SSB-DATA TX high cut", 1, TX_HI),
    item(36, 35, DSP, "Speech processor effect", 1, Kind::Choice(&["SOFT", "HARD"])),
    item(37, 36, DSP, "TX equalizer", 1, TX_EQ),
    item(38, 37, DSP, "RX equalizer", 1, RX_EQ),
    item(39, 38, CW, "Electronic keyer mode", 1, Kind::Choice(&["A", "B"])),
    item(40, 39, CW, "Insert keying", 1, OFFON),
    item(41, 40, CW, "Sidetone / pitch frequency", 2, PITCH),
    item(42, 41, CW, "CW rise time", 1, Kind::Choice(&["1 ms", "2 ms", "4 ms", "6 ms"])),
    item(43, 42, CW, "Keying weight ratio", 2, WEIGHT),
    item(44, 43, CW, "Reverse keying auto weight", 1, OFFON),
    item(45, 44, CW, "Bug key function", 1, OFFON),
    item(46, 45, CW, "Paddle dot/dash swap", 1, OFFON),
    item(47, 46, CW, "Mic paddle function", 1, Kind::Choice(&["PF", "PA"])),
    item(48, 47, CW, "Auto CW TX in SSB mode", 1, OFFON),
    item(49, 48, CW, "Frequency correction SSB to CW", 1, OFFON),
    item(50, 49, CW, "Break-in null when setting keying speed", 1, OFFON),
    item(51, 50, FSK, "FSK shift", 1, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz", "850 Hz"])),
    item(52, 51, FSK, "FSK keying polarity", 1, OFFON),
    item(53, 52, FSK, "FSK tone frequency", 1, Kind::Choice(&["1275 Hz", "2125 Hz"])),
    item(54, 53, TX, "Mic gain for FM", 1, Kind::Choice(&["1", "2", "3"])),
    item(55, 54, TX, "Power fine", 1, OFFON),
    item(56, 55, TX, "Time-out timer", 1, TOT),
    item(57, 56, TX, "Transverter and power down", 1, Kind::Choice(&["OFF", "1", "2"])),
    item(58, 57, TX, "TX hold when AT completes", 1, OFFON),
    item(59, 58, TX, "AT operation when receiving", 1, OFFON),
    item(60, 59, TX, "HF linear amplifier control", 1, Kind::Choice(&["OFF", "1", "2", "3", "4", "5"])),
    item(61, 60, TX, "50 MHz linear amplifier control", 1, Kind::Choice(&["OFF", "1", "2", "3", "4", "5"])),
    item(62, 61, MSG, "Constant recording", 1, OFFON),
    item(63, 62, MSG, "Voice/message playback repeat", 1, OFFON),
    item(64, 63, MSG, "Voice/message repeat interval", 2, num(0, 60, 1, "s")),
    // The reference's labels are garbled; this reading is a best guess.
    item(65, 64, SPLIT, "Split transfer function", 1, Kind::Choice(&["OFF", "A-TR", "A-SUB R", "B"])),
    item(66, 65, SPLIT, "Write split transfer data to the VFO", 1, OFFON),
    item(67, 66, TX, "Transmit inhibit", 1, OFFON),
    item(68, 67, COM, "COM port speed", 1, BAUD),
    item(69, 68, COM, "USB port speed", 1, BAUD),
    item(70, 69, DATA, "DATA modulation line", 1, Kind::Choice(&["ACC2", "USB"])),
    item(71, 70, DATA, "Audio source of SEND/PTT in DATA mode", 1, Kind::Choice(&["FRONT", "REAR"])),
    item(72, 71, DATA, "USB audio input level", 1, LVL9),
    item(73, 72, DATA, "USB audio output level", 1, LVL9),
    item(74, 73, DATA, "ACC2 AF input level", 1, LVL9),
    item(75, 74, DATA, "ACC2 AF output level", 1, LVL9),
    item(76, 75, DATA, "External AF output beep mix", 1, OFFON),
    item(77, 76, DATA, "DATA VOX", 1, OFFON),
    item(78, 77, DATA, "DATA VOX delay", 2, VOX_DELAY),
    item(79, 78, DATA, "DATA VOX gain (USB audio)", 1, LVL9),
    item(80, 79, DATA, "DATA VOX gain (ACC2)", 1, LVL9),
    item(81, 80, IFACE, "PKS polarity reverse", 1, OFFON),
    item(82, 81, IFACE, "Busy transmit inhibit", 1, OFFON),
    item(83, 82, IFACE, "CTCSS mute operation", 1, Kind::Choice(&["1", "2"])),
    item(84, 83, IFACE, "PSQ control signal logic", 1, Kind::Choice(&["LO", "OPEN"])),
    item(85, 84, IFACE, "PSQ control signal output condition", 1, PSQ_OUT),
    item(86, 85, IFACE, "DRV connector output function", 1, Kind::Choice(&["DRO", "ANT"])),
    item(87, 86, IFACE, "APO function", 1, APO),
    item(88, 87, PFK, "Panel PF A function", 3, PF),
    item(89, 88, PFK, "Panel PF B function", 3, PF),
    item(90, 89, PFK, "RIT key function", 3, PF),
    item(91, 90, PFK, "XIT key function", 3, PF),
    item(92, 91, PFK, "CL key function", 3, PF),
    item(93, 92, PFK, "MULTI/CH key (except CW)", 3, PF),
    item(94, 93, PFK, "MULTI/CH key (CW)", 3, PF),
    item(95, 94, PFK, "Mic PF 1 function", 3, PF),
    item(96, 95, PFK, "Mic PF 2 function", 3, PF),
    item(97, 96, PFK, "Mic PF 3 function", 3, PF),
    item(98, 97, PFK, "Mic PF 4 function", 3, PF),
    item(99, 98, PFK, "Mic PF (DWN) function", 3, PF),
    item(100, 99, PFK, "Mic PF (UP) function", 3, PF),
];

/// TS-590S menu 000-087 (RigHelm number = menu number + 1).
pub static MENU_S: &[MenuItem] = &[
    item(1, 0, PANEL, "Display brightness", 1, Kind::Choice(&["OFF", "1", "2", "3", "4", "5", "6"])),
    item(2, 1, PANEL, "Back light color", 1, Kind::Choice(&["1", "2"])),
    item(3, 2, PANEL, "Panel key response for double function", 1, Kind::Choice(&["1", "2", "3"])),
    item(4, 3, AUDIO, "Beep volume", 1, VOL9),
    item(5, 4, AUDIO, "Sidetone volume", 1, VOL9),
    item(6, 5, AUDIO, "Message playback volume", 1, VOL9),
    item(7, 6, AUDIO, "Voice guide volume", 1, Kind::Steps { max_code: 7, off_at_zero: true, first: 1, step: 1, unit: "" }),
    item(8, 7, AUDIO, "Voice guide speed", 1, Kind::Choice(&["0", "1", "2", "3", "4"])),
    item(9, 8, AUDIO, "Voice guide language", 1, Kind::Choice(&["EN", "JP"])),
    item(10, 9, AUDIO, "Auto announcement", 1, OFFON),
    item(11, 10, TUNING, "MHz step", 1, Kind::Choice(&["0.1 MHz", "0.5 MHz", "1 MHz"])),
    item(12, 11, TUNING, "Tuning control adjustment rate", 1, Kind::Choice(&["250 Hz", "500 Hz", "1000 Hz"])),
    item(13, 12, TUNING, "MULTI/CH control rounding off", 1, OFFON),
    item(14, 13, TUNING, "Dedicated step inside the BC band (AM)", 1, OFFON),
    item(15, 14, TUNING, "MULTI/CH step for SSB/CW/FSK", 1, Kind::Choice(&["0.5 kHz", "1 kHz", "2.5 kHz", "5 kHz", "10 kHz"])),
    item(16, 15, TUNING, "MULTI/CH step for AM", 1, AM_FM_STEP),
    item(17, 16, TUNING, "MULTI/CH step for FM", 1, AM_FM_STEP),
    item(18, 17, MEMORY, "Maximum number of Quick Memory channels", 1, Kind::Choice(&["3", "5", "10"])),
    item(19, 18, MEMORY, "Temporary change of memory frequency", 1, OFFON),
    item(20, 19, SCAN, "Program Scan slow down", 1, OFFON),
    item(21, 20, SCAN, "Program Scan slow down range", 1, Kind::Choice(&["100 Hz", "200 Hz", "300 Hz", "400 Hz", "500 Hz"])),
    item(22, 21, SCAN, "Program Scan hold", 1, OFFON),
    item(23, 22, SCAN, "Scan resume method", 1, Kind::Choice(&["TO", "CO"])),
    item(24, 23, TUNING, "Auto mode change", 1, OFFON),
    item(25, 24, DSP, "AUTO NOTCH following speed", 1, Kind::Choice(&["0", "1", "2", "3", "4"])),
    item(26, 25, DSP, "SSB/AM TX low cut", 1, TX_LO),
    item(27, 26, DSP, "SSB/AM TX high cut", 1, TX_HI),
    item(28, 27, DSP, "SSB-DATA TX low cut", 1, TX_LO),
    item(29, 28, DSP, "SSB-DATA TX high cut", 1, TX_HI),
    item(30, 29, DSP, "Speech processor effect", 1, Kind::Choice(&["SOFT", "HARD"])),
    item(31, 30, DSP, "TX equalizer", 1, TX_EQ),
    item(32, 31, DSP, "RX equalizer", 1, RX_EQ),
    item(33, 32, CW, "Electronic keyer mode", 1, Kind::Choice(&["A", "B"])),
    item(34, 33, CW, "Insert keying", 1, OFFON),
    item(35, 34, CW, "Sidetone / pitch frequency", 2, PITCH),
    item(36, 35, CW, "CW rise time", 1, Kind::Choice(&["1 ms", "2 ms", "4 ms", "6 ms"])),
    item(37, 36, CW, "Keying weight ratio", 2, WEIGHT),
    item(38, 37, CW, "Reverse keying auto weight", 1, OFFON),
    item(39, 38, CW, "Bug key function", 1, OFFON),
    item(40, 39, CW, "Paddle dot/dash swap", 1, OFFON),
    item(41, 40, CW, "Mic paddle function", 1, Kind::Choice(&["PF", "PA"])),
    item(42, 41, CW, "Auto CW TX in SSB mode", 1, OFFON),
    item(43, 42, CW, "Frequency correction SSB to CW", 1, OFFON),
    item(44, 43, CW, "Break-in null when setting keying speed", 1, OFFON),
    item(45, 44, FSK, "FSK shift", 1, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz", "850 Hz"])),
    item(46, 45, FSK, "FSK keying polarity", 1, OFFON),
    item(47, 46, FSK, "FSK tone frequency", 1, Kind::Choice(&["1275 Hz", "2125 Hz"])),
    item(48, 47, TX, "Mic gain for FM", 1, Kind::Choice(&["1", "2", "3"])),
    item(49, 48, TX, "Power fine", 1, OFFON),
    item(50, 49, TX, "Time-out timer", 1, TOT),
    item(51, 50, TX, "Transverter and power down", 1, Kind::Choice(&["OFF", "1", "2"])),
    item(52, 51, TX, "TX hold when AT completes", 1, OFFON),
    item(53, 52, TX, "AT operation when receiving", 1, OFFON),
    item(54, 53, TX, "HF linear amplifier control", 1, Kind::Choice(&["OFF", "1", "2", "3"])),
    item(55, 54, TX, "50 MHz linear amplifier control", 1, Kind::Choice(&["OFF", "1", "2", "3"])),
    item(56, 55, MSG, "Constant recording", 1, OFFON),
    item(57, 56, MSG, "Voice/message playback repeat", 1, OFFON),
    item(58, 57, MSG, "Voice/message repeat interval", 2, num(0, 60, 1, "s")),
    item(59, 58, SPLIT, "Split transfer function", 1, OFFON),
    item(60, 59, SPLIT, "Write split transfer data to the VFO", 1, OFFON),
    item(61, 60, TX, "Transmit inhibit", 1, OFFON),
    item(62, 61, COM, "COM port speed", 1, BAUD),
    item(63, 62, COM, "USB port speed", 1, BAUD),
    item(64, 63, DATA, "DATA modulation line", 1, Kind::Choice(&["ACC2", "USB"])),
    item(65, 64, DATA, "USB audio input level", 1, LVL9),
    item(66, 65, DATA, "USB audio output level", 1, LVL9),
    item(67, 66, DATA, "ACC2 AF input level", 1, LVL9),
    item(68, 67, DATA, "ACC2 AF output level", 1, LVL9),
    item(69, 68, DATA, "External AF output beep mix", 1, OFFON),
    item(70, 69, DATA, "DATA VOX", 1, OFFON),
    item(71, 70, DATA, "DATA VOX delay", 2, VOX_DELAY),
    item(72, 71, DATA, "DATA VOX gain (USB audio)", 1, LVL9),
    item(73, 72, DATA, "DATA VOX gain (ACC2)", 1, LVL9),
    item(74, 73, IFACE, "PKS polarity reverse", 1, OFFON),
    item(75, 74, IFACE, "Busy transmit inhibit", 1, OFFON),
    item(76, 75, IFACE, "CTCSS mute operation", 1, Kind::Choice(&["1", "2"])),
    item(77, 76, IFACE, "PSQ control signal logic", 1, Kind::Choice(&["LO", "OPEN"])),
    item(78, 77, IFACE, "PSQ control signal output condition", 1, PSQ_OUT),
    item(79, 78, IFACE, "APO function", 1, APO),
    item(80, 79, PFK, "Panel PF A function", 3, PF),
    item(81, 80, PFK, "Panel PF B function", 3, PF),
    item(82, 81, PFK, "Mic PF 1 function", 3, PF),
    item(83, 82, PFK, "Mic PF 2 function", 3, PF),
    item(84, 83, PFK, "Mic PF 3 function", 3, PF),
    item(85, 84, PFK, "Mic PF 4 function", 3, PF),
    item(86, 85, PFK, "Mic PF (DWN) function", 3, PF),
    item(87, 86, PFK, "Mic PF (UP) function", 3, PF),
    item(88, 87, PANEL, "Power on message", 8, Kind::Text { max: 8 }),
];

/// Band keys (`BU`/`BD` codes); 10 is GENE (general coverage).
pub static BANDS: &[Band] = &[
    Band { code: 0, label: "160", lo: 1_800_000, hi: 2_000_000 },
    Band { code: 1, label: "80", lo: 3_500_000, hi: 4_000_000 },
    Band { code: 2, label: "40", lo: 7_000_000, hi: 7_300_000 },
    Band { code: 3, label: "30", lo: 10_100_000, hi: 10_150_000 },
    Band { code: 4, label: "20", lo: 14_000_000, hi: 14_350_000 },
    Band { code: 5, label: "17", lo: 18_068_000, hi: 18_168_000 },
    Band { code: 6, label: "15", lo: 21_000_000, hi: 21_450_000 },
    Band { code: 7, label: "12", lo: 24_890_000, hi: 24_990_000 },
    Band { code: 8, label: "10", lo: 28_000_000, hi: 29_700_000 },
    Band { code: 9, label: "6", lo: 50_000_000, hi: 54_000_000 },
    Band { code: 10, label: "GEN", lo: 30_000, hi: 60_000_000 },
];

pub static MODES: &[Mode] = &[
    Mode::Lsb,
    Mode::Usb,
    Mode::Cw,
    Mode::CwR,
    Mode::RttyLsb,
    Mode::RttyUsb,
    Mode::Fm,
    Mode::Am,
    Mode::DataLsb,
    Mode::DataUsb,
    Mode::DataFm,
    Mode::DataAm,
];

/// The RigHelm commands the TS-590 can carry out.
const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AN", "BC", "BP", "BS", "BY", "CH", "CN", "CT", "DN", "EX", "FA", "FB", "FR", "FT", "GT", "ID", "IF",
    "KP", "KS", "LK", "MA", "MC", "MD", "MG", "ML", "MR", "MW", "NB", "NL", "NR", "PA", "PB", "PC", "PL", "PR", "PS", "QI", "RA",
    "RC", "RD", "RG", "RL", "RM", "RS", "RT", "RU", "SC", "SD", "SH", "SM", "SQ", "TS", "TX", "UP", "VD", "VG", "VS", "VX", "XT",
];

const fn caps(last_memory: u16) -> Caps {
    Caps {
        commands: COMMANDS,
        widths,
        narrow: false,
        preamp: &[(0, "OFF"), (1, "ON")],
        att: &[(0, "OFF"), (1, "ON")],
        agc: &[(0, "OFF"), (1, "FAST"), (3, "SLOW")],
        roofing: &[],
        roofing_answers: &[],
        nb: &[(0, "OFF"), (1, "NB1"), (2, "NB2"), (3, "NB1+NB2")],
        nr_max: 10,
        nb_level_max: 10,
        antennas: 2,
        power: (5, 100),
        meter_select: &[],
        voice_memories: 4,
        keyer_memories: 0,
        keyer_memory_len: 0,
        // 100-109 are the program scan channels P0-P9 (each holds a lower
        // and upper edge), 110-119 the SG's extension channels E0-E9.
        memory: Memory { first: 0, last: last_memory, pms: None },
        shared_clarifier: true,
    }
}

const BAUDS: &[u32] = &[115_200, 57_600, 38_400, 19_200, 9_600, 4_800];

pub static TS590S: RadioModel = RadioModel {
    key: "ts590s",
    name: "TS-590S",
    maker: Maker::Kenwood,
    family: Family::Kenwood,
    proto: Proto::Text(&SPEC),
    id: "021",
    civ_addr: 0,
    hamlib: 2031,
    freq_min: 30_000,
    freq_max: 60_000_000,
    bands: BANDS,
    modes: MODES,
    menu: MENU_S,
    roles: Roles {
        // The S has no menu that sends the microphone or the rear input
        // on SEND/PTT: TX0 is the microphone, TX1 the rear input.
        mic_sel: &[],
        mic_front: 0,
        mic_usb: 0,
        pc_keying: None,
        cat_tot: None,
        cat_items: &[62, 63],
        cw_bfo: None,
        cw_pitch_display: None,
        // SSB-DATA always uses SL as a width (the CW/FSK width table): an
        // item whose value is never "9" keeps RigHelm on that table.
        data_other: Some((64, "9")),
        data_disp: None,
    },
    caps: caps(109),
    bauds: BAUDS,
    stop_bits: 1,
    rts_cts: false,
    sim: &[],
};

pub static TS590SG: RadioModel = RadioModel {
    key: "ts590sg",
    name: "TS-590SG",
    maker: Maker::Kenwood,
    family: Family::Kenwood,
    proto: Proto::Text(&SPEC),
    id: "023",
    civ_addr: 0,
    hamlib: 2037,
    freq_min: 30_000,
    freq_max: 60_000_000,
    bands: BANDS,
    modes: MODES,
    menu: MENU_SG,
    roles: Roles {
        // Menu 070: the audio SEND/PTT transmits in DATA mode (FRONT
        // microphone / REAR = the DATA modulation line, menu 069).
        mic_sel: &[71],
        mic_front: 0,
        mic_usb: 1,
        pc_keying: None,
        cat_tot: None,
        cat_items: &[68, 69],
        cw_bfo: None,
        cw_pitch_display: None,
        // Menu 029 = 1 (HI/LO): SSB-DATA filters like SSB.
        data_other: Some((30, "0")),
        data_disp: None,
    },
    caps: caps(119),
    bauds: BAUDS,
    stop_bits: 1,
    rts_cts: false,
    sim: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    fn cx(m: &'static RadioModel) -> Ctx {
        Ctx::new(m)
    }

    fn d(cx: &mut Ctx, f: &str) -> Vec<String> {
        down(cx, f)
    }

    fn u(cx: &mut Ctx, f: &str) -> Vec<String> {
        up(cx, f)
    }

    #[test]
    fn manual_frames_up_and_down() {
        let c = &mut cx(&TS590SG);
        // FA / FB: 11 digits.
        assert_eq!(d(c, "FA07000000;"), vec!["FA00007000000;"]);
        assert_eq!(u(c, "FA00014195000;"), vec!["FA14195000;"]);
        assert_eq!(d(c, "FB;"), vec!["FB;"]);
        // MD + DA.
        assert_eq!(d(c, "MD0C;"), vec!["MD2;", "DA1;"]);
        assert_eq!(d(c, "MD03;"), vec!["MD3;"]);
        assert_eq!(d(c, "MD0;"), vec!["MD;", "DA;"]);
        assert_eq!(u(c, "MD2;"), vec!["MD0C;"]);
        assert_eq!(u(c, "DA1;"), Vec::<String>::new());
        assert_eq!(u(c, "DA0;"), vec!["MD02;"]);
        assert_eq!(u(c, "MD6;"), vec!["MD06;"]);
        assert_eq!(u(c, "MD9;"), vec!["MD09;"]);
        // IF: "00014175000" + 5 spaces + RIT/XIT + ... (35 characters).
        assert_eq!(
            u(c, "IF00014175000     -001010000030000000;"),
            vec!["IF00014175000-001010300000;", "TX0;", "CT00;"]
        );
        assert_eq!(d(c, "TX;"), vec!["IF;"]);
        // Split: FR receives, FT transmits.
        assert_eq!(u(c, "FR1;"), vec!["FR4;", "VS1;"]);
        assert_eq!(u(c, "FT0;"), vec!["FT0;"]);
        assert_eq!(d(c, "FT3;"), vec!["FT1;"]);
        assert_eq!(d(c, "FR0;"), vec!["FR0;"]);
        assert_eq!(d(c, "VS1;"), vec!["FR1;"]);
        // TX / RX.
        assert_eq!(d(c, "TX1;"), vec!["TX0;"]);
        assert_eq!(u(c, "TX0;"), vec!["TX1;"]);
        assert_eq!(d(c, "TX0;"), vec!["RX;"]);
        assert_eq!(u(c, "RX;"), vec!["TX0;"]);
        // AI.
        assert_eq!(d(c, "AI1;"), vec!["AI2;"]);
        assert_eq!(u(c, "AI2;"), vec!["AI1;"]);
        // Menu: EX P1(3) 0000 P5.
        assert_eq!(d(c, "EX068;"), vec!["EX0670000;"]);
        assert_eq!(u(c, "EX06700005;"), vec!["EX0685;"]);
        assert_eq!(d(c, "EX0685;"), vec!["EX06700005;"]);
        assert_eq!(u(c, "EX040000008;"), vec!["EX04108;", "KP40;"]);
        assert_eq!(d(c, "KP40;"), vec!["EX040000008;"]);
        // Meters.
        assert_eq!(d(c, "SM0;"), vec!["SM0;"]);
        assert_eq!(u(c, "SM00015;"), vec!["SM0130;", "RM1130;", "RM5000;"]);
        assert_eq!(d(c, "RM6;"), vec!["RM;"]);
        assert_eq!(u(c, "RM10012;"), vec!["RM6089;"]);
        // Width by mode.
        u(c, "MD3;");
        assert_eq!(d(c, "SH0;"), vec!["FW;"]);
        assert_eq!(u(c, "FW0500;"), vec!["SH008;"]);
        assert_eq!(d(c, "SH010;"), vec!["FW1000;"]);
        u(c, "MD2;");
        u(c, "DA0;");
        assert_eq!(d(c, "SH0;"), vec!["SH;"]);
        assert_eq!(u(c, "SH07;"), vec!["SH007;"]);
        assert_eq!(u(c, "SL03;"), Vec::<String>::new());
        u(c, "DA1;");
        assert_eq!(d(c, "SH0;"), vec!["SL;"]);
        // Others.
        assert_eq!(u(c, "GC1;"), vec!["GT03;"]);
        assert_eq!(d(c, "GT01;"), vec!["GC2;"]);
        assert_eq!(d(c, "PA01;"), vec!["PA1;"]);
        assert_eq!(u(c, "PA10;"), vec!["PA01;"]);
        assert_eq!(d(c, "RA01;"), vec!["RA01;"]);
        assert_eq!(u(c, "RA0100;"), vec!["RA01;"]);
        assert_eq!(u(c, "MC 05;"), vec!["MC005;"]);
        assert_eq!(d(c, "BS05;"), vec!["BU05;"]);
        assert_eq!(d(c, "AC002;"), vec!["AC111;"]);
        assert_eq!(u(c, "AC111;"), vec!["AC002;"]);
        assert_eq!(d(c, "AN02;"), vec!["AN299;"]);
        assert_eq!(u(c, "AN200;"), vec!["AN020;"]);
        assert_eq!(d(c, "AB;"), vec!["VV;"]);
        assert_eq!(d(c, "MR005;"), vec!["MR0005;"]);
        assert_eq!(u(c, "MR0 050001407400020000000000000000000000000;"), vec!["MR00514074000+000000210000;"]);
        assert_eq!(u(c, "MR0 060000000000000000000000000000000000000;"), vec!["?;"]);
        assert_eq!(d(c, "CN012;"), vec!["CN12;", "TN12;"]);
        assert_eq!(u(c, "CN32;"), vec!["CN040;"]);
        assert_eq!(d(c, "CT02;"), vec!["CT0;", "TO1;"]);
        assert_eq!(u(c, "TO1;"), vec!["CT02;"]);
        assert_eq!(d(c, "PL050;"), vec!["PL050050;"]);
        assert_eq!(u(c, "PL030070;"), vec!["PL030;"]);
        assert_eq!(d(c, "PL040;"), vec!["PL040070;"]);
        assert_eq!(d(c, "ML1100;"), vec!["ML020;"]);
        assert_eq!(u(c, "ML000;"), vec!["ML0000;", "ML1100;"]);
        assert_eq!(d(c, "ML1050;"), Vec::<String>::new());
        assert_eq!(d(c, "RU0100;"), vec!["RU00100;"]);
        assert_eq!(d(c, "BP00001;"), vec!["NT20;"]);
        assert_eq!(u(c, "NT10;"), vec!["BP00000;"]);
    }

    #[test]
    fn program_side() {
        // A program reading the radio: Reads come up, RigHelm's answers go down.
        let c = &mut cx(&TS590SG);
        assert_eq!(u(c, "PA;"), vec!["PA0;"]);
        assert!(emulating(c));
        assert_eq!(d(c, "PA01;"), vec!["PA10;"]);
        assert_eq!(u(c, "PA1;"), vec!["PA01;"]);
        assert_eq!(d(c, "MC005;"), vec!["MC 05;"]);
        assert_eq!(u(c, "FT1;"), vec!["FT3;"]);
        assert_eq!(u(c, "TX;"), vec!["TX1;"]);
        assert_eq!(u(c, "TX1;"), vec!["TX1;"]);
        assert_eq!(u(c, "RM;"), vec!["RM6;", "RM3;", "RM4;"]);
        assert_eq!(d(c, "MD03;"), vec!["MD3;", "DA0;"]);
        assert_eq!(u(c, "DA1;"), Vec::<String>::new());
        assert_eq!(u(c, "MD2;"), vec!["MD02;"]);
        assert_eq!(u(c, "DA1;"), vec!["MD0C;"]);
        assert_eq!(u(c, "MR0005;"), vec!["MR005;"]);
        assert_eq!(d(c, "MR00114074000+000000C1000000;"), vec![format!("MR0 05000140740002100000{};", "0".repeat(17))]);
        assert_eq!(u(c, "IF;"), vec!["FR;", "FT;", "TX;", "IF;"]);
        assert_eq!(d(c, "FR4;"), Vec::<String>::new());
        assert_eq!(d(c, "FT1;"), Vec::<String>::new());
        assert_eq!(d(c, "TX0;"), Vec::<String>::new());
        assert_eq!(d(c, "IF00114074000+012010200000;"), vec!["IF00014074000     +012010001021000000;"]);
        assert_eq!(u(c, "PC000;"), vec!["PC005;"]);
        assert_eq!(d(c, "FR0;"), vec!["FR0;"]);
        assert_eq!(u(c, "EX0670000;"), vec!["EX068;"]);
        assert_eq!(d(c, "AC002;"), vec!["AC011;"]);
        assert_eq!(u(c, "SL;"), vec!["SH0;"]);
        assert_eq!(d(c, "SH007;"), vec!["SL00;"]);
        assert_eq!(u(c, "SH;"), vec!["SH0;"]);
        assert_eq!(d(c, "SH007;"), vec!["SH07;"]);
        assert_eq!(u(c, "SF000007074000D;"), vec!["FA07074000;", "MD0C;"]);
        assert_eq!(u(c, "SF0;"), vec!["IF;"]);
        assert_eq!(d(c, "IF00114074000+012010C00000;"), vec!["SF000014074000D;"]);
        assert_eq!(u(c, "SF1;"), vec!["FB;"]);
        assert_eq!(d(c, "FB07074000;"), vec!["SF100007074000D;"]);
        assert_eq!(d(c, "FB07074000;"), vec!["FB00007074000;"]);
    }

    #[test]
    fn round_trips() {
        for m in [&TS590S, &TS590SG] {
            for x in [
                "FA14074000;",
                "FB07074000;",
                "MD01;",
                "MD02;",
                "MD03;",
                "MD07;",
                "MD06;",
                "MD09;",
                "MD04;",
                "MD05;",
                "MD08;",
                "MD0C;",
                "MD0A;",
                "MD0H;",
                "IF00114074000+012010200000;",
                "IF00014074000-005001C00000;",
                "AG0128;",
                "RG0200;",
                "SQ0010;",
                "PC050;",
                "MG040;",
                "KS022;",
                "SD0300;",
                "VD0450;",
                "VX1;",
                "RT1;",
                "XT0;",
                "AI1;",
                "AI0;",
                "AC000;",
                "AC001;",
                "AC002;",
                "BC01;",
                "BP00001;",
                "BP00000;",
                "BP01202;",
                "CN012;",
                "CT00;",
                "CT01;",
                "CT02;",
                "FR0;",
                "FR4;",
                "FT0;",
                "FT1;",
                "GT00;",
                "GT01;",
                "GT03;",
                "LK4;",
                "LK5;",
                "NB01;",
                "NB03;",
                "NL0005;",
                "NR01;",
                "RL007;",
                "PR01;",
                "PL050;",
                "VG056;",
                "ML0000;",
                "ML1100;",
                "SM0130;",
                "RM6089;",
                "EX0012;",
                "KP40;",
                "TX0;",
                "SC0;",
                "MC005;",
                "MC105;",
                "TS1;",
                "PS1;",
                "ID023;",
            ] {
                let c = &mut cx(m);
                let native = down(c, x);
                assert!(!native.is_empty(), "{}: {x} went nowhere", m.key);
                let back: Vec<String> = native.iter().flat_map(|n| up(c, n)).collect();
                assert!(back.iter().any(|b| b == x), "{}: {x} -> {native:?} -> {back:?}", m.key);
            }
        }
    }

    #[test]
    fn menus() {
        assert_eq!(MENU_SG.len(), 100);
        assert_eq!(MENU_S.len(), 88);
        for (menu, n) in [(MENU_SG, 100), (MENU_S, 88)] {
            for (i, m) in menu.iter().enumerate() {
                assert_eq!(m.num as usize, i + 1);
                assert_eq!(m.addr as usize, i);
            }
            assert_eq!(menu.len(), n);
        }
        assert_eq!(TS590SG.menu_item(41).unwrap().name, "Sidetone / pitch frequency");
        assert_eq!(TS590S.menu_item(35).unwrap().name, "Sidetone / pitch frequency");
    }

    #[test]
    fn tones() {
        for k in 0..42 {
            assert_eq!(kw_tone(rh_tone(k)), k);
        }
        assert_eq!(kw_tone(26), kw_tone(26)); // 159.8 Hz: the nearest
    }
}
