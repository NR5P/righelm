//! Elecraft K4 (K4, K4D, K4HD): "a superset of the commands from the
//! K3/K3S" (K4 Programmer's Reference, rev D11/D12).
//!
//! RigHelm keeps the K4 in its basic, K3-compatible mode (`K40;`, as
//! Hamlib does; `K41;` would change the `ID;` answer to the user's call
//! sign, which other programs and RigHelm's own detection rely on) with the
//! K3 extended mode on (`K31;`). So frequencies, modes, `IF`, split, RIT,
//! the S-meter and AGC work as on the K3 ([`super::k3`] does those), and
//! this file has what is the K4's own:
//!
//! * levels with the K4's ranges: AF gain 0-60, RF gain `RG-nn` (-0 to
//!   -60 dB), squelch 0-40, mic 0-80, compression 0-30, CW pitch 250-950 Hz;
//! * power `PCnnnL` (tenths of a watt, to 10 W) / `PCnnnH` (watts), read with
//!   `PCX;` which always answers that way;
//! * preamp `PAnm`, attenuator `RAnnm` (3 dB steps to 21 dB), noise blanker
//!   `NBnnmf` (level 0-15), LMS noise reduction `NRnnm` (level 0-10), auto
//!   notch `NA`, manual notch `NMnnnnm`, ATU mode `AT` and `TU` (tune);
//! * `AB3;` / `AB4;` / `AB5;` (A>B, B>A, swap), `BN+;` / `BN-;` (band up/down);
//! * meters: power `PO` (tenths of a watt); ALC, compression and SWR come
//!   only with the `TM` reports the K4 sends while transmitting once `TM1;`
//!   is set (RigHelm sets it with the extended mode, and `TM0;` with `AI0;`
//!   when it lets go of the radio);
//! * menu: `MEiiii.nnnn` reads and sets an entry directly.

use super::k3::{self, int, level_down, level_up, memo_u32, scale};
use crate::dialect::text::{Ctx, REPLY, TextSpec, parts};
use crate::menu::{Kind, MenuItem};
use crate::model::{Caps, Family, Maker, Memory, Proto, RadioModel, Roles};
use crate::types::{PO_CAL, SWR_CAL, interpolate};

pub static SPEC: TextSpec = TextSpec { down, up, answer_prefix };

/// `PCX;` is answered `PCnnnr;`.
fn answer_prefix(n: &str) -> String {
    let p = k3::answer_prefix(n);
    if p == "PCX" { "PC".into() } else { p }
}

// ------------------------------------------------------------- menu

const RAW: Kind = crate::menu::num(0, 9999, 1, "");

const fn me(num: u16, id: u32, group: &'static str, name: &'static str) -> MenuItem {
    MenuItem { num, addr: id, group, name, width: 4, kind: RAW }
}

const AGC: &str = "AGC";
const DISP: &str = "DISPLAY";
const TX: &str = "TX";
const RX: &str = "RX";
const SER: &str = "SERIAL";
const GEN: &str = "GENERAL";
const XV: &str = "XVTR";

