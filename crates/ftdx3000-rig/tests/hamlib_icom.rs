//! Hamlib's Icom drivers against RigHelm's simulators of the IC-7300,
//! IC-7300MK2, IC-7610, IC-705 and IC-9700: VFOs, split, RF power, PTT,
//! levels and meters, the way loggers and digital-mode programs use them.
//! Skipped where Hamlib's `rigctl` isn't installed. The simulator is not a
//! radio: "transmitting" here keys nothing.
#![cfg(unix)]

use ftdx3000_cat::RadioModel;
use ftdx3000_cat::models::{ic705, ic7300, ic7610, ic9700};
use ftdx3000_rig::hamlib_check::{SimPort, answers, available, rigctl};

fn run(m: &'static RadioModel, cmds: &[&str]) -> (SimPort, Vec<String>, String) {
    let port = SimPort::start(m).unwrap();
    let out = rigctl(m, &port, cmds).unwrap();
    let a = answers(&out);
    (port, a, out)
}

fn check(m: &'static RadioModel, ok: bool, what: &str, out: &str, port: &SimPort) {
    assert!(ok, "{} (Hamlib {}): {what}\n{out}\n--- the simulator heard: {}", m.key, m.hamlib, port.heard_text());
}

/// A frequency on the radio's first band and one on another.
fn freqs(m: &RadioModel) -> (u64, u64) {
    let bands: Vec<_> = m.bands.iter().filter(|b| b.label != "GEN").collect();
    let a = bands[0].lo + 50_000;
    let b = bands[bands.len().min(5) - 1].lo + 70_000;
    (a, b)
}

#[test]
fn hamlib_icom_vfos_split_power_ptt() {
    if !available() {
        eprintln!("rigctl not installed; skipped");
        return;
    }
    for m in [&ic7300::IC7300, &ic7300::IC7300MK2, &ic7610::IC7610, &ic705::IC705, &ic9700::IC9700] {
        let (fa, fb) = freqs(m);
        let (fa_s, fb_s) = (fa.to_string(), fb.to_string());
        let (port, a, out) = run(m, &["F", &fa_s, "f", "I", &fb_s, "i", "S", "1", "VFOB", "s", "S", "0", "VFOA", "s"]);
        check(m, a.iter().any(|l| *l == fa_s), "VFO-A frequency not read back", &out, &port);
        check(m, a.iter().any(|l| *l == fb_s), "split (TX) frequency not read back", &out, &port);
        check(m, !out.contains("RPRT -"), "rigctl reported an error", &out, &port);
        {
            let core = port.core.lock();
            check(m, core.freq_a == fa && core.freq_b == fb, &format!("the radio has A {} B {}", core.freq_a, core.freq_b), &out, &port);
        }
        drop(port);

        let (port, a, out) = run(m, &["L", "RFPOWER", "0.5", "l", "RFPOWER", "L", "AF", "0.25", "l", "AF", "T", "1", "t", "T", "0", "t", "l", "STRENGTH"]);
        check(m, !out.contains("RPRT -"), "rigctl reported an error", &out, &port);
        check(m, a.iter().any(|l| l.starts_with("0.5") || l.starts_with("0.49") || l.starts_with("0.50")), "RF power 0.5 not read back", &out, &port);
        check(m, a.iter().any(|l| *l == "1"), "PTT on not read back", &out, &port);
        {
            let core = port.core.lock();
            check(m, core.tx == 0 && !core.mox, "still transmitting after T 0", &out, &port);
        }
    }
}
