//! Hamlib's drivers for the FTDX101D/MP, FTDX10 and FT-710 against
//! RigHelm's simulators of them: split, levels, PTT and the clarifier, the
//! things loggers and WSJT-X use beyond frequency and mode. Skipped where
//! Hamlib's `rigctl` isn't installed. This is a simulator: "transmitting"
//! here keys nothing.
#![cfg(unix)]

use ftdx3000_cat::RadioModel;
use ftdx3000_cat::models::{ft710, ftdx10, ftdx101};
use ftdx3000_rig::hamlib_check::{SimPort, available, rigctl};

const MODELS: [&RadioModel; 4] = [&ftdx101::FTDX101D, &ftdx101::FTDX101MP, &ftdx10::FTDX10, &ft710::FT710];

fn models() -> Vec<&'static RadioModel> {
    let only = std::env::var("RIGHELM_MODELS").unwrap_or_default();
    let only: Vec<&str> = only.split(',').map(str::trim).filter(|k| !k.is_empty()).collect();
    MODELS.into_iter().filter(|m| only.is_empty() || only.contains(&m.key)).collect()
}

/// What the simulated radio says (in RigHelm's language) to a read.
fn ask(port: &SimPort, read: &str) -> String {
    port.core.lock().handle(read).into_iter().next().unwrap_or_default()
}

fn run(m: &'static RadioModel) -> Result<(), String> {
    let port = SimPort::start(m).map_err(|e| e.to_string())?;
    let mut problems = Vec::new();
    let mut check = |cmds: &[&str], what: &str, ok: &dyn Fn(&str, &SimPort) -> bool| {
        let out = rigctl(m, &port, cmds).unwrap_or_default();
        if out.contains("RPRT -") || !ok(&out, &port) {
            problems.push(format!("{what}: {cmds:?} gave\n{out}\n--- heard: {}", port.heard_text()));
        }
    };
    let lines = |out: &str| out.lines().map(|l| l.trim().to_string()).collect::<Vec<_>>();

    check(&["F", "14074000", "I", "14076500", "S", "1", "VFOB", "s"], "split on", &|out, p| {
        ask(p, "FT;") == "FT1;" && ask(p, "FB;") == "FB14076500;" && lines(out).iter().any(|l| l == "1")
    });
    check(&["S", "0", "VFOA", "s"], "split off", &|out, p| ask(p, "FT;") == "FT0;" && lines(out).iter().any(|l| l == "0"));
    check(&["L", "RFPOWER", "0.5", "l", "RFPOWER"], "power", &|out, p| {
        let max = m.caps.power.1 as u32;
        let want = format!("PC{:03};", max / 2);
        ask(p, "PC;") == want && lines(out).iter().any(|l| l.parse::<f32>().is_ok_and(|v| (v - 0.5).abs() < 0.02))
    });
    check(&["L", "AF", "0.5", "l", "AF"], "AF gain", &|out, p| {
        // 0.5 of 255 is 127 or 128.
        matches!(ask(p, "AG0;").as_str(), "AG0127;" | "AG0128;") && lines(out).iter().any(|l| l.parse::<f32>().is_ok_and(|v| (v - 0.5).abs() < 0.01))
    });
    check(&["L", "KEYSPD", "22", "l", "KEYSPD"], "key speed", &|out, p| ask(p, "KS;") == "KS022;" && lines(out).iter().any(|l| l == "22"));
    check(&["T", "1", "t"], "PTT on", &|out, p| ask(p, "TX;") == "TX1;" && lines(out).iter().any(|l| l == "1"));
    check(&["T", "0", "t"], "PTT off", &|out, p| ask(p, "TX;") == "TX0;" && lines(out).iter().any(|l| l == "0"));
    // (Hamlib answers `j` from its cache of IF, so only the radio's state
    // tells whether the offset arrived.)
    check(&["U", "RIT", "1", "J", "300", "u", "RIT"], "RIT", &|out, p| {
        ask(p, "RT;") == "RT1;" && ask(p, "IF;").contains("+0300") && lines(out).iter().any(|l| l == "1")
    });
    check(&["U", "RIT", "0", "u", "RIT"], "RIT off", &|_, p| ask(p, "RT;") == "RT0;");
    // Hamlib names the FTDX101's VFOs Main and Sub.
    check(&["V", "VFOB", "v", "V", "VFOA", "v"], "VFO", &|out, p| {
        lines(out).iter().any(|l| l == "VFOB" || l == "Sub") && ask(p, "VS;") == "VS0;"
    });
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n\n")) }
}

#[test]
fn hamlib_split_levels_ptt_and_rit() {
    if !available() {
        eprintln!("rigctl not installed; skipped");
        return;
    }
    let failed: Vec<String> = std::thread::scope(|s| {
        let runs: Vec<_> = models().into_iter().map(|m| (m, s.spawn(move || run(m)))).collect();
        runs.into_iter()
            .filter_map(|(m, h)| match h.join().unwrap() {
                Ok(()) => None,
                Err(e) => {
                    eprintln!("=== {} (Hamlib {}):\n{e}\n", m.key, m.hamlib);
                    Some(m.key.to_string())
                }
            })
            .collect()
    });
    assert!(failed.is_empty(), "Hamlib problems with: {failed:?}");
}
