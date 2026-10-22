//! The main operating panel.

use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, ComboBox, RichText, Slider, Ui};
use ftdx3000_cat::types::{self, BANDS, Meter, Mode, WidthClass, interpolate};
use ftdx3000_cat::{RadioState, set_freq_cmd, set_mode_cmd};
use ftdx3000_rig::Rig;

use crate::App;
use crate::widgets::{self, AMBER, CYAN, DIM, FreqAction, GREEN, RED, button, led_button};

pub struct OperateState {
    /// Frequency being typed: (vfo_b, text).
    freq_edit: Option<(bool, String)>,
    peaks: std::collections::HashMap<Meter, (f32, Instant)>,
    keyer_text: [String; 5],
    keyer_loaded: [bool; 5],
}

impl Default for OperateState {
    fn default() -> Self {
        Self {
            freq_edit: None,
            peaks: Default::default(),
            keyer_text: Default::default(),
            keyer_loaded: [false; 5],
        }
    }
}

/// The transmit-audio switch, where the operator looks: one click between
/// the radio's own microphone and this computer's.
fn mic_switch(app: &mut App, ui: &mut Ui) {
    let computer = app.services.config.tx_audio.computer_mic;
    let (sending, level, error) = if app.services.config.engine.remote.is_some() {
        let st = app.services.audio_client_status().unwrap_or_default();
        (st.sending_mic, st.mic_level_db, st.error)
    } else {
        let st = app.services.mic_status().unwrap_or_default();
        (st.sending, st.level_db, st.error)
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new("MIC").strong());
        let text = if computer { "COMPUTER MIC" } else { "RADIO MIC" };
        let button = egui::Button::new(RichText::new(text).strong().size(15.0).color(if computer { Color32::BLACK } else { Color32::LIGHT_GRAY }))
            .min_size(egui::vec2(170.0, 30.0))
            .fill(if computer { AMBER } else { Color32::from_rgb(40, 34, 20) });
        let hover = if computer {
            "Talking into this computer's microphone. Click to go back to the radio's own mic."
        } else {
            "Talking into the radio's own mic. Click to use this computer's microphone."
        };
        if ui.add(button).on_hover_text(hover).clicked() {
            app.services.set_computer_mic(!computer);
        }
        if computer {
            let frac = ((level + 60.0) / 60.0).clamp(0.0, 1.0);
            widgets::Meter { label: "", fraction: frac, text: String::new(), color: GREEN, ticks: &[], red_from: Some(0.9), peak: None }.show(ui, 90.0);
            if sending {
                ui.label(RichText::new("● ON AIR").color(RED).strong());
            }
            if let Some(e) = error {
                ui.label(RichText::new("⚠").color(AMBER)).on_hover_text(e);
            }
        }
    });
}

const STEPS: [(i64, &str); 6] = [(1, "1 Hz"), (10, "10 Hz"), (100, "100 Hz"), (1000, "1 kHz"), (5000, "5 kHz"), (10_000, "10 kHz")];

pub fn show(app: &mut App, ui: &mut Ui) {
    let rig = app.rig().clone();
    let s = rig.state().clone();
    let live = rig.link().is_open() && s.power_on != Some(false);
    if live && !ui.ctx().egui_wants_keyboard_input() {
        keyboard(ui, &rig, &s, app.services.config.ui.tune_step);
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_enabled_ui(live, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| vfo_block(app, ui, &rig, &s, live));
                ui.add_space(16.0);
                ui.vertical(|ui| meters_block(app, ui, &rig, &s));
            });
            if app.services.cw().is_some() {
                ui.add_space(6.0);
                crate::cw_key::panel(app, ui);
            }
            ui.add_space(6.0);
            bands_and_modes(ui, &rig, &s);
            ui.add_space(4.0);
            filter_strip(ui, &rig, &s);
            ui.add_space(4.0);
            crate::waterfall::show(app, ui, &rig, &s);
            ui.add_space(6.0);
            ui.columns(3, |cols| {
                receiver_group(&mut cols[0], &rig, &s);
                dsp_group(&mut cols[1], &rig, &s);
                transmitter_group(&mut cols[2], &rig, &s);
            });
            ui.add_space(6.0);
            ui.columns(2, |cols| {
                clarifier_group(&mut cols[0], &rig, &s);
                if app.services.config.ui.show_macros {
                    cols[0].add_space(6.0);
                    macro_group(&mut cols[0], app, &rig);
                }
                if app.services.config.ui.show_rotator {
                    cols[0].add_space(6.0);
                    rotator_group(&mut cols[0], &rig, &s);
                }
                cw_group(&mut cols[1], &mut app.operate, &rig, &s);
            });
        });
        if !live {
            ui.add_space(10.0);
            ui.label(RichText::new("Not connected to the radio. Use Connect (top right) or check Ports & sharing.").color(AMBER));
        }
    });
}

