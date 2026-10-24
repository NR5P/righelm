//! Kenwood TS-990S ("PC Control Command Reference Guide"; firmware 1.20 or
//! later for the filter tables).
//!
//! A TS-890S with a main and a sub receiver: most commands take the band
//! as P1 (0 main, 1 sub), and RigHelm's VFO A is the main band, VFO B the
//! sub band. What differs from the TS-890S ([`super::ts890`]):
//!
//! * `AG`, `RG`, `SQ`, `BC`, `CN`, `PA`, `RA`, `NR` have P1 = band, which
//!   makes them RigHelm's layout; `NB1`/`NB2`/`NL1`/`RL1`/`NT`/`TO`/`GC`/
//!   `MN`/`SM` take the band too.
//! * `VS` (operating band) is `CB`; split is `TB` (transmit on the sub
//!   band), RigHelm's `FT`. There is no `FR` and no `IF`: RigHelm's
//!   `IF` reads `RF` (RIT/XIT offset) and fills in the rest from the last
//!   `FA`, `OM`, `RT`, `XT`, `MN` seen.
//! * There is no TX read: RigHelm's `TX;` reads `TB` and answers from the
//!   last TX/RX the radio sent (auto-information) or RigHelm keyed.
//! * `MG`, `PL`, `ML` (monitor level) and `VG0` (microphone) are 0-255,
//!   `PT` is 300 Hz + 10 Hz steps (RigHelm's `KP`), `PC` 5-200 W, `BI` 0-2
//!   (semi, full: RigHelm's on), `LK` locks main and sub.
//! * `OM` has three DATA variants (D1-D3); RigHelm uses D1.
//! * Memories (`MA0`, with two frequencies) are not translated.

