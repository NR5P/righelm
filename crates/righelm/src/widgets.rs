//! Custom widgets: the frequency display and bar meters.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

pub const AMBER: Color32 = Color32::from_rgb(255, 176, 46);
pub const CYAN: Color32 = Color32::from_rgb(80, 200, 255);
pub const GREEN: Color32 = Color32::from_rgb(90, 220, 120);
pub const RED: Color32 = Color32::from_rgb(235, 70, 60);
pub const DIM: Color32 = Color32::from_rgb(90, 90, 96);
pub const PANEL: Color32 = Color32::from_rgb(14, 16, 20);

/// What the user did to a frequency display this frame.
pub enum FreqAction {
    None,
    /// Tune by this many Hz.
    Step(i64),
    /// Start typing a new frequency.
    Edit,
}

/// A segment-style frequency readout. Scrolling over a digit tunes that
/// digit; scrolling elsewhere tunes by `step`; double-click to type.
pub fn freq_display(ui: &mut Ui, hz: u64, size: f32, color: Color32, step: i64, enabled: bool) -> FreqAction {
    let text = format_freq(hz);
    let font = FontId::monospace(size);
    let char_w = ui.ctx().fonts_mut(|f| f.glyph_width(&font, '0'));
    let width = char_w * text.chars().count() as f32 + 16.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, size * 1.25), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(6), PANEL);
    let color = if enabled { color } else { DIM };

    // Lay out characters and remember which power of ten each digit is.
    let digits: Vec<char> = text.chars().collect();
    let mut digit_rects: Vec<(Rect, i64)> = Vec::new();
    let mut place = digits.iter().filter(|c| c.is_ascii_digit()).count() as i32;
    let mut x = rect.left() + 8.0;
    for c in &digits {
        let r = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(char_w, rect.height()));
        if c.is_ascii_digit() {
            place -= 1;
            digit_rects.push((r, 10i64.pow(place as u32)));
        }
        x += char_w;
    }
    let hover = response.hover_pos().and_then(|p| digit_rects.iter().find(|(r, _)| r.contains(p)).map(|(_, v)| *v));
    let mut x = rect.left() + 8.0;
    let mut di = 0;
    let mut seen_nonzero = false;
    for c in &digits {
        let mut col = color;
        if c.is_ascii_digit() {
            if hover == Some(digit_rects[di].1) && enabled {
                let r = digit_rects[di].0.shrink2(Vec2::new(0.0, 4.0));
                painter.rect_filled(r, CornerRadius::same(3), Color32::from_rgb(40, 44, 52));
            }
            // Leading zeros are dimmed, like the radio.
            if *c == '0' && !seen_nonzero {
                col = col.gamma_multiply(0.25);
            } else {
                seen_nonzero = true;
            }
            di += 1;
        }
        painter.text(Pos2::new(x, rect.center().y), Align2::LEFT_CENTER, c, font.clone(), col);
        x += char_w;
    }
    // Claim the scroll even when locked, so the page never scrolls under it.
    let ticks = wheel_ticks(ui, &response);
    if !enabled {
        return FreqAction::None;
    }
    if response.double_clicked() {
        return FreqAction::Edit;
    }
    if ticks != 0 {
        return FreqAction::Step(ticks * hover.unwrap_or(step));
    }
    // Drag horizontally to tune by the step size.
    if response.dragged() {
        let dx = response.drag_delta().x;
        if dx.abs() >= 1.0 {
            return FreqAction::Step((dx / 4.0).round() as i64 * step);
        }
    }
    FreqAction::None
}

/// Stop the page from scrolling while the pointer is over `response`.
pub fn claim_scroll(ui: &Ui, response: &egui::Response) {
    if response.hovered() {
        ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
    }
}

/// Wheel/trackpad events over `response`, one step each (the tuning feel),
/// claimed so the page doesn't scroll as well.
pub fn wheel_ticks(ui: &Ui, response: &egui::Response) -> i64 {
    if !response.hovered() {
        return 0;
    }
    let ticks = ui.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::MouseWheel { delta, .. } => {
                    let d = delta.y + delta.x;
                    (d != 0.0).then_some(d.signum() as i64)
                }
                _ => None,
            })
            .sum::<i64>()
    });
    claim_scroll(ui, response);
    ticks
}

