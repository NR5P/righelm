//! Hamlib's own drivers for the FTDX5000, FTDX1200, FT-950 and FT-2000
//! against RigHelm's simulators of them: split, levels, PTT, RIT, beyond
//! the frequency and mode `hamlib_models` checks. Skipped where Hamlib's
//! `rigctl` isn't installed. This is a simulator: transmitting is fine.
//!
//! `RIGHELM_MODELS=ft950 cargo test --test hamlib_ftdx5000` checks just that.
#![cfg(unix)]

use ftdx3000_cat::RadioModel;
use ftdx3000_rig::hamlib_check::{SimPort, answers, available, rigctl};

const MINE: &[&str] = &["ftdx5000", "ftdx1200", "ft950", "ft2000"];

fn models() -> Vec<&'static RadioModel> {
    let only = std::env::var("RIGHELM_MODELS").unwrap_or_default();
    let only: Vec<&str> = only.split(',').map(str::trim).filter(|k| !k.is_empty()).collect();
    ftdx3000_cat::model::all()
        .iter()
        .copied()
        .filter(|m| MINE.contains(&m.key) && (only.is_empty() || only.contains(&m.key)))
        .collect()
}

fn run(m: &'static RadioModel, port: &SimPort, cmds: &[&str]) -> Vec<String> {
    let out = rigctl(m, port, cmds).unwrap();
    assert!(!out.contains("RPRT -"), "{}: {cmds:?} gave an error:\n{out}\n--- heard: {}", m.key, port.heard_text());
    answers(&out)
}

#[test]
fn hamlib_split_levels_ptt_rit() {
    if !available() {
        eprintln!("rigctl not installed; skipped");
        return;
    }
    std::thread::scope(|s| {
        for m in models() {
            s.spawn(move || {
                let port = SimPort::start(m).unwrap();

                // Split: transmit on VFO-B.
                let a = run(m, &port, &["F 14074000", "I 14076000", "S 1 VFOB", "s", "i"]);
                assert!(a.iter().any(|l| l == "1"), "{}: split not on: {a:?}", m.key);
                assert!(a.iter().any(|l| l == "14076000"), "{}: TX frequency: {a:?}", m.key);
                assert_eq!(port.core.lock().freq_b, 14_076_000, "{}", m.key);
                let a = run(m, &port, &["S 0 VFOA", "s"]);
                assert!(a.iter().any(|l| l == "0"), "{}: split not off: {a:?}", m.key);

                // Levels, as Hamlib scales them.
                let a = run(m, &port, &["L AF 0.5", "l AF", "L RFPOWER 0.5", "l RFPOWER", "L MICGAIN 0.5", "l MICGAIN"]);
                let nums: Vec<f32> = a.iter().filter_map(|l| l.parse().ok()).collect();
                assert_eq!(nums.len(), 3, "{}: {a:?}", m.key);
                for v in &nums {
                    assert!((v - 0.5).abs() < 0.02, "{}: level read back {v}: {a:?}", m.key);
                }

                // PTT.
                let a = run(m, &port, &["T 1", "t"]);
                assert!(a.iter().any(|l| l == "1"), "{}: PTT not on: {a:?}", m.key);
                assert!(port.core.lock().tx != 0, "{}", m.key);
                let a = run(m, &port, &["T 0", "t"]);
                assert!(a.iter().any(|l| l == "0"), "{}: PTT not off: {a:?}", m.key);
                assert_eq!(port.core.lock().tx, 0, "{}", m.key);

                // RIT on/off.
                let a = run(m, &port, &["U RIT 1", "u RIT", "U RIT 0", "u RIT"]);
                assert_eq!(a.iter().filter(|l| *l == "1" || *l == "0").cloned().collect::<Vec<_>>(), vec!["1", "0"], "{}: {a:?}", m.key);
            });
        }
    });
}