/// Arrow keys tune VFO-A by the step; PageUp/PageDown change band.
fn keyboard(ui: &mut Ui, rig: &Rig, s: &RadioState, step: i64) {
    let (up, down, pgup, pgdn) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::ArrowRight),
            i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::ArrowLeft),
            i.key_pressed(egui::Key::PageUp),
            i.key_pressed(egui::Key::PageDown),
        )
    });
    let tune = |d: i64| {
        let hz = (s.freq_a as i64 + d).clamp(types::FREQ_MIN as i64, types::FREQ_MAX as i64) as u64;
        rig.send(set_freq_cmd(false, hz));
    };
    if up {
        tune(step);
    }
    if down {
        tune(-step);
    }
    if pgup {
        rig.send("BU0;");
    }
    if pgdn {
        rig.send("BD0;");
    }
}

fn badge(ui: &mut Ui, text: &str, on: bool, color: Color32) {
    let t = RichText::new(text).strong().size(12.0);
    if on {
        ui.label(t.color(Color32::BLACK).background_color(color));
    } else {
        ui.label(t.color(DIM));
    }
}

fn vfo_line(app: &mut App, ui: &mut Ui, rig: &Rig, s: &RadioState, b: bool, live: bool) {
    let (freq, mode) = if b { (s.freq_b, s.mode_b) } else { (s.freq_a, s.mode_a) };
    let rx = s.rx_on_b() == b;
    let tx = (s.tx_vfo == 1) == b;
    ui.horizontal(|ui| {
        ui.label(RichText::new(if b { "VFO-B" } else { "VFO-A" }).strong().color(if b { CYAN } else { AMBER }));
        badge(ui, "RX", rx, GREEN);
        badge(ui, "TX", tx, if s.transmitting() && tx { RED } else { AMBER });
        if let Some(m) = mode {
            ui.label(RichText::new(m.label()).strong().size(15.0));
        }
        if let Some(band) = types::band_of(freq) {
            ui.label(RichText::new(format!("{} m", band.label)).weak());
        }
        if !b {
            if s.info_a.mem_mode != 0 {
                ui.label(RichText::new(format!("MEM {:03}", s.info_a.channel)).color(CYAN));
            }
            if s.lock & 1 == 1 {
                ui.label(RichText::new("LOCK").color(RED));
            }
        }
    });
    let step = app.services.config.ui.tune_step;
    let op = &mut app.operate;
    if let Some((eb, text)) = &mut op.freq_edit {
        if *eb == b {
            let mut done = None;
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(text).desired_width(200.0).font(egui::FontId::monospace(if b { 22.0 } else { 34.0 })));
                r.request_focus();
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    done = Some(true);
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) || ui.button("Cancel").clicked() {
                    done = Some(false);
                }
                ui.label(RichText::new("e.g. 14.074, 7074.5, 3573k").weak());
            });
            match done {
                Some(true) => {
                    if let Some(hz) = widgets::parse_freq(text) {
                        if (types::FREQ_MIN..=types::FREQ_MAX).contains(&hz) {
                            rig.send(set_freq_cmd(b, hz));
                        }
                    }
                    op.freq_edit = None;
                }
                Some(false) => op.freq_edit = None,
                None => {}
            }
            return;
        }
    }
    let size = if b { 30.0 } else { 56.0 };
    match widgets::freq_display(ui, freq, size, if b { CYAN } else { AMBER }, step, live && !(s.lock & if b { 2 } else { 1 } != 0)) {
        FreqAction::Step(d) => {
            let hz = (freq as i64 + d).clamp(types::FREQ_MIN as i64, types::FREQ_MAX as i64) as u64;
            rig.send(set_freq_cmd(b, hz));
        }
        FreqAction::Edit => op.freq_edit = Some((b, String::new())),
        FreqAction::None => {}
    }
}

