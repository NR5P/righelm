//! Kenwood TS-890S ("PC Control Command Reference Guide", January 2019).
//!
//! Much is the TS-590's (see [`super::ts590`], whose translator handles
//! what is the same). The differences:
//!
//! * `MD0x` is `OM0x`, which has the DATA modes and PSK itself.
//! * `AG`, `RG`, `SQ` have no P1; `NB` is `NB1`/`NB2`, `NL` `NL1`, `RL`
//!   `RL1`; `PA` 0-2, `RA` 0-3 (6/12/18 dB); `MC` is `MN`; `SV` is `EC`
//!   (the radio's `SV` is M>V, RigHelm's `MA`); `BS` is `BU0nn`, band
//!   up/down `BU;`/`BD;`; `LK` has one parameter; `TO` is the tone mode
//!   (RigHelm `CT0`); `CN`/`TN` use the same 50 tones as RigHelm.
//! * Manual notch: `NT` on/off, `BP` 000-255. Pitch `PT` (5 Hz steps).
//!   Monitor `MO0` on/off, `ML` 0-20. `VG0` (microphone) 0-20, `VD0` in
//!   150 ms steps.
//! * Width: `SH0nnn` (high cut, SSB), `SL0nn` (width: CW, FSK, PSK, and
//!   SSB-DATA when menu 6-12 is Shift & Width).
//! * Meters: `SM` 0-70 dots; `RM` reads the meters switched on with
//!   `RMn1` (done with every `AI` RigHelm sends, i.e. on connect).
//! * Memories: `MA0`.
//! * `IF` (used for TX state and split) is not in this reference, but
//!   Hamlib's TS-890S driver relies on it (TS-590 layout, `OM` mode codes).