/// Scroll over `response` as whole steps: a mouse-wheel notch is one step,
/// trackpad movement adds up (`points_per_step`) so one swipe doesn't jump
/// a slider end to end. Claimed so the page doesn't scroll as well.
pub fn scroll_steps(ui: &Ui, response: &egui::Response, points_per_step: f32) -> i64 {
    if !response.hovered() {
        return 0;
    }
    let id = response.id.with("scroll_steps");
    let mut acc: f32 = ui.data(|d| d.get_temp(id).unwrap_or(0.0));
    let mut steps = 0i64;
    ui.input(|i| {
        for e in &i.events {
            if let egui::Event::MouseWheel { unit, delta, .. } = e {
                let d = delta.y + delta.x;
                match unit {
                    egui::MouseWheelUnit::Point => acc += d,
                    egui::MouseWheelUnit::Line => acc += d * points_per_step,
                    egui::MouseWheelUnit::Page => steps += d.signum() as i64,
                }
            }
        }
    });
    let whole = (acc / points_per_step).trunc();
    steps += whole as i64;
    acc -= whole * points_per_step;
    ui.data_mut(|d| d.insert_temp(id, acc));
    claim_scroll(ui, response);
    steps
}

pub fn format_freq(hz: u64) -> String {
    let mhz = hz / 1_000_000;
    let khz = (hz / 1_000) % 1_000;
    let h = hz % 1_000;
    format!("{mhz:>2}.{khz:03}.{h:03}")
}

/// Parse user input: "14074", "14.074", "14.074.000", "7074.5", "14074000".
pub fn parse_freq(s: &str) -> Option<u64> {
    let t: String = s.trim().chars().filter(|c| !c.is_whitespace() && *c != ',').collect();
    let t = t.to_lowercase();
    let (num, mult) = if let Some(v) = t.strip_suffix("mhz").or_else(|| t.strip_suffix('m')) {
        (v.to_string(), 1e6)
    } else if let Some(v) = t.strip_suffix("khz").or_else(|| t.strip_suffix('k')) {
        (v.to_string(), 1e3)
    } else if let Some(v) = t.strip_suffix("hz") {
        (v.to_string(), 1.0)
    } else {
        // Two dots: radio style 14.074.000
        let dots = t.matches('.').count();
        if dots >= 2 {
            let digits: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
            return digits.parse().ok();
        }
        let v: f64 = t.parse().ok()?;
        // Guess units: < 100 → MHz, < 100000 → kHz, else Hz.
        let mult = if v < 100.0 { 1e6 } else if v < 100_000.0 { 1e3 } else { 1.0 };
        return Some((v * mult).round() as u64);
    };
    let v: f64 = num.parse().ok()?;
    Some((v * mult).round() as u64)
}

/// A horizontal bar meter with a scale.
pub struct Meter<'a> {
    pub label: &'a str,
    /// 0.0 - 1.0
    pub fraction: f32,
    pub text: String,
    pub color: Color32,
    /// (fraction, label) tick marks.
    pub ticks: &'a [(f32, &'a str)],
    /// Fraction above which the bar is drawn red.
    pub red_from: Option<f32>,
    pub peak: Option<f32>,
}

impl Meter<'_> {
    pub fn show(self, ui: &mut Ui, width: f32) {
        let h = 34.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, h), Sense::hover());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, CornerRadius::same(4), PANEL);
        let label_w = 46.0;
        let text_w = 74.0;
        p.text(Pos2::new(rect.left() + 6.0, rect.center().y), Align2::LEFT_CENTER, self.label, FontId::proportional(13.0), Color32::LIGHT_GRAY);
        let bar = Rect::from_min_max(
            Pos2::new(rect.left() + label_w, rect.top() + 16.0),
            Pos2::new(rect.right() - text_w, rect.bottom() - 6.0),
        );
        p.rect_filled(bar, CornerRadius::same(2), Color32::from_rgb(30, 32, 38));
        let f = self.fraction.clamp(0.0, 1.0);
        let filled = Rect::from_min_max(bar.min, Pos2::new(bar.left() + bar.width() * f, bar.bottom()));
        match self.red_from {
            Some(r) if f > r => {
                let split = bar.left() + bar.width() * r;
                p.rect_filled(Rect::from_min_max(filled.min, Pos2::new(split, filled.bottom())), CornerRadius::same(2), self.color);
                p.rect_filled(Rect::from_min_max(Pos2::new(split, filled.top()), filled.max), CornerRadius::same(2), RED);
            }
            _ => {
                p.rect_filled(filled, CornerRadius::same(2), self.color);
            }
        }
        if let Some(pk) = self.peak {
            let x = bar.left() + bar.width() * pk.clamp(0.0, 1.0);
            p.line_segment([Pos2::new(x, bar.top() - 1.0), Pos2::new(x, bar.bottom() + 1.0)], Stroke::new(2.0, Color32::WHITE));
        }
        for (t, l) in self.ticks {
            let x = bar.left() + bar.width() * t;
            p.line_segment([Pos2::new(x, bar.top() - 3.0), Pos2::new(x, bar.top())], Stroke::new(1.0, Color32::GRAY));
            p.text(Pos2::new(x, bar.top() - 3.0), Align2::CENTER_BOTTOM, l, FontId::proportional(10.0), Color32::GRAY);
        }
        p.rect_stroke(bar, CornerRadius::same(2), Stroke::new(1.0, Color32::from_rgb(60, 62, 70)), StrokeKind::Outside);
        p.text(Pos2::new(rect.right() - 6.0, rect.center().y), Align2::RIGHT_CENTER, self.text, FontId::monospace(14.0), Color32::WHITE);
    }
}

