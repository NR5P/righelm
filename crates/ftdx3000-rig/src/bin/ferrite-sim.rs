//! Run the built-in FTDX3000 simulator on a PTY, so any program (including
//! this project's own app) can be tested without a radio.
//!
//!     ferrite-sim [LINK_PATH]     (default: $XDG_RUNTIME_DIR/ftdx3000/sim)

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use ftdx3000_cat::Framer;
use ftdx3000_rig::pty::{Pty, PtyEvent, default_link_dir};
use ftdx3000_rig::sim::SimCore;

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: nix::libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

fn main() -> std::io::Result<()> {
    let link = std::env::args().nth(1).map(Into::into).unwrap_or_else(|| default_link_dir().join("sim"));
    install_signal_handlers();
    let mut pty = Pty::open(Some(&link))?;
    println!("FTDX3000 simulator on {} -> {}", link.display(), pty.slave_path.display());
    println!("Press Ctrl-C to stop.");
    let mut core = SimCore::new();
    let mut framer = Framer::new();
    let mut buf = [0u8; 512];
    let mut active = false;
    while !STOP.load(Ordering::Relaxed) {
        match pty.wait(Duration::from_millis(200))? {
            PtyEvent::Readable => match pty.master.read(&mut buf) {
                Ok(n) if n > 0 => {
                    active = true;
                    let mut out = String::new();
                    for f in framer.push(&buf[..n]) {
                        for a in core.handle(&f) {
                            out.push_str(&a);
                        }
                    }
                    for a in core.outbox.drain(..) {
                        out.push_str(&a);
                    }
                    let _ = pty.master.write_all(out.as_bytes());
                }
                _ => {}
            },
            PtyEvent::Hangup => {
                if active {
                    // Fresh PTY so an exclusive-mode client can't lock us out.
                    active = false;
                    framer.clear();
                    pty.renew()?;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            PtyEvent::Timeout => {}
        }
    }
    println!("simulator stopped");
    Ok(())
}

fn install_signal_handlers() {
    use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, Signal, sigaction};
    let action = SigAction::new(SigHandler::Handler(on_signal), SaFlags::empty(), SigSet::empty());
    for sig in [Signal::SIGINT, Signal::SIGTERM, Signal::SIGHUP] {
        // SAFETY: the handler only stores to an atomic.
        unsafe {
            let _ = sigaction(sig, &action);
        }
    }
}