/// The K4's menu entries (ME IDs, in the manual's order). The values are
/// the radio's own numbers (MEDF describes them; the manual doesn't). Left
/// out: IP Address, Radio Serial Number and Radio Type (read-only facts,
/// not numbers to set) and Reference Freq (a calibration).
pub static K4_MENU: &[MenuItem] = &[
    me(1, 39, AGC, "AF Limiter (AGC off)"),
    me(2, 11, AGC, "AGC Attack"),
    me(3, 9, AGC, "AGC Decay, Fast"),
    me(4, 8, AGC, "AGC Decay, Slow"),
    me(5, 7, AGC, "AGC Hold Time"),
    me(6, 13, AGC, "AGC Noise Pulse Reject"),
    me(7, 12, AGC, "AGC Slope"),
    me(8, 10, AGC, "AGC Threshold"),
    me(9, 48, DISP, "AutoRef Averaging"),
    me(10, 49, DISP, "AutoRef Debounce"),
    me(11, 50, DISP, "AutoRef Offset"),
    me(12, 104, TX, "CW TX in SSB Mode"),
    me(13, 100, GEN, "DIGOUT1 (ACC jack, pin 11)"),
    me(14, 70, DISP, "Ext. Monitor Function"),
    me(15, 71, DISP, "Ext. Monitor Location"),
    me(16, 3, GEN, "Fan Speed Min"),
    me(17, 89, TX, "FM Deviation, Tone"),
    me(18, 88, TX, "FM Deviation, Voice"),
    me(19, 64, RX, "FSK Dual-Tone RX Filter"),
    me(20, 85, TX, "FSK Mark-Tone"),
    me(21, 84, TX, "FSK Polarity"),
    me(22, 4, GEN, "KAT4 ATU Option"),
    me(23, 6, GEN, "KPA4 PA Option"),
    me(24, 5, GEN, "KRX4 2ND RX Option"),
    me(25, 36, DISP, "LCD Brightness"),
    me(26, 37, DISP, "LED Brightness"),
    me(27, 87, TX, "Message Repeat Interval"),
    me(28, 74, DISP, "Mouse L/R Button QSY"),
    me(29, 109, DISP, "Mouse Pointer Size, Ext. Mon."),
    me(30, 107, DISP, "Mouse Pointer Size, LCD"),
    me(31, 46, TX, "Per-Band Power"),
    me(32, 93, RX, "Preamp 3 (12/10/6 m)"),
    me(33, 42, GEN, "RIT CLR 2nd Tap Restore"),
    me(34, 44, GEN, "RIT Knob Alt. Function"),
    me(35, 91, RX, "RX 1.5 MHz High-Pass Fil."),
    me(36, 106, RX, "RX All-Mode Squelch"),
    me(37, 105, RX, "RX Audio Mix with Sub On"),
    me(38, 113, RX, "RX Audio Gain Boost"),
    me(39, 73, RX, "RX Auto Attenuation"),
    me(40, 102, RX, "RX CW IIR Filters (50-200 Hz)"),
    me(41, 97, RX, "RX Dyn. Range Optimization"),
    me(42, 80, DISP, "Screen Cap File"),
    me(43, 65, SER, "Serial RS232: Auto Info"),
    me(44, 57, SER, "Serial RS232: Baud Rate"),
    me(45, 54, SER, "Serial RS232: DTR"),
    me(46, 55, SER, "Serial RS232: RTS"),
    me(47, 66, SER, "Serial USB-PC1: Auto Info"),
    me(48, 60, SER, "Serial USB-PC1: Baud Rate"),
    me(49, 58, SER, "Serial USB-PC1: DTR"),
    me(50, 59, SER, "Serial USB-PC1: RTS"),
    me(51, 67, SER, "Serial USB-PC2: Auto Info"),
    me(52, 63, SER, "Serial USB-PC2: Baud Rate"),
    me(53, 61, SER, "Serial USB-PC2: DTR"),
    me(54, 62, SER, "Serial USB-PC2: RTS"),
    me(55, 1, GEN, "Speaker, Internal"),
    me(56, 72, GEN, "Speakers + Phones"),
    me(57, 83, GEN, "Speakers, External"),
    me(58, 92, DISP, "Spectrum Amplitude Units"),
    me(59, 90, DISP, "Spectrum Freq. Marks"),
    me(60, 30, DISP, "Spectrum Trace Fill"),
    me(61, 69, TX, "TUNE LP (Low power TUNE)"),
    me(62, 14, TX, "TX 2-Tone Generator"),
    me(63, 2, TX, "TX ALC"),
    me(64, 108, TX, "TX Audio LF Cutoff, SSB"),
    me(65, 52, TX, "TX DLY, Key Out to RF Out"),
    me(66, 110, TX, "TX DLY, Unkey to Receive"),
    me(67, 28, TX, "TX Gain Cal"),
    me(68, 15, TX, "TX Gain Cal via TUNE"),
    me(69, 53, TX, "TX Inhibit Mode"),
    me(70, 101, TX, "TX Monitor Level, Line Out"),
    me(71, 114, TX, "TX Monitor Level, Remote"),
    me(72, 112, TX, "TX Monitor Method, Voice"),
    me(73, 103, TX, "TX Noise Gate Threshold"),
    me(74, 111, TX, "TX QSK Method"),
    me(75, 41, GEN, "VFO B Different Band"),
    me(76, 45, GEN, "VFO Coarse Tuning"),
    me(77, 38, GEN, "VFO Counts per Turn"),
    me(78, 27, TX, "Wattmeter Cal"),
    me(79, 86, XV, "XVTR Band # Select"),
    me(80, 78, XV, "XVTR Band <n> I.F."),
    me(81, 76, XV, "XVTR Band <n> Mode"),
    me(82, 79, XV, "XVTR Band <n> Offset"),
    me(83, 98, XV, "XVTR Band <n> Power Out"),
    me(84, 77, XV, "XVTR Band <n> R.F."),
    me(85, 75, XV, "XVTR OUT Test"),
];

/// RigHelm `EXnnn<raw>` as `MEiiii.<raw>` (a read: `MEiiii;`).
fn menu_down(cx: &Ctx, p: &str) -> Vec<String> {
    let Some(item) = p.get(..3).and_then(|n| n.parse::<u16>().ok()).and_then(|n| cx.model.menu_item(n)) else { return Vec::new() };
    let raw = &p[3..];
    if raw.is_empty() { vec![format!("ME{:04};", item.addr)] } else { vec![format!("ME{:04}.{raw};", item.addr)] }
}

fn menu_up(cx: &Ctx, p: &str) -> Vec<String> {
    let (id, raw) = p.split_once('.').unwrap_or((p, ""));
    let Some(item) = int(id).and_then(|id| cx.model.menu.iter().find(|m| m.addr == id)) else { return Vec::new() };
    vec![format!("EX{:03}{raw};", item.num)]
}

// ------------------------------------------------------------- values

/// RigHelm watts as the K4's power: tenths of a watt up to 10 W ("L"),
/// watts above ("H").
fn pc_k4(w: u32) -> String {
    if w <= 10 { format!("PC{:03}L;", w * 10) } else { format!("PC{:03}H;", w.min(110)) }
}

/// Watts from `nnnr` (r: L tenths, H watts, X milliwatts; without r, L),
/// or the K22 form `nnnx` (x: 0 tenths, 1 watts).
fn pc_watts(p: &str) -> Option<u32> {
    let n = int(p.get(..3)?)?;
    Some(match p.get(3..4) {
        Some("H" | "1") => n,
        Some("X") => 0,
        _ => (n as f32 / 10.0).round() as u32,
    })
}

/// A read whose answer has the K4's own layout only in K41 mode: asked in
/// K41 mode, then back to K40 (which also resets the K3 mode).
fn k41_read(cmd: &str) -> String {
    format!("K41;{cmd};K40;K31;")
}

/// The program put the radio in K41 mode (K4-style answers).
fn k41(cx: &Ctx) -> bool {
    cx.recall("k4") == Some("1")
}

