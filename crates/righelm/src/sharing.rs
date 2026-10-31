//! Radio detection, CAT port settings, and sharing the radio with other
//! programs (virtual serial ports, rigctld, Wine COM mapping).

use std::path::PathBuf;

use eframe::egui::{self, ComboBox, RichText, Ui};
use ftdx3000_rig::detect::{self, SerialPortEntry};
#[cfg(unix)]
use ftdx3000_rig::pty;
use ftdx3000_cat::model::Maker;
use ftdx3000_rig::transport::FlowControl;
use ftdx3000_rig::vport::VirtualPortConfig;
use ftdx3000_rig::{Config, EngineConfig};

use crate::App;
use crate::widgets::{AMBER, CYAN, DIM, GREEN, RED, ink};

#[cfg_attr(not(unix), allow(dead_code))] // the Wine fields are Unix-only
pub struct SharingState {
    engine: EngineConfig,
    ports: Vec<VirtualPortConfig>,
    rigctld: ftdx3000_rig::rigctld::RigctldConfig,
    serial_ports: Vec<SerialPortEntry>,
    wine_prefixes: Vec<PathBuf>,
    wine_prefix: usize,
    wine_com: u8,
    wine_port: usize,
    scanned: bool,
}

impl SharingState {
    pub fn new(config: &Config) -> Self {
        Self {
            engine: config.engine.clone(),
            ports: config.virtual_ports.clone(),
            rigctld: config.rigctld.clone(),
            serial_ports: Vec::new(),
            wine_prefixes: Vec::new(),
            wine_prefix: 0,
            wine_com: 3,
            wine_port: 0,
            scanned: false,
        }
    }
}

fn copy_button(ui: &mut Ui, text: &str) {
    if ui.small_button("Copy").on_hover_text(text).clicked() {
        ui.ctx().copy_text(text.to_string());
    }
}

fn section(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).strong().size(16.0).color(ink(AMBER)));
        ui.add_space(4.0);
        body(ui);
    });
    ui.add_space(8.0);
}

pub fn show(app: &mut App, ui: &mut Ui) {
    if !app.sharing.scanned {
        app.sharing.serial_ports = detect::list_serial_ports();
        #[cfg(unix)]
        {
            app.sharing.wine_prefixes = pty::find_wine_prefixes();
        }
        app.sharing.scanned = true;
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        radio_section(app, ui);
        cat_section(app, ui);
        vport_section(app, ui);
        rigctld_section(app, ui);
        #[cfg(unix)]
        wine_section(app, ui);
    });
}

fn radio_section(app: &mut App, ui: &mut Ui) {
    section(ui, "Detected radio", |ui| {
        if app.radios.is_empty() {
            ui.label(
                RichText::new("No radio found on USB (looking for the Silicon Labs, Icom or FTDI USB bridges radios use). Is the radio's USB cable plugged in? A radio on an RS-232 adapter can be chosen below.")
                    .color(ink(AMBER)),
            );
        }
        for p in &app.driver_problems {
            ui.label(RichText::new(p.text()).color(ink(RED)));
        }
        for r in app.radios.clone() {
            ui.label(RichText::new(format!("{} on USB {} ({}) — serial {}", r.title(), r.usb_path, r.bridge, r.serial_number.as_deref().unwrap_or("?"))).strong());
            egui::Grid::new(("radio", r.usb_path.clone())).num_columns(3).spacing([12.0, 4.0]).show(ui, |ui| {
                ui.label(if r.bridge == "CP2105" { "CAT (Enhanced COM)" } else { "CAT" });
                ui.label(RichText::new(&r.cat.dev).monospace());
                ui.horizontal(|ui| {
                    copy_button(ui, r.cat.stable_path());
                    if app.sharing.engine.serial.path != r.cat.stable_path() && ui.small_button("Use this port").clicked() {
                        app.sharing.engine.serial.path = r.cat.stable_path().to_string();
                    }
                    for u in &r.cat.users {
                        ui.label(RichText::new(format!("in use by {} (pid {})", u.name, u.pid)).color(ink(if u.pid == std::process::id() { GREEN } else { AMBER })));
                    }
                });
                ui.end_row();
                if let Some(k) = &r.keying {
                    ui.label("Keying (Standard COM)");
                    ui.label(RichText::new(&k.dev).monospace());
                    ui.horizontal(|ui| {
                        copy_button(ui, k.stable_path());
                        ui.label(RichText::new("RTS/DTR keying for PTT, CW and FSK (other programs can open this directly)").weak());
                    });
                    ui.end_row();
                }
                match &r.audio {
                    Some(a) => {
                        ui.label("Audio device");
                        ui.label(format!("card {} \"{}\" — {}", a.card_index, a.card_id, a.name));
                        ui.label("");
                        ui.end_row();
                        if cfg!(target_os = "linux") {
                            ui.label("  ALSA");
                            ui.label(RichText::new(a.alsa_plughw()).monospace());
                            copy_button(ui, &a.alsa_plughw());
                            ui.end_row();
                        }
                        if let Some((n, d)) = &a.source {
                            ui.label("  RX audio (input)");
                            ui.label(RichText::new(d).color(ink(CYAN))).on_hover_text(n);
                            copy_button(ui, n);
                            ui.end_row();
                        }
                        if let Some((n, d)) = &a.sink {
                            ui.label("  TX audio (output)");
                            ui.label(RichText::new(d).color(ink(CYAN))).on_hover_text(n);
                            copy_button(ui, n);
                            ui.end_row();
                        }
                    }
                    None => {
                        ui.label("Audio device");
                        ui.label(RichText::new("not found next to the CAT bridge").color(ink(AMBER)));
                        ui.end_row();
                    }
                }
            });
            ui.label(RichText::new("In WSJT-X/fldigi pick the audio devices named above (often \"USB Audio CODEC\") for input and output.").weak());
        }
    });
}

