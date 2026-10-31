//! Headless rig server: owns the radio's CAT port and shares it through
//! virtual serial ports and a rigctld-compatible TCP server.
//!
//!     righelmd [--port PATH] [--baud N] [--model KEY] [--no-rigctld] [--detect] [--discover]
//!
//! Uses the same config file as the GUI (~/.config/ftdx3000/config.toml).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use ftdx3000_rig::{Config, LinkState, Services, detect, remote};

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
            "--model" => {
                let key = args.next().expect("--model KEY");
                if key != "auto" && ftdx3000_cat::model::by_key(&key).is_none() {
                    eprintln!("unknown radio model \"{key}\"; known: {}", ftdx3000_cat::model::all().iter().map(|m| m.key).collect::<Vec<_>>().join(", "));
                    std::process::exit(2);
                }
                config.engine.model = if key == "auto" { String::new() } else { key };
            }
            "--baud" => config.engine.serial.baud = args.next().and_then(|b| b.parse().ok()).expect("--baud N"),
            "--no-rigctld" => config.rigctld.enabled = false,
            "--rigctld" => config.rigctld.bind = args.next().expect("--rigctld ADDR:PORT"),
            "--no-menu" => config.engine.read_menu_on_connect = false,
            "--detect" => {
                print_detect();
                return;
            }
            "--discover" => {
                print_discover();
                return;
            }
            "-h" | "--help" => {
                println!("righelmd [--port PATH] [--baud N] [--model KEY|auto] [--rigctld ADDR:PORT | --no-rigctld] [--no-menu] [--detect] [--discover]");
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
            Some(r) => log::info!("auto-detected {} CAT port {}", r.title(), r.cat.stable_path()),
            None => {
                eprintln!("no radio found; use --port PATH");
                std::process::exit(1);
            }
        }
    }
    config.auto_connect = true;
    // The GUI saves a remote radio as its target in the same file; the
    // daemon always serves the radio plugged into this computer.
    config.engine.remote = None;
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
        services.tick();
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

/// Listens for RigHelm servers on the LAN for a few seconds and lists them.
fn print_discover() {
    let discovery = match remote::Discovery::start() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("discovery failed to start: {e}");
            std::process::exit(1);
        }
    };
    println!("Looking for RigHelm servers on UDP {} ...", remote::DISCOVERY_PORT);
    std::thread::sleep(Duration::from_secs(6));
    let servers = discovery.servers();
    if servers.is_empty() {
        println!("No RigHelm servers found.");
    }
    for f in servers {
        println!(
            "{:<20} {:<22} radio={} audio={} password={}",
            f.name,
            f.addr,
            f.radio,
            if f.audio { "yes" } else { "no" },
            if f.auth { "yes" } else { "no" }
        );
    }
}

fn print_detect() {
    let radios = detect::detect_radios();
    if radios.is_empty() {
        println!("No radio found on USB (Silicon Labs, Icom or FTDI bridge).");
    }
    for p in detect::driver_problems() {
        println!("Driver problem: {}", p.text());
    }
    for r in radios {
        println!("{} on USB {} ({}, serial {})", r.title(), r.usb_path, r.bridge, r.serial_number.as_deref().unwrap_or("?"));
        println!("  CAT port            : {}  [{}]", r.cat.dev, r.cat.stable_path());
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
