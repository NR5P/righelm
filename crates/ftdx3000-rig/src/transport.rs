//! Byte transports to the radio: a real serial port, or an in-process link
//! (simulator, tests).

use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

use ftdx3000_cat::dialect::{Dialect, Out};
use ftdx3000_cat::{Family, RadioModel};
use serde::{Deserialize, Serialize};

/// A connected byte stream split into independently owned halves, so a
/// reader thread can block while the engine writes.
pub struct Link {
    /// Must return `ErrorKind::TimedOut`/`WouldBlock` (or `Ok(0)`) periodically
    /// so the reader thread can notice shutdown.
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FlowControl {
    /// RTS/CTS hardware handshake (what Hamlib uses for the FTDX3000).
    #[default]
    Hardware,
    /// No handshake; RTS is held asserted so a radio with "CAT RTS = ENABLE"
    /// still sends.
    None,
    /// No handshake, RTS and DTR held off: for radios that can be set to
    /// key the transmitter or send CW on those lines of the CAT port
    /// (Icom's USB SEND / USB KEYING, Kenwood's COM port PTT).
    Quiet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SerialSettings {
    pub path: String,
    /// 0 = probe 38400, 19200, 9600, 4800 (menu 038 CAT RATE).
    pub baud: u32,
    pub stop_bits: u8,
    pub flow: FlowControl,
}

impl Default for SerialSettings {
    fn default() -> Self {
        Self { path: String::new(), baud: 0, stop_bits: 2, flow: FlowControl::Hardware }
    }
}

pub const BAUD_RATES: [u32; 4] = [38400, 19200, 9600, 4800];

/// Open the serial port with exclusive access (`TIOCEXCL`): other programs
/// must go through our virtual ports instead of fighting over the radio.
pub fn open_serial(s: &SerialSettings, baud: u32, flow: FlowControl) -> io::Result<Link> {
    // macOS sets the baud rate with an ioctl that pseudo-terminals (the
    // simulator) reject with ENOTTY; `serialport` skips it for a rate of 0.
    #[cfg(target_os = "macos")]
    let port_baud = if is_pty(&s.path) { 0 } else { baud };
    #[cfg(not(target_os = "macos"))]
    let port_baud = baud;
    let port = serialport::new(&s.path, port_baud)
        .data_bits(serialport::DataBits::Eight)
        .parity(serialport::Parity::None)
        .stop_bits(if s.stop_bits == 1 { serialport::StopBits::One } else { serialport::StopBits::Two })
        .flow_control(match flow {
            FlowControl::Hardware => serialport::FlowControl::Hardware,
            FlowControl::None | FlowControl::Quiet => serialport::FlowControl::None,
        })
        .timeout(Duration::from_millis(50))
        .open_native()
        .map_err(io::Error::other)?;
    let mut port = port;
    // Not supported on PTYs (simulator); harmless to ignore there.
    match flow {
        FlowControl::None => {
            let _ = serialport::SerialPort::write_request_to_send(&mut port, true);
        }
        FlowControl::Quiet => {
            let _ = serialport::SerialPort::write_request_to_send(&mut port, false);
            let _ = serialport::SerialPort::write_data_terminal_ready(&mut port, false);
        }
        FlowControl::Hardware => {}
    }
    let _ = serialport::SerialPort::clear(&port, serialport::ClearBuffer::All);
    let reader = serialport::SerialPort::try_clone(&port).map_err(io::Error::other)?;
    let flow_txt = match flow {
        FlowControl::Hardware => "RTS/CTS",
        FlowControl::None => "no handshake",
        FlowControl::Quiet => "no handshake, RTS/DTR off",
    };
    Ok(Link {
        reader,
        writer: Box::new(port),
        description: format!("{} @ {baud} bps, 8N{}, {flow_txt}", s.path, s.stop_bits.clamp(1, 2)),
    })
}

/// Is `path` (or the link it points through) a macOS PTY slave, `/dev/ttysN`?
#[cfg(target_os = "macos")]
fn is_pty(path: &str) -> bool {
    std::fs::canonicalize(path)
        .ok()
        .and_then(|p| p.to_str().and_then(|s| s.strip_prefix("/dev/ttys")).map(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())))
        .unwrap_or(false)
}

/// Send `ID;` and wait for an `ID....;` answer.
fn probe_link(link: &mut Link, wait: Duration) -> Option<String> {
    // Exactly "ID;": the FTDX3000 answers an empty command (";") with "?;"
    // and then ignores the rest of that write.
    link.writer.write_all(b"ID;").ok()?;
    link.writer.flush().ok()?;
    read_until(link, wait, |got| {
        let text = String::from_utf8_lossy(got);
        let pos = text.find("ID")?;
        let end = text[pos..].find(';')?;
        Some(text[pos..pos + end + 1].to_string())
    })
}

/// Which Elecraft radio answered `ID017;`: a K4 knows `K4;`; the others
/// tell themselves apart in their `OM;` option string (KX3 `..02`, KX2
/// `..01` at the end; anything else is a K3).
fn elecraft_model(link: &mut Link, wait: Duration) -> Option<&'static RadioModel> {
    let ask = |link: &mut Link, cmd: &[u8], prefix: &'static str| -> Option<String> {
        link.writer.write_all(cmd).ok()?;
        link.writer.flush().ok()?;
        read_until(link, wait, |got| {
            let text = String::from_utf8_lossy(got);
            if text.contains("?;") {
                return Some(String::new());
            }
            let pos = text.find(prefix)?;
            let end = text[pos..].find(';')?;
            Some(text[pos..pos + end].to_string())
        })
        .filter(|a| !a.is_empty())
    };
    let by = |key: &str| ftdx3000_cat::model::by_key(key);
    if ask(link, b"K4;", "K4").is_some() {
        return by("k4");
    }
    let om = ask(link, b"OM;", "OM")?;
    let opts = om.strip_prefix("OM").unwrap_or(&om).trim_start_matches(' ');
    if opts.as_bytes().get(8) == Some(&b'4') {
        by("k4")
    } else if opts.ends_with("02") {
        by("kx3")
    } else if opts.ends_with("01") {
        by("kx2")
    } else {
        by("k3")
    }
}

