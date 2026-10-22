//! RigHelm — the Linux alternative to Win4Yaesu for the Yaesu FTDX3000.

mod console;
mod cw_key;
mod memories;
mod menu_tab;
mod operate;
mod remote_tab;
mod settings_tab;
mod sharing;
mod waterfall;
mod widgets;

use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText};
use ftdx3000_rig::detect::{self, DetectedRadio};
use ftdx3000_rig::{Config, LinkState, Rig, Services};

use widgets::{AMBER, GREEN, RED};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Operate,
    Menu,
    Memories,
    Sharing,
    Remote,
    Console,
    Settings,
}

/// The app icon (a ship's wheel around a VFO knob), embedded.
pub const ICON_PNG: &[u8] = include_bytes!("../../../assets/righelm-256.png");

/// Something that needs a second click to confirm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    PowerOff,
    RestoreMenu,
}

pub struct App {
    services: Services,
    tab: Tab,
    pub radios: Vec<DetectedRadio>,
    last_detect: Instant,
    pub status: Option<(String, Instant)>,
    pub confirm: Option<Confirm>,
    pub operate: operate::OperateState,
    pub menu: menu_tab::MenuState,
    pub memories: memories::MemoriesState,
    pub sharing: sharing::SharingState,
    pub console: console::ConsoleState,
    pub remote: remote_tab::RemoteState,
    pub cw_key: cw_key::CwKeyState,
    pub logo: egui::TextureHandle,
    pub waterfall: waterfall::Waterfall,
    /// Self-test: render each tab to `<dir>/<tab>.ppm` and exit
    /// (`FTDX3000_SCREENSHOTS=dir`).
    screenshots: Option<(std::path::PathBuf, Instant, usize)>,
}

