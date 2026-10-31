//! Check RigHelm against the real FTDX3000 without transmitting.
//!
//! Every byte for the radio passes a guard that drops any command that
//! could transmit (TX/MX on, tuner start, keyer/voice playback, VOX on).
//! Only reads, and writes of values the radio already has, are made.
//!
//!     cargo run --release --example radio_check -- [PORT]
//!
//! Close RigHelm first: it needs the CAT port.

use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ftdx3000_cat::command;
use ftdx3000_rig::transport::{Link, SerialSettings, probe};
use ftdx3000_rig::{EngineConfig, Freshness, LinkState, Rig, Source};
use parking_lot::Mutex;

/// Drops transmit commands on the way to the radio, and records traffic.
struct TxGuard {
    inner: Box<dyn Write + Send>,
    framer: ftdx3000_cat::Framer,
    blocked: Arc<Mutex<Vec<String>>>,
    sent: Arc<Mutex<Vec<String>>>,
}

impl Write for TxGuard {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for f in self.framer.push(buf) {
            if command::is_tx_command(&f) || f.eq_ignore_ascii_case("PS0;") {
                self.blocked.lock().push(f);
                continue;
            }
            self.sent.lock().push(f.clone());
            self.inner.write_all(f.as_bytes())?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        ftdx3000_rig::detect::detect_radios().into_iter().next().map(|r| r.cat.stable_path().to_string()).expect("no radio found")
    });
    let mut problems: Vec<String> = Vec::new();
    let mut ok = |what: &str, good: bool, detail: String| {
        println!("{} {what}: {detail}", if good { "PASS" } else { "FAIL" });
        if !good {
            problems.push(what.to_string());
        }
    };

    // 1. Probing finds it and knows it's an FTDX3000.
    let settings = SerialSettings { path: path.clone(), ..Default::default() };
    let (found, link) = probe(&settings, None, |b, f| println!("  probing at {b} ({f:?})")).expect("probe");
    ok("detected as FTDX3000", found.model.map(|m| m.key) == Some("ftdx3000"), format!("{} at {} bps, model {:?}", found.id_answer, found.baud, found.model.map(|m| m.key)));

    let blocked = Arc::new(Mutex::new(Vec::new()));
    let sent = Arc::new(Mutex::new(Vec::new()));
    let link = Link {
        reader: link.reader,
        writer: Box::new(TxGuard { inner: link.writer, framer: Default::default(), blocked: blocked.clone(), sent: sent.clone() }),
        description: link.description,
    };

    // 2. The engine's start-up: every read answered, the whole menu read.
    let rig = Rig::start();
    rig.set_log_polls(true);
    rig.connect_link(link, EngineConfig::default());
    let t0 = Instant::now();
    while rig.state().menu.len() < 196 && t0.elapsed() < Duration::from_secs(40) {
        std::thread::sleep(Duration::from_millis(200));
    }
    std::thread::sleep(Duration::from_secs(3));
    let s = rig.state().clone();
    ok("whole menu read", s.menu.len() == 196, format!("{}/196 in {:.1} s", s.menu.len(), t0.elapsed().as_secs_f32()));
    ok("model is FTDX3000, no translator", rig.model().is_ftdx3000(), rig.model().full_name());
    ok("link connected", matches!(rig.link(), LinkState::Connected { .. }), format!("{:?}", rig.link()));
    ok("radio on", s.power_on == Some(true), format!("{:?}", s.power_on));
    ok("VFO-A sensible", rig.model().in_range(s.freq_a) && s.mode_a.is_some(), format!("{} Hz {:?}, VFO-B {} Hz {:?}", s.freq_a, s.mode_a, s.freq_b, s.mode_b));
    let missed: Vec<String> = rig.log_snapshot(4000).into_iter().filter(|e| e.text.starts_with("no answer") || e.text.contains("stopped answering") || e.text.contains("rejected")).map(|e| e.text).collect();
    ok("nothing unanswered at start-up", missed.is_empty(), format!("{missed:?}"));

    // 3. Every read the screens and polls use, straight from the radio.
    let reads = [
        "ID;", "FA;", "FB;", "MD0;", "IF;", "OI;", "AG0;", "RG0;", "SQ0;", "GT0;", "PA0;", "RA0;", "RF0;", "NB0;", "NL0;", "NR0;", "RL0;", "BC0;",
        "BP00;", "BP01;", "CO00;", "CO01;", "CO02;", "SH0;", "NA0;", "IS0;", "PC;", "MG;", "PR0;", "PR1;", "PL;", "VX;", "VG;", "VD;", "ML0;",
        "ML1;", "KR;", "BI;", "CS;", "KS;", "KP;", "SD;", "TX;", "MX;", "FT;", "FR;", "VS;", "AC;", "AN0;", "MS;", "SM0;", "RM1;", "LK;", "FS;",
        "PS;", "RT;", "XT;", "MC;", "CN0;", "CT0;", "OS0;", "BY;", "RI0;", "DA;", "TS;", "SC;", "SF;", "UL;", "VF0;", "RO;", "AI;", "KM1;",
        "MR001;", "EX038;", "EX196;",
    ];
    let mut failed = Vec::new();
    for r in reads {
        match rig.read(r, Freshness::Radio, Source::Gui, Duration::from_secs(2)) {
            Ok(a) if a.starts_with(&r[..2]) => {}
            other => failed.push(format!("{r} -> {other:?}")),
        }
    }
    ok("all reads answered", failed.is_empty(), if failed.is_empty() { format!("{} reads", reads.len()) } else { format!("{failed:?}") });

    // 4. Writes of what the radio already has (nothing changes), read back.
    let mut same = Vec::new();
    for r in ["FA;", "MD0;", "AG0;", "RG0;", "SH0;", "NA0;", "PC;", "KS;", "KP;", "EX034;", "GT0;"] {
        let Ok(before) = rig.read(r, Freshness::Radio, Source::Gui, Duration::from_secs(2)) else { continue };
        let set = match &before[..2] {
            // GT answers 4-6 for AUTO but is set with 4.
            "GT" => format!("GT0{};", before[3..4].parse::<u8>().unwrap_or(0).min(4)),
            _ => before.clone(),
        };
        rig.send(&set);
        std::thread::sleep(Duration::from_millis(400));
        let after = rig.read(r, Freshness::Radio, Source::Gui, Duration::from_secs(2));
        if after.as_deref() != Ok(before.as_str()) {
            same.push(format!("{set} -> {after:?} (was {before})"));
        }
    }
    ok("unchanged after writing the same values", same.is_empty(), format!("{same:?}"));

    // 5. rigctld server on another port, read-only, against the real radio
    // (needs Hamlib's rigctl, which the check finds on Unix).
    #[cfg(unix)]
    if ftdx3000_rig::hamlib_check::available() {
        let cfg = ftdx3000_rig::rigctld::RigctldConfig { enabled: true, bind: "127.0.0.1:45329".into(), block_tx: true };
        let _srv = ftdx3000_rig::rigctld::RigctldServer::start(rig.clone(), cfg).expect("rigctld");
        let out = std::process::Command::new("rigctl").args(["-m", "2", "-r", "127.0.0.1:45329", "f", "m", "t", "l", "STRENGTH", "s"]).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        let freq_ok = text.lines().next().and_then(|l| l.trim().parse::<u64>().ok()) == Some(rig.state().freq_a);
        ok("rigctld (Hamlib NET rigctl) reads the radio", freq_ok && out.status.success(), text.replace('\n', " | "));
    }

    // 6. Never anything that transmits.
    let b = blocked.lock().clone();
    ok("no transmit command reached the radio", b.is_empty(), format!("blocked: {b:?}"));
    let tx_seen = sent.lock().iter().filter(|f| command::is_tx_command(f)).count();
    ok("guard saw no transmit frames pass", tx_seen == 0, format!("{} frames sent in all", sent.lock().len()));
    ok("never transmitted", !rig.state().transmitting(), format!("TX {} MOX {}", rig.state().tx, rig.state().mox));

    rig.shutdown();
    std::thread::sleep(Duration::from_millis(800));
    println!("\n{}", if problems.is_empty() { "ALL PASSED".to_string() } else { format!("FAILED: {problems:?}") });
}