use super::ts590::{
    self, ack, at, cur_mode, emulating, if_down, if_read, info_up, item, one, rd, remember_mode, scale,
};
use crate::dialect::text::{Ctx, TextSpec, is_read, parts, read_prefix};
use crate::dialect::{num as pnum, rescale};
use crate::menu::{Kind, MenuItem, num};
use crate::model::{Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::state::ChannelInfo;
use crate::types::{Band, Mode, PO_CAL, S_METER_CAL, SWR_CAL, WidthClass};

pub static SPEC: TextSpec = TextSpec { down, up, answer_prefix: read_prefix };

/// Meter dots 0-70 (`SM`, `RM`). Approximate: the TS-590's scales stretched
/// over 70 dots.
pub const S_CAL: [(u8, f32); 5] = [(0, -54.0), (35, 0.0), (47, 20.0), (58, 40.0), (70, 60.0)];
pub const PO_CAL70: [(u8, f32); 5] = [(0, 0.0), (12, 10.0), (23, 25.0), (35, 50.0), (70, 100.0)];
pub const SWR_CAL70: [(u8, f32); 5] = [(0, 1.0), (21, 1.5), (28, 2.0), (35, 3.0), (70, 10.0)];
pub const LIN70: [(u8, f32); 2] = [(0, 0.0), (70, 255.0)];
const RAW: [(u8, f32); 2] = ts590::RAW_LINEAR_CAL;

/// `SH0nnn` high cut in SSB/SSB-DATA (Hz).
pub const HICUT: [u32; 28] = [
    600, 700, 800, 900, 1000, 1100, 1200, 1300, 1400, 1500, 1600, 1700, 1800, 1900, 2000, 2100, 2200, 2300, 2400, 2500, 2600, 2700,
    2800, 2900, 3000, 3400, 4000, 5000,
];
/// `SL0nn` width in CW.
pub const CW_WIDTH: [u32; 19] = [50, 80, 100, 150, 200, 250, 300, 350, 400, 450, 500, 600, 700, 800, 900, 1000, 1500, 2000, 2500];
/// `SL0nn` width in FSK.
pub const FSK_WIDTH: [u32; 8] = [250, 300, 350, 400, 450, 500, 1000, 1500];
/// `SL0nn` width in PSK (RigHelm's table for FSK, PSK and SSB-DATA widths).
pub const PSK_WIDTH: [u32; 27] = [
    50, 80, 100, 150, 200, 250, 300, 350, 400, 450, 500, 600, 700, 800, 900, 1000, 1200, 1400, 1500, 1600, 1800, 2000, 2200, 2400,
    2600, 2800, 3000,
];
/// `SL0nn` width in SSB-DATA (Shift & Width).
pub const DATA_WIDTH: [u32; 36] = [
    50, 80, 100, 150, 200, 250, 300, 350, 400, 450, 500, 600, 700, 800, 900, 1000, 1100, 1200, 1300, 1400, 1500, 1600, 1700, 1800,
    1900, 2000, 2100, 2200, 2300, 2400, 2500, 2600, 2700, 2800, 2900, 3000,
];

pub fn widths(class: WidthClass, _narrow: bool) -> Vec<(u8, u32)> {
    let t: &[u32] = match class {
        WidthClass::Ssb => &HICUT,
        WidthClass::Cw => &CW_WIDTH,
        WidthClass::Rtty => &PSK_WIDTH,
        WidthClass::Fixed => &[],
    };
    t.iter().enumerate().map(|(i, &w)| (i as u8, w)).collect()
}

fn nearest(table: &[u32], hz: u32) -> usize {
    table.iter().enumerate().min_by_key(|(_, w)| (**w as i64 - hz as i64).abs()).map(|(i, _)| i).unwrap_or(0)
}

/// How the receive width is set in the current mode: the command, the
/// radio's table and RigHelm's (the same table where they match).
pub fn width_of(cx: &Ctx, data_addr: u32) -> Option<(&'static str, &'static [u32], &'static [u32])> {
    Some(match cur_mode(cx) {
        Some(Mode::Cw | Mode::CwR) => ("SL", &CW_WIDTH, &CW_WIDTH),
        Some(Mode::RttyLsb | Mode::RttyUsb) => ("SL", &FSK_WIDTH, &PSK_WIDTH),
        Some(Mode::Psk) => ("SL", &PSK_WIDTH, &PSK_WIDTH),
        // Menu "Filter Control in SSB-DATA Mode": 0 High & Low Cut.
        Some(Mode::DataLsb | Mode::DataUsb) if cx.recall(&format!("ex{data_addr}")) != Some("000") => ("SL", &DATA_WIDTH, &PSK_WIDTH),
        Some(Mode::Lsb | Mode::Usb | Mode::DataLsb | Mode::DataUsb) | None => ("SH", &HICUT, &HICUT),
        _ => return None,
    })
}

/// RigHelm `SH0` as the radio's `SH0`/`SL0`.
pub fn width_down(cx: &mut Ctx, read: bool, p: &str, data_addr: u32) -> Vec<String> {
    if !read {
        if let Some(q) = cx.memo.remove("wq") {
            // The other filter edge, which RigHelm doesn't keep.
            return one(if q == "SH" { "SH0010;" } else { "SL000;" });
        }
    }
    let Some((c, radio, ours)) = width_of(cx, data_addr) else { return Vec::new() };
    if read {
        return one(format!("{c}0;"));
    }
    let Some(i) = pnum::<usize>(&p[1..]).filter(|&i| i < ours.len()) else { return Vec::new() };
    let id = if radio.as_ptr() == ours.as_ptr() { i } else { nearest(radio, ours[i]) };
    one(if c == "SH" { format!("SH0{id:03};") } else { format!("SL0{id:02};") })
}

/// The radio's `SH`/`SL` as RigHelm's `SH0`.
pub fn width_up(cx: &mut Ctx, code: &str, p: &str, data_addr: u32) -> Vec<String> {
    let w = width_of(cx, data_addr);
    if p.len() <= 1 {
        cx.remember("emu", "1");
        if w.map(|w| w.0) != Some(code) {
            cx.remember("wq", code);
        }
        return one("SH0;");
    }
    let Some((c, radio, ours)) = w.filter(|w| w.0 == code) else {
        return if emulating(cx) { ack(cx) } else { Vec::new() };
    };
    let Some(id) = pnum::<usize>(&p[1..]).filter(|&i| i < radio.len()) else { return Vec::new() };
    let i = if radio.as_ptr() == ours.as_ptr() { id } else { nearest(ours, radio[id]) };
    let _ = c;
    one(format!("SH0{i:02};"))
}

/// RigHelm's `MD0x` as the `OM` code.
pub fn om_down(cx: &mut Ctx, read: bool, p: &str, target: char) -> Vec<String> {
    if read {
        return one(format!("OM{target};"));
    }
    let Some(m) = Mode::from_code(at(p, 1)).filter(|m| cx.model.modes.contains(m)) else { return Vec::new() };
    remember_mode(cx, m);
    match ts590::om_code(m) {
        Some(c) => one(format!("OM{target}{c};")),
        None => Vec::new(),
    }
}

pub fn om_up(cx: &mut Ctx, p: &str) -> Vec<String> {
    if p.len() <= 1 {
        return rd(cx, &["MD0;"]);
    }
    let Some(m) = ts590::om_mode(at(p, 1)) else { return Vec::new() };
    remember_mode(cx, m);
    one(format!("MD0{};", m.code()))
}

/// RigHelm's `MR` answer or `MW` set as the radio's `MA0` (with `cmd` =
/// "MA0"), the TS-990 has the same layout.
pub fn ma_down(cx: &mut Ctx, code: &str, p: &str) -> Vec<String> {
    let Some(info) = ChannelInfo::parse(p) else { return Vec::new() };
    let ch = match code {
        "MR" => cx.recall("mr").and_then(|c| c.parse().ok()).unwrap_or(info.channel),
        _ => info.channel,
    };
    let Some(m) = info.mode.and_then(ts590::om_code) else { return Vec::new() };
    let t = info.tone.min(49);
    one(format!("MA0{ch:03}{:011}{m}0{}{t:02}{t:02}{}0000;", info.freq, ts590::kw_tone_mode(info.ctcss), "0".repeat(12)))
}

/// The radio's `MA0` answer (or set) as RigHelm's `MR` answer (or `MW`).
pub fn ma_up(cx: &mut Ctx, p: &str) -> Vec<String> {
    if p.len() == 4 {
        let ch = &p[1..4];
        cx.remember("mr", ch);
        return rd(cx, &[&format!("MR{ch};")]);
    }
    if p.len() < 37 || at(p, 0) != '0' {
        return Vec::new();
    }
    let Ok(ch) = p[1..4].parse::<u16>() else { return Vec::new() };
    let Some(freq) = pnum::<u64>(&p[4..15]).filter(|&f| f > 0) else {
        // A blank channel.
        return if emulating(cx) { Vec::new() } else { one("?;") };
    };
    let Some(mode) = ts590::om_mode(at(p, 15)) else { return Vec::new() };
    let tmode = at(p, 17);
    let tone: u8 = p[if tmode == '1' { 18..20 } else { 20..22 }].parse().unwrap_or(0);
    let info = ChannelInfo { channel: ch, freq, mode: Some(mode), mem_mode: 1, ctcss: ts590::rh_tone_mode(tmode), tone: tone.min(49), ..Default::default() };
    if emulating(cx) { one(info.to_mw(ch)) } else { one(info.to_answer("MR")) }
}

/// A RigHelm `EXnnn...` as the radio's `EXaaaaa v` (P1 P2P2 P3P3, space).
pub fn ex_down(cx: &mut Ctx, p: &str) -> Vec<String> {
    let Some(item) = p.get(..3).and_then(|n| n.parse::<u16>().ok()).and_then(|n| cx.model.menu_item(n)) else { return Vec::new() };
    let raw = &p[3..];
    if raw.is_empty() {
        return one(format!("EX{:05};", item.addr));
    }
    cx.remember(&format!("ex{}", item.addr), raw);
    one(format!("EX{:05} {raw};", item.addr))
}

pub fn ex_up(cx: &mut Ctx, p: &str) -> Vec<String> {
    let Some(addr) = p.get(..5).and_then(|a| a.parse::<u32>().ok()) else { return Vec::new() };
    let Some(item) = cx.model.menu.iter().find(|m| m.addr == addr) else { return Vec::new() };
    if p.len() == 5 {
        return rd(cx, &[&format!("EX{:03};", item.num)]);
    }
    // P4: a space (9 = back to the initial value, which RigHelm can't).
    if at(p, 5) != ' ' {
        return Vec::new();
    }
    let raw = &p[6..];
    cx.remember(&format!("ex{addr}"), raw);
    one(format!("EX{:03}{raw};", item.num))
}

/// The meters `RM;` answers, switched on once (with each `AI` set).
pub const RM_ON: [&str; 5] = ["RM11;", "RM21;", "RM31;", "RM41;", "RM51;"];

/// Radio `RM` type -> RigHelm `RM` code, and its calibration.
fn rm_map(t: char) -> Option<(char, &'static [(u8, f32)], &'static [(u8, f32)])> {
    Some(match t {
        '1' => ('4', &LIN70, &RAW),
        '2' => ('6', &SWR_CAL70, &SWR_CAL),
        '3' => ('3', &LIN70, &RAW),
        '4' => ('7', &LIN70, &RAW),
        '5' => ('8', &LIN70, &RAW),
        _ => return None,
    })
}

pub fn rm_down(cx: &mut Ctx, read: bool, p: &str) -> Vec<String> {
    if read {
        return match at(p, 0) {
            '1' | '5' => one("SM;"),
            '3' | '4' | '6' | '7' | '8' => one("RM;"),
            _ => Vec::new(),
        };
    }
    let Some(v) = pnum::<u8>(&p[1..]) else { return Vec::new() };
    let c = at(p, 0);
    if c == '5' {
        return one(format!("SM{:04};", rescale(v, &PO_CAL, &PO_CAL70)));
    }
    let Some(t) = ['1', '2', '3', '4', '5'].into_iter().find(|&t| rm_map(t).is_some_and(|m| m.0 == c)) else { return Vec::new() };
    let (_, ours, rh) = rm_map(t).unwrap();
    let _ = cx;
    one(format!("RM{t}{:04};", rescale(v, rh, ours)))
}

pub fn rm_up(cx: &mut Ctx, p: &str) -> Vec<String> {
    match p.len() {
        0 => rd(cx, &["RM4;", "RM6;", "RM3;", "RM7;", "RM8;"]),
        2 => ack(cx),
        5 => {
            let (Some((c, ours, rh)), Some(v)) = (rm_map(at(p, 0)), pnum::<u8>(&p[1..])) else { return Vec::new() };
            one(format!("RM{c}{:03};", rescale(v.min(70), ours, rh)))
        }
        _ => Vec::new(),
    }
}

/// The radio's `SM` (0-70): S meter receiving, power meter transmitting.
pub fn sm_up(cx: &mut Ctx, p: &str) -> Vec<String> {
    let tx = cx.recall("tx") == Some("1");
    if p.is_empty() {
        return rd(cx, &[if tx { "RM5;" } else { "SM0;" }]);
    }
    let Some(v) = pnum::<u8>(p) else { return Vec::new() };
    let v = v.min(70);
    if tx {
        vec!["SM0000;".into(), "RM1000;".into(), format!("RM5{:03};", rescale(v, &PO_CAL70, &PO_CAL))]
    } else {
        let s = rescale(v, &S_CAL, &S_METER_CAL);
        vec![format!("SM0{s:03};"), format!("RM1{s:03};"), "RM5000;".into()]
    }
}

/// `NB1`/`NB2` as RigHelm's `NB0` (0 off, 1 NB1, 2 NB2, 3 both).
pub fn nb_up(cx: &mut Ctx, p: &str) -> Vec<String> {
    let which = at(p, 0);
    if !matches!(which, '1' | '2') {
        return Vec::new();
    }
    if p.len() == 1 {
        return rd(cx, &["NB0;"]);
    }
    let key = if which == '1' { "nb1" } else { "nb2" };
    let changed = cx.recall(key) != Some(&p[1..2]);
    cx.remember(key, &p[1..2]);
    // NB2's answer comes first and NB1's says it all.
    if which == '2' && !changed && !emulating(cx) {
        return Vec::new();
    }
    let n = (cx.recall("nb1") == Some("1")) as u8 | (((cx.recall("nb2") == Some("1")) as u8) << 1);
    one(format!("NB0{n};"))
}

pub fn nb_down(cx: &mut Ctx, read: bool, p: &str) -> Vec<String> {
    if read {
        return vec!["NB2;".into(), "NB1;".into()];
    }
    let Some(n) = pnum::<u8>(&p[1..]) else { return Vec::new() };
    let (a, b) = (n & 1, (n >> 1) & 1);
    cx.remember("nb1", a.to_string());
    cx.remember("nb2", b.to_string());
    vec![format!("NB1{a};"), format!("NB2{b};")]
}

/// The TS-890's own frames; `None`: as the TS-590.
fn down890(cx: &mut Ctx, code: &str, f: &str, p: &str, read: bool) -> Option<Vec<String>> {
    let emu = emulating(cx);
    let tail = p.get(1..).unwrap_or("");
    Some(match code {
        "AG" | "RG" | "SQ" if read => one(format!("{code};")),
        "AG" | "RG" | "SQ" => one(format!("{code}{tail};")),
        "AI" if !read && !emu => {
            let mut out = ts590::down(cx, f);
            out.extend(RM_ON.iter().map(|s| s.to_string()));
            out
        }
        "AN" => match (read, at(p, 1)) {
            (true, _) => one("AN;"),
            (false, a @ ('1' | '2')) if p.len() == 2 => one(format!("AN{a}999;")),
            (false, a @ ('1' | '2')) if p.len() == 3 => one(format!("AN{a}000;")),
            _ => Vec::new(),
        },
        "BI" => one(f),
        "BP" => match (p.get(..2), p.get(2..).and_then(pnum::<u32>)) {
            (Some("00"), None) => one("NT;"),
            (Some("01"), None) => one("BP;"),
            (Some("00"), Some(v)) => one(format!("NT{};", (v > 0) as u8)),
            (Some("01"), Some(v)) => one(format!("BP{:03};", ((v.clamp(1, 400) - 1) * 255 + 199) / 399)),
            _ => Vec::new(),
        },
        "BS" if p.len() == 2 => one(format!("BU0{p};")),
        "BU" | "BD" => one(format!("{code};")),
        "BY" if read => one("BY;"),
        "BY" => one(format!("BY{};", at(p, 0))),
        "CN" if read => one("CN;"),
        "CN" => vec![format!("CN{tail};"), format!("TN{tail};")],
        "CT" if read => one("TO;"),
        "CT" => match tail {
            "0" => one("TO0;"),
            "1" => one("TO2;"),
            "2" => one("TO1;"),
            _ => Vec::new(),
        },
        "EX" => ex_down(cx, p),
        "GT" if read => one("GC;"),
        "GT" => match tail {
            "0" => one("GC0;"),
            "1" | "4" => one("GC3;"),
            "2" | "5" => one("GC2;"),
            "3" | "6" => one("GC1;"),
            _ => Vec::new(),
        },
        "IF" if read => one("IF;"),
        "IF" => if_down(cx, p, true),
        "KP" => match pnum::<u32>(p) {
            None => one("PT;"),
            Some(n) => one(format!("PT{:03};", (n * 2).min(160))),
        },
        "LK" => match pnum::<u8>(p) {
            None => one("LK;"),
            Some(v @ (0 | 1)) => one(format!("LK{v};")),
            Some(v @ 4..=7) => one(format!("LK{};", v & 1)),
            _ => Vec::new(),
        },
        "MC" if read => one("MN;"),
        "MC" => one(format!("MN{p};")),
        "MD" => om_down(cx, read, p, '0'),
        "ML" if read => one(if p == "0" { "MO0;" } else { "ML;" }),
        "ML" => match (at(p, 0), pnum::<u32>(tail)) {
            ('0', Some(v)) => one(format!("MO0{};", (v > 0) as u8)),
            ('1', Some(v)) => one(format!("ML{:03};", scale(v.min(100), 100, 20))),
            _ => Vec::new(),
        },
        "MR" if read => one(format!("MA0{p};")),
        "MR" | "MW" => ma_down(cx, code, p),
        "NB" => nb_down(cx, read, p),
        "NL" | "RL" if read => one(format!("{code}1;")),
        "NL" | "RL" => one(format!("{code}1{tail};")),
        "PA" | "RA" if read => one(format!("{code};")),
        "PA" | "RA" => one(format!("{code}{tail};")),
        "PR" => match p {
            "0" => one("PR0;"),
            "00" | "01" => one(format!("PR{p};")),
            _ => Vec::new(),
        },
        "RM" => rm_down(cx, read, p),
        "SC" if read => one("SC0;"),
        "SC" => match pnum::<u8>(p) {
            Some(n) if emu => one(format!("SC0{}0;", n.min(1))),
            Some(n) => one(format!("SC0{};", n.min(1))),
            None => Vec::new(),
        },
        "SH" => width_down(cx, read, p, 612),
        "SM" if read => one("SM;"),
        "SM" => match pnum::<u8>(tail) {
            Some(v) => one(format!("SM{:04};", rescale(v, &S_METER_CAL, &S_CAL))),
            None => Vec::new(),
        },
        "SV" => one("EC;"),
        "VD" => match pnum::<u32>(p) {
            None => one("VD0;"),
            Some(ms) => one(format!("VD0{:03};", ((ms + 75) / 150).min(20))),
        },
        "VG" => match pnum::<u32>(p) {
            None => one("VG00;"),
            Some(v) => one(format!("VG00{:03};", scale(v.min(100), 100, 20))),
        },
        _ => return None,
    })
}

fn up890(cx: &mut Ctx, code: &str, f: &str, p: &str) -> Option<Vec<String>> {
    let c0 = at(p, 0);
    Some(match code {
        "AG" | "RG" | "SQ" if p.is_empty() => rd(cx, &[&format!("{code}0;")]),
        "AG" | "RG" | "SQ" => one(format!("{code}0{p};")),
        "AN" if p.is_empty() => rd(cx, &["AN0;"]),
        "AN" if p.len() == 4 && matches!(c0, '1' | '2') => one(if emulating(cx) { format!("AN0{c0};") } else { format!("AN0{c0}0;") }),
        "AN" => ack(cx),
        "BI" if p.is_empty() => rd(cx, &["BI;"]),
        "BI" => one(f),
        "BP" if p.is_empty() => rd(cx, &["BP01;"]),
        "BP" => match pnum::<u32>(p) {
            Some(k) => one(format!("BP01{:03};", 1 + (k.min(255) * 399 + 127) / 255)),
            None => Vec::new(),
        },
        "NT" if p.is_empty() => rd(cx, &["BP00;"]),
        "NT" => one(format!("BP0000{};", (c0 != '0') as u8)),
        "BU" | "BD" if p.is_empty() => one(format!("{code}0;")),
        "BU" | "BD" if p.len() == 3 => one(format!("BS{};", &p[1..])),
        "BU" | "BD" => Vec::new(),
        "BY" if p.is_empty() => rd(cx, &["BY;"]),
        "BY" => one(format!("BY{c0}0;")),
        "CN" | "TN" if p.is_empty() => rd(cx, &["CN0;"]),
        "CN" | "TN" => match pnum::<u8>(p).filter(|&t| t <= 49) {
            Some(t) => one(format!("CN0{t:02};")),
            None => Vec::new(),
        },
        "TO" if p.is_empty() => rd(cx, &["CT0;"]),
        "TO" => one(match c0 {
            '1' => "CT02;",
            '2' | '3' => "CT01;",
            _ => "CT00;",
        }),
        "EC" => one("SV;"),
        "FV" if p.is_empty() => ts590::fv_read(cx, Some(159)),
        "TB" if p.is_empty() => {
            cx.remember("tbq", "1");
            rd(cx, &["FR;", "FT;"])
        }
        "TB" => {
            // Transmit on the VFO not receiving (or the same one).
            let rx_b = cx.recall("vfo") == Some("1");
            let tx_b = (c0 == '1') != rx_b;
            cx.remember("ft", if tx_b { "1" } else { "0" });
            one(format!("FT{};", if tx_b { 3 } else { 2 }))
        }
        "EX" => ex_up(cx, p),
        "GC" => match c0 {
            '\0' => rd(cx, &["GT0;"]),
            '0' => one("GT00;"),
            '1' => one("GT03;"),
            '2' => one("GT02;"),
            '3' | '4' => one("GT01;"),
            _ => Vec::new(),
        },
        "IF" if p.is_empty() => if_read(cx),
        "IF" => info_up(cx, p, true),
        // RIT/XIT offset: from RigHelm's IF.
        "RF" if p.is_empty() => {
            cx.remember("rfq", "1");
            rd(cx, &["IF;"])
        }
        "RF" => Vec::new(),
        "PT" if p.is_empty() => rd(cx, &["KP;"]),
        "PT" => match pnum::<u32>(p) {
            Some(n) => one(format!("KP{:02};", ((n + 1) / 2).min(75))),
            None => Vec::new(),
        },
        "LK" if p.is_empty() => rd(cx, &["LK;"]),
        "LK" => one(format!("LK{};", 4 + (c0 == '1') as u8)),
        "MN" if p.is_empty() => rd(cx, &["MC;"]),
        "MN" => one(format!("MC{p};")),
        "OM" => om_up(cx, p),
        "MO" if p == "0" => rd(cx, &["ML0;"]),
        "MO" if p.len() == 2 && c0 == '0' => one(format!("ML000{};", at(p, 1))),
        "ML" if p.is_empty() => rd(cx, &["ML1;"]),
        "ML" => match pnum::<u32>(p) {
            Some(k) => one(format!("ML1{:03};", scale(k.min(20), 20, 100))),
            None => Vec::new(),
        },
        "MA" => ma_up(cx, p),
        "NB" => nb_up(cx, p),
        "NL" | "RL" if p == "1" => rd(cx, &[&format!("{code}0;")]),
        "NL" | "RL" if c0 == '1' => one(format!("{code}0{};", &p[1..])),
        "NL" | "RL" => Vec::new(),
        "PA" | "RA" if p.is_empty() => rd(cx, &[&format!("{code}0;")]),
        "PA" | "RA" => one(format!("{code}0{c0};")),
        "PR" if p == "0" => rd(cx, &["PR0;"]),
        "PR" if p.len() == 2 && c0 == '0' => one(f),
        "PR" => Vec::new(),
        "RM" => rm_up(cx, p),
        "SC" if p == "0" => rd(cx, &["SC;"]),
        "SC" if c0 == '0' && p.len() >= 2 => one(format!("SC{};", (at(p, 1) != '0') as u8)),
        "SC" => Vec::new(),
        "SH" | "SL" => width_up(cx, code, p, 612),
        "SM" => sm_up(cx, p),
        "VD" if p == "0" => rd(cx, &["VD;"]),
        "VD" if c0 == '0' && p.len() == 4 => match pnum::<u32>(&p[1..]) {
            Some(n) => one(format!("VD{:04};", n.min(20) * 150)),
            None => Vec::new(),
        },
        "VD" => Vec::new(),
        "VG" if p == "00" => rd(cx, &["VG;"]),
        "VG" if p.starts_with("00") && p.len() == 5 => match pnum::<u32>(&p[2..]) {
            Some(n) => one(format!("VG{:03};", scale(n.min(20), 20, 100))),
            None => Vec::new(),
        },
        "VG" => Vec::new(),
        // The TS-590's that mean something else here.
        "DA" | "FW" | "GT" | "PB" | "TP" => Vec::new(),
        _ => return None,
    })
}

pub fn down(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !p.is_ascii() {
        return Vec::new();
    }
    let read = is_read(f);
    // A dropped answer (see `ack`) and IF fill-ins are the TS-590's.
    if emulating(cx) && !read {
        let fv = cx.recall("fvq").is_some() && matches!(code.as_str(), "ID" | "EX");
        let fill = (cx.recall("swallow").is_some() || cx.recall("ifq").is_some()) && matches!(code.as_str(), "ID" | "FR" | "FT" | "TX");
        if fv || fill {
            return ts590::down(cx, f);
        }
    }
    // A program's TB; (split): from RigHelm's FR and FT.
    if emulating(cx) && !read && cx.recall("tbq").is_some() {
        match (code.as_str(), at(p, 0)) {
            ("FR", c) => {
                cx.remember("vfo", if c >= '4' { "1" } else { "0" });
                return Vec::new();
            }
            ("FT", c) => {
                cx.memo.remove("tbq");
                let rx_b = cx.recall("vfo") == Some("1");
                return one(format!("TB{};", ((c == '1') != rx_b) as u8));
            }
            _ => {}
        }
    }
    down890(cx, &code, f, p, read).unwrap_or_else(|| ts590::down(cx, f))
}

pub fn up(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !p.is_ascii() {
        return Vec::new();
    }
    up890(cx, &code, f, p).unwrap_or_else(|| ts590::up(cx, f))
}

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
    Band { code: 10, label: "GEN", lo: 130_000, hi: 30_000_000 },
];

