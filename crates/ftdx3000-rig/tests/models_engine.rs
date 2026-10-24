//! Every supported radio, through the real engine and that radio's
//! translator, against its simulator (which speaks the radio's own
//! protocol). Checks that what a model says it has actually works.
//!
//! `RIGHELM_MODELS=ft991a,ic7300 cargo test --test models_engine` checks
//! just those.

use std::time::{Duration, Instant};

use ftdx3000_cat::menu::Value;
use ftdx3000_cat::{RadioModel, set_freq_cmd_for, set_mode_cmd};
use ftdx3000_rig::sim::SimRadio;
use ftdx3000_rig::{EngineConfig, Freshness, LinkState, Rig, Source};

const T: Duration = Duration::from_secs(3);

/// The models to check (all, or those in `RIGHELM_MODELS`), each on its
/// own thread.
fn each_model(f: impl Fn(&'static RadioModel) + Sync) {
    let only = std::env::var("RIGHELM_MODELS").unwrap_or_default();
    let only: Vec<&str> = only.split(',').map(str::trim).filter(|k| !k.is_empty()).collect();
    let models: Vec<&'static RadioModel> = ftdx3000_cat::model::all().iter().copied().filter(|m| only.is_empty() || only.contains(&m.key)).collect();
    assert!(!models.is_empty(), "no model matches RIGHELM_MODELS");
    std::thread::scope(|s| {
        let handles: Vec<_> = models.iter().map(|&m| (m, s.spawn(|| f(m)))).collect();
        let failed: Vec<&str> = handles.into_iter().filter_map(|(m, h)| h.join().is_err().then_some(m.key)).collect();
        assert!(failed.is_empty(), "failed: {failed:?}");
    });
}

fn start(m: &'static RadioModel) -> (Rig, SimRadio) {
    let sim = SimRadio::for_model(m);
    let rig = Rig::start();
    let config = EngineConfig { model: m.key.into(), read_menu_on_connect: false, auto_info: false, ..Default::default() };
    rig.connect_link(sim.link(), config);
    let t0 = Instant::now();
    while !(matches!(rig.link(), LinkState::Connected { .. }) && rig.model() == m) {
        assert!(t0.elapsed() < T, "{}: never connected", m.key);
        std::thread::sleep(Duration::from_millis(10));
    }
    (rig, sim)
}

fn read(rig: &Rig, m: &RadioModel, cmd: &str) -> String {
    match rig.read(cmd, Freshness::Radio, Source::Gui, T) {
        Ok(a) => a,
        Err(e) => panic!("{}: {cmd} failed: {e}", m.key),
    }
}

/// Wait until a read gives `want` (sets go out asynchronously).
fn read_until(rig: &Rig, m: &RadioModel, cmd: &str, want: &str) {
    let t0 = Instant::now();
    let mut last = String::new();
    while t0.elapsed() < T {
        last = read(rig, m, cmd);
        if last == want {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("{}: {cmd} gave {last}, wanted {want}", m.key);
}

/// Reads the engine polls and the screens use.
const READS: &[&str] = &[
    "ID;", "FA;", "FB;", "MD0;", "IF;", "OI;", "AG0;", "RG0;", "SQ0;", "GT0;", "PA0;", "RA0;", "RF0;", "NB0;", "NL0;", "NR0;", "RL0;",
    "BC0;", "BP00;", "BP01;", "CO00;", "CO01;", "CO02;", "SH0;", "NA0;", "IS0;", "PC;", "MG;", "PR0;", "PL;", "VX;", "VG;", "VD;",
    "ML0;", "ML1;", "KR;", "BI;", "CS;", "KS;", "KP;", "SD;", "TX;", "MX;", "FT;", "FR;", "VS;", "AC;", "AN0;", "MS;", "SM0;",
    "RM1;", "RM5;", "RM6;", "LK;", "FS;", "PS;", "RT;", "XT;", "MC;", "CN0;", "CT0;", "OS0;", "BY;", "RI0;", "DA;", "TS;", "SC;",
    "SF;", "UL;", "VF0;", "RO;", "AI;",
];

#[test]
fn every_model_answers_what_it_says_it_has() {
    each_model(|m| {
        let (rig, _sim) = start(m);
        for cmd in READS {
            if m.has(&cmd[..2]) {
                let a = read(&rig, m, cmd);
                assert!(a.starts_with(&cmd[..2]), "{}: {cmd} answered {a}", m.key);
            } else {
                assert!(rig.read(cmd, Freshness::Radio, Source::Gui, T).is_err(), "{}: {cmd} should be refused", m.key);
            }
        }
        rig.shutdown();
    });
}

#[test]
fn every_model_tunes_and_changes_mode() {
    each_model(|m| {
        let (rig, _sim) = start(m);
        for b in m.bands.iter().filter(|b| b.label != "GEN").take(4).chain(m.bands.iter().filter(|b| b.lo > 100_000_000).take(1)) {
            let hz = b.lo + 10_000;
            rig.send(set_freq_cmd_for(m, false, hz));
            read_until(&rig, m, "FA;", &set_freq_cmd_for(m, false, hz));
        }
        for &mode in m.modes {
            rig.send(set_mode_cmd(mode));
            read_until(&rig, m, "MD0;", &set_mode_cmd(mode));
        }
        if m.has("BS") {
            for b in m.bands.iter().filter(|b| b.label != "GEN") {
                rig.send(format!("BS{:02};", b.code));
                let t0 = Instant::now();
                loop {
                    let fa = read(&rig, m, "FA;");
                    let hz: u64 = fa[2..fa.len() - 1].parse().unwrap();
                    if (b.lo..=b.hi).contains(&hz) {
                        break;
                    }
                    assert!(t0.elapsed() < T, "{}: BS{:02} ({} m) left FA at {hz}", m.key, b.code, b.label);
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
        rig.shutdown();
    });
}

#[test]
fn every_model_reads_and_writes_its_whole_menu() {
    each_model(|m| {
        let (rig, _sim) = start(m);
        for item in m.menu {
            let cmd = ftdx3000_cat::menu::read_cmd(item.num);
            let a = read(&rig, m, &cmd);
            let raw = &a[5..a.len() - 1];
            let Some(v) = item.decode(raw) else { panic!("{}: menu {} {} answered \"{raw}\" ({a})", m.key, item.num, item.name) };
            // Write it back as RigHelm would, and read it again.
            let set = item.set_cmd(&v, Some(raw.len()));
            rig.send(&set);
            read_until(&rig, m, &cmd, &a);
            // And another value, where there is one.
            let other = match (&v, item.options().iter().map(|o| o.0).find(|c| Value::Int(*c) != v)) {
                (Value::Int(_), Some(c)) => Some(Value::Int(c)),
                _ => None,
            };
            if let Some(o) = other {
                let set = item.set_cmd(&o, Some(raw.len()));
                rig.send(&set);
                read_until(&rig, m, &cmd, &set);
            }
        }
        rig.shutdown();
    });
}

#[test]
fn every_model_takes_level_sets() {
    each_model(|m| {
        let (rig, _sim) = start(m);
        let (lo, hi) = m.caps.power;
        let mid = (lo as u32 + hi as u32) / 2;
        let sets: Vec<(String, String)> = [
            ("AG", "AG0", "AG0128;".to_string()),
            ("RG", "RG0", "RG0200;".to_string()),
            ("PC", "PC", format!("PC{mid:03};")),
            ("MG", "MG", "MG040;".to_string()),
            ("KS", "KS", "KS022;".to_string()),
        ]
        .into_iter()
        .filter(|(c, _, _)| m.has(c))
        .map(|(_, k, s)| (format!("{k};"), s))
        .collect();
        for (r, s) in sets {
            rig.send(&s);
            read_until(&rig, m, &r, &s);
        }
        rig.shutdown();
    });
}
