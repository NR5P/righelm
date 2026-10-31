//! Hardware check of the everyday controls against the real FTDX3000,
//! through RigHelm's engine as the app sends them: bands, modes, VFO B,
//! split, AGC, front end, DSP, filters, gains, clarifier and lock are each
//! changed, read back from the radio, and put back as they were. With an
//! RTL-SDR on RX OUT the band scope must keep delivering rows across band
//! changes (the dongle retunes). Nothing here transmits.
//!
//!     cargo run --release --example functions_check
//!
//! Close RigHelm first: it needs the CAT port and the dongle.

use std::time::{Duration, Instant};

use ftdx3000_rig::scope::{Feed, ScopeView};
use ftdx3000_rig::{Config, Freshness, Services, Source};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    // Work on a copy of the settings: Services saves them.
    let copy = std::env::temp_dir().join("righelm-check.toml");
    let _ = std::fs::copy(Config::path(), &copy);
    // SAFETY: no other threads exist yet.
    unsafe { std::env::set_var("FTDX3000_CONFIG", &copy) };
    let mut config = Config::load();
    config.engine.remote = None;
    config.remote_server.enabled = false;
    config.rigctld.enabled = false;
    config.virtual_ports.clear();
    let scope_rx_out = config.scope.enabled && config.scope.feed == Feed::RxOut;
    let mut s = Services::start(config);
    let t0 = Instant::now();
    while !matches!(s.rig.link(), ftdx3000_rig::LinkState::Connected { .. }) && t0.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_secs(2));
    let read = |s: &Services, c: &str| s.rig.read(c, Freshness::Radio, Source::Gui, Duration::from_secs(2)).unwrap_or_default();
    let mut problems: Vec<String> = Vec::new();
    let mut report = |what: &str, good: bool, detail: String| {
        println!("{} {what}: {detail}", if good { "PASS" } else { "FAIL" });
        if !good {
            problems.push(what.to_string());
        }
    };
    // Send `set` and wait for the radio to answer `read` with `want`.
    let set_and_check = |s: &Services, read_cmd: &str, set: &str, want: &str| -> (bool, Duration, String) {
        let t = Instant::now();
        s.rig.send(set);
        let mut got = String::new();
        while t.elapsed() < Duration::from_secs(2) {
            got = read(s, read_cmd);
            if got == want {
                return (true, t.elapsed(), got);
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        (false, t.elapsed(), got)
    };

    let reads = ["FA;", "FB;", "MD0;", "FT;", "GT0;", "PA0;", "RA0;", "NB0;", "NR0;", "BC0;", "NA0;", "SH0;", "IS0;", "AG0;", "RG0;", "RT;", "XT;", "LK;"];
    let before: Vec<(&str, String)> = reads.iter().map(|r| (*r, read(&s, r))).collect();
    println!("before: {}", before.iter().map(|(_, v)| v.as_str()).collect::<Vec<_>>().join(" "));
    let tx_watch = |s: &Services| s.rig.state().transmitting();

    // 1. Bands: each frequency reaches the radio, and the scope follows.
    let sub = s.scope_tap.subscribe(ScopeView { center_hz: 7_074_000.0, span_hz: 200_000.0, cols: 256, fps: 15 });
    for hz in [14_074_000u64, 3_573_000, 21_074_000, 28_074_000, 7_074_000] {
        let cmd = format!("FA{hz:08};");
        let (good, took, got) = set_and_check(&s, "FA;", &cmd, &cmd);
        report(&format!("band change to {hz}"), good, format!("radio answered {got} after {} ms", took.as_millis()));
        if scope_rx_out {
            sub.set_view(ScopeView { center_hz: hz as f64, span_hz: 200_000.0, cols: 256, fps: 15 });
            let t = Instant::now();
            let (mut rows, mut on_freq) = (0, 0);
            while t.elapsed() < Duration::from_secs(3) {
                if let Ok(row) = sub.rows.recv_timeout(Duration::from_millis(500)) {
                    rows += 1;
                    let covered = row.cols.iter().filter(|&&v| v > 0).count();
                    if (row.start_hz - (hz as f64 - 100_000.0)).abs() < 1_000.0 && covered > row.cols.len() / 2 {
                        on_freq += 1;
                    }
                }
            }
            report(&format!("band scope after moving to {hz}"), on_freq >= 10, format!("{rows} rows in 3 s, {on_freq} showing the new band; {}", s.scope_tap.status().text));
        }
    }

    // 2. Modes.
    let modes = [("1", "LSB"), ("2", "USB"), ("3", "CW"), ("4", "FM"), ("5", "AM"), ("6", "RTTY-L"), ("7", "CW-R"), ("8", "DATA-L"), ("9", "RTTY-U"), ("A", "DATA-FM"), ("B", "FM-N"), ("C", "DATA-U"), ("D", "AM-N")];
    let mut bad = Vec::new();
    for (code, name) in modes {
        let mode = format!("MD0{code};");
        let (good, _, got) = set_and_check(&s, "MD0;", &mode, &mode);
        if !good {
            bad.push(format!("{name}: {got}"));
        }
    }
    report("all 13 modes", bad.is_empty(), if bad.is_empty() { "each read back".into() } else { format!("{bad:?}") });
    set_and_check(&s, "MD0;", "MD01;", "MD01;");

    // 3. Settings: each value read back from the radio.
    // (set, the answer that means it took). Split is set the way the SPLIT
    // button does it: the radio toggles on a plain FT0;/FT1;.
    let same = |v: &'static str| (v, v);
    let lk = before.iter().find(|(r, _)| *r == "LK;").and_then(|(_, v)| v.get(2..3)?.parse::<u8>().ok()).unwrap_or(0);
    let (lock_on, lock_off) = (format!("LK{};", lk | 1), format!("LK{};", lk & !1));
    let settings: Vec<(&str, Vec<(&str, &str)>, &str)> = vec![
        ("FB;", vec![same("FB07100000;")], "VFO-B frequency"),
        ("FT;", vec![("FT3;", "FT1;"), ("FT2;", "FT0;")], "split (TX on VFO-B and back)"),
        ("GT0;", vec![same("GT01;"), same("GT02;"), same("GT03;")], "AGC fast/mid/slow"),
        ("PA0;", vec![same("PA00;"), same("PA01;"), same("PA02;")], "IPO/AMP1/AMP2"),
        ("RA0;", vec![same("RA00;"), same("RA01;"), same("RA02;"), same("RA03;")], "attenuator off/6/12/18"),
        ("NB0;", vec![same("NB01;"), same("NB00;")], "noise blanker"),
        ("NR0;", vec![same("NR01;"), same("NR00;")], "DNR"),
        ("BC0;", vec![same("BC01;"), same("BC00;")], "auto notch"),
        ("NA0;", vec![same("NA01;"), same("NA00;")], "narrow"),
        ("SH0;", vec![same("SH012;"), same("SH010;")], "filter width"),
        ("IS0;", vec![same("IS0+0300;"), same("IS0-0300;")], "IF shift"),
        ("AG0;", vec![same("AG0040;")], "AF gain"),
        ("RG0;", vec![same("RG0200;")], "RF gain"),
        ("RT;", vec![same("RT1;"), same("RT0;")], "RX clarifier"),
        ("XT;", vec![same("XT1;"), same("XT0;")], "TX clarifier"),
        // The radio packs other bits into LK; bit 0 is the dial lock.
        ("LK;", vec![(lock_on.as_str(), lock_on.as_str()), (lock_off.as_str(), lock_off.as_str())], "lock"),
    ];
    for (read_cmd, values, name) in &settings {
        let mut bad = Vec::new();
        let mut slowest = Duration::ZERO;
        for (set, want) in values {
            let (good, took, got) = set_and_check(&s, read_cmd, set, want);
            slowest = slowest.max(took);
            if !good {
                bad.push(format!("{set} -> {got}"));
            }
        }
        report(name, bad.is_empty(), if bad.is_empty() { format!("read back, slowest {} ms", slowest.as_millis()) } else { format!("{bad:?}") });
    }

    // 4. Put everything back, then check it is.
    for (_, v) in &before {
        if !v.is_empty() {
            // GT answers 4-6 for AUTO but is set with 4; split is set with FT2/FT3.
            let v = match v.as_str() {
                v if v.starts_with("GT0") && v.as_bytes().get(3).is_some_and(|b| (b'4'..=b'6').contains(b)) => "GT04;".to_string(),
                "FT0;" => "FT2;".to_string(),
                "FT1;" => "FT3;".to_string(),
                v => v.to_string(),
            };
            s.rig.send(v);
        }
    }
    std::thread::sleep(Duration::from_millis(800));
    let after: Vec<(&str, String)> = reads.iter().map(|r| (*r, read(&s, r))).collect();
    let differ: Vec<String> = before.iter().zip(&after).filter(|(b, a)| b.1 != a.1).map(|(b, a)| format!("{} {} -> {}", b.0, b.1, a.1)).collect();
    report("radio back as it was", differ.is_empty(), if differ.is_empty() { "all settings restored".into() } else { format!("{differ:?}") });
    report("never transmitted", !tx_watch(&s), format!("TX {}", read(&s, "TX;")));

    s.shutdown();
    std::thread::sleep(Duration::from_millis(500));
    println!("\n{}", if problems.is_empty() { "ALL PASSED".to_string() } else { format!("FAILED: {problems:?}") });
}
