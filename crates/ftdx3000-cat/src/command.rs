//! The FTDX3000 command table (CAT Operation Reference Book, pages 3-17).

use crate::frame;

/// Static description of one two-letter CAT command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CmdInfo {
    pub code: &'static str,
    pub name: &'static str,
    /// Accepts a Set command.
    pub set: bool,
    /// Accepts a Read command (and therefore produces an Answer).
    pub read: bool,
    /// Reported unsolicited while Auto Information (`AI1;`) is on.
    pub ai: bool,
    /// Number of parameter characters in the Read command, e.g. 1 for `AG0;`,
    /// 3 for `EX038;`. The answer is identified by the command plus this prefix.
    pub read_params: usize,
    /// A Set command has exactly the same text as the Answer it causes, so a
    /// cache can be updated from the Set without waiting for the radio.
    pub set_is_answer: bool,
}

const fn c(
    code: &'static str,
    name: &'static str,
    set: bool,
    read: bool,
    ai: bool,
    read_params: usize,
    set_is_answer: bool,
) -> CmdInfo {
    CmdInfo { code, name, set, read, ai, read_params, set_is_answer }
}

/// Every command the FTDX3000 implements.
pub const COMMANDS: &[CmdInfo] = &[
    c("AB", "VFO-A to VFO-B", true, false, false, 0, false),
    c("AC", "Antenna tuner control", true, true, true, 0, false),
    c("AG", "AF gain", true, true, true, 1, true),
    c("AI", "Auto information", true, true, false, 0, true),
    c("AM", "VFO-A to memory channel", true, false, false, 0, false),
    c("AN", "Antenna number", true, true, true, 1, false),
    c("BA", "VFO-B to VFO-A", true, false, false, 0, false),
    c("BC", "Auto notch", true, true, true, 1, true),
    c("BD", "Band down", true, false, false, 0, false),
    c("BI", "Break-in", true, true, true, 0, true),
    c("BP", "Manual notch", true, true, true, 2, true),
    c("BS", "Band select", true, false, false, 0, false),
    c("BU", "Band up", true, false, false, 0, false),
    c("BY", "Busy", false, true, true, 0, false),
    c("CH", "Channel up/down", true, false, false, 0, false),
    c("CN", "CTCSS tone number", true, true, true, 1, true),
    c("CO", "Contour / APF", true, true, true, 2, true),
    c("CS", "CW spot", true, true, true, 0, true),
    c("CT", "CTCSS", true, true, true, 1, true),
    c("DA", "Dimmer", true, true, false, 0, true),
    c("DN", "Mic down", true, false, false, 0, false),
    c("ED", "Encoder down", true, false, false, 0, false),
    c("EK", "ENT key", true, false, false, 0, false),
    c("EM", "Encode memory", true, true, false, 2, true),
    c("EN", "Encode", true, false, false, 0, false),
    c("EU", "Encoder up", true, false, false, 0, false),
    c("EX", "Menu", true, true, true, 3, true),
    c("FA", "Frequency VFO-A", true, true, true, 0, true),
    c("FB", "Frequency VFO-B", true, true, true, 0, true),
    c("FR", "Function RX", true, true, true, 0, true),
    c("FS", "Fast step", true, true, true, 0, false),
    c("FT", "Function TX", true, true, true, 0, false),
    c("GT", "AGC function", true, true, true, 1, false),
    c("ID", "Identification", false, true, false, 0, false),
    c("IF", "Information", false, true, true, 0, false),
    c("IS", "IF shift", true, true, true, 1, true),
    c("KM", "Keyer memory", true, true, false, 1, true),
    c("KP", "Key pitch", true, true, true, 0, true),
    c("KR", "Keyer", true, true, true, 0, true),
    c("KS", "Key speed", true, true, true, 0, true),
    c("KY", "CW keying", true, false, false, 0, false),
    c("LK", "Lock", true, true, true, 0, false),
    c("LM", "Load message", true, true, false, 1, false),
    c("MA", "Memory channel to VFO-A", true, false, false, 0, false),
    c("MC", "Memory channel", true, true, false, 0, true),
    c("MD", "Operating mode", true, true, true, 1, true),
    c("MG", "Mic gain", true, true, true, 0, true),
    c("ML", "Monitor level", true, true, true, 1, true),
    c("MR", "Memory channel read", false, true, false, 3, false),
    c("MS", "Meter switch", true, true, true, 0, true),
    c("MW", "Memory channel write", true, false, false, 0, false),
    c("MX", "MOX", true, true, true, 0, true),
    c("NA", "Narrow", true, true, true, 1, true),
    c("NB", "Noise blanker", true, true, true, 1, true),
    c("NL", "Noise blanker level", true, true, true, 1, true),
    c("NR", "Noise reduction", true, true, true, 1, true),
    c("OI", "Opposite band information", false, true, false, 0, false),
    c("OS", "Offset (repeater shift)", true, true, true, 1, true),
    c("PA", "Pre-amp (IPO)", true, true, true, 1, true),
    c("PB", "Play back", true, true, false, 1, false),
    c("PC", "Power control", true, true, true, 0, true),
    c("PL", "Speech processor level", true, true, true, 0, true),
    c("PR", "Speech processor", true, true, true, 1, true),
    c("PS", "Power switch", true, true, false, 0, true),
    c("QI", "QMB store", true, false, false, 0, false),
    c("QR", "QMB recall", true, false, false, 0, false),
    c("QS", "Quick split", true, false, false, 0, false),
    c("RA", "RF attenuator", true, true, true, 1, true),
    c("RC", "Clar clear", true, false, false, 0, false),
    c("RD", "Clar down", true, false, false, 0, false),
    c("RF", "Roofing filter", true, true, true, 1, false),
    c("RG", "RF gain", true, true, true, 1, true),
    c("RI", "Radio information", false, true, true, 1, false),
    c("RL", "Noise reduction level", true, true, true, 1, true),
    c("RM", "Read meter", false, true, false, 1, false),
    c("RO", "Rotator", true, true, true, 0, false),
    c("RS", "Radio status", false, true, true, 0, false),
    c("RT", "RX clarifier", true, true, true, 0, true),
    c("RU", "Clar up", true, false, false, 0, false),
    c("SC", "Scan", true, true, true, 0, true),
    c("SD", "CW break-in delay", true, true, true, 0, true),
    c("SF", "Sub-dial function", true, true, true, 0, true),
    c("SH", "Width", true, true, true, 1, true),
    c("SM", "S-meter", false, true, true, 1, false),
    c("SQ", "Squelch level", true, true, true, 1, true),
    c("SV", "Swap VFO", true, false, false, 0, false),
    c("TS", "TXW", true, true, true, 0, true),
    c("TX", "TX set", true, true, true, 0, false),
    c("UL", "PLL unlock status", false, true, true, 0, false),
    c("UP", "Mic up", true, false, false, 0, false),
    c("VD", "VOX delay", true, true, true, 0, true),
    c("VF", "VRF / uTune filter", true, true, true, 1, false),
    c("VG", "VOX gain", true, true, true, 0, true),
    c("VM", "[V/M] key", true, false, false, 0, false),
    c("VS", "VFO select", true, true, true, 0, true),
    c("VX", "VOX", true, true, true, 0, true),
    c("XT", "TX clarifier", true, true, true, 0, true),
];

