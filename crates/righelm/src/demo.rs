//! Demo mode, for making videos of the real app: plays a script of pointer
//! moves, clicks, scrolls and captions over the live app, and records every
//! frame to a video with ffmpeg. The pointer and keys are synthetic, so the
//! real mouse doesn't disturb it.
//!
//!     RIGHELM_DEMO=script.txt RIGHELM_DEMO_OUT=out.mp4 righelm
//!
//! Script lines are `<seconds> <action> <args…>` (see `parse`). The clock
//! starts after `warmup` seconds, so the waterfall is full by then. Next to
//! the video goes `out.events.txt`: when each click and card happened, for
//! the soundtrack.

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, ColorImage, CornerRadius, Event, FontId, Id, LayerId, Modifiers, Order, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};
use parking_lot::Mutex;

use crate::widgets::AMBER;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static MARKS: Mutex<Option<HashMap<String, Rect>>> = Mutex::new(None);
static SCOPE: Mutex<Option<ScopeGeo>> = Mutex::new(None);

pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Remember where a named widget is, for the script to point at.
pub fn mark(name: impl Into<String>, rect: Rect) {
    if active() {
        MARKS.lock().get_or_insert_with(HashMap::new).insert(name.into(), rect);
    }
}

/// Where the band scope is and what it shows, for pointing at signals.
#[derive(Clone)]
pub struct ScopeGeo {
    pub spec: Rect,
    pub start: f64,
    pub span: f64,
    pub vfo: f64,
    /// Recent peak level per column.
    pub peaks: Vec<Option<f32>>,
}

pub fn scope(geo: impl FnOnce() -> ScopeGeo) {
    if active() {
        *SCOPE.lock() = Some(geo());
    }
}

/// What the script points at.
#[derive(Debug, Clone)]
enum Target {
    /// A marked widget (`span:200k`, `mode:CW`, `tab:Settings`, …).
    Mark(String),
    /// A radio frequency on the scope's spectrum.
    Freq(f64),
    /// The strongest signal between two frequencies relative to the dial.
    Signal(f64, f64),
    /// The strongest wide (voice) signal between two frequencies relative to
    /// the dial: the most energy over 2.4 kHz.
    Voice(f64, f64),
    /// A point on the window.
    Point(Pos2),
}

#[derive(Debug, Clone)]
enum Action {
    /// Full-screen card: kind ("title"/"end"), heading, lines, seconds.
    Card(String, String, Vec<String>, f64),
    Caption(String, String, f64),
    Move(Target, f64),
    Click(Modifiers),
    Scroll(i64, Modifiers),
    /// Show held keys next to the pointer for a while ("Shift", "Ctrl + scroll").
    Keys(String, f64),
    Cat(String),
    End,
}

#[derive(Debug, Clone)]
struct Step {
    at: f64,
    action: Action,
}

fn mods(s: &str) -> Modifiers {
    let mut m = Modifiers::NONE;
    for k in s.split('+') {
        match k {
            "shift" => m.shift = true,
            "ctrl" => {
                m.ctrl = true;
                m.command = true;
            }
            _ => {}
        }
    }
    m
}

/// Split a line into words, keeping "quoted strings" together.
fn words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => {
                if quoted {
                    out.push(std::mem::take(&mut cur));
                }
                quoted = !quoted;
            }
            ' ' | '\t' if !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn target(s: &str) -> Option<Target> {
    let parts: Vec<&str> = s.split(':').collect();
    Some(match parts.as_slice() {
        ["freq", f] => Target::Freq(f.parse().ok()?),
        ["signal", lo, hi] => Target::Signal(lo.parse().ok()?, hi.parse().ok()?),
        ["voice", lo, hi] => Target::Voice(lo.parse().ok()?, hi.parse().ok()?),
        ["at", x, y] => Target::Point(pos2(x.parse().ok()?, y.parse().ok()?)),
        _ => Target::Mark(s.to_string()),
    })
}

