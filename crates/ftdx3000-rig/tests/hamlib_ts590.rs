//! Hamlib's own TS-590S / TS-590SG drivers against RigHelm's simulator of
//! them: split, VFOs, levels, PTT, RIT. Skipped where `rigctl` isn't
//! installed. (A simulator: "transmitting" here keys nothing.)
#![cfg(unix)]

use ftdx3000_cat::RadioModel;
use ftdx3000_cat::models::ts590::{TS590S, TS590SG};
use ftdx3000_rig::hamlib_check::{SimPort, available, rigctl};

/// Run `cmds`, fail on any rigctl error, and return the output.
fn run(m: &'static RadioModel, port: &SimPort, cmds: &[&str]) -> String {
    let out = rigctl(m, port, cmds).unwrap();
    assert!(!out.contains("RPRT -") && !out.to_lowercase().contains("error"), "{}: {cmds:?}\n{out}\n--- heard: {}", m.key, port.heard_text());
    out
}

fn lines(out: &str) -> Vec<String> {
    out.lines().map(|l| l.trim().to_string()).collect()
}

#[test]
fn hamlib_ts590_split_levels_ptt() {
    if !available() {
        eprintln!("rigctl not installed; skipped");
        return;
    }
    for m in [&TS590SG, &TS590S] {
        let port = SimPort::start(m).unwrap();

        // Split on VFO B.
        run(m, &port, &["F 14074000", "I 14076000", "S 1 VFOB"]);
        let out = run(m, &port, &["s", "i"]);
        let l = lines(&out);
        assert!(l.contains(&"1".to_string()) && l.contains(&"VFOB".to_string()), "{}: split not read back\n{out}\n--- heard: {}", m.key, port.heard_text());
        assert!(l.contains(&"14076000".to_string()), "{}: TX frequency not read back\n{out}", m.key);
        {
            let c = port.core.lock();
            assert_eq!(c.freq_a, 14_074_000, "{}", m.key);
            assert_eq!(c.freq_b, 14_076_000, "{}", m.key);
        }
        run(m, &port, &["S 0 VFOA"]);
        let out = run(m, &port, &["s"]);
        assert!(lines(&out).first().is_some_and(|l| l == "0"), "{}: split still on\n{out}", m.key);

        // Levels. (Not `l RFPOWER`: Hamlib 4.6's power range probe writes
        // PC000 back to the radio afterwards.)
        run(m, &port, &["L RFPOWER 0.5", "l AF", "L AF 0.5", "L KEYSPD 25"]);
        assert_eq!(port.core.lock().handle("PC;"), vec!["PC050;"], "{}", m.key);
        assert_eq!(port.core.lock().handle("KS;"), vec!["KS025;"], "{}", m.key);
        assert_eq!(port.core.lock().handle("AG0;"), vec!["AG0127;"], "{}", m.key);
        let out = run(m, &port, &["l AF", "l KEYSPD"]);
        let l = lines(&out);
        assert!(l.iter().any(|x| x.starts_with("0.49") || x.starts_with("0.5")), "{}: AF\n{out}", m.key);
        assert!(l.iter().any(|x| x == "25"), "{}: KEYSPD\n{out}", m.key);

        // PTT.
        run(m, &port, &["T 1"]);
        assert!(port.core.lock().tx != 0, "{}: T 1 did not key", m.key);
        let out = run(m, &port, &["t"]);
        assert!(lines(&out).iter().any(|x| x == "1"), "{}: PTT not read back\n{out}", m.key);
        run(m, &port, &["T 0"]);
        assert_eq!(port.core.lock().tx, 0, "{}: T 0 did not unkey", m.key);

        // RIT.
        run(m, &port, &["J 500", "U RIT 1"]);
        let out = run(m, &port, &["j"]);
        assert!(lines(&out).iter().any(|x| x == "500"), "{}: RIT offset\n{out}", m.key);

        // Meter and mode with passband.
        run(m, &port, &["l STRENGTH", "M CW 500"]);
        let out = run(m, &port, &["m"]);
        assert!(lines(&out).iter().any(|x| x == "CW"), "{}: CW\n{out}", m.key);
        assert_eq!(port.core.lock().handle("SH0;"), vec!["SH008;"], "{}: CW width 500 Hz", m.key);
    }
}