fn vfo_block(app: &mut App, ui: &mut Ui, rig: &Rig, s: &RadioState, live: bool) {
    vfo_line(app, ui, rig, s, false, live);
    ui.add_space(4.0);
    vfo_line(app, ui, rig, s, true, live);
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if button(ui, "A ▸ B") {
            rig.send("AB;");
        }
        if button(ui, "B ▸ A") {
            rig.send("BA;");
        }
        if button(ui, "A ⇄ B") {
            rig.send("SV;");
        }
        let split = s.split();
        if led_button(ui, split, "SPLIT", AMBER) {
            // Transmit on the other VFO than the receiver (or back on the same one).
            let tx_b = if split { s.rx_on_b() } else { !s.rx_on_b() };
            rig.send(if tx_b { "FT3;" } else { "FT2;" });
        }
        if button(ui, "Q.SPLIT") {
            rig.send("QS;");
        }
    });
    ui.horizontal(|ui| {
        ui.label("RX on");
        if ui.selectable_label(!s.rx_on_b(), "A").clicked() {
            rig.send("FR0;");
        }
        if ui.selectable_label(s.rx_on_b(), "B").clicked() {
            rig.send("FR4;");
        }
        ui.separator();
        if led_button(ui, s.lock & 1 == 1, "LOCK", RED) {
            rig.send(if s.lock & 1 == 1 { "LK0;" } else { "LK1;" });
        }
        if led_button(ui, s.fast & 1 == 1, "FAST", AMBER) {
            rig.send(if s.fast & 1 == 1 { "FS0;" } else { "FS1;" });
        }
        ui.separator();
        ui.label("Step");
        let step = &mut app.services.config.ui.tune_step;
        ComboBox::from_id_salt("step")
            .width(70.0)
            .selected_text(STEPS.iter().find(|x| x.0 == *step).map(|x| x.1).unwrap_or("?"))
            .show_ui(ui, |ui| {
                for (v, l) in STEPS {
                    ui.selectable_value(step, v, l);
                }
            });
    });
    ui.label(RichText::new("Scroll over a digit to tune it · scroll elsewhere or drag to tune by step · double-click to type").weak().size(11.0));
}

fn meters_block(app: &mut App, ui: &mut Ui, rig: &Rig, s: &RadioState) {
    let w = 380.0;
    let tx = s.transmitting();
    let now = Instant::now();
    let mut peak = |m: Meter, f: f32| -> f32 {
        let e = app.operate.peaks.entry(m).or_insert((0.0, now));
        if f >= e.0 || now.duration_since(e.1) > Duration::from_millis(1500) {
            *e = (f, now);
        }
        e.0
    };
    if !tx {
        let raw = s.s_meter;
        let f = raw as f32 / 255.0;
        let pk = peak(Meter::S, f);
        widgets::Meter {
            label: "S",
            fraction: f,
            text: types::s_meter_text(raw),
            color: GREEN,
            ticks: &[(0.0, "0"), (40.0 / 255.0, "3"), (80.0 / 255.0, "6"), (130.0 / 255.0, "9"), (172.0 / 255.0, "+20"), (220.0 / 255.0, "+40"), (1.0, "+60")],
            red_from: Some(130.0 / 255.0),
            peak: Some(pk),
        }
        .show(ui, w);
    }
    let get = |m: Meter| s.meter(m).unwrap_or(0);
    let po_w = interpolate(&types::PO_CAL, get(Meter::Po));
    let pf = if tx { po_w / 100.0 } else { 0.0 };
    let pk = peak(Meter::Po, pf);
    widgets::Meter {
        label: "PO",
        fraction: pf,
        text: if tx { format!("{po_w:.0} W") } else { "—".into() },
        color: AMBER,
        ticks: &[(0.0, "0"), (0.25, "25"), (0.5, "50"), (0.75, "75"), (1.0, "100")],
        red_from: None,
        peak: tx.then_some(pk),
    }
    .show(ui, w);
    let swr = interpolate(&types::SWR_CAL, get(Meter::Swr));
    widgets::Meter {
        label: "SWR",
        fraction: if tx { get(Meter::Swr) as f32 / 255.0 } else { 0.0 },
        text: if tx { format!("{swr:.1}:1") } else { "—".into() },
        color: CYAN,
        ticks: &[(0.0, "1"), (52.0 / 255.0, "1.5"), (89.0 / 255.0, "2"), (126.0 / 255.0, "3"), (1.0, "∞")],
        red_from: Some(89.0 / 255.0),
        peak: None,
    }
    .show(ui, w);
    // Always shown (blank on receive) so the buttons below never move when keying.
    let txt = |t: String| if tx { t } else { "—".into() };
    let alc = if tx { get(Meter::Alc) as f32 / 255.0 } else { 0.0 };
    widgets::Meter { label: "ALC", fraction: alc, text: txt(format!("{:.0}%", alc * 100.0)), color: CYAN, ticks: &[], red_from: Some(0.5), peak: None }
        .show(ui, w);
    let comp = if tx { get(Meter::Comp) as f32 / 255.0 } else { 0.0 };
    widgets::Meter { label: "COMP", fraction: comp, text: txt(format!("{:.0} dB", comp * 30.0)), color: CYAN, ticks: &[], red_from: None, peak: None }
        .show(ui, w);
    let id = if tx { interpolate(&types::ID_CAL, get(Meter::Id)) } else { 0.0 };
    widgets::Meter { label: "ID", fraction: id / 25.0, text: txt(format!("{id:.1} A")), color: CYAN, ticks: &[], red_from: None, peak: None }
        .show(ui, w);
    let vdd = interpolate(&types::VDD_CAL, get(Meter::Vdd));
    if s.meter(Meter::Vdd).is_some() {
        ui.label(RichText::new(format!("VDD {vdd:.1} V   ·   meter calibration is approximate")).weak().size(11.0));
    }

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let txing = s.transmitting();
        let mox = egui::Button::new(RichText::new(if txing { "TX  (click to stop)" } else { "MOX / PTT" }).strong().size(16.0).color(if txing { Color32::WHITE } else { RED }))
            .min_size(egui::vec2(170.0, 36.0))
            .fill(if txing { RED } else { Color32::from_rgb(40, 20, 20) });
        if ui.add(mox).on_hover_text("Transmits! Uses CAT TX (TX1;/TX0;). Or hold the space bar to talk (Settings).").clicked() {
            rig.send(if txing { "TX0;" } else { "TX1;" });
        }
        let tuning = s.tuner == 2;
        if ui
            .add(egui::Button::new(RichText::new("TUNE").strong()).min_size(egui::vec2(70.0, 36.0)).fill(if tuning { AMBER } else { Color32::from_rgb(40, 34, 20) }))
            .on_hover_text("Starts the antenna tuner — transmits a carrier")
            .clicked()
        {
            rig.send("AC002;");
        }
        if led_button(ui, s.tuner != 0, "ATU", AMBER) {
            rig.send(if s.tuner != 0 { "AC000;" } else { "AC001;" });
        }
        if s.radio_info.get(&0) == Some(&true) {
            ui.label(RichText::new("HI-SWR").strong().color(RED));
        }
    });
    mic_switch(app, ui);
    crate::cw_key::arm_switch(app, ui);
    ui.horizontal(|ui| {
        ui.label("Antenna");
        for a in 1..=3u8 {
            if ui.selectable_label(s.antenna == a, format!("ANT {a}")).clicked() {
                rig.send(format!("AN0{a};"));
            }
        }
        ui.separator();
        ui.label("Radio meter");
        let names = [(0, "COMP"), (1, "ALC"), (2, "PO"), (3, "SWR"), (4, "ID"), (5, "VDD")];
        ComboBox::from_id_salt("ms")
            .width(70.0)
            .selected_text(names.iter().find(|n| n.0 == s.meter_sw).map(|n| n.1).unwrap_or("PO"))
            .show_ui(ui, |ui| {
                for (c, n) in names {
                    if ui.selectable_label(s.meter_sw == c, n).clicked() {
                        rig.send(format!("MS{c};"));
                    }
                }
            });
    });
}

