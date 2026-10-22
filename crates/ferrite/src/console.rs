//! Raw CAT console and traffic log.

use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText, Ui};
use ftdx3000_cat::{Request, classify, command};
use ftdx3000_rig::engine::Dir;
use ftdx3000_rig::{Freshness, Source};

use crate::App;
use crate::widgets::{AMBER, DIM, GREEN};

pub struct ConsoleState {
    input: String,
    show_polls: bool,
    filter: String,
    paused: bool,
    started: Instant,
    history: Vec<String>,
}

impl Default for ConsoleState {
    fn default() -> Self {
        Self { input: String::new(), show_polls: false, filter: String::new(), paused: false, started: Instant::now(), history: Vec::new() }
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let rig = app.rig().clone();
    let st = &mut app.console;
    ui.horizontal(|ui| {
        ui.label("Command");
        let r = ui.add(egui::TextEdit::singleline(&mut st.input).desired_width(260.0).font(egui::TextStyle::Monospace).hint_text("e.g. FA;  or  EX038;"));
        let send = (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || ui.button("Send").clicked();
        if send && !st.input.trim().is_empty() {
            let mut f = ftdx3000_cat::Framer::new();
            let text = if st.input.trim().ends_with(';') { st.input.trim().to_string() } else { format!("{};", st.input.trim()) };
            for frame in f.push(text.as_bytes()) {
                let cmd = command::normalize(&frame);
                match classify(&cmd) {
                    // Reads go through the engine so they don't collide with polling.
                    Request::Read { .. } => {
                        let r = rig.clone();
                        std::thread::spawn(move || {
                            if let Err(e) = r.read(&cmd, Freshness::Radio, Source::Gui, Duration::from_secs(2)) {
                                r.note(format!("{cmd} → {e}"));
                            }
                        });
                    }
                    Request::Set => rig.send(&cmd),
                    Request::Unknown => {
                        let r = rig.clone();
                        std::thread::spawn(move || {
                            r.raw(&cmd, Duration::from_millis(400));
                        });
                    }
                }
            }
            st.history.push(st.input.clone());
            st.input.clear();
            r.request_focus();
        }
        ui.separator();
        if ui.checkbox(&mut st.show_polls, "Show background polling").changed() {
            rig.set_log_polls(st.show_polls);
        }
        ui.checkbox(&mut st.paused, "Pause");
        ui.label("Filter");
        ui.add(egui::TextEdit::singleline(&mut st.filter).desired_width(100.0));
        if ui.button("Clear").clicked() {
            rig.clear_log();
        }
    });
    let stats = rig.stats();
    ui.label(
        RichText::new(format!(
            "sent {}  received {}  served from cache {}  timeouts {}  rejected (?;) {}",
            stats.tx_frames, stats.rx_frames, stats.cache_hits, stats.timeouts, stats.rejected
        ))
        .weak(),
    );
    ui.separator();
    let entries = rig.log_snapshot(1500);
    let filter = st.filter.to_uppercase();
    let started = st.started;
    egui::ScrollArea::vertical().stick_to_bottom(!st.paused).auto_shrink([false, false]).show(ui, |ui| {
        for e in entries.iter().filter(|e| filter.is_empty() || e.text.to_uppercase().contains(&filter)) {
            let t = e.at.saturating_duration_since(started).as_secs_f32();
            let (arrow, color) = match e.dir {
                Dir::Tx => ("→", AMBER),
                Dir::Rx => ("←", GREEN),
                Dir::Info => ("•", Color32::LIGHT_GRAY),
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{t:9.3}")).monospace().color(DIM));
                ui.label(RichText::new(arrow).monospace().color(color));
                ui.label(RichText::new(format!("{:<14}", e.source.as_ref().map(|s| s.to_string()).unwrap_or_default())).monospace().color(DIM));
                ui.label(RichText::new(&e.text).monospace().color(color));
            });
        }
    });
}
