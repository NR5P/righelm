//! Hamlib's FT-991 driver (model 1035, also used for the FT-991A) against
//! RigHelm's FT-991A and FT-991 simulators: split, levels, PTT, clarifier,
//! CTCSS and tuner, checked on both sides (what rigctl reads back, and what
//! the simulated radio ended up with). Skipped where `rigctl` isn't
//! installed. This is a simulator: "transmitting" here keys nothing.
#![cfg(unix)]

use ftdx3000_cat::models::ft991a::{FT991, FT991A};
use ftdx3000_rig::hamlib_check::{SimPort, available, rigctl};

fn check(m: &'static ftdx3000_cat::RadioModel) {
    let port = SimPort::start(m).unwrap();
    let cmds = [
        "F", "145500000", "f", "S", "1", "VFOB", "s", "L", "RFPOWER", "0.5", "l", "RFPOWER", "l", "AF", "L", "SQL", "0.4", "l", "SQL", "T", "1",
        "t", "U", "RIT", "1", "u", "RIT", "C", "885", "U", "TUNER", "1", "U", "APF", "1", "u", "APF",
    ];
    let out = rigctl(m, &port, &cmds).unwrap();
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    for want in ["145500000", "0.5", "0.392157", "0.4"] {
        assert!(lines.contains(&want), "{}: {want} not read back:\n{out}\n{}", m.key, port.heard_text());
    }
    assert!(!out.contains("RPRT -"), "{}: rigctl error:\n{out}", m.key);
    let mut core = port.core.lock();
    assert_eq!(core.freq_a, 145_500_000, "{}", m.key);
    // Still keyed from "T 1" (the simulator transmits nothing).
    assert_eq!(core.tx, 1, "{}", m.key);
    for (read, want) in [("FT;", "FT1;"), ("PC;", "PC050;"), ("SQ0;", "SQ0102;"), ("RT;", "RT1;"), ("CN0;", "CN008;"), ("CT0;", "CT02;"), ("AC;", "AC001;"), ("CO00;", "CO0002;")] {
        assert_eq!(core.handle(read), vec![want.to_string()], "{}: {read}", m.key);
    }
    drop(core);
    let out = rigctl(m, &port, &["T", "0", "t"]).unwrap();
    assert!(out.lines().any(|l| l.trim() == "0"), "{}: not un-keyed:\n{out}", m.key);
    assert_eq!(port.core.lock().tx, 0);
}

#[test]
fn hamlib_drives_the_ft991a_and_ft991() {
    if !available() {
        eprintln!("rigctl not installed; skipped");
        return;
    }
    check(&FT991A);
    check(&FT991);
}
