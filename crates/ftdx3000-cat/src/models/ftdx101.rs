//! Yaesu FTDX101D and FTDX101MP (one CAT manual covers both; the MP is the
//! 200 W one and has a higher AM power limit, otherwise they are the same).
//!
//! Also the translator the FTDX10 and FT-710 use ([`down_with`] /
//! [`up_with`]): the three are one CAT generation, which differs from the
//! FTDX3000's in the same ways:
//!
//! * 9-digit frequencies (`FA014250000;`), in `IF`/`OI`/`MR`/`MW` too, and
//!   PMS memories named `P1L`..`P9U` (RigHelm's 100..117);
//! * a MAIN/SUB P1 on most commands (RigHelm talks to MAIN), and a fixed
//!   extra P2 in `SH` (`SH0 0 nn`), `IS` (`IS0 0 +nnnn`) and `CN`;
//! * mode `E` is PSK and `F` DATA-FM-N (RigHelm's `I` and `J`);
//! * the menu is `EX` with a 6-digit P1 P2 P3 address;
//! * `CO` has separate contour and APF switches, the contour frequency in Hz;
//! * `SD` (break-in delay) is a 2-digit code, `SQ` is 0-100, `RM` answers
//!   carry a second (unused) meter, `DA` has other fields;
//! * split is also `ST`, which programs (Hamlib) use.
//!
//! Where a radio's command has a Set that reads like an Answer of another
//! meaning (the FT-710's `FT1;` sets SUB as transmitter, RigHelm's `FT1;`
//! answers it), `up` tells programs' commands from the radio's answers by
//! which side spoke first: towards the radio RigHelm writes before the radio
//! says anything, towards a program the program does (see [`program_side`]).

