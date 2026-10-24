//! Radios with text CAT commands (`FA014250000;`): Yaesu's other radios,
//! Kenwood, Elecraft.
//!
//! Their commands look like the FTDX3000's, and a Set has the same layout as
//! the Answer it would produce, so one pair of functions per model does all
//! the translating ([`TextSpec`]):
//!
//! * `down`: a RigHelm (FTDX3000) frame, Set, Read or Answer, as the radio
//!   writes it;
//! * `up`: one of the radio's frames as RigHelm's.
//!
//! Towards the radio ([`TextDialect`]) RigHelm's commands go `down` and the
//! radio's answers come `up`. Towards a program that expects the radio
//! ([`TextEmulator`]: virtual ports, the simulator) it's the other way round.

use std::collections::{HashMap, VecDeque};

use super::{Back, Dialect, Emulator, Out, Request, TextSplitter};
use crate::command::{self, Request as CatRequest};
use crate::frame;
use crate::model::RadioModel;

/// How one text radio's commands differ from the FTDX3000's.
#[derive(Debug)]
pub struct TextSpec {
    /// A RigHelm frame as this radio's frame(s), or none if it has nothing
    /// like it. Called for Sets and Reads going to the radio, and for
    /// Answers going to a program that expects this radio.
    pub down: fn(&mut Ctx, &str) -> Vec<String>,
    /// This radio's frame as RigHelm frame(s), or none if RigHelm has
    /// nothing like it. Called for the radio's Answers and auto-information,
    /// and for a program's Sets and Reads.
    pub up: fn(&mut Ctx, &str) -> Vec<String>,
    /// What the answer to one of this radio's Read commands starts with
    /// (normally the Read without its `;`).
    pub answer_prefix: fn(&str) -> String,
}

/// The usual answer prefix: the Read itself (`FA;` -> `FA`), except memory
/// reads, whose answers name the radio's current channel instead.
pub fn read_prefix(read: &str) -> String {
    let body = read.trim_end_matches(';').to_string();
    if body.len() > 2 && body[..2].eq_ignore_ascii_case("MR") { body[..2].to_string() } else { body }
}

/// What a translation can look at: the model, and values it remembered
/// (for RigHelm values made of two of the radio's, or the other way round).
pub struct Ctx {
    pub model: &'static RadioModel,
    pub memo: HashMap<String, String>,
}

impl Ctx {
    pub fn new(model: &'static RadioModel) -> Self {
        Self { model, memo: HashMap::new() }
    }

    pub fn remember(&mut self, key: &str, value: impl Into<String>) {
        self.memo.insert(key.to_string(), value.into());
    }

    pub fn recall(&self, key: &str) -> Option<&str> {
        self.memo.get(key).map(String::as_str)
    }

    /// A RigHelm frame as is, if this radio has that command.
    pub fn same(&self, f: &str) -> Vec<String> {
        match frame::split(f) {
            Some((code, _)) if self.model.has(&code) => vec![f.to_string()],
            _ => Vec::new(),
        }
    }
}

/// Is this RigHelm frame a Read command (`FA;`, `AG0;`, `EX038;`)?
pub fn is_read(f: &str) -> bool {
    matches!(command::classify(f), CatRequest::Read { .. })
}

/// Code and parameters of a frame (`MD02;` -> `("MD", "02")`).
pub fn parts(f: &str) -> Option<(String, &str)> {
    frame::split(f)
}

struct Pending {
    prefix: String,
    /// Pass a `?;` for it up (the first of a RigHelm Read's reads).
    report: bool,
}

/// Radio side.
pub struct TextDialect {
    spec: &'static TextSpec,
    cx: Ctx,
    split: TextSplitter,
    waiting: VecDeque<Pending>,
    /// Sets written since the last answer: a `?;` is theirs first (the
    /// radio answers in order). `true`: pass it up.
    unconfirmed: VecDeque<bool>,
}

impl TextDialect {
    pub fn new(model: &'static RadioModel, spec: &'static TextSpec) -> Self {
        Self { spec, cx: Ctx::new(model), split: TextSplitter::default(), waiting: VecDeque::new(), unconfirmed: VecDeque::new() }
    }
}