/// Ask CI-V address `to` (0 = any radio) for its address, and return the
/// address that answered.
fn probe_civ(link: &mut Link, to: u8, wait: Duration) -> Option<u8> {
    link.writer.write_all(&[0xFE, 0xFE, to, 0xE0, 0x19, 0x00, 0xFD]).ok()?;
    link.writer.flush().ok()?;
    read_until(link, wait, |got| {
        // FE FE E0 <radio> 19 00 <radio> FD (after the echo of our own).
        got.windows(8).find(|w| w[0] == 0xFE && w[1] == 0xFE && w[2] == 0xE0 && w[4] == 0x19 && w[5] == 0x00 && w[7] == 0xFD).map(|w| w[6])
    })
}

fn read_until<T>(link: &mut Link, wait: Duration, mut done: impl FnMut(&[u8]) -> Option<T>) -> Option<T> {
    let start = Instant::now();
    let mut buf = [0u8; 256];
    let mut got = Vec::new();
    while start.elapsed() < wait {
        match link.reader.read(&mut buf) {
            Ok(n) if n > 0 => {
                got.extend_from_slice(&buf[..n]);
                if let Some(v) = done(&got) {
                    return Some(v);
                }
            }
            Ok(_) => {}
            Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock) => {}
            Err(_) => return None,
        }
    }
    None
}

