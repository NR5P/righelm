//! Remote operation over the LAN: serve this radio to other computers, or
//! control (and listen to) a radio on another computer.

use std::time::{Duration, Instant};

use eframe::egui::{self, ComboBox, RichText, Slider, Ui};
use ftdx3000_rig::audio::AudioDevice;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ftdx3000_rig::audio::DeviceEntry;
use ftdx3000_rig::firewall::{self, FirewallHelp, Role};
use ftdx3000_rig::remote::RemoteServerConfig;
use ftdx3000_rig::remote::{DEFAULT_PORT, RemoteTarget, local_addresses};

use crate::App;
use crate::widgets::{self, AMBER, CYAN, DIM, GREEN, RED};

pub struct RemoteState {
    addr: String,
    password: String,
    show_password: bool,
    nodes: Vec<DeviceEntry>,
    nodes_at: Option<Instant>,
    /// Server settings being edited (applied with the Apply button).
    server_edit: Option<RemoteServerConfig>,
    /// Firewall advice for (server, client), refreshed now and then.
    fw: Option<(Instant, Option<FirewallHelp>, Option<FirewallHelp>)>,
    fw_popup: Option<Role>,
    fw_running: Arc<AtomicBool>,
    fw_result: Arc<Mutex<Option<Result<(), String>>>>,
}

impl Default for RemoteState {
    fn default() -> Self {
        Self {
            addr: String::new(),
            password: String::new(),
            show_password: false,
            nodes: Vec::new(),
            nodes_at: None,
            server_edit: None,
            fw: None,
            fw_popup: None,
            fw_running: Arc::new(AtomicBool::new(false)),
            fw_result: Arc::new(Mutex::new(None)),
        }
    }
}

fn section(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).strong().size(16.0).color(AMBER));
        ui.add_space(4.0);
        body(ui);
    });
    ui.add_space(8.0);
}

fn device_combo(ui: &mut Ui, id: &str, dev: &mut AudioDevice, nodes: &[DeviceEntry], input: bool) -> bool {
    let label = match &*dev {
        AudioDevice::Default => "System default".to_string(),
        other => nodes.iter().find(|x| x.device == *other).map(|x| x.label.clone()).unwrap_or_else(|| other.to_string()),
    };
    let mut changed = false;
    ComboBox::from_id_salt(id).width(320.0).selected_text(label).show_ui(ui, |ui| {
        changed |= ui.selectable_value(dev, AudioDevice::Default, "System default").changed();
        for n in nodes.iter().filter(|n| n.input == input) {
            changed |= ui.selectable_value(dev, n.device.clone(), &n.label).changed();
        }
    });
    changed
}

pub fn show(app: &mut App, ui: &mut Ui) {
    if app.remote.nodes_at.is_none_or(|t| t.elapsed() > Duration::from_secs(10)) {
        app.remote.nodes = ftdx3000_rig::audio::list_devices();
        app.remote.nodes_at = Some(Instant::now());
        if app.remote.password.is_empty() {
            if let Some(t) = &app.services.config.last_remote {
                app.remote.addr = t.addr.clone();
                app.remote.password = t.password.clone();
            }
        }
    }
    if app.remote.fw.as_ref().is_none_or(|(t, _, _)| t.elapsed() > Duration::from_secs(15)) {
        app.remote.fw = Some((Instant::now(), firewall::check(Role::Server), firewall::check(Role::Client)));
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        client_section(app, ui);
        server_section(app, ui);
    });
    firewall_window(app, ui);
}

/// Open the firewall instructions (screenshot self-test).
pub fn show_firewall_help(state: &mut RemoteState) {
    state.fw = Some((Instant::now(), firewall::check(Role::Server), firewall::check(Role::Client)));
    state.fw_popup = Some(Role::Server);
}

/// The firewall advice for a role, if a firewall needs action.
fn fw_help(app: &App, role: Role) -> Option<FirewallHelp> {
    let (_, server, client) = app.remote.fw.as_ref()?;
    if role == Role::Server { server.clone() } else { client.clone() }
}

