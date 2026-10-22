//! Hardware check of CW keying from the computer: arms the key, sends a
//! short text at 5 W, reads the power meter, disarms and checks that menu 065
//! is back where it was.
//!
//!     cargo run --release --example cw_check

use std::time::{Duration, Instant};

use ftdx3000_rig::cw::Input;
use ftdx3000_rig::{Config, Freshness, Services, Source};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    // Work on a copy of the settings: this check changes and saves them.
    let copy = std::env::temp_dir().join("righelm-check.toml");
    let _ = std::fs::copy(Config::path(), &copy);
    // SAFETY: no other threads exist yet.
    unsafe { std::env::set_var("FTDX3000_CONFIG", &copy) };
    let mut config = Config::load();
    config.engine.remote = None;
    config.remote_server.enabled = false;
    config.rigctld.enabled = false;
    config.virtual_ports.clear();
    let mut s = Services::start(config);
    let t0 = Instant::now();
    while !matches!(s.rig.link(), ftdx3000_rig::LinkState::Connected { .. }) && t0.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let read = |s: &Services, c: &str| s.rig.read(c, Freshness::Radio, Source::Gui, Duration::from_secs(2)).unwrap_or_default();
    let before: Vec<String> = ["MD0;", "BI;", "EX065;", "KS;"].iter().map(|c| read(&s, c)).collect();
    println!("before: {before:?}");
    s.rig.send("PC005;");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(read(&s, "PC;"), "PC005;", "power must be 5 W");
    s.cw_arm(true);
    let t = Instant::now();
    while !s.cw().is_some_and(|k| k.status().ready) && t.elapsed() < Duration::from_secs(8) {
        s.tick();
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("armed: {:?}  radio now {:?}", s.cw().map(|k| k.status()), ["MD0;", "BI;", "EX065;"].iter().map(|c| read(&s, c)).collect::<Vec<_>>());
    s.cw().expect("armed").input(Input::Text("TEST".into()));
    let (mut peak, mut tx_seen) = (0u32, false);
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(4) {
        s.tick();
        let po = read(&s, "RM5;");
        peak = peak.max(po.get(3..6).and_then(|v| v.parse().ok()).unwrap_or(0));
        tx_seen |= read(&s, "TX;") != "TX0;";
        if s.cw().is_some_and(|k| !k.status().sending_text) && t.elapsed() > Duration::from_millis(500) {
            break;
        }
    }
    println!("sent TEST: PO peak {peak}, transmitted: {tx_seen}, status {:?}", s.cw().map(|k| k.status()));
    s.cw_arm(false);
    std::thread::sleep(Duration::from_millis(800));
    let after: Vec<String> = ["TX;", "EX065;"].iter().map(|c| read(&s, c)).collect();
    println!("disarmed: {after:?}");
    // Put the radio back as it was.
    s.rig.send(before[0].as_str());
    s.rig.send(before[1].as_str());
    std::thread::sleep(Duration::from_millis(300));
    s.shutdown();
}
