//! Windows virtual ports: one end of a com0com virtual null-modem pair.
//!
//! Windows has no PTYs. With com0com installed, RigHelm opens one port of a
//! pair (say COM10) and other programs open its partner (COM11), exactly as
//! Win4Yaesu does. The pair carries no reliable "other side closed" signal,
//! so unlike the Unix PTY ports, a client vanishing is not detected.

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::Duration;

/// What `wait` saw (same shape as the Unix PTY's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtyEvent {
    Readable,
    /// Never reported on Windows (see module docs).
    Hangup,
    Timeout,
}

pub struct ComEnd {
    port: Box<dyn serialport::SerialPort>,
    ahead: Vec<u8>,
    /// Shown in the UI: where other programs connect.
    pub slave_path: PathBuf,
}

impl ComEnd {
    pub fn open(com: &str) -> io::Result<ComEnd> {
        if com.trim().is_empty() {
            return Err(io::Error::other(
                "set the com0com port RigHelm should open (e.g. COM10); other programs use its partner (e.g. COM11)",
            ));
        }
        let port = serialport::new(com.trim(), 38400).timeout(Duration::from_millis(50)).open().map_err(io::Error::other)?;
        Ok(ComEnd { port, ahead: Vec::new(), slave_path: PathBuf::from(format!("the com0com partner of {}", com.trim())) })
    }

    pub fn wait(&mut self, timeout: Duration) -> io::Result<PtyEvent> {
        if !self.ahead.is_empty() {
            return Ok(PtyEvent::Readable);
        }
        let _ = self.port.set_timeout(timeout.clamp(Duration::from_millis(1), Duration::from_millis(200)));
        let mut buf = [0u8; 512];
        match self.port.read(&mut buf) {
            Ok(n) if n > 0 => {
                self.ahead.extend_from_slice(&buf[..n]);
                Ok(PtyEvent::Readable)
            }
            Ok(_) => Ok(PtyEvent::Timeout),
            Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock) => Ok(PtyEvent::Timeout),
            Err(e) => Err(e),
        }
    }

    pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.ahead.is_empty() {
            let n = buf.len().min(self.ahead.len());
            buf[..n].copy_from_slice(&self.ahead[..n]);
            self.ahead.drain(..n);
            return Ok(n);
        }
        self.port.read(buf)
    }

    /// A handle for answers and auto-information (writes time out if the
    /// other program doesn't read).
    pub fn writer(&self) -> io::Result<Box<dyn Write + Send>> {
        Ok(self.port.try_clone().map_err(io::Error::other)?)
    }

    /// Nothing to renew: the com0com pair stays as it is.
    pub fn renew(&mut self) -> io::Result<()> {
        Ok(())
    }
}
