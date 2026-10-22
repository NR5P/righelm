//! Hardware check of the computer-mic path: switches to the computer mic,
//! keys the radio for 2 s at 5 W in USB as the operator would, reads the
//! power meter, un-keys and switches back.
//!
//!     cargo run --release --example mic_check -- [PIPEWIRE_SOURCE_NODE]

use std::time::{Duration, Instant};

use ftdx3000_rig::audio::AudioDevice;
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
    if let Some(node) = std::env::args().nth(1) {
        config.tx_audio.mic = AudioDevice::PipeWire(node);
    }
    let mut s = Services::start(config);
    let t0 = Instant::now();
    while !matches!(s.rig.link(), ftdx3000_rig::LinkState::Connected { .. }) && t0.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let read = |s: &Services, c: &str| s.rig.read(c, Freshness::Radio, Source::Gui, Duration::from_secs(2)).unwrap_or_default();
    let before: Vec<String> = ["EX053;", "EX085;", "EX103;"].iter().map(|c| read(&s, c)).collect();
    println!("MIC SEL before: {before:?}");
    s.set_computer_mic(true);
    std::thread::sleep(Duration::from_millis(1500));
    for _ in 0..10 {
        s.tick();
        std::thread::sleep(Duration::from_millis(100));
    }
    let on: Vec<String> = ["EX053;", "EX085;", "EX103;"].iter().map(|c| read(&s, c)).collect();
    println!("MIC SEL computer: {on:?}  bridge: {:?}", s.mic_status());
    let mode = read(&s, "MD0;");
    s.rig.send("MD02;PC005;");
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(read(&s, "PC;"), "PC005;", "power must be 5 W");
    let mut peak = 0;
    s.rig.send("TX1;");
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(2) {
        let po = read(&s, "RM5;");
        peak = peak.max(po.get(3..6).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0));
        std::thread::sleep(Duration::from_millis(100));
    }
    let st = s.mic_status();
    s.rig.send("TX0;");
    std::thread::sleep(Duration::from_millis(500));
    println!("keyed 2 s: PO peak {peak}, TX now {}, bridge while keyed: {st:?}", read(&s, "TX;"));
    // A program on a virtual port keying must not get the mic.
    s.rig.send_from("TX1;", Source::Client("vcom1".into()));
    std::thread::sleep(Duration::from_millis(600));
    let st = s.mic_status();
    s.rig.send("TX0;");
    std::thread::sleep(Duration::from_millis(400));
    println!("keyed by vcom1: bridge sending = {:?}", st.map(|m| m.sending));
    s.set_computer_mic(false);
    std::thread::sleep(Duration::from_millis(800));
    let after: Vec<String> = ["EX053;", "EX085;", "EX103;"].iter().map(|c| read(&s, c)).collect();
    println!("MIC SEL after switching back: {after:?}");
    s.rig.send(mode.as_str());
    std::thread::sleep(Duration::from_millis(300));
    s.shutdown();
}
