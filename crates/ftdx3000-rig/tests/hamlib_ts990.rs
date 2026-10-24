//! Hamlib's own TS-990S driver against RigHelm's simulator of
//! them: split, VFOs, levels, PTT, RIT. Skipped where `rigctl` isn't
//! installed. (A simulator: "transmitting" here keys nothing.)
#![cfg(unix)]

use ftdx3000_cat::RadioModel;
use ftdx3000_cat::models::ts990::TS990S;
use ftdx3000_rig::hamlib_check::{SimPort, available, rigctl};

/// Run `cmds`, fail on any rigctl error, and return the output.
fn run(m: &'static RadioModel, port: &SimPort, cmds: &[&str]) -> String {
    let out = rigctl(m, port, cmds).unwrap();
    // (rigctl's "error = ..." lines also come with successes.)
    let failed = ["RPRT -", "Command rejected", "timed out", "Invalid parameter", "Protocol error", "not implemented"].iter().any(|e| out.contains(e));
    assert!(!failed, "{}: {cmds:?}\n{out}\n--- heard: {}", m.key, port.heard_text());
    out
}

fn lines(out: &str) -> Vec<String> {
    out.lines().map(|l| l.trim().to_string()).collect()
}

#[test]
fn hamlib_ts990_split_levels_ptt() {
    if !available() {
        eprintln!("rigctl not installed; skipped");
        return;
    }
    for m in [&TS990S] {
        let port = SimPort::start(m).unwrap();

        // Split on VFO B.
        run(m, &port, &["F 14074000", "I 14076000", "S 1 VFOB"]);
        let out = run(m, &port, &["s", "i"]);
        let l = lines(&out);
        assert!(l.contains(&"1".to_string()) && (l.contains(&"VFOB".to_string()) || l.contains(&"Sub".to_string())), "{}: split not read back\n{out}\n--- heard: {}", m.key, port.heard_text());
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
        // 0.5 of 200 W.
        assert_eq!(port.core.lock().handle("PC;"), vec!["PC100;"], "{}", m.key);
        assert_eq!(port.core.lock().handle("KS;"), vec!["KS025;"], "{}", m.key);
        // (Hamlib 4.6's TS-990S driver never learns its AG format, so `L AF` sends nothing.)
        let out = run(m, &port, &["l AF", "l KEYSPD"]);
        let l = lines(&out);
        assert!(l.iter().any(|x| x == "25"), "{}: KEYSPD\n{out}", m.key);

        // PTT.
        run(m, &port, &["T 1"]);
        assert!(port.core.lock().tx != 0, "{}: T 1 did not key", m.key);
        // (The TS-990S has no TX read: Hamlib's `t` answers from its own cache.)
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
        // (Hamlib's TS-990S driver sets no passband with M.)
    }
}
