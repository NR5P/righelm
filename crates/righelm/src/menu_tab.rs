//! All 196 EX menu items, with backup and restore.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use eframe::egui::{self, ComboBox, DragValue, RichText, Ui};
use ftdx3000_cat::menu::{self, Kind, MENU, MenuItem, Value};

use crate::widgets::{AMBER, DIM, RED};
use crate::{App, Confirm};

/// Menu items that change how we talk to the radio; never restored by default.
const CAT_ITEMS: [u16; 4] = [37, 38, 39, 40];

pub struct MenuState {
    group: Option<&'static str>,
    filter: String,
    num_edits: HashMap<u16, i32>,
    text_edits: HashMap<u16, String>,
    path: String,
    /// Pending restore: item -> (current raw, new raw).
    restore: Option<BTreeMap<u16, (Option<String>, String)>>,
    include_cat: bool,
}

impl Default for MenuState {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        Self {
            group: None,
            filter: String::new(),
            num_edits: HashMap::new(),
            text_edits: HashMap::new(),
            path: format!("{home}/ftdx3000-menu-backup.txt"),
            restore: None,
            include_cat: false,
        }
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let rig = app.rig().clone();
    let menu_vals = rig.state().menu.clone();
    let live = rig.link().is_open();
    let read = menu_vals.len();

    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{read}/196 items read from the radio")).color(if read == 196 { crate::widgets::GREEN } else { AMBER }));
        if ui.add_enabled(live, egui::Button::new("Re-read all")).clicked() {
            rig.refresh_menu();
        }
        ui.separator();
        ui.label("Search");
        ui.add(egui::TextEdit::singleline(&mut app.menu.filter).desired_width(160.0).hint_text("name or number"));
    });
    ui.horizontal(|ui| {
        ui.label("Backup file");
        ui.add(egui::TextEdit::singleline(&mut app.menu.path).desired_width(360.0));
        if ui.add_enabled(read > 0, egui::Button::new("Save backup")).clicked() {
            match save_backup(&PathBuf::from(&app.menu.path), &menu_vals) {
                Ok(n) => app.flash(format!("saved {n} menu items to {}", app.menu.path)),
                Err(e) => app.flash(format!("backup failed: {e}")),
            }
        }
        if ui.add_enabled(live, egui::Button::new("Restore…")).clicked() {
            match load_backup(&PathBuf::from(&app.menu.path)) {
                Ok(items) => {
                    let diff: BTreeMap<u16, (Option<String>, String)> = items
                        .into_iter()
                        .filter(|(n, raw)| menu_vals.get(n) != Some(raw))
                        .map(|(n, raw)| (n, (menu_vals.get(&n).cloned(), raw)))
                        .collect();
                    if diff.is_empty() {
                        app.flash("radio already matches the backup");
                    } else {
                        app.menu.restore = Some(diff);
                        app.confirm = Some(Confirm::RestoreMenu);
                    }
                }
                Err(e) => app.flash(format!("could not read backup: {e}")),
            }
        }
    });
    restore_dialog(app, ui);

    ui.separator();
    let groups = menu::groups();
    egui::Panel::left("menu_groups").resizable(false).exact_size(150.0).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("groups").show(ui, |ui| {
            if ui.selectable_label(app.menu.group.is_none(), "All").clicked() {
                app.menu.group = None;
            }
            for g in groups {
                if ui.selectable_label(app.menu.group == Some(g), g).clicked() {
                    app.menu.group = Some(g);
                }
            }
        });
    });
    let filter = app.menu.filter.to_lowercase();
    let items: Vec<&MenuItem> = MENU
        .iter()
        .filter(|m| app.menu.group.is_none_or(|g| g == m.group))
        .filter(|m| filter.is_empty() || m.name.to_lowercase().contains(&filter) || format!("{:03}", m.num).contains(&filter))
        .collect();
    egui::ScrollArea::vertical().id_salt("items").auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("menu_grid").num_columns(5).striped(true).spacing([14.0, 6.0]).show(ui, |ui| {
            ui.label(RichText::new("No.").strong());
            ui.label(RichText::new("Group").strong());
            ui.label(RichText::new("Function").strong());
            ui.label(RichText::new("Value").strong());
            ui.label(RichText::new("Raw").strong());
            ui.end_row();
            for item in items {
                ui.label(RichText::new(format!("{:03}", item.num)).monospace());
                ui.label(RichText::new(item.group).weak());
                ui.label(item.name);
                let raw = menu_vals.get(&item.num);
                ui.add_enabled_ui(live, |ui| value_editor(app, ui, item, raw.map(String::as_str)));
                ui.label(RichText::new(raw.map(String::as_str).unwrap_or("")).monospace().color(DIM));
                ui.end_row();
            }
        });
    });
}

