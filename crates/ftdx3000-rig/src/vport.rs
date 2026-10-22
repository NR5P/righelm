//! Virtual CAT ports: the Linux answer to Win4Yaesu's "multi port" feature.
//!
//! Each virtual port is a PTY that behaves like an FTDX3000 to whatever
//! program opens it (WSJT-X, fldigi, a logger, a Wine program via a COM
//! mapping, ...). Commands are multiplexed onto the single real CAT port by
//! the engine:
//!
//! * Read commands are answered from the engine's cache when it is fresh, so
//!   five programs polling `FA;` cost the radio one read.
//! * Set commands are forwarded in order.
//! * `AI` (auto information) is emulated per client: a program that turns AI
//!   on gets pushed updates without changing the radio's AI for everyone else.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use ftdx3000_cat::command::{self, Request};
use ftdx3000_cat::{Framer, frame};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::engine::{CatError, Freshness, Rig, Source};
#[cfg(windows)]
use crate::comport::{ComEnd as End, PtyEvent};
#[cfg(unix)]
use crate::pty::{Pty as End, PtyEvent};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VirtualPortConfig {
    /// Short name, also the file name of the link (`vcom1`).
    pub name: String,
    /// What it's for, shown in the UI ("WSJT-X").
    pub label: String,
    pub enabled: bool,
    /// Ignore TX/MOX commands from this client.
    pub block_tx: bool,
    /// Windows only: the com0com port RigHelm opens (programs use its partner).
    pub com_port: String,
}

impl Default for VirtualPortConfig {
    fn default() -> Self {
        Self { name: "vcom1".into(), label: String::new(), enabled: true, block_tx: false, com_port: String::new() }
    }
}

#[derive(Debug, Clone, Default)]
pub struct VirtualPortStatus {
    pub link_path: PathBuf,
    pub slave_path: PathBuf,
    pub client_connected: bool,
    pub client_ai: bool,
    pub frames_in: u64,
    pub frames_out: u64,
    pub last_activity: Option<Instant>,
    pub last_command: String,
    pub error: Option<String>,
}

/// A running virtual port. Dropping it stops the port and removes the link.
pub struct VirtualPort {
    pub config: VirtualPortConfig,
    status: Arc<Mutex<VirtualPortStatus>>,
    stop: Arc<AtomicBool>,
    threads: Vec<thread::JoinHandle<()>>,
}

impl VirtualPort {
    pub fn start(rig: Rig, config: VirtualPortConfig, link_dir: PathBuf) -> std::io::Result<VirtualPort> {
        #[cfg(unix)]
        let (pty, link_path) = {
            let link = link_dir.join(&config.name);
            (End::open(Some(&link))?, link)
        };
        #[cfg(windows)]
        let (pty, link_path) = {
            let _ = &link_dir;
            (End::open(&config.com_port)?, PathBuf::from(config.com_port.trim()))
        };
        let status = Arc::new(Mutex::new(VirtualPortStatus {
            link_path: link_path.clone(),
            slave_path: pty.slave_path.clone(),
            ..Default::default()
        }));
        rig.note(format!("virtual port {} -> {}", link_path.display(), pty.slave_path.display()));
        let stop = Arc::new(AtomicBool::new(false));
        let writer: SharedWriter = Arc::new(Mutex::new(pty.writer()?));
        let client = Arc::new(ClientCtx::default());
        let connected = Arc::new(AtomicBool::new(false));

        let pusher = ai_pusher(rig.clone(), writer.clone(), client.clone(), connected.clone(), stop.clone(), {
            let status = status.clone();
            move || status.lock().frames_out += 1
        })?;

        let server = {
            let (stop, status) = (stop.clone(), status.clone());
            let cfg = config.clone();
            thread::Builder::new()
                .name(format!("vport-{}", config.name))
                .spawn(move || serve(rig, cfg, pty, writer, client, connected, status, stop))?
        };

        Ok(VirtualPort { config, status, stop, threads: vec![pusher, server] })
    }

    pub fn status(&self) -> VirtualPortStatus {
        self.status.lock().clone()
    }
}

