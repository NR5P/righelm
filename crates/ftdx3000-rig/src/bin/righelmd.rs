//! Headless rig server: owns the radio's CAT port and shares it through
//! virtual serial ports and a rigctld-compatible TCP server.
//!
//!     righelmd [--port PATH] [--baud N] [--no-rigctld] [--detect]
//!
//! Uses the same config file as the GUI (~/.config/ftdx3000/config.toml).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use ftdx3000_rig::{Config, LinkState, Services, detect};

static STOP: AtomicBool = AtomicBool::new(false);

fn install_signal_handlers() {
    // Ctrl-C, SIGTERM and SIGHUP (or the Windows equivalents): shut down cleanly.
    let _ = ctrlc::set_handler(|| STOP.store(true, Ordering::Relaxed));
}

fn main() {
    install_signal_handlers();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let mut config = Config::load();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--port" => config.engine.serial.path = args.next().expect("--port PATH"),
            "--baud" => config.engine.serial.baud = args.next().and_then(|b| b.parse().ok()).expect("--baud N"),
            "--no-rigctld" => config.rigctld.enabled = false,
            "--rigctld" => config.rigctld.bind = args.next().expect("--rigctld ADDR:PORT"),
            "--no-menu" => config.engine.read_menu_on_connect = false,
            "--detect" => {
                print_detect();
                return;
            }
            "-h" | "--help" => {
                println!("righelmd [--port PATH] [--baud N] [--rigctld ADDR:PORT | --no-rigctld] [--no-menu] [--detect]");
                return;
            }
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    if config.engine.serial.path.is_empty() {
        match config.autodetect_port() {
            Some(r) => log::info!("auto-detected FTDX3000 CAT port {}", r.cat.stable_path()),
            None => {
                eprintln!("no FTDX3000 found; use --port PATH");
                std::process::exit(1);
            }
        }
    }
    config.auto_connect = true;
    let mut services = Services::start(config);
    for (cfg, st) in services.port_status() {
        match st {
            Ok(s) => println!("virtual port {:<6} {:<12} {}", cfg.name, cfg.label, s.link_path.display()),
            Err(e) => println!("virtual port {} failed: {e}", cfg.name),
        }
    }
    if let Some(Ok((addr, _))) = services.rigctld_status() {
        println!("rigctld (Hamlib NET rigctl, model 2) on {addr}");
    }
    let mut last = LinkState::Disconnected;
    while !STOP.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(200));
        let link = services.rig.link();
        if link != last {
            println!("link: {link:?}");
            last = link;
        }
    }
    // Un-keys the radio if we keyed it, turns AI off, removes the port links.
    println!("shutting down");
    services.shutdown();
}

fn print_detect() {
    let radios = detect::detect_radios();
    if radios.is_empty() {
        println!("No FTDX3000 (CP2105 USB bridge) found.");
    }
    for r in radios {
        println!("FTDX3000 on USB {} (serial {})", r.usb_path, r.serial_number.as_deref().unwrap_or("?"));
        println!("  CAT (Enhanced) port : {}  [{}]", r.cat.dev, r.cat.stable_path());
        for u in &r.cat.users {
            println!("      in use by {} (pid {})", u.name, u.pid);
        }
        if let Some(k) = &r.keying {
            println!("  Keying (Standard)   : {}  [{}]", k.dev, k.stable_path());
            for u in &k.users {
                println!("      in use by {} (pid {})", u.name, u.pid);
            }
        }
        match &r.audio {
            Some(a) => {
                println!("  Audio               : card {} \"{}\" ({})", a.card_index, a.card_id, a.name);
                println!("      ALSA            : {} / {}", a.alsa_hw(), a.alsa_plughw());
                if let Some((n, d)) = &a.source {
                    println!("      RX audio source : {n}  ({d})");
                }
                if let Some((n, d)) = &a.sink {
                    println!("      TX audio sink   : {n}  ({d})");
                }
            }
            None => println!("  Audio               : not found"),
        }
    }
    println!();
    println!("All USB serial ports:");
    for p in detect::list_serial_ports() {
        println!("  {:<14} {}", p.port.dev, p.description);
    }
}
