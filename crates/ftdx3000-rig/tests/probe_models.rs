//! Connecting finds out which radio it is: every simulated model, probed
//! on a PTY like a real port, is recognised (except models that answer
//! exactly like another one, which have to be chosen in the settings).
#![cfg(unix)]

use ftdx3000_cat::RadioModel;
use ftdx3000_rig::hamlib_check::SimPort;
use ftdx3000_rig::transport::{SerialSettings, probe};

/// Another model answers the same way and comes first in the table.
fn indistinguishable(m: &RadioModel) -> bool {
    ftdx3000_cat::model::all().iter().take_while(|x| x.key != m.key).any(|x| {
        x.family == m.family && x.maker == m.maker && if m.family == ftdx3000_cat::Family::Icom { x.civ_addr == m.civ_addr } else { x.id == m.id && x.maker != ftdx3000_cat::model::Maker::Elecraft }
    })
}

#[test]
fn every_model_is_recognised_when_connecting() {
    let only = std::env::var("RIGHELM_MODELS").unwrap_or_default();
    let mut failed = Vec::new();
    std::thread::scope(|s| {
        let runs: Vec<_> = ftdx3000_cat::model::all()
            .iter()
            .copied()
            .filter(|m| only.is_empty() || only.split(',').any(|k| k.trim() == m.key))
            .filter(|m| !indistinguishable(m))
            .map(|m| {
                (
                    m,
                    s.spawn(move || {
                        let port = SimPort::start(m).unwrap();
                        let settings = SerialSettings { path: port.path.to_string_lossy().to_string(), baud: m.bauds[0], ..Default::default() };
                        probe(&settings, None, |_, _| {}).map(|(r, _)| (r.model.map(|x| x.key), r.id_answer))
                    }),
                )
            })
            .collect();
        for (m, h) in runs {
            match h.join().unwrap() {
                Ok((Some(k), _)) if k == m.key => {}
                other => {
                    eprintln!("{}: probe gave {other:?}", m.key);
                    failed.push(m.key);
                }
            }
        }
    });
    assert!(failed.is_empty(), "not recognised: {failed:?}");
}
