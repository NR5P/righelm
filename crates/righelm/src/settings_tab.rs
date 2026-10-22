//! Transmit audio, display preferences and about.

use eframe::egui::{self, Color32, RichText, Slider, Ui};
use ftdx3000_rig::audio::{self, DeviceEntry};
use ftdx3000_rig::txaudio::{self, MIC_SEL_MENUS, MIC_SEL_USB};

use crate::App;
use crate::widgets::{self, AMBER, DIM, GREEN, RED};

pub fn show(app: &mut App, ui: &mut Ui) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        tx_audio_panel(app, ui);
        ui.add_space(12.0);
        crate::cw_key::settings_panel(app, ui);
        ui.add_space(12.0);
        display(app, ui);
        ui.add_space(12.0);
        about(app, ui);
    });
}

pub const REFUSES_MIC: &str = "The remote radio's computer is not accepting microphone audio, so your voice is not transmitted. \
     Turn on \"Accept microphone audio for transmit\" on that computer (Remote (LAN) tab).";

/// One-click switch between the radio's own microphone and this computer's,
/// with the computer mic's device, gain and level.
pub fn tx_audio_panel(app: &mut App, ui: &mut Ui) {
    crate::remote_tab::refresh_nodes(app);
    let remote = app.services.config.engine.remote.clone();
    let on = app.services.config.tx_audio.computer_mic;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("Transmit audio").strong().size(16.0).color(AMBER));
        ui.horizontal(|ui| {
            let choice = |text: &str, selected: bool| {
                let color = if selected { Color32::BLACK } else { Color32::LIGHT_GRAY };
                egui::Button::new(RichText::new(text).size(16.0).strong().color(color))
                    .fill(if selected { AMBER } else { Color32::from_rgb(40, 34, 20) })
                    .min_size(egui::vec2(240.0, 40.0))
            };
            if ui.add(choice("Radio's microphone", !on)).on_hover_text("Talk into the mic plugged into the radio (MIC SEL = FRONT)").clicked() && on {
                app.services.set_computer_mic(false);
            }
            if ui
                .add(choice("Computer microphone", on))
                .on_hover_text("Talk into this computer's microphone; it is sent to the radio over USB (MIC SEL = USB)")
                .clicked()
                && !on
            {
                app.services.set_computer_mic(true);
            }
        });
        let on = app.services.config.tx_audio.computer_mic;
        mic_sel_line(app, ui, on);
        if ui
            .checkbox(&mut app.services.config.ui.space_ptt, "Hold the space bar to transmit (push-to-talk)")
            .on_hover_text("Transmits while the space bar is held in RigHelm's window, except while typing in a text box or with the CW key armed")
            .changed()
        {
            let _ = app.services.config.save();
        }
        if on {
            mic_controls(app, ui, remote.is_some());
        }
        ui.label(
            RichText::new(
                "The computer mic goes to the radio while you transmit in SSB, AM or FM with MOX here, the radio's PTT or a footswitch — \
                 never when WSJT-X or another program keys the radio. Switching back restores the radio's mic settings.",
            )
            .weak(),
        );
    });
}

/// What the radio's MIC SEL menus say now, and whether that matches.
fn mic_sel_line(app: &App, ui: &mut Ui, computer: bool) {
    let rig = app.rig();
    if !rig.link().is_open() {
        ui.label(RichText::new("The radio is not connected: its mic setting will be changed when it connects.").color(DIM));
        return;
    }
    let mut parts = Vec::new();
    let mut matches = true;
    for (menu, mode) in MIC_SEL_MENUS.iter().zip(["AM", "FM", "SSB"]) {
        let v = txaudio::mic_sel(rig, *menu);
        let label = match v {
            Some(0) => "FRONT",
            Some(1) => "DATA",
            Some(2) => "USB",
            _ => "…",
        };
        matches &= v.is_some_and(|v| (v == MIC_SEL_USB) == computer);
        parts.push(format!("{mode} {label}"));
    }
    let text = format!("Radio MIC SEL: {}", parts.join(" · "));
    ui.label(RichText::new(text).color(if matches { GREEN } else { AMBER }));
}

