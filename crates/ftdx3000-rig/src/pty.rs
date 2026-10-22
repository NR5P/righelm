//! Pseudo-terminals with stable symlinked names.
//!
//! Linux has no com0com, but it doesn't need one: a PTY pair *is* a virtual
//! null-modem cable. We keep the master side; other programs open the slave
//! side (through a stable symlink such as `/run/user/1000/ftdx3000/vcom1`)
//! exactly as if it were the radio's serial port.

use std::fs::{self, File};
use std::io::{self, Read};
use std::os::fd::AsFd;
use std::os::unix::fs::{FileTypeExt, symlink};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(target_os = "linux")]
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::pty::openpty;
use nix::sys::termios::{SetArg, cfmakeraw, tcgetattr, tcsetattr};

pub struct Pty {
    pub master: File,
    /// `/dev/pts/N`.
    pub slave_path: PathBuf,
    /// Stable symlink pointing at `slave_path`, if requested.
    pub link_path: Option<PathBuf>,
    /// Bytes read ahead by `wait` (macOS), returned first by `read`.
    ahead: Vec<u8>,
}

/// Wait until `fd` can be read (or written). macOS's poll() doesn't work on
/// terminal devices, so it uses select() there.
fn ready(fd: std::os::fd::BorrowedFd<'_>, write: bool, timeout: Duration) -> io::Result<bool> {
    #[cfg(target_os = "linux")]
    {
        let mut fds = [PollFd::new(fd, if write { PollFlags::POLLOUT } else { PollFlags::POLLIN })];
        let ms = timeout.as_millis().min(u16::MAX as u128) as u16;
        Ok(poll(&mut fds, PollTimeout::from(ms)).map_err(io::Error::from)? > 0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        use nix::sys::select::{FdSet, select};
        use nix::sys::time::TimeVal;
        let mut set = FdSet::new();
        set.insert(fd);
        let mut tv = TimeVal::new(timeout.as_secs() as _, timeout.subsec_micros() as _);
        let n = if write {
            select(None, None, Some(&mut set), None, Some(&mut tv))
        } else {
            select(None, Some(&mut set), None, None, Some(&mut tv))
        };
        Ok(n.map_err(io::Error::from)? > 0)
    }
}

/// Writes to a non-blocking PTY master, waiting at most a moment for room.
/// If the program on the other side doesn't read, data is dropped instead of
/// blocking the caller forever.
pub struct PtyWriter(pub File);

impl io::Write for PtyWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        loop {
            match self.0.write(buf) {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if !ready(self.0.as_fd(), true, Duration::from_millis(200))? {
                        return Err(io::Error::new(io::ErrorKind::TimedOut, "client is not reading"));
                    }
                }
                r => return r,
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// What `wait` saw on the master side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtyEvent {
    Readable,
    /// No program has the slave open.
    Hangup,
    Timeout,
}

impl Pty {
    /// Create a raw-mode PTY pair. The slave is closed again immediately, so
    /// `wait` reports `Hangup` until a program opens it.
    pub fn open(link_path: Option<&Path>) -> io::Result<Pty> {
        let pair = openpty(None, None).map_err(io::Error::from)?;
        let mut t = tcgetattr(&pair.slave).map_err(io::Error::from)?;
        cfmakeraw(&mut t);
        tcsetattr(&pair.slave, SetArg::TCSANOW, &t).map_err(io::Error::from)?;
        let slave_path = nix::unistd::ttyname(&pair.slave).map_err(io::Error::from)?;
        drop(pair.slave);
        // Non-blocking master: a program that stops reading must never be
        // able to block us (see `PtyWriter`).
        use nix::fcntl::{FcntlArg, OFlag, fcntl};
        let flags = OFlag::from_bits_truncate(fcntl(&pair.master, FcntlArg::F_GETFL).map_err(io::Error::from)?);
        fcntl(&pair.master, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK)).map_err(io::Error::from)?;
        let link_path = match link_path {
            Some(p) => {
                replace_symlink(p, &slave_path)?;
                Some(p.to_path_buf())
            }
            None => None,
        };
        Ok(Pty { master: File::from(pair.master), slave_path, link_path, ahead: Vec::new() })
    }

    /// Replace the PTY pair with a fresh one behind the same link.
    ///
    /// Needed after a client disconnects: if it had put the slave in
    /// exclusive mode (`TIOCEXCL`, which e.g. the Rust `serialport` crate
    /// does by default), Linux keeps that flag while our master stays open
    /// and every later open fails with EBUSY.
    pub fn renew(&mut self) -> io::Result<()> {
        let fresh = Pty::open(None)?;
        if let Some(link) = &self.link_path {
            replace_symlink(link, &fresh.slave_path)?;
        }
        // `fresh` has no link, so dropping it later removes nothing.
        self.master = fresh.master.try_clone()?;
        self.slave_path = fresh.slave_path.clone();
        self.ahead.clear();
        Ok(())
    }

    /// Wait for input from the program on the other side.
    pub fn wait(&mut self, timeout: Duration) -> io::Result<PtyEvent> {
        if !self.ahead.is_empty() {
            return Ok(PtyEvent::Readable);
        }
        #[cfg(target_os = "linux")]
        {
            let mut fds = [PollFd::new(self.master.as_fd(), PollFlags::POLLIN)];
            let ms = timeout.as_millis().min(u16::MAX as u128) as u16;
            let n = poll(&mut fds, PollTimeout::from(ms)).map_err(io::Error::from)?;
            if n == 0 {
                return Ok(PtyEvent::Timeout);
            }
            let ev = fds[0].revents().unwrap_or(PollFlags::empty());
            if ev.contains(PollFlags::POLLIN) {
                Ok(PtyEvent::Readable)
            } else if ev.intersects(PollFlags::POLLHUP | PollFlags::POLLERR) {
                Ok(PtyEvent::Hangup)
            } else {
                Ok(PtyEvent::Timeout)
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            // select() reports a closed slave as readable; a read tells
            // data (keep it for `read`) from hang-up (0 bytes or EIO).
            if !ready(self.master.as_fd(), false, timeout)? {
                return Ok(PtyEvent::Timeout);
            }
            let mut buf = [0u8; 512];
            match self.master.read(&mut buf) {
                Ok(0) => Ok(PtyEvent::Hangup),
                Ok(n) => {
                    self.ahead.extend_from_slice(&buf[..n]);
                    Ok(PtyEvent::Readable)
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(PtyEvent::Timeout),
                Err(_) => Ok(PtyEvent::Hangup),
            }
        }
    }

    /// A handle for answers and auto-information.
    pub fn writer(&self) -> io::Result<Box<dyn io::Write + Send>> {
        Ok(Box::new(PtyWriter(self.master.try_clone()?)))
    }

    /// Read what the program on the other side sent.
    pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.ahead.is_empty() {
            let n = buf.len().min(self.ahead.len());
            buf[..n].copy_from_slice(&self.ahead[..n]);
            self.ahead.drain(..n);
            return Ok(n);
        }
        self.master.read(buf)
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        if let Some(link) = &self.link_path {
            // Only remove it if it still points at our slave.
            if fs::read_link(link).ok().as_deref() == Some(self.slave_path.as_path()) {
                let _ = fs::remove_file(link);
            }
        }
    }
}

/// Point `link` at `target`, replacing an existing symlink (never a regular
/// file). The swap is atomic (rename), so a program opening the link never
/// finds it missing.
pub fn replace_symlink(link: &Path, target: &Path) -> io::Result<()> {
    if let Some(dir) = link.parent() {
        fs::create_dir_all(dir)?;
    }
    match fs::symlink_metadata(link) {
        Ok(m) if m.file_type().is_symlink() => {}
        Ok(m) if m.file_type().is_char_device() => {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{} is a real device", link.display())));
        }
        Ok(_) => {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{} exists and is not a symlink", link.display())));
        }
        Err(_) => {}
    }
    let name = link.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tmp = link.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let _ = fs::remove_file(&tmp);
    symlink(target, &tmp)?;
    fs::rename(&tmp, link)
}

pub use crate::services::default_link_dir;


/// Wine prefixes found on this machine (directories containing `dosdevices`).
pub fn find_wine_prefixes() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else { return out };
    if let Some(p) = std::env::var_os("WINEPREFIX").map(PathBuf::from) {
        out.push(p);
    }
    fn walk(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
        if dir.join("dosdevices").is_dir() && !out.iter().any(|p| p == dir) {
            out.push(dir.to_path_buf());
            return;
        }
        if depth == 0 {
            return;
        }
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                && !matches!(name.as_ref(), "node_modules" | "target" | ".cache" | ".cargo" | ".rustup" | "snap")
            {
                walk(&e.path(), depth - 1, out);
            }
        }
    }
    walk(&home.join(".wine"), 0, &mut out);
    walk(&home.join(".local/share/wineprefixes"), 1, &mut out);
    walk(&home, 3, &mut out);
    out
}