pub static MODES: &[Mode] = &[
    Mode::Lsb,
    Mode::Usb,
    Mode::Cw,
    Mode::CwR,
    Mode::RttyLsb,
    Mode::RttyUsb,
    Mode::Psk,
    Mode::Fm,
    Mode::Am,
    Mode::DataLsb,
    Mode::DataUsb,
    Mode::DataFm,
    Mode::DataAm,
];

const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AN", "BC", "BD", "BI", "BP", "BS", "BU", "BY", "CH", "CN", "CT", "DN", "EX", "FA", "FB", "FR", "FT", "GT",
    "ID", "IF", "KP", "KS", "LK", "MA", "MC", "MD", "MG", "ML", "MR", "MW", "NB", "NL", "NR", "PA", "PC", "PL", "PR", "PS", "QI",
    "RA", "RC", "RD", "RG", "RL", "RM", "RT", "RU", "SC", "SD", "SH", "SM", "SQ", "SV", "TS", "TX", "UP", "VD", "VG", "VS", "VX",
    "XT",
];

pub static TS890S: RadioModel = RadioModel {
    key: "ts890s",
    name: "TS-890S",
    maker: Maker::Kenwood,
    family: Family::Kenwood,
    proto: Proto::Text(&SPEC),
    id: "024",
    civ_addr: 0,
    hamlib: 2041,
    freq_min: 130_000,
    freq_max: 74_800_000,
    bands: BANDS,
    modes: MODES,
    menu: MENU,
    roles: Roles {
        // The transmit audio source is the MS command, not a menu item.
        mic_sel: &[],
        mic_front: 0,
        mic_usb: 0,
        // Advanced menu 17: Virtual Standard COM port RTS (0 flow control,
        // 1 CW keying).
        pc_keying: Some((153, 0, 1)),
        cat_tot: None,
        cat_items: &[112, 113, 114, 152, 153, 154, 155, 156],
        cw_bfo: None,
        cw_pitch_display: None,
        // Menu 6-12 Filter Control in SSB-DATA: 0 High & Low Cut.
        data_other: Some((108, "000")),
        data_disp: None,
        data_psk: false,
    },
    caps: Caps {
        commands: COMMANDS,
        widths,
        narrow: false,
        preamp: &[(0, "OFF"), (1, "PRE 1"), (2, "PRE 2")],
        att: &[(0, "OFF"), (1, "6 dB"), (2, "12 dB"), (3, "18 dB")],
        agc: &[(0, "OFF"), (1, "FAST"), (2, "MID"), (3, "SLOW")],
        roofing: &[],
        roofing_answers: &[],
        nb: &[(0, "OFF"), (1, "NB1"), (2, "NB2"), (3, "NB1+NB2")],
        nr_max: 10,
        nb_level_max: 20,
        antennas: 2,
        power: (5, 100),
        meter_select: &[],
        voice_memories: 0,
        keyer_memories: 0,
        keyer_memory_len: 0,
        // 100-109 program scan channels P0-P9, 110-119 extension E0-E9.
        memory: Memory { first: 0, last: 119, pms: None },
        shared_clarifier: true,
    },
    bauds: &[115_200, 57_600, 38_400, 19_200, 9_600, 4_800],
    stop_bits: 1,
    rts_cts: false,
    sim: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_frames() {
        let c = &mut Ctx::new(&TS890S);
        assert_eq!(down(c, "FA07000000;"), vec!["FA00007000000;"]);
        assert_eq!(down(c, "MD0C;"), vec!["OM0D;"]);
        assert_eq!(up(c, "OM0A;"), vec!["MD0I;"]);
        assert_eq!(down(c, "MD0;"), vec!["OM0;"]);
        assert_eq!(down(c, "AG0128;"), vec!["AG128;"]);
        assert_eq!(up(c, "AG128;"), vec!["AG0128;"]);
        assert_eq!(down(c, "NB03;"), vec!["NB11;", "NB21;"]);
        assert_eq!(down(c, "NB0;"), vec!["NB2;", "NB1;"]);
        assert_eq!(up(c, "NB20;"), vec!["NB01;"]);
        assert_eq!(up(c, "NB11;"), vec!["NB01;"]);
        assert_eq!(down(c, "KP40;"), vec!["PT080;"]);
        assert_eq!(up(c, "PT080;"), vec!["KP40;"]);
        assert_eq!(down(c, "EX108;"), vec!["EX00612;"]);
        assert_eq!(up(c, "EX00612 001;"), vec!["EX108001;"]);
        assert_eq!(down(c, "EX108000;"), vec!["EX00612 000;"]);
        assert_eq!(down(c, "AI0;"), vec!["AI0;", "RM11;", "RM21;", "RM31;", "RM41;", "RM51;"]);
        assert_eq!(up(c, "RM20028;"), vec!["RM6089;"]);
        assert_eq!(up(c, "SM0035;"), vec!["SM0130;", "RM1130;", "RM5000;"]);
        assert_eq!(down(c, "SV;"), vec!["EC;"]);
        assert_eq!(down(c, "MA;"), vec!["SV;"]);
        assert_eq!(down(c, "BS05;"), vec!["BU005;"]);
        assert_eq!(down(c, "BU0;"), vec!["BU;"]);
        assert_eq!(down(c, "CT01;"), vec!["TO2;"]);
        assert_eq!(up(c, "TO1;"), vec!["CT02;"]);
        assert_eq!(down(c, "ML0001;"), vec!["MO01;"]);
        assert_eq!(down(c, "ML1050;"), vec!["ML010;"]);
        assert_eq!(down(c, "MR005;"), vec!["MA0005;"]);
        assert_eq!(up(c, &format!("MA0005000140740002000000{};", "0".repeat(15))), vec!["MR00514074000+000000210000;"]);
        // CW width.
        up(c, "OM03;");
        assert_eq!(down(c, "SH0;"), vec!["SL0;"]);
        assert_eq!(up(c, "SL010;"), vec!["SH010;"]);
        // FSK: through RigHelm's PSK table.
        up(c, "OM06;");
        assert_eq!(down(c, "SH010;"), vec!["SL005;"]);
        assert_eq!(up(c, "SL005;"), vec!["SH010;"]);
        up(c, "OM02;");
        assert_eq!(down(c, "SH024;"), vec!["SH0024;"]);
        assert_eq!(up(c, "TB;"), vec!["FR;", "FT;"]);
        assert_eq!(down(c, "FR0;"), Vec::<String>::new());
        assert_eq!(down(c, "FT1;"), vec!["TB1;"]);
        assert_eq!(up(c, "TB0;"), vec!["FT2;"]);
        assert_eq!(up(c, "RF;"), vec!["IF;"]);
        assert_eq!(down(c, "IF00114074000-012010C00000;"), vec!["RF10120;"]);
        assert_eq!(down(c, "IF00114074000+012010C00000;"), vec!["IF00014074000     +0120100010D0000000;"]);
        assert_eq!(up(c, "IF00014074000     +0120100010D0000000;")[0], "IF00114074000+012010C00000;");
    }

    #[test]
    fn round_trips() {
        for x in [
            "FA14074000;",
            "MD01;",
            "MD0C;",
            "MD08;",
            "MD0I;",
            "MD0H;",
            "MD06;",
            "AG0100;",
            "RG0255;",
            "SQ0010;",
            "PA02;",
            "RA03;",
            "NB01;",
            "NB02;",
            "NL0010;",
            "RL005;",
            "KP40;",
            "LK5;",
            "MC105;",
            "CN012;",
            "CT00;",
            "CT01;",
            "CT02;",
            "GT00;",
            "GT01;",
            "GT02;",
            "GT03;",
            "BP00001;",
            "BP01201;",
            "ML0001;",
            "ML1050;",
            "VG050;",
            "VD0300;",
            "PR01;",
            "SM0130;",
            "RM6089;",
            "EX001001;",
            "TX0;",
            "BI1;",
            "BY10;",
            "IF00114074000+012010200000;",
        ] {
            let c = &mut Ctx::new(&TS890S);
            let native = down(c, x);
            assert!(!native.is_empty(), "{x} went nowhere");
            let back: Vec<String> = native.iter().flat_map(|n| up(c, n)).collect();
            assert!(back.iter().any(|b| b == x), "{x} -> {native:?} -> {back:?}");
        }
    }

    #[test]
    fn menu() {
        for (i, m) in TS890S.menu.iter().enumerate() {
            assert_eq!(m.num as usize, i + 1);
            assert!(m.kind != Kind::Choice(&[]), "{}", m.name);
        }
    }
}