fn bands_and_modes(ui: &mut Ui, rig: &Rig, s: &RadioState) {
    let cur = types::band_of(s.freq_a).map(|b| b.code);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Band").strong());
        for b in BANDS {
            if b.code == 2 {
                continue; // no band key for 60 m; tune it directly
            }
            let on = cur == Some(b.code);
            if ui.add(egui::Button::new(RichText::new(b.label).strong()).min_size(egui::vec2(40.0, 26.0)).selected(on)).clicked() {
                rig.send(format!("BS{:02};", b.code));
            }
        }
        if button(ui, "◀ Band") {
            rig.send("BD0;");
        }
        if button(ui, "Band ▶") {
            rig.send("BU0;");
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Mode").strong());
        for m in Mode::ALL {
            let on = s.mode_a == Some(m);
            if ui.add(egui::Button::new(RichText::new(m.label()).strong()).min_size(egui::vec2(52.0, 26.0)).selected(on)).clicked() {
                rig.send(set_mode_cmd(m));
            }
        }
    });
}

/// A slider bound to a radio value: sends `fmt(v)` whenever it moves.
/// A slider bound to a radio value: sends `fmt(v)` whenever it moves.
/// Scrolling over it (wheel or trackpad) adjusts it too, in about 1% steps.
fn slider(ui: &mut Ui, rig: &Rig, label: &str, value: i32, range: std::ops::RangeInclusive<i32>, fmt: impl Fn(i32) -> String) {
    let mut v = value;
    let (lo, hi) = (*range.start(), *range.end());
    ui.label(label);
    let r = ui.add(Slider::new(&mut v, range));
    let steps = widgets::scroll_steps(ui, &r, 12.0);
    if steps != 0 {
        let inc = ((hi - lo) / 100).max(1);
        rig.send(fmt((value + steps as i32 * inc).clamp(lo, hi)));
    } else if r.changed() {
        rig.send(fmt(v));
    }
    ui.end_row();
}

fn choice(ui: &mut Ui, rig: &Rig, label: &str, id: &str, current: Option<&str>, options: &[(&str, String)]) {
    ui.label(label);
    ComboBox::from_id_salt(id).width(150.0).selected_text(current.unwrap_or("—")).show_ui(ui, |ui| {
        for (l, cmd) in options {
            if ui.selectable_label(current == Some(*l), *l).clicked() {
                rig.send(cmd);
            }
        }
    });
    ui.end_row();
}

fn group(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).strong().size(15.0).color(AMBER));
        ui.add_space(2.0);
        body(ui);
    });
}