/// A one-line firewall status with a button to the instructions.
fn firewall_line(app: &mut App, ui: &mut Ui, role: Role) {
    let Some(help) = fw_help(app, role) else { return };
    ui.horizontal_wrapped(|ui| {
        if help.commands.is_empty() {
            ui.label(RichText::new(&help.note).weak());
        } else if app.services.config.ui.firewall_ok {
            ui.label(RichText::new(format!("✔ {} ports opened", help.firewall)).color(GREEN));
            if ui.small_button("Show again").clicked() {
                app.remote.fw_popup = Some(role);
            }
        } else {
            ui.label(RichText::new(format!("⚠ {}", help.note)).color(AMBER));
            if ui.button("Show me how").clicked() {
                app.remote.fw_popup = Some(role);
            }
        }
    });
}

fn firewall_window(app: &mut App, ui: &mut Ui) {
    let Some(role) = app.remote.fw_popup else { return };
    let Some(help) = fw_help(app, role) else {
        app.remote.fw_popup = None;
        return;
    };
    let mut open = true;
    let mut close = false;
    egui::Window::new("Open the firewall for RigHelm").collapsible(false).resizable(false).open(&mut open).show(ui.ctx(), |ui| {
        ui.label(&help.note);
        ui.add_space(6.0);
        if !help.commands.is_empty() {
            ui.label("Run these in a terminal (they ask for your password):");
            for c in &help.commands {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(c).monospace().color(CYAN));
                    if ui.small_button("Copy").clicked() {
                        ui.ctx().copy_text(c.clone());
                    }
                });
            }
            ui.add_space(6.0);
            let running = app.remote.fw_running.load(Ordering::Relaxed);
            if help.can_open() {
                ui.horizontal(|ui| {
                    if ui.add_enabled(!running, egui::Button::new(RichText::new("Open the ports for me").strong())).on_hover_text("Runs the commands above; your desktop asks for your password").clicked() {
                        app.remote.fw_running.store(true, Ordering::Relaxed);
                        *app.remote.fw_result.lock().unwrap() = None;
                        let (running, result, help, ctx) = (app.remote.fw_running.clone(), app.remote.fw_result.clone(), help.clone(), ui.ctx().clone());
                        std::thread::spawn(move || {
                            let r = help.open_ports();
                            *result.lock().unwrap() = Some(r);
                            running.store(false, Ordering::Relaxed);
                            ctx.request_repaint();
                        });
                    }
                    if running {
                        ui.spinner();
                        ui.label("waiting for your password…");
                    }
                });
            }
            let result = app.remote.fw_result.lock().unwrap().clone();
            match result {
                Some(Ok(())) => {
                    ui.label(RichText::new("✔ Ports opened.").color(GREEN));
                    if !app.services.config.ui.firewall_ok {
                        app.services.config.ui.firewall_ok = true;
                        let _ = app.services.config.save();
                    }
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(format!("Didn't work: {e}")).color(RED));
                }
                None => {}
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if !help.commands.is_empty() && ui.button("I've done this").clicked() {
                app.services.config.ui.firewall_ok = true;
                let _ = app.services.config.save();
                close = true;
            }
            if ui.button("Close").clicked() {
                close = true;
            }
        });
    });
    if !open || close {
        app.remote.fw_popup = None;
    }
}