impl Dialect for TextDialect {
    fn command(&mut self, f: &str) -> Vec<Out> {
        let req = command::classify(f);
        let has = frame::split(f).is_some_and(|(c, _)| self.cx.model.has(&c));
        let native = if has && req != CatRequest::Unknown { (self.spec.down)(&mut self.cx, f) } else { Vec::new() };
        if native.is_empty() {
            return vec![Out::reject()];
        }
        match req {
            CatRequest::Read { .. } => native
                .into_iter()
                .enumerate()
                .map(|(i, n)| {
                    self.waiting.push_back(Pending { prefix: (self.spec.answer_prefix)(&n), report: i == 0 });
                    Out::Radio { bytes: n.into_bytes(), reply: true }
                })
                .collect(),
            _ => native
                .into_iter()
                .enumerate()
                .map(|(i, n)| {
                    if self.unconfirmed.len() >= 64 {
                        self.unconfirmed.pop_front();
                    }
                    self.unconfirmed.push_back(i == 0);
                    Out::Radio { bytes: n.into_bytes(), reply: false }
                })
                .collect(),
        }
    }

    fn split(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.split.split(bytes)
    }

    fn answer(&mut self, raw: &[u8]) -> Back {
        let f = String::from_utf8_lossy(raw).to_string();
        if f == "?;" {
            if let Some(report) = self.unconfirmed.pop_front() {
                return Back::frames(if report { vec![f] } else { Vec::new() });
            }
            if let Some(p) = self.waiting.pop_front() {
                return Back::reply(if p.report { vec![f] } else { Vec::new() });
            }
            return Back::frames(vec![f]);
        }
        let frames = (self.spec.up)(&mut self.cx, &f);
        let is_reply = self.waiting.front().is_some_and(|p| f.len() >= p.prefix.len() && f[..p.prefix.len()].eq_ignore_ascii_case(&p.prefix));
        if is_reply {
            self.waiting.pop_front();
            // Everything written before that read has been dealt with.
            self.unconfirmed.clear();
            Back::reply(frames)
        } else {
            Back::frames(frames)
        }
    }

    fn timeout(&mut self) -> Vec<String> {
        self.waiting.pop_front();
        Vec::new()
    }
}

/// Program side: makes a program see this radio.
pub struct TextEmulator {
    spec: &'static TextSpec,
    cx: Ctx,
    split: TextSplitter,
}

impl TextEmulator {
    pub fn new(model: &'static RadioModel, spec: &'static TextSpec) -> Self {
        Self { spec, cx: Ctx::new(model), split: TextSplitter::default() }
    }
}

impl Emulator for TextEmulator {
    fn split(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.split.split(bytes)
    }

    fn request(&mut self, raw: &[u8]) -> Request {
        let f = command::normalize(&String::from_utf8_lossy(raw));
        let ours = (self.spec.up)(&mut self.cx, &f);
        let mut req = Request { tag: (self.spec.answer_prefix)(&f), ..Default::default() };
        if ours.is_empty() {
            req.immediate = Some(b"?;".to_vec());
            return req;
        }
        for c in ours {
            match command::classify(&c) {
                CatRequest::Read { .. } => req.reads.push(c),
                CatRequest::Set => match c.as_str() {
                    "AI0;" => req.auto_info = Some(false),
                    "AI1;" => req.auto_info = Some(true),
                    _ => req.sets.push(c),
                },
                CatRequest::Unknown => {}
            }
        }
        if req.reads.is_empty() && req.sets.is_empty() && req.auto_info.is_none() {
            req.immediate = Some(b"?;".to_vec());
        }
        req
    }

    fn respond(&mut self, req: &Request, answers: &[Option<String>]) -> Vec<u8> {
        let mut out = String::new();
        for a in answers {
            match a.as_deref() {
                Some(a) if a != "?;" => {
                    let native = (self.spec.down)(&mut self.cx, a);
                    // Only the answer to what was asked (a RigHelm value can
                    // be two of the radio's).
                    let asked: Vec<&String> = native.iter().filter(|n| n.len() >= req.tag.len() && n[..req.tag.len()].eq_ignore_ascii_case(&req.tag)).collect();
                    if asked.is_empty() {
                        native.iter().for_each(|n| out.push_str(n));
                    } else {
                        asked.into_iter().for_each(|n| out.push_str(n));
                    }
                }
                _ => {
                    out.push_str("?;");
                    break;
                }
            }
        }
        out.into_bytes()
    }

    fn unsolicited(&mut self, f: &str) -> Option<Vec<u8>> {
        let native = (self.spec.down)(&mut self.cx, f);
        (!native.is_empty()).then(|| native.concat().into_bytes())
    }
}