impl Drop for VirtualPort {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

/// Per-client state shared by the command handler and the AI pusher.
#[derive(Debug, Default)]
pub(crate) struct ClientCtx {
    /// The client turned auto-information on.
    pub ai: AtomicBool,
    /// The client keyed the transmitter (TX1/MX1) and hasn't released it.
    pub keyed: AtomicBool,
    /// The client turned VOX on (any audio could then key the radio).
    pub vox: AtomicBool,
}

impl ClientCtx {
    /// Track what an allowed Set leaves switched on, for `disconnected`.
    pub fn observe_set(&self, frame: &str) {
        let Some((code, p)) = ftdx3000_cat::frame::split(frame) else { return };
        match (code.as_str(), p) {
            ("TX" | "MX", "0") => self.keyed.store(false, Ordering::Relaxed),
            ("TX" | "MX", p) if !p.is_empty() => self.keyed.store(true, Ordering::Relaxed),
            ("VX", "0" | "1") => self.vox.store(p == "1", Ordering::Relaxed),
            _ => {}
        }
    }

    /// The client is gone: reset, and release the transmitter if it held it.
    pub fn disconnected(&self, rig: &Rig, source: &Source) {
        self.ai.store(false, Ordering::Relaxed);
        if self.keyed.swap(false, Ordering::Relaxed) {
            rig.send_from("TX0;MX0;", source.clone());
            rig.note(format!("{source} went away while transmitting: un-keyed the radio"));
        }
        if self.vox.swap(false, Ordering::Relaxed) {
            rig.send_from("VX0;", source.clone());
            rig.note(format!("{source} went away with VOX on: turned VOX off"));
        }
    }
}

/// Where answers and auto-information for one client are written.
pub(crate) type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;

/// Push changed auto-information frames to a client that turned AI on.
pub(crate) fn ai_pusher(
    rig: Rig,
    writer: SharedWriter,
    client: Arc<ClientCtx>,
    connected: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    on_sent: impl Fn() + Send + 'static,
) -> std::io::Result<thread::JoinHandle<()>> {
    let updates = rig.subscribe();
    thread::Builder::new().name("ai-pusher".into()).spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let Ok(u) = updates.recv_timeout(Duration::from_millis(300)) else { continue };
            if !(u.changed && !u.first && client.ai.load(Ordering::Relaxed) && connected.load(Ordering::Relaxed)) {
                continue;
            }
            // Meters change on every poll; pushing them would flood the client.
            let is_ai = frame::split(&u.frame)
                .filter(|(c, _)| !matches!(c.as_str(), "SM" | "RM" | "BY"))
                .and_then(|(c, _)| command::lookup(&c))
                .is_some_and(|c| c.ai);
            if is_ai && writer.lock().write_all(u.frame.as_bytes()).is_ok() {
                on_sent();
            }
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn serve(
    rig: Rig,
    config: VirtualPortConfig,
    mut pty: End,
    writer: SharedWriter,
    client: Arc<ClientCtx>,
    connected: Arc<AtomicBool>,
    status: Arc<Mutex<VirtualPortStatus>>,
    stop: Arc<AtomicBool>,
) {
    let source = Source::Client(if config.label.is_empty() { config.name.clone() } else { config.label.clone() });
    let mut framer = Framer::new();
    let mut buf = [0u8; 512];
    let mark_open = || {
        if !connected.swap(true, Ordering::Relaxed) {
            status.lock().client_connected = true;
        }
    };
    // The slave was closed. If someone had it open, clean up after them and
    // give the next program a fresh PTY (clears any exclusive-mode lock,
    // even from a program that opened the port and never wrote to it).
    let hangup = |pty: &mut End, framer: &mut Framer| {
        if !connected.swap(false, Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(100));
            return;
        }
        client.disconnected(&rig, &source);
        framer.clear();
        {
            let mut s = status.lock();
            s.client_connected = false;
            s.client_ai = false;
        }
        // A program that closed and immediately reopened the port is using
        // this PTY again: keep it.
        if !matches!(pty.wait(Duration::ZERO), Ok(PtyEvent::Hangup)) {
            mark_open();
            return;
        }
        match pty.renew().and_then(|_| pty.writer()) {
            Ok(w) => {
                *writer.lock() = w;
                status.lock().slave_path = pty.slave_path.clone();
            }
            Err(e) => status.lock().error = Some(format!("renewing port: {e}")),
        }
    };
    while !stop.load(Ordering::Relaxed) {
        match pty.wait(Duration::from_millis(200)) {
            // No hang-up means a program has the port open, even if idle.
            Ok(PtyEvent::Timeout) => {
                mark_open();
                continue;
            }
            Ok(PtyEvent::Hangup) => {
                hangup(&mut pty, &mut framer);
                continue;
            }
            Ok(PtyEvent::Readable) => {}
            Err(e) => {
                status.lock().error = Some(e.to_string());
                thread::sleep(Duration::from_millis(200));
                continue;
            }
        }
        let n = match pty.read(&mut buf) {
            Ok(n) if n > 0 => n,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            _ => {
                // 0 bytes or EIO: the last program closed the port.
                hangup(&mut pty, &mut framer);
                continue;
            }
        };
        mark_open();
        for f in framer.push(&buf[..n]) {
            let reply = handle(&rig, &config, &source, &client, &f);
            {
                let mut s = status.lock();
                s.frames_in += 1;
                s.last_activity = Some(Instant::now());
                s.last_command = f.clone();
                s.client_ai = client.ai.load(Ordering::Relaxed);
                if reply.is_some() {
                    s.frames_out += 1;
                }
            }
            if let Some(r) = reply {
                // A client that doesn't read just loses the answer.
                let _ = writer.lock().write_all(r.as_bytes());
            }
        }
    }
    client.disconnected(&rig, &source);
}

/// Handle one frame from a client; returns the answer to send back, if any.
pub(crate) fn handle(rig: &Rig, config: &VirtualPortConfig, source: &Source, client: &ClientCtx, f: &str) -> Option<String> {
    let f = command::normalize(f);
    let (code, params) = frame::split(&f)?;
    match command::classify(&f) {
        Request::Read { .. } if code == "AI" => Some(format!("AI{};", client.ai.load(Ordering::Relaxed) as u8)),
        Request::Read { .. } => match rig.read(&f, Freshness::Auto, source.clone(), Duration::from_millis(1500)) {
            Ok(answer) => Some(answer),
            Err(CatError::Rejected) => Some("?;".into()),
            // A radio that doesn't answer is emulated by not answering.
            Err(_) => None,
        },
        Request::Set if code == "AI" => {
            match params {
                "0" | "1" => client.ai.store(params == "1", Ordering::Relaxed),
                _ => return Some("?;".into()),
            }
            None
        }
        Request::Set => {
            if command::is_tx_command(&f) && config.block_tx {
                rig.note(format!("{source}: blocked {f} (transmit not allowed)"));
                return None;
            }
            client.observe_set(&f);
            rig.send_from(&f, source.clone());
            None
        }
        Request::Unknown => Some("?;".into()),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Read;
    use crate::engine::EngineConfig;
    use crate::sim::SimRadio;
    use std::fs::OpenOptions;

    /// Read until `needle` shows up (other auto-information may come first).
    fn wait_for(port: &mut std::fs::File, needle: &str) -> bool {
        let mut got = String::new();
        let mut buf = [0u8; 256];
        let t0 = Instant::now();
        while !got.contains(needle) && t0.elapsed() < Duration::from_secs(3) {
            if let Ok(n) = port.read(&mut buf) {
                got.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        }
        got.contains(needle)
    }

    fn read_answer(port: &mut std::fs::File, expect_end: usize) -> String {
        let mut got = String::new();
        let mut buf = [0u8; 64];
        let t0 = Instant::now();
        while got.matches(';').count() < expect_end && t0.elapsed() < Duration::from_secs(3) {
            if let Ok(n) = port.read(&mut buf) {
                got.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        }
        got
    }

    #[test]
    fn two_clients_share_one_radio() {
        let sim = SimRadio::new();
        let rig = Rig::start();
        rig.connect_link(sim.link(), EngineConfig { read_menu_on_connect: false, ..Default::default() });
        let dir = std::env::temp_dir().join(format!("ftdx-vport-test-{}", std::process::id()));
        let p1 = VirtualPort::start(rig.clone(), VirtualPortConfig { name: "a".into(), ..Default::default() }, dir.clone()).unwrap();
        let p2 = VirtualPort::start(
            rig.clone(),
            VirtualPortConfig { name: "b".into(), block_tx: true, ..Default::default() },
            dir.clone(),
        )
        .unwrap();
        let mut c1 = OpenOptions::new().read(true).write(true).open(dir.join("a")).unwrap();
        let mut c2 = OpenOptions::new().read(true).write(true).open(dir.join("b")).unwrap();

        c1.write_all(b"ID;").unwrap();
        assert_eq!(read_answer(&mut c1, 1), "ID0462;");
        c2.write_all(b"FA;MD0;").unwrap();
        assert_eq!(read_answer(&mut c2, 2), "FA14074000;MD02;");

        // Client 1 tunes; client 2 sees it.
        c1.write_all(b"FA07074000;").unwrap();
        thread::sleep(Duration::from_millis(100));
        c2.write_all(b"FA;").unwrap();
        assert_eq!(read_answer(&mut c2, 1), "FA07074000;");

        // Per-client auto information.
        c2.write_all(b"AI1;AI;").unwrap();
        assert_eq!(read_answer(&mut c2, 1), "AI1;");
        c1.write_all(b"AI;").unwrap();
        assert_eq!(read_answer(&mut c1, 1), "AI0;");
        c1.write_all(b"FA10136000;").unwrap();
        assert!(wait_for(&mut c2, "FA10136000;"));
        c2.write_all(b"AI0;").unwrap();

        // TX blocked on port b, allowed on port a.
        c2.write_all(b"TX1;").unwrap();
        thread::sleep(Duration::from_millis(100));
        assert_eq!(sim.core().tx, 0);
        c1.write_all(b"TX1;").unwrap();
        thread::sleep(Duration::from_millis(150));
        assert_eq!(sim.core().tx, 1);
        c1.write_all(b"TX0;").unwrap();

        c1.write_all(b"XX;").unwrap();
        assert_eq!(read_answer(&mut c1, 1), "?;");

        // Every way of transmitting is blocked on a no-TX port.
        c2.write_all(b"AC002;KY1;PB01;MX1;").unwrap();
        thread::sleep(Duration::from_millis(150));
        assert_eq!(sim.core().tx, 0);
        assert!(!sim.core().mox);

        // A client that vanishes while transmitting gets un-keyed.
        c1.write_all(b"TX1;").unwrap();
        thread::sleep(Duration::from_millis(150));
        assert_eq!(sim.core().tx, 1);
        drop(c1);
        let t0 = Instant::now();
        while sim.core().tx != 0 && t0.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(sim.core().tx, 0, "radio left transmitting after the client disappeared");
        thread::sleep(Duration::from_millis(300));
        let c1 = OpenOptions::new().read(true).write(true).open(dir.join("a")).unwrap();
        // An open-but-idle client counts as connected (seen at the next poll).
        let t0 = Instant::now();
        while !p1.status().client_connected && t0.elapsed() < Duration::from_secs(1) {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(p1.status().client_connected);

        // A client that locks the port exclusively, then closes it, must not
        // lock out the next client.
        drop(c1);
        {
            let excl = OpenOptions::new().read(true).write(true).open(dir.join("a")).unwrap();
            nix::ioctl_none_bad!(tiocexcl, nix::libc::TIOCEXCL);
            unsafe { tiocexcl(std::os::fd::AsRawFd::as_raw_fd(&excl)) }.unwrap();
        }
        let t0 = Instant::now();
        let mut reopened = None;
        while t0.elapsed() < Duration::from_secs(3) {
            if let Ok(f) = OpenOptions::new().read(true).write(true).open(dir.join("a")) {
                reopened = Some(f);
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let mut c1 = reopened.expect("port usable again after an exclusive client closed it");
        c1.write_all(b"ID;").unwrap();
        assert_eq!(read_answer(&mut c1, 1), "ID0462;");
        drop((c1, c2, p1, p2));
        rig.shutdown();
        let _ = std::fs::remove_dir_all(dir);
    }
}
