//! Hamlib's FT-891 driver (model 1036) against RigHelm's FT-891
//! simulator: split (`ST`), IF shift and width (with their on/off digit),
//! RF gain (0-30 on the radio), PTT. (Hamlib's FT-891 has no RIT function,
//! so the clarifier, `CF`, is only in the unit tests.) Skipped where
//! `rigctl` isn't installed. This is a simulator: "transmitting" here keys
//! nothing.
#![cfg(unix)]

use ftdx3000_cat::models::ft891::FT891;
use ftdx3000_rig::hamlib_check::{SimPort, available, rigctl};

#[test]
fn hamlib_drives_the_ft891() {
    if !available() {
        eprintln!("rigctl not installed; skipped");
        return;
    }
    let m = &FT891;
    let port = SimPort::start(m).unwrap();
    let cmds = [
        "F", "7074000", "M", "USB", "2400", "m", "S", "1", "VFOB", "s", "L", "RF", "0.5", "l", "RF", "L", "RFPOWER", "0.25", "l", "RFPOWER",
        "L", "IF", "300", "T", "1", "t",
    ];
    let out = rigctl(m, &port, &cmds).unwrap();
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    for want in ["USB", "2400", "0.5", "0.25"] {
        assert!(lines.contains(&want), "{want} not read back:\n{out}\n{}", port.heard_text());
    }
    assert!(!out.contains("RPRT -"), "rigctl error:\n{out}");
    let heard = port.heard_text();
    for native in ["ST1;", "IS01+0300;", "SH0114;", "RG0015;"] {
        assert!(heard.contains(native), "Hamlib didn't send {native}: {heard}");
    }
    let mut core = port.core.lock();
    assert_eq!(core.freq_a, 7_074_000);
    assert_eq!(core.tx, 1);
    for (read, want) in [("FT;", "FT1;"), ("IS0;", "IS0+0300;"), ("SH0;", "SH014;"), ("RG0;", "RG0128;"), ("PC;", "PC025;")] {
        assert_eq!(core.handle(read), vec![want.to_string()], "{read}");
    }
    drop(core);
    rigctl(m, &port, &["T", "0"]).unwrap();
    assert_eq!(port.core.lock().tx, 0);
}