const SCREENSHOT_TABS: [(Tab, &str); 7] = [
    (Tab::Operate, "operate"),
    (Tab::Menu, "menu"),
    (Tab::Memories, "memories"),
    (Tab::Sharing, "sharing"),
    (Tab::Remote, "remote"),
    (Tab::Console, "console"),
    (Tab::Settings, "settings"),
];

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_style(&cc.egui_ctx);
        let mut config = Config::load();
        let detected = config.autodetect_port();
        let services = Services::start(config);
        let ctx = cc.egui_ctx.clone();
        services.rig.set_repaint(move || ctx.request_repaint());
        if let Some(r) = &detected {
            services.rig.note(format!("auto-detected FTDX3000: CAT {} (USB {})", r.cat.stable_path(), r.usb_path));
            if let Some(a) = &r.audio {
                services.rig.note(format!("radio audio: ALSA {} — {}", a.alsa_plughw(), a.name));
            }
            if !r.cat.users.is_empty() {
                let names: Vec<String> = r.cat.users.iter().map(|u| format!("{} (pid {})", u.name, u.pid)).collect();
                services.rig.note(format!("CAT port is in use by {}", names.join(", ")));
            }
        }
        let sharing = sharing::SharingState::new(&services.config);
        let logo = {
            let icon = eframe::icon_data::from_png_bytes(ICON_PNG).expect("embedded icon");
            let img = egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba);
            cc.egui_ctx.load_texture("logo", img, egui::TextureOptions::LINEAR)
        };
        App {
            services,
            tab: Tab::Operate,
            radios: detect::detect_radios(),
            last_detect: Instant::now(),
            status: None,
            confirm: None,
            operate: Default::default(),
            menu: Default::default(),
            memories: Default::default(),
            sharing,
            console: Default::default(),
            remote: Default::default(),
            cw_key: Default::default(),
            logo,
            waterfall: Default::default(),
            screenshots: std::env::var_os("FTDX3000_SCREENSHOTS").map(|d| (d.into(), Instant::now(), 0)),
        }
    }

    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some((dir, at, idx)) = &mut self.screenshots else { return };
        // Give the radio time to report its state before the first shot.
        let wait = if *idx == 0 { Duration::from_secs(4) } else { Duration::from_millis(900) };
        if *idx > 0 && *idx <= SCREENSHOT_TABS.len() {
            self.tab = SCREENSHOT_TABS[*idx - 1].0;
            if self.tab == Tab::Remote && std::env::var_os("RIGHELM_FIREWALL_TEST").is_some() {
                remote_tab::show_firewall_help(&mut self.remote);
            }
        }
        if at.elapsed() < wait {
            ctx.request_repaint();
            return;
        }
        if *idx >= 1 && *idx <= SCREENSHOT_TABS.len() {
            let path = dir.join(format!("{}.ppm", SCREENSHOT_TABS[*idx - 1].1));
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                let mut out = format!("P6\n{} {}\n255\n", img.size[0], img.size[1]).into_bytes();
                for p in &img.pixels {
                    out.extend_from_slice(&[p.r(), p.g(), p.b()]);
                }
                let _ = std::fs::write(&path, out);
            })));
        }
        if *idx > SCREENSHOT_TABS.len() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        *idx += 1;
        *at = Instant::now();
        ctx.request_repaint();
    }

    pub fn rig(&self) -> &Rig {
        &self.services.rig
    }

    pub fn flash(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), Instant::now()));
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let link = self.rig().link();
        ui.horizontal(|ui| {
            let (dot, text) = match &link {
                LinkState::Connected { description } => (GREEN, description.clone()),
                LinkState::NoResponse { description } => (AMBER, format!("{description} — radio not answering")),
                LinkState::Connecting(s) => (AMBER, s.clone()),
                LinkState::Error(e) => (RED, e.clone()),
                LinkState::Disconnected => (Color32::GRAY, "disconnected".into()),
            };
            ui.label(RichText::new("●").color(dot).size(18.0));
            ui.add(egui::Image::new(&self.logo).fit_to_exact_size(egui::vec2(26.0, 26.0)));
            ui.label(RichText::new("RigHelm").strong().size(18.0).color(AMBER));
            ui.label(RichText::new("FTDX3000").weak());
            ui.add(egui::Label::new(RichText::new(text).weak()).truncate());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.services.cw().is_some()
                    && ui
                        .add(egui::Button::new(RichText::new("● CW KEY ARMED").strong().color(Color32::WHITE)).fill(RED))
                        .on_hover_text("Click to disarm the CW key")
                        .clicked()
                {
                    self.services.cw_arm(false);
                }
                if link.is_open() || matches!(link, LinkState::Error(_) | LinkState::Connecting(_)) {
                    if ui.button("Disconnect").clicked() {
                        self.rig().disconnect();
                    }
                } else if ui.button(RichText::new("Connect").strong()).clicked() {
                    if self.services.config.engine.serial.path.is_empty() {
                        self.services.config.autodetect_port();
                    }
                    self.services.connect();
                }
                if link.is_open() {
                    let on = self.rig().state().power_on != Some(false);
                    if on {
                        if self.confirm == Some(Confirm::PowerOff) {
                            if ui.button(RichText::new("Really power off?").color(RED)).clicked() {
                                self.rig().send("PS0;");
                                self.confirm = None;
                            }
                        } else if ui.button("Power off").clicked() {
                            self.confirm = Some(Confirm::PowerOff);
                        }
                    } else if ui.button("Power on").clicked() {
                        self.rig().send("PS1;");
                    }
                } else if matches!(link, LinkState::NoResponse { .. }) && ui.button("Power on").clicked() {
                    self.rig().send("PS1;");
                }
            });
        });
        ui.horizontal(|ui| {
            for (tab, label) in [
                (Tab::Operate, "Operate"),
                (Tab::Menu, "Radio menu"),
                (Tab::Memories, "Memories"),
                (Tab::Sharing, "Ports & sharing"),
                (Tab::Remote, "Remote (LAN)"),
                (Tab::Console, "CAT console"),
                (Tab::Settings, "Settings"),
            ] {
                if ui.selectable_label(self.tab == tab, RichText::new(label).size(15.0)).clicked() {
                    self.tab = tab;
                }
            }
            if let Some((msg, at)) = &self.status {
                if at.elapsed() < Duration::from_secs(6) {
                    ui.separator();
                    ui.label(RichText::new(msg).color(AMBER));
                }
            }
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.services.tick();
        if self.last_detect.elapsed() > Duration::from_secs(3) {
            self.radios = detect::detect_radios();
            self.last_detect = Instant::now();
        }
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui);
            ui.add_space(2.0);
        });
        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Operate => operate::show(self, ui),
            Tab::Menu => menu_tab::show(self, ui),
            Tab::Memories => memories::show(self, ui),
            Tab::Sharing => sharing::show(self, ui),
            Tab::Remote => remote_tab::show(self, ui),
            Tab::Console => console::show(self, ui),
            Tab::Settings => settings_tab::show(self, ui),
        });
        let ctx = ui.ctx().clone();
        self.screenshot_step(&ctx);
        // Meters keep moving even when nothing else changes.
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    }

    fn on_exit(&mut self) {
        if let Err(e) = self.services.config.save() {
            log::warn!("could not save config: {e}");
        }
        self.services.shutdown();
    }
}

fn setup_style(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(6.0, 6.0);
        style.spacing.slider_width = 170.0;
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
    });
}

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,righelm=info,ftdx3000_rig=info")).init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RigHelm — FTDX3000 Rig Control")
            .with_app_id("righelm")
            .with_icon(eframe::icon_data::from_png_bytes(ICON_PNG).expect("embedded icon"))
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native("RigHelm", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