fn send_item(app: &App, item: &MenuItem, value: &Value, raw: &str) {
    let cmd = item.set_cmd(value, Some(raw.len()));
    app.rig().send(&cmd);
    app.rig().refresh(&[&menu::read_cmd(item.num)]);
}

fn value_editor(app: &mut App, ui: &mut Ui, item: &MenuItem, raw: Option<&str>) {
    let rig = app.rig().clone();
    let Some(raw) = raw else {
        ui.horizontal(|ui| {
            ui.label(RichText::new("not read").color(DIM));
            if ui.small_button("Read").clicked() {
                rig.refresh(&[&menu::read_cmd(item.num)]);
            }
        });
        return;
    };
    let decoded = item.decode(raw);
    match (item.kind, decoded) {
        (Kind::Choice(_) | Kind::Coded(_) | Kind::Steps { .. } | Kind::Tone, Some(Value::Int(v))) => {
            let opts = item.options();
            let cur = item.display(raw);
            ComboBox::from_id_salt(("menu", item.num)).width(170.0).selected_text(cur).show_ui(ui, |ui| {
                for (code, label) in opts {
                    if ui.selectable_label(code == v, label).clicked() && code != v {
                        send_item(app, item, &Value::Int(code), raw);
                    }
                }
            });
        }
        (Kind::Num { min, max, step, unit, div, .. }, Some(Value::Int(v))) => {
            // Edit a local copy while the user drags/types; send when done.
            let mut edit = app.menu.num_edits.get(&item.num).copied().unwrap_or(v);
            let mut dv = DragValue::new(&mut edit).range(min..=max).speed(step.max(1) as f64 * 0.2);
            if div > 1 {
                dv = dv.custom_formatter(move |x, _| format!("{:.1}", x / div as f64));
            }
            let r = ui.add(dv.suffix(if unit.is_empty() { String::new() } else { format!(" {unit}") }));
            let snapped = (edit / step.max(1)) * step.max(1);
            if r.drag_stopped() || r.lost_focus() {
                app.menu.num_edits.remove(&item.num);
                if snapped != v {
                    send_item(app, item, &Value::Int(snapped), raw);
                }
            } else if r.dragged() || r.has_focus() {
                app.menu.num_edits.insert(item.num, edit);
            } else {
                app.menu.num_edits.remove(&item.num);
            }
        }
        (Kind::MicGain, Some(Value::Int(v))) => {
            ui.horizontal(|ui| {
                let mut mcvr = v == 1000;
                if ui.checkbox(&mut mcvr, "MCVR (front knob)").changed() {
                    send_item(app, item, &Value::Int(if mcvr { 1000 } else { 50 }), raw);
                }
                if !mcvr {
                    let mut edit = app.menu.num_edits.get(&item.num).copied().unwrap_or(v);
                    let r = ui.add(DragValue::new(&mut edit).range(0..=100));
                    if r.drag_stopped() || r.lost_focus() {
                        app.menu.num_edits.remove(&item.num);
                        if edit != v {
                            send_item(app, item, &Value::Int(edit), raw);
                        }
                    } else if r.dragged() || r.has_focus() {
                        app.menu.num_edits.insert(item.num, edit);
                    }
                }
            });
        }
        (Kind::Text { max }, Some(Value::Text(t))) => {
            let edit = app.menu.text_edits.entry(item.num).or_insert(t.clone());
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(edit).char_limit(max).desired_width(140.0));
                if ui.small_button("Write").clicked() {
                    let v = Value::Text(edit.to_uppercase());
                    // Text has no fixed width; send as typed.
                    let cmd = item.set_cmd(&v, None);
                    rig.send(&cmd);
                    rig.refresh(&[&menu::read_cmd(item.num)]);
                }
            });
        }
        _ => {
            // The radio answered something the table doesn't expect: show it
            // raw and allow writing raw values of the same width.
            let edit = app.menu.text_edits.entry(item.num).or_insert(raw.to_string());
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(edit).char_limit(raw.len().max(1)).desired_width(80.0).font(egui::TextStyle::Monospace));
                if ui.small_button("Write raw").on_hover_text("Sent exactly as typed").clicked() && edit.len() == raw.len() {
                    rig.send(format!("EX{:03}{};", item.num, edit));
                    rig.refresh(&[&menu::read_cmd(item.num)]);
                }
                ui.label(RichText::new("unexpected value").color(AMBER).size(11.0));
            });
        }
    }
}

