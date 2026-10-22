//! Byte transports to the radio: a real serial port, or an in-process link
//! (simulator, tests).

use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

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
    let port = serialport::new(&s.path, baud)
        .data_bits(serialport::DataBits::Eight)
        .parity(serialport::Parity::None)
        .stop_bits(if s.stop_bits == 1 { serialport::StopBits::One } else { serialport::StopBits::Two })
        .flow_control(match flow {
            FlowControl::Hardware => serialport::FlowControl::Hardware,
            FlowControl::None => serialport::FlowControl::None,
        })
        .timeout(Duration::from_millis(50))
        .open_native()
        .map_err(io::Error::other)?;
    let mut port = port;
    if flow == FlowControl::None {
        // Not supported on PTYs (simulator); harmless to ignore there.
        let _ = serialport::SerialPort::write_request_to_send(&mut port, true);
    }
    let _ = serialport::SerialPort::clear(&port, serialport::ClearBuffer::All);
    let reader = serialport::SerialPort::try_clone(&port).map_err(io::Error::other)?;
    let flow_txt = match flow {
        FlowControl::Hardware => "RTS/CTS",
        FlowControl::None => "no handshake",
    };
    Ok(Link {
        reader,
        writer: Box::new(port),
        description: format!("{} @ {baud} bps, 8N{}, {flow_txt}", s.path, s.stop_bits.clamp(1, 2)),
    })
}

/// Send `ID;` and wait for an `ID....;` answer.
fn probe_link(link: &mut Link, wait: Duration) -> Option<String> {
    // Exactly "ID;": the FTDX3000 answers an empty command (";") with "?;"
    // and then ignores the rest of that write.
    link.writer.write_all(b"ID;").ok()?;
    link.writer.flush().ok()?;
    let start = Instant::now();
    let mut buf = [0u8; 256];
    let mut got = Vec::new();
    while start.elapsed() < wait {
        match link.reader.read(&mut buf) {
            Ok(n) if n > 0 => {
                got.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&got);
                if let Some(pos) = text.find("ID") {
                    if let Some(end) = text[pos..].find(';') {
                        return Some(text[pos..pos + end + 1].to_string());
                    }
                }
            }
            Ok(_) => {}
            Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock) => {}
            Err(_) => return None,
        }
    }
    None
}

/// Result of a successful probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    pub baud: u32,
    pub flow: FlowControl,
    pub id_answer: String,
}

/// Find the baud rate (and handshake) the radio answers on, and return the
/// port still open (re-opening would briefly drop RTS/DTR and the link).
///
/// This *does* talk to the radio (it sends `ID;`), so it is only used when the
/// user asks to connect.
pub fn probe(s: &SerialSettings, mut on_try: impl FnMut(u32, FlowControl)) -> io::Result<(ProbeResult, Link)> {
    let bauds: Vec<u32> = if s.baud == 0 {
        BAUD_RATES.to_vec()
    } else {
        std::iter::once(s.baud).chain(BAUD_RATES.into_iter().filter(|b| *b != s.baud)).collect()
    };
    let flows = [s.flow, if s.flow == FlowControl::Hardware { FlowControl::None } else { FlowControl::Hardware }];
    let mut last_err = None;
    for flow in flows {
        for &baud in &bauds {
            on_try(baud, flow);
            match open_serial(s, baud, flow) {
                Ok(mut link) => {
                    if let Some(id) = probe_link(&mut link, Duration::from_millis(400)) {
                        return Ok((ProbeResult { baud, flow, id_answer: id }, link));
                    }
                }
                Err(e) => {
                    // Busy/missing port won't get better at another baud rate.
                    if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied)
                        || e.to_string().contains("busy")
                    {
                        return Err(e);
                    }
                    last_err = Some(e);
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "radio did not answer ID; at any baud rate")))
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
}