// ------------------------------------------------------------- helpers
// Building blocks for the models' `down` and `up`.

/// `FA14074000;` with the frequency in `digits` digits (`FA014074000;`).
/// Works both ways (RigHelm's frequency field is 8 digits, 9 above 99.999999
/// MHz), and leaves Reads alone.
pub fn freq_digits(f: &str, digits: usize) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if p.is_empty() {
        return vec![f.to_string()];
    }
    match p.parse::<u64>() {
        Ok(hz) => vec![format!("{code}{hz:0digits$};")],
        Err(_) => Vec::new(),
    }
}

/// Re-lay an `IF`/`OI`/`MR` answer or an `MW` set whose fields are the
/// FTDX3000's but with a `digits`-digit frequency. `MR` reads pass through.
pub fn info_digits(f: &str, digits: usize) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if p.len() <= 3 {
        return vec![f.to_string()];
    }
    // P1 (3) + frequency + the 13 characters after it.
    let flen = p.len().checked_sub(3 + 13).filter(|n| (8..=11).contains(n));
    let Some(flen) = flen else { return Vec::new() };
    let Ok(hz) = p[3..3 + flen].parse::<u64>() else { return Vec::new() };
    vec![format!("{code}{}{hz:0digits$}{};", &p[..3], &p[3 + flen..])]
}

/// Menu items numbered with the radio's own `addr` in `digits` digits:
/// `EX038;` <-> `EX0103;`, `EX0383;` <-> `EX01033;`.
pub fn menu_down(cx: &Ctx, f: &str, digits: usize) -> Vec<String> {
    let Some((_, p)) = parts(f) else { return Vec::new() };
    let Some(num) = p.get(..3).and_then(|n| n.parse::<u16>().ok()) else { return Vec::new() };
    let Some(item) = cx.model.menu_item(num) else { return Vec::new() };
    vec![format!("EX{:0digits$}{};", item.addr, &p[3..])]
}

pub fn menu_up(cx: &Ctx, f: &str, digits: usize) -> Vec<String> {
    let Some((_, p)) = parts(f) else { return Vec::new() };
    let Some(addr) = p.get(..digits).and_then(|n| n.parse::<u32>().ok()) else { return Vec::new() };
    let Some(item) = cx.model.menu.iter().find(|m| m.addr == addr) else { return Vec::new() };
    vec![format!("EX{:03}{};", item.num, &p[digits..])]
}

/// Insert `text` at parameter position `at` (`SH0` + `0` + `12` for a radio
/// with a fixed P2) — for Sets and Answers, not Reads of `read_len` params.
pub fn insert_param(f: &str, at: usize, text: &str, read_len: usize) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if p.len() <= read_len || p.len() < at {
        return vec![f.to_string()];
    }
    vec![format!("{code}{}{text}{};", &p[..at], &p[at..])]
}

/// Remove `len` parameter characters at `at` (the reverse of
/// [`insert_param`]).
pub fn remove_param(f: &str, at: usize, len: usize, read_len: usize) -> Vec<String> {
    let Some((code, p)) = parts(f) else { return Vec::new() };
    if p.len() <= read_len || p.len() < at + len {
        return vec![f.to_string()];
    }
    vec![format!("{code}{}{};", &p[..at], &p[at + len..])]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(freq_digits("FA14074000;", 9), vec!["FA014074000;"]);
        assert_eq!(freq_digits("FA014074000;", 8), vec!["FA14074000;"]);
        assert_eq!(freq_digits("FA;", 9), vec!["FA;"]);
        assert_eq!(freq_digits("FA144174000;", 8), vec!["FA144174000;"]);
        assert_eq!(info_digits("IF00114074000+012010200000;", 9), vec!["IF001014074000+012010200000;"]);
        assert_eq!(info_digits("IF001014074000+012010200000;", 8), vec!["IF00114074000+012010200000;"]);
        assert_eq!(info_digits("MR001;", 9), vec!["MR001;"]);
        assert_eq!(insert_param("SH012;", 1, "0", 1), vec!["SH0012;"]);
        assert_eq!(insert_param("SH0;", 1, "0", 1), vec!["SH0;"]);
        assert_eq!(remove_param("SH0012;", 1, 1, 1), vec!["SH012;"]);
        assert_eq!(read_prefix("MR005;"), "MR");
        assert_eq!(read_prefix("EX010101;"), "EX010101");
    }
}