/// The RM answers the K4 can give, from its power reading and the last
/// `TM` report (ALC bars, compression dB, SWR).
fn tx_meters(cx: &Ctx) -> Vec<String> {
    let po = memo_u32(cx, "po_raw").unwrap_or(0);
    let cmp = memo_u32(cx, "tm_cmp").unwrap_or(0);
    let alc = memo_u32(cx, "tm_alc").unwrap_or(0);
    let swr = memo_u32(cx, "tm_swr").unwrap_or(0);
    vec![format!("RM3{cmp:03};"), format!("RM4{alc:03};"), format!("RM5{po:03};"), format!("RM6{swr:03};")]
}

fn on(b: bool) -> u8 {
    b as u8
}

// ------------------------------------------------------------- down

fn down(cx: &mut Ctx, f: &str) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if !cx.model.has(&code) {
        return Vec::new();
    }
    let p = p.to_string();
    let own = if cx.program { to_program(cx, &code, &p) } else { to_radio(cx, f, &code, &p) };
    own.unwrap_or_else(|| k3::down(cx, f))
}

/// The K4's own forms of RigHelm Sets and Reads (`None`: as the K3).
fn to_radio(cx: &mut Ctx, f: &str, code: &str, p: &str) -> Option<Vec<String>> {
    let read = crate::dialect::text::is_read(f);
    let one = |s: String| Some(vec![s]);
    let n = int(p.get(1..).unwrap_or("")).unwrap_or(0);
    match code {
        "ID" if read => one("K40;K31;TM1;ID;".into()),
        "AI" if !read => Some(vec![
            "K40;".into(),
            "K31;".into(),
            format!("AI{};", if p == "0" { 0 } else { 2 }),
            format!("TM{};", if p == "0" { 0 } else { 1 }),
        ]),
        "AG" if !read => one(format!("AG{:03};", level_down(cx, "ag", n, 255, 0, 60))),
        "RG" if !read => one(format!("RG-{:02};", 60 - level_down(cx, "rg", n, 255, 0, 60))),
        "SQ" if !read => one(format!("SQ{:03};", level_down(cx, "sq", n, 255, 0, 40))),
        "MG" if !read => one(format!("MG{:03};", level_down(cx, "mg", int(p).unwrap_or(0), 100, 0, 80))),
        "PL" if !read => one(format!("CP{:03};", level_down(cx, "cp", int(p).unwrap_or(0), 100, 0, 30))),
        "KP" if !read => one(format!("CW{:02};", (int(p).unwrap_or(0) + 30).min(95))),
        "PC" if read => one("PCX;".into()),
        "PC" => {
            cx.remember("pc", p);
            one(pc_k4(int(p).unwrap_or(0)))
        }
        "AB" => one("AB3;".into()),
        "BA" => one("AB4;".into()),
        "SV" => one("AB5;".into()),
        "BU" => one("BN+;".into()),
        "BD" => one("BN-;".into()),
        "AC" if read => Some(vec!["AT;".into(), "TU;".into()]),
        "AC" => match p {
            "000" => one("AT1;".into()),
            "001" => one("AT2;".into()),
            "002" => one("TU3;".into()),
            _ => Some(Vec::new()),
        },
        "BC" if read => one("NA;".into()),
        "BC" => one(format!("NA{};", n.min(1))),
        "BP" if read => one("NM;".into()),
        // BP00 nnn: on/off; BP01 nnn: frequency in 10 Hz.
        "BP" if p.starts_with("00") => {
            let v = int(&p[2..]).unwrap_or(0).min(1);
            cx.remember("nm_on", v.to_string());
            one(format!("NM{v};"))
        }
        "BP" => {
            let hz = (int(p.get(2..).unwrap_or("")).unwrap_or(0) * 10).clamp(150, 5000);
            one(format!("NM{hz:04}{};", memo_u32(cx, "nm_on").unwrap_or(0)))
        }
        "NR" if read => one("NR;".into()),
        "RL" if read => one("NR;".into()),
        "NR" => {
            cx.remember("nr_on", n.min(1).to_string());
            one(format!("NR{:02}{};", memo_u32(cx, "nr_lvl").unwrap_or(5), n.min(1)))
        }
        "RL" => {
            cx.remember("nr_lvl", n.min(10).to_string());
            one(format!("NR{:02}{};", n.min(10), memo_u32(cx, "nr_on").unwrap_or(1)))
        }
        "NB" | "NL" if read => one(k41_read("NB")),
        "PA" if read => one(k41_read("PA")),
        "RA" if read => one(k41_read("RA")),
        "NB" => {
            cx.remember("nb_on", n.min(1).to_string());
            one(format!("NB{};", n.min(1)))
        }
        "NL" => {
            cx.remember("nb_lvl", n.min(15).to_string());
            one(format!("NB{:02}{}{};", n.min(15), memo_u32(cx, "nb_on").unwrap_or(1), cx.recall("nb_f").unwrap_or("0")))
        }
        "PA" if !read => match n {
            0 => one(format!("PA{}0;", memo_u32(cx, "pa_lvl").unwrap_or(1))),
            l => {
                cx.remember("pa_lvl", l.min(3).to_string());
                one(format!("PA{}1;", l.min(3)))
            }
        },
        "RA" if !read => match n {
            0 => one(format!("RA{:02}0;", memo_u32(cx, "ra_db").unwrap_or(3))),
            c => {
                cx.remember("ra_db", (c.min(7) * 3).to_string());
                one(format!("RA{:02}1;", c.min(7) * 3))
            }
        },
        "RM" => match p {
            "1" => None,
            "3" | "4" | "5" | "6" => one("PO;".into()),
            _ => Some(Vec::new()),
        },
        "EX" => Some(menu_down(cx, p)),
        _ => None,
    }
}

