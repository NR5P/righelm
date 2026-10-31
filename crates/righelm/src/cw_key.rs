//! Keying CW from this computer: the RADIO KEY / COMPUTER KEY switch, a
//! touchpad/mouse key pad, keyboard paddles and a text line.

use eframe::egui::{self, Color32, PointerButton, RichText, Sense, Ui};
use ftdx3000_rig::cw::{Input, KeyerMode};

use crate::App;
use crate::widgets::{AMBER, DIM, RED, ink};

#[derive(Default)]
pub struct CwKeyState {
    text: String,
    /// Last paddle state sent to the keyer.
    sent: (bool, bool),
}

const RADIO_KEY_HOVER: &str = "CW from the key or paddle plugged into the radio (always the default). Click to key from this computer instead.";
const COMPUTER_KEY_HOVER: &str = "CW from this computer (touchpad, keyboard, typed text) is live. Click to go back to the radio's key.";

/// The key switch on the Operate page: RADIO KEY (default) or COMPUTER KEY.
pub fn arm_button(app: &mut App, ui: &mut Ui, size: egui::Vec2) {
    let armed = app.services.cw().is_some();
    let (text, fill, color) = if armed {
        ("COMPUTER KEY", RED, Color32::WHITE)
    } else {
        ("RADIO KEY", Color32::from_rgb(40, 34, 20), Color32::LIGHT_GRAY)
    };
    let button = egui::Button::new(RichText::new(text).strong().size(15.0).color(color)).min_size(size).fill(fill);
    if ui.add(button).on_hover_text(if armed { COMPUTER_KEY_HOVER } else { RADIO_KEY_HOVER }).clicked() {
        set(app, !armed);
    }
}

/// Why the computer key let go by itself, if it did.
pub fn disarm_note(app: &App) -> Option<String> {
    app.services.cw_disarmed.clone().filter(|_| app.services.cw().is_none()).map(|why| format!("back to radio key: {why}"))
}

fn set(app: &mut App, computer: bool) {
    app.services.cw_arm(computer);
    app.cw_key.sent = (false, false);
}

/// The same switch in Settings, with the keyer options.
pub fn settings_panel(app: &mut App, ui: &mut Ui) {
    let armed = app.services.cw().is_some();
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("CW key").strong().size(16.0).color(ink(AMBER)));
        ui.horizontal(|ui| {
            let choice = |text: &str, selected: bool, on: Color32| {
                let color = if !selected { Color32::LIGHT_GRAY } else if on == AMBER { Color32::BLACK } else { Color32::WHITE };
                egui::Button::new(RichText::new(text).size(16.0).strong().color(color))
                    .fill(if selected { on } else { Color32::from_rgb(40, 34, 20) })
                    .min_size(egui::vec2(240.0, 40.0))
            };
            if ui.add(choice("Radio's key / paddle", !armed, AMBER)).on_hover_text(RADIO_KEY_HOVER).clicked() && armed {
                set(app, false);
            }
            if ui.add(choice("Computer key", armed, RED)).on_hover_text(COMPUTER_KEY_HOVER).clicked() && !armed {
                set(app, true);
            }
        });
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Computer key type");
            let cfg = &mut app.services.config.cw;
            for m in [KeyerMode::Straight, KeyerMode::IambicA, KeyerMode::IambicB] {
                changed |= ui.selectable_value(&mut cfg.mode, m, m.label()).changed();
            }
            ui.separator();
            changed |= ui.checkbox(&mut cfg.sidetone, "Sidetone on this computer").changed();
        });
        ui.label(
            RichText::new(
                "RigHelm always starts on the radio's key. The computer key uses the touchpad or mouse (left button = dit, right button = dah; \
                 on a ThinkPad the buttons above the touchpad make good paddles), [ and ] on the keyboard, or typed text.",
            )
            .weak(),
        );
        if changed {
            let _ = app.services.config.save();
            if armed {
                set(app, true);
            }
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
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("● COMPUTER KEY").strong().color(ink(RED)));
            if !st.ready && st.error.is_none() {
                ui.label(RichText::new("opening the keying line…").color(ink(AMBER)));
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
            ui.add(egui::Label::new(format!("{wpm} WPM")).extend());
            ui.add(egui::Label::new(RichText::new("(CW group: Speed)").weak()).extend());
            ui.separator();
            if ui.checkbox(&mut cfg.sidetone, "Sidetone here").on_hover_text("Play a tone on this computer's speakers while keying").changed() {
                rearm = true;
            }
            if let Some(n) = &st.note {
                ui.label(RichText::new(n).color(ink(AMBER)));
            }
        });

        // The pad: press and hold. Left = dit (or the straight key), right = dah.
        let straight = app.services.config.cw.mode == KeyerMode::Straight;
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 110.0), Sense::click_and_drag());
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
        let idle = if resp.hovered() { Color32::from_rgb(50, 30, 30) } else { Color32::from_rgb(30, 22, 22) };
        let lit = |on: bool| if on { RED } else { idle };
        let text_color = |on: bool| if on { Color32::WHITE } else { DIM };
        let big = egui::FontId::proportional(22.0);
        let small = egui::FontId::proportional(13.0);
        let p = ui.painter();
        if straight {
            p.rect_filled(rect, 8.0, lit(st.down));
            p.text(rect.center() - egui::vec2(0.0, 12.0), egui::Align2::CENTER_CENTER, "KEY", big, text_color(st.down));
            p.text(rect.center() + egui::vec2(0.0, 16.0), egui::Align2::CENTER_CENTER, "press and hold (touchpad, mouse or [ )", small, text_color(st.down));
        } else {
            // Two paddles: each half lights while its paddle is pressed.
            let (left, right) = rect.split_left_right_at_fraction(0.5);
            for (half, on, name, how) in [
                (left.shrink2(egui::vec2(2.0, 0.0)), paddles.0, "DIT", "left button  ·  [ key"),
                (right.shrink2(egui::vec2(2.0, 0.0)), paddles.1, "DAH", "right button  ·  ] key"),
            ] {
                p.rect_filled(half, 8.0, lit(on));
                p.text(half.center() - egui::vec2(0.0, 12.0), egui::Align2::CENTER_CENTER, name, big.clone(), text_color(on));
                p.text(half.center() + egui::vec2(0.0, 16.0), egui::Align2::CENTER_CENTER, how, small.clone(), text_color(on));
            }
        }

        ui.horizontal(|ui| {
            ui.label("Send text");
            let edit = ui.add(egui::TextEdit::singleline(&mut app.cw_key.text).desired_width((ui.available_width() - 190.0).max(120.0)).hint_text("CQ CQ DE ..."));
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button(RichText::new("Send").strong()).on_hover_text("Transmits the text in CW").clicked() || enter) && !app.cw_key.text.trim().is_empty() {
                if let Some(k) = app.services.cw() {
                    k.input(Input::Text(app.cw_key.text.clone()));
                }
                app.cw_key.text.clear();
            }
            if ui.button(RichText::new("Stop").color(ink(RED))).on_hover_text("Stop sending and let go of the key (Esc)").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                if let Some(k) = app.services.cw() {
                    k.input(Input::Stop);
                }
            }
            if st.sending_text {
                ui.label(RichText::new("sending…").color(ink(AMBER)));
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
