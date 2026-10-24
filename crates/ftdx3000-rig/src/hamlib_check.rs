//! Check a simulated radio against Hamlib's own driver for it.
//!
//! Hamlib's drivers were written and fixed by people with the real radios.
//! If `rigctl -m <model>` gets sensible answers from RigHelm's simulator of
//! that radio, the simulator speaks the radio's protocol, and since the
//! simulator speaks it through the same translator RigHelm uses on the real
//! radio, the translator is right too.
//!
//! Needs Hamlib's `rigctl` on the PATH; [`rigctl`] returns `None` without it
//! so tests can skip.

use std::io::{self, Write};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use ftdx3000_cat::RadioModel;
use parking_lot::Mutex;

use crate::pty::{Pty, PtyEvent};
use crate::sim::{SimCore, SimLine};

/// Is Hamlib's `rigctl` installed?
pub fn available() -> bool {
    Command::new("rigctl").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// A simulated `model` on a PTY, until dropped.
pub struct SimPort {
    pub path: std::path::PathBuf,
    pub core: Arc<Mutex<SimCore>>,
    /// Everything the program sent, as text (for failure messages).
    pub heard: Arc<Mutex<Vec<u8>>>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl SimPort {
    pub fn start(model: &'static RadioModel) -> io::Result<SimPort> {
        let mut pty = Pty::open(None)?;
        let path = pty.slave_path.clone();
        let core = Arc::new(Mutex::new(SimCore::for_model(model)));
        let stop = Arc::new(AtomicBool::new(false));
        let heard = Arc::new(Mutex::new(Vec::new()));
        let (core2, stop2, heard2) = (core.clone(), stop.clone(), heard.clone());
        let thread = thread::spawn(move || {
            let mut line = SimLine::new(core2.clone());
            let mut buf = [0u8; 512];
            while !stop2.load(Ordering::Relaxed) {
                match pty.wait(Duration::from_millis(50)) {
                    Ok(PtyEvent::Readable) => {
                        if let Ok(n) = pty.read(&mut buf) {
                            if n > 0 {
                                heard2.lock().extend_from_slice(&buf[..n]);
                                let out = line.handle(&buf[..n]);
                                let _ = pty.master.write_all(&out);
                            }
                        }
                    }
                    Ok(PtyEvent::Hangup) => {
                        // Between rigctl runs: a fresh line, same radio.
                        line = SimLine::new(core2.clone());
                        thread::sleep(Duration::from_millis(20));
                    }
                    Ok(PtyEvent::Timeout) => {}
                    Err(_) => break,
                }
            }
        });
        Ok(SimPort { path, core, heard, stop, thread: Some(thread) })
    }

    pub fn heard_text(&self) -> String {
        String::from_utf8_lossy(&self.heard.lock()).to_string()
    }
}

impl Drop for SimPort {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Run `rigctl` with Hamlib's driver for `model` against `port`, with these
/// commands (`["f", "F 7074000", "m"]`). Returns rigctl's output (stdout,
/// then stderr) or `None` if rigctl isn't installed.
pub fn rigctl(model: &'static RadioModel, port: &SimPort, cmds: &[&str]) -> Option<String> {
    if !available() {
        return None;
    }
    let mut c = Command::new("rigctl");
    c.arg("-m").arg(model.hamlib.to_string()).arg("-r").arg(&port.path);
    if let Some(&b) = model.bauds.first() {
        c.arg("-s").arg(b.to_string());
    }
    // A PTY has no handshake lines.
    if model.rts_cts {
        c.arg("--set-conf=serial_handshake=None");
    }
    for cmd in cmds {
        for w in cmd.split_whitespace() {
            c.arg(w);
        }
    }
    let out = c.output().ok()?;
    Some(format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

/// Answer lines of a rigctl run, without its warnings and debug chatter.
pub fn answers(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| !l.contains("rig_") && !l.starts_with("Warning") && !l.contains(": ") || l.starts_with("RPRT"))
        .map(String::from)
        .collect()
}