/// RigHelm Answers as the K4 would send them to a program (`None`: as the
/// K3).
fn to_program(cx: &mut Ctx, code: &str, p: &str) -> Option<Vec<String>> {
    let asked = cx.recall("asked").unwrap_or("").to_string();
    let one = |s: String| Some(vec![s]);
    let n = int(p.get(1..).unwrap_or("")).unwrap_or(0);
    match code {
        "AG" => one(format!("AG{:03};", scale(n, 0, 255, 0, 60))),
        "RG" => one(format!("RG-{:02};", 60 - scale(n, 0, 255, 0, 60))),
        "SQ" => one(format!("SQ{:03};", scale(n, 0, 255, 0, 40))),
        "MG" => one(format!("MG{:03};", scale(int(p).unwrap_or(0), 0, 100, 0, 80))),
        "PL" => one(format!("CP{:03};", scale(int(p).unwrap_or(0), 0, 100, 0, 30))),
        "KP" => one(format!("CW{:02};", (int(p).unwrap_or(0) + 30).min(95))),
        // A K3-style `PC;` gets the legacy answer (as the K3's), `PCX;`
        // the K4's.
        "PC" if asked == "PC" && !k41(cx) => None,
        "PC" => one(pc_k4(int(p).unwrap_or(0))),
        "AC" => {
            let v = int(p.get(2..3).unwrap_or("0")).unwrap_or(0);
            if asked == "TU" { one(format!("TU{};", if v == 2 { 3 } else { 0 })) } else { one(format!("AT{};", if v == 0 { 1 } else { 2 })) }
        }
        "BC" => one(format!("NA{};", n.min(1))),
        "BP" if p.starts_with("00") => {
            cx.remember("nm_on", int(&p[2..]).unwrap_or(0).min(1).to_string());
            Some(Vec::new())
        }
        "BP" => one(format!("NM{:04}{};", (int(p.get(2..).unwrap_or("")).unwrap_or(0) * 10).clamp(150, 5000), memo_u32(cx, "nm_on").unwrap_or(0))),
        "NR" => {
            cx.remember("nr_on", n.min(1).to_string());
            Some(Vec::new())
        }
        "RL" => one(format!("NR{:02}{};", n.min(10), memo_u32(cx, "nr_on").unwrap_or(0))),
        // K41 layouts, or the legacy (K3) ones.
        "NB" if k41(cx) => {
            cx.remember("nb_on", n.min(1).to_string());
            Some(Vec::new())
        }
        "NB" => one(format!("NB{};", n.min(1))),
        "NL" if k41(cx) => one(format!("NB{:02}{}0;", n.min(15), memo_u32(cx, "nb_on").unwrap_or(0))),
        "NL" => Some(Vec::new()),
        "PA" if k41(cx) => one(format!("PA{}{};", n.min(3), on(n > 0))),
        "PA" => one(format!("PA{};", n.min(3))),
        "RA" if k41(cx) => one(format!("RA{:02}{};", n.min(7) * 3, on(n > 0))),
        "RA" => one(format!("RA0{};", on(n > 0))),
        "RM" if asked == "TM" => {
            cx.remember(&format!("tm{}", &p[..1]), n.to_string());
            if !p.starts_with('6') {
                return Some(Vec::new());
            }
            let get = |k: &str| memo_u32(cx, k).unwrap_or(0).min(255) as u8;
            let w = interpolate(&PO_CAL, get("tm5"));
            let fwd = if w > 10.0 { w.round() } else { (w * 10.0).round() } as u32;
            let swr = (interpolate(&SWR_CAL, get("tm6")) * 10.0).round() as u32;
            one(format!("TM{:03}{:03}{:03}{:03};", scale(get("tm4") as u32, 0, 255, 0, 10), get("tm3") / 10, fwd.min(999), swr.clamp(10, 999)))
        }
        "RM" if p.starts_with('5') => {

            let w = interpolate(&PO_CAL, n.min(255) as u8);
            one(format!("PO{:04};", ((w * 10.0).round() as u32).min(9999)))
        }
        "RM" if p.starts_with('6') && asked == "SW" => {
            let swr = interpolate(&SWR_CAL, n.min(255) as u8);
            one(format!("SW{:03};", ((swr * 10.0).round() as u32).clamp(10, 999)))
        }
        "RM" if !p.starts_with('1') => Some(Vec::new()),
        "EX" => {
            let item = p.get(..3).and_then(|n| n.parse::<u16>().ok()).and_then(|n| cx.model.menu_item(n))?;
            one(format!("ME{:04}.{};", item.addr, &p[3..]))
        }
        _ => None,
    }
}

// ------------------------------------------------------------- up

fn up(cx: &mut Ctx, f: &str) -> Vec<String> {
    // The K4 meta-command isn't two letters.
    if let Some(lvl) = f.strip_prefix("K4") {
        let lvl = lvl.trim_end_matches(';');
        if !cx.program {
            return Vec::new();
        }
        if lvl.is_empty() {
            return vec![format!("{REPLY}K4{};", cx.recall("k4").unwrap_or("0"))];
        }
        // K4n turns K2 mode off.
        cx.remember("k4", lvl);
        cx.remember("k2", "0");
        return vec![REPLY.to_string()];
    }
    let Some((code, p)) = parts(f) else { return k3::up(cx, f) };
    let p = p.to_string();
    let own = if cx.program { from_program(cx, &code, &p) } else { from_radio(cx, &code, &p) };
    own.unwrap_or_else(|| k3::up(cx, f))
}