pub fn lookup(code: &str) -> Option<&'static CmdInfo> {
    COMMANDS.iter().find(|c| c.code.eq_ignore_ascii_case(code))
}

/// What a frame sent *to* the radio is asking for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// A Read command. `key` identifies the answer it expects (see [`answer_key`]).
    Read { key: String },
    /// A Set command (possibly also a radio-side action like `BU0;`).
    Set,
    /// Not a command the FTDX3000 knows; it would answer `?;`.
    Unknown,
}

/// Classify a frame sent by a controlling program.
pub fn classify(frame: &str) -> Request {
    let Some((code, params)) = frame::split(frame) else {
        return Request::Unknown;
    };
    let Some(info) = lookup(&code) else {
        return Request::Unknown;
    };
    if info.read && params.len() == info.read_params {
        Request::Read { key: format!("{}{}", info.code, params.to_ascii_uppercase()) }
    } else if info.set {
        Request::Set
    } else {
        Request::Unknown
    }
}

/// The cache key of an answer frame: the command plus its Read parameters.
/// `FA014250000;` -> `FA`, `SM0123;` -> `SM0`, `EX0383;` -> `EX038`.
pub fn answer_key(frame: &str) -> Option<String> {
    let (code, params) = frame::split(frame)?;
    let info = lookup(&code)?;
    if params.len() < info.read_params {
        return None;
    }
    Some(format!("{}{}", info.code, &params[..info.read_params]))
}