fn mic_controls(app: &mut App, ui: &mut Ui, remote: bool) {
    // Never offer the radio's own codec as "the computer mic".
    let nodes: Vec<DeviceEntry> = app
        .remote
        .nodes
        .iter()
        .filter(|n| n.input && !audio::is_radio_codec(&n.device) && !audio::is_radio_codec(&audio::AudioDevice::Named(n.label.clone())))
        .cloned()
        .collect();
    let (level, sending, error, mic_name) = if remote {
        let st = app.services.audio_client_status();
        let s = st.unwrap_or_default();
        (s.mic_level_db, s.sending_mic, s.error, None)
    } else {
        let s = app.services.mic_status().unwrap_or_default();
        (s.level_db, s.sending, s.error, s.mic)
    };
    let refuses = remote && app.services.remote_refuses_mic();
    let mut changed = false;
    let tx = &mut app.services.config.tx_audio;
    egui::Grid::new("txmic").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label("Microphone");
        ui.horizontal(|ui| {
            changed |= crate::remote_tab::device_combo(ui, "txmic", &mut tx.mic, &nodes, true);
            if let Some(n) = &mic_name {
                ui.label(RichText::new(format!("using {n}")).weak());
            }
        });
        ui.end_row();
        ui.label("Mic gain");
        changed |= ui.add(Slider::new(&mut tx.mic_gain, 0.0..=4.0).step_by(0.05)).changed();
        ui.end_row();
        ui.label("Level");
        ui.horizontal(|ui| {
            let frac = ((level + 60.0) / 60.0).clamp(0.0, 1.0);
            let color = if refuses { AMBER } else { GREEN };
            widgets::Meter { label: "MIC", fraction: frac, text: format!("{level:.0} dB"), color, ticks: &[], red_from: Some(0.9), peak: None }
                .show(ui, 260.0);
            if sending && refuses {
                ui.label(RichText::new("✖ NOT SENT").color(AMBER).strong());
            } else if sending {
                ui.label(RichText::new("● ON AIR").color(RED).strong());
            }
        });
        ui.end_row();
    });
    if let Some(e) = error {
        ui.label(RichText::new(e).color(AMBER));
    }
    if refuses {
        ui.label(
            RichText::new(format!("⚠ {REFUSES_MIC}")).color(AMBER).strong(),
        );
    }
    if changed {
        app.services.apply_tx_audio();
        let _ = app.services.config.save();
    }
}

fn display(app: &mut App, ui: &mut Ui) {
    let before = app.services.config.ui.clone();
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("Display").strong().size(16.0).color(AMBER));
        let prefs = &mut app.services.config.ui;
        ui.checkbox(&mut prefs.show_waterfall, "Show the audio waterfall on the Operate page");
        ui.horizontal(|ui| {
            ui.label("Waterfall height");
            ui.add(egui::Slider::new(&mut prefs.wf_height, 80..=600).suffix(" px"));
        });
        ui.checkbox(&mut prefs.show_macros, "Show the macro buttons on the Operate page");
        ui.checkbox(&mut prefs.show_rotator, "Show rotator control (Yaesu G-800DXA/1000DXA/2800DXA on the radio's ROT jack)");
    });
    if app.services.config.ui != before {
        let _ = app.services.config.save();
    }
}

fn about(app: &App, ui: &mut Ui) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.add(egui::Image::new(&app.logo).fit_to_exact_size(egui::vec2(96.0, 96.0)));
            ui.vertical(|ui| {
                ui.label(RichText::new("RigHelm").strong().size(28.0).color(AMBER));
                ui.label(RichText::new(format!("The Linux alternative to Win4Yaesu for the Yaesu FTDX3000 · version {}", env!("CARGO_PKG_VERSION"))).size(14.0));
                ui.label(RichText::new("At the helm of your rig: control, sharing and remote operation.").color(DIM));
                ui.label(RichText::new(format!("Settings file: {}", ftdx3000_rig::Config::path().display())).color(DIM));
            });
        });
    });
}