/// The K4's own answers (`None`: as the K3's).
fn from_radio(cx: &mut Ctx, code: &str, p: &str) -> Option<Vec<String>> {
    let one = |s: String| Some(vec![s]);
    let n = int(p).unwrap_or(0);
    if p.starts_with('$') {
        return match code {
            "MD" | "LK" | "SM" => None,
            _ => Some(Vec::new()),
        };
    }
    match code {
        "AG" => one(format!("AG0{:03};", level_up(cx, "ag", n, 255, 0, 60))),
        "RG" => {
            let att = int(p.trim_start_matches('-')).unwrap_or(0).min(60);
            one(format!("RG0{:03};", level_up(cx, "rg", 60 - att, 255, 0, 60)))
        }
        "SQ" => one(format!("SQ0{:03};", level_up(cx, "sq", n, 255, 0, 40))),
        "MG" => one(format!("MG{:03};", level_up(cx, "mg", n, 100, 0, 80))),
        "CP" => one(format!("PL{:03};", level_up(cx, "cp", n, 100, 0, 30))),
        "CW" => one(format!("KP{:02};", (n.max(30) - 30).min(75))),
        "PC" => {
            let w = pc_watts(p)?;
            cx.remember("pc", w.to_string());
            cx.remember("pc_range", p.get(3..4).unwrap_or("L"));
            one(format!("PC{w:03};"))
        }
        "AT" | "TU" => {
            cx.remember(if code == "AT" { "at" } else { "tu" }, p);
            let tuning = matches!(cx.recall("tu"), Some("3" | "4"));
            let atu = cx.recall("at") == Some("2");
            one(format!("AC00{};", if tuning { 2 } else { on(atu) }))
        }
        "NA" => one(format!("BC0{};", n.min(1))),
        "NM" if p.len() >= 5 => {
            let hz = int(&p[..4]).unwrap_or(0);
            let m = int(&p[4..5]).unwrap_or(0);
            cx.remember("nm_on", m.to_string());
            Some(vec![format!("BP00{m:03};"), format!("BP01{:03};", (hz / 10).min(400))])
        }
        "NM" => one(format!("BP00{:03};", n.min(1))),
        // SSNR (NRS) is the other noise reduction; RigHelm has one.
        "NR" if p.starts_with('S') => Some(Vec::new()),
        "NR" if p.len() >= 3 => {
            let lvl = int(&p[..2]).unwrap_or(0);
            let m = (int(&p[2..3]) == Some(1)) as u8;
            cx.remember("nr_lvl", lvl.to_string());
            cx.remember("nr_on", m.to_string());
            Some(vec![format!("NR0{m};"), format!("RL0{lvl:02};")])
        }
        "NB" if p.len() >= 4 => {
            let lvl = int(&p[..2]).unwrap_or(0);
            let m = int(&p[2..3]).unwrap_or(0).min(1);
            cx.remember("nb_lvl", lvl.to_string());
            cx.remember("nb_on", m.to_string());
            cx.remember("nb_f", &p[3..4]);
            Some(vec![format!("NB0{m};"), format!("NL0{lvl:03};")])
        }
        "PA" if p.len() >= 2 => {
            let (l, m) = (int(&p[..1]).unwrap_or(0), int(&p[1..2]).unwrap_or(0));
            if l > 0 {
                cx.remember("pa_lvl", l.to_string());
            }
            one(format!("PA0{};", if m == 1 { l } else { 0 }))
        }
        // The legacy answer (K40 mode): the setting number.
        "PA" => one(format!("PA0{};", n.min(3))),
        "RA" if p.len() >= 3 => {
            let (db, m) = (int(&p[..2]).unwrap_or(0), int(&p[2..3]).unwrap_or(0));
            if db > 0 {
                cx.remember("ra_db", db.to_string());
            }
            one(format!("RA0{};", if m == 1 { (db / 3).min(7) } else { 0 }))
        }
        // Legacy: 00 off, 01 on (at the last level), else dB.
        "RA" => one(format!(
            "RA0{};",
            match n {
                0 => 0,
                1 => memo_u32(cx, "ra_db").map_or(1, |db| (db / 3).clamp(1, 7)),
                db => (db / 3).clamp(1, 7),
            }
        )),
        "NB" if p.len() == 1 => one(format!("NB0{};", n.min(1))),

        // Always tenths of a watt.
        "PO" => {
            cx.remember("po_raw", k3::meter_raw(n as f32 / 10.0, false, &PO_CAL).to_string());
            Some(tx_meters(cx))
        }
        // TX meter report: ALC bars, compression dB, forward power (watts
        // in QRO, tenths in QRP), SWR in tenths.
        "TM" if p.len() >= 12 => {
            let f = |i: usize| int(&p[i..i + 3]).unwrap_or(0);
            let w = if cx.recall("pc_range") == Some("H") { f(6) as f32 } else { f(6) as f32 / 10.0 };
            cx.remember("tm_alc", scale(f(0).min(10), 0, 10, 0, 255).to_string());
            cx.remember("tm_cmp", (f(3) * 10).min(255).to_string());
            cx.remember("po_raw", k3::meter_raw(w, false, &PO_CAL).to_string());
            cx.remember("tm_swr", k3::meter_raw(f(9) as f32 / 10.0, true, &SWR_CAL).to_string());
            Some(tx_meters(cx))
        }
        "ME" => Some(menu_up(cx, p)),
        "BN" if p.starts_with('+') || p.starts_with('-') => Some(Vec::new()),
        _ => None,
    }
}