/// Does this Set command make the radio transmit (or arm it to)?
///
/// Used to enforce "no transmit" for shared clients: PTT/MOX on, antenna
/// tuner start (sends a carrier), keyer/voice/encoder memory playback, and
/// turning VOX on.
pub fn is_tx_command(frame: &str) -> bool {
    let Some((code, p)) = frame::split(frame) else { return false };
    if p.is_empty() {
        return false; // a Read
    }
    match code.as_str() {
        "TX" | "MX" => p != "0",
        "AC" => p.ends_with('2'),
        "KY" | "EN" => true,
        // PB P1 P2: P2 = 0 stops playback.
        "PB" => p.len() >= 2 && &p[1..2] != "0",
        "VX" => p == "1",
        _ => false,
    }
}

/// Does this Set command stop transmitting?
pub fn is_unkey_command(frame: &str) -> bool {
    matches!(frame::split(frame), Some((c, p)) if (c == "TX" || c == "MX") && p == "0")
}

/// Normalize a frame: upper-case command letters, keep parameters as sent,
/// and make sure it is terminated.
pub fn normalize(frame: &str) -> String {
    let body = frame.trim().trim_end_matches(';');
    if body.len() < 2 || !body.is_char_boundary(2) {
        return format!("{body};");
    }
    let (code, params) = body.split_at(2);
    format!("{}{};", code.to_ascii_uppercase(), params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_reads_and_sets() {
        assert_eq!(classify("FA;"), Request::Read { key: "FA".into() });
        assert_eq!(classify("FA014074000;"), Request::Set);
        assert_eq!(classify("ag0;"), Request::Read { key: "AG0".into() });
        assert_eq!(classify("AG0128;"), Request::Set);
        assert_eq!(classify("EX038;"), Request::Read { key: "EX038".into() });
        assert_eq!(classify("EX0383;"), Request::Set);
        assert_eq!(classify("AB;"), Request::Set);
        assert_eq!(classify("BU0;"), Request::Set);
        assert_eq!(classify("ID;"), Request::Read { key: "ID".into() });
        assert_eq!(classify("ZZ;"), Request::Unknown);
        assert_eq!(classify("SM;"), Request::Unknown);
    }

    #[test]
    fn transmit_commands() {
        for f in ["TX1;", "tx1;", "MX1;", "AC002;", "KY1;", "EN01;", "PB01;", "PB11;", "VX1;"] {
            assert!(is_tx_command(f), "{f}");
        }
        for f in ["TX0;", "MX0;", "AC001;", "AC000;", "PB00;", "PB10;", "VX0;", "FA14074000;", "TX;"] {
            assert!(!is_tx_command(f), "{f}");
        }
        assert!(is_unkey_command("TX0;"));
        assert!(!is_unkey_command("TX1;"));
    }

    #[test]
    fn answer_keys() {
        assert_eq!(answer_key("FA014250000;").as_deref(), Some("FA"));
        assert_eq!(answer_key("SM0123;").as_deref(), Some("SM0"));
        assert_eq!(answer_key("EX0383;").as_deref(), Some("EX038"));
        assert_eq!(answer_key("BP01040;").as_deref(), Some("BP01"));
        assert_eq!(answer_key("?;"), None);
    }

    #[test]
    fn table_is_sorted_and_unique() {
        for w in COMMANDS.windows(2) {
            assert!(w[0].code < w[1].code, "{} before {}", w[0].code, w[1].code);
        }
    }
}