use crate::dialect::text::{Ctx, TextSpec, freq_digits, insert_param, is_read, menu_down, menu_up, parts, read_prefix, remove_param};
use crate::menu::{Kind, MenuItem, num, signed};
use crate::model::{Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::types::{self, Band, Mode, WidthClass};

pub static SPEC: TextSpec = TextSpec { down, up, answer_prefix: read_prefix };

fn down(cx: &mut Ctx, f: &str) -> Vec<String> {
    down_with(cx, f, &FLAVOR)
}

fn up(cx: &mut Ctx, f: &str) -> Vec<String> {
    up_with(cx, f, &FLAVOR)
}

/// How one radio of this generation differs from the others.
#[derive(Debug)]
pub(super) struct Flavor {
    /// `MS P1 P2`: two meter selectors (FTDX101); otherwise P1 is the one
    /// meter and P2 is fixed.
    pub dual_meter: bool,
    /// `DA` P2 is the TFT contrast (FTDX10, FT-710) rather than fixed.
    pub da_contrast: bool,
    /// `ED`/`EU` encoders: (RigHelm's P1, the radio's P1).
    pub encoders: &'static [(u8, u8)],
    /// `FT0;`/`FT1;` select the transmitter (FT-710); otherwise the radio
    /// takes `FT2;`/`FT3;` like RigHelm.
    pub ft_select: bool,
    /// The clarifier is `CF` only (no `RT`/`XT`/`RC`/`RU`/`RD`).
    pub clar_cf: bool,
    /// `LK` locks only the main dial (`LK0;`/`LK1;`).
    pub single_lock: bool,
    /// `RI0;` answers every flag in one frame, busy included (FT-710).
    pub packed_ri: bool,
    /// `AC` P3 code that starts tuning.
    pub tune_start: char,
    /// `KY P1 P2`: text or message memory, then the channel (FT-710).
    pub ky_split: bool,
    /// VC tune (`VT`) is RigHelm's VRF (`VF`).
    pub vc_tune: bool,
}

pub(super) static FLAVOR: Flavor = Flavor {
    dual_meter: true,
    da_contrast: false,
    // MAIN, SUB (MPVD), MIC/SPEED, PROC/PITCH, NOTCH, CONTOUR.
    encoders: &[(0, 0), (1, 1), (4, 2), (5, 3), (6, 4), (7, 5)],
    ft_select: false,
    clar_cf: false,
    single_lock: false,
    packed_ri: false,
    tune_start: '2',
    ky_split: false,
    vc_tune: true,
};

// ------------------------------------------------------------- helpers

/// RigHelm's mode code as this generation's (`I` PSK -> `E`, `J`
/// DATA-FM-N -> `F`); `None` for modes it doesn't have.
pub(super) fn mode_down(c: char) -> Option<char> {
    match c.to_ascii_uppercase() {
        c @ ('1'..='9' | 'A'..='D') => Some(c),
        'I' => Some('E'),
        'J' => Some('F'),
        _ => None,
    }
}

pub(super) fn mode_up(c: char) -> char {
    match c.to_ascii_uppercase() {
        'E' => 'I',
        'F' => 'J',
        c => c,
    }
}

/// A memory channel number as the radio writes it: `001`-`099`, `P1L`-`P9U`
/// for RigHelm's PMS channels 100-117, `EMG` (118), `5xx` (5 MHz).
pub(super) fn chan_down(n: u16) -> String {
    match n {
        100..=117 => {
            let k = n - 100;
            format!("P{}{}", k / 2 + 1, if k.is_multiple_of(2) { 'L' } else { 'U' })
        }
        118 => "EMG".into(),
        n => format!("{n:03}"),
    }
}

pub(super) fn chan_up(s: &str) -> Option<u16> {
    let b = s.as_bytes();
    if b.len() == 3 && b[0].eq_ignore_ascii_case(&b'P') {
        let d = (b[1] as char).to_digit(10).filter(|d| (1..=9).contains(d))? as u16;
        let u = match b[2].to_ascii_uppercase() {
            b'L' => 0,
            b'U' => 1,
            _ => return None,
        };
        return Some(100 + (d - 1) * 2 + u);
    }
    if s.eq_ignore_ascii_case("EMG") {
        return Some(118);
    }
    s.parse().ok().filter(|_| s.len() == 3)
}

/// Is this `up` call a program's command (rather than the radio's answer)?
/// Whichever side a translator hears first decides: RigHelm talks to the
/// radio before the radio says anything, a program talks to an emulated
/// radio before it answers.
pub(super) fn program_side(cx: &mut Ctx) -> bool {
    match cx.recall("side") {
        Some(s) => s == "program",
        None => {
            cx.remember("side", "program");
            true
        }
    }
}

fn radio_side(cx: &mut Ctx) {
    if cx.recall("side").is_none() {
        cx.remember("side", "radio");
    }
}

fn flag(cx: &Ctx, key: &str) -> bool {
    cx.recall(key) == Some("1")
}

fn set_flag(cx: &mut Ctx, key: &str, on: bool) {
    cx.remember(key, if on { "1" } else { "0" });
}

/// Did the program ask this (a Read RigHelm answers with another
/// command)? Forgets it.
fn asked(cx: &mut Ctx, key: &str) -> bool {
    let a = cx.recall(key).is_some();
    cx.memo.remove(key);
    a
}

fn scale(v: u32, from: u32, to: u32) -> u32 {
    ((v.min(from) as f32) * to as f32 / from as f32).round() as u32
}

/// `SD` break-in delay codes: 00 30 ms, 01 50, 02 100, 03 150, 04 200,
/// 05 250, 06 300, then 100 ms steps up to 33 = 3000 ms.
pub(super) fn sd_ms(code: u32) -> u32 {
    match code {
        0 => 30,
        1 => 50,
        2..=6 => 50 * code,
        n => 300 + (n.min(33) - 6) * 100,
    }
}

pub(super) fn sd_code(ms: u32) -> u32 {
    (0..=33).min_by_key(|&c| sd_ms(c).abs_diff(ms)).unwrap_or(0)
}

/// `IF`/`OI`/`MR` answers and `MW` sets, RigHelm's layout -> the radio's.
fn info_down(cx: &mut Ctx, code: &str, p: &str, fl: &Flavor) -> Vec<String> {
    if p.is_empty() {
        return vec![format!("{code};")];
    }
    if p.len() == 3 {
        // MR read.
        return p.parse().map(|n| vec![format!("{code}{};", chan_down(n))]).unwrap_or_default();
    }
    let Some(flen) = p.len().checked_sub(16).filter(|n| (8..=9).contains(n)) else { return Vec::new() };
    let (Ok(ch), Ok(hz)) = (p[..3].parse::<u16>(), p[3..3 + flen].parse::<u64>()) else { return Vec::new() };
    let rest = &p[3 + flen..];
    let Some(mode) = rest[7..8].chars().next().and_then(mode_down) else { return Vec::new() };
    if code == "IF" && fl.clar_cf {
        // A program asked CF (the clarifier); RigHelm's IF answers it.
        if asked(cx, "ask_cf0") {
            return vec![format!("CF000{}{}000;", &rest[5..6], &rest[6..7])];
        }
        if asked(cx, "ask_cf1") {
            return vec![format!("CF001{};", &rest[..5])];
        }
    }
    vec![format!("{code}{}{hz:09}{}{mode}{};", chan_down(ch), &rest[..7], &rest[8..])]
}

fn info_up(cx: &mut Ctx, code: &str, p: &str, fl: &Flavor) -> Vec<String> {
    if p.is_empty() {
        return vec![format!("{code};")];
    }
    if p.len() == 3 {
        return chan_up(p).map(|n| vec![format!("{code}{n:03};")]).unwrap_or_default();
    }
    if p.len() != 25 || !p.is_ascii() {
        return Vec::new();
    }
    let (Some(ch), Ok(hz)) = (chan_up(&p[..3]), p[3..12].parse::<u64>()) else { return Vec::new() };
    let rest = &p[12..];
    if code == "IF" && fl.clar_cf {
        cx.remember("clar", rest[..5].trim_start_matches('+'));
        set_flag(cx, "rt", &rest[5..6] == "1");
        set_flag(cx, "xt", &rest[6..7] == "1");
    }
    let mode = rest[7..8].chars().next().map(mode_up).unwrap_or('2');
    vec![format!("{code}{ch:03}{hz:08}{}{mode}{};", &rest[..7], &rest[8..])]
}

/// RigHelm's `CO`: CO00 0 off / 1 contour / 2 APF, CO01 1-40 (x100 Hz),
/// CO02 0-50 (APF -250..+250 Hz). The radio: CO0 0 contour on/off, CO0 1
/// contour Hz, CO0 2 APF on/off, CO0 3 APF 0-50.
fn co_down(cx: &mut Ctx, p: &str, read: bool) -> Vec<String> {
    let Some(p2) = p.get(1..2) else { return Vec::new() };
    if read {
        return match p2 {
            "0" => vec!["CO00;".into(), "CO02;".into()],
            "1" => vec!["CO01;".into()],
            "2" => vec!["CO03;".into()],
            _ => Vec::new(),
        };
    }
    let Ok(v) = p[2..].parse::<u32>() else { return Vec::new() };
    match p2 {
        "0" => {
            set_flag(cx, "contour", v == 1);
            set_flag(cx, "apf", v == 2);
            // The one switched on last, so the last answer has both.
            match v {
                1 => vec!["CO020000;".into(), "CO000001;".into()],
                2 => vec!["CO000000;".into(), "CO020001;".into()],
                _ => vec!["CO000000;".into(), "CO020000;".into()],
            }
        }
        "1" => vec![format!("CO01{:04};", (v * 100).clamp(10, 3200))],
        "2" => vec![format!("CO03{:04};", v.min(50))],
        _ => Vec::new(),
    }
}

fn co_up(cx: &mut Ctx, p: &str) -> Vec<String> {
    let Some(p2) = p.get(1..2) else { return Vec::new() };
    if p.len() == 2 {
        return match p2 {
            "0" | "2" => vec!["CO00;".into()],
            "1" => vec!["CO01;".into()],
            "3" => vec!["CO02;".into()],
            _ => Vec::new(),
        };
    }
    let Ok(v) = p[2..].parse::<u32>() else { return Vec::new() };
    let mode = |contour: bool, apf: bool, apf_first: bool| match (contour, apf) {
        (true, true) => if apf_first { 2 } else { 1 },
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 0,
    };
    match p2 {
        "0" => {
            set_flag(cx, "contour", v != 0);
            vec![format!("CO00{:02};", mode(v != 0, flag(cx, "apf"), false))]
        }
        "2" => {
            set_flag(cx, "apf", v != 0);
            vec![format!("CO00{:02};", mode(flag(cx, "contour"), v != 0, true))]
        }
        "1" => vec![format!("CO01{:02};", ((v as f32 / 100.0).round() as u32).clamp(1, 40))],
        "3" => vec![format!("CO02{:02};", v.min(50))],
        _ => Vec::new(),
    }
}

/// RigHelm's `DA` (FTDX3000: VFO display, keypad, TFT, 00-15) as the
/// radio's `DA 00 P2 tft led` (00-20; P2 fixed or the TFT contrast).
fn da_down(p: &str, fl: &Flavor) -> Vec<String> {
    let v: Vec<u32> = (0..3).filter_map(|i| p.get(i * 2..i * 2 + 2)?.parse().ok()).collect();
    let [a, b, c] = v[..] else { return Vec::new() };
    let p2 = if fl.da_contrast { scale(b, 15, 20) } else { 0 };
    vec![format!("DA00{p2:02}{:02}{:02};", scale(c, 15, 20), scale(a, 15, 20))]
}

fn da_up(p: &str, fl: &Flavor) -> Vec<String> {
    let v: Vec<u32> = (0..4).filter_map(|i| p.get(i * 2..i * 2 + 2)?.parse().ok()).collect();
    let [_, contrast, tft, led] = v[..] else { return Vec::new() };
    let a = scale(led, 20, 15);
    let b = if fl.da_contrast { scale(contrast, 20, 15) } else { a };
    vec![format!("DA{a:02}{b:02}{:02};", scale(tft, 20, 15))]
}

/// RigHelm's `MS` codes for the FTDX101's two meter selectors: 0-2 the
/// MAIN meter (P1: PO, COMP, TEMP), 3-6 the SUB meter (P2: ALC, VDD, ID,
/// SWR). Elsewhere RigHelm's code is the radio's P1.
fn ms_down(cx: &mut Ctx, p: &str, fl: &Flavor) -> Vec<String> {
    let Ok(n) = p.parse::<u8>() else { return Vec::new() };
    if !fl.dual_meter {
        return vec![format!("MS{n}0;")];
    }
    let cur = cx.recall("ms").unwrap_or("00").to_string();
    let (mut p1, mut p2) = (cur[..1].to_string(), cur[1..2].to_string());
    match n {
        0..=2 => {
            p1 = n.to_string();
            cx.remember("ms_side", "main");
        }
        3..=6 => {
            p2 = (n - 3).to_string();
            cx.remember("ms_side", "sub");
        }
        _ => return Vec::new(),
    }
    cx.remember("ms", format!("{p1}{p2}"));
    vec![format!("MS{p1}{p2};")]
}

fn ms_up(cx: &mut Ctx, p: &str, fl: &Flavor) -> Vec<String> {
    let Some(p1) = p.get(..1).and_then(|c| c.parse::<u8>().ok()) else { return Vec::new() };
    if !fl.dual_meter {
        return vec![format!("MS{p1};")];
    }
    let Some(p2) = p.get(1..2).and_then(|c| c.parse::<u8>().ok()) else { return Vec::new() };
    cx.remember("ms", &p[..2]);
    if cx.recall("ms_side") == Some("main") { vec![format!("MS{p1};")] } else { vec![format!("MS{};", p2 + 3)] }
}

/// FR: RigHelm 0 A receives, 1 A muted, 4 B receives, 5 B muted; the
/// FTDX101 mutes MAIN (P1) and SUB (P2) separately (both on = dual watch,
/// which RigHelm sees as receiving on A and leaves alone).
fn fr_down(cx: &Ctx, p: &str) -> Vec<String> {
    let v = match p {
        "" => return vec!["FR;".into()],
        "0" if cx.recall("fr") == Some("00") => "00",
        "0" => "01",
        "1" | "5" => "11",
        "4" => "10",
        _ => return Vec::new(),
    };
    vec![format!("FR{v};")]
}

fn fr_up(cx: &mut Ctx, p: &str) -> Vec<String> {
    if p.is_empty() {
        return vec!["FR;".into()];
    }
    if p.len() != 2 {
        return Vec::new();
    }
    cx.remember("fr", p);
    let v = match (&p[..1], &p[1..]) {
        ("0", _) => '0',
        (_, "0") => '4',
        _ => '1',
    };
    vec![format!("FR{v};")]
}

/// FT-710 clarifier (`CF0 0 0 rx tx 000`, `CF0 0 1 ±nnnn`) for RigHelm's
/// RT/XT/RC/RU/RD.
fn clar_down(cx: &mut Ctx, code: &str, p: &str) -> Vec<String> {
    let both = |cx: &Ctx| format!("CF000{}{}000;", flag(cx, "rt") as u8, flag(cx, "xt") as u8);
    let offset = |cx: &mut Ctx, v: i32| {
        let v = v.clamp(-9999, 9999);
        cx.remember("clar", v.to_string());
        vec![format!("CF001{}{:04};", if v < 0 { '-' } else { '+' }, v.unsigned_abs())]
    };
    let cur: i32 = cx.recall("clar").and_then(|v| v.parse().ok()).unwrap_or(0);
    match (code, p) {
        ("RT" | "XT", "") => vec!["CF000;".into()],
        ("RT" | "XT", "0" | "1") => {
            set_flag(cx, if code == "RT" { "rt" } else { "xt" }, p == "1");
            vec![both(cx)]
        }
        ("RC", "") => offset(cx, 0),
        ("RU" | "RD", _) => match p.parse::<i32>() {
            Ok(v) => offset(cx, if code == "RU" { cur + v } else { cur - v }),
            Err(_) => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn cf_up(cx: &mut Ctx, p: &str, program: bool) -> Vec<String> {
    match (p.get(..3), p.len()) {
        (Some("000"), 3) => {
            cx.remember("ask_cf0", "1");
            vec!["IF;".into()]
        }
        (Some("001"), 3) => {
            cx.remember("ask_cf1", "1");
            vec!["IF;".into()]
        }
        (Some("000"), 8) => {
            let (rx, tx) = (&p[3..4] == "1", &p[4..5] == "1");
            set_flag(cx, "rt", rx);
            set_flag(cx, "xt", tx);
            vec![format!("RT{};", rx as u8), format!("XT{};", tx as u8)]
        }
        (Some("001"), 8) => {
            let Ok(v) = p[3..].trim_start_matches('+').parse::<i32>() else { return Vec::new() };
            cx.remember("clar", v.to_string());
            match (program, v) {
                // A program sets the offset; the radio's own report of it
                // is in IF.
                (false, _) => Vec::new(),
                (true, 0) => vec!["RC;".into()],
                (true, v) if v > 0 => vec!["RC;".into(), format!("RU{v:04};")],
                (true, v) => vec!["RC;".into(), format!("RD{:04};", -v)],
            }
        }
        _ => Vec::new(),
    }
}

/// FT-710 `RI0;`: every flag in one answer (`RI0 swr rec/play tx 0 tuner
/// scan busy`), RigHelm's RI0 (Hi-SWR), RI3 (REC), RI4 (PLAY) and BY.
fn ri_packed_down(cx: &mut Ctx, code: &str, p: &str, read: bool) -> Vec<String> {
    if read {
        return match (code, p) {
            ("BY", _) | ("RI", "0" | "3" | "4") => vec!["RI0;".into()],
            _ => Vec::new(),
        };
    }
    let key = match (code, p.get(..1)) {
        ("BY", _) => "by",
        ("RI", Some("0")) => "ri0",
        ("RI", Some("3")) => "ri3",
        ("RI", Some("4")) => "ri4",
        _ => return Vec::new(),
    };
    let v = if code == "BY" { &p[..1] } else { p.get(1..2).unwrap_or("0") };
    set_flag(cx, key, v == "1");
    let recplay = if flag(cx, "ri3") { 1 } else if flag(cx, "ri4") { 2 } else { 0 };
    vec![format!("RI0{}{recplay}0000{};", flag(cx, "ri0") as u8, flag(cx, "by") as u8)]
}

fn ri_packed_up(cx: &mut Ctx, p: &str) -> Vec<String> {
    if p == "0" {
        return vec!["RI0;".into()];
    }
    if p.len() != 8 || !p.starts_with('0') {
        return Vec::new();
    }
    let b = p.as_bytes();
    let on = |i: usize, v: u8| (b[i] == v) as u8;
    for (k, v) in [("ri0", on(1, b'1')), ("ri3", on(2, b'1')), ("ri4", on(2, b'2')), ("by", on(7, b'1'))] {
        set_flag(cx, k, v == 1);
    }
    vec![format!("RI0{};", on(1, b'1')), format!("RI3{};", on(2, b'1')), format!("RI4{};", on(2, b'2')), format!("BY{}0;", on(7, b'1'))]
}

// ------------------------------------------------------------- down / up

/// Commands whose P1 picks MAIN (0) or SUB (1); RigHelm only has MAIN.
const MAIN_SUB: &[&str] = &[
    "AG", "AN", "BC", "BD", "BP", "BU", "CF", "CN", "CO", "CT", "GT", "IS", "MD", "NA", "NB", "NR", "OS", "PA", "RA", "RF",
    "RG", "RL", "SH", "SM", "SQ", "VT",
];

/// The RM meters RigHelm and these radios share (S, COMP, ALC, PO, SWR,
/// ID, VDD).
const METERS: &[char] = &['1', '3', '4', '5', '6', '7', '8'];

/// A RigHelm frame as this generation's.
pub(super) fn down_with(cx: &mut Ctx, f: &str, fl: &Flavor) -> Vec<String> {
    radio_side(cx);
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !cx.model.has(&code) {
        return Vec::new();
    }
    let read = is_read(f);
    let same = || vec![f.to_string()];
    let one = |s: String| vec![s];
    match code.as_str() {
        "FA" | "FB" => freq_digits(f, 9),
        "IF" | "OI" | "MR" | "MW" => info_down(cx, &code, p, fl),
        "MC" if read => same(),
        "MC" => p.parse().map(|n| one(format!("MC{};", chan_down(n)))).unwrap_or_default(),
        "MD" if read => same(),
        "MD" => p.get(1..2).and_then(|c| mode_down(c.chars().next()?)).map(|c| one(format!("MD0{c};"))).unwrap_or_default(),
        "EX" => menu_down(cx, f, 6),
        "SH" => insert_param(f, 1, "0", 1),
        "IS" if read => same(),
        "IS" => match p.get(1..).and_then(|v| v.trim_start_matches('+').parse::<i32>().ok()) {
            Some(v) => {
                let v = ((v as f32 / 20.0).round() as i32 * 20).clamp(-1200, 1200);
                one(format!("IS00{}{:04};", if v < 0 { '-' } else { '+' }, v.unsigned_abs()))
            }
            None => Vec::new(),
        },
        "CN" if read => one("CN00;".into()),
        "CN" => p.get(1..).and_then(|v| v.parse::<u16>().ok()).map(|v| one(format!("CN00{v:03};"))).unwrap_or_default(),
        "CO" => co_down(cx, p, read),
        "BP" if !read && p.get(1..2) == Some("1") => match p[2..].parse::<u16>() {
            Ok(v) => one(format!("BP01{:03};", v.clamp(1, 320))),
            Err(_) => Vec::new(),
        },
        "SD" if !read => p.parse().map(|ms| one(format!("SD{:02};", sd_code(ms)))).unwrap_or_default(),
        "SQ" if !read => match p.get(1..).and_then(|v| v.parse::<u32>().ok()) {
            Some(v) => one(format!("SQ0{:03};", scale(v, 255, 100))),
            None => Vec::new(),
        },
        "DA" if !read => da_down(p, fl),
        "MS" if !read => ms_down(cx, p, fl),
        "RM" => match p.chars().next() {
            Some(c) if METERS.contains(&c) => one(if read { f.to_string() } else { format!("RM{p}000;") }),
            _ => Vec::new(),
        },
        "FR" => fr_down(cx, p),
        "ED" | "EU" => {
            let rh = p.get(..1).and_then(|c| c.parse::<u8>().ok());
            match fl.encoders.iter().find(|(r, _)| Some(*r) == rh) {
                Some((_, radio)) => one(format!("{code}{radio}{};", &p[1..])),
                None => Vec::new(),
            }
        }
        "VF" if fl.vc_tune => one(format!("VT{p};")),
        "RI" | "BY" if fl.packed_ri => ri_packed_down(cx, &code, p, read),
        "RI" => match p.get(..1) {
            Some("0" | "3" | "4") => same(),
            _ => Vec::new(),
        },
        "LM" | "PB" => if p.starts_with('0') { same() } else { Vec::new() },
        "FT" => match p {
            "0" | "1" if asked(cx, "ask_st") => one(format!("ST{p};")),
            "2" | "3" if fl.ft_select => one(format!("FT{};", if p == "2" { 0 } else { 1 })),
            _ => same(),
        },
        "LK" if fl.single_lock => match p.parse::<u8>() {
            _ if read => same(),
            Ok(v @ (0 | 1)) => one(format!("LK{v};")),
            Ok(v @ 4..=7) => one(format!("LK{};", v & 1)),
            _ => Vec::new(),
        },
        "AC" if !read && p.len() == 3 => {
            let p3 = p.chars().nth(2).unwrap_or('0');
            one(format!("AC00{};", if p3 == '2' { fl.tune_start } else { p3 }))
        }
        "KY" if fl.ky_split => match p.chars().next().and_then(|c| c.to_digit(16)) {
            Some(n @ 1..=5) => one(format!("KY0{n};")),
            Some(n @ 6..=10) => one(format!("KY1{};", n - 5)),
            _ => Vec::new(),
        },
        "RT" | "XT" | "RC" | "RU" | "RD" if fl.clar_cf => clar_down(cx, &code, p),
        _ => same(),
    }
}

/// One of this generation's frames as RigHelm's.
pub(super) fn up_with(cx: &mut Ctx, f: &str, fl: &Flavor) -> Vec<String> {
    let program = program_side(cx);
    let mut out = up_inner(cx, f, fl, program);
    out.retain(|r| parts(r).is_some_and(|(c, _)| cx.model.has(&c)));
    out
}

fn up_inner(cx: &mut Ctx, f: &str, fl: &Flavor, program: bool) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if MAIN_SUB.contains(&code.as_str()) && p.starts_with('1') {
        return Vec::new();
    }
    let same = || vec![f.to_string()];
    let one = |s: String| vec![s];
    match code.as_str() {
        "FA" | "FB" => freq_digits(f, 8),
        "IF" | "OI" | "MR" | "MW" => info_up(cx, &code, p, fl),
        "MC" if p.is_empty() => same(),
        "MC" => chan_up(p).map(|n| one(format!("MC{n:03};"))).unwrap_or_default(),
        "MD" if p.len() == 2 => one(format!("MD0{};", mode_up(p.chars().nth(1).unwrap_or('2')))),
        "EX" => menu_up(cx, f, 6),
        "SH" | "IS" => remove_param(f, 1, 1, 1),
        "CN" if p.len() == 2 => one("CN0;".into()),
        "CN" => match p.get(2..).and_then(|v| v.parse::<u16>().ok()) {
            Some(v) if p.starts_with("00") && v < 50 => one(format!("CN0{v:02};")),
            _ => Vec::new(),
        },
        "CO" => co_up(cx, p),
        "SD" if !p.is_empty() => p.parse().map(|c| one(format!("SD{:04};", sd_ms(c)))).unwrap_or_default(),
        "SQ" if p.len() > 1 => match p[1..].parse::<u32>() {
            Ok(v) => one(format!("SQ0{:03};", scale(v, 100, 255))),
            Err(_) => Vec::new(),
        },
        "DA" if !p.is_empty() => da_up(p, fl),
        "MS" if !p.is_empty() => ms_up(cx, p, fl),
        "RM" => match p.chars().next() {
            Some(c) if METERS.contains(&c) && p.len() == 1 => same(),
            Some(c) if METERS.contains(&c) && p.len() >= 4 => one(format!("RM{};", &p[..4])),
            _ => Vec::new(),
        },
        "FR" => fr_up(cx, p),
        "ED" | "EU" => {
            let radio = p.get(..1).and_then(|c| c.parse::<u8>().ok());
            match fl.encoders.iter().find(|(_, r)| Some(*r) == radio) {
                Some((rh, _)) => one(format!("{code}{rh}{};", &p[1..])),
                None => Vec::new(),
            }
        }
        "VT" if fl.vc_tune => one(format!("VF{p};")),
        "VF" if fl.vc_tune => Vec::new(),
        "RI" if fl.packed_ri => ri_packed_up(cx, p),
        "BY" if fl.packed_ri => Vec::new(),
        "RI" => match p.get(..1) {
            Some("0" | "3" | "4") => same(),
            _ => Vec::new(),
        },
        "BY" if p.len() >= 2 => one(format!("BY{}0;", &p[..1])),
        "ST" if p.is_empty() => {
            cx.remember("ask_st", "1");
            one("FT;".into())
        }
        "ST" => match (p, program) {
            ("0", true) => one("FT2;".into()),
            ("1" | "2", true) => one("FT3;".into()),
            ("0", false) => one("FT0;".into()),
            ("1" | "2", false) => one("FT1;".into()),
            _ => Vec::new(),
        },
        "FT" if fl.ft_select && program => match p {
            "0" => one("FT2;".into()),
            "1" => one("FT3;".into()),
            _ => same(),
        },
        "LK" if fl.single_lock && !program => match p {
            "0" | "1" => one(format!("LK{};", 4 + (p == "1") as u8)),
            _ => same(),
        },
        "AC" if p.len() == 3 => {
            if &p[1..2] != "0" {
                return Vec::new();
            }
            let p3 = p.chars().nth(2).unwrap_or('0');
            one(format!("AC00{};", if p3 == fl.tune_start { '2' } else { p3 }))
        }
        "KY" if fl.ky_split => match (p.get(..1), p.get(1..2).and_then(|c| c.parse::<u32>().ok())) {
            (Some("0"), Some(n @ 1..=5)) => one(format!("KY{n};")),
            (Some("1"), Some(n @ 1..=5)) => one(format!("KY{:X};", n + 5)),
            _ => Vec::new(),
        },
        "CF" if fl.clar_cf => cf_up(cx, p, program),
        "RT" | "XT" | "RC" | "RU" | "RD" if fl.clar_cf => Vec::new(),
        _ => same(),
    }
}

// ------------------------------------------------------------- model tables

/// Table 3 (FTDX101): SSB 01-21, CW/RTTY/PSK 01-18. 00 is the default for
/// the roofing filter, whose width the radio doesn't say.
const SSB_101: [u32; 21] = [
    300, 400, 600, 850, 1100, 1200, 1500, 1650, 1800, 1950, 2100, 2200, 2300, 2400, 2500, 2600, 2700, 2800, 2900, 3000, 3200,
];
pub(super) const CW_101: [u32; 18] = [50, 100, 150, 200, 250, 300, 350, 400, 450, 500, 600, 800, 1200, 1400, 1700, 2000, 2400, 3000];

pub(super) fn table(t: &[u32]) -> Vec<(u8, u32)> {
    t.iter().enumerate().map(|(i, w)| (i as u8 + 1, *w)).collect()
}

fn widths(class: WidthClass, _narrow: bool) -> Vec<(u8, u32)> {
    match class {
        WidthClass::Ssb => table(&SSB_101),
        WidthClass::Cw | WidthClass::Rtty => table(&CW_101),
        WidthClass::Fixed => Vec::new(),
    }
}

/// Band select codes 00-10 like the FTDX3000's, 11 GEN, 12 MW, 17 70 MHz.
pub(super) const BANDS_101: &[Band] = &[
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
    Band { code: 17, label: "4", lo: 70_000_000, hi: 70_500_000 },
    Band { code: 12, label: "MW", lo: 522_000, hi: 1_710_000 },
    Band { code: 11, label: "GEN", lo: 30_000, hi: 75_000_000 },
];

/// Front-panel order.
pub(super) const MODES: &[Mode] = &[
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
    Mode::Psk,
    Mode::DataLsb,
    Mode::DataUsb,
    Mode::DataFm,
    Mode::DataFmN,
];

const COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AM", "AN", "BA", "BC", "BD", "BI", "BP", "BS", "BU", "BY", "CH", "CN", "CO", "CS", "CT", "DA",
    "DN", "ED", "EM", "EN", "EU", "EX", "FA", "FB", "FR", "FS", "FT", "GT", "ID", "IF", "IS", "KM", "KP", "KR", "KS", "KY",
    "LK", "LM", "MA", "MC", "MD", "MG", "ML", "MR", "MS", "MW", "MX", "NA", "NB", "NL", "NR", "OI", "OS", "PA", "PB", "PC",
    "PL", "PR", "PS", "QI", "QR", "QS", "RA", "RC", "RD", "RF", "RG", "RI", "RL", "RM", "RS", "RT", "RU", "SC", "SD", "SH",
    "SM", "SQ", "SV", "TX", "UL", "UP", "VD", "VF", "VG", "VM", "VS", "VX", "XT",
];

/// Set codes 1-5; the radio answers 6-A (the Set codes are listed too in
/// case it echoes them).
const ROOFING_SET: &[(char, &str)] = &[('1', "12 kHz"), ('2', "3 kHz"), ('3', "1.2 kHz"), ('4', "600 Hz"), ('5', "300 Hz")];
const ROOFING_ANSWERS: &[(char, &str)] = &[
    ('6', "12 kHz"),
    ('7', "3 kHz"),
    ('8', "1.2 kHz"),
    ('9', "600 Hz"),
    ('A', "300 Hz"),
    ('1', "12 kHz"),
    ('2', "3 kHz"),
    ('3', "1.2 kHz"),
    ('4', "600 Hz"),
    ('5', "300 Hz"),
];

const CAPS: Caps = Caps {
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
    antennas: 3,
    power: (5, 100),
    meter_select: &[(0, "PO"), (1, "COMP"), (2, "TEMP"), (3, "ALC"), (4, "VDD"), (5, "ID"), (6, "SWR")],
    voice_memories: 5,
    keyer_memories: 5,
    keyer_memory_len: 50,
    memory: Memory { first: 1, last: 117, pms: Some(100) },
    shared_clarifier: true,
};

const ROLES: Roles = Roles {
    // SSB/AM/FM/DATA MOD SOURCE: MIC or REAR (REAR SELECT then picks DATA
    // or USB).
    mic_sel: &[11, 25, 39, 58],
    mic_front: 0,
    mic_usb: 1,
    pc_keying: Some((101, 0, 2)),
    cat_tot: Some((129, "1")),
    cat_items: &[126, 127, 128, 129, 130],
    // CW-U and CW-L are modes of their own.
    cw_bfo: None,
    cw_pitch_display: Some((100, "1")),
    // PSK is a mode of its own; DATA always uses the SSB widths.
    data_other: None,
    data_disp: None,
    data_psk: false,
};

/// Simulator answers where the FTDX3000's defaults don't fit.
const SIM: &[&str] = &["MS3;", "SH014;"];

pub static FTDX101D: RadioModel = RadioModel {
    key: "ftdx101d",
    name: "FTDX101D",
    maker: Maker::Yaesu,
    family: Family::Yaesu,
    proto: Proto::Text(&SPEC),
    id: "0681",
    civ_addr: 0,
    hamlib: 1040,
    freq_min: 30_000,
    freq_max: 75_000_000,
    bands: BANDS_101,
    modes: MODES,
    menu: MENU_D,
    roles: ROLES,
    caps: CAPS,
    bauds: &[38400, 19200, 9600, 4800],
    stop_bits: 2,
    rts_cts: true,
    sim: SIM,
};

pub static FTDX101MP: RadioModel = RadioModel {
    key: "ftdx101mp",
    name: "FTDX101MP",
    id: "0682",
    hamlib: 1044,
    menu: MENU_MP,
    caps: Caps { power: (5, 200), ..CAPS },
    ..FTDX101D
};

// ------------------------------------------------------------- menu

pub(super) const fn mi(num: u16, addr: u32, group: &'static str, name: &'static str, width: u8, kind: Kind) -> MenuItem {
    MenuItem { num, addr, group, name, width, kind }
}

pub(super) const AGC_DELAY: Kind = num(20, 4000, 20, "ms");
pub(super) const LCUT: Kind = Kind::Steps { max_code: 19, off_at_zero: true, first: 100, step: 50, unit: "Hz" };
pub(super) const HCUT: Kind = Kind::Steps { max_code: 67, off_at_zero: true, first: 700, step: 50, unit: "Hz" };
pub(super) const SLOPE: Kind = Kind::Choice(&["6 dB/oct", "18 dB/oct"]);
pub(super) const OUT_SEL: Kind = Kind::Choice(&["MAIN", "SUB"]);
pub(super) const LEVEL: Kind = num(0, 100, 1, "");
pub(super) const TX_BPF: Kind = Kind::Choice(&["50-3050 Hz", "100-2900 Hz", "200-2800 Hz", "300-2700 Hz", "400-2600 Hz"]);
pub(super) const MOD_SRC: Kind = Kind::Choice(&["MIC", "REAR"]);
pub(super) const REAR_SEL: Kind = Kind::Choice(&["DATA", "USB"]);
pub(super) const RPTT: Kind = Kind::Choice(&["DAKY", "RTS", "DTR"]);
pub(super) const RPT_SHIFT_28: Kind = num(0, 1000, 10, "kHz");
pub(super) const RPT_SHIFT_50: Kind = num(0, 4000, 10, "kHz");
pub(super) const PSK_TONE: Kind = Kind::Choice(&["1000 Hz", "1500 Hz", "2000 Hz"]);
pub(super) const DATA_SHIFT: Kind = num(0, 3000, 10, "Hz");
pub(super) const NOR_REV: Kind = Kind::Choice(&["NOR", "REV"]);
pub(super) const SHIFT_PORT: Kind = Kind::Choice(&["SHIFT", "DTR", "RTS"]);
// The manual lists "1: 1275 Hz 2: 2125 Hz" and "1: 170 Hz 1: 200 Hz 2: 425
// Hz 3: 850 Hz"; read as codes from 0.
pub(super) const MARK_FREQ: Kind = Kind::Choice(&["1275 Hz", "2125 Hz"]);
pub(super) const SHIFT_FREQ: Kind = Kind::Choice(&["170 Hz", "200 Hz", "425 Hz", "850 Hz"]);
pub(super) const PSK_MODE: Kind = Kind::Choice(&["BPSK", "QPSK"]);
pub(super) const AFC_RANGE: Kind = Kind::Choice(&["8 Hz", "15 Hz", "30 Hz"]);
pub(super) const ENABLE: Kind = Kind::Choice(&["DISABLE", "ENABLE"]);
pub(super) const NEW_LINE: Kind = Kind::Choice(&["CR, LF or CR+LF", "CR+LF"]);
pub(super) const DIDDLE: Kind = Kind::Choice(&["OFF", "BLANK", "LTRS"]);
pub(super) const BAUDOT: Kind = Kind::Choice(&["CCITT", "US"]);
pub(super) const CW_AUTO: Kind = Kind::Choice(&["OFF", "50 MHz", "ON"]);
pub(super) const BK_IN_TYPE: Kind = Kind::Choice(&["SEMI", "FULL"]);
pub(super) const BK_IN_DELAY: Kind = Kind::Coded(&[
    (0, "30 ms"),
    (1, "50 ms"),
    (2, "100 ms"),
    (3, "150 ms"),
    (4, "200 ms"),
    (5, "250 ms"),
    (6, "300 ms"),
    (7, "400 ms"),
    (8, "500 ms"),
    (9, "600 ms"),
    (10, "700 ms"),
    (11, "800 ms"),
    (12, "900 ms"),
    (13, "1000 ms"),
    (14, "1100 ms"),
    (15, "1200 ms"),
    (16, "1300 ms"),
    (17, "1400 ms"),
    (18, "1500 ms"),
    (19, "1600 ms"),
    (20, "1700 ms"),
    (21, "1800 ms"),
    (22, "1900 ms"),
    (23, "2000 ms"),
    (24, "2100 ms"),
    (25, "2200 ms"),
    (26, "2300 ms"),
    (27, "2400 ms"),
    (28, "2500 ms"),
    (29, "2600 ms"),
    (30, "2700 ms"),
    (31, "2800 ms"),
    (32, "2900 ms"),
    (33, "3000 ms"),
]);
pub(super) const WAVE_SHAPE: Kind = Kind::Choice(&["1 ms", "2 ms", "4 ms", "6 ms"]);
pub(super) const FREQ_DISPLAY: Kind = Kind::Choice(&["DIRECT FREQ", "PITCH OFFSET"]);
pub(super) const PC_KEYING: Kind = Kind::Choice(&["OFF", "DAKY", "RTS", "DTR"]);
pub(super) const QSK_DELAY: Kind = Kind::Choice(&["15 ms", "20 ms", "25 ms", "30 ms"]);
pub(super) const OFF_ON: Kind = Kind::Choice(&["OFF", "ON"]);
pub(super) const KEYER_TYPE: Kind = Kind::Choice(&["OFF", "BUG", "ELEKEY-A", "ELEKEY-B", "ELEKEY-Y", "ACS"]);
pub(super) const CW_WEIGHT: Kind = Kind::Num { min: 25, max: 45, step: 1, unit: "", signed: false, div: 10 };
pub(super) const NUMBER_STYLE: Kind = Kind::Choice(&["1290", "AUNO", "AUNT", "A2NO", "A2NT", "12NO", "12NT"]);
pub(super) const CONTEST_NUMBER: Kind = num(1, 9999, 1, "");
pub(super) const CW_MEM: Kind = Kind::Choice(&["TEXT", "MESSAGE"]);
pub(super) const REPEAT_INTERVAL: Kind = num(1, 60, 1, "s");
pub(super) const DECODE_BW: Kind = Kind::Choice(&["25 Hz", "50 Hz", "100 Hz", "250 Hz"]);
pub(super) const HEADPHONE_MIX: Kind = Kind::Choice(&["SEPARATE", "COMBINE-1", "COMBINE-2"]);
pub(super) const ANT3_SELECT: Kind = Kind::Choice(&["TRX", "RX ANT"]);
pub(super) const NB_WIDTH: Kind = Kind::Choice(&["1 ms", "3 ms", "10 ms"]);
pub(super) const NB_REJECTION: Kind = Kind::Choice(&["10 dB", "30 dB", "50 dB"]);
pub(super) const RF_SQL: Kind = Kind::Choice(&["RF", "SQL"]);
pub(super) const TUNER_SELECT: Kind = Kind::Choice(&["INT", "EXT1", "EXT2", "EXT3"]);
pub(super) const RATE: Kind = Kind::Choice(&["4800 bps", "9600 bps", "19200 bps", "38400 bps"]);
pub(super) const TOT: Kind = Kind::Choice(&["10 ms", "100 ms", "1000 ms", "3000 ms"]);
pub(super) const QMB_CH: Kind = Kind::Choice(&["5 ch", "10 ch"]);
pub(super) const QUICK_SPLIT: Kind = signed(-20, 20, 1, "kHz");
pub(super) const TX_TOT: Kind = num(0, 30, 1, "min");
pub(super) const SCAN_RESUME: Kind = Kind::Choice(&["PAUSE", "TIME"]);
pub(super) const REF_FREQ_ADJ: Kind = signed(-25, 25, 1, "");
const CS_DIAL: Kind = Kind::Choice(&[
    "RF POWER", "MONI LEVEL", "DNR LEVEL", "NB LEVEL", "VOX GAIN", "VOX DELAY", "ANTI VOX", "STEP DIAL", "MEM CH", "GROUP",
    "R.FIL", "LEVEL",
]);
pub(super) const KEYBOARD: Kind = Kind::Choice(&[
    "JAPANESE",
    "ENGLISH (US)",
    "ENGLISH (UK)",
    "FRENCH",
    "FRENCH (CA)",
    "GERMAN",
    "PORTUGUESE",
    "PORTUGUESE (BR)",
    "SPANISH",
    "SPANISH (LATAM)",
    "ITALIAN",
]);
pub(super) const APF_WIDTH: Kind = Kind::Choice(&["NARROW", "MEDIUM", "WIDE"]);
pub(super) const CONTOUR_LEVEL: Kind = signed(-40, 20, 1, "dB");
pub(super) const CONTOUR_WIDTH: Kind = num(1, 11, 1, "");
pub(super) const DNR_LEVEL: Kind = num(1, 15, 1, "");
pub(super) const NOTCH_WIDTH: Kind = Kind::Choice(&["NARROW", "WIDE"]);
pub(super) const PROC_TYPE: Kind = Kind::Choice(&["COMP", "AMC"]);
pub(super) const AMC_RELEASE: Kind = Kind::Choice(&["FAST", "MID", "SLOW"]);
pub(super) const EQ1_FREQ: Kind = Kind::Steps { max_code: 7, off_at_zero: true, first: 100, step: 100, unit: "Hz" };
pub(super) const EQ2_FREQ: Kind = Kind::Steps { max_code: 9, off_at_zero: true, first: 700, step: 100, unit: "Hz" };
pub(super) const EQ3_FREQ: Kind = Kind::Steps { max_code: 18, off_at_zero: true, first: 1500, step: 100, unit: "Hz" };
pub(super) const EQ_LEVEL: Kind = signed(-20, 10, 1, "dB");
pub(super) const EQ_BWTH: Kind = num(1, 10, 1, "");
pub(super) const POWER_100W: Kind = num(5, 100, 1, "W");
const POWER_200W: Kind = num(5, 200, 1, "W");
pub(super) const POWER_50W: Kind = num(5, 50, 1, "W");
pub(super) const POWER_25W: Kind = num(5, 25, 1, "W");
pub(super) const VOX_SELECT: Kind = Kind::Choice(&["MIC", "DATA"]);
pub(super) const DIAL_STEP: Kind = Kind::Choice(&["5 Hz", "10 Hz"]);
const CH_STEP: Kind = Kind::Choice(&["1 kHz", "2.5 kHz", "5 kHz"]);
pub(super) const AM_CH_STEP: Kind = Kind::Choice(&["2.5 kHz", "5 kHz", "9 kHz", "10 kHz", "12.5 kHz", "25 kHz"]);
pub(super) const FM_CH_STEP: Kind = Kind::Choice(&["5 kHz", "6.25 kHz", "10 kHz", "12.5 kHz", "20 kHz", "25 kHz"]);
pub(super) const MAIN_STEPS: Kind = Kind::Choice(&["250", "500", "1000"]);
pub(super) const MPVD_STEPS: Kind = Kind::Choice(&["50", "250", "500"]);
pub(super) const MY_CALL_TIME: Kind = Kind::Choice(&["OFF", "0.5 s", "1 s", "2 s", "3 s", "5 s"]);
pub(super) const SCREEN_SAVER: Kind = Kind::Choice(&["OFF", "15 min", "30 min", "60 min"]);
pub(super) const BRIGHTNESS: Kind = num(0, 20, 1, "");
pub(super) const FREQ_STYLE: Kind = Kind::Choice(&["LIGHT", "BOLD"]);
pub(super) const RBW: Kind = Kind::Choice(&["HIGH", "MID", "LOW"]);
pub(super) const SCOPE_CTR: Kind = Kind::Choice(&["FILTER", "CAR POINT"]);
pub(super) const SENSITIVITY: Kind = Kind::Choice(&["NORMAL", "HI"]);
pub(super) const PIXEL: Kind = Kind::Choice(&["800x480", "800x600"]);

/// Table 2. The two models differ only in the HF/50 MHz and AM power
/// limits.
macro_rules! ftdx101_menu {
    ($hf:expr, $am:expr) => {
        &[
            mi(1, 10101, "MODE SSB", "AGC FAST DELAY", 4, AGC_DELAY),
            mi(2, 10102, "MODE SSB", "AGC MID DELAY", 4, AGC_DELAY),
            mi(3, 10103, "MODE SSB", "AGC SLOW DELAY", 4, AGC_DELAY),
            mi(4, 10104, "MODE SSB", "LCUT FREQ", 2, LCUT),
            mi(5, 10105, "MODE SSB", "LCUT SLOPE", 1, SLOPE),
            mi(6, 10106, "MODE SSB", "HCUT FREQ", 2, HCUT),
            mi(7, 10107, "MODE SSB", "HCUT SLOPE", 1, SLOPE),
            mi(8, 10108, "MODE SSB", "SSB OUT SELECT", 1, OUT_SEL),
            mi(9, 10109, "MODE SSB", "SSB OUT LEVEL", 3, LEVEL),
            mi(10, 10110, "MODE SSB", "TX BPF SEL", 1, TX_BPF),
            mi(11, 10111, "MODE SSB", "SSB MOD SOURCE", 1, MOD_SRC),
            mi(12, 10112, "MODE SSB", "REAR SELECT", 1, REAR_SEL),
            mi(13, 10113, "MODE SSB", "RPORT GAIN", 3, LEVEL),
            mi(14, 10114, "MODE SSB", "RPTT SELECT", 1, RPTT),
            mi(15, 10201, "MODE AM", "AGC FAST DELAY", 4, AGC_DELAY),
            mi(16, 10202, "MODE AM", "AGC MID DELAY", 4, AGC_DELAY),
            mi(17, 10203, "MODE AM", "AGC SLOW DELAY", 4, AGC_DELAY),
            mi(18, 10204, "MODE AM", "LCUT FREQ", 2, LCUT),
            mi(19, 10205, "MODE AM", "LCUT SLOPE", 1, SLOPE),
            mi(20, 10206, "MODE AM", "HCUT FREQ", 2, HCUT),
            mi(21, 10207, "MODE AM", "HCUT SLOPE", 1, SLOPE),
            mi(22, 10208, "MODE AM", "AM OUT SELECT", 1, OUT_SEL),
            mi(23, 10209, "MODE AM", "AM OUT LEVEL", 3, LEVEL),
            mi(24, 10210, "MODE AM", "TX BPF SEL", 1, TX_BPF),
            mi(25, 10211, "MODE AM", "AM MOD SOURCE", 1, MOD_SRC),
            mi(26, 10212, "MODE AM", "MIC GAIN", 4, Kind::MicGain),
            mi(27, 10213, "MODE AM", "REAR SELECT", 1, REAR_SEL),
            mi(28, 10214, "MODE AM", "RPORT GAIN", 3, LEVEL),
            mi(29, 10215, "MODE AM", "RPTT SELECT", 1, RPTT),
            mi(30, 10301, "MODE FM", "AGC FAST DELAY", 4, AGC_DELAY),
            mi(31, 10302, "MODE FM", "AGC MID DELAY", 4, AGC_DELAY),
            mi(32, 10303, "MODE FM", "AGC SLOW DELAY", 4, AGC_DELAY),
            mi(33, 10304, "MODE FM", "LCUT FREQ", 2, LCUT),
            mi(34, 10305, "MODE FM", "LCUT SLOPE", 1, SLOPE),
            mi(35, 10306, "MODE FM", "HCUT FREQ", 2, HCUT),
            mi(36, 10307, "MODE FM", "HCUT SLOPE", 1, SLOPE),
            mi(37, 10308, "MODE FM", "FM OUT SELECT", 1, OUT_SEL),
            mi(38, 10309, "MODE FM", "FM OUT LEVEL", 3, LEVEL),
            mi(39, 10310, "MODE FM", "FM MOD SOURCE", 1, MOD_SRC),
            mi(40, 10311, "MODE FM", "MIC GAIN", 4, Kind::MicGain),
            mi(41, 10312, "MODE FM", "REAR SELECT", 1, REAR_SEL),
            mi(42, 10313, "MODE FM", "RPORT GAIN", 3, LEVEL),
            mi(43, 10314, "MODE FM", "RPTT SELECT", 1, RPTT),
            mi(44, 10315, "MODE FM", "RPT SHIFT (28 MHz)", 4, RPT_SHIFT_28),
            mi(45, 10316, "MODE FM", "RPT SHIFT (50 MHz)", 4, RPT_SHIFT_50),
            mi(46, 10401, "MODE PSK/DATA", "AGC FAST DELAY", 4, AGC_DELAY),
            mi(47, 10402, "MODE PSK/DATA", "AGC MID DELAY", 4, AGC_DELAY),
            mi(48, 10403, "MODE PSK/DATA", "AGC SLOW DELAY", 4, AGC_DELAY),
            mi(49, 10404, "MODE PSK/DATA", "PSK TONE", 1, PSK_TONE),
            mi(50, 10405, "MODE PSK/DATA", "DATA SHIFT (SSB)", 4, DATA_SHIFT),
            mi(51, 10406, "MODE PSK/DATA", "LCUT FREQ", 2, LCUT),
            mi(52, 10407, "MODE PSK/DATA", "LCUT SLOPE", 1, SLOPE),
            mi(53, 10408, "MODE PSK/DATA", "HCUT FREQ", 2, HCUT),
            mi(54, 10409, "MODE PSK/DATA", "HCUT SLOPE", 1, SLOPE),
            mi(55, 10410, "MODE PSK/DATA", "DATA OUT SELECT", 1, OUT_SEL),
            mi(56, 10411, "MODE PSK/DATA", "DATA OUT LEVEL", 3, LEVEL),
            mi(57, 10412, "MODE PSK/DATA", "TX BPF SEL", 1, TX_BPF),
            mi(58, 10413, "MODE PSK/DATA", "DATA MOD SOURCE", 1, MOD_SRC),
            mi(59, 10414, "MODE PSK/DATA", "REAR SELECT", 1, REAR_SEL),
            mi(60, 10415, "MODE PSK/DATA", "RPORT GAIN", 3, LEVEL),
            mi(61, 10416, "MODE PSK/DATA", "RPTT SELECT", 1, RPTT),
            mi(62, 10501, "MODE RTTY", "AGC FAST DELAY", 4, AGC_DELAY),
            mi(63, 10502, "MODE RTTY", "AGC MID DELAY", 4, AGC_DELAY),
            mi(64, 10503, "MODE RTTY", "AGC SLOW DELAY", 4, AGC_DELAY),
            mi(65, 10504, "MODE RTTY", "POLARITY-RX", 1, NOR_REV),
            mi(66, 10505, "MODE RTTY", "POLARITY-TX", 1, NOR_REV),
            mi(67, 10506, "MODE RTTY", "LCUT FREQ", 2, LCUT),
            mi(68, 10507, "MODE RTTY", "LCUT SLOPE", 1, SLOPE),
            mi(69, 10508, "MODE RTTY", "HCUT FREQ", 2, HCUT),
            mi(70, 10509, "MODE RTTY", "HCUT SLOPE", 1, SLOPE),
            mi(71, 10510, "MODE RTTY", "RTTY OUT SELECT", 1, OUT_SEL),
            mi(72, 10511, "MODE RTTY", "RTTY OUT LEVEL", 3, LEVEL),
            mi(73, 10512, "MODE RTTY", "SHIFT PORT", 1, SHIFT_PORT),
            mi(74, 10513, "MODE RTTY", "MARK FREQUENCY", 1, MARK_FREQ),
            mi(75, 10514, "MODE RTTY", "SHIFT FREQUENCY", 1, SHIFT_FREQ),
            mi(76, 10601, "ENC/DEC PSK", "PSK MODE", 1, PSK_MODE),
            mi(77, 10602, "ENC/DEC PSK", "DECODE AFC RANGE", 1, AFC_RANGE),
            mi(78, 10603, "ENC/DEC PSK", "QPSK POLARITY RX", 1, NOR_REV),
            mi(79, 10604, "ENC/DEC PSK", "QPSK POLARITY TX", 1, NOR_REV),
            mi(80, 10605, "ENC/DEC PSK", "PSK TX LEVEL", 3, LEVEL),
            mi(81, 10701, "ENC/DEC RTTY", "RX USOS", 1, ENABLE),
            mi(82, 10702, "ENC/DEC RTTY", "TX USOS", 1, ENABLE),
            mi(83, 10703, "ENC/DEC RTTY", "RX NEW LINE CODE", 1, NEW_LINE),
            mi(84, 10704, "ENC/DEC RTTY", "TX AUTO CR+LF", 1, ENABLE),
            mi(85, 10705, "ENC/DEC RTTY", "TX DIDDLE", 1, DIDDLE),
            mi(86, 10706, "ENC/DEC RTTY", "BAUDOT CODE", 1, BAUDOT),
            mi(87, 20101, "MODE CW", "AGC FAST DELAY", 4, AGC_DELAY),
            mi(88, 20102, "MODE CW", "AGC MID DELAY", 4, AGC_DELAY),
            mi(89, 20103, "MODE CW", "AGC SLOW DELAY", 4, AGC_DELAY),
            mi(90, 20104, "MODE CW", "LCUT FREQ", 2, LCUT),
            mi(91, 20105, "MODE CW", "LCUT SLOPE", 1, SLOPE),
            mi(92, 20106, "MODE CW", "HCUT FREQ", 2, HCUT),
            mi(93, 20107, "MODE CW", "HCUT SLOPE", 1, SLOPE),
            mi(94, 20108, "MODE CW", "CW OUT SELECT", 1, OUT_SEL),
            mi(95, 20109, "MODE CW", "CW OUT LEVEL", 3, LEVEL),
            mi(96, 20110, "MODE CW", "CW AUTO MODE", 1, CW_AUTO),
            mi(97, 20111, "MODE CW", "CW BK-IN TYPE", 1, BK_IN_TYPE),
            mi(98, 20112, "MODE CW", "CW BK-IN DELAY", 2, BK_IN_DELAY),
            mi(99, 20113, "MODE CW", "CW WAVE SHAPE", 1, WAVE_SHAPE),
            mi(100, 20114, "MODE CW", "CW FREQ DISPLAY", 1, FREQ_DISPLAY),
            mi(101, 20115, "MODE CW", "PC KEYING", 1, PC_KEYING),
            mi(102, 20116, "MODE CW", "QSK DELAY TIME", 1, QSK_DELAY),
            mi(103, 20117, "MODE CW", "CW INDICATOR", 1, OFF_ON),
            mi(104, 20201, "KEYER", "F KEYER TYPE", 1, KEYER_TYPE),
            mi(105, 20202, "KEYER", "F KEYER DOT/DASH", 1, NOR_REV),
            mi(106, 20203, "KEYER", "R KEYER TYPE", 1, KEYER_TYPE),
            mi(107, 20204, "KEYER", "R KEYER DOT/DASH", 1, NOR_REV),
            mi(108, 20205, "KEYER", "CW WEIGHT", 2, CW_WEIGHT),
            mi(109, 20206, "KEYER", "NUMBER STYLE", 1, NUMBER_STYLE),
            mi(110, 20207, "KEYER", "CONTEST NUMBER", 4, CONTEST_NUMBER),
            mi(111, 20208, "KEYER", "CW MEMORY 1", 1, CW_MEM),
            mi(112, 20209, "KEYER", "CW MEMORY 2", 1, CW_MEM),
            mi(113, 20210, "KEYER", "CW MEMORY 3", 1, CW_MEM),
            mi(114, 20211, "KEYER", "CW MEMORY 4", 1, CW_MEM),
            mi(115, 20212, "KEYER", "CW MEMORY 5", 1, CW_MEM),
            mi(116, 20213, "KEYER", "REPEAT INTERVAL", 2, REPEAT_INTERVAL),
            mi(117, 20301, "DECODE CW", "CW DECODE BW", 1, DECODE_BW),
            mi(118, 30101, "GENERAL", "DECODE RX SELECT", 1, OUT_SEL),
            mi(119, 30102, "GENERAL", "HEADPHONE MIX", 1, HEADPHONE_MIX),
            mi(120, 30103, "GENERAL", "ANT3 SELECT", 1, ANT3_SELECT),
            mi(121, 30104, "GENERAL", "NB WIDTH", 1, NB_WIDTH),
            mi(122, 30105, "GENERAL", "NB REJECTION", 1, NB_REJECTION),
            mi(123, 30106, "GENERAL", "BEEP LEVEL", 3, LEVEL),
            mi(124, 30107, "GENERAL", "RF/SQL VR", 1, RF_SQL),
            mi(125, 30108, "GENERAL", "TUNER SELECT", 1, TUNER_SELECT),
            mi(126, 30109, "GENERAL", "232C RATE", 1, RATE),
            mi(127, 30110, "GENERAL", "232C TIME OUT TIMER", 1, TOT),
            mi(128, 30111, "GENERAL", "CAT RATE", 1, RATE),
            mi(129, 30112, "GENERAL", "CAT TIME OUT TIMER", 1, TOT),
            mi(130, 30113, "GENERAL", "CAT RTS", 1, ENABLE),
            mi(131, 30114, "GENERAL", "QMB CH", 1, QMB_CH),
            mi(132, 30115, "GENERAL", "MEM GROUP", 1, ENABLE),
            mi(133, 30116, "GENERAL", "QUICK SPLIT INPUT", 1, ENABLE),
            mi(134, 30117, "GENERAL", "QUICK SPLIT FREQ", 3, QUICK_SPLIT),
            mi(135, 30118, "GENERAL", "TX TIME OUT TIMER", 2, TX_TOT),
            mi(136, 30119, "GENERAL", "MIC SCAN", 1, ENABLE),
            mi(137, 30120, "GENERAL", "MIC SCAN RESUME", 1, SCAN_RESUME),
            mi(138, 30121, "GENERAL", "REF FREQ ADJ", 3, REF_FREQ_ADJ),
            mi(139, 30122, "GENERAL", "CS DIAL", 2, CS_DIAL),
            mi(140, 30123, "GENERAL", "KEYBOARD LANGUAGE", 2, KEYBOARD),
            mi(141, 30201, "RX DSP", "APF WIDTH", 1, APF_WIDTH),
            mi(142, 30202, "RX DSP", "CONTOUR LEVEL", 3, CONTOUR_LEVEL),
            mi(143, 30203, "RX DSP", "CONTOUR WIDTH", 2, CONTOUR_WIDTH),
            mi(144, 30204, "RX DSP", "DNR LEVEL", 2, DNR_LEVEL),
            mi(145, 30205, "RX DSP", "IF NOTCH WIDTH", 1, NOTCH_WIDTH),
            mi(146, 30301, "TX AUDIO", "PROC TYPE", 1, PROC_TYPE),
            mi(147, 30302, "TX AUDIO", "AMC RELEASE TIME", 1, AMC_RELEASE),
            mi(148, 30303, "TX AUDIO", "PRMTRC EQ1 FREQ", 2, EQ1_FREQ),
            mi(149, 30304, "TX AUDIO", "PRMTRC EQ1 LEVEL", 3, EQ_LEVEL),
            mi(150, 30305, "TX AUDIO", "PRMTRC EQ1 BWTH", 2, EQ_BWTH),
            mi(151, 30306, "TX AUDIO", "PRMTRC EQ2 FREQ", 2, EQ2_FREQ),
            mi(152, 30307, "TX AUDIO", "PRMTRC EQ2 LEVEL", 3, EQ_LEVEL),
            mi(153, 30308, "TX AUDIO", "PRMTRC EQ2 BWTH", 2, EQ_BWTH),
            mi(154, 30309, "TX AUDIO", "PRMTRC EQ3 FREQ", 2, EQ3_FREQ),
            mi(155, 30310, "TX AUDIO", "PRMTRC EQ3 LEVEL", 3, EQ_LEVEL),
            mi(156, 30311, "TX AUDIO", "PRMTRC EQ3 BWTH", 2, EQ_BWTH),
            mi(157, 30312, "TX AUDIO", "P PRMTRC EQ1 FREQ", 2, EQ1_FREQ),
            mi(158, 30313, "TX AUDIO", "P PRMTRC EQ1 LEVEL", 3, EQ_LEVEL),
            mi(159, 30314, "TX AUDIO", "P PRMTRC EQ1 BWTH", 2, EQ_BWTH),
            mi(160, 30315, "TX AUDIO", "P PRMTRC EQ2 FREQ", 2, EQ2_FREQ),
            mi(161, 30316, "TX AUDIO", "P PRMTRC EQ2 LEVEL", 3, EQ_LEVEL),
            mi(162, 30317, "TX AUDIO", "P PRMTRC EQ2 BWTH", 2, EQ_BWTH),
            mi(163, 30318, "TX AUDIO", "P PRMTRC EQ3 FREQ", 2, EQ3_FREQ),
            mi(164, 30319, "TX AUDIO", "P PRMTRC EQ3 LEVEL", 3, EQ_LEVEL),
            mi(165, 30320, "TX AUDIO", "P PRMTRC EQ3 BWTH", 2, EQ_BWTH),
            mi(166, 30401, "TX GENERAL", "HF MAX POWER", 3, $hf),
            mi(167, 30402, "TX GENERAL", "50M MAX POWER", 3, $hf),
            mi(168, 30403, "TX GENERAL", "70M MAX POWER", 3, POWER_50W),
            mi(169, 30404, "TX GENERAL", "AM MAX POWER", 3, $am),
            mi(170, 30405, "TX GENERAL", "VOX SELECT", 1, VOX_SELECT),
            mi(171, 30406, "TX GENERAL", "DATA VOX GAIN", 3, LEVEL),
            mi(172, 30407, "TX GENERAL", "EMERGENCY FREQ TX", 1, ENABLE),
            mi(173, 30501, "TUNING", "SSB/CW DIAL STEP", 1, DIAL_STEP),
            mi(174, 30502, "TUNING", "RTTY/PSK DIAL STEP", 1, DIAL_STEP),
            mi(175, 30503, "TUNING", "CH STEP", 1, CH_STEP),
            mi(176, 30504, "TUNING", "AM CH STEP", 1, AM_CH_STEP),
            mi(177, 30505, "TUNING", "FM CH STEP", 1, FM_CH_STEP),
            mi(178, 30506, "TUNING", "MAIN STEPS PER REV.", 1, MAIN_STEPS),
            mi(179, 30507, "TUNING", "MPVD STEPS PER REV.", 1, MPVD_STEPS),
            mi(180, 40101, "DISPLAY", "MY CALL", 12, Kind::Text { max: 12 }),
            mi(181, 40102, "DISPLAY", "MY CALL TIME", 1, MY_CALL_TIME),
            mi(182, 40103, "DISPLAY", "SCREEN SAVER", 1, SCREEN_SAVER),
            mi(183, 40104, "DISPLAY", "TFT CONTRAST", 2, BRIGHTNESS),
            mi(184, 40105, "DISPLAY", "DIMMER TFT", 2, BRIGHTNESS),
            mi(185, 40106, "DISPLAY", "DIMMER LED", 2, BRIGHTNESS),
            mi(186, 40107, "DISPLAY", "MOUSE POINTER SPEED", 2, BRIGHTNESS),
            mi(187, 40108, "DISPLAY", "FREQ STYLE", 1, FREQ_STYLE),
            mi(188, 40201, "SCOPE", "RBW", 1, RBW),
            mi(189, 40202, "SCOPE", "SCOPE CTR", 1, SCOPE_CTR),
            mi(190, 40203, "SCOPE", "2D DISP SENSITIVITY", 1, SENSITIVITY),
            mi(191, 40204, "SCOPE", "3DSS DISP SENSITIVITY", 1, SENSITIVITY),
            mi(192, 40301, "EXT MONITOR", "EXT DISPLAY", 1, ENABLE),
            mi(193, 40302, "EXT MONITOR", "PIXEL", 1, PIXEL),
        ]
    };
}

const MENU_D: &[MenuItem] = ftdx101_menu!(POWER_100W, POWER_25W);
const MENU_MP: &[MenuItem] = ftdx101_menu!(POWER_200W, POWER_50W);

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// `down` then `up` gives the frame back (the last frame, where one
    /// RigHelm frame is several of the radio's).
    pub fn round_trip(m: &'static RadioModel, spec: &TextSpec, frames: &[&str]) {
        for f in frames {
            let mut cx = Ctx::new(m);
            let native = (spec.down)(&mut cx, f);
            assert!(!native.is_empty(), "{}: {f} has no translation", m.key);
            let back: Vec<String> = native.iter().flat_map(|n| (spec.up)(&mut cx, n)).collect();
            assert_eq!(back.last().map(String::as_str), Some(*f), "{}: {f} -> {native:?} -> {back:?}", m.key);
        }
    }

    pub fn d(m: &'static RadioModel, spec: &TextSpec, f: &str) -> Vec<String> {
        (spec.down)(&mut Ctx::new(m), f)
    }

    /// `up` of a radio answer (the translator has spoken to the radio first).
    pub fn ua(m: &'static RadioModel, spec: &TextSpec, f: &str) -> Vec<String> {
        let mut cx = Ctx::new(m);
        (spec.down)(&mut cx, "ID;");
        (spec.up)(&mut cx, f)
    }

    /// `up` of a program's command.
    pub fn up_prog(m: &'static RadioModel, spec: &TextSpec, f: &str) -> Vec<String> {
        (spec.up)(&mut Ctx::new(m), f)
    }

    fn dd(f: &str) -> Vec<String> {
        d(&FTDX101D, &SPEC, f)
    }

    fn uu(f: &str) -> Vec<String> {
        ua(&FTDX101D, &SPEC, f)
    }

    #[test]
    fn menu_tables() {
        assert_eq!(MENU_D.len(), 193);
        assert_eq!(MENU_MP.len(), 193);
        let cat = FTDX101D.menu_item(128).unwrap();
        assert_eq!((cat.name, cat.addr), ("CAT RATE", 30111));
        assert_eq!(FTDX101D.menu_item(101).unwrap().name, "PC KEYING");
        assert_eq!(FTDX101D.menu_item(100).unwrap().name, "CW FREQ DISPLAY");
        assert_eq!(FTDX101D.menu_item(129).unwrap().name, "CAT TIME OUT TIMER");
        for i in [11, 25, 39, 58] {
            assert!(FTDX101D.menu_item(i).unwrap().name.ends_with("MOD SOURCE"));
        }
        let hf = |m: &RadioModel| match m.menu.iter().find(|i| i.name == "HF MAX POWER").unwrap().kind {
            Kind::Num { max, .. } => max,
            _ => 0,
        };
        assert_eq!((hf(&FTDX101D), hf(&FTDX101MP)), (100, 200));
        assert_eq!(MENU_D.last().unwrap().addr, 40302);
    }

    #[test]
    fn manual_frames_down() {
        assert_eq!(dd("FA14250000;"), ["FA014250000;"]);
        assert_eq!(dd("FB7074000;"), ["FB007074000;"]);
        assert_eq!(dd("FA;"), ["FA;"]);
        assert_eq!(dd("MD0I;"), ["MD0E;"]);
        assert_eq!(dd("MD0J;"), ["MD0F;"]);
        assert_eq!(dd("MD03;"), ["MD03;"]);
        assert!(dd("MD0E;").is_empty(), "C4FM");
        assert_eq!(dd("SH0;"), ["SH0;"]);
        assert_eq!(dd("SH014;"), ["SH0014;"]);
        assert_eq!(dd("IS0;"), ["IS0;"]);
        assert_eq!(dd("IS0+0100;"), ["IS00+0100;"]);
        assert_eq!(dd("IS0-0050;"), ["IS00-0060;"]);
        assert_eq!(dd("CN0;"), ["CN00;"]);
        assert_eq!(dd("CN012;"), ["CN00012;"]);
        assert_eq!(dd("EX001;"), ["EX010101;"]);
        assert_eq!(dd("EX1283;"), ["EX0301113;"]);
        assert_eq!(dd("CO00;"), ["CO00;", "CO02;"]);
        assert_eq!(dd("CO0001;"), ["CO020000;", "CO000001;"]);
        assert_eq!(dd("CO0002;"), ["CO000000;", "CO020001;"]);
        assert_eq!(dd("CO0110;"), ["CO011000;"]);
        assert_eq!(dd("CO02;"), ["CO03;"]);
        assert_eq!(dd("CO0230;"), ["CO030030;"]);
        assert_eq!(dd("BP01400;"), ["BP01320;"]);
        assert_eq!(dd("SD0200;"), ["SD04;"]);
        assert_eq!(dd("SD3000;"), ["SD33;"]);
        assert_eq!(dd("SD0400;"), ["SD07;"]);
        assert_eq!(dd("SQ0255;"), ["SQ0100;"]);
        assert_eq!(dd("DA151515;"), ["DA00002020;"]);
        assert_eq!(dd("RM5;"), ["RM5;"]);
        assert_eq!(dd("RM5161;"), ["RM5161000;"]);
        assert!(dd("RM2;").is_empty());
        assert_eq!(dd("MS3;"), ["MS00;"]);
        assert_eq!(dd("MS1;"), ["MS10;"]);
        assert_eq!(dd("FR0;"), ["FR01;"]);
        assert_eq!(dd("FR4;"), ["FR10;"]);
        assert_eq!(dd("FT3;"), ["FT3;"]);
        assert_eq!(dd("VF0;"), ["VT0;"]);
        assert_eq!(dd("VF001281;"), ["VT001281;"]);
        assert_eq!(dd("ED405;"), ["ED205;"]);
        assert!(dd("ED205;").is_empty());
        assert_eq!(dd("MC005;"), ["MC005;"]);
        assert_eq!(dd("MC100;"), ["MCP1L;"]);
        assert_eq!(dd("MC117;"), ["MCP9U;"]);
        assert_eq!(dd("MR101;"), ["MRP1U;"]);
        assert_eq!(dd("IF00114074000+012010200000;"), ["IF001014074000+012010200000;"]);
        assert_eq!(dd("OI00050313000-005000I00000;"), ["OI000050313000-005000E00000;"]);
        assert_eq!(dd("MW10314074000+000000I00000;"), ["MWP2U014074000+000000E00000;"]);
        assert_eq!(dd("PC100;"), ["PC100;"]);
        assert_eq!(dd("AN0;"), ["AN0;"]);
        assert_eq!(dd("RI0;"), ["RI0;"]);
        assert!(dd("RI5;").is_empty());
        assert!(dd("TS1;").is_empty(), "no TXW");
        assert!(dd("SF1;").is_empty(), "different sub-dial functions");
    }

    #[test]
    fn manual_frames_up() {
        assert_eq!(uu("FA014250000;"), ["FA14250000;"]);
        assert_eq!(uu("FB070100000;"), ["FB70100000;"]);
        assert_eq!(uu("MD0E;"), ["MD0I;"]);
        assert_eq!(uu("MD0F;"), ["MD0J;"]);
        assert!(uu("MD13;").is_empty(), "SUB");
        assert!(uu("AG1128;").is_empty(), "SUB");
        assert_eq!(uu("AG0128;"), ["AG0128;"]);
        assert_eq!(uu("SH0014;"), ["SH014;"]);
        assert_eq!(uu("IS00+0100;"), ["IS0+0100;"]);
        assert_eq!(uu("CN00012;"), ["CN012;"]);
        assert_eq!(uu("EX0101010500;"), ["EX0010500;"]);
        assert_eq!(uu("EX0401MYCALL      ;").len(), 0, "a 4-digit address is nothing");
        assert_eq!(uu("EX040101W1AW        ;"), ["EX180W1AW        ;"]);
        assert_eq!(uu("CO000001;"), ["CO0001;"]);
        assert_eq!(uu("CO020001;"), ["CO0002;"]);
        assert_eq!(uu("CO011000;"), ["CO0110;"]);
        assert_eq!(uu("CO010010;"), ["CO0101;"]);
        assert_eq!(uu("CO030025;"), ["CO0225;"]);
        assert_eq!(uu("SD04;"), ["SD0200;"]);
        assert_eq!(uu("SD33;"), ["SD3000;"]);
        assert_eq!(uu("SQ0100;"), ["SQ0255;"]);
        assert_eq!(uu("DA00002020;"), ["DA151515;"]);
        assert_eq!(uu("RM5161000;"), ["RM5161;"]);
        assert!(uu("RM2100000;").is_empty(), "SUB S-meter");
        assert!(uu("RM9050000;").is_empty(), "TEMP");
        assert_eq!(uu("MS13;"), ["MS6;"]);
        assert_eq!(uu("FR00;"), ["FR0;"]);
        assert_eq!(uu("FR01;"), ["FR0;"]);
        assert_eq!(uu("FR10;"), ["FR4;"]);
        assert_eq!(uu("BY11;"), ["BY10;"]);
        assert_eq!(uu("VT001281;"), ["VF001281;"]);
        assert_eq!(uu("RF07;"), ["RF07;"]);
        assert!(uu("RF17;").is_empty());
        assert_eq!(uu("MCP1L;"), ["MC100;"]);
        assert_eq!(uu("MC099;"), ["MC099;"]);
        assert_eq!(uu("MCEMG;"), ["MC118;"]);
        assert_eq!(uu("IF001014074000+012010200000;"), ["IF00114074000+012010200000;"]);
        assert_eq!(uu("IFP3L014074000+000000E5000;").len(), 0, "too short");
        assert_eq!(uu("IFP3L014074000+000000E50000;"), ["IF10414074000+000000I50000;"]);
        assert_eq!(uu("MRP9U050313000-010010F10000;"), ["MR11750313000-010010J10000;"]);
        // Split, as the radio reports it and as a program sets it.
        assert_eq!(uu("ST1;"), ["FT1;"]);
        assert_eq!(uu("ST0;"), ["FT0;"]);
        assert_eq!(up_prog(&FTDX101D, &SPEC, "ST1;"), ["FT3;"]);
        assert_eq!(up_prog(&FTDX101D, &SPEC, "ST0;"), ["FT2;"]);
        assert_eq!(up_prog(&FTDX101D, &SPEC, "ST;"), ["FT;"]);
        // Commands RigHelm doesn't have.
        assert!(uu("SS04000000;").is_empty());
        assert!(uu("SY1;").is_empty());
    }

    #[test]
    fn st_read_is_answered_as_st() {
        let mut cx = Ctx::new(&FTDX101D);
        assert_eq!(up(&mut cx, "ST;"), ["FT;"]);
        assert_eq!(down(&mut cx, "FT1;"), ["ST1;"]);
        assert_eq!(down(&mut cx, "FT1;"), ["FT1;"]);
    }

    #[test]
    fn contour_and_apf_follow_each_other() {
        let mut cx = Ctx::new(&FTDX101D);
        down(&mut cx, "ID;");
        assert_eq!(up(&mut cx, "CO020001;"), ["CO0002;"]);
        assert_eq!(up(&mut cx, "CO000000;"), ["CO0002;"]);
        assert_eq!(up(&mut cx, "CO020000;"), ["CO0000;"]);
        assert_eq!(up(&mut cx, "CO000001;"), ["CO0001;"]);
    }

    #[test]
    fn meter_switch_keeps_the_other_side() {
        let mut cx = Ctx::new(&FTDX101D);
        assert_eq!(down(&mut cx, "MS1;"), ["MS10;"]);
        assert_eq!(down(&mut cx, "MS6;"), ["MS13;"]);
        assert_eq!(up(&mut cx, "MS13;"), ["MS6;"]);
        assert_eq!(down(&mut cx, "MS0;"), ["MS03;"]);
        assert_eq!(up(&mut cx, "MS03;"), ["MS0;"]);
    }

    #[test]
    fn dual_watch_survives_selecting_main() {
        let mut cx = Ctx::new(&FTDX101D);
        down(&mut cx, "FR;");
        assert_eq!(up(&mut cx, "FR00;"), ["FR0;"]);
        assert_eq!(down(&mut cx, "FR0;"), ["FR00;"]);
    }

    #[test]
    fn round_trips() {
        let frames = [
            "FA14074000;", "FB144174000;", "FA;", "MD0;", "MD01;", "MD0C;", "MD0I;", "MD0J;", "IF;", "OI;",
            "IF00114074000+012010200000;", "OI10050313000-001000I10000;", "MR001;", "MR117;",
            "MR00114074000+000000C10000;", "SH0;", "SH005;", "SH021;", "IS0;", "IS0+0100;", "IS0-1000;", "CN0;",
            "CN049;", "CO00;", "CO0000;", "CO0001;", "CO0002;", "CO01;", "CO0101;", "CO0132;", "CO02;", "CO0200;",
            "CO0250;", "BP00;", "BP00001;", "BP01320;", "SD;", "SD0030;", "SD0200;", "SD0300;", "SD3000;", "SQ0;",
            "SQ0000;", "SQ0255;", "SQ0051;", "DA;", "DA000000;", "DA151515;", "DA070707;", "RM1;", "RM5;", "RM1130;",
            "RM6026;", "MS;", "MS0;", "MS2;", "MS3;", "MS6;", "FR;", "FR0;", "FR1;", "FR4;", "FT;", "FT2;", "FT3;",
            "FT0;", "FT1;", "VF0;", "VF001281;", "ED001;", "EU705;", "MC;", "MC001;", "MC100;", "MC117;", "EX001;",
            "EX0012000;", "EX1283;", "EX180W1AW        ;", "AG0;", "AG0128;", "RG0255;", "GT0;", "GT04;", "PA0;",
            "PA02;", "RA03;", "RF0;", "RF01;", "RF0A;", "NB01;", "NL0010;", "NR01;", "RL015;", "BC01;", "NA01;",
            "AN0;", "AN030;", "AC;", "AC002;", "BY10;", "RI0;", "RI31;", "PB0;", "PB01;", "LM01;", "LK7;", "FS5;",
            "PC200;", "KP40;", "KS060;", "KM1CQ TEST};", "PR01;", "ML1050;", "ID;", "AI1;", "TX1;", "MX0;", "VS1;",
            "RT1;", "XT0;", "RU0100;", "RD9990;", "RC;", "BS17;", "BU0;", "BD0;", "SM0;", "SM0255;", "OS02;",
            "CT01;", "UL;", "RS;", "VD0300;", "EM11 ;", "EN11;",
        ];
        round_trip(&FTDX101D, &SPEC, &frames);
    }

    #[test]
    fn every_menu_item_maps_both_ways() {
        for m in [&FTDX101D, &FTDX101MP] {
            for item in m.menu {
                let read = format!("EX{:03};", item.num);
                let native = d(m, &SPEC, &read);
                assert_eq!(native, [format!("EX{:06};", item.addr)]);
                assert_eq!(ua(m, &SPEC, &format!("EX{:06}0;", item.addr)), [format!("EX{:03}0;", item.num)]);
            }
        }
    }
}