/// A program's commands in the K4's own forms (`None`: as the K3's).
fn from_program(cx: &mut Ctx, code: &str, p: &str) -> Option<Vec<String>> {
    let read = p.is_empty();
    if read {
        cx.remember("asked", code);
    } else if code != "PC" || p != "X" {
        cx.memo.remove("asked");
    }
    let one = |s: &str| Some(vec![s.to_string()]);
    let n = int(p).unwrap_or(0);
    if p.starts_with('$') && !matches!(code, "MD" | "LK" | "BW" | "FW") {
        return Some(Vec::new());
    }
    match code {
        "AG" if !read => Some(vec![format!("AG0{:03};", scale(n, 0, 60, 0, 255))]),
        "RG" if !read => {
            let att = int(p.trim_start_matches('-')).unwrap_or(0).min(60);
            Some(vec![format!("RG0{:03};", scale(60 - att, 0, 60, 0, 255))])
        }
        "SQ" if !read => Some(vec![format!("SQ0{:03};", scale(n, 0, 40, 0, 255))]),
        "MG" if !read => Some(vec![format!("MG{:03};", scale(n, 0, 80, 0, 100))]),
        "CP" if !read => Some(vec![format!("PL{:03};", scale(n, 0, 30, 0, 100))]),
        "CW" if !read => Some(vec![format!("KP{:02};", (n.max(30) - 30).min(75))]),
        "PC" if read => one("PC;"),
        "PC" if p == "X" => {
            cx.remember("asked", "PCX");
            one("PC;")
        }
        "PC" => Some(vec![format!("PC{:03};", pc_watts(p).unwrap_or(0))]),
        "AB" => match p {
            "0" | "3" => one("AB;"),
            "1" | "4" => one("BA;"),
            "2" | "5" => one("SV;"),
            _ => Some(Vec::new()),
        },
        "BN" if p == "+" => one("BU0;"),
        "BN" if p == "-" => one("BD0;"),
        "AT" | "TU" if read => one("AC;"),
        "AT" => one(if n == 2 { "AC001;" } else { "AC000;" }),
        "TU" => match n {
            3 | 4 => one("AC002;"),
            _ => Some(Vec::new()),
        },
        "NA" if read => one("BC0;"),
        "NA" => Some(vec![format!("BC0{};", n.min(1))]),
        "NM" if read => Some(vec!["BP00;".into(), "BP01;".into()]),
        "NM" if p.len() >= 5 => {
            let hz = int(&p[..4]).unwrap_or(0);
            Some(vec![format!("BP01{:03};", (hz / 10).min(400)), format!("BP00{:03};", int(&p[4..5]).unwrap_or(0).min(1))])
        }
        "NM" => Some(vec![format!("BP00{:03};", n.min(1))]),
        "NR" if read => Some(vec!["NR0;".into(), "RL0;".into()]),
        "NR" if p.len() >= 3 => Some(vec![format!("RL0{:02};", int(&p[..2]).unwrap_or(0)), format!("NR0{};", (int(&p[2..3]) == Some(1)) as u8)]),
        "NB" if read => Some(vec!["NB0;".into(), "NL0;".into()]),
        "NB" if p.len() >= 4 => {
            Some(vec![format!("NL0{:03};", int(&p[..2]).unwrap_or(0)), format!("NB0{};", int(&p[2..3]).unwrap_or(0).min(1))])
        }
        "NB" => Some(vec![format!("NB0{};", n.min(1))]),
        "PA" if p.len() >= 2 => {
            let (l, m) = (int(&p[..1]).unwrap_or(0), int(&p[1..2]).unwrap_or(0));
            Some(vec![format!("PA0{};", if m == 1 { l.min(3) } else { 0 })])
        }
        "RA" if p.len() >= 3 => {
            let (db, m) = (int(&p[..2]).unwrap_or(0), int(&p[2..3]).unwrap_or(0));
            Some(vec![format!("RA0{};", if m == 1 { (db / 3).min(7) } else { 0 })])
        }
        // Option modules: "4" in the 9th place says K4 (with S: K4D).
        "OM" if read => Some(vec![format!("{REPLY}OM -P------4---;")]),
        "PO" if read => one("RM5;"),
        "TM" if !read => Some(vec![REPLY.to_string()]),
        // The manual has no TM GET (the report comes by itself), but
        // Hamlib's K4 driver asks for it: one report from RigHelm's meters.
        "TM" => Some(vec!["RM4;".into(), "RM3;".into(), "RM5;".into(), "RM6;".into()]),
        "ME" => {
            let (id, raw) = p.split_once('.').unwrap_or((p, ""));
            let item = int(id).and_then(|id| cx.model.menu.iter().find(|m| m.addr == id))?;
            Some(vec![format!("EX{:03}{raw};", item.num)])
        }
        _ => None,
    }
}

// ------------------------------------------------------------- the radio

const K4_COMMANDS: &[&str] = &[
    "AB", "AC", "AG", "AI", "AN", "BA", "BC", "BD", "BP", "BS", "BU", "DN", "EX", "FA", "FB", "FR", "FT", "GT", "ID", "IF", "KP", "KS",
    "LK", "MD", "MG", "ML", "NB", "NL", "NR", "OI", "PA", "PC", "PL", "PS", "RA", "RC", "RD", "RG", "RL", "RM", "RT", "RU", "SH", "SM",
    "SQ", "SV", "TX", "UP", "VS", "XT",
];

pub static K4: RadioModel = RadioModel {
    key: "k4",
    name: "K4",
    maker: Maker::Elecraft,
    family: Family::Kenwood,
    proto: Proto::Text(&SPEC),
    id: "017",
    civ_addr: 0,
    hamlib: 2047,
    freq_min: 100_000,
    freq_max: 54_000_000,
    bands: k3::BANDS_160_6,
    modes: k3::MODES,
    menu: K4_MENU,
    roles: Roles { cat_items: &[43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54], ..Roles::NONE },
    caps: Caps {
        commands: K4_COMMANDS,
        widths: k3::widths,
        narrow: false,
        preamp: &[(0, "OFF"), (1, "10 dB"), (2, "18 dB / LNA"), (3, "PRE + LNA")],
        att: &[(0, "OFF"), (1, "3 dB"), (2, "6 dB"), (3, "9 dB"), (4, "12 dB"), (5, "15 dB"), (6, "18 dB"), (7, "21 dB")],
        agc: &[(1, "FAST"), (3, "SLOW")],
        roofing: &[],
        roofing_answers: &[],
        nb: &[(0, "OFF"), (1, "ON")],
        nr_max: 10,
        nb_level_max: 15,
        antennas: 3,
        power: (0, 110),
        meter_select: &[],
        voice_memories: 0,
        keyer_memories: 0,
        keyer_memory_len: 0,
        memory: Memory { first: 0, last: 0, pms: None },
        shared_clarifier: true,
    },
    bauds: &[115200, 57600, 38400, 19200, 9600, 4800],
    stop_bits: 1,
    rts_cts: false,
    sim: &["SH037;", "PC050;", "RL005;"],
};

