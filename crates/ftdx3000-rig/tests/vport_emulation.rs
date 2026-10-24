//! The whole chain for another radio, with Hamlib at the far end:
//!
//!   rigctl (Hamlib's driver for the radio)
//!     -> RigHelm virtual port, showing that radio (its translator backwards)
//!     -> engine (RigHelm's language)
//!     -> the radio's translator
//!     -> simulator speaking the radio's protocol.
//!
//! Skipped where Hamlib's `rigctl` isn't installed.
//! `RIGHELM_MODELS=ic7300 cargo test --test vport_emulation` checks one.
#![cfg(unix)]

use std::process::Command;
use std::time::{Duration, Instant};

use ftdx3000_cat::RadioModel;
use ftdx3000_rig::hamlib_check::available;
use ftdx3000_rig::sim::SimRadio;
use ftdx3000_rig::vport::{VirtualPort, VirtualPortConfig};
use ftdx3000_rig::{EngineConfig, LinkState, Rig};

fn models() -> Vec<&'static RadioModel> {
    let only = std::env::var("RIGHELM_MODELS").unwrap_or_default();
    let only: Vec<&str> = only.split(',').map(str::trim).filter(|k| !k.is_empty()).collect();
    ftdx3000_cat::model::all().iter().copied().filter(|m| only.is_empty() || only.contains(&m.key)).collect()
}

#[test]
fn hamlib_through_a_virtual_port_to_every_model() {
    if !available() {
        eprintln!("rigctl not installed; skipped");
        return;
    }
    let mut failed = Vec::new();
    for m in models() {
        let sim = SimRadio::for_model(m);
        let rig = Rig::start();
        rig.connect_link(sim.link(), EngineConfig { model: m.key.into(), read_menu_on_connect: false, ..Default::default() });
        let t0 = Instant::now();
        while !(matches!(rig.link(), LinkState::Connected { .. }) && rig.model() == m) && t0.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(10));
        }
        let dir = std::env::temp_dir().join(format!("righelm-vport-emu-{}-{}", std::process::id(), m.key));
        let port = VirtualPort::start(rig.clone(), VirtualPortConfig { name: "v".into(), ..Default::default() }, dir.clone()).unwrap();
        let mut c = Command::new("rigctl");
        c.arg("-m").arg(m.hamlib.to_string()).arg("-r").arg(dir.join("v"));
        if let Some(&b) = m.bauds.first() {
            c.arg("-s").arg(b.to_string());
        }
        if m.family == ftdx3000_cat::Family::Icom {
            c.arg(format!("--civaddr=0x{:02X}", m.civ_addr));
        }
        if m.rts_cts {
            c.arg("--set-conf=serial_handshake=None");
        }
        let hz = 7_074_000u64;
        c.args(["F", &hz.to_string(), "f", "M", "USB", "0", "m"]);
        let out = c.output().unwrap();
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        let freq_ok = sim.core().freq_a == hz && text.lines().any(|l| l.trim() == hz.to_string());
        let mode_ok = text.lines().any(|l| l.trim() == "USB");
        if !(freq_ok && mode_ok) {
            eprintln!("=== {} through a virtual port: freq {freq_ok} mode {mode_ok}\n{text}", m.key);
            failed.push(m.key);
        }
        drop(port);
        rig.shutdown();
        let _ = std::fs::remove_dir_all(dir);
    }
    assert!(failed.is_empty(), "failed: {failed:?}");
}