use super::ts590::{self, at, emulating, item, one, rd, scale};
use super::ts890::{self, ex_down, ex_up, rm_down, rm_up, width_down, width_up};
use crate::dialect::text::{Ctx, TextSpec, is_read, parts, read_prefix};
use crate::dialect::{num as pnum, rescale};
use crate::menu::{Kind, MenuItem, num};
use crate::model::{Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::types::{Band, S_METER_CAL};

pub static SPEC: TextSpec = TextSpec { down, up, answer_prefix: read_prefix };

/// Menu 6-08 "Filter Control in SSB-Data Mode".
const DATA_FILTER: u32 = 608;

fn s255(v: u32) -> u32 {
    scale(v.min(100), 100, 255)
}

fn s100(v: u32) -> u32 {
    scale(v.min(255), 255, 100)
}

fn down990(cx: &mut Ctx, code: &str, f: &str, p: &str, read: bool) -> Option<Vec<String>> {
    let emu = emulating(cx);
    let tail = p.get(1..).unwrap_or("");
    Some(match code {
        // P1 = band: RigHelm's layout already.
        "AG" | "RG" | "SQ" | "BC" | "RA" | "NR" | "PR" => one(f),
        "AI" if !read && !emu => {
            let mut out = ts590::down(cx, f);
            out.extend(ts890::RM_ON.iter().map(|s| s.to_string()));
            out
        }
        "AN" => match (read, at(p, 1)) {
            (true, _) => one("AN0;"),
            (false, a @ '1'..='4') if p.len() == 2 => one(format!("AN00{a}99;")),
            (false, a @ '1'..='4') if p.len() == 3 => one(format!("AN00{a}00;")),
            _ => Vec::new(),
        },
        "BI" => one(f),
        "BP" => match (p.get(..2), p.get(2..).and_then(pnum::<u32>)) {
            (Some("00"), None) => one("NT0;"),
            (Some("01"), None) => one("BP;"),
            (Some("00"), Some(v)) => one(if v > 0 { "NT02;" } else { "NT00;" }),
            (Some("01"), Some(v)) => one(format!("BP0{:03};", ((v.clamp(1, 400) - 1) * 127 + 199) / 399)),
            _ => Vec::new(),
        },
        "BS" if p.len() == 2 => one(format!("BU0{p};")),
        "BU" | "BD" => one(format!("{code};")),
        "CN" if read => one("CN;"),
        "CN" => vec![format!("CN0{tail};"), format!("TN0{tail};")],
        "CT" if read => one("TO0;"),
        "CT" => match tail {
            "0" => one("TO00;"),
            "1" => one("TO02;"),
            "2" => one("TO01;"),
            _ => Vec::new(),
        },
        "EX" => ex_down(cx, p),
        "FR" => Vec::new(),
        "FT" if read => one("TB;"),
        "FT" => match p {
            "0" | "2" => one("TB0;"),
            "1" | "3" => one("TB1;"),
            _ => Vec::new(),
        },
        "TX" if read => one("TB;"),
        // No IF: RigHelm's is made from the RIT/XIT offset and what was seen.
        "IF" if read => one("RF;"),
        "IF" if cx.recall("rfq").is_some() => ts590::if_down(cx, p, true),
        "IF" => Vec::new(),
        "GT" if read => one("GC0;"),
        "GT" => match tail {
            "0" => one("GC00;"),
            "1" | "4" => one("GC03;"),
            "2" | "5" => one("GC02;"),
            "3" | "6" => one("GC01;"),
            _ => Vec::new(),
        },
        "KP" => match pnum::<u32>(p) {
            None => one("PT;"),
            Some(n) => one(format!("PT{:03};", n.min(80))),
        },
        "LK" => match pnum::<u8>(p) {
            None => one("LK;"),
            Some(v) => {
                let sub = cx.recall("lks").unwrap_or("0").to_string();
                match v {
                    0 | 1 => one(format!("LK{v}{sub};")),
                    4..=7 => one(format!("LK{}{};", v & 1, (v >> 1) & 1)),
                    _ => Vec::new(),
                }
            }
        },
        "MC" if read => one("MN0;"),
        "MC" => one(format!("MN0{p};")),
        "MD" => ts890::om_down(cx, read, p, '0'),
        "MG" => match pnum::<u32>(p) {
            None => one("MG;"),
            Some(v) => one(format!("MG{:03};", s255(v))),
        },
        "ML" if read => one(if p == "0" { "MO0;" } else { "ML;" }),
        "ML" => match (at(p, 0), pnum::<u32>(tail)) {
            ('0', Some(v)) => one(format!("MO0{};", (v > 0) as u8)),
            ('1', Some(v)) => one(format!("ML{:03};", s255(v))),
            _ => Vec::new(),
        },
        "NB" if read => vec!["NB20;".into(), "NB10;".into()],
        "NB" => match pnum::<u8>(tail) {
            Some(n) => {
                let (a, b) = (n & 1, (n >> 1) & 1);
                cx.remember("nb1", a.to_string());
                cx.remember("nb2", b.to_string());
                vec![format!("NB10{a};"), format!("NB20{b};")]
            }
            None => Vec::new(),
        },
        "NL" | "RL" if read => one(format!("{code}10;")),
        "NL" | "RL" => one(format!("{code}10{tail};")),
        "PA" if read => one("PA;"),
        "PA" => one(f),
        "PL" => match pnum::<u32>(p) {
            None => one("PL;"),
            Some(v) => {
                let out = cx.recall("plout").unwrap_or("128").to_string();
                one(format!("PL{:03}{out};", s255(v)))
            }
        },
        // S meter and power meter: SM0 (main band).
        "RM" if matches!(at(p, 0), '1' | '5') => match (read, pnum::<u8>(tail)) {
            (true, _) => one("SM0;"),
            (false, Some(v)) if at(p, 0) == '5' => one(format!("SM0{:04};", rescale(v, &crate::types::PO_CAL, &ts890::PO_CAL70))),
            _ => Vec::new(),
        },
        "RM" => rm_down(cx, read, p),
        "SC" if read => one("SC0;"),
        "SC" => match pnum::<u8>(p) {
            Some(n) if emu => one(format!("SC0{}0;", n.min(1))),
            Some(n) => one(format!("SC0{};", n.min(1))),
            None => Vec::new(),
        },
        "SH" => width_down(cx, read, p, DATA_FILTER),
        "SM" if read => one("SM0;"),
        "SM" => match pnum::<u8>(tail) {
            Some(v) => one(format!("SM0{:04};", rescale(v, &S_METER_CAL, &ts890::S_CAL))),
            None => Vec::new(),
        },
        "SV" => one("EC;"),
        "VD" => match pnum::<u32>(p) {
            None => one("VD0;"),
            Some(ms) => one(format!("VD0{:03};", ((ms + 75) / 150).min(20))),
        },
        "VG" => match pnum::<u32>(p) {
            None => one("VG00;"),
            Some(v) => one(format!("VG00{:03};", s255(v))),
        },
        "VS" => match p {
            "" => one("CB;"),
            "0" | "1" => one(format!("CB{p};")),
            _ => Vec::new(),
        },
        _ => return None,
    })
}

fn up990(cx: &mut Ctx, code: &str, f: &str, p: &str) -> Option<Vec<String>> {
    let c0 = at(p, 0);
    let main = c0 == '0';
    Some(match code {
        "AG" | "RG" | "SQ" | "BC" | "RA" | "NR" | "PR" | "PA" if p.len() == 1 => rd(cx, &[f]),
        "PA" if p.is_empty() => rd(cx, &["PA0;"]),
        "AG" | "RG" | "SQ" | "RA" | "PR" | "PA" if main => one(f),
        "BC" | "NR" if main => one(format!("{code}0{};", (at(p, 1) != '0') as u8)),
        "AG" | "RG" | "SQ" | "BC" | "RA" | "NR" | "PR" | "PA" => Vec::new(),
        "AN" if p == "0" => rd(cx, &["AN0;"]),
        "AN" if p.len() == 5 && c0 == '0' && at(p, 1) == '0' => {
            let a = at(p, 2);
            if !('1'..='4').contains(&a) {
                return Some(ts590::ack(cx));
            }
            one(if emulating(cx) { format!("AN0{a};") } else { format!("AN0{a}0;") })
        }
        "AN" => Vec::new(),
        "BI" if p.is_empty() => rd(cx, &["BI;"]),
        "BI" => one(format!("BI{};", (c0 != '0') as u8)),
        "BP" if p.is_empty() => rd(cx, &["BP01;"]),
        "BP" if main && p.len() == 4 => match pnum::<u32>(&p[1..]) {
            Some(k) => one(format!("BP01{:03};", 1 + (k.min(127) * 399 + 63) / 127)),
            None => Vec::new(),
        },
        "BP" => Vec::new(),
        "NT" if p == "0" => rd(cx, &["BP00;"]),
        "NT" if main && p.len() == 2 => one(format!("BP0000{};", (at(p, 1) == '2') as u8)),
        "NT" => Vec::new(),
        "BU" | "BD" if p.is_empty() => one(format!("{code}0;")),
        "BU" | "BD" if p.len() == 3 => one(format!("BS{};", &p[1..])),
        "BU" | "BD" => Vec::new(),
        "CB" if p.is_empty() => rd(cx, &["VS;"]),
        "CB" => one(format!("VS{};", (c0 == '1') as u8)),
        "CN" | "TN" if p.len() <= 1 => rd(cx, &["CN0;"]),
        "CN" | "TN" if main => match pnum::<u8>(&p[1..]).filter(|&t| t <= 49) {
            Some(t) => one(format!("CN0{t:02};")),
            None => Vec::new(),
        },
        "CN" | "TN" => Vec::new(),
        "TO" if p.len() <= 1 => rd(cx, &["CT0;"]),
        "TO" if main => one(match at(p, 1) {
            '1' => "CT02;",
            '2' | '3' => "CT01;",
            _ => "CT00;",
        }),
        "TO" => Vec::new(),
        "EC" => one("SV;"),
        "EX" => ex_up(cx, p),
        "FV" if p.is_empty() => {
            cx.remember("fv990", "1");
            rd(cx, &["ID;"])
        }
        "GC" if p.len() <= 1 => rd(cx, &["GT0;"]),
        "GC" if main => one(match at(p, 1) {
            '0' => "GT00;",
            '1' => "GT03;",
            '2' => "GT02;",
            _ => "GT01;",
        }),
        "GC" => Vec::new(),
        "PT" if p.is_empty() => rd(cx, &["KP;"]),
        "PT" => match pnum::<u32>(p) {
            Some(n) => one(format!("KP{:02};", n.min(75))),
            None => Vec::new(),
        },
        "LK" if p.is_empty() => rd(cx, &["LK;"]),
        "LK" => {
            cx.remember("lks", if at(p, 1) == '1' { "1" } else { "0" });
            one(format!("LK{};", 4 + (c0 == '1') as u8 + 2 * (at(p, 1) == '1') as u8))
        }
        "MG" if p.is_empty() => rd(cx, &["MG;"]),
        "MG" => match pnum::<u32>(p) {
            Some(v) => one(format!("MG{:03};", s100(v))),
            None => Vec::new(),
        },
        "MN" if p.len() <= 1 => rd(cx, &["MC;"]),
        "MN" if main => {
            cx.remember("mn", &p[1..]);
            one(format!("MC{};", &p[1..]))
        }
        "MN" => Vec::new(),
        "OM" if p.len() == 2 && !main => Vec::new(),
        "OM" => {
            // D2/D3 are RigHelm's DATA modes too.
            let c = match at(p, 1) {
                'G' | 'K' => 'C',
                'H' | 'L' => 'D',
                'I' | 'M' => 'E',
                'J' | 'N' => 'F',
                c => c,
            };
            ts890::om_up(cx, &format!("{c0}{c}")[..p.len().min(2)])
        }
        "MO" if p == "0" => rd(cx, &["ML0;"]),
        "MO" if p.len() == 2 && main => one(format!("ML000{};", at(p, 1))),
        "ML" if p.is_empty() => rd(cx, &["ML1;"]),
        "ML" => match pnum::<u32>(p) {
            Some(k) => one(format!("ML1{:03};", s100(k))),
            None => Vec::new(),
        },
        "NB" if p.len() == 2 && matches!(c0, '1' | '2') => rd(cx, &["NB0;"]),
        "NB" if p.len() == 3 && matches!(c0, '1' | '2') && at(p, 1) == '0' => ts890::nb_up(cx, &format!("{c0}{}", at(p, 2))),
        "NB" => Vec::new(),
        "NL" | "RL" if p == "10" => rd(cx, &[&format!("{code}0;")]),
        "NL" | "RL" if p.starts_with("10") && p.len() > 2 => one(format!("{code}0{};", &p[2..])),
        "NL" | "RL" => Vec::new(),
        "PL" if p.is_empty() => rd(cx, &["PL;"]),
        "PL" if p.len() == 6 => {
            cx.remember("plout", &p[3..]);
            match pnum::<u32>(&p[..3]) {
                Some(v) => one(format!("PL{:03};", s100(v))),
                None => Vec::new(),
            }
        }
        "PL" => Vec::new(),
        "RM" => rm_up(cx, p),
        "SC" if p == "0" => rd(cx, &["SC;"]),
        "SC" if c0 == '0' && p.len() >= 2 => one(format!("SC{};", (at(p, 1) != '0') as u8)),
        "SC" => Vec::new(),
        "SH" | "SL" if !main && p.len() > 1 => Vec::new(),
        "SH" | "SL" => width_up(cx, code, p, DATA_FILTER),
        "SM" if p.len() == 1 => {
            let tx = cx.recall("tx") == Some("1");
            rd(cx, &[if tx { "RM5;" } else { "SM0;" }])
        }
        "SM" if main => ts890::sm_up(cx, &p[1..]),
        "SM" => Vec::new(),
        "TB" if p.is_empty() => rd(cx, &["FT;"]),
        "TB" => {
            cx.remember("ft", if c0 == '1' { "1" } else { "0" });
            let tx = cx.recall("tx") == Some("1");
            if emulating(cx) {
                one(format!("FT{};", if c0 == '1' { 3 } else { 2 }))
            } else {
                vec![format!("FT{};", (c0 == '1') as u8), ts590::rh_tx(cx, tx)]
            }
        }
        "VD" if p == "0" => rd(cx, &["VD;"]),
        "VD" if c0 == '0' && p.len() == 4 => match pnum::<u32>(&p[1..]) {
            Some(n) => one(format!("VD{:04};", n.min(20) * 150)),
            None => Vec::new(),
        },
        "VD" => Vec::new(),
        "VG" if p == "00" => rd(cx, &["VG;"]),
        "VG" if p.starts_with("00") && p.len() == 5 => match pnum::<u32>(&p[2..]) {
            Some(n) => one(format!("VG{:03};", s100(n))),
            None => Vec::new(),
        },
        "VG" => Vec::new(),
        "RF" if p.is_empty() => {
            cx.remember("rfq", "1");
            rd(cx, &["IF;"])
        }
        "RF" if p.len() == 5 => {
            let Some(hz) = pnum::<u32>(&p[1..]) else { return Some(Vec::new()) };
            let fa = cx.recall("fa").and_then(|f| f.parse().ok()).unwrap_or(0);
            let mode = cx.recall("rhm").and_then(|m| m.chars().next()).unwrap_or('0');
            let rt = cx.recall("rt") == Some("1");
            let xt = cx.recall("xt") == Some("1");
            let ch = cx.recall("mn").and_then(|c| c.parse().ok()).unwrap_or(0);
            let clar = if c0 == '1' { -(hz as i32) } else { hz as i32 };
            one(ts590::rh_info("IF", ch, fa, clar, rt, xt, mode, 0, 0, 0))
        }
        "RT" | "XT" if p.len() == 1 => {
            cx.remember(&code.to_ascii_lowercase(), p);
            one(f)
        }
        "DA" | "FW" | "GT" | "PB" | "IF" | "FR" | "FT" | "SF" | "MD" => Vec::new(),
        _ => return None,
    })
}

pub fn down(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !p.is_ascii() {
        return Vec::new();
    }
    let read = is_read(f);
    if emulating(cx) && !read && code == "ID" && cx.memo.remove("fv990").is_some() {
        return one("FV1.20;");
    }
    if emulating(cx) && !read && code == "ID" && cx.recall("swallow").is_some() {
        return ts590::down(cx, f);
    }
    down990(cx, &code, f, p, read).unwrap_or_else(|| ts590::down(cx, f))
}

pub fn up(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !p.is_ascii() {
        return Vec::new();
    }
    up990(cx, &code, f, p).unwrap_or_else(|| ts590::up(cx, f))
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
    Band { code: 10, label: "GEN", lo: 30_000, hi: 60_000_000 },
];

const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AN", "BC", "BD", "BI", "BP", "BS", "BU", "BY", "CH", "CN", "CT", "DN", "EX", "FA", "FB", "FT", "GT",
    "ID", "IF", "KP", "KS", "LK", "MA", "MC", "MD", "MG", "ML", "NB", "NL", "NR", "PA", "PC", "PL", "PR", "PS", "QI", "RA", "RC", "RD",
    "RG", "RL", "RM", "RT", "RU", "SC", "SD", "SH", "SM", "SQ", "SV", "TS", "TX", "UP", "VD", "VG", "VS", "VX", "XT",
];

