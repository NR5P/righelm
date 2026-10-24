//! Hamlib's Elecraft drivers (K3, KX3, KX2, K4) against RigHelm's simulators
//! of them: split, levels, PTT and RIT, beyond what `hamlib_models` checks.
//! Skipped where Hamlib's `rigctl` isn't installed. Only the simulator
//! "transmits" here.
#![cfg(unix)]

use ftdx3000_cat::RadioModel;
use ftdx3000_cat::models::k3::{K3, KX2, KX3};
use ftdx3000_cat::models::k4::K4;
use ftdx3000_rig::hamlib_check::{SimPort, answers, available, rigctl};

fn run(m: &'static RadioModel, cmds: &[&str]) -> (Vec<String>, SimPort) {
    let port = SimPort::start(m).unwrap();
    let out = rigctl(m, &port, cmds).unwrap();
    assert!(!out.contains("RPRT -"), "{}: rigctl reported an error:\n{out}\n--- heard: {}", m.key, port.heard_text());
    (answers(&out), port)
}

#[test]
fn hamlib_split_levels_ptt_and_rit() {
    if !available() {
        eprintln!("rigctl not installed; skipped");
        return;
    }
    std::thread::scope(|s| {
        for m in [&K3, &KX3, &KX2, &K4] {
            s.spawn(move || {
                let (a, port) = run(
                    m,
                    &["J 120", "j", "S 1 VFOB", "s", "S 0 VFOA", "s", "L AF 0.5", "l AF", "L RF 0.5", "l RF", "T 1", "t", "T 0", "t"],
                );
                let heard = port.heard_text();
                // Hamlib's K4 driver scales AF and RF gain as on the K3
                // (0-255, 0-250); the K4 manual has AG 000-060 and RG -nn,
                // which RigHelm follows. So no level check for the K4.
                let levels: &[&str] = if m.key == "k4" { &[] } else { &["0.5", "0.5"] };
                let want: Vec<&str> = ["120", "1", "VFOB", "0", "VFOA"].iter().chain(levels).chain(&["1", "0"]).copied().collect();
                let mut it = a.iter();
                for w in want {
                    assert!(it.any(|l| l == w), "{}: wanted {w} in {a:?}\n--- heard: {heard}", m.key);
                }
                let core = port.core.lock();
                assert_eq!(core.tx, 0, "{}: left transmitting", m.key);
                assert!(heard.contains("FT1;") && heard.contains("FR0;"), "{}: {heard}", m.key);
                assert!(heard.contains("TX;") && heard.contains("RX;"), "{}: {heard}", m.key);
            });
        }
    });
}

#[test]
fn hamlib_power_and_modes() {
    if !available() {
        eprintln!("rigctl not installed; skipped");
        return;
    }
    std::thread::scope(|s| {
        for m in [&K3, &KX3, &KX2, &K4] {
            s.spawn(move || {
                let (lo, hi) = m.caps.power;
                let (a, port) = run(m, &["L RFPOWER 0.6", "l RFPOWER", "M RTTY 0", "m", "M PKTLSB 0", "m", "M CWR 0", "m", "l STRENGTH"]);
                // The radio is at 60% of its range, within a watt.
                let watts: f32 = port.core.lock().model.caps.power.1 as f32 * 0.6;
                drop(port);
                for w in ["RTTY", "PKTLSB", "CWR"] {
                    assert!(a.iter().any(|l| l == w), "{}: no {w} in {a:?}", m.key);
                }
                let p: f32 = a[0].parse().unwrap();
                assert!((p * hi as f32 - watts).abs() <= 1.0 && lo == 0, "{}: RFPOWER {p} ({a:?})", m.key);
            });
        }
    });
}