/// One width control across the narrow and wide filter tables, IF shift, and
/// a picture of the resulting audio passband.
fn filter_strip(ui: &mut Ui, rig: &Rig, s: &RadioState) {
    let Some(mode) = s.mode_a else { return };
    let class = s.width_class(mode);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new("FILTER").strong().size(15.0).color(AMBER));
            ui.add_space(6.0);
            if class == WidthClass::Fixed {
                ui.label(RichText::new(format!("{} width is fixed", mode.label())).weak());
                if led_button(ui, s.narrow, "NAR", AMBER) {
                    rig.send(format!("NA0{};", (!s.narrow) as u8));
                }
            } else {
                // Narrow and wide tables as one list, narrowest first.
                let mut all: Vec<(u32, bool, u8)> = types::width_choices(class, true)
                    .into_iter()
                    .filter(|(i, _)| *i != 0)
                    .map(|(i, w)| (w, true, i))
                    .chain(types::width_choices(class, false).into_iter().filter(|(i, _)| *i != 0).map(|(i, w)| (w, false, i)))
                    .collect();
                all.sort_by_key(|x| (x.0, !x.1));
                all.dedup_by_key(|x| x.0);
                let cur = s.width_hz().unwrap_or(0);
                let mut pos = all
                    .iter()
                    .position(|x| x.1 == s.narrow && x.2 == s.width_index)
                    .unwrap_or_else(|| all.iter().enumerate().min_by_key(|(_, x)| (x.0 as i64 - cur as i64).abs()).map(|(i, _)| i).unwrap_or(0));
                let set = |e: (u32, bool, u8)| {
                    if e.1 != s.narrow {
                        rig.send(format!("NA0{};", e.1 as u8));
                    }
                    rig.send(format!("SH0{:02};", e.2));
                };
                ui.label("Width");
                ui.label(RichText::new(format!("{cur:>4} Hz")).monospace().size(20.0).color(AMBER));
                if button(ui, "−") && pos > 0 {
                    set(all[pos - 1]);
                }
                let was = pos;
                let r = ui.add_sized([220.0, 20.0], Slider::new(&mut pos, 0..=all.len().saturating_sub(1)).show_value(false));
                let steps = widgets::scroll_steps(ui, &r, 16.0);
                if steps != 0 {
                    set(all[(was as i64 + steps).clamp(0, all.len() as i64 - 1) as usize]);
                } else if r.changed() {
                    set(all[pos]);
                }
                if button(ui, "+") && pos + 1 < all.len() {
                    set(all[pos + 1]);
                }
                if led_button(ui, s.narrow, "NAR", AMBER) {
                    rig.send(format!("NA0{};", (!s.narrow) as u8));
                }
            }
            ui.separator();
            ui.label("IF shift");
            let mut shift = s.if_shift;
            let r = ui.add_sized([160.0, 20.0], Slider::new(&mut shift, -1000..=1000).step_by(20.0).show_value(false));
            let steps = widgets::scroll_steps(ui, &r, 12.0);
            if steps != 0 {
                shift = (s.if_shift + steps as i32 * 20).clamp(-1000, 1000);
            }
            if steps != 0 || r.changed() {
                rig.send(format!("IS0{}{:04};", if shift < 0 { '-' } else { '+' }, shift.abs()));
            }
            ui.label(RichText::new(format!("{:+5} Hz", s.if_shift)).monospace());
            if ui.small_button("0").on_hover_text("Center the IF shift").clicked() {
                rig.send("IS0+0000;");
            }
            ui.separator();
            passband(ui, s, mode, class);
        });
    });
}