pub static TS990S: RadioModel = RadioModel {
    key: "ts990s",
    name: "TS-990S",
    maker: Maker::Kenwood,
    family: Family::Kenwood,
    proto: Proto::Text(&SPEC),
    id: "022",
    civ_addr: 0,
    hamlib: 2039,
    freq_min: 30_000,
    freq_max: 60_000_000,
    bands: BANDS,
    modes: ts890::MODES,
    menu: MENU,
    roles: Roles {
        // The transmit audio source is the MS command, not a menu item.
        mic_sel: &[],
        mic_front: 0,
        mic_usb: 0,
        pc_keying: None,
        cat_tot: None,
        cat_items: &[108, 109, 182, 192],
        cw_bfo: None,
        cw_pitch_display: None,
        // Menu 6-08 Filter Control in SSB-Data: 0 High & Low Cuts.
        data_other: Some((105, "000")),
        data_disp: None,
        data_psk: false,
    },
    caps: Caps {
        commands: COMMANDS,
        widths: ts890::widths,
        narrow: false,
        preamp: &[(0, "OFF"), (1, "ON")],
        att: &[(0, "OFF"), (1, "6 dB"), (2, "12 dB"), (3, "18 dB")],
        agc: &[(0, "OFF"), (1, "FAST"), (2, "MID"), (3, "SLOW")],
        roofing: &[],
        roofing_answers: &[],
        nb: &[(0, "OFF"), (1, "NB1"), (2, "NB2"), (3, "NB1+NB2")],
        nr_max: 10,
        nb_level_max: 32,
        antennas: 4,
        power: (5, 200),
        meter_select: &[],
        voice_memories: 0,
        keyer_memories: 0,
        keyer_memory_len: 0,
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
        let c = &mut Ctx::new(&TS990S);
        assert_eq!(down(c, "FA07000000;"), vec!["FA00007000000;"]);
        assert_eq!(down(c, "AG0128;"), vec!["AG0128;"]);
        assert_eq!(up(c, "AG0128;"), vec!["AG0128;"]);
        assert_eq!(up(c, "AG1100;"), Vec::<String>::new());
        assert_eq!(down(c, "MD0C;"), vec!["OM0D;"]);
        assert_eq!(up(c, "OM0H;"), vec!["MD0C;"]);
        assert_eq!(down(c, "VS1;"), vec!["CB1;"]);
        assert_eq!(down(c, "FT3;"), vec!["TB1;"]);
        assert_eq!(up(c, "TB1;"), vec!["FT1;", "TX0;"]);
        assert_eq!(down(c, "TX;"), vec!["TB;"]);
        assert_eq!(down(c, "MG050;"), vec!["MG128;"]);
        assert_eq!(up(c, "MG128;"), vec!["MG050;"]);
        assert_eq!(down(c, "KP40;"), vec!["PT040;"]);
        assert_eq!(down(c, "LK5;"), vec!["LK10;"]);
        assert_eq!(up(c, "LK11;"), vec!["LK7;"]);
        assert_eq!(down(c, "NB03;"), vec!["NB101;", "NB201;"]);
        assert_eq!(up(c, "NB200;"), vec!["NB01;"]);
        assert_eq!(down(c, "NL0010;"), vec!["NL10010;"]);
        assert_eq!(down(c, "SM0;"), vec!["SM0;"]);
        assert_eq!(up(c, "SM00035;"), vec!["SM0130;", "RM1130;", "RM5000;"]);
        assert_eq!(down(c, "EX105;"), vec!["EX00608;"]);
        assert_eq!(down(c, "BP00001;"), vec!["NT02;"]);
        assert_eq!(up(c, "NT02;"), vec!["BP00001;"]);
        assert_eq!(down(c, "AN03;"), vec!["AN00399;"]);
        up(c, "FA00014074000;");
        up(c, "OM02;");
        up(c, "RT1;");
        assert_eq!(down(c, "IF;"), vec!["RF;"]);
        assert_eq!(up(c, "RF10120;"), vec!["IF00014074000-012010200000;"]);
        assert_eq!(up(c, "RF;"), vec!["IF;"]);
        assert_eq!(down(c, "IF00014074000-012010200000;"), vec!["RF10120;"]);
    }

    #[test]
    fn round_trips() {
        for x in [
            "FA14074000;",
            "FB07074000;",
            "MD01;",
            "MD0C;",
            "MD0I;",
            "AG0100;",
            "RG0255;",
            "SQ0010;",
            "BC01;",
            "PA01;",
            "RA03;",
            "NR01;",
            "PR01;",
            "NB01;",
            "NB02;",
            "NL0010;",
            "RL005;",
            "KP40;",
            "LK4;",
            "LK7;",
            "MC105;",
            "CN012;",
            "CT01;",
            "CT02;",
            "GT01;",
            "GT03;",
            "BP00001;",
            "MG051;",
            "ML0001;",
            "ML1051;",
            "VG051;",
            "PL051;",
            "VS1;",
            "FT1;",
            "SM0130;",
            "RM6089;",
            "EX001001;",
            "BI1;",
            "TX0;",
            "PC150;",
        ] {
            let c = &mut Ctx::new(&TS990S);
            let native = down(c, x);
            assert!(!native.is_empty(), "{x} went nowhere");
            let back: Vec<String> = native.iter().flat_map(|n| up(c, n)).collect();
            assert!(back.iter().any(|b| b == x), "{x} -> {native:?} -> {back:?}");
        }
    }

    #[test]
    fn menu() {
        for (i, m) in TS990S.menu.iter().enumerate() {
            assert_eq!(m.num as usize, i + 1);
        }
        assert_eq!(TS990S.menu_item(105).unwrap().addr, DATA_FILTER);
    }
}