// ------------------------------------------------------------- menu
// The EX menu, transcribed from ts890.pdf (PC Control Command Reference Guide,
// January/30/2019) pages 25-29 (PDF pages 26-30). Advanced Menu: P2 is any value, written as 00.

pub static MENU: &[MenuItem] = &[
    // BASIC (P1=0 P2=00)
    item(1, 0, "BASIC", "Color Display Pattern", 3, Kind::Choice(&["Type 1", "Type 2", "Type 3"])),
    item(2, 1, "BASIC", "Function Key Style", 3, Kind::Choice(&["Type 1", "Type 2", "Type 3"])),
    item(3, 2, "BASIC", "Font Style (Frequency Display)", 3, Kind::Choice(&["Font 1", "Font 2", "Font 3", "Font 4", "Font 5"])),
    item(4, 3, "BASIC", "Screen Saver", 3, Kind::Choice(&["Off", "Type 1", "Type 2", "Type 3", "Display Off"])),
    item(5, 4, "BASIC", "Screen Saver Wait Time", 3, Kind::Choice(&["Preview (5 s)", "5 min", "15 min", "30 min", "60 min"])),
    item(6, 5, "BASIC", "Screen Saver Message", 10, Kind::Text { max: 10 }),
    item(7, 6, "BASIC", "Power-on Message", 15, Kind::Text { max: 15 }),
    item(8, 7, "BASIC", "FM Mode S-Meter Sensitivity", 3, Kind::Choice(&["Normal", "High"])),
    item(9, 8, "BASIC", "Meter Response Speed (Analog)", 3, num(1, 4, 1, "")),
    item(10, 9, "BASIC", "Meter Display Pattern", 3, Kind::Choice(&["Digital", "Analog (White)", "Analog (Black)"])),
    item(11, 10, "BASIC", "Meter Display Peak Hold", 3, Kind::Choice(&["Off", "On"])),
    item(12, 11, "BASIC", "S-Meter Scale", 3, Kind::Choice(&["Type 1", "Type 2"])),
    item(13, 12, "BASIC", "TX Digital Meter", 3, Kind::Choice(&["Off", "On"])),
    item(14, 13, "BASIC", "Long Press Duration of Panel Keys", 3, Kind::Steps { max_code: 18, off_at_zero: false, first: 200, step: 100, unit: "ms" }),
    item(15, 14, "BASIC", "Touchscreen Tuning", 3, Kind::Choice(&["Off", "On"])),
    item(16, 15, "BASIC", "PF A: Key Assignment", 4, num(0, 9999, 1, "")),
    item(17, 16, "BASIC", "PF B: Key Assignment", 4, num(0, 9999, 1, "")),
    item(18, 17, "BASIC", "PF C: Key Assignment", 4, num(0, 9999, 1, "")),
    item(19, 18, "BASIC", "External PF 1: Key Assignment", 4, num(0, 9999, 1, "")),
    item(20, 19, "BASIC", "External PF 2: Key Assignment", 4, num(0, 9999, 1, "")),
    item(21, 20, "BASIC", "External PF 3: Key Assignment", 4, num(0, 9999, 1, "")),
    item(22, 21, "BASIC", "External PF 4: Key Assignment", 4, num(0, 9999, 1, "")),
    item(23, 22, "BASIC", "External PF 5: Key Assignment", 4, num(0, 9999, 1, "")),
    item(24, 23, "BASIC", "External PF 6: Key Assignment", 4, num(0, 9999, 1, "")),
    item(25, 24, "BASIC", "External PF 7: Key Assignment", 4, num(0, 9999, 1, "")),
    item(26, 25, "BASIC", "External PF 8: Key Assignment", 4, num(0, 9999, 1, "")),
    item(27, 26, "BASIC", "Microphone PF 1: Key Assignment", 4, num(0, 9999, 1, "")),
    item(28, 27, "BASIC", "Microphone PF 2: Key Assignment", 4, num(0, 9999, 1, "")),
    item(29, 28, "BASIC", "Microphone PF 3: Key Assignment", 4, num(0, 9999, 1, "")),
    item(30, 29, "BASIC", "Microphone PF 4: Key Assignment", 4, num(0, 9999, 1, "")),
    item(31, 30, "BASIC", "Microphone DOWN: Key Assignment", 4, num(0, 9999, 1, "")),
    item(32, 31, "BASIC", "Microphone UP: Key Assignment", 4, num(0, 9999, 1, "")),
    item(33, 32, "BASIC", "Automatic Power Off", 3, Kind::Choice(&["Off", "60 min", "120 min", "180 min"])),
    // AUDIO (P1=0 P2=01)
    item(34, 100, "AUDIO", "Beep Volume", 3, Kind::Steps { max_code: 20, off_at_zero: true, first: 1, step: 1, unit: "" }),
    item(35, 101, "AUDIO", "Voice Message Volume (Play)", 3, Kind::Steps { max_code: 20, off_at_zero: true, first: 1, step: 1, unit: "" }),
    item(36, 102, "AUDIO", "Sidetone Volume", 3, Kind::Coded(&[(0, "Linked with Monitor Control"), (1, "Off"), (2, "1"), (3, "2"), (4, "3"), (5, "4"), (6, "5"), (7, "6"), (8, "7"), (9, "8"), (10, "9"), (11, "10"), (12, "11"), (13, "12"), (14, "13"), (15, "14"), (16, "15"), (17, "16"), (18, "17"), (19, "18"), (20, "19"), (21, "20")])),
    item(37, 103, "AUDIO", "Voice Guidance Volume", 3, Kind::Steps { max_code: 20, off_at_zero: true, first: 1, step: 1, unit: "" }),
    item(38, 104, "AUDIO", "Voice Guidance Speed", 3, num(1, 4, 1, "")),
    item(39, 105, "AUDIO", "User Interface Language (Voice Guidance & Messages)", 3, Kind::Choice(&["English", "Japanese"])),
    item(40, 106, "AUDIO", "Automatic Voice Guidance", 3, Kind::Choice(&["Off", "On"])),
    // DECODE (P1=0 P2=02)
    item(41, 200, "DECODE", "FFT Scope Averaging (RTTY Decode)", 3, num(0, 9, 1, "")),
    item(42, 201, "DECODE", "RX UOS", 3, Kind::Choice(&["Off", "On"])),
    item(43, 202, "DECODE", "Newline Code", 3, Kind::Choice(&["CR+LF", "All"])),
    item(44, 203, "DECODE", "Diddle", 3, Kind::Choice(&["Off", "Blank Code", "Letters Code"])),
    item(45, 204, "DECODE", "TX UOS", 3, Kind::Choice(&["Off", "On"])),
    item(46, 205, "DECODE", "Automatic Newline Insertion", 3, Kind::Choice(&["Off", "On"])),
    item(47, 206, "DECODE", "FSK Spacing", 3, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz", "850 Hz"])),
    item(48, 207, "DECODE", "FSK Keying Polarity", 3, Kind::Choice(&["Off", "On"])),
    item(49, 208, "DECODE", "FSK Tone Frequency", 3, Kind::Choice(&["1275 Hz", "2125 Hz"])),
    item(50, 209, "DECODE", "RTTY Tuning Scope", 3, Kind::Choice(&["FFT Scope", "X-Y Scope"])),
    item(51, 210, "DECODE", "FFT Scope Averaging (PSK Decode)", 3, num(0, 9, 1, "")),
    item(52, 211, "DECODE", "PSK AFC Tuning Range", 3, Kind::Choice(&["±8 Hz", "±15 Hz"])),
    item(53, 212, "DECODE", "PSK Tone Frequency", 3, Kind::Choice(&["1.0 kHz", "1.5 kHz", "2.0 kHz"])),
    item(54, 213, "DECODE", "PSK Tuning Scope", 3, Kind::Choice(&["FFT Scope", "Vector Scope"])),
    item(55, 214, "DECODE", "CW/RTTY/PSK Log File Format", 3, Kind::Choice(&["html", "txt"])),
    item(56, 215, "DECODE", "CW/RTTY/PSK Time Stamp", 3, Kind::Choice(&["Off", "Time Stamp", "Stamp + Frequency"])),
    item(57, 216, "DECODE", "Clock (CW/RTTY/PSK Time Stamp)", 3, Kind::Choice(&["Local Clock", "Secondary Clock"])),
    item(58, 217, "DECODE", "Waterfall when Tuning (RTTY/PSK Audio Scope)", 3, Kind::Choice(&["Straight", "Follow"])),
    // TUNING (P1=0 P2=03)
    item(59, 300, "TUNING", "Frequency Rounding Off (Multi/Channel Control)", 3, Kind::Choice(&["Off", "On"])),
    item(60, 301, "TUNING", "SSB Mode Frequency Step Size (Multi/Channel Control)", 3, Kind::Choice(&["0.5 kHz", "1 kHz", "2.5 kHz", "5 kHz", "10 kHz"])),
    item(61, 302, "TUNING", "CW/FSK/PSK Mode Frequency Step Size (Multi/Channel Control)", 3, Kind::Choice(&["0.5 kHz", "1 kHz", "2.5 kHz", "5 kHz", "10 kHz"])),
    item(62, 303, "TUNING", "FM Mode Frequency Step Size (Multi/Channel Control)", 3, Kind::Choice(&["5 kHz", "6.25 kHz", "10 kHz", "12.5 kHz", "15 kHz", "20 kHz", "25 kHz", "30 kHz", "50 kHz", "100 kHz"])),
    item(63, 304, "TUNING", "AM Mode Frequency Step Size (Multi/Channel Control)", 3, Kind::Choice(&["5 kHz", "6.25 kHz", "10 kHz", "12.5 kHz", "15 kHz", "20 kHz", "25 kHz", "30 kHz", "50 kHz", "100 kHz"])),
    item(64, 305, "TUNING", "9 kHz Step in AM Broadcast Band (Multi/Channel Control)", 3, Kind::Choice(&["Off", "On"])),
    item(65, 306, "TUNING", "MHz Step", 3, Kind::Choice(&["100 kHz", "500 kHz", "1000 kHz"])),
    item(66, 307, "TUNING", "Tuning Control: Steps per Revolution", 3, Kind::Choice(&["250 Step", "500 Step", "1000 Step"])),
    item(67, 308, "TUNING", "Tuning Speed Control", 3, Kind::Coded(&[(1, "Off"), (2, "2"), (3, "3"), (4, "4"), (5, "5"), (6, "6"), (7, "7"), (8, "8"), (9, "9"), (10, "10")])),
    item(68, 309, "TUNING", "Tuning Speed Control Sensitivity", 3, num(1, 10, 1, "")),
    item(69, 310, "TUNING", "Lock Function", 3, Kind::Choice(&["Frequency Lock", "Tuning Control Lock"])),
    item(70, 311, "TUNING", "Number of Band Memories", 3, Kind::Choice(&["1", "3", "5"])),
    item(71, 312, "TUNING", "Split Frequency Offset by RIT/XIT Control", 3, Kind::Choice(&["Off", "TX Frequency Offset while RX", "RX Frequency Offset while TX", "Both"])),
    item(72, 313, "TUNING", "Band Direct Keys in Split Mode", 3, Kind::Choice(&["RX Band", "RX Band and Cancel Split Mode", "RX/TX Band"])),
    // MEMORY/SCAN (P1=0 P2=04)
    item(73, 400, "MEMORY/SCAN", "Number of Quick Memory Channels", 3, Kind::Choice(&["3 ch", "5 ch", "10 ch"])),
    item(74, 401, "MEMORY/SCAN", "Temporary Change (Memory Channel Configurations)", 3, Kind::Choice(&["Off", "On"])),
    item(75, 402, "MEMORY/SCAN", "Program Slow Scan", 3, Kind::Choice(&["Off", "On"])),
    item(76, 403, "MEMORY/SCAN", "Program Slow Scan Range", 3, Kind::Choice(&["100 Hz", "200 Hz", "300 Hz", "400 Hz", "500 Hz"])),
    item(77, 404, "MEMORY/SCAN", "Scan Hold", 3, Kind::Choice(&["Off", "On"])),
    item(78, 405, "MEMORY/SCAN", "Scan Resume", 3, Kind::Choice(&["Time-operated", "Carrier-operated"])),
    // CW (P1=0 P2=05)
    item(79, 500, "CW", "Paddle Jack Configuration (Front)", 3, Kind::Choice(&["Key", "Paddle", "Paddle (Bug Key Mode)"])),
    item(80, 501, "CW", "Key Jack Configuration (Rear)", 3, Kind::Choice(&["Key", "Paddle", "Paddle (Bug Key Mode)"])),
    item(81, 502, "CW", "Electronic Keyer Squeeze Mode", 3, Kind::Choice(&["Mode A", "Mode B"])),
    item(82, 503, "CW", "Dot and Dash Reversed Keying", 3, Kind::Choice(&["Off", "On"])),
    item(83, 504, "CW", "Paddle (Microphone Up/Down Keys)", 3, Kind::Choice(&["Off", "On"])),
    item(84, 505, "CW", "CW BFO Sideband", 3, Kind::Choice(&["USB", "LSB"])),
    item(85, 506, "CW", "Automatic CW TX with Keying in SSB Mode", 3, Kind::Choice(&["Off", "On"])),
    item(86, 507, "CW", "Carrier Frequency Offset", 3, Kind::Choice(&["Off", "On"])),
    item(87, 508, "CW", "CW Keying Weight Ratio", 3, Kind::Choice(&["Automatic", "2.5", "2.6", "2.7", "2.8", "2.9", "3.0", "3.1", "3.2", "3.3", "3.4", "3.5", "3.6", "3.7", "3.8", "3.9", "4.0"])),
    item(88, 509, "CW", "CW Keying Reversed Weight Ratio", 3, Kind::Choice(&["Off", "On"])),
    item(89, 510, "CW", "Interrupt Keying", 3, Kind::Choice(&["Off", "On"])),
    item(90, 511, "CW", "CW Message Entry", 3, Kind::Choice(&["Text String", "Paddle"])),
    item(91, 512, "CW", "Contest Number", 4, num(1, 9999, 1, "")),
    item(92, 513, "CW", "Contest Number Format", 3, Kind::Choice(&["Off", "190 to ANO", "190 to ANT", "90 to NO", "90 to NT"])),
    item(93, 514, "CW", "Channel Number (Countup Message)", 3, Kind::Choice(&["Off", "Channel 1", "Channel 2", "Channel 3", "Channel 4", "Channel 5", "Channel 6", "Channel 7", "Channel 8"])),
    item(94, 515, "CW", "CW Rise Time", 3, Kind::Choice(&["1 ms", "2 ms", "4 ms", "6 ms"])),
    item(95, 516, "CW", "CW/Voice Message Retransmit Interval Time", 3, num(0, 60, 1, "s")),
    // TX/RX (P1=0 P2=06)
    item(96, 600, "TX/RX", "Playback Time (Fulltime Recording)", 3, Kind::Choice(&["Last 10 s", "Last 20 s", "Last 30 s"])),
    item(97, 601, "TX/RX", "Recording with Squelch", 3, Kind::Choice(&["Off", "On"])),
    item(98, 602, "TX/RX", "Time-out Timer", 3, Kind::Choice(&["Off", "3 min", "5 min", "10 min", "20 min", "30 min"])),
    item(99, 603, "TX/RX", "TX Inhibit", 3, Kind::Choice(&["Off", "On"])),
    item(100, 604, "TX/RX", "Transmit Power Step Size", 3, Kind::Choice(&["1 W", "5 W"])),
    item(101, 605, "TX/RX", "ID Beep", 3, Kind::Choice(&["Off", "1 min", "2 min", "3 min", "4 min", "5 min"])),
    item(102, 606, "TX/RX", "TX Filter Low Cut (SSB/AM)", 3, Kind::Choice(&["10 Hz", "100 Hz", "200 Hz", "300 Hz", "400 Hz", "500 Hz"])),
    item(103, 607, "TX/RX", "TX Filter High Cut (SSB/AM)", 3, Kind::Choice(&["2500 Hz", "2600 Hz", "2700 Hz", "2800 Hz", "2900 Hz", "3000 Hz", "3500 Hz", "4000 Hz"])),
    item(104, 608, "TX/RX", "TX Filter Low Cut (SSB-DATA/AM-DATA)", 3, Kind::Choice(&["10 Hz", "100 Hz", "200 Hz", "300 Hz", "400 Hz", "500 Hz"])),
    item(105, 609, "TX/RX", "TX Filter High Cut (SSB-DATA/AM-DATA)", 3, Kind::Choice(&["2500 Hz", "2600 Hz", "2700 Hz", "2800 Hz", "2900 Hz", "3000 Hz", "3500 Hz", "4000 Hz"])),
    item(106, 610, "TX/RX", "RX Filter Numbers", 3, Kind::Choice(&["2", "3"])),
    item(107, 611, "TX/RX", "Filter Control in SSB Mode (High/Low and Shift/Width)", 3, Kind::Choice(&["High & Low Cut", "Shift & Width"])),
    item(108, 612, "TX/RX", "Filter Control in SSB-DATA Mode (High/Low and Shift/Width)", 3, Kind::Choice(&["High & Low Cut", "Shift & Width"])),
    item(109, 613, "TX/RX", "VOX Voice Delay (Microphone)", 3, Kind::Choice(&["Off", "Short", "Middle", "Long"])),
    item(110, 614, "TX/RX", "VOX Voice Delay (Except Microphone)", 3, Kind::Choice(&["Off", "Short", "Middle", "Long"])),
    item(111, 615, "TX/RX", "Delta Frequency Display", 3, Kind::Choice(&["Off", "On"])),
    // REAR CONN (P1=0 P2=07)
    item(112, 700, "REAR CONN", "Baud Rate (COM Port)", 3, Kind::Choice(&["4800 bps", "9600 bps", "19200 bps", "38400 bps", "57600 bps", "115200 bps"])),
    item(113, 701, "REAR CONN", "Baud Rate (Virtual Standard COM)", 3, Kind::Choice(&["9600 bps", "19200 bps", "38400 bps", "57600 bps", "115200 bps"])),
    item(114, 702, "REAR CONN", "Baud Rate (Virtual Enhanced COM)", 3, Kind::Choice(&["9600 bps", "19200 bps", "38400 bps", "57600 bps", "115200 bps"])),
    item(115, 703, "REAR CONN", "Decoded Character Output", 3, Kind::Choice(&["Off", "On"])),
    item(116, 704, "REAR CONN", "Quick Data Transfer", 3, Kind::Choice(&["Off", "1 (TX/RX)", "1 (Sub RX)", "2"])),
    item(117, 705, "REAR CONN", "Overwrite Location (Quick Data Transfer)", 3, Kind::Choice(&["VFO", "Quick Memory"])),
    item(118, 706, "REAR CONN", "USB: Audio Input Level", 3, num(0, 100, 1, "")),
    item(119, 707, "REAR CONN", "ACC 2: Audio Input Level", 3, num(0, 100, 1, "")),
    item(120, 708, "REAR CONN", "USB: Audio Output Level", 3, num(0, 100, 1, "")),
    item(121, 709, "REAR CONN", "ACC 2: Audio Output Level", 3, num(0, 100, 1, "")),
    item(122, 710, "REAR CONN", "TX Monitor Level (Rear Connectors)", 3, Kind::Choice(&["Linked", "0"])), // manual lists only these two codes
    item(123, 711, "REAR CONN", "Audio Output Type (Rear Connectors)", 3, Kind::Choice(&["All", "Received Audio Only"])),
    // SCOPE (P1=0 P2=08)
    item(124, 800, "SCOPE", "Bandscope Display during TX", 3, Kind::Choice(&["Off", "On"])),
    item(125, 801, "SCOPE", "TX Audio Waveform Display", 3, Kind::Choice(&["Off", "On"])),
    item(126, 802, "SCOPE", "Bandscope Maximum Hold", 3, Kind::Choice(&["10 s", "Continuous"])),
    item(127, 803, "SCOPE", "Waterfall when Tuning (Center Mode)", 3, Kind::Choice(&["Straight", "Follow"])),
    item(128, 804, "SCOPE", "Waterfall Gradation Level", 3, num(1, 10, 1, "")),
    item(129, 805, "SCOPE", "Tuning Assist Line (SSB Mode)", 3, Kind::Choice(&["Off", "300 Hz", "400 Hz", "500 Hz", "600 Hz", "700 Hz", "800 Hz", "1000 Hz", "1500 Hz", "2210 Hz"])),
    item(130, 806, "SCOPE", "Frequency Scale (Center Mode)", 3, Kind::Choice(&["Relative Frequency", "Absolute Frequency"])),
    item(131, 807, "SCOPE", "Automatic Correction Step (Touchscreen Tuning)", 3, Kind::Choice(&["100 Hz", "250 Hz", "500 Hz", "1000 Hz"])),
    // USB KEYBOARD (P1=0 P2=09)
    item(132, 900, "USB KEYBOARD", "Send Message by Function Keys (USB Keyboard)", 3, Kind::Choice(&["Off", "On"])),
    item(133, 901, "USB KEYBOARD", "Keyboard Language (USB Keyboard)", 3, Kind::Choice(&["Japanese", "English (US)", "English (UK)", "French", "French (Canadian)", "German", "Portuguese", "Portuguese (Brazilian)", "Spanish", "Spanish (Latin American)", "Italian"])),
    item(134, 902, "USB KEYBOARD", "Repeat Delay Time (USB Keyboard)", 3, num(1, 4, 1, "")),
    item(135, 903, "USB KEYBOARD", "Repeat Speed (USB Keyboard)", 3, Kind::Steps { max_code: 31, off_at_zero: false, first: 1, step: 1, unit: "" }),
    // ADVANCED (P1=1 P2=00)
    item(136, 10000, "ADVANCED", "Indication Signal Type (External Meter 1)", 3, Kind::Choice(&["Automatic", "TX Power", "ALC", "Drain Voltage (Vd)", "Compression Level (COMP)", "Current (Id)", "SWR"])),
    item(137, 10001, "ADVANCED", "Indication Signal Type (External Meter 2)", 3, Kind::Choice(&["Automatic", "TX Power", "ALC", "Drain Voltage (Vd)", "Compression Level (COMP)", "Current (Id)", "SWR"])),
    item(138, 10002, "ADVANCED", "Output Level (External Meter 1)", 3, num(0, 100, 1, "%")),
    item(139, 10003, "ADVANCED", "Output Level (External Meter 2)", 3, num(0, 100, 1, "%")),
    item(140, 10004, "ADVANCED", "Reference Signal Source", 3, Kind::Choice(&["Internal", "External"])),
    item(141, 10005, "ADVANCED", "Reference Oscillator Calibration", 4, Kind::Steps { max_code: 1000, off_at_zero: false, first: -500, step: 1, unit: "" }),
    item(142, 10006, "ADVANCED", "TX Power Down with Transverter Enabled", 3, Kind::Choice(&["Off", "On"])),
    item(143, 10007, "ADVANCED", "TX Hold After Antenna Tuning", 3, Kind::Choice(&["Off", "On"])),
    item(144, 10008, "ADVANCED", "Antenna Tuner during RX", 3, Kind::Choice(&["Off", "On"])),
    item(145, 10009, "ADVANCED", "Antenna Tuner Operation per Band", 3, Kind::Choice(&["Off", "On"])),
    item(146, 10010, "ADVANCED", "Microphone Gain (FM Mode)", 3, num(0, 100, 1, "")),
    item(147, 10011, "ADVANCED", "PKS Polarity Reverse", 3, Kind::Choice(&["Off", "On"])),
    item(148, 10012, "ADVANCED", "TX Inhibit While Busy", 3, Kind::Choice(&["Off", "On"])),
    item(149, 10013, "ADVANCED", "CTCSS Unmute for Internal Speaker", 3, Kind::Choice(&["Mute", "Unmute"])),
    item(150, 10014, "ADVANCED", "PSQ Logic State", 3, Kind::Choice(&["Low", "Open"])),
    item(151, 10015, "ADVANCED", "PSQ Reverse Condition", 3, Kind::Choice(&["Off", "Busy", "Sql", "Send", "Busy-Send", "Sql-Send"])),
    item(152, 10016, "ADVANCED", "PSQ/PKS Pin Assignment (COM Connector)", 3, Kind::Choice(&["Off", "On"])),
    item(153, 10017, "ADVANCED", "Virtual Standard COM Port - RTS", 3, Kind::Choice(&["Flow Control", "CW Keying", "RTTY Keying", "PTT", "DATA SEND"])),
    item(154, 10018, "ADVANCED", "Virtual Standard COM Port - DTR", 3, Kind::Choice(&["Off", "CW Keying", "RTTY Keying", "PTT", "DATA SEND"])),
    item(155, 10019, "ADVANCED", "Virtual Enhanced COM Port - RTS", 3, Kind::Choice(&["Off", "CW Keying", "RTTY Keying", "PTT", "DATA SEND"])),
    item(156, 10020, "ADVANCED", "Virtual Enhanced COM Port - DTR", 3, Kind::Choice(&["Off", "CW Keying", "RTTY Keying", "PTT", "DATA SEND"])),
    item(157, 10021, "ADVANCED", "External Display", 3, Kind::Choice(&["Off", "On"])),
    item(158, 10022, "ADVANCED", "Resolution (External Display)", 3, Kind::Choice(&["800x600", "848x480"])),
    item(159, 10027, "ADVANCED", "Firmware Version", 15, Kind::Text { max: 15 }), // read only; P2 is any value
];
