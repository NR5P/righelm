//! Hamlib's own driver for each supported radio against RigHelm's simulator
//! of it: frequency, mode, PTT state and power level the way WSJT-X and
//! loggers use them. Skipped where Hamlib's `rigctl` isn't installed.
//!
//! `RIGHELM_MODELS=ft991a cargo test --test hamlib_models` checks just that.
#![cfg(unix)]

use ftdx3000_cat::RadioModel;
use ftdx3000_cat::types::Mode;
use ftdx3000_rig::hamlib_check::{SimPort, available, rigctl};

fn models() -> Vec<&'static RadioModel> {
    let only = std::env::var("RIGHELM_MODELS").unwrap_or_default();
    let only: Vec<&str> = only.split(',').map(str::trim).filter(|k| !k.is_empty()).collect();
    ftdx3000_cat::model::all().iter().copied().filter(|m| only.is_empty() || only.contains(&m.key)).collect()
}

#[test]
fn hamlib_drives_every_simulated_model() {
    if !available() {
        eprintln!("rigctl not installed; skipped");
        return;
    }
    std::thread::scope(|s| {
        let runs: Vec<_> = models()
            .into_iter()
            .map(|m| {
                (
                    m,
                    s.spawn(move || {
                        let port = SimPort::start(m).unwrap();
                        // 40 m FT8, or the first band of a VHF/UHF radio.
                        let hz = if m.in_range(7_074_000) { 7_074_000 } else { m.bands[0].lo + 100_000 };
                        let mut cmds = vec![format!("F {hz}"), "f".into()];
                        let modes: Vec<Mode> = [Mode::Usb, Mode::Cw, Mode::DataUsb, Mode::Am].into_iter().filter(|x| m.modes.contains(x)).collect();
                        for x in &modes {
                            cmds.push(format!("M {} 0", x.hamlib()));
                            cmds.push("m".into());
                        }
                        cmds.push("t".into());
                        let refs: Vec<&str> = cmds.iter().map(String::as_str).collect();
                        let out = rigctl(m, &port, &refs).unwrap();
                        let mut problems = Vec::new();
                        if !out.lines().any(|l| l.trim() == hz.to_string()) {
                            problems.push(format!("frequency {hz} not read back"));
                        }
                        for x in &modes {
                            if !out.lines().any(|l| l.trim() == x.hamlib()) {
                                problems.push(format!("mode {} not read back", x.hamlib()));
                            }
                        }
                        if out.contains("error") || out.contains("RPRT -") {
                            problems.push("rigctl reported an error".into());
                        }
                        // What Hamlib set must have arrived as meant.
                        let core = port.core.lock();
                        if core.freq_a != hz {
                            problems.push(format!("the radio's VFO-A is at {} after F {hz}", core.freq_a));
                        }
                        if let Some(last) = modes.last() {
                            if core.mode_a != *last {
                                problems.push(format!("the radio is in {} after M {}", core.mode_a.label(), last.hamlib()));
                            }
                        }
                        drop(core);
                        (problems, out, port.heard_text())
                    }),
                )
            })
            .collect();
        let mut failed = Vec::new();
        for (m, h) in runs {
            let (problems, out, heard) = h.join().unwrap();
            if !problems.is_empty() {
                eprintln!("=== {} (Hamlib {}): {}\n{out}\n--- the simulator heard: {heard}\n", m.key, m.hamlib, problems.join("; "));
                failed.push(m.key);
            }
        }
        assert!(failed.is_empty(), "Hamlib problems with: {failed:?}");
    });
}
