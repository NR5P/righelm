//! Memory channels (on the FTDX3000 001-099 and the PMS pairs P-1L..P-9U,
//! 100-117; other radios have their own range).

use eframe::egui::{self, RichText, Ui};
use ftdx3000_cat::ChannelInfo;
use ftdx3000_cat::types::CTCSS_DHZ;

use crate::App;
use crate::widgets::{self, AMBER, DIM};

pub struct MemoriesState {
    /// Channels were requested from this model's radio this session.
    requested: Option<&'static str>,
    show_empty: bool,
    csv_path: String,
    confirm_write: Option<u16>,
}

impl Default for MemoriesState {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        Self { requested: None, show_empty: false, csv_path: format!("{home}/ftdx3000-memories.csv"), confirm_write: None }
    }
}

/// An FTDX3000 channel's name (`005`, `P-1L`).
#[cfg(test)]
fn channel_name(ch: u16) -> String {
    ftdx3000_cat::model::FTDX3000.caps.memory.name(ch)
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let rig = app.rig().clone();
    let s = rig.state().clone();
    let model = s.model;
    let mem = model.caps.memory;
    let name = |ch: u16| mem.name(ch);
    if !model.has("MR") {
        ui.label(RichText::new(format!("RigHelm can't read the {}'s memory channels over CAT.", model.full_name())).color(AMBER));
        return;
    }
    let live = rig.link().is_open();
    let auto = live && app.memories.requested != Some(model.key) && s.memories.is_empty();
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{} channels read", s.memories.len())).color(AMBER));
        if ui.add_enabled(live, egui::Button::new("Read all channels")).clicked() || auto {
            app.memories.requested = Some(model.key);
            let cmds: Vec<String> = mem.channels().map(|c| format!("MR{c:03};")).collect();
            let refs: Vec<&str> = cmds.iter().map(String::as_str).collect();
            rig.refresh(&refs);
        }
        ui.checkbox(&mut app.memories.show_empty, "Show empty");
        ui.separator();
        ui.label("CSV");
        ui.add(egui::TextEdit::singleline(&mut app.memories.csv_path).desired_width(300.0));
        if ui.add_enabled(!s.memories.is_empty(), egui::Button::new("Export")).clicked() {
            let mut out = String::from("channel,frequency_hz,mode,clarifier_hz,rx_clar,tx_clar,ctcss,tone_hz,shift\n");
            for (ch, m) in &s.memories {
                out.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{}\n",
                    name(*ch),
                    m.freq,
                    m.mode.map(|m| m.label()).unwrap_or(""),
                    m.clar_offset,
                    m.rx_clar as u8,
                    m.tx_clar as u8,
                    m.ctcss,
                    CTCSS_DHZ.get(m.tone as usize).map(|t| format!("{}.{}", t / 10, t % 10)).unwrap_or_default(),
                    m.shift
                ));
            }
            match std::fs::write(&app.memories.csv_path, out) {
                Ok(()) => app.flash(format!("exported {} channels", s.memories.len())),
                Err(e) => app.flash(format!("export failed: {e}")),
            }
        }
    });
    ui.label(RichText::new(format!("Current channel: {}   ·   VFO-A {} {}", name(s.memory_channel), widgets::format_freq(s.freq_a), s.mode_a.map(|m| m.label()).unwrap_or(""))).weak());
    ui.separator();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("mem").num_columns(7).striped(true).spacing([14.0, 5.0]).show(ui, |ui| {
            for h in ["Ch", "Frequency", "Mode", "Clarifier", "Tone", "", ""] {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            for ch in mem.channels() {
                let m = s.memories.get(&ch);
                if m.is_none() && !app.memories.show_empty {
                    continue;
                }
                let cur = s.memory_channel == ch;
                ui.label(RichText::new(name(ch)).monospace().color(if cur { AMBER } else { egui::Color32::WHITE }));
                match m {
                    Some(m) => {
                        ui.label(RichText::new(widgets::format_freq(m.freq)).monospace().size(15.0));
                        ui.label(m.mode.map(|m| m.label()).unwrap_or("?"));
                        ui.label(if m.rx_clar || m.tx_clar { format!("{:+} Hz", m.clar_offset) } else { String::new() });
                        ui.label(match m.ctcss {
                            0 => String::new(),
                            c => {
                                let t = CTCSS_DHZ.get(m.tone as usize).copied().unwrap_or(0);
                                format!("{} {}.{}", if c == 1 { "TSQL" } else { "ENC" }, t / 10, t % 10)
                            }
                        });
                    }
                    None => {
                        ui.label(RichText::new("empty").color(DIM));
                        ui.label("");
                        ui.label("");
                        ui.label("");
                    }
                }
                ui.horizontal(|ui| {
                    if model.has("MC") && ui.add_enabled(live && m.is_some(), egui::Button::new("Recall").small()).on_hover_text("Select this channel (memory mode)").clicked() {
                        rig.send(format!("MC{ch:03};"));
                    }
                    if model.has("MA") && ui.add_enabled(live && m.is_some(), egui::Button::new("▸ VFO-A").small()).on_hover_text("Copy this channel into VFO-A").clicked() {
                        rig.send(format!("MC{ch:03};MA;"));
                        rig.send("VS0;");
                    }
                });
                ui.horizontal(|ui| {
                    if !model.has("MW") {
                        return;
                    }
                    if app.memories.confirm_write == Some(ch) {
                        if ui.add(egui::Button::new(RichText::new("Overwrite?").color(crate::widgets::RED)).small()).clicked() {
                            let info = ChannelInfo { freq: s.freq_a, mode: s.mode_a, ..s.info_a.clone() };
                            rig.send(info.to_mw(ch));
                            rig.refresh(&[&format!("MR{ch:03};")]);
                            app.memories.confirm_write = None;
                        }
                        if ui.small_button("✕").clicked() {
                            app.memories.confirm_write = None;
                        }
                    } else if ui.add_enabled(live, egui::Button::new("Store VFO-A").small()).clicked() {
                        if m.is_some() {
                            app.memories.confirm_write = Some(ch);
                        } else {
                            let info = ChannelInfo { freq: s.freq_a, mode: s.mode_a, ..s.info_a.clone() };
                            rig.send(info.to_mw(ch));
                            rig.refresh(&[&format!("MR{ch:03};")]);
                        }
                    }
                });
                ui.end_row();
            }
        });
        if s.memories.is_empty() {
            ui.label(RichText::new("Press \"Read all channels\" to load the radio's memories.").weak());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(channel_name(1), "001");
        assert_eq!(channel_name(100), "P-1L");
        assert_eq!(channel_name(101), "P-1U");
        assert_eq!(channel_name(117), "P-9U");
    }
}