/// Approximate audio passband: width and IF shift around the mode's centre
/// (the CW pitch in CW, ~1.5 kHz otherwise).
fn passband(ui: &mut Ui, s: &RadioState, mode: Mode, class: WidthClass) {
    use egui::{Align2, FontId, Pos2, Sense, Stroke, vec2};
    let (rect, _) = ui.allocate_exact_size(vec2(260.0, 38.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, widgets::PANEL);
    let span = 4000.0_f32;
    let x = |hz: f32| rect.left() + 6.0 + (rect.width() - 12.0) * (hz / span).clamp(0.0, 1.0);
    let base = rect.bottom() - 12.0;
    p.line_segment([Pos2::new(x(0.0), base), Pos2::new(x(span), base)], Stroke::new(1.0, Color32::GRAY));
    for k in [1000.0, 2000.0, 3000.0] {
        p.line_segment([Pos2::new(x(k), base), Pos2::new(x(k), base + 3.0)], Stroke::new(1.0, Color32::GRAY));
        p.text(Pos2::new(x(k), base + 3.0), Align2::CENTER_TOP, format!("{}k", k as u32 / 1000), FontId::proportional(9.0), Color32::GRAY);
    }
    // AM/FM: roughly 3 kHz of audio either way.
    let width = match class {
        WidthClass::Fixed => 3000.0,
        _ => s.width_hz().unwrap_or(2400) as f32,
    };
    let centre = if mode.is_cw() { 300.0 + 10.0 * s.key_pitch as f32 } else { 1500.0 } + s.if_shift as f32;
    let (lo, hi) = (centre - width / 2.0, centre + width / 2.0);
    let top = rect.top() + 5.0;
    let slope = 6.0;
    let shape = vec![
        Pos2::new(x(lo) - slope, base),
        Pos2::new(x(lo), top),
        Pos2::new(x(hi), top),
        Pos2::new(x(hi) + slope, base),
    ];
    p.add(egui::Shape::convex_polygon(shape, AMBER.gamma_multiply(0.35), Stroke::new(1.5, AMBER)));
    if mode.is_cw() {
        let pitch = 300.0 + 10.0 * s.key_pitch as f32;
        p.line_segment([Pos2::new(x(pitch), top - 2.0), Pos2::new(x(pitch), base)], Stroke::new(1.0, CYAN));
    }
}

fn receiver_group(ui: &mut Ui, rig: &Rig, s: &RadioState) {
    group(ui, "Receiver", |ui| {
        egui::Grid::new("rx").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            slider(ui, rig, "AF gain", s.af_gain as i32, 0..=255, |v| format!("AG0{v:03};"));
            slider(ui, rig, "RF gain", s.rf_gain as i32, 0..=255, |v| format!("RG0{v:03};"));
            slider(ui, rig, "Squelch", s.squelch as i32, 0..=255, |v| format!("SQ0{v:03};"));
            let agc: Vec<(&str, String)> = types::AGC_SET.iter().map(|(c, l)| (*l, format!("GT0{c};"))).collect();
            let cur_agc = types::agc_label(s.agc);
            let cur_agc = if cur_agc.starts_with("AUTO") { "AUTO" } else { cur_agc };
            choice(ui, rig, "AGC", "agc", Some(cur_agc), &agc);
            ui.label("");
            ui.label(RichText::new(format!("radio reports {}", types::agc_label(s.agc))).weak().size(11.0));
            ui.end_row();
            ui.label("Front end");
            ui.horizontal(|ui| {
                for (c, l) in [(0, "IPO"), (1, "AMP1"), (2, "AMP2")] {
                    if ui.selectable_label(s.preamp == c, l).clicked() {
                        rig.send(format!("PA0{c};"));
                    }
                }
            });
            ui.end_row();
            ui.label("Attenuator");
            ui.horizontal(|ui| {
                for (c, l) in [(0, "OFF"), (1, "6"), (2, "12"), (3, "18")] {
                    if ui.selectable_label(s.att == c, l).clicked() {
                        rig.send(format!("RA0{c};"));
                    }
                }
            });
            ui.end_row();
            let roof: Vec<(&str, String)> = types::ROOFING_SET.iter().map(|(c, l)| (*l, format!("RF0{c};"))).collect();
            let cur = types::roofing_label(s.roofing);
            ui.label("Roofing");
            ComboBox::from_id_salt("roof").width(150.0).selected_text(cur.unwrap_or("—")).show_ui(ui, |ui| {
                for (l, cmd) in &roof {
                    let selected = cur.is_some_and(|c| c.trim_start_matches("AUTO ") == *l) || (*l == "AUTO" && types::roofing_is_auto(s.roofing));
                    if ui.selectable_label(selected, *l).clicked() {
                        rig.send(cmd);
                    }
                }
            });
            ui.end_row();
        });
    });
}

fn dsp_group(ui: &mut Ui, rig: &Rig, s: &RadioState) {
    group(ui, "Filters & DSP", |ui| {
        egui::Grid::new("dsp").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label("Noise blanker");
            ui.horizontal(|ui| {
                for (c, l) in [(0, "OFF"), (1, "NB"), (2, "WIDE")] {
                    if ui.selectable_label(s.nb == c, l).clicked() {
                        rig.send(format!("NB0{c};"));
                    }
                }
            });
            ui.end_row();
            slider(ui, rig, "NB level", s.nb_level as i32, 0..=100, |v| format!("NL0{v:03};"));
            ui.label("DNR");
            ui.horizontal(|ui| {
                if led_button(ui, s.nr, "DNR", AMBER) {
                    rig.send(format!("NR0{};", (!s.nr) as u8));
                }
                let mut lvl = s.nr_level.max(1) as i32;
                let r = ui.add(Slider::new(&mut lvl, 1..=15));
                let steps = widgets::scroll_steps(ui, &r, 16.0);
                if steps != 0 {
                    lvl = (s.nr_level.max(1) as i32 + steps as i32).clamp(1, 15);
                }
                if steps != 0 || r.changed() {
                    rig.send(format!("RL0{lvl:02};"));
                }
            });
            ui.end_row();
            ui.label("Notch");
            ui.horizontal(|ui| {
                if led_button(ui, s.auto_notch, "DNF", AMBER) {
                    rig.send(format!("BC0{};", (!s.auto_notch) as u8));
                }
                if led_button(ui, s.manual_notch, "NOTCH", AMBER) {
                    rig.send(format!("BP00{:03};", (!s.manual_notch) as u8));
                }
            });
            ui.end_row();
            slider(ui, rig, "Notch freq", s.notch_freq.max(1) as i32 * 10, 10..=4000, |v| format!("BP01{:03};", (v / 10).clamp(1, 400)));
            ui.label("Contour / APF");
            ui.horizontal(|ui| {
                for (c, l) in [(0, "OFF"), (1, "CONT"), (2, "APF")] {
                    if ui.selectable_label(s.contour_mode == c, l).clicked() {
                        rig.send(format!("CO00{c:02};"));
                    }
                }
            });
            ui.end_row();
            if s.contour_mode == 2 {
                slider(ui, rig, "APF offset", (s.apf_freq as i32 - 25) * 10, -250..=250, |v| format!("CO02{:02};", (v / 10 + 25).clamp(0, 50)));
            } else {
                slider(ui, rig, "Contour freq", s.contour_freq.max(1) as i32 * 100, 100..=4000, |v| format!("CO01{:02};", (v / 100).clamp(1, 40)));
            }
        });
    });
}