#[cfg(test)]
mod tests {
    use super::*;

    fn radio() -> Ctx {
        Ctx::new(&K4)
    }

    fn program() -> Ctx {
        Ctx { program: true, ..Ctx::new(&K4) }
    }

    #[test]
    fn frames_from_the_manual() {
        let cx = &mut radio();
        assert_eq!(down(cx, "ID;"), ["K40;K31;TM1;ID;"]);
        assert_eq!(answer_prefix("K40;K31;TM1;ID;"), "ID");
        assert_eq!(down(cx, "AI0;"), ["K40;", "K31;", "AI0;", "TM0;"]);
        assert_eq!(down(cx, "FA14074000;"), ["FA00014074000;"]);
        assert_eq!(up(cx, "FA00014074000;"), ["FA14074000;"]);
        assert_eq!(down(cx, "MD0C;"), ["MD6;", "DT0;"]);
        assert_eq!(down(cx, "AG0255;"), ["AG060;"]);
        assert_eq!(up(cx, "AG030;"), ["AG0128;"]);
        assert_eq!(down(cx, "RG0255;"), ["RG-00;"]);
        assert_eq!(down(cx, "RG0000;"), ["RG-60;"]);
        assert_eq!(up(cx, "RG-30;"), ["RG0128;"]);
        assert_eq!(down(cx, "SQ0255;"), ["SQ040;"]);
        assert_eq!(down(cx, "MG100;"), ["MG080;"]);
        assert_eq!(down(cx, "PL100;"), ["CP030;"]);
        assert_eq!(up(cx, "CW60;"), ["KP30;"]);
        assert_eq!(up(cx, "CW25;"), ["KP00;"]);
        assert_eq!(down(cx, "KP40;"), ["CW70;"]);
        // Power: "for 5.0 W, use PC050L; for 50 W, use PC050H".
        assert_eq!(down(cx, "PC005;"), ["PC050L;"]);
        assert_eq!(down(cx, "PC050;"), ["PC050H;"]);
        assert_eq!(down(cx, "PC;"), ["PCX;"]);
        assert_eq!(answer_prefix("PCX;"), "PC");
        assert_eq!(up(cx, "PC050L;"), ["PC005;"]);
        assert_eq!(up(cx, "PC070H;"), ["PC070;"]);
        assert_eq!(down(cx, "AB;"), ["AB3;"]);
        assert_eq!(down(cx, "BA;"), ["AB4;"]);
        assert_eq!(down(cx, "SV;"), ["AB5;"]);
        assert_eq!(down(cx, "BU0;"), ["BN+;"]);
        assert_eq!(down(cx, "BD0;"), ["BN-;"]);
        assert_eq!(down(cx, "AC002;"), ["TU3;"]);
        assert_eq!(down(cx, "AC001;"), ["AT2;"]);
        assert_eq!(down(cx, "AC;"), ["AT;", "TU;"]);
        assert_eq!(up(cx, "AT2;"), ["AC001;"]);
        assert_eq!(up(cx, "TU3;"), ["AC002;"]);
        assert_eq!(up(cx, "TU0;"), ["AC001;"]);
        assert_eq!(up(cx, "NA1;"), ["BC01;"]);
        assert_eq!(up(cx, "NM15001;"), ["BP00001;", "BP01150;"]);
        assert_eq!(down(cx, "BP01100;"), ["NM10001;"]);
        assert_eq!(down(cx, "BP00000;"), ["NM0;"]);
        assert_eq!(up(cx, "NR071;"), ["NR01;", "RL007;"]);
        assert_eq!(down(cx, "NR00;"), ["NR070;"]);
        assert_eq!(down(cx, "RL003;"), ["NR030;"]);
        assert_eq!(up(cx, "NRS101;"), Vec::<String>::new());
        assert_eq!(up(cx, "NB0812;"), ["NB01;", "NL0008;"]);
        assert_eq!(down(cx, "NL0010;"), ["NB1012;"]);
        assert_eq!(down(cx, "NB00;"), ["NB0;"]);
        assert_eq!(up(cx, "PA21;"), ["PA02;"]);
        assert_eq!(up(cx, "PA10;"), ["PA00;"]);
        assert_eq!(down(cx, "PA00;"), ["PA10;"]);

        assert_eq!(down(cx, "PA03;"), ["PA31;"]);
        assert_eq!(up(cx, "RA091;"), ["RA03;"]);
        assert_eq!(down(cx, "RA07;"), ["RA211;"]);
        assert_eq!(down(cx, "RA00;"), ["RA210;"]);
        assert_eq!(down(cx, "RM6;"), ["PO;"]);
        assert_eq!(up(cx, "TM004012100015;"), ["RM3120;", "RM4102;", "RM5161;", "RM6052;"]);
        assert_eq!(up(cx, "PO0000;"), ["RM3120;", "RM4102;", "RM5000;", "RM6052;"]);
        assert_eq!(down(cx, "PA0;"), ["K41;PA;K40;K31;"]);
        assert_eq!(answer_prefix("K41;PA;K40;K31;"), "PA");
        assert_eq!(down(cx, "NL0;"), ["K41;NB;K40;K31;"]);
        assert_eq!(up(cx, "PA1;"), ["PA01;"]);
        assert_eq!(up(cx, "RA00;"), ["RA00;"]);
        assert_eq!(up(cx, "NB1;"), ["NB01;"]);
        assert_eq!(down(cx, "SM0;"), ["SM;"]);
        assert_eq!(up(cx, "SM0009;"), ["SM0130;", "RM1130;"]);
        assert_eq!(down(cx, "EX001;"), ["ME0039;"]);
        assert_eq!(up(cx, "ME0039.0020;"), ["EX0010020;"]);
        assert_eq!(down(cx, "EX0010020;"), ["ME0039.0020;"]);
        assert_eq!(down(cx, "AN03;"), ["AN3;"]);
        assert_eq!(down(cx, "FT3;"), ["FT1;"]);
        assert_eq!(down(cx, "TX1;"), ["TX;"]);
        assert_eq!(up(cx, "TQ1;"), ["TX1;"]);
    }