/// A toggle that looks like a lit front-panel button. Returns true if clicked.
pub fn led_button(ui: &mut Ui, on: bool, label: &str, color: Color32) -> bool {
    let text = egui::RichText::new(label).strong();
    let text = if on { text.color(Color32::BLACK) } else { text };
    let mut b = egui::Button::new(text).min_size(Vec2::new(54.0, 26.0));
    if on {
        b = b.fill(color);
    }
    ui.add(b).clicked()
}

/// A wide plain button with a fixed minimum size.
pub fn button(ui: &mut Ui, label: &str) -> bool {
    ui.add(egui::Button::new(label).min_size(Vec2::new(54.0, 26.0))).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run frames with `events`, pointer over a 200x50 control at the top
    /// left; returns (wheel_ticks, scroll_steps, page scroll left over).
    fn scroll_frame(ctx: &egui::Context, events: Vec<egui::Event>) -> (i64, i64, Vec2) {
        let mut evs = vec![egui::Event::PointerMoved(Pos2::new(50.0, 20.0))];
        evs.extend(events);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0))),
            events: evs,
            ..Default::default()
        };
        let mut out = (0, 0, Vec2::ZERO);
        let mut full = ctx.run_ui(input, |ui| {
            let (_, r1) = ui.allocate_exact_size(Vec2::new(200.0, 25.0), Sense::hover());
            let ticks = wheel_ticks(ui, &r1);
            let (_, r2) = ui.allocate_exact_size(Vec2::new(200.0, 25.0), Sense::hover());
            let steps = scroll_steps(ui, &r2, 12.0);
            out = (ticks, steps, ui.input(|i| i.smooth_scroll_delta));
        });
        full.textures_delta.clear(); // no renderer in tests
        out
    }

    fn wheel(unit: egui::MouseWheelUnit, x: f32, y: f32) -> egui::Event {
        egui::Event::MouseWheel { unit, delta: Vec2::new(x, y), phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Unknown, modifiers: Default::default() }
    }

    #[test]
    fn scrolling_over_a_control_adjusts_it_and_not_the_page() {
        let ctx = egui::Context::default();
        let _ = scroll_frame(&ctx, vec![]); // let egui learn the layout
        // Two mouse-wheel notches up over the first control.
        let (ticks, _, page) = scroll_frame(&ctx, vec![wheel(egui::MouseWheelUnit::Line, 0.0, 1.0), wheel(egui::MouseWheelUnit::Line, 0.0, 1.0)]);
        assert_eq!(ticks, 2);
        assert_eq!(page, Vec2::ZERO, "the page must not scroll");
        // A sideways trackpad swipe left tunes down, not up.
        let (ticks, _, _) = scroll_frame(&ctx, vec![wheel(egui::MouseWheelUnit::Point, -3.0, 0.0)]);
        assert_eq!(ticks, -1);
    }

    #[test]
    fn freq_parsing() {
        assert_eq!(parse_freq("14.074"), Some(14_074_000));
        assert_eq!(parse_freq("14074"), Some(14_074_000));
        assert_eq!(parse_freq("14.074.500"), Some(14_074_500));
        assert_eq!(parse_freq("7074.5"), Some(7_074_500));
        assert_eq!(parse_freq("14074000"), Some(14_074_000));
        assert_eq!(parse_freq("3.573 MHz"), Some(3_573_000));
        assert_eq!(parse_freq("475 k"), Some(475_000));
        assert_eq!(parse_freq("abc"), None);
        assert_eq!(format_freq(7_074_000), " 7.074.000");
        assert_eq!(format_freq(14_074_000), "14.074.000");
    }
}