fn cat_section(app: &mut App, ui: &mut Ui) {
    let st = &mut app.sharing;
    let connected_model = app.services.rig.model();
    let connected = app.services.rig.link().is_open();
    section(ui, "CAT connection", |ui| {
        egui::Grid::new("cat").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Radio");
            ui.horizontal(|ui| {
                let chosen = st.engine.wanted_model();
                let text = chosen.map_or("Auto-detect".to_string(), |m| m.full_name());
                ComboBox::from_id_salt("model").width(220.0).selected_text(text).show_ui(ui, |ui| {
                    if ui.selectable_label(chosen.is_none(), "Auto-detect").clicked() {
                        st.engine.model.clear();
                    }
                    for maker in [Maker::Yaesu, Maker::Icom, Maker::Kenwood, Maker::Elecraft] {
                        let models: Vec<_> = ftdx3000_cat::model::all().iter().filter(|m| m.maker == maker).collect();
                        if models.is_empty() {
                            continue;
                        }
                        ui.separator();
                        ui.label(RichText::new(maker.name()).weak());
                        for m in models {
                            if ui.selectable_label(chosen == Some(*m), m.full_name()).clicked() {
                                st.engine.model = m.key.to_string();
                            }
                        }
                    }
                });
                if connected {
                    ui.label(RichText::new(format!("connected: {}", connected_model.full_name())).color(ink(CYAN)));
                }
            })
            .response
            .on_hover_text("Auto-detect asks the radio who it is. Choose the model if it isn't found, or if several share an answer (the Elecraft K3, KX3, KX2 and K4 all answer as a TS-570).");
            ui.end_row();
            ui.label("Port");
            ui.horizontal(|ui| {
                ComboBox::from_id_salt("catport").width(420.0).selected_text(if st.engine.serial.path.is_empty() { "(choose)" } else { &st.engine.serial.path }).show_ui(ui, |ui| {
                    for p in &st.serial_ports {
                        let path = p.port.stable_path().to_string();
                        if ui.selectable_label(st.engine.serial.path == path, format!("{}  — {}", p.port.dev, p.description)).clicked() {
                            st.engine.serial.path = path;
                        }
                    }
                    let sim = ftdx3000_rig::services::default_link_dir().join("sim").to_string_lossy().to_string();
                    if ui.selectable_label(st.engine.serial.path == sim, format!("{sim}  — simulator (run righelm-sim)")).clicked() {
                        st.engine.serial.path = sim;
                    }
                });
                if ui.small_button("Rescan").clicked() {
                    st.serial_ports = detect::list_serial_ports();
                }
            });
            ui.end_row();
            ui.label("");
            ui.add(egui::TextEdit::singleline(&mut st.engine.serial.path).desired_width(420.0).hint_text("or type a device path"));
            ui.end_row();
            ui.label("Baud rate");
            ComboBox::from_id_salt("baud").selected_text(if st.engine.serial.baud == 0 { "Auto-detect".to_string() } else { st.engine.serial.baud.to_string() }).show_ui(ui, |ui| {
                ui.selectable_value(&mut st.engine.serial.baud, 0, "Auto-detect");
                for b in [4800, 9600, 19200, 38400, 57600, 115200] {
                    ui.selectable_value(&mut st.engine.serial.baud, b, b.to_string());
                }
            });
            ui.end_row();
            ui.label("Handshake");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut st.engine.serial.flow, FlowControl::Hardware, "RTS/CTS");
                ui.selectable_value(&mut st.engine.serial.flow, FlowControl::None, "None (RTS held on)");
                ui.selectable_value(&mut st.engine.serial.flow, FlowControl::Quiet, "None (RTS/DTR off)")
                    .on_hover_text("For radios that can key the transmitter or send CW on RTS/DTR of the CAT port (Icom, Kenwood). Icom radios always use this.");
                ui.label(RichText::new("auto-detect tries both").weak());
            });
            ui.end_row();
            ui.label("Stop bits");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut st.engine.serial.stop_bits, 1, "1");
                ui.selectable_value(&mut st.engine.serial.stop_bits, 2, "2");
            });
            ui.end_row();
            ui.label("Options");
            ui.vertical(|ui| {
                ui.checkbox(&mut st.engine.auto_info, "Auto information (radio reports front-panel changes instantly)");
                let m = st.engine.wanted_model().unwrap_or(connected_model);
                ui.checkbox(&mut st.engine.read_menu_on_connect, format!("Read all {} menu items after connecting", m.menu.len()));
                match m.roles.cat_tot.and_then(|(item, _)| m.menu_item(item).map(|i| (item, i.name))) {
                    Some((item, name)) => {
                        ui.checkbox(&mut st.engine.set_cat_tot, format!("Set {name} (menu {item:03}) to 100 ms after connecting"));
                    }
                    None => {}
                }
            });
            ui.end_row();
            ui.label("Polling");
            ui.horizontal(|ui| {
                ui.label("meters every");
                ui.add(egui::DragValue::new(&mut st.engine.meter_poll_ms).range(50..=2000).suffix(" ms"));
                ui.label("VFOs every");
                ui.add(egui::DragValue::new(&mut st.engine.vfo_poll_ms).range(200..=10_000).suffix(" ms"));
            });
            ui.end_row();
        });
        ui.horizontal(|ui| {
            let remote = app.services.config.engine.remote.is_some();
            let mut local = st.engine.clone();
            local.remote = None;
            let changed = remote || local != EngineConfig { remote: None, ..app.services.config.engine.clone() };
            let label = if remote { "Use this local radio" } else if changed { "Apply & connect" } else { "Reconnect" };
            if ui.button(RichText::new(label).strong()).clicked() {
                app.services.config.engine = local;
                let _ = app.services.config.save();
                app.services.use_local();
            }
            if remote {
                ui.label(RichText::new("currently controlling a remote radio (see Remote tab)").color(ink(AMBER)));
            }
            ui.checkbox(&mut app.services.config.auto_connect, "Connect at start-up");
        });
        let hint = if connected_model.is_ftdx3000() {
            "Radio side: menu 037 CAT SELECT = USB, 038 CAT RATE = 38400 recommended. "
        } else if connected_model.family == ftdx3000_cat::Family::Icom {
            "Radio side: CI-V USB port = Unlink from [REMOTE], CI-V Transceive = ON. "
        } else {
            ""
        };
        ui.label(RichText::new(format!("{hint}This program opens the port exclusively; other programs share the radio through the ports below.")).weak());
    });
}