fn transmitter_group(ui: &mut Ui, rig: &Rig, s: &RadioState) {
    group(ui, "Transmitter", |ui| {
        egui::Grid::new("tx").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            slider(ui, rig, "Power (W)", s.power as i32, 5..=100, |v| format!("PC{v:03};"));
            slider(ui, rig, "Mic gain", s.mic_gain as i32, 0..=100, |v| format!("MG{v:03};"));
            ui.label("Processor");
            ui.horizontal(|ui| {
                if led_button(ui, s.processor, "PROC", AMBER) {
                    rig.send(format!("PR0{};", (!s.processor) as u8));
                }
                if led_button(ui, s.mic_eq, "MIC EQ", AMBER) {
                    rig.send(format!("PR1{};", (!s.mic_eq) as u8));
                }
            });
            ui.end_row();
            slider(ui, rig, "Proc level", s.proc_level as i32, 0..=100, |v| format!("PL{v:03};"));
            ui.label("VOX");
            ui.horizontal(|ui| {
                if led_button(ui, s.vox, "VOX", AMBER) {
                    rig.send(format!("VX{};", (!s.vox) as u8));
                }
            });
            ui.end_row();
            slider(ui, rig, "VOX gain", s.vox_gain as i32, 0..=100, |v| format!("VG{v:03};"));
            slider(ui, rig, "VOX delay (ms)", s.vox_delay.max(30) as i32, 30..=3000, |v| format!("VD{:04};", (v / 10) * 10));
            ui.label("Monitor");
            ui.horizontal(|ui| {
                if led_button(ui, s.monitor, "MONI", AMBER) {
                    rig.send(format!("ML0{:03};", (!s.monitor) as u8));
                }
            });
            ui.end_row();
            slider(ui, rig, "Monitor level", s.monitor_level.max(1) as i32, 1..=100, |v| format!("ML1{v:03};"));
            ui.label("Voice memory");
            ui.horizontal(|ui| {
                for ch in 1..=5 {
                    if ui.small_button(format!("▶{ch}")).on_hover_text(format!("Play DVS channel {ch} (transmits)")).clicked() {
                        rig.send(format!("PB0{ch};"));
                    }
                }
                if ui.small_button("■").on_hover_text("Stop playback").clicked() {
                    rig.send("PB00;");
                }
            });
            ui.end_row();
        });
    });
}

fn clarifier_group(ui: &mut Ui, rig: &Rig, s: &RadioState) {
    group(ui, "Clarifier (RIT / XIT)", |ui| {
        ui.horizontal(|ui| {
            if led_button(ui, s.rx_clar, "RX CLAR", AMBER) {
                rig.send(format!("RT{};", (!s.rx_clar) as u8));
            }
            if led_button(ui, s.tx_clar, "TX CLAR", AMBER) {
                rig.send(format!("XT{};", (!s.tx_clar) as u8));
            }
            ui.label(RichText::new(format!("{:+} Hz", s.info_a.clar_offset)).monospace().size(20.0).color(if s.rx_clar || s.tx_clar { AMBER } else { DIM }));
        });
        ui.horizontal(|ui| {
            for (d, l) in [(-100, "−100"), (-10, "−10"), (10, "+10"), (100, "+100")] {
                if button(ui, l) {
                    rig.send(if d < 0 { format!("RD{:04};", -d) } else { format!("RU{d:04};") });
                }
            }
            if button(ui, "CLEAR") {
                rig.send("RC;");
            }
        });
        ui.label(RichText::new("The FTDX3000 has one clarifier offset shared by RX and TX clarifier.").weak().size(11.0));
    });
}

