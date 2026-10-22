//! Keying CW from this computer: the ARM switch, a touchpad/mouse key pad,
//! keyboard paddles and a text line.

use eframe::egui::{self, Color32, PointerButton, RichText, Sense, Ui};
use ftdx3000_rig::cw::{Input, KeyerMode};

use crate::App;
use crate::widgets::{AMBER, DIM, RED};

#[derive(Default)]
pub struct CwKeyState {
    text: String,
    /// Last paddle state sent to the keyer.
    sent: (bool, bool),
}

/// The ARM switch (always shown on the Operate page).
pub fn arm_switch(app: &mut App, ui: &mut Ui) {
    let armed = app.services.cw().is_some();
    ui.horizontal(|ui| {
        ui.label(RichText::new("CW ").strong());
        let (text, fill, color) = if armed {
            ("CW KEY ARMED", RED, Color32::WHITE)
        } else {
            ("CW KEY OFF", Color32::from_rgb(40, 20, 20), Color32::LIGHT_GRAY)
        };
        let button = egui::Button::new(RichText::new(text).strong().size(15.0).color(color)).min_size(egui::vec2(170.0, 30.0)).fill(fill);
        let hover = if armed {
            "Click to disarm: the key pad, keyboard paddles and text sending stop."
        } else {
            "Arm to key CW from this computer (touchpad, keyboard or typed text). Puts the radio in CW with break-in."
        };
        if ui.add(button).on_hover_text(hover).clicked() {
            app.services.cw_arm(!armed);
            app.cw_key.sent = (false, false);
        }
        if let Some(why) = app.services.cw_disarmed.clone().filter(|_| !armed) {
            ui.label(RichText::new(format!("disarmed: {why}")).color(AMBER));
        }
    });
}

/// The key pad and text line, while armed.
pub fn panel(app: &mut App, ui: &mut Ui) {
    let Some(st) = app.services.cw().map(|k| k.status()) else { return };
    let mut rearm = false;
    let mut paddles = (false, false);
    let wpm = app.rig().state().key_speed.max(4);
    egui::Frame::group(ui.style()).stroke(egui::Stroke::new(2.0, RED)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new("● CW KEY ARMED").strong().color(RED));
            if !st.ready && st.error.is_none() {
                ui.label(RichText::new("opening the keying line…").color(AMBER));
            }
            ui.separator();
            let cfg = &mut app.services.config.cw;
            for m in [KeyerMode::Straight, KeyerMode::IambicA, KeyerMode::IambicB] {
                if ui.selectable_label(cfg.mode == m, m.label()).clicked() && cfg.mode != m {
                    cfg.mode = m;
                    rearm = true;
                }
            }
            ui.separator();
            ui.label(format!("{wpm} WPM"));
            ui.label(RichText::new("(CW group: Speed)").weak());
            ui.separator();
            if ui.checkbox(&mut cfg.sidetone, "Sidetone here").on_hover_text("Play a tone on this computer's speakers while keying").changed() {
                rearm = true;
            }
            if let Some(n) = &st.note {
                ui.label(RichText::new(n).color(AMBER));
            }
        });

        // The pad: press and hold. Left = dit (or the straight key), right = dah.
        let straight = app.services.config.cw.mode == KeyerMode::Straight;
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 90.0), Sense::click_and_drag());
        let (primary, secondary) = ui.input(|i| {
            let from_pad = i.pointer.press_origin().is_some_and(|p| rect.contains(p));
            (from_pad && i.pointer.button_down(PointerButton::Primary), from_pad && i.pointer.button_down(PointerButton::Secondary))
        });
        let keys = if ui.ctx().egui_wants_keyboard_input() {
            (false, false)
        } else {
            ui.input(|i| (i.key_down(egui::Key::OpenBracket), i.key_down(egui::Key::CloseBracket)))
        };
        paddles = (primary || keys.0, secondary || keys.1);
        if straight {
            paddles = (paddles.0 || paddles.1, false);
        }
        let fill = if st.down { RED } else if resp.hovered() { Color32::from_rgb(50, 30, 30) } else { Color32::from_rgb(30, 22, 22) };
        ui.painter().rect_filled(rect, 8.0, fill);
        let hint = if straight {
            "KEY PAD — press and hold to key (touchpad, mouse, or hold [ )"
        } else {
            "KEY PAD — left button = dit · right button (two-finger press) = dah · keyboard [ = dit, ] = dah"
        };
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, hint, egui::FontId::proportional(15.0), if st.down { Color32::WHITE } else { DIM });

        ui.horizontal(|ui| {
            ui.label("Send text");
            let edit = ui.add(egui::TextEdit::singleline(&mut app.cw_key.text).desired_width(360.0).hint_text("CQ CQ DE ..."));
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button(RichText::new("Send").strong()).on_hover_text("Transmits the text in CW").clicked() || enter) && !app.cw_key.text.trim().is_empty() {
                if let Some(k) = app.services.cw() {
                    k.input(Input::Text(app.cw_key.text.clone()));
                }
                app.cw_key.text.clear();
            }
            if ui.button(RichText::new("Stop").color(RED)).on_hover_text("Stop sending and let go of the key (Esc)").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                if let Some(k) = app.services.cw() {
                    k.input(Input::Stop);
                }
            }
            if st.sending_text {
                ui.label(RichText::new("sending…").color(AMBER));
            }
        });
    });
    if paddles != app.cw_key.sent {
        if let Some(k) = app.services.cw() {
            k.input(Input::Paddles { dit: paddles.0, dah: paddles.1 });
        }
        app.cw_key.sent = paddles;
    }
    if rearm {
        let _ = app.services.config.save();
        app.services.cw_arm(true);
        app.cw_key.sent = (false, false);
    }
    // Keep the pad and the status fresh while keying.
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
}