fn vport_section(app: &mut App, ui: &mut Ui) {
    let status = app.services.port_status();
    let dir = app.services.config.link_dir();
    let radio = app.services.rig.model();
    section(ui, "Virtual CAT ports (share the radio)", |ui| {
        ui.label(format!(
            "Each port is a virtual serial cable that looks exactly like the radio ({}). Point WSJT-X, fldigi, a logger, CW Skimmer, etc. at a port; all of them can run at once.",
            radio.full_name()
        ));
        ui.add_space(4.0);
        let st = &mut app.sharing;
        let mut remove = None;
        egui::Grid::new("vports").num_columns(8).striped(true).spacing([10.0, 6.0]).show(ui, |ui| {
            let path_header = if cfg!(windows) { "com0com port RigHelm opens (programs use its partner)" } else { "Path (use this in the other program)" };
            for h in ["On", "Name", "Used by", path_header, "Programs see", "Status", "Block TX", ""] {
                ui.label(RichText::new(h).strong());
            }
            ui.end_row();
            for (i, p) in st.ports.iter_mut().enumerate() {
                ui.checkbox(&mut p.enabled, "");
                ui.add_sized([70.0, 22.0], egui::TextEdit::singleline(&mut p.name));
                ui.add_sized([110.0, 22.0], egui::TextEdit::singleline(&mut p.label));
                ui.horizontal(|ui| {
                    if cfg!(windows) {
                        ui.add_sized([90.0, 22.0], egui::TextEdit::singleline(&mut p.com_port).hint_text("COM10"));
                        ui.label(RichText::new("needs com0com").weak());
                    } else {
                        let path = dir.join(&p.name).to_string_lossy().to_string();
                        ui.label(RichText::new(&path).monospace());
                        copy_button(ui, &path);
                    }
                });
                let seen = p.seen_as(radio);
                ComboBox::from_id_salt(("seen", i)).width(130.0).selected_text(seen.name).show_ui(ui, |ui| {
                    if !radio.is_ftdx3000() {
                        ui.selectable_value(&mut p.emulate, String::new(), format!("{} (the radio)", radio.name));
                    } else {
                        ui.selectable_value(&mut p.emulate, String::new(), "the radio");
                    }
                    ui.selectable_value(&mut p.emulate, "ftdx3000".to_string(), "FTDX3000");
                })
                .response
                .on_hover_text("Programs on this port see the radio as it is (choose it in the program), or an FTDX3000 whatever the radio (for programs set up for the FTDX3000)");
                match status.iter().find(|(c, _)| c.name == p.name) {
                    Some((_, Ok(s))) => {
                        if s.client_connected {
                            ui.label(RichText::new(format!("● in use  ({} cmds{})", s.frames_in, if s.client_ai { ", AI" } else { "" })).color(ink(GREEN)))
                                .on_hover_text(format!("last: {}  ({})", s.last_command, s.slave_path.display()));
                        } else {
                            ui.label(RichText::new("○ waiting").color(DIM)).on_hover_text(s.slave_path.display().to_string());
                        }
                    }
                    Some((_, Err(e))) => {
                        ui.label(RichText::new(format!("error: {e}")).color(ink(RED)));
                    }
                    None => {
                        ui.label(RichText::new(if p.enabled { "not started — Apply" } else { "off" }).color(DIM));
                    }
                }
                ui.checkbox(&mut p.block_tx, "").on_hover_text("Ignore TX/MOX commands from this program (e.g. a skimmer)");
                if ui.small_button("Remove").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
        if let Some(i) = remove {
            st.ports.remove(i);
        }
        ui.horizontal(|ui| {
            if ui.button("Add port").clicked() {
                let n = (1..).find(|n| !st.ports.iter().any(|p| p.name == format!("vcom{n}"))).unwrap_or(9);
                st.ports.push(VirtualPortConfig { name: format!("vcom{n}"), ..Default::default() });
            }
            let changed = st.ports != app.services.config.virtual_ports;
            if ui.add_enabled(changed, egui::Button::new(RichText::new("Apply").strong())).clicked() {
                // Names must be unique and safe as file names.
                st.ports.retain(|p| !p.name.is_empty() && !p.name.contains('/'));
                let mut seen = std::collections::HashSet::new();
                st.ports.retain(|p| seen.insert(p.name.clone()));
                app.services.config.virtual_ports = st.ports.clone();
                app.services.apply_sharing();
                let _ = app.services.config.save();
            }
        });
        ui.collapsing("How to set up other programs", |ui| {
            let rig = format!("{} {}", radio.maker.name(), radio.name);
            ui.label(format!("WSJT-X / JTDX:  Settings → Radio → Rig: {rig} (or Yaesu FTDX-3000 for a port that shows an FTDX3000), Serial port: a path above (type it in), Baud: any, Data/Stop bits: default, Handshake: None, PTT method: CAT."));
            ui.label(format!("fldigi:  Configure → Rig control → Hamlib → Rig: {}, Device: a path above, PTT via Hamlib command.", radio.name));
            ui.label(format!("flrig:  Transceiver: {}, Serial port: a path above.", radio.name));
            ui.label("Any program with Hamlib: \"Hamlib NET rigctl\" at 127.0.0.1:4532 (below) works for every radio, with no port at all.");
            ui.label("Wine programs (N1MM, CW Skimmer, Log4OM...): map a COM port below, then choose that COM port in the program.");
            ui.label("PTT: virtual ports carry CAT only. Use CAT PTT (TX1;) in other programs — RTS/DTR on a virtual port does nothing.");
        });
    });
}

fn rigctld_section(app: &mut App, ui: &mut Ui) {
    let status = app.services.rigctld_status();
    section(ui, "Hamlib network server (rigctld compatible)", |ui| {
        let st = &mut app.sharing;
        ui.horizontal(|ui| {
            ui.checkbox(&mut st.rigctld.enabled, "Enabled");
            ui.label("Listen on");
            ui.add(egui::TextEdit::singleline(&mut st.rigctld.bind).desired_width(150.0));
            ui.checkbox(&mut st.rigctld.block_tx, "Block TX");
            let changed = st.rigctld != app.services.config.rigctld;
            if ui.add_enabled(changed, egui::Button::new("Apply")).clicked() {
                app.services.config.rigctld = st.rigctld.clone();
                app.services.apply_sharing();
                let _ = app.services.config.save();
            }
            match status {
                Some(Ok((addr, n))) => ui.label(RichText::new(format!("● listening on {addr}, {n} client(s)")).color(ink(GREEN))),
                Some(Err(e)) => ui.label(RichText::new(format!("error: {e}")).color(ink(RED))),
                None => ui.label(RichText::new("off").color(DIM)),
            };
        });
        ui.label("Programs with Hamlib support can use Rig \"Hamlib NET rigctl\" with server 127.0.0.1:4532 — no serial port needed (WSJT-X, JTDX, fldigi, GridTracker, CQRLOG, Log4OM...).");
        ui.label(RichText::new("Listening on 0.0.0.0 makes it reachable from your LAN; there is no password, so only do that on a trusted network.").weak());
    });
}

#[cfg(unix)]
fn wine_section(app: &mut App, ui: &mut Ui) {
    let dir = app.services.config.link_dir();
    section(ui, "Wine COM port mapping", |ui| {
        let st = &mut app.sharing;
        if st.wine_prefixes.is_empty() {
            ui.label(RichText::new("No Wine prefixes found (looked in ~/.wine, $WINEPREFIX, and the CrossOver, Whisky, PlayOnMac, PlayOnLinux, Bottles and Lutris folders).").weak());
            return;
        }
        let ports: Vec<String> = st.ports.iter().filter(|p| p.enabled).map(|p| p.name.clone()).collect();
        ui.horizontal(|ui| {
            ui.label("Prefix");
            st.wine_prefix = st.wine_prefix.min(st.wine_prefixes.len() - 1);
            ComboBox::from_id_salt("wineprefix").width(300.0).selected_text(st.wine_prefixes[st.wine_prefix].display().to_string()).show_ui(ui, |ui| {
                for (i, p) in st.wine_prefixes.iter().enumerate() {
                    ui.selectable_value(&mut st.wine_prefix, i, p.display().to_string());
                }
            });
            ui.label("COM");
            ui.add(egui::DragValue::new(&mut st.wine_com).range(1..=32));
            ui.label("→");
            st.wine_port = st.wine_port.min(ports.len().saturating_sub(1));
            ComboBox::from_id_salt("wineport").selected_text(ports.get(st.wine_port).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                for (i, p) in ports.iter().enumerate() {
                    ui.selectable_value(&mut st.wine_port, i, p);
                }
            });
            if ui.add_enabled(!ports.is_empty(), egui::Button::new("Map")).clicked() {
                let target = dir.join(&ports[st.wine_port]);
                match pty::map_wine_com(&st.wine_prefixes[st.wine_prefix], st.wine_com, &target) {
                    Ok(link) => app.status = Some((format!("{} → {}", link.display(), target.display()), std::time::Instant::now())),
                    Err(e) => app.status = Some((format!("mapping failed: {e}"), std::time::Instant::now())),
                }
            }
        });
        let st = &app.sharing;
        let maps = pty::wine_com_ports(&st.wine_prefixes[st.wine_prefix]);
        if !maps.is_empty() {
            ui.label("Current COM ports in this prefix:");
            for (n, target) in maps {
                ui.label(RichText::new(format!("  COM{n} → {}", target.display())).monospace());
            }
        }
        ui.label(RichText::new("Restart the Wine program after changing COM mappings.").weak());
    });
}
