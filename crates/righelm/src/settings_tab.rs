//! Display preferences and about.

use eframe::egui::{self, RichText, Ui};

use crate::App;
use crate::widgets::{AMBER, DIM};

pub fn show(app: &mut App, ui: &mut Ui) {
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
    ui.add_space(12.0);
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
