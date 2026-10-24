//! Hardware check of the band scope: opens the RTL-SDR dongle (receive only,
//! nothing is sent to the radio), prints a coarse spectrum of what it sees
//! each second, and the strongest signals.
//!
//!     cargo run --release --example scope_check -- [if|rx] [dial Hz] [ppm]
//!
//! `if` (default) = dongle on IF OUT, `rx` = on RX OUT. The radio isn't
//! used: a simulated one supplies the dial frequency.

use std::time::{Duration, Instant};

use ftdx3000_rig::sim::SimRadio;
use ftdx3000_rig::{EngineConfig, Rig};
use ftdx3000_rig::scope::{Feed, Scope, ScopeConfig, ScopeTap, ScopeView, level_db};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let feed = if args.first().map(String::as_str) == Some("rx") { Feed::RxOut } else { Feed::IfOut };
    let dial: f64 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(14_200_000.0);
    let ppm: i32 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(0);
    let sim = SimRadio::new();
    let rig = Rig::start();
    rig.connect_link(sim.link(), EngineConfig { read_menu_on_connect: false, ..Default::default() });
    // Commands sent before the link is up are dropped: wait for it.
    let t0 = Instant::now();
    while !matches!(rig.link(), ftdx3000_rig::LinkState::Connected { .. }) && t0.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(50));
    }
    rig.send(format!("FA{:08};", dial as u64));
    std::thread::sleep(Duration::from_millis(500));
    let tap = ScopeTap::default();
    let _scope = Scope::start(rig.clone(), tap.clone(), ScopeConfig { feed, ppm, calibrated: true, ..Default::default() });
    let span = 1_600_000.0;
    let sub = tap.subscribe(ScopeView { center_hz: dial, span_hz: span, cols: 64, fps: 5 });
    let t0 = Instant::now();
    let mut last = Instant::now();
    while t0.elapsed() < Duration::from_secs(8) {
        if let Ok(row) = sub.rows.recv_timeout(Duration::from_millis(500)) {
            if last.elapsed() < Duration::from_millis(900) {
                continue;
            }
            last = Instant::now();
            let levels: Vec<String> = row.cols.iter().map(|&v| level_db(v).map_or("  .".into(), |d| format!("{:3.0}", d))).collect();
            println!("{} .. {} kHz:", (row.start_hz / 1e3) as i64, ((row.start_hz + span) / 1e3) as i64);
            println!("  {}", levels.join(""));
            let mut peaks: Vec<(usize, u8)> = row.cols.iter().copied().enumerate().collect();
            peaks.sort_by_key(|p| std::cmp::Reverse(p.1));
            let top: Vec<String> = peaks
                .iter()
                .take(3)
                .map(|(c, v)| format!("{:.1} kHz {:.0} dB", (row.start_hz + (*c as f64 + 0.5) * row.col_hz) / 1e3, level_db(*v).unwrap_or(-130.0)))
                .collect();
            println!("  strongest: {}", top.join(", "));
        }
        println!("status: {:?}", tap.status().text);
    }
    // Row rate at 15 a second.
    sub.set_view(ScopeView { center_hz: dial, span_hz: span, cols: 64, fps: 15 });
    while sub.rows.try_recv().is_ok() {}
    let t0 = Instant::now();
    let mut n = 0;
    while t0.elapsed() < Duration::from_secs(3) {
        if sub.rows.recv_timeout(Duration::from_millis(500)).is_ok() {
            n += 1;
        }
    }
    println!("rows: {:.1} a second (asked for 15)", n as f64 / t0.elapsed().as_secs_f64());
    // Calibrate against broadcast carriers in view.
    tap.request_calibration();
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(5) {
        if let Some(c) = tap.take_calibration() {
            println!("calibration: {} ({:?})", c.message, c.fix);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // Click on the strongest signals: what would a click find there?
    let Ok(row) = sub.rows.recv_timeout(Duration::from_secs(2)) else { return };
    let mut cols: Vec<(usize, u8)> = row.cols.iter().copied().enumerate().collect();
    cols.sort_by_key(|c| std::cmp::Reverse(c.1));
    for (c, _) in cols.into_iter().take(5) {
        let rf = row.start_hz + (c as f64 + 0.5) * row.col_hz;
        let answer = tap.ask_detail(rf - row.col_hz, rf + row.col_hz);
        match answer.recv_timeout(Duration::from_secs(2)).ok().and_then(|d| d.find_signal(rf, row.col_hz / 2.0)) {
            Some(s) => println!(
                "  near {:.1} kHz: peak {:.3} kHz, {:.3}..{:.3} kHz ({:.0} Hz wide), +{:.0} dB",
                rf / 1e3,
                s.peak_hz / 1e3,
                s.lo_hz / 1e3,
                s.hi_hz / 1e3,
                s.width_hz(),
                s.snr_db
            ),
            None => println!("  near {:.1} kHz: no clear signal", rf / 1e3),
        }
    }
}