fn client_section(app: &mut App, ui: &mut Ui) {
    let remote_now = app.services.config.engine.remote.clone();
    section(ui, "Control a radio on another computer", |ui| {
        if let Some(t) = &remote_now {
            ui.horizontal(|ui| {
                let link = app.rig().link();
                let (c, text) = if link.is_open() { (GREEN, "● connected") } else { (AMBER, "● connecting…") };
                ui.label(RichText::new(text).color(c));
                ui.label(RichText::new(format!("Controlling the radio at {}", t.addr)).strong());
                if ui.button(RichText::new("Back to local radio").strong()).clicked() {
                    app.services.use_local();
                }
            });
            audio_controls(app, ui);
            ui.separator();
        }
        ui.label("Radios found on your network:");
        let found = app.services.discovery.as_ref().map(|d| d.servers()).unwrap_or_default();
        if found.is_empty() {
            ui.label(RichText::new("  none yet — start this program on the computer with the radio and enable \"Let other computers use this radio\" below").color(DIM));
            firewall_line(app, ui, Role::Client);
        }
        egui::Grid::new("found").num_columns(4).spacing([12.0, 6.0]).show(ui, |ui| {
            for f in &found {
                let radio_col = if f.radio == "connected" { GREEN } else { AMBER };
                ui.label(RichText::new(&f.name).strong().size(15.0));
                ui.label(RichText::new(f.addr.to_string()).monospace());
                ui.label(RichText::new(format!("radio {}{}{}", f.radio, if f.audio { " · audio" } else { "" }, if f.auth { " · password required" } else { "" })).color(radio_col));
                if ui.button("Connect").clicked() {
                    app.remote.addr = f.addr.to_string();
                    let t = RemoteTarget { addr: f.addr.to_string(), password: app.remote.password.clone() };
                    app.services.connect_remote(t);
                    let _ = app.services.config.save();
                }
                ui.end_row();
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Address");
            ui.add(egui::TextEdit::singleline(&mut app.remote.addr).desired_width(200.0).hint_text(format!("192.168.1.20:{DEFAULT_PORT}")));
            ui.label("Password");
            ui.add(egui::TextEdit::singleline(&mut app.remote.password).password(!app.remote.show_password).desired_width(150.0));
            ui.checkbox(&mut app.remote.show_password, "show");
            let valid = !app.remote.addr.trim().is_empty();
            if ui.add_enabled(valid, egui::Button::new("Connect")).clicked() {
                let mut addr = app.remote.addr.trim().to_string();
                if !addr.contains(':') {
                    addr = format!("{addr}:{DEFAULT_PORT}");
                }
                app.services.connect_remote(RemoteTarget { addr, password: app.remote.password.clone() });
                let _ = app.services.config.save();
            }
        });
        ui.label(RichText::new("While connected, everything in this program — and its virtual ports and rigctld server — controls the remote radio, so WSJT-X etc. can run on this computer too.").weak());
    });
}

fn audio_controls(app: &mut App, ui: &mut Ui) {
    let st = app.services.audio_client_status();
    let nodes = app.remote.nodes.clone();
    let cfg = &mut app.services.config.remote_client;
    let mut changed = false;
    egui::Grid::new("audio").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label("Receive audio");
        ui.horizontal(|ui| {
            changed |= ui.checkbox(&mut cfg.audio, "Listen to the radio").changed();
            match &st {
                Some(s) if s.connected => {
                    let level = s.rx_level_db;
                    let frac = ((level + 60.0) / 60.0).clamp(0.0, 1.0);
                    widgets::Meter { label: "RX", fraction: frac, text: format!("{level:.0} dB"), color: GREEN, ticks: &[], red_from: Some(0.95), peak: None }
                        .show(ui, 260.0);
                    ui.label(RichText::new(format!("{} packets, {} lost", s.packets, s.lost)).weak());
                }
                Some(s) => {
                    ui.label(RichText::new(s.error.clone().unwrap_or("connecting…".into())).color(AMBER));
                }
                None => {
                    ui.label(RichText::new("off").color(DIM));
                }
            }
        });
        ui.end_row();
        ui.label("Volume");
        changed |= ui.add(Slider::new(&mut cfg.volume, 0.0..=4.0).step_by(0.05)).changed();
        ui.end_row();
        ui.label("Speaker");
        changed |= device_combo(ui, "speaker", &mut cfg.speaker, &nodes, false);
        ui.end_row();
        ui.label("Microphone");
        ui.horizontal(|ui| {
            changed |= ui.checkbox(&mut cfg.send_mic, "Send my microphone while transmitting").changed();
            if st.as_ref().is_some_and(|s| s.sending_mic) {
                ui.label(RichText::new("● ON AIR").color(RED).strong());
            }
        });
        ui.end_row();
        ui.label("");
        changed |= device_combo(ui, "mic", &mut cfg.mic, &nodes, true);
        ui.end_row();
        ui.label("Mic gain");
        changed |= ui.add(Slider::new(&mut cfg.mic_gain, 0.0..=4.0).step_by(0.05)).changed();
        ui.end_row();
    });
    ui.label(RichText::new("For voice/data over the network the radio must take TX audio from USB: menu 103 SSB MIC SEL / 075 DATA IN SELECT = USB.").weak());
    if changed {
        app.services.update_audio_client();
        let _ = app.services.config.save();
    }
}

