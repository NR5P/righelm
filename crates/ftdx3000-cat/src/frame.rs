//! Splitting a raw CAT byte stream into `;`-terminated frames.

/// Longest frame we accept. The longest legitimate frame is a keyer/encode
/// memory with 50 characters (~55 bytes); anything longer is line noise.
pub const MAX_FRAME: usize = 128;

/// Accumulates bytes and yields complete frames (including the trailing `;`).
///
/// Control characters (CR, LF, NUL, ...) are dropped, matching the radio's own
/// behaviour, so clients that append newlines still work.
#[derive(Debug, Default, Clone)]
pub struct Framer {
    buf: String,
}

impl Framer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed raw bytes and return every complete frame they finish.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        for &b in bytes {
            if b == b';' {
                let mut frame = std::mem::take(&mut self.buf);
                frame.push(';');
                out.push(frame);
            } else if b >= 0x20 && b < 0x7f {
                if self.buf.len() >= MAX_FRAME {
                    self.buf.clear();
                }
                // Leading spaces are noise; spaces inside a frame (keyer text) are data.
                if !(b == b' ' && self.buf.is_empty()) {
                    self.buf.push(b as char);
                }
            }
        }
        out
    }

    /// Bytes received that are not yet terminated.
    pub fn pending(&self) -> &str {
        &self.buf
    }

    pub fn clear(&mut self) {
        self.buf.clear();
    }
}

/// Split a frame into its two-letter command (upper-cased) and parameter text
/// (without the terminator). Returns `None` for frames shorter than a command.
pub fn split(frame: &str) -> Option<(String, &str)> {
    let body = frame.strip_suffix(';').unwrap_or(frame);
    if body.len() < 2 || !body.is_char_boundary(2) {
        return None;
    }
    let (code, params) = body.split_at(2);
    if !code.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some((code.to_ascii_uppercase(), params))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_frames_across_reads() {
        let mut f = Framer::new();
        assert!(f.push(b"FA0142").is_empty());
        assert_eq!(f.push(b"50000;MD02;\r\nID"), vec!["FA014250000;", "MD02;"]);
        assert_eq!(f.pending(), "ID");
        assert_eq!(f.push(b"0462;"), vec!["ID0462;"]);
    }

    #[test]
    fn keeps_spaces_inside_text() {
        let mut f = Framer::new();
        assert_eq!(f.push(b" KM1CQ TEST;"), vec!["KM1CQ TEST;"]);
    }

    #[test]
    fn split_upper_cases_command() {
        assert_eq!(split("fa;"), Some(("FA".to_string(), "")));
        assert_eq!(split("MD02;"), Some(("MD".to_string(), "02")));
        assert_eq!(split("?;"), None);
    }
}