/// Map `COMn` in a Wine prefix to `target` (e.g. one of our virtual ports).
pub fn map_wine_com(prefix: &Path, com: u8, target: &Path) -> io::Result<PathBuf> {
    let link = prefix.join("dosdevices").join(format!("com{com}"));
    replace_symlink(&link, target)?;
    Ok(link)
}

/// Current Wine COM mappings in a prefix: `(n, target)`.
pub fn wine_com_ports(prefix: &Path) -> Vec<(u8, PathBuf)> {
    let mut out: Vec<(u8, PathBuf)> = fs::read_dir(prefix.join("dosdevices"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            let n = name.strip_prefix("com")?.parse().ok()?;
            Some((n, fs::read_link(e.path()).ok()?))
        })
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn pty_round_trip_and_hangup() {
        let dir = std::env::temp_dir().join(format!("ftdx-pty-test-{}", std::process::id()));
        let link = dir.join("vcom-test");
        let mut pty = Pty::open(Some(&link)).unwrap();
        assert_eq!(fs::read_link(&link).unwrap(), pty.slave_path);
        assert_eq!(pty.wait(Duration::from_millis(10)).unwrap(), PtyEvent::Hangup);

        let mut client = fs::OpenOptions::new().read(true).write(true).open(&link).unwrap();
        client.write_all(b"FA;").unwrap();
        assert_eq!(pty.wait(Duration::from_secs(1)).unwrap(), PtyEvent::Readable);
        let mut buf = [0u8; 16];
        let n = pty.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"FA;");
        pty.master.write_all(b"FA14074000;").unwrap();
        let n = client.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"FA14074000;");
        drop(client);
        assert_eq!(pty.wait(Duration::from_millis(100)).unwrap(), PtyEvent::Hangup);
        drop(pty);
        assert!(fs::symlink_metadata(&link).is_err(), "link removed on drop");
        let _ = fs::remove_dir_all(dir);
    }
}