fn restore_dialog(app: &mut App, ui: &mut Ui) {
    if app.confirm != Some(Confirm::RestoreMenu) {
        return;
    }
    let Some(diff) = app.menu.restore.clone() else { return };
    let mut open = true;
    let mut apply = false;
    egui::Window::new("Restore menu settings").collapsible(false).open(&mut open).default_width(520.0).show(ui.ctx(), |ui| {
        ui.label(format!("{} items differ from the radio:", diff.len()));
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            egui::Grid::new("restore").striped(true).show(ui, |ui| {
                for (n, (cur, new)) in &diff {
                    let item = menu::item(*n);
                    let name = item.map(|i| i.name).unwrap_or("?");
                    let cat = CAT_ITEMS.contains(n);
                    ui.label(RichText::new(format!("{n:03} {name}")).color(if cat && !app.menu.include_cat { DIM } else { egui::Color32::WHITE }));
                    ui.label(cur.as_deref().map(|r| item.map(|i| i.display(r)).unwrap_or(r.to_string())).unwrap_or("?".into()));
                    ui.label("→");
                    ui.label(item.map(|i| i.display(new)).unwrap_or(new.clone()));
                    ui.end_row();
                }
            });
        });
        ui.checkbox(&mut app.menu.include_cat, "Also restore CAT settings (037-040) — may disconnect this program");
        ui.horizontal(|ui| {
            if ui.button(RichText::new("Write to radio").color(RED)).clicked() {
                apply = true;
            }
            if ui.button("Cancel").clicked() {
                app.confirm = None;
            }
        });
    });
    if apply {
        let rig = app.rig().clone();
        let mut n = 0;
        // CAT settings last, so the rest is written before the link may change.
        let (cat, rest): (Vec<_>, Vec<_>) = diff.iter().partition(|(k, _)| CAT_ITEMS.contains(k));
        for (num, (_, raw)) in rest.into_iter().chain(cat.into_iter().filter(|_| app.menu.include_cat)) {
            rig.send(format!("EX{num:03}{raw};"));
            n += 1;
        }
        rig.refresh_menu();
        app.flash(format!("wrote {n} menu items"));
        app.confirm = None;
    }
    if !open {
        app.confirm = None;
    }
}

fn save_backup(path: &PathBuf, vals: &BTreeMap<u16, String>) -> std::io::Result<usize> {
    let mut out = String::from("# FTDX3000 menu backup (ftdx3000 rig control)\n# format: EXnnn=raw value\n");
    for (n, raw) in vals {
        let item = menu::item(*n);
        out.push_str(&format!(
            "EX{n:03}={raw}    # {} {} = {}\n",
            item.map(|i| i.group).unwrap_or(""),
            item.map(|i| i.name).unwrap_or(""),
            item.map(|i| i.display(raw)).unwrap_or_default()
        ));
    }
    std::fs::write(path, out)?;
    Ok(vals.len())
}

fn load_backup(path: &PathBuf) -> std::io::Result<BTreeMap<u16, String>> {
    let text = std::fs::read_to_string(path)?;
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((k, v)) = line.split_once('=') else { continue };
        let Some(n) = k.trim().strip_prefix("EX").and_then(|n| n.parse::<u16>().ok()) else { continue };
        let v = v.trim_end().to_string();
        // Keep spaces inside text values (MY CALL) but nothing that breaks framing.
        if menu::item(n).is_some() && !v.contains(';') {
            out.insert(n, v);
        }
    }
    Ok(out)
}