/// The model an `ID;` answer means. A model chosen in the settings wins
/// when the answer fits it (several models share an answer) or is unknown.
fn model_for_id(id: &str, want: Option<&'static RadioModel>) -> Option<&'static RadioModel> {
    let found = ftdx3000_cat::model::by_id_answer(id);
    match (want, found) {
        (Some(w), Some(f)) if f.id == w.id => Some(w),
        (_, Some(f)) => Some(f),
        (w, None) => w,
    }
}

/// Result of a successful probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    pub baud: u32,
    pub flow: FlowControl,
    /// What identified the radio: `ID0462;`, or `CI-V 94` for an Icom.
    pub id_answer: String,
    /// The model it answered as, if RigHelm knows it.
    pub model: Option<&'static RadioModel>,
    /// CI-V address that answered (Icom).
    pub civ_addr: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ask {
    Text,
    CiV,
}

/// Rates any supported radio may use.
const ALL_BAUDS: [u32; 6] = [38400, 19200, 115200, 9600, 57600, 4800];

/// The attempts to make, in order: what to ask, at which rate and handshake.
fn attempts(s: &SerialSettings, want: Option<&'static RadioModel>) -> Vec<(Ask, u32, FlowControl)> {
    let first = |list: &[u32]| -> Vec<u32> {
        let mut v: Vec<u32> = if s.baud == 0 { Vec::new() } else { vec![s.baud] };
        v.extend(list.iter().copied().filter(|b| *b != s.baud));
        v
    };
    let other = |f: FlowControl| if f == FlowControl::Hardware { FlowControl::None } else { FlowControl::Hardware };
    let text = |bauds: &[u32], flows: [FlowControl; 2]| -> Vec<(Ask, u32, FlowControl)> {
        flows.iter().flat_map(|&f| first(bauds).into_iter().map(move |b| (Ask::Text, b, f))).collect()
    };
    let civ = |bauds: &[u32]| -> Vec<(Ask, u32, FlowControl)> { first(bauds).into_iter().map(|b| (Ask::CiV, b, FlowControl::Quiet)).collect() };
    match want {
        Some(m) if m.family == Family::Icom => civ(m.bauds),
        Some(m) if m.family == Family::Ftdx3000 => text(m.bauds, [s.flow, other(s.flow)]),
        // Other text radios: never hold RTS up without handshake (it can be
        // their PTT line); RTS/CTS or both lines off.
        Some(m) => {
            let flows = if m.rts_cts { [FlowControl::Hardware, FlowControl::Quiet] } else { [FlowControl::Quiet, FlowControl::Hardware] };
            text(m.bauds, flows)
        }
        None => {
            let mut v = text(&ALL_BAUDS, [s.flow, other(s.flow)]);
            let icom = civ(&[115200, 19200, 9600, 4800, 38400, 57600]);
            // A port named after an Icom radio is asked in CI-V first.
            if s.path.contains("IC-") || s.path.contains("IC_") || s.path.to_ascii_lowercase().contains("icom") {
                let mut i = icom;
                i.extend(v);
                v = i;
            } else {
                v.extend(icom);
            }
            v
        }
    }
}

/// Find the baud rate (and handshake) the radio answers on, and which radio
/// it is, and return the port still open (re-opening would briefly drop
/// RTS/DTR and the link). `want`: the model chosen in the settings (only its
/// protocol is tried), or `None` to find out.
///
/// This *does* talk to the radio (it sends `ID;` or a CI-V address query),
/// so it is only used when the user asks to connect.
pub fn probe(s: &SerialSettings, want: Option<&'static RadioModel>, mut on_try: impl FnMut(u32, FlowControl)) -> io::Result<(ProbeResult, Link)> {
    let mut last_err = None;
    for (ask, baud, flow) in attempts(s, want) {
        on_try(baud, flow);
        match open_serial(s, baud, flow) {
            Ok(mut link) => {
                let wait = Duration::from_millis(400);
                let found = match ask {
                    Ask::Text => probe_link(&mut link, wait).map(|id| {
                        let mut model = model_for_id(&id, want);
                        // K3, K3S, KX3, KX2 and K4 all answer as a TS-570.
                        if id == "ID017;" && !want.is_some_and(|w| w.id == "017") {
                            model = elecraft_model(&mut link, wait).or(model);
                        }
                        ProbeResult { baud, flow, id_answer: id, model, civ_addr: None }
                    }),
                    Ask::CiV => {
                        let to = want.map_or(0, |m| m.civ_addr);
                        probe_civ(&mut link, to, wait)
                            .or_else(|| if to != 0 { probe_civ(&mut link, 0, wait) } else { None })
                            .map(|addr| ProbeResult {
                                baud,
                                flow,
                                id_answer: format!("CI-V {addr:02X}"),
                                model: want.filter(|m| m.civ_addr == addr).or_else(|| ftdx3000_cat::model::by_civ_addr(addr)).or(want),
                                civ_addr: Some(addr),
                            })
                    }
                };
                if let Some(found) = found {
                    return Ok((found, link));
                }
            }
            Err(e) => {
                // Busy/missing port won't get better at another baud rate.
                if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied) || e.to_string().contains("busy") {
                    return Err(e);
                }
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "the radio did not answer at any baud rate")))
}