/// Script: `warmup <s>`, then lines of `<t> <action> …`:
/// * `card title|end "Heading" "line" "line" <secs>`
/// * `caption "Title" "Subtitle" <secs>`
/// * `move <target> <secs>`: target = mark name, `freq:<Hz>`,
///   `signal:<lo>:<hi>` (Hz from the dial), `at:<x>:<y>`
/// * `click [shift|ctrl]`, `scroll <ticks> [ctrl]`, `keys "Text" <secs>`
/// * `cat <CAT commands>`, `end`
fn parse(text: &str) -> (f64, Vec<Step>) {
    let mut warmup = 0.0;
    let mut steps = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let w = words(line);
        if w[0] == "warmup" {
            warmup = w.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);
            continue;
        }
        let Ok(at) = w[0].parse::<f64>() else {
            log::warn!("demo: bad line {line}");
            continue;
        };
        let arg = |i: usize| w.get(i).cloned().unwrap_or_default();
        let num = |i: usize| w.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        let action = match w.get(1).map(String::as_str) {
            Some("card") => {
                let secs = num(w.len() - 1);
                Action::Card(arg(2), arg(3), w[4..w.len() - 1].to_vec(), secs)
            }
            Some("caption") => Action::Caption(arg(2), arg(3), num(4)),
            Some("move") => match target(&arg(2)) {
                Some(t) => Action::Move(t, num(3)),
                None => continue,
            },
            Some("click") => Action::Click(mods(&arg(2))),
            Some("scroll") => Action::Scroll(num(2) as i64, mods(&arg(3))),
            Some("keys") => Action::Keys(arg(2), num(3)),
            Some("cat") => Action::Cat(arg(2)),
            Some("end") => Action::End,
            _ => {
                log::warn!("demo: unknown action in {line}");
                continue;
            }
        };
        steps.push(Step { at, action });
    }
    steps.sort_by(|a, b| a.at.total_cmp(&b.at));
    (warmup, steps)
}

/// Writes frames to ffmpeg at a steady rate: whatever frame is newest at
/// each tick, so the video runs in real time however fast the app draws.
struct Recorder {
    latest: Arc<Mutex<Option<Arc<ColorImage>>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

const FPS: u32 = 30;

impl Recorder {
    fn start(path: &str, size: [usize; 2]) -> std::io::Result<Recorder> {
        let [w, h] = size;
        let mut child = std::process::Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{w}x{h}"), "-r", &FPS.to_string(), "-i", "-"])
            .args(["-c:v", "libx264", "-preset", "ultrafast", "-crf", "10", "-pix_fmt", "yuv444p", path])
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let latest: Arc<Mutex<Option<Arc<ColorImage>>>> = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (latest2, stop2) = (latest.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            let t0 = Instant::now();
            let mut frame: u64 = 0;
            let mut buf = vec![0u8; w * h * 3];
            while !stop2.load(Ordering::Relaxed) {
                let due = t0 + Duration::from_secs_f64(frame as f64 / FPS as f64);
                if let Some(wait) = due.checked_duration_since(Instant::now()) {
                    std::thread::sleep(wait);
                }
                if let Some(img) = latest2.lock().clone() {
                    if frame == 0 || img.size != [w, h] && frame % 300 == 0 {
                        log::info!("demo: recording {}x{} frames into {w}x{h}", img.size[0], img.size[1]);
                    }
                    // Fit whatever size the window is into the video frame.
                    buf.iter_mut().for_each(|b| *b = 0);
                    for y in 0..h.min(img.size[1]) {
                        for x in 0..w.min(img.size[0]) {
                            let p = img.pixels[y * img.size[0] + x];
                            let o = (y * w + x) * 3;
                            buf[o..o + 3].copy_from_slice(&[p.r(), p.g(), p.b()]);
                        }
                    }
                }
                if stdin.write_all(&buf).is_err() {
                    break;
                }
                frame += 1;
            }
            drop(stdin);
            let _ = child.wait();
        });
        Ok(Recorder { latest, stop, thread: Some(thread) })
    }