// ------------------------------------------------------------- menu
// The EX menu, transcribed from ts990.pdf (PC Control Command Reference Guide)
// pages 22-27 (PDF pages 23-28): the normal Menu (P1 = 0) and the Advanced Menu (P1 = 1).
// The manual gives no category titles; groups are named after the items in each P2 category.
// Four Advanced Menu rows (Touchscreen Calibration, Software License Agreement, Important
// Notices Concerning Free Open Source, About Various Software License Agreements) have no
// command and are not listed.

pub static MENU: &[MenuItem] = &[
    item(1, 0, "BASIC", "Color Display Pattern (Main screen)", 3, Kind::Choice(&["Type 1", "Type 2", "Type 3"])),
    item(2, 1, "BASIC", "Color Display Pattern (Sub screen)", 3, Kind::Choice(&["Type 1", "Type 2", "Type 3", "Same as Main"])),
    item(3, 2, "BASIC", "Font Style (Frequency display)", 3, Kind::Choice(&["Font 1", "Font 2", "Font 3"])),
    item(4, 3, "BASIC", "Dial Color Pattern", 3, Kind::Choice(&["Type 1", "Type 2"])),
    item(5, 4, "BASIC", "Screen Saver", 3, Kind::Choice(&["Off", "Type 1", "Type 2"])),
    item(6, 5, "BASIC", "Screen Saver Wait Time", 3, Kind::Choice(&["Preview (5 s)", "5 min", "15 min", "30 min", "60 min"])),
    item(7, 6, "BASIC", "Screen Saver Message", 10, Kind::Text { max: 10 }),
    item(8, 7, "BASIC", "Power-on Message", 15, Kind::Text { max: 15 }),
    item(9, 8, "BASIC", "FM Mode S-meter Sensitivity", 3, Kind::Choice(&["Low", "High"])),
    item(10, 9, "BASIC", "Meter Response Speed", 3, Kind::Coded(&[(1, "1"), (2, "2"), (3, "3"), (4, "4")])),
    item(11, 10, "BASIC", "Meter Display Pattern", 3, Kind::Choice(&["Type 1", "Type 2", "Type 3"])),
    item(12, 11, "BASIC", "Meter Display Peak Hold", 3, Kind::Choice(&["Off", "On"])),
    item(13, 12, "BASIC", "Long Press Duration of Panel Keys", 3, Kind::Steps { max_code: 18, off_at_zero: false, first: 200, step: 100, unit: "ms" }),
    item(14, 13, "BASIC", "Touchscreen Tuning", 3, Kind::Choice(&["Off", "On"])),
    item(15, 14, "BASIC", "Operating Band (High/Low & Shift/Width Controls)", 3, Kind::Choice(&["Main and Sub Bands", "Main Band Only"])),
    // PF key allotment IDs: 0000-0903 menus, 1000-1128 functions, 2000-2030 advanced menu, 9999 = Off.
    item(16, 15, "BASIC", "PF A: Key Assignment", 4, num(0, 9999, 1, "")),
    item(17, 16, "BASIC", "PF B: Key Assignment", 4, num(0, 9999, 1, "")),
    item(18, 17, "BASIC", "Voice (Main Band): Key Assignment", 4, num(0, 9999, 1, "")),
    item(19, 18, "BASIC", "Voice (Sub Band): Key Assignment", 4, num(0, 9999, 1, "")),
    item(20, 19, "BASIC", "External PF 1: Key Assignment", 4, num(0, 9999, 1, "")),
    item(21, 20, "BASIC", "External PF 2: Key Assignment", 4, num(0, 9999, 1, "")),
    item(22, 21, "BASIC", "External PF 3: Key Assignment", 4, num(0, 9999, 1, "")),
    item(23, 22, "BASIC", "External PF 4: Key Assignment", 4, num(0, 9999, 1, "")),
    item(24, 23, "BASIC", "External PF 5: Key Assignment", 4, num(0, 9999, 1, "")),
    item(25, 24, "BASIC", "External PF 6: Key Assignment", 4, num(0, 9999, 1, "")),
    item(26, 25, "BASIC", "External PF 7: Key Assignment", 4, num(0, 9999, 1, "")),
    item(27, 26, "BASIC", "External PF 8: Key Assignment", 4, num(0, 9999, 1, "")),
    item(28, 27, "BASIC", "Microphone PF 1: Key Assignment", 4, num(0, 9999, 1, "")),
    item(29, 28, "BASIC", "Microphone PF 2: Key Assignment", 4, num(0, 9999, 1, "")),
    item(30, 29, "BASIC", "Microphone PF 3: Key Assignment", 4, num(0, 9999, 1, "")),
    item(31, 30, "BASIC", "Microphone PF 4: Key Assignment", 4, num(0, 9999, 1, "")),
    item(32, 31, "BASIC", "Microphone Down: Key Assignment", 4, num(0, 9999, 1, "")),
    item(33, 32, "BASIC", "Microphone Up: Key Assignment", 4, num(0, 9999, 1, "")),
    item(34, 33, "BASIC", "Automatic Power Off", 3, Kind::Choice(&["Off", "60 min", "120 min", "180 min"])),
    // Firmware 1.20 or later.
    item(35, 34, "BASIC", "Data Mode Numbers", 3, Kind::Coded(&[(1, "1"), (2, "2"), (3, "3")])),
    item(36, 100, "AUDIO", "Beep Volume", 3, Kind::Steps { max_code: 20, off_at_zero: true, first: 1, step: 1, unit: "" }),
    item(37, 101, "AUDIO", "Voice Message Volume (Play)", 3, Kind::Steps { max_code: 20, off_at_zero: true, first: 1, step: 1, unit: "" }),
    // 000 = linked with the monitor control, 001 = Off, 002-021 = 1-20.
    item(38, 102, "AUDIO", "Sidetone Volume", 3, Kind::Coded(&[(0, "Linked with monitor control"), (1, "Off"), (2, "1"), (3, "2"), (4, "3"), (5, "4"), (6, "5"), (7, "6"), (8, "7"), (9, "8"), (10, "9"), (11, "10"), (12, "11"), (13, "12"), (14, "13"), (15, "14"), (16, "15"), (17, "16"), (18, "17"), (19, "18"), (20, "19"), (21, "20")])),
    item(39, 103, "AUDIO", "Voice Guidance Volume", 3, Kind::Steps { max_code: 20, off_at_zero: true, first: 1, step: 1, unit: "" }),
    item(40, 104, "AUDIO", "Voice Guidance Speed", 3, Kind::Coded(&[(1, "1"), (2, "2"), (3, "3"), (4, "4")])),
    item(41, 105, "AUDIO", "User Interface Language (Voice Guidance & Messages)", 3, Kind::Choice(&["English", "Japanese"])),
    item(42, 106, "AUDIO", "Automatic Voice Guidance", 3, Kind::Choice(&["Off", "On"])),
    item(43, 107, "AUDIO", "Headphones Mixing Balance", 3, num(0, 10, 1, "")),
    item(44, 108, "AUDIO", "Headphones Left/Right Reverse", 3, Kind::Choice(&["Off", "On"])),
    item(45, 200, "DECODE", "FFT Scope Averaging (RTTY Decode)", 3, num(0, 9, 1, "")),
    item(46, 201, "DECODE", "RX UOS", 3, Kind::Choice(&["Off", "On"])),
    item(47, 202, "DECODE", "Newline Code", 3, Kind::Choice(&["CR+LF", "All"])),
    item(48, 203, "DECODE", "Diddle", 3, Kind::Choice(&["Off", "Blank Code", "Letters Code"])),
    item(49, 204, "DECODE", "TX UOS", 3, Kind::Choice(&["Off", "On"])),
    item(50, 205, "DECODE", "Automatic Newline Insertion", 3, Kind::Choice(&["Off", "On"])),
    item(51, 206, "DECODE", "FSK Spacing", 3, Kind::Choice(&["170 Hz", "200 Hz", "425 Hz", "850 Hz"])),
    item(52, 207, "DECODE", "FSK Keying Polarity", 3, Kind::Choice(&["Off", "On"])),
    item(53, 208, "DECODE", "FSK Tone Frequency", 3, Kind::Choice(&["1275 Hz", "2125 Hz"])),
    item(54, 209, "DECODE", "FFT Scope Averaging (PSK Decode)", 3, num(0, 9, 1, "")),
    item(55, 210, "DECODE", "PSK AFC Tuning Range", 3, Kind::Choice(&["±8 Hz", "±15 Hz"])),
    item(56, 211, "DECODE", "PSK Tone Frequency", 3, Kind::Choice(&["1.0 kHz", "1.5 kHz", "2.0 kHz"])),
    // Renamed with firmware 1.20 or later.
    item(57, 212, "DECODE", "CW/RTTY/PSK Log File Format", 3, Kind::Choice(&["html", "txt"])),
    // Renamed with firmware 1.20 or later.
    item(58, 213, "DECODE", "CW/RTTY/PSK Time Stamp", 3, Kind::Choice(&["Off", "Time Stamp", "Time Stamp + Frequency"])),
    // Renamed with firmware 1.20 or later.
    item(59, 214, "DECODE", "Clock (CW/RTTY/PSK Time Stamp)", 3, Kind::Choice(&["Local Clock", "Secondary Clock"])),
    item(60, 300, "TUNING", "Frequency Rounding Off (Multi/Channel Control)", 3, Kind::Choice(&["Off", "On"])),
    // Firmware 1.13 or lower: SSB/CW/FSK/PSK Mode Frequency Step Size (Multi/Channel Control).
    item(61, 301, "TUNING", "SSB Mode Frequency Step Size (Multi/Channel Control)", 3, Kind::Choice(&["0.5 kHz", "1 kHz", "2.5 kHz", "5 kHz", "10 kHz"])),
    item(62, 302, "TUNING", "AM Mode Frequency Step Size (Multi/Channel Control)", 3, Kind::Choice(&["5 kHz", "6.25 kHz", "10 kHz", "12.5 kHz", "15 kHz", "20 kHz", "25 kHz", "30 kHz", "50 kHz", "100 kHz"])),
    item(63, 303, "TUNING", "FM Mode Frequency Step Size (Multi/Channel Control)", 3, Kind::Choice(&["5 kHz", "6.25 kHz", "10 kHz", "12.5 kHz", "15 kHz", "20 kHz", "25 kHz", "30 kHz", "50 kHz", "100 kHz"])),
    item(64, 304, "TUNING", "Frequency Step Size (Up/Down Keys)", 3, Kind::Choice(&["100 kHz", "500 kHz", "1000 kHz"])),
    item(65, 305, "TUNING", "9 kHz Step in AM Broadcast Band (Multi/Channel Control)", 3, Kind::Choice(&["Off", "On"])),
    item(66, 306, "TUNING", "Tuning Control (Main): Number of Steps per Revolution", 3, Kind::Choice(&["250 Step", "500 Step", "1000 Step"])),
    item(67, 307, "TUNING", "Tuning Control (Sub): Number of Steps per Revolution", 3, Kind::Choice(&["250 Step", "500 Step", "1000 Step"])),
    item(68, 308, "TUNING", "Number of Band Memories", 3, Kind::Choice(&["1", "3", "5"])),
    // Firmware 1.20 or later.
    item(69, 309, "TUNING", "CW/FSK/PSK Mode Frequency Step Size (Multi/Channel Control)", 3, Kind::Choice(&["0.5 kHz", "1 kHz", "2.5 kHz", "5 kHz", "10 kHz"])),
    // Firmware 1.20 or later.
    item(70, 310, "TUNING", "Lock Function", 3, Kind::Choice(&["Frequency Lock", "Tuning Control Lock"])),
    // Firmware 1.20 or later. 001 = Off, 002-010 = 2-10.
    item(71, 311, "TUNING", "Tuning Speed Control (Main)", 3, Kind::Coded(&[(1, "Off"), (2, "2"), (3, "3"), (4, "4"), (5, "5"), (6, "6"), (7, "7"), (8, "8"), (9, "9"), (10, "10")])),
    // Firmware 1.20 or later.
    item(72, 312, "TUNING", "Sensitivity (Main)", 3, num(1, 10, 1, "")),
    // Firmware 1.20 or later. 001 = Off, 002-010 = 2-10.
    item(73, 313, "TUNING", "Tuning Speed Control (Sub)", 3, Kind::Coded(&[(1, "Off"), (2, "2"), (3, "3"), (4, "4"), (5, "5"), (6, "6"), (7, "7"), (8, "8"), (9, "9"), (10, "10")])),
    // Firmware 1.20 or later.
    item(74, 314, "TUNING", "Tuning Speed Control Sensitivity (Sub)", 3, num(1, 10, 1, "")),
    item(75, 400, "MEMORY", "Number of Quick Memory Channels", 3, Kind::Choice(&["3 ch", "5 ch", "10 ch"])),
    item(76, 401, "MEMORY", "Temporary Change (Memory Channel Configurations)", 3, Kind::Choice(&["Off", "On"])),
    item(77, 402, "MEMORY", "Program Slow Scan", 3, Kind::Choice(&["Off", "On"])),
    item(78, 403, "MEMORY", "Program Slow Scan Range", 3, Kind::Choice(&["100 Hz", "200 Hz", "300 Hz", "400 Hz", "500 Hz"])),
    item(79, 404, "MEMORY", "Scan Hold", 3, Kind::Choice(&["Off", "On"])),
    item(80, 405, "MEMORY", "Scan Resume", 3, Kind::Choice(&["Time-operated", "Carrier-operated"])),
    item(81, 500, "CW", "Paddle Jack Configuration (Front)", 3, Kind::Choice(&["Key", "Paddle", "Paddle (Bug key mode)"])),
    item(82, 501, "CW", "Key Jack Configuration (Rear)", 3, Kind::Choice(&["Key", "Paddle", "Paddle (Bug key mode)"])),
    item(83, 502, "CW", "Electronic Keyer Squeeze Mode", 3, Kind::Choice(&["Mode A", "Mode B"])),
    item(84, 503, "CW", "Dot and Dash Reversed Keying", 3, Kind::Choice(&["Off", "On"])),
    item(85, 504, "CW", "Paddle (Microphone Up/Down Keys)", 3, Kind::Choice(&["Off", "On"])),
    item(86, 505, "CW", "Automatic CW TX with Keying in SSB Mode", 3, Kind::Choice(&["Off", "On"])),
    item(87, 506, "CW", "Carrier Frequency Offset (SSB Mode to CW Mode)", 3, Kind::Choice(&["Off", "On"])),
    // 000 = Automatic, 001-016 = 2.5-4.0 (steps of 0.1).
    item(88, 507, "CW", "CW Keying Weight Ratio", 3, Kind::Coded(&[(0, "Automatic"), (1, "2.5"), (2, "2.6"), (3, "2.7"), (4, "2.8"), (5, "2.9"), (6, "3.0"), (7, "3.1"), (8, "3.2"), (9, "3.3"), (10, "3.4"), (11, "3.5"), (12, "3.6"), (13, "3.7"), (14, "3.8"), (15, "3.9"), (16, "4.0")])),
    item(89, 508, "CW", "CW Keying Reversed Weight Ratio", 3, Kind::Choice(&["Off", "On"])),
    item(90, 509, "CW", "Interrupt Keying", 3, Kind::Choice(&["Off", "On"])),
    item(91, 510, "CW", "CW Message Entry", 3, Kind::Choice(&["Text string", "Paddle"])),
    // Must be a 4-digit number (0001-9999).
    item(92, 511, "CW", "Contest Number", 4, num(1, 9999, 1, "")),
    item(93, 512, "CW", "Contest Number Format", 3, Kind::Choice(&["Off", "190 to ANO", "190 to ANT", "90 to NO", "90 to NT"])),
    item(94, 513, "CW", "Channel Number (Count-up Message)", 3, Kind::Choice(&["Off", "Ch 1", "Ch 2", "Ch 3", "Ch 4", "Ch 5", "Ch 6", "Ch 7", "Ch 8"])),
    item(95, 514, "CW", "CW Rise Time", 3, Kind::Choice(&["1 ms", "2 ms", "4 ms", "6 ms"])),
    item(96, 515, "CW", "CW/Voice Message Retransmit Interval Time", 3, num(0, 60, 1, "s")),
    item(97, 600, "TX/RX", "Playback Time (Full-time Recording)", 3, Kind::Choice(&["Last 10 s", "Last 20 s", "Last 30 s"])),
    item(98, 601, "TX/RX", "Recorded Audio File Storage Location", 3, Kind::Choice(&["Internal", "USB"])),
    item(99, 602, "TX/RX", "Time-out Timer", 3, Kind::Choice(&["Off", "3 min", "5 min", "10 min", "20 min", "30 min"])),
    item(100, 603, "TX/RX", "TX Inhibit", 3, Kind::Choice(&["Off", "On"])),
    item(101, 604, "TX/RX", "Transmit Power Step Size", 3, Kind::Choice(&["1 W", "5 W"])),
    item(102, 605, "TX/RX", "TX Filter Numbers", 3, Kind::Choice(&["2", "3"])),
    item(103, 606, "TX/RX", "RX Filter Numbers", 3, Kind::Choice(&["2", "3"])),
    item(104, 607, "TX/RX", "Filter Control in SSB Mode (High/Low and Shift/Width)", 3, Kind::Choice(&["High & Low Cuts", "Shift & Width"])),
    item(105, 608, "TX/RX", "Filter Control in SSB-Data Mode (High/Low and Shift/Width)", 3, Kind::Choice(&["High & Low Cuts", "Shift & Width"])),
    item(106, 609, "TX/RX", "VOX Voice Delay (Microphone)", 3, Kind::Choice(&["Off", "Short", "Medium", "Long"])),
    item(107, 610, "TX/RX", "VOX Voice Delay (except Microphone)", 3, Kind::Choice(&["Off", "Short", "Medium", "Long"])),
    item(108, 700, "REAR I/O", "Baud Rate (COM Port)", 3, Kind::Choice(&["4800 bps", "9600 bps", "19200 bps", "38400 bps", "57600 bps", "115200 bps"])),
    item(109, 701, "REAR I/O", "Baud Rate (USB Port)", 3, Kind::Choice(&["4800 bps", "9600 bps", "19200 bps", "38400 bps", "57600 bps", "115200 bps"])),
    item(110, 702, "REAR I/O", "Quick Data Transfer", 3, Kind::Choice(&["Off", "On"])),
    item(111, 703, "REAR I/O", "Overwrite Location (Quick Data Transfer)", 3, Kind::Choice(&["VFO", "Quick Memory"])),
    item(112, 704, "REAR I/O", "Overwrite Location (DX Packet Cluster Tuned Data)", 3, Kind::Choice(&["Operating Band", "Sub Band"])),
    item(113, 705, "REAR I/O", "USB: Audio Input Level", 3, num(0, 100, 1, "")),
    item(114, 706, "REAR I/O", "ACC 2: Audio Input Level", 3, num(0, 100, 1, "")),
    item(115, 707, "REAR I/O", "Optical: Audio Input Level", 3, num(0, 100, 1, "")),
    item(116, 708, "REAR I/O", "USB: Audio Output Level (Main Band)", 3, num(0, 100, 1, "")),
    item(117, 709, "REAR I/O", "USB: Audio Output Level (Sub Band)", 3, num(0, 100, 1, "")),
    item(118, 710, "REAR I/O", "ACC 2: Audio Output Level (Main Band)", 3, num(0, 100, 1, "")),
    item(119, 711, "REAR I/O", "ACC 2: Audio Output Level (Sub Band)", 3, num(0, 100, 1, "")),
    item(120, 712, "REAR I/O", "Optical: Audio Output Level (Main Band)", 3, num(0, 100, 1, "")),
    item(121, 713, "REAR I/O", "Optical: Audio Output Level (Sub Band)", 3, num(0, 100, 1, "")),
    item(122, 714, "REAR I/O", "Audio Output Type (Rear Connectors)", 3, Kind::Choice(&["All", "Received Audio Only"])),
    item(123, 715, "REAR I/O", "Speaker Output Configuration", 3, Kind::Choice(&["Normal", "Reversed", "Mixed"])),
    item(124, 716, "REAR I/O", "USB: Audio Output Configuration", 3, Kind::Choice(&["Normal", "Reversed", "Mixed"])),
    item(125, 717, "REAR I/O", "ACC2: Audio Output Configuration", 3, Kind::Choice(&["Normal", "Reversed", "Mixed"])),
    item(126, 718, "REAR I/O", "Optical: Audio Output Configuration", 3, Kind::Choice(&["Normal", "Reversed", "Mixed"])),
    // Firmware 1.20 or later.
    item(127, 719, "REAR I/O", "Antenna Numbers", 3, Kind::Coded(&[(1, "1"), (2, "2"), (3, "3"), (4, "4")])),
    item(128, 800, "SCOPE", "Bandscope Display During TX", 3, Kind::Choice(&["Off", "On"])),
    item(129, 801, "SCOPE", "TX Audio Signal Waveform Display", 3, Kind::Choice(&["Off", "On"])),
    item(130, 802, "SCOPE", "Bandscope Maximum Hold", 3, Kind::Choice(&["10 s", "Continuous"])),
    item(131, 803, "SCOPE", "Marker Offset Frequency (SSB Mode)", 3, Kind::Choice(&["Off (Carrier Point)", "300 Hz", "400 Hz", "500 Hz", "600 Hz", "700 Hz", "800 Hz", "1000 Hz", "1500 Hz", "2200 Hz"])),
    item(132, 804, "SCOPE", "Frequency Scale (Center Mode)", 3, Kind::Choice(&["Relative Frequency", "Absolute Frequency"])),
    // Fixed-mode scope limits: 8-digit frequency in Hz (unused digits 0), steps of 1 kHz.
    item(133, 805, "SCOPE", "Fixed Mode LF Band Lower Limit (min. 0.03 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(134, 806, "SCOPE", "Fixed Mode LF Band Upper Limit (max. 0.300 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(135, 807, "SCOPE", "Fixed Mode MF Band 1 Lower Limit (min. 0.300 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(136, 808, "SCOPE", "Fixed Mode MF Band 1 Upper Limit (max. 0.522 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(137, 809, "SCOPE", "Fixed Mode MF Band 2 Lower Limit (min. 0.522 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(138, 810, "SCOPE", "Fixed Mode MF Band 2 Upper Limit (max. 1.705 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(139, 811, "SCOPE", "Fixed Mode 1.8 MHz Band Lower Limit (min. 1.705 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(140, 812, "SCOPE", "Fixed Mode 1.8 MHz Band Upper Limit (max. 2.0 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(141, 813, "SCOPE", "Fixed Mode 3.5 MHz Band Lower Limit (min. 2.0 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(142, 814, "SCOPE", "Fixed Mode 3.5 MHz Band Upper Limit (max. 4.0 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(143, 815, "SCOPE", "Fixed Mode 5 MHz Band Lower Limit (min. 4.0 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(144, 816, "SCOPE", "Fixed Mode 5 MHz Band Upper Limit (max. 6.0 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(145, 817, "SCOPE", "Fixed Mode 7 MHz Band Lower Limit (min. 6.0 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(146, 818, "SCOPE", "Fixed Mode 7 MHz Band Upper Limit (max. 8.0 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(147, 819, "SCOPE", "Fixed Mode 10 MHz Band Lower Limit (min. 8.0 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(148, 820, "SCOPE", "Fixed Mode 10 MHz Band Upper Limit (max. 11 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(149, 821, "SCOPE", "Fixed Mode 14 MHz Band Lower Limit (min. 11 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(150, 822, "SCOPE", "Fixed Mode 14 MHz Band Upper Limit (max. 15 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(151, 823, "SCOPE", "Fixed Mode 18 MHz Band Lower Limit (min. 15 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(152, 824, "SCOPE", "Fixed Mode 18 MHz Band Upper Limit (max. 20 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(153, 825, "SCOPE", "Fixed Mode 21 MHz Band Lower Limit (min. 20 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(154, 826, "SCOPE", "Fixed Mode 21 MHz Band Upper Limit (max. 22 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(155, 827, "SCOPE", "Fixed Mode 24 MHz Band Lower Limit (min. 22 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(156, 828, "SCOPE", "Fixed Mode 24 MHz Band Upper Limit (max. 26 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(157, 829, "SCOPE", "Fixed Mode 28 MHz Band Lower Limit (min. 26 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(158, 830, "SCOPE", "Fixed Mode 28 MHz Band Upper Limit (max. 30 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(159, 831, "SCOPE", "Fixed Mode 50 MHz Band Lower Limit (min. 30 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    item(160, 832, "SCOPE", "Fixed Mode 50 MHz Band Upper Limit (max. 60 MHz)", 8, num(0, 99999999, 1000, "Hz")),
    // Firmware 1.20 or later.
    item(161, 833, "SCOPE", "Waterfall Pause when Tuning (Center Mode)", 3, Kind::Choice(&["Off", "On"])),
    // Firmware 1.20 or later.
    item(162, 834, "SCOPE", "Touchscreen Tuning Step Correction (SSB/CW/FSK/PSK)", 3, Kind::Choice(&["Off", "On"])),
    // Firmware 1.20 or later.
    item(163, 835, "SCOPE", "Waterfall Color Type", 3, Kind::Choice(&["Type1", "Type2"])),
    item(164, 900, "USB KEYBOARD", "Send Message by Function Keys (USB Keyboard)", 3, Kind::Choice(&["Off", "On"])),
    item(165, 901, "USB KEYBOARD", "Keyboard Language (USB Keyboard)", 3, Kind::Choice(&["Japanese", "English (US)", "English (UK)", "French", "French (Canadian)", "German", "Spanish", "Portuguese", "Portuguese (Brazilian)", "Spanish (Latin American)", "Italian"])),
    item(166, 902, "USB KEYBOARD", "Repeat Delay Time (USB Keyboard)", 3, Kind::Coded(&[(1, "1"), (2, "2"), (3, "3"), (4, "4")])),
    item(167, 903, "USB KEYBOARD", "Repeat Speed (USB Keyboard)", 3, num(1, 32, 1, "")),
    // Advanced Menu (P1 = 1).
    item(168, 10000, "ADVANCED", "Indication Signal Type (Main Band)", 3, Kind::Choice(&["Automatic", "TX Power", "ALC", "Drain Voltage (Vd)", "Compression Level (COMP)", "Current (Id)", "SWR"])),
    item(169, 10001, "ADVANCED", "Indication Signal Type (Sub Band)", 3, Kind::Choice(&["TX Power", "ALC", "Drain Voltage (Vd)", "Compression Level (COMP)", "Current (Id)", "SWR"])),
    item(170, 10002, "ADVANCED", "Output Level (Main Band)", 3, num(0, 100, 1, "%")),
    item(171, 10003, "ADVANCED", "Output Level (Sub Band)", 3, num(0, 100, 1, "%")),
    item(172, 10004, "ADVANCED", "REF I/O Connector Configuration", 3, Kind::Choice(&["Off", "Output", "Input"])),
    // 000-510 = -255..+255.
    item(173, 10005, "ADVANCED", "Reference Oscillator Calibration", 3, Kind::Steps { max_code: 510, off_at_zero: false, first: -255, step: 1, unit: "" }),
    // 000 = Off, 001-033 = 300-3500 Hz (steps of 100).
    item(174, 10006, "ADVANCED", "Bandwidth (Additional Roofing Filter)", 3, Kind::Steps { max_code: 33, off_at_zero: true, first: 300, step: 100, unit: "Hz" }),
    // 000-040 = -20..+20.
    item(175, 10007, "ADVANCED", "Attenuation (Additional Roofing Filter)", 3, Kind::Steps { max_code: 40, off_at_zero: false, first: -20, step: 1, unit: "" }),
    item(176, 10008, "ADVANCED", "TX Power Down with Transverter Enabled", 3, Kind::Choice(&["Off", "On"])),
    item(177, 10009, "ADVANCED", "TX Hold After Antenna Tuning", 3, Kind::Choice(&["Off", "On"])),
    item(178, 10010, "ADVANCED", "Antenna Tuner During RX", 3, Kind::Choice(&["Off", "On"])),
    item(179, 10011, "ADVANCED", "Linear Amplifier Control (HF Band)", 3, Kind::Choice(&["Off", "Active High", "Active High + Relay Control", "Active High + Relay & TX Delay Ctrl", "Active Low", "Active Low + TX Delay Control"])),
    item(180, 10012, "ADVANCED", "Linear Amplifier Control (50 MHz Band)", 3, Kind::Choice(&["Off", "Active High", "Active High + Relay Control", "Active High + Relay & TX Delay Ctrl", "Active Low", "Active Low + TX Delay Control"])),
    item(181, 10013, "ADVANCED", "Microphone Gain (FM Mode)", 3, num(1, 100, 1, "")),
    item(182, 10014, "ADVANCED", "PKS Polarity Reverse", 3, Kind::Choice(&["Off", "On"])),
    item(183, 10015, "ADVANCED", "TX Inhibit While Busy", 3, Kind::Choice(&["Off", "On"])),
    item(184, 10016, "ADVANCED", "CTCSS Unmute for Internal Speaker (Main Band)", 3, Kind::Choice(&["Mute", "Unmute"])),
    item(185, 10017, "ADVANCED", "CTCSS Unmute for Internal Speaker (Sub Band)", 3, Kind::Choice(&["Mute", "Unmute"])),
    item(186, 10018, "ADVANCED", "MSQ Logic State", 3, Kind::Choice(&["Low", "Open"])),
    item(187, 10019, "ADVANCED", "SSQ Logic State", 3, Kind::Choice(&["Low", "Open"])),
    item(188, 10020, "ADVANCED", "MSQ Reverse Condition", 3, Kind::Choice(&["Off", "Busy", "Sql", "Send", "Busy-Send", "Sql-Send"])),
    item(189, 10021, "ADVANCED", "SSQ Reverse Condition", 3, Kind::Choice(&["Off", "Busy", "Sql", "Send", "Busy-Send", "Sql-Send"])),
    item(190, 10022, "ADVANCED", "Standby State Low Power Consumption", 3, Kind::Choice(&["Off", "On"])),
    item(191, 10023, "ADVANCED", "Cooling Fan Control After Shutdown", 3, Kind::Choice(&["Off", "On"])),
    item(192, 10024, "ADVANCED", "MSQ/PKS Pin Assignment", 3, Kind::Choice(&["Off", "On"])),
    item(193, 10025, "ADVANCED", "External Display", 3, Kind::Choice(&["Off", "On"])),
    item(194, 10026, "ADVANCED", "Resolution (External Display)", 3, Kind::Choice(&["800x600", "840x480"])),
];
