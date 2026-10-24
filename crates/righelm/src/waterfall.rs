//! Audio waterfall: a scrolling spectrogram of the radio's receive audio,
//! with the filter passband and CW pitch overlaid. Hover shows the radio
//! frequency of a signal; in CW, clicking a trace tunes onto it.

use std::time::Instant;

use crossbeam_channel::Receiver;
use eframe::egui::{self, Align2, Color32, ColorImage, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, TextureHandle, TextureOptions, Ui, vec2};
use ftdx3000_cat::types::{FREQ_MAX, FREQ_MIN, Mode, WidthClass};
use ftdx3000_cat::{RadioState, set_freq_cmd};
use ftdx3000_rig::spectrum::Spectrum;
use ftdx3000_rig::{Rig, UiPrefs};

use crate::widgets::{self, AMBER, CYAN};

const COLS: usize = 1024;
const ROWS: usize = 256;
const FFT: usize = 4096;

pub struct Waterfall {
    rx: Option<Receiver<Vec<i16>>>,
    spectrum: Option<Spectrum>,
    texture: Option<TextureHandle>,
    /// Row the next line is written to (moves upwards, newest at the top).
    row: usize,
    avg: Vec<f32>,
    noise: f32,
    lut: Vec<Color32>,
    last_audio: Option<Instant>,
    lines: u64,
}

impl Default for Waterfall {
    fn default() -> Self {
        Self { rx: None, spectrum: None, texture: None, row: 0, avg: vec![-120.0; COLS], noise: -100.0, lut: palette(), last_audio: None, lines: 0 }
    }
}

