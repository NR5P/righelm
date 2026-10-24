//! Run the built-in simulator on a PTY, so any program (including this
//! project's own app, or Hamlib) can be tested without a radio.
//!
//!     righelm-sim [--model KEY] [LINK_PATH]   (default: $XDG_RUNTIME_DIR/ftdx3000/sim)
//!     righelm-sim --list                      (the models it can be)
//!
//! An FTDX3000 by default; with `--model` (e.g. `ic7300`, `ft991a`) that
//! radio, speaking its own protocol.

#[cfg(not(unix))]
fn main() {
    eprintln!("righelm-sim needs a Unix pseudo-terminal (Linux or macOS).");
    std::process::exit(1);
}

#[cfg(unix)]
fn main() -> std::io::Result<()> {
    unix::main()
}

#[cfg(unix)]
mod unix {
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use std::sync::Arc;

use ftdx3000_rig::pty::{Pty, PtyEvent, default_link_dir};
use ftdx3000_rig::sim::{SimCore, SimLine};
use parking_lot::Mutex;

static STOP: AtomicBool = AtomicBool::new(false);

pub fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut model = &ftdx3000_cat::model::FTDX3000;
    let mut link = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--list" => {
                for m in ftdx3000_cat::model::all() {
                    println!("{:12} {}", m.key, m.full_name());
                }
                return Ok(());
            }
            "--model" => {
                let key = args.next().unwrap_or_default();
                let Some(m) = ftdx3000_cat::model::by_key(&key) else {
                    eprintln!("unknown model \"{key}\" (righelm-sim --list shows them)");
                    std::process::exit(2);
                };
                model = m;
            }
            _ => link = Some(std::path::PathBuf::from(a)),
        }
    }
    let link = link.unwrap_or_else(|| default_link_dir().join("sim"));
    install_signal_handlers();
    let mut pty = Pty::open(Some(&link))?;
    println!("{} simulator on {} -> {}", model.full_name(), link.display(), pty.slave_path.display());
    println!("Press Ctrl-C to stop.");
    let core = Arc::new(Mutex::new(SimCore::for_model(model)));
    let mut line = SimLine::new(core.clone());
    let mut buf = [0u8; 512];
    let mut active = false;
    while !STOP.load(Ordering::Relaxed) {
        match pty.wait(Duration::from_millis(200))? {
            PtyEvent::Readable => match pty.read(&mut buf) {
                Ok(n) if n > 0 => {
                    active = true;
                    let out = line.handle(&buf[..n]);
                    let _ = pty.master.write_all(&out);
                }
                _ => {}
            },
            PtyEvent::Hangup => {
                if active {
                    // Fresh PTY so an exclusive-mode client can't lock us out.
                    active = false;
                    line = SimLine::new(core.clone());
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
    let _ = ctrlc::set_handler(|| STOP.store(true, Ordering::Relaxed));
}
}
