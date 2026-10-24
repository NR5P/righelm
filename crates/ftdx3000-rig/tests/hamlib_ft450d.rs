//! Hamlib's FT-450D and FT-450 drivers (models 1046, 1027) against
//! RigHelm's simulators: the VFO shown (`VS`, RigHelm's `FR`), relative
//! split (`FT`), 0-255 mic and VOX gain, PTT. Skipped where `rigctl` isn't
//! installed. This is a simulator: "transmitting" here keys nothing.
//!
//! Where Hamlib and the FT-450D reference book disagree, not checked:
//! * CW pitch: Hamlib sends `KP00`-`KP04` for 400-800 Hz; the book says
//!   `KP02`/`04`/`06`/`08`/`10`.
//! * `FT`: Hamlib takes 0/1 as VFO-A/VFO-B; the book says "the displayed
//!   band" / "the opposite band". They agree while VFO-A is shown, which is
//!   what this checks.
#![cfg(unix)]

use ftdx3000_cat::models::ft450d::{FT450, FT450D};
use ftdx3000_rig::hamlib_check::{SimPort, available, rigctl};

fn check(m: &'static ftdx3000_cat::RadioModel) {
    let port = SimPort::start(m).unwrap();
    // Split (VFO-A shown, transmit on VFO-B), then show VFO-B.
    let cmds = [
        "S", "1", "VFOB", "s", "V", "VFOB", "v", "L", "MICGAIN", "0.5", "l", "MICGAIN", "L", "VOXGAIN", "0.3", "l", "VOXGAIN", "U", "RIT", "1",
        "T", "1", "t",
    ];
    let out = rigctl(m, &port, &cmds).unwrap();
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    for want in ["VFOB", "1"] {
        assert!(lines.contains(&want), "{}: {want} not read back:\n{out}\n{}", m.key, port.heard_text());
    }
    assert!(!out.contains("RPRT -"), "{}: rigctl error:\n{out}", m.key);
    let heard = port.heard_text();
    for native in ["VS1;", "FT1;", "MG127;", "VG077;", "RT1;"] {
        assert!(heard.contains(native), "{}: Hamlib didn't send {native}: {heard}", m.key);
    }
    let mut core = port.core.lock();
    assert_eq!(core.tx, 1, "{}", m.key);
    // RigHelm's view: transmitting and now receiving on B.
    for (read, want) in [("FT;", "FT1;"), ("FR;", "FR4;"), ("VS;", "VS1;"), ("MG;", "MG050;"), ("VG;", "VG030;"), ("RT;", "RT1;")] {
        assert_eq!(core.handle(read), vec![want.to_string()], "{}: {read}", m.key);
    }
    drop(core);
    rigctl(m, &port, &["T", "0"]).unwrap();
    assert_eq!(port.core.lock().tx, 0);
}

#[test]
fn hamlib_drives_the_ft450d_and_ft450() {
    if !available() {
        eprintln!("rigctl not installed; skipped");
        return;
    }
    check(&FT450D);
    check(&FT450);
}