/// Black → blue → cyan → yellow → orange → white.
pub(crate) fn palette() -> Vec<Color32> {
    let stops: [(f32, [f32; 3]); 6] = [
        (0.0, [0.0, 0.0, 6.0]),
        (0.25, [10.0, 26.0, 110.0]),
        (0.5, [18.0, 164.0, 214.0]),
        (0.7, [242.0, 227.0, 11.0]),
        (0.85, [245.0, 120.0, 30.0]),
        (1.0, [255.0, 255.0, 255.0]),
    ];
    (0..256)
        .map(|i| {
            let t = i as f32 / 255.0;
            let k = stops.windows(2).position(|w| t <= w[1].0).unwrap_or(stops.len() - 2);
            let (a, b) = (stops[k], stops[k + 1]);
            let f = ((t - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
            let c = |j: usize| (a.1[j] + f * (b.1[j] - a.1[j])) as u8;
            Color32::from_rgb(c(0), c(1), c(2))
        })
        .collect()
}

/// Is a CW signal heard on the upper side of the carrier (like USB)?
/// Menu 060 CW BFO: 0 USB, 1 LSB, 2 AUTO (LSB below 10 MHz). CW-R flips it.
pub(crate) fn cw_upper(s: &RadioState, mode: Mode, dial: u64) -> bool {
    let normal = match s.menu.get(&60).map(String::as_str) {
        Some("1") => false,
        Some("2") => dial >= 10_000_000,
        _ => true,
    };
    if mode == Mode::CwR { !normal } else { normal }
}

pub(crate) fn pitch(s: &RadioState) -> f32 {
    300.0 + 10.0 * s.key_pitch as f32
}

/// The receiving VFO's dial frequency and mode.
pub(crate) fn rx_vfo(s: &RadioState) -> Option<(u64, Mode)> {
    if s.rx_on_b() { Some((s.freq_b, s.mode_b?)) } else { Some((s.freq_a, s.mode_a?)) }
}

/// Radio frequency of a signal heard at audio frequency `af`, if the mode
/// has a defined relationship (not AM/FM).
pub(crate) fn rf_of_audio(s: &RadioState, af: f32) -> Option<f64> {
    let (dial, mode) = rx_vfo(s)?;
    let (upper, offset) = match mode {
        Mode::Cw | Mode::CwR => {
            // Menu 064 CW FREQ DISPLAY = PITCH OFFSET: the dial shows the
            // frequency of a signal heard at the pitch.
            let off = if s.menu.get(&64).map(String::as_str) == Some("1") { af - pitch(s) } else { af };
            (cw_upper(s, mode, dial), off)
        }
        Mode::DataUsb | Mode::DataLsb => {
            // DATA MODE = OTHER shifts the display by menu 069 (e.g. +1500).
            let disp = if s.menu.get(&67).map(String::as_str) == Some("1") {
                s.menu.get(&69).and_then(|v| v.trim_start_matches('+').parse::<f32>().ok()).unwrap_or(0.0)
            } else {
                0.0
            };
            (mode == Mode::DataUsb, af - disp)
        }
        Mode::Usb | Mode::RttyUsb => (true, af),
        Mode::Lsb | Mode::RttyLsb => (false, af),
        _ => return None,
    };
    Some(dial as f64 + if upper { offset as f64 } else { -(offset as f64) })
}

/// Where the filter passband sits in the audio.
pub(crate) fn passband(s: &RadioState) -> Option<(f32, f32)> {
    let (_, mode) = rx_vfo(s)?;
    let width = match s.width_class(mode) {
        WidthClass::Fixed => 3000.0,
        _ => s.width_hz().unwrap_or(2400) as f32,
    };
    let centre = if mode.is_cw() { pitch(s) } else { 1500.0 } + s.if_shift as f32;
    Some((centre - width / 2.0, centre + width / 2.0))
}

impl Waterfall {
    /// `band` is the filter passband in audio Hz: the noise floor is measured
    /// there (outside it the audio is silent and would drag the floor down).
    fn ingest(&mut self, ctx: &egui::Context, tap_rate: u32, prefs: &UiPrefs, band: Option<(f32, f32)>) {
        let Some(rx) = &self.rx else { return };
        let rate = if tap_rate == 0 { ftdx3000_rig::remote::AUDIO_RATE } else { tap_rate };
        let hop = (rate as usize / 50) * prefs.wf_speed.clamp(1, 8) as usize;
        let spectrum = match &mut self.spectrum {
            Some(s) if s.rate() == rate => s,
            _ => self.spectrum.insert(Spectrum::new(rate, FFT, hop)),
        };
        spectrum.set_hop(hop);
        let texture = self.texture.get_or_insert_with(|| {
            ctx.load_texture("waterfall", ColorImage::filled([COLS, ROWS], Color32::BLACK), TextureOptions::LINEAR)
        });
        let res = spectrum.resolution();
        let max_hz = prefs.wf_max_hz.clamp(1000, rate / 2) as f32;
        let mut lines = Vec::new();
        while let Ok(frame) = rx.try_recv() {
            self.last_audio = Some(Instant::now());
            lines.extend(spectrum.push(&frame));
        }
        for line in lines {
            // Resample bins onto the columns (0..max_hz), linear in dB.
            let cols: Vec<f32> = (0..COLS)
                .map(|c| {
                    let bin = (c as f32 + 0.5) / COLS as f32 * max_hz / res;
                    let i = (bin as usize).min(line.len() - 2);
                    let f = bin - i as f32;
                    line[i] * (1.0 - f) + line[i + 1] * f
                })
                .collect();
            // Noise floor: a low percentile inside the passband, smoothed.
            let (lo, hi) = band.unwrap_or((300.0, 2700.0));
            // At least 8 columns, even for a passband above the shown range.
            let c0 = (((lo.max(100.0) / max_hz) * COLS as f32) as usize).min(COLS - 8);
            let c1 = (((hi / max_hz) * COLS as f32) as usize).max(c0 + 8).min(COLS);
            let mut inband: Vec<f32> = cols[c0..c1].to_vec();
            inband.sort_by(|a, b| a.total_cmp(b));
            let p = inband[inband.len() * 3 / 10];
            self.noise = if self.lines == 0 { p } else { 0.95 * self.noise + 0.05 * p };
            self.lines += 1;
            let (floor, range) = levels(prefs, self.noise);
            let pixels: Vec<Color32> = cols
                .iter()
                .map(|db| {
                    let t = ((db - floor) / range).clamp(0.0, 1.0);
                    self.lut[(t * 255.0) as usize]
                })
                .collect();
            texture.set_partial([0, self.row], ColorImage::new([COLS, 1], pixels), TextureOptions::LINEAR);
            self.row = (self.row + ROWS - 1) % ROWS;
            for (a, v) in self.avg.iter_mut().zip(&cols) {
                *a = 0.7 * *a + 0.3 * v;
            }
        }
    }
}

impl Waterfall {
    /// Lines drawn so far (to wait for fresh audio after retuning).
    pub fn lines(&self) -> u64 {
        self.lines
    }

    /// The exact audio frequency of the strongest clear tone within `width`
    /// Hz of `hz`, from the recent spectrum (`max_hz` = the span shown).
    pub fn tone_near(&self, max_hz: f32, hz: f32, width: f32) -> Option<f32> {
        if self.last_audio.is_none_or(|t| t.elapsed().as_secs_f32() > 1.0) {
            return None;
        }
        let col = |f: f32| f / max_hz * COLS as f32 - 0.5;
        let (a, b) = (col(hz - width).max(1.0) as usize, (col(hz + width).max(0.0) as usize).min(COLS - 2));
        let k = (a..=b).max_by(|x, y| self.avg[*x].total_cmp(&self.avg[*y]))?;
        if self.avg[k] < self.noise + 12.0 {
            return None;
        }
        let (l, c, r) = (self.avg[k - 1], self.avg[k], self.avg[k + 1]);
        let d = l - 2.0 * c + r;
        let off = if d < 0.0 { (0.5 * (l - r) / d).clamp(-0.5, 0.5) } else { 0.0 };
        Some((k as f32 + 0.5 + off) / COLS as f32 * max_hz)
    }
}

/// dB at the bottom of the colour scale, and the span of the scale.
/// Auto puts the noise just above black so signals stand out.
fn levels(prefs: &UiPrefs, noise: f32) -> (f32, f32) {
    let range = prefs.wf_range_db.clamp(20, 120) as f32;
    let floor = if prefs.wf_auto { noise - range * 0.12 } else { prefs.wf_floor_db as f32 };
    (floor, range)
}

pub fn show(app: &mut crate::App, ui: &mut Ui, rig: &Rig, s: &RadioState) {
    if !app.services.config.ui.show_waterfall {
        // Unsubscribing lets the audio recorder stop.
        app.waterfall.rx = None;
        return;
    }
    if app.waterfall.rx.is_none() {
        app.waterfall.rx = Some(app.services.rx_tap.subscribe());
    }
    let tap_rate = app.services.rx_tap.rate();
    let source = app.services.rx_audio_status();
    let wf = &mut app.waterfall;
    let prefs = &mut app.services.config.ui;
    let tune_step = prefs.tune_step;
    wf.ingest(ui.ctx(), tap_rate, prefs, passband(s));

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            // Rows as tall as the buttons, so a label starting one lines up.
            ui.set_row_height(22.0);
            ui.label(RichText::new("AUDIO WATERFALL").strong().size(15.0).color(AMBER));
            ui.label("Span");
            for hz in [3000u32, 4000, 6000] {
                if ui.selectable_label(prefs.wf_max_hz == hz, format!("{} kHz", hz / 1000)).clicked() {
                    prefs.wf_max_hz = hz;
                }
            }
            ui.separator();
            widgets::wrap_unless_room(ui, 200.0);
            ui.add_sized([40.0, 22.0], egui::Label::new("Speed"));
            for (v, l) in [(4u32, "slow"), (2, "normal"), (1, "fast")] {
                if ui.selectable_label(prefs.wf_speed == v, l).clicked() {
                    prefs.wf_speed = v;
                }
            }
            ui.separator();
            widgets::wrap_unless_room(ui, 100.0);
            ui.checkbox(&mut prefs.wf_auto, "Auto level");
            ui.spacing_mut().slider_width = 110.0;
            // The floor is set by hand only with auto level off (as on the band scope).
            if !prefs.wf_auto {
                widgets::wrap_unless_room(ui, 230.0);
                let r = ui.add(egui::Slider::new(&mut prefs.wf_floor_db, -140..=-40).text("floor dB"));
                prefs.wf_floor_db = (prefs.wf_floor_db + widgets::scroll_steps(ui, &r, 12.0) as i32).clamp(-140, -40);
            }
            widgets::wrap_unless_room(ui, 180.0);
            let r = ui.add(egui::Slider::new(&mut prefs.wf_range_db, 20..=120).text("contrast").show_value(false));
            prefs.wf_range_db = (prefs.wf_range_db + widgets::scroll_steps(ui, &r, 12.0) as i32).clamp(20, 120);
            let stale = wf.last_audio.is_none_or(|t| t.elapsed().as_secs_f32() > 1.0);
            if stale {
                ui.label(RichText::new(format!("no audio — {}", source.as_deref().unwrap_or("starting…"))).color(AMBER));
            }
        });

        let max_hz = prefs.wf_max_hz as f32;
        // Spectrum and waterfall share the height left, the spectrum up to 70.
        let width = ui.available_width();
        let scale_h = 16.0;
        let graphs = (ui.available_height() - 2.0 - scale_h).max(60.0);
        let spec_h = (graphs * 0.35).clamp(30.0, 70.0);
        let wf_h = graphs - spec_h;
        let (rect, resp) = ui.allocate_exact_size(vec2(width, spec_h + wf_h + scale_h), Sense::click());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 4.0, widgets::PANEL);
        let spec = Rect::from_min_size(rect.min, vec2(width, spec_h));
        let water = Rect::from_min_size(Pos2::new(rect.left(), spec.bottom()), vec2(width, wf_h));
        let x_of = |hz: f32| rect.left() + width * (hz / max_hz).clamp(0.0, 1.0);
        let hz_of = |x: f32| ((x - rect.left()) / width * max_hz).clamp(0.0, max_hz);

        // Waterfall image, newest line at the top.
        if let Some(tex) = &wf.texture {
            let top = (wf.row + 1) % ROWS;
            let first = (ROWS - top) as f32 / ROWS as f32;
            let split = water.top() + wf_h * first;
            let uv_a = Rect::from_min_max(Pos2::new(0.0, top as f32 / ROWS as f32), Pos2::new(1.0, 1.0));
            let uv_b = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, top as f32 / ROWS as f32));
            p.image(tex.id(), Rect::from_min_max(water.min, Pos2::new(water.right(), split)), uv_a, Color32::WHITE);
            p.image(tex.id(), Rect::from_min_max(Pos2::new(water.left(), split), water.max), uv_b, Color32::WHITE);
        }

        // Spectrum trace.
        let (floor, range) = levels(prefs, wf.noise);
        let pts: Vec<Pos2> = wf
            .avg
            .iter()
            .enumerate()
            .map(|(c, db)| {
                let t = ((db - floor) / range).clamp(0.0, 1.0);
                Pos2::new(spec.left() + width * (c as f32 + 0.5) / COLS as f32, spec.bottom() - 2.0 - t * (spec_h - 6.0))
            })
            .collect();
        let mut fill = vec![Pos2::new(spec.left(), spec.bottom())];
        fill.extend(pts.iter().copied());
        fill.push(Pos2::new(spec.right(), spec.bottom()));
        for w in fill.windows(2) {
            let quad = vec![w[0], w[1], Pos2::new(w[1].x, spec.bottom()), Pos2::new(w[0].x, spec.bottom())];
            p.add(Shape::convex_polygon(quad, CYAN.gamma_multiply(0.12), Stroke::NONE));
        }
        p.add(Shape::line(pts, Stroke::new(1.2, CYAN)));

        // Passband and pitch overlays.
        if let Some((lo, hi)) = passband(s) {
            let band = Rect::from_min_max(Pos2::new(x_of(lo), rect.top()), Pos2::new(x_of(hi), water.bottom()));
            p.rect_filled(Rect::from_min_max(band.min, Pos2::new(band.right(), spec.bottom())), 0.0, AMBER.gamma_multiply(0.08));
            for x in [band.left(), band.right()] {
                p.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, water.bottom())], Stroke::new(1.0, AMBER.gamma_multiply(0.7)));
            }
        }
        let mode = rx_vfo(s).map(|v| v.1);
        if mode.is_some_and(Mode::is_cw) {
            let x = x_of(pitch(s));
            p.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, water.bottom())], Stroke::new(1.0, Color32::from_rgb(255, 80, 80)));
        }

        // Audio scale.
        let base = water.bottom();
        let mut hz = 0.0;
        while hz <= max_hz {
            let x = x_of(hz);
            let major = (hz as u32) % 1000 == 0;
            p.line_segment([Pos2::new(x, base), Pos2::new(x, base + if major { 5.0 } else { 3.0 })], Stroke::new(1.0, Color32::GRAY));
            if major && hz > 0.0 && hz < max_hz {
                p.text(Pos2::new(x, base + 4.0), Align2::CENTER_TOP, format!("{}k", hz as u32 / 1000), FontId::proportional(10.0), Color32::GRAY);
            }
            hz += 500.0;
        }

        // Hover readout and click-to-tune.
        if let Some(pos) = resp.hover_pos() {
            let af = hz_of(pos.x);
            p.line_segment([Pos2::new(pos.x, rect.top()), Pos2::new(pos.x, water.bottom())], Stroke::new(1.0, Color32::from_white_alpha(120)));
            let rf = rf_of_audio(s, af).map(|f| format!("  ·  {}", widgets::format_freq(f.round() as u64).trim())).unwrap_or_default();
            let text = format!("{af:.0} Hz{rf}");
            let anchor = if pos.x > rect.center().x { Align2::RIGHT_TOP } else { Align2::LEFT_TOP };
            let at = Pos2::new(pos.x + if pos.x > rect.center().x { -6.0 } else { 6.0 }, rect.top() + 4.0);
            let galley_rect = p.text(at, anchor, &text, FontId::monospace(13.0), Color32::WHITE);
            p.rect_filled(galley_rect.expand(3.0), 3.0, Color32::from_black_alpha(160));
            p.text(at, anchor, &text, FontId::monospace(13.0), Color32::WHITE);
        }
        // Scroll over the waterfall tunes, like the frequency display.
        let ticks = widgets::wheel_ticks(ui, &resp);
        if ticks != 0 {
            if let Some((dial, _)) = rx_vfo(s) {
                let hz = (dial as i64 + ticks * tune_step).clamp(FREQ_MIN as i64, FREQ_MAX as i64) as u64;
                rig.send(set_freq_cmd(s.rx_on_b(), hz));
            }
        }
        if resp.clicked() {
            if let (Some(pos), Some((dial, mode))) = (resp.interact_pointer_pos(), rx_vfo(s)) {
                if mode.is_cw() {
                    // Move the clicked signal onto the CW pitch.
                    let delta = (hz_of(pos.x) - pitch(s)).round() as i64;
                    let delta = if cw_upper(s, mode, dial) { delta } else { -delta };
                    rig.send(set_freq_cmd(s.rx_on_b(), (dial as i64 + delta).max(0) as u64));
                }
            }
        }
        if mode.is_some_and(Mode::is_cw) {
            resp.on_hover_text("Click a signal to tune onto it (zero-beat at your CW pitch) · scroll to tune");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(mode: &str, freq: u64) -> RadioState {
        let mut s = RadioState::new();
        s.apply(&format!("FA{freq:08};"));
        s.apply(&format!("MD0{mode};"));
        s.apply("KP40;"); // 700 Hz pitch
        s
    }

    #[test]
    fn audio_to_radio_frequency() {
        let s = state("2", 14_200_000); // USB
        assert_eq!(rf_of_audio(&s, 1000.0), Some(14_201_000.0));
        let s = state("1", 7_200_000); // LSB
        assert_eq!(rf_of_audio(&s, 1000.0), Some(7_199_000.0));
        // CW, pitch-offset display (menu 064 = 1): a signal at the pitch is on the dial.
        let mut s = state("3", 14_025_000);
        s.apply("EX0641;");
        assert_eq!(rf_of_audio(&s, 700.0), Some(14_025_000.0));
        assert_eq!(rf_of_audio(&s, 800.0), Some(14_025_100.0));
        // CW BFO AUTO below 10 MHz is LSB side.
        s.apply("FA07025000;");
        s.apply("EX0602;");
        assert_eq!(rf_of_audio(&s, 800.0), Some(7_024_900.0));
        // DATA-U with DATA MODE = OTHER and display offset +1500.
        let mut s = state("C", 14_075_500);
        s.apply("EX0671;");
        s.apply("EX069+1500;");
        assert_eq!(rf_of_audio(&s, 1500.0), Some(14_075_500.0));
        // AM has no single answer.
        assert_eq!(rf_of_audio(&state("5", 7_200_000), 1000.0), None);
    }

    #[test]
    fn palette_runs_dark_to_bright() {
        let p = palette();
        assert_eq!(p.len(), 256);
        assert!(p[0].r() < 10 && p[0].g() < 10);
        assert_eq!(p[255], Color32::WHITE);
    }
}