/// Put a translator on a link: RigHelm writes and reads FTDX3000 CAT, the
/// radio gets and sends its own protocol.
///
/// A thread does the translating. It sends the radio one command at a time
/// where the radio answers each one (CI-V), and keeps answers RigHelm gets
/// without the radio in their place in the order. When RigHelm closes the
/// link, what it wrote last (an un-key) still goes out before the thread
/// ends.
pub fn translated(link: Link, dialect: Box<dyn Dialect>) -> Link {
    use crossbeam_channel::{RecvTimeoutError, select, unbounded};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let (to_pump, from_righelm) = unbounded::<Vec<u8>>();
    let (to_righelm, up) = unbounded::<Vec<u8>>();
    let (native_tx, native_rx) = unbounded::<io::Result<Vec<u8>>>();
    let stop = Arc::new(AtomicBool::new(false));
    let description = link.description.clone();
    let mut native_reader = link.reader;
    let stop_r = stop.clone();
    std::thread::Builder::new()
        .name("rig-native-reader".into())
        .spawn(move || {
            let mut buf = [0u8; 512];
            while !stop_r.load(Ordering::Relaxed) {
                match native_reader.read(&mut buf) {
                    Ok(0) => {}
                    Ok(n) => {
                        if native_tx.send(Ok(buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                    Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => {}
                    Err(e) => {
                        let _ = native_tx.send(Err(e));
                        break;
                    }
                }
            }
        })
        .expect("spawn native reader");
    let failed = Arc::new(AtomicBool::new(false));
    let failed_w = failed.clone();
    let mut writer = link.writer;
    std::thread::Builder::new()
        .name("rig-translator".into())
        .spawn(move || {
            let mut pump = Pump { dialect, framer: ftdx3000_cat::Framer::new(), queue: std::collections::VecDeque::new(), waiting: None, up: to_righelm };
            let mut closing: Option<Instant> = None;
            loop {
                let wait = pump.waiting.map_or(Duration::from_millis(100), |d| d.saturating_duration_since(Instant::now()).max(Duration::from_millis(1)));
                if closing.is_none() {
                    select! {
                        recv(from_righelm) -> msg => match msg {
                            Ok(bytes) => pump.from_righelm(&bytes),
                            Err(_) => closing = Some(Instant::now()),
                        },
                        recv(native_rx) -> msg => match msg {
                            Ok(Ok(bytes)) => pump.from_radio(&bytes),
                            _ => break,
                        },
                        default(wait) => {}
                    }
                } else {
                    match native_rx.recv_timeout(wait) {
                        Ok(Ok(bytes)) => pump.from_radio(&bytes),
                        Ok(Err(_)) | Err(RecvTimeoutError::Disconnected) => break,
                        Err(RecvTimeoutError::Timeout) => {}
                    }
                }
                pump.check_timeout();
                if pump.send(&mut writer).is_err() {
                    failed_w.store(true, Ordering::Relaxed);
                    break;
                }
                // Closed: finish what was written, then stop.
                if let Some(t) = closing {
                    if (pump.queue.is_empty() && pump.waiting.is_none()) || t.elapsed() > Duration::from_secs(1) {
                        break;
                    }
                }
            }
            stop.store(true, Ordering::Relaxed);
        })
        .expect("spawn translator");
    let reader = pipe::pipe_from(up);
    Link { reader: Box::new(reader), writer: Box::new(TranslatorWriter { tx: to_pump, failed }), description }
}

struct TranslatorWriter {
    tx: crossbeam_channel::Sender<Vec<u8>>,
    failed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Write for TranslatorWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.failed.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.tx.send(buf.to_vec()).map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The translating thread's state.
struct Pump {
    dialect: Box<dyn Dialect>,
    framer: ftdx3000_cat::Framer,
    queue: std::collections::VecDeque<Out>,
    /// Waiting for the radio's answer until then.
    waiting: Option<Instant>,
    up: crossbeam_channel::Sender<Vec<u8>>,
}

impl Pump {
    fn from_righelm(&mut self, bytes: &[u8]) {
        for frame in self.framer.push(bytes) {
            let frame = ftdx3000_cat::command::normalize(&frame);
            self.queue.extend(self.dialect.command(&frame));
        }
    }

    fn from_radio(&mut self, bytes: &[u8]) {
        for f in self.dialect.split(bytes) {
            let back = self.dialect.answer(&f);
            if back.reply {
                self.waiting = None;
            }
            for o in back.then.into_iter().rev() {
                self.queue.push_front(o);
            }
            self.pass_up(back.frames);
        }
    }

    fn check_timeout(&mut self) {
        if self.waiting.is_some_and(|d| Instant::now() >= d) {
            self.waiting = None;
            let frames = self.dialect.timeout();
            self.pass_up(frames);
        }
    }

    fn pass_up(&mut self, frames: Vec<String>) {
        let text: String = frames.concat();
        if !text.is_empty() {
            let _ = self.up.send(text.into_bytes());
        }
    }

    /// Send what can go now.
    fn send(&mut self, w: &mut (dyn Write + Send)) -> io::Result<()> {
        let mut wrote = false;
        while self.waiting.is_none() {
            let Some(out) = self.queue.pop_front() else { break };
            match out {
                Out::Answer(a) => self.pass_up(vec![a]),
                Out::Radio { bytes, reply } => {
                    w.write_all(&bytes)?;
                    wrote = true;
                    if reply {
                        w.flush()?;
                        self.waiting = Some(Instant::now() + Duration::from_millis(self.dialect.reply_timeout_ms()));
                    }
                }
            }
        }
        if wrote {
            w.flush()?;
        }
        Ok(())
    }
}

/// An in-process duplex pipe (used for the built-in simulator and tests).
pub mod pipe {
    use super::*;
    use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};

    pub struct PipeReader {
        rx: Receiver<Vec<u8>>,
        pending: Vec<u8>,
    }

    pub struct PipeWriter {
        tx: Sender<Vec<u8>>,
    }

    impl Read for PipeReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.pending.is_empty() {
                match self.rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(d) => self.pending = d,
                    Err(RecvTimeoutError::Timeout) => return Err(io::ErrorKind::TimedOut.into()),
                    Err(RecvTimeoutError::Disconnected) => return Err(io::ErrorKind::BrokenPipe.into()),
                }
            }
            let n = buf.len().min(self.pending.len());
            buf[..n].copy_from_slice(&self.pending[..n]);
            self.pending.drain(..n);
            Ok(n)
        }
    }

    impl Write for PipeWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.tx.send(buf.to_vec()).map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// One direction of a pipe.
    pub fn pipe() -> (PipeWriter, PipeReader) {
        let (tx, rx) = crossbeam_channel::unbounded();
        (PipeWriter { tx }, PipeReader { rx, pending: Vec::new() })
    }

    /// A reader over an existing channel.
    pub fn pipe_from(rx: Receiver<Vec<u8>>) -> PipeReader {
        PipeReader { rx, pending: Vec::new() }
    }
}