fn server_section(app: &mut App, ui: &mut Ui) {
    let status = app.services.remote_server_status();
    let is_client = app.services.config.engine.remote.is_some();
    section(ui, "Let other computers use this radio", |ui| {
        if is_client {
            ui.label(RichText::new("Not available while this program is controlling a remote radio.").color(DIM));
            return;
        }
        let show_password = app.remote.show_password;
        let cfg = app.remote.server_edit.get_or_insert_with(|| app.services.config.remote_server.clone());
        egui::Grid::new("server").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Server");
            ui.checkbox(&mut cfg.enabled, "Allow remote control from my network");
            ui.end_row();
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut cfg.name).desired_width(200.0).hint_text(ftdx3000_rig::remote::hostname()));
            ui.end_row();
            ui.label("Password");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut cfg.password).password(!show_password).desired_width(200.0));
                if cfg.password.is_empty() {
                    ui.label(RichText::new("no password: remote users can listen and tune, but never transmit").color(AMBER));
                }
            });
            ui.end_row();
            ui.label("Permissions");
            ui.vertical(|ui| {
                ui.add_enabled(!cfg.password.is_empty(), egui::Checkbox::new(&mut cfg.allow_tx, "Allow transmit (PTT, TUNE, keyer, VOX) — needs a password"));
                ui.checkbox(&mut cfg.audio, "Stream receive audio");
                ui.checkbox(&mut cfg.tx_audio, "Accept microphone audio for transmit");
            });
            ui.end_row();
            ui.label("Listen on");
            ui.add(egui::TextEdit::singleline(&mut cfg.bind).desired_width(200.0));
            ui.end_row();
        });
        let edited = cfg.clone();
        let dirty = edited != app.services.config.remote_server;
        ui.horizontal(|ui| {
            if ui.add_enabled(dirty, egui::Button::new(RichText::new("Apply").strong())).clicked() {
                let was_enabled = app.services.config.remote_server.enabled;
                app.services.config.remote_server = edited.clone();
                app.services.apply_remote_server();
                let _ = app.services.config.save();
                // Turning the server on behind a firewall: say what to do.
                let needs_help = fw_help(app, Role::Server).is_some_and(|h| !h.commands.is_empty());
                if edited.enabled && !was_enabled && needs_help && !app.services.config.ui.firewall_ok {
                    app.remote.fw_popup = Some(Role::Server);
                }
            }
            if dirty {
                if ui.button("Revert").clicked() {
                    app.remote.server_edit = None;
                }
                ui.label(RichText::new("changes not applied yet").color(AMBER));
            }
        });
        if app.services.config.remote_server.enabled {
            firewall_line(app, ui, Role::Server);
        }
        match status {
            Some(Ok((addr, st))) => {
                ui.label(RichText::new(format!(
                    "● listening on {addr} — {} control, {} audio client(s){}",
                    st.cat_clients.len(),
                    st.audio_clients.len(),
                    st.rx_audio.map(|d| format!(" — audio from {d}")).unwrap_or_default()
                ))
                .color(GREEN));
                if let Some(e) = st.error {
                    ui.label(RichText::new(e).color(AMBER));
                }
                let port = addr.rsplit(':').next().unwrap_or("4533").to_string();
                let ips: Vec<String> = local_addresses().into_iter().map(|ip| format!("{ip}:{port}")).collect();
                ui.label(RichText::new(format!("This computer: {}", ips.join("   "))).color(CYAN));
            }
            Some(Err(e)) => {
                ui.label(RichText::new(format!("error: {e}")).color(RED));
            }
            None => {
                ui.label(RichText::new("off").color(DIM));
            }
        }
        ui.label(RichText::new(format!(
            "Other computers find this one automatically (UDP {}). If a firewall is on, allow TCP+UDP {DEFAULT_PORT} and UDP {}.",
            ftdx3000_rig::remote::DISCOVERY_PORT,
            ftdx3000_rig::remote::DISCOVERY_PORT
        ))
        .weak());
    });
}