    #[test]
    fn as_seen_by_a_program() {
        let cx = &mut program();
        assert_eq!(up(cx, "K4;"), ["=K40;"]);
        assert_eq!(up(cx, "K40;"), ["="]);
        assert_eq!(up(cx, "ID;"), ["ID;"]);
        assert_eq!(up(cx, "PC050H;"), ["PC050;"]);
        assert_eq!(up(cx, "PCX;"), ["PC;"]);
        assert_eq!(down(cx, "PC005;"), ["PC050L;"]);
        assert_eq!(up(cx, "PC;"), ["PC;"]);
        assert_eq!(down(cx, "PC005;"), ["PC005;"]);
        assert_eq!(up(cx, "AG030;"), ["AG0128;"]);
        assert_eq!(down(cx, "AG0128;"), ["AG030;"]);
        assert_eq!(up(cx, "RG-10;"), ["RG0213;"]);
        assert_eq!(down(cx, "RG0213;"), ["RG-10;"]);
        assert_eq!(up(cx, "AB5;"), ["SV;"]);
        assert_eq!(up(cx, "BN+;"), ["BU0;"]);
        assert_eq!(up(cx, "TU3;"), ["AC002;"]);
        assert_eq!(up(cx, "NR;"), ["NR0;", "RL0;"]);
        assert_eq!(down(cx, "NR01;"), Vec::<String>::new());
        assert_eq!(down(cx, "RL007;"), ["NR071;"]);
        // K40 mode: the legacy answers.
        assert_eq!(up(cx, "NB;"), ["NB0;", "NL0;"]);
        assert_eq!(down(cx, "NB01;"), ["NB1;"]);
        assert_eq!(down(cx, "NL0008;"), Vec::<String>::new());
        assert_eq!(down(cx, "PA02;"), ["PA2;"]);
        assert_eq!(down(cx, "RA03;"), ["RA01;"]);
        assert_eq!(up(cx, "PC0551;"), ["PC055;"]);
        assert_eq!(up(cx, "PC0500;"), ["PC005;"]);
        // K41 mode: the K4's own.
        assert_eq!(up(cx, "K41;"), ["="]);
        assert_eq!(down(cx, "NB01;"), Vec::<String>::new());
        assert_eq!(down(cx, "NL0008;"), ["NB0810;"]);
        assert_eq!(down(cx, "PA02;"), ["PA21;"]);
        assert_eq!(down(cx, "RA03;"), ["RA091;"]);
        assert_eq!(up(cx, "ME0039;"), ["EX001;"]);
        assert_eq!(down(cx, "EX0010020;"), ["ME0039.0020;"]);
        assert_eq!(up(cx, "TM1;"), ["="]);
        assert_eq!(up(cx, "MD;"), ["MD0;"]);
        assert_eq!(down(cx, "MD03;"), ["MD3;"]);
    }

    #[test]
    fn round_trips() {
        let sets = [
            "AG0128;", "RG0200;", "MG040;",
 "PL060;", "KP40;", "PC005;", "PC050;", "PC110;", "BC01;", "BP01150;",
            "BP00001;", "RL007;", "NL0008;", "PA02;", "PA00;", "RA03;", "RA00;", "EX0050100;", "FA14074000;", "MD0C;", "MD0I;",
        ];
        for s in sets {
            let native = down(&mut radio(), s);
            assert!(!native.is_empty(), "{s}");
            let mut prog = program();
            let back: Vec<String> = native.iter().flat_map(|n| up(&mut prog, n)).collect();
            assert!(back.iter().any(|b| b == s), "{s} -> {native:?} -> {back:?}");
        }
        let answers = ["AG0128;", "RG0200;", "MG040;", "PL060;", "KP40;", "PC005;", "PC050;", "BC01;", "PA02;", "RA03;", "EX0050100;"];
        for a in answers {
            let mut prog = program();
            prog.remember("k4", "1");
            let native = down(&mut prog, a);

            assert_eq!(native.len(), 1, "{a} -> {native:?}");
            let back = up(&mut radio(), &native[0]);
            assert!(back.contains(&a.to_string()), "{a} -> {native:?} -> {back:?}");
        }
    }

    #[test]
    fn menu_is_numbered_in_order() {
        for (i, item) in K4.menu.iter().enumerate() {
            assert_eq!(item.num as usize, i + 1, "{}", item.name);
        }
        assert_eq!(K4.menu.len(), 85);
        for r in K4.roles.cat_items {
            assert!(K4.menu_item(*r).unwrap().name.starts_with("Serial"));
        }
    }
}
