//! Hamlib's own TS-480 and TS-2000 drivers against RigHelm's simulators of
//! them: split, VFO B, PTT, levels, RIT, CW pitch, VHF. Skipped where
//! Hamlib's `rigctl` isn't installed. Only the simulator is used.
#![cfg(unix)]

use ftdx3000_cat::RadioModel;
use ftdx3000_cat::models::{ts2000, ts480};
use ftdx3000_rig::hamlib_check::{SimPort, answers, available, rigctl};

/// Run `cmds` through rigctl; the answer lines.
fn run(m: &'static RadioModel, port: &SimPort, cmds: &[&str]) -> Vec<String> {
    let out = rigctl(m, port, cmds).unwrap();
    assert!(!out.contains("RPRT -"), "{}: {cmds:?}\n{out}\n--- heard: {}", m.key, port.heard_text());
    answers(&out)
}

fn has(lines: &[String], want: &str) -> bool {
    lines.iter().any(|l| l == want)
}

fn near(lines: &[String], want: f64) -> bool {
    lines.iter().filter_map(|l| l.parse::<f64>().ok()).any(|v| (v - want).abs() < 0.02)
}

fn common(m: &'static RadioModel) {
    let port = SimPort::start(m).unwrap();
    // Split: transmit on VFO B.
    let a = run(m, &port, &["S 1 VFOB", "s", "I 14080000", "i"]);
    assert!(has(&a, "VFOB") && has(&a, "14080000"), "{}: split {a:?}", m.key);
    assert_eq!(port.core.lock().freq_b, 14_080_000, "{}", m.key);
    let a = run(m, &port, &["S 0 VFOA", "s"]);
    assert!(has(&a, "0"), "{}: split off {a:?}", m.key);
    // PTT (the simulator: it is fine to "transmit" here).
    let a = run(m, &port, &["T 1", "t"]);
    assert!(has(&a, "1"), "{}: ptt {a:?}", m.key);
    assert!(port.core.lock().tx != 0, "{}: not transmitting", m.key);
    let a = run(m, &port, &["T 0", "t"]);
    assert!(has(&a, "0"), "{}: unkey {a:?}", m.key);
    assert_eq!(port.core.lock().tx, 0, "{}", m.key);
    // Levels.
    let a = run(m, &port, &["L AF 0.5", "l AF", "L RF 0.8", "l RF", "L CWPITCH 650", "l CWPITCH", "L KEYSPD 22", "l KEYSPD"]);
    assert!(near(&a, 0.5) && near(&a, 0.8), "{}: AF/RF {a:?}", m.key);
    assert!(has(&a, "650") && has(&a, "22"), "{}: pitch/speed {a:?}", m.key);
    // RIT (read in another run: Hamlib answers j from the IF it read at start).
    run(m, &port, &["J 300", "U RIT 1"]);
    let a = run(m, &port, &["j", "u RIT"]);
    assert!(has(&a, "300") && has(&a, "1"), "{}: rit {a:?}", m.key);
    // S-meter (the simulator's wanders): a reading in range.
    let a = run(m, &port, &["l STRENGTH"]);
    assert!(a.iter().filter_map(|l| l.parse::<i32>().ok()).any(|v| (-60..=60).contains(&v)), "{}: strength {a:?}", m.key);
}

#[test]
fn hamlib_drives_the_ts480() {
    if !available() {
        eprintln!("rigctl not installed; skipped");
        return;
    }
    common(&ts480::TS480);
    common(&ts480::TS480HX);
    let m = &ts480::TS480;
    let port = SimPort::start(m).unwrap();
    // Negative RIT comes back signed.
    run(m, &port, &["J -120"]);
    let a = run(m, &port, &["j"]);
    assert!(has(&a, "-120"), "rit {a:?}");
    // The tuner and the meters (RM; answers all three).
    let a = run(m, &port, &["U TUNER 1", "u TUNER", "l SWR", "l ALC"]);
    assert!(has(&a, "1") && near(&a, 1.0), "tuner/swr {a:?}");
}

#[test]
fn hamlib_drives_the_ts2000() {
    if !available() {
        eprintln!("rigctl not installed; skipped");
        return;
    }
    let m = &ts2000::TS2000;
    common(m);
    let port = SimPort::start(m).unwrap();
    // 2 m and 70 cm FM.
    let a = run(m, &port, &["F 145500000", "f", "M FM 0", "m", "F 432100000", "f"]);
    assert!(has(&a, "145500000") && has(&a, "FM") && has(&a, "432100000"), "vhf {a:?}");
    assert_eq!(port.core.lock().freq_a, 432_100_000);
    // AGC (Hamlib: SLOW = GT020) and power (Hamlib probes the range with PC000/PC255).
    let a = run(m, &port, &["L AGC 3", "l AGC", "L RFPOWER 0.5", "l RFPOWER"]);
    assert!(has(&a, "3") && near(&a, 0.5), "agc/power {a:?}");
}