    fn finish(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Shown {
    start: f64,
    secs: f64,
}

pub struct Demo {
    steps: Vec<Step>,
    next: usize,
    warmup: f64,
    started: Instant,
    /// Script clock zero (after the warm-up), when recording starts.
    t0: Option<Instant>,
    out: String,
    size: [usize; 2],
    recorder: Option<Recorder>,
    pointer: Pos2,
    glide: Option<(Pos2, Target, f64, f64)>,
    modifiers: Modifiers,
    /// Events to send in the coming frames (a click is a press, then a release).
    queue: Vec<Vec<Event>>,
    card: Option<(String, String, Vec<String>, Shown)>,
    caption: Option<(String, String, Shown)>,
    keys: Option<(String, Shown)>,
    ripples: Vec<(Pos2, f64)>,
    events: Vec<String>,
    done: bool,
    frames: u64,
}

impl Demo {
    pub fn from_env() -> Option<Demo> {
        let script = std::env::var("RIGHELM_DEMO").ok()?;
        let text = std::fs::read_to_string(&script).map_err(|e| log::warn!("demo script {script}: {e}")).ok()?;
        let (warmup, steps) = parse(&text);
        ACTIVE.store(true, Ordering::Relaxed);
        Some(Demo {
            steps,
            next: 0,
            warmup,
            started: Instant::now(),
            t0: None,
            out: std::env::var("RIGHELM_DEMO_OUT").unwrap_or_else(|_| "righelm-demo.mp4".into()),
            size: [1920, 1080],
            recorder: None,
            pointer: pos2(1500.0, 300.0),
            glide: None,
            modifiers: Modifiers::NONE,
            queue: Vec::new(),
            card: None,
            caption: None,
            keys: None,
            ripples: Vec::new(),
            events: Vec::new(),
            done: false,
            frames: 0,
        })
    }

    fn now(&self) -> Option<f64> {
        self.t0.map(|t| t.elapsed().as_secs_f64())
    }

    /// The frequency of the strongest signal a `Signal` target covers.
    fn signal_freq(&self, t: &Target) -> Option<f64> {
        let (lo, hi, wide) = match t {
            Target::Signal(lo, hi) => (*lo, *hi, false),
            Target::Voice(lo, hi) => (*lo, *hi, true),
            _ => return None,
        };
        let g = SCOPE.lock().clone()?;
        let n = g.peaks.len();
        let col = |f: f64| (((f - g.start) / g.span) * n as f64).clamp(0.0, (n - 1) as f64) as usize;
        let (a, b) = (col(g.vfo + lo), col(g.vfo + hi));
        let level = |c: usize| g.peaks[c].unwrap_or(-200.0);
        // Voice: strong across most of 2 kHz (a low percentile of the
        // window), so one wide signal beats a narrow spike or a gap
        // between stations.
        let half = if wide { ((1000.0 / g.span) * n as f64).ceil() as usize } else { 0 };
        let score = |c: usize| {
            let (x, y) = (c.saturating_sub(half), (c + half).min(n - 1));
            let mut v: Vec<f32> = (x..=y).map(level).collect();
            v.sort_by(|a, b| a.total_cmp(b));
            v[v.len() * 3 / 10]
        };
        let c = (a.min(b)..=a.max(b)).filter(|&c| g.peaks[c].is_some()).max_by(|&x, &y| score(x).total_cmp(&score(y)))?;
        Some(g.start + (c as f64 + 0.5) / n as f64 * g.span)
    }

    fn resolve(&self, t: &Target) -> Option<Pos2> {
        match t {
            Target::Point(p) => Some(*p),
            Target::Mark(name) => MARKS.lock().as_ref()?.get(name).map(|r| r.center()),
            Target::Freq(f) => {
                let g = SCOPE.lock().clone()?;
                let x = g.spec.left() + g.spec.width() * ((f - g.start) / g.span) as f32;
                Some(pos2(x, g.spec.top() + g.spec.height() * 0.62))
            }
            Target::Signal(..) | Target::Voice(..) => self.resolve(&Target::Freq(self.signal_freq(t)?)),
        }
    }

    /// Replace the real input with the script's.
    pub fn input(&mut self, raw: &mut egui::RawInput) {
        raw.events.retain(|e| {
            !matches!(
                e,
                Event::PointerMoved(_)
                    | Event::PointerButton { .. }
                    | Event::MouseWheel { .. }
                    | Event::Key { .. }
                    | Event::Text(_)
                    | Event::PointerGone
                    | Event::Zoom(_)
                    | Event::ModifiersChanged(_)
                    | Event::WindowFocused(_)
            )
        });
        if !self.queue.is_empty() {
            raw.events.extend(self.queue.remove(0));
        }
        raw.events.insert(0, Event::ModifiersChanged(self.modifiers));
        raw.events.push(Event::PointerMoved(self.pointer));
    }

    /// Run the script, draw the overlay, record the frame.
    pub fn frame(&mut self, ctx: &egui::Context, rig: &ftdx3000_rig::Rig, logo: &egui::TextureHandle) {
        ctx.request_repaint();
        self.frames += 1;
        if self.frames % 30 == 0 {
            log::info!("demo frame {} at {:.2}s", self.frames, self.started.elapsed().as_secs_f64());
        }
        if self.t0.is_none() && self.started.elapsed().as_secs_f64() >= self.warmup {
            match Recorder::start(&self.out, self.size) {
                Ok(r) => self.recorder = Some(r),
                Err(e) => log::warn!("demo: can't start ffmpeg: {e}"),
            }
            self.t0 = Some(Instant::now());
        }
        // Steps before zero set the scene during the warm-up.
        while self.t0.is_none() && self.next < self.steps.len() && self.steps[self.next].at < 0.0 {
            if let Action::Cat(cmd) = &self.steps[self.next].action {
                rig.send(cmd);
            }
            self.next += 1;
        }
        let Some(now) = self.now() else { return };
        // Due steps.
        while self.next < self.steps.len() && self.steps[self.next].at <= now {
            let step = self.steps[self.next].clone();
            self.next += 1;
            log::info!("demo {now:.2}: {:?}", step.action);
            let shown = Shown { start: now, secs: 0.0 };
            match step.action {
                Action::Card(kind, head, lines, secs) => {
                    self.events.push(format!("{now:.3} card {kind}"));
                    self.card = Some((kind, head, lines, Shown { secs, ..shown }));
                }
                Action::Caption(t, s, secs) => {
                    self.events.push(format!("{now:.3} caption"));
                    self.caption = Some((t, s, Shown { secs, ..shown }));
                }
                Action::Move(t, secs) => {
                    // Pick the signal once: the strongest can change on the way.
                    let t = match t {
                        Target::Signal(..) | Target::Voice(..) => self.signal_freq(&t).map_or(t, Target::Freq),
                        t => t,
                    };
                    self.glide = Some((self.pointer, t, now, secs.max(0.01)));
                }
                Action::Click(m) => {
                    self.modifiers = m;
                    let ev = |pressed| Event::PointerButton { pos: self.pointer, button: egui::PointerButton::Primary, pressed, modifiers: m };
                    self.queue.push(vec![ev(true)]);
                    self.queue.push(vec![ev(false)]);
                    self.queue.push(vec![]);
                    self.ripples.push((self.pointer, now));
                    self.events.push(format!("{now:.3} click"));
                }
                Action::Scroll(ticks, m) => {
                    self.modifiers = m;
                    for _ in 0..ticks.unsigned_abs() {
                        self.queue.push(vec![Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Line,
                            delta: vec2(0.0, ticks.signum() as f32),
                            phase: egui::TouchPhase::Move,
                            source: egui::MouseWheelSource::Unknown,
                            modifiers: m,
                        }]);
                        self.queue.push(vec![]);
                        self.queue.push(vec![]);
                        self.events.push(format!("{now:.3} scroll"));
                    }
                }
                Action::Keys(text, secs) => self.keys = Some((text, Shown { secs, ..shown })),
                Action::Cat(cmd) => rig.send(cmd),
                Action::End => self.done = true,
            }
        }
        // Let go of modifier keys once their clicks and scrolls are through.
        if self.queue.is_empty() {
            self.modifiers = Modifiers::NONE;
        }
        // Glide the pointer, re-aiming at moving targets (signals drift).
        if let Some((from, t, start, secs)) = self.glide.clone() {
            if let Some(to) = self.resolve(&t) {
                let k = ((now - start) / secs).clamp(0.0, 1.0) as f32;
                let e = k * k * (3.0 - 2.0 * k);
                self.pointer = from + (to - from) * e;
                if k >= 1.0 {
                    self.glide = None;
                }
            }
        }
        self.overlay(ctx, now, logo);
        if let Some(r) = &self.recorder {
            let latest = r.latest.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                *latest.lock() = Some(img);
            })));
        }
        if self.done && self.queue.is_empty() {
            if let Some(mut r) = self.recorder.take() {
                r.finish();
            }
            let _ = std::fs::write(format!("{}.events.txt", self.out.trim_end_matches(".mp4")), self.events.join("\n") + "\n");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn overlay(&mut self, ctx: &egui::Context, now: f64, logo: &egui::TextureHandle) {
        let p = ctx.layer_painter(LayerId::new(Order::Debug, Id::unique("demo")));
        let screen = ctx.content_rect();
        let fade = |s: &Shown, edge: f64| -> f32 {
            let t = now - s.start;
            ((t / edge).min((s.secs - t) / edge)).clamp(0.0, 1.0) as f32
        };

        // Click ripples.
        self.ripples.retain(|(_, at)| now - at < 0.6);
        for (pos, at) in &self.ripples {
            let k = ((now - at) / 0.6) as f32;
            p.circle_stroke(*pos, 6.0 + 26.0 * k, Stroke::new(3.0 * (1.0 - k), AMBER.gamma_multiply(1.0 - k)));
        }

        // Caption: in the open space right of the meters.
        if let Some((title, sub, s)) = &self.caption {
            let a = fade(s, 0.35);
            if a > 0.0 {
                let slide = (1.0 - a) * 16.0;
                let title_g = p.layout(title.clone(), FontId::proportional(46.0), Color32::WHITE.gamma_multiply(a), 900.0);
                let sub_g = p.layout(sub.clone(), FontId::proportional(25.0), Color32::from_gray(205).gamma_multiply(a), 900.0);
                let w = title_g.size().x.max(sub_g.size().x) + 64.0;
                let h = title_g.size().y + sub_g.size().y + 44.0;
                let r = Rect::from_min_size(pos2(screen.right() - 990.0, 150.0 + slide), vec2(w, h));
                p.rect_filled(r, CornerRadius::same(10), Color32::from_black_alpha((215.0 * a) as u8));
                p.rect_filled(Rect::from_min_size(r.min, vec2(6.0, h)), CornerRadius { nw: 10, sw: 10, ne: 0, se: 0 }, AMBER.gamma_multiply(a));
                p.galley(r.min + vec2(34.0, 16.0), title_g.clone(), Color32::WHITE);
                p.galley(r.min + vec2(34.0, 26.0 + title_g.size().y), sub_g, Color32::WHITE);
            }
            if now - s.start > s.secs {
                self.caption = None;
            }
        }

        // Pointer (drawn: the real one isn't in the picture).
        let c = self.pointer;
        let arrow = [vec2(0.0, 0.0), vec2(0.0, 22.0), vec2(5.5, 17.0), vec2(9.5, 26.0), vec2(13.0, 24.5), vec2(9.0, 15.5), vec2(16.0, 15.5)];
        let pts: Vec<Pos2> = arrow.iter().map(|v| c + *v).collect();
        p.add(Shape::Path(egui::epaint::PathShape::closed_line(pts.iter().map(|q| *q + vec2(1.5, 2.0)).collect(), Stroke::new(3.0, Color32::from_black_alpha(90)))));
        p.add(Shape::Path(egui::epaint::PathShape { points: pts.clone(), closed: true, fill: Color32::WHITE, stroke: Stroke::new(1.2, Color32::BLACK).into() }));

        // Held keys beside the pointer.
        if let Some((text, s)) = &self.keys {
            let a = fade(s, 0.2);
            let g = p.layout_no_wrap(text.clone(), FontId::proportional(18.0), Color32::BLACK);
            let r = Rect::from_min_size(c + vec2(24.0, 26.0), g.size() + vec2(18.0, 10.0));
            p.rect_filled(r, CornerRadius::same(6), AMBER.gamma_multiply(a));
            p.galley(r.min + vec2(9.0, 5.0), g, Color32::BLACK.gamma_multiply(a));
            if now - s.start > s.secs {
                self.keys = None;
            }
        }

        // Full-screen cards.
        if let Some((kind, head, lines, s)) = &self.card {
            let a = fade(s, 0.6);
            if a > 0.0 {
                p.rect_filled(screen, 0.0, Color32::from_rgb(8, 10, 15).gamma_multiply(a));
                let mid = screen.center();
                let rise = (1.0 - a) * 24.0;
                let size = if kind == "end" { 150.0 } else { 190.0 };
                let logo_r = Rect::from_center_size(mid + vec2(0.0, -170.0 + rise), Vec2::splat(size));
                p.image(logo.id(), logo_r, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE.gamma_multiply(a));
                p.text(mid + vec2(0.0, -20.0 + rise), Align2::CENTER_CENTER, head, FontId::proportional(84.0), AMBER.gamma_multiply(a));
                for (i, l) in lines.iter().enumerate() {
                    let (size, color) = if i == 0 { (40.0, Color32::WHITE) } else { (26.0, Color32::from_gray(190)) };
                    let y = 62.0 + i as f32 * 52.0 + if i > 0 { 8.0 } else { 0.0 };
                    p.text(mid + vec2(0.0, y + rise), Align2::CENTER_CENTER, l, FontId::proportional(size), color.gamma_multiply(a));
                }
            }
            if now - s.start > s.secs {
                self.card = None;
            }
        }
    }
}