fn rotator_group(ui: &mut Ui, rig: &Rig, s: &RadioState) {
    group(ui, "Rotator (via the radio's ROT jack)", |ui| {
        ui.horizontal(|ui| {
            match s.rotator {
                Some((_, dir, speed)) => {
                    ui.label(RichText::new(format!("{dir:>3}°")).monospace().size(22.0).color(CYAN));
                    ui.label(RichText::new(format!("speed {speed}%")).weak());
                }
                None => {
                    ui.label(RichText::new("no rotator data").color(DIM));
                }
            }
            if button(ui, "◀ CCW") {
                rig.send("RO1;");
            }
            if button(ui, "STOP") {
                rig.send("RO0;");
            }
            if button(ui, "CW ▶") {
                rig.send("RO2;");
            }
            if ui.small_button("speed −").clicked() {
                rig.send("RO3;");
            }
            if ui.small_button("speed +").clicked() {
                rig.send("RO4;");
            }
        });
        ui.label(RichText::new("Works with a Yaesu G-800DXA/1000DXA/2800DXA connected to the radio.").weak().size(11.0));
    });
}

fn macro_group(ui: &mut Ui, app: &mut App, rig: &Rig) {
    group(ui, "Macros", |ui| {
        ui.horizontal_wrapped(|ui| {
            for m in &app.services.config.macros {
                if ui.button(&m.label).on_hover_text(&m.commands).clicked() {
                    rig.send(&m.commands);
                }
            }
        });
        let mut save = false;
        ui.collapsing("Edit macros", |ui| {
            let macros = &mut app.services.config.macros;
            let mut remove = None;
            for (i, m) in macros.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.add_sized([110.0, 22.0], egui::TextEdit::singleline(&mut m.label).hint_text("label"));
                    ui.add_sized([260.0, 22.0], egui::TextEdit::singleline(&mut m.commands).hint_text("FA14074000;MD0C;").font(egui::TextStyle::Monospace));
                    if ui.small_button("✕").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                macros.remove(i);
            }
            ui.horizontal(|ui| {
                if ui.button("Add").clicked() {
                    macros.push(ftdx3000_rig::Macro::default());
                }
                save = ui.button("Save").clicked();
            });
            ui.label(RichText::new("Any FTDX3000 CAT commands, separated by ';'. See the CAT console to try them.").weak().size(11.0));
        });
        if save {
            match app.services.config.save() {
                Ok(()) => app.flash("macros saved"),
                Err(e) => app.flash(format!("could not save: {e}")),
            }
        }
    });
}

fn cw_group(ui: &mut Ui, op: &mut OperateState, rig: &Rig, s: &RadioState) {
    group(ui, "CW", |ui| {
        egui::Grid::new("cw").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label("Keyer");
            ui.horizontal(|ui| {
                if led_button(ui, s.keyer, "KEYER", AMBER) {
                    rig.send(format!("KR{};", (!s.keyer) as u8));
                }
                if led_button(ui, s.break_in, "BK-IN", AMBER) {
                    rig.send(format!("BI{};", (!s.break_in) as u8));
                }
                if led_button(ui, s.cw_spot, "SPOT", AMBER) {
                    rig.send(format!("CS{};", (!s.cw_spot) as u8));
                }
            });
            ui.end_row();
            slider(ui, rig, "Speed (WPM)", s.key_speed.max(4) as i32, 4..=60, |v| format!("KS{v:03};"));
            slider(ui, rig, "Pitch (Hz)", 300 + 10 * s.key_pitch as i32, 300..=1050, |v| format!("KP{:02};", ((v - 300) / 10).clamp(0, 75)));
            slider(ui, rig, "BK-IN delay (ms)", s.bk_delay.max(30) as i32, 30..=3000, |v| format!("SD{:04};", (v / 10) * 10));
        });
        ui.label(RichText::new("Keyer memories (up to 50 characters)").strong());
        for i in 0..5 {
            if !op.keyer_loaded[i] {
                if let Some(t) = &s.keyer_memory[i] {
                    op.keyer_text[i] = t.trim_end().to_string();
                    op.keyer_loaded[i] = true;
                }
            }
            ui.horizontal(|ui| {
                if ui.button(format!("▶ {}", i + 1)).on_hover_text("Send this keyer memory (transmits)").clicked() {
                    rig.send(format!("KY{};", i + 1));
                }
                ui.add(egui::TextEdit::singleline(&mut op.keyer_text[i]).char_limit(50).desired_width(260.0));
                if ui.small_button("Save").on_hover_text("Write this text into the radio's keyer memory").clicked() {
                    let text: String = op.keyer_text[i].chars().filter(|c| c.is_ascii() && *c != ';').collect();
                    rig.send(format!("KM{}{};", i + 1, text.to_uppercase()));
                }
            });
        }
    });
}
