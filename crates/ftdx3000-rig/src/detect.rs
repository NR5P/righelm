//! Find FTDX3000 CAT ports and the radio's USB audio codec without opening
//! anything.
//!
//! The FTDX3000 has an internal USB hub carrying a Silicon Labs CP2105 dual
//! UART (interface 0 = "Enhanced" port for CAT, interface 1 = "Standard" port
//! for RTS/DTR PTT, CW and FSK keying) and a TI/Burr-Brown PCM2903B audio codec.
//! Devices that share the hub are the same radio, which is how a CAT port is
//! paired with its audio device.
//!
//! Detection only reads sysfs/procfs. Ports are never opened here: opening a
//! tty raises RTS/DTR, and on the Standard port that can key the transmitter.

// The sysfs/procfs helpers are Linux-only; other systems use `other`.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const CP2105: (&str, &str) = ("10c4", "ea70");
const SYS_USB: &str = "/sys/bus/usb/devices";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortInfo {
    /// Kernel device, e.g. `/dev/ttyUSB1`.
    pub dev: String,
    /// Stable `/dev/serial/by-id/...` link, if udev created one.
    pub by_id: Option<String>,
    /// Processes (of this user) that currently have the port open.
    pub users: Vec<PortUser>,
}

impl PortInfo {
    /// Best path to store in a config: by-id survives re-plugging.
    pub fn stable_path(&self) -> &str {
        self.by_id.as_deref().unwrap_or(&self.dev)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortUser {
    pub pid: u32,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioInfo {
    pub card_index: u32,
    /// ALSA card id, e.g. `CODEC`.
    pub card_id: String,
    /// Long name, e.g. `Burr-Brown from TI USB Audio CODEC`.
    pub name: String,
    pub usb_path: String,
    /// PipeWire/PulseAudio sink (radio TX audio) node name and description.
    pub sink: Option<(String, String)>,
    /// PipeWire/PulseAudio source (radio RX audio) node name and description.
    pub source: Option<(String, String)>,
}

impl AudioInfo {
    pub fn alsa_hw(&self) -> String {
        format!("hw:CARD={},DEV=0", self.card_id)
    }

    pub fn alsa_plughw(&self) -> String {
        format!("plughw:CARD={},DEV=0", self.card_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedRadio {
    /// USB topology path of the CP2105, e.g. `3-1.2.1`.
    pub usb_path: String,
    pub serial_number: Option<String>,
    /// Enhanced COM port: CAT.
    pub cat: PortInfo,
    /// Standard COM port: RTS/DTR keying.
    pub keying: Option<PortInfo>,
    pub audio: Option<AudioInfo>,
}

/// Any serial port present on the system (for manual selection).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialPortEntry {
    pub port: PortInfo,
    pub description: String,
}

fn read_trim(p: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// tty name (e.g. `ttyUSB1`) under a USB interface directory.
fn tty_of_interface(iface: &Path) -> Option<String> {
    for entry in fs::read_dir(iface).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("ttyUSB") || name.starts_with("ttyACM") {
            return Some(name);
        }
        // cp210x: <iface>/ttyUSB1 ; cdc_acm: <iface>/tty/ttyACM0
        if name == "tty" {
            if let Some(t) = fs::read_dir(entry.path()).ok()?.flatten().next() {
                return Some(t.file_name().to_string_lossy().to_string());
            }
        }
    }
    None
}

/// Map `/dev/ttyUSB1` -> `/dev/serial/by-id/...`.
fn by_id_link(dev: &str) -> Option<String> {
    let dir = Path::new("/dev/serial/by-id");
    for entry in fs::read_dir(dir).ok()?.flatten() {
        if let Ok(target) = fs::canonicalize(entry.path()) {
            if target == Path::new(dev) {
                return Some(entry.path().to_string_lossy().to_string());
            }
        }
    }
    None
}

/// Processes owned by this user holding `dev` open (scans `/proc/*/fd`).
pub fn port_users(dev: &str) -> Vec<PortUser> {
    let target = fs::canonicalize(dev).unwrap_or_else(|_| PathBuf::from(dev));
    let me = std::process::id();
    let mut out = Vec::new();
    let Ok(procs) = fs::read_dir("/proc") else { return out };
    for p in procs.flatten() {
        let Ok(pid) = p.file_name().to_string_lossy().parse::<u32>() else { continue };
        if pid == me {
            continue;
        }
        let Ok(fds) = fs::read_dir(p.path().join("fd")) else { continue };
        if fds.flatten().any(|fd| fs::read_link(fd.path()).map(|l| l == target).unwrap_or(false)) {
            let name = read_trim(p.path().join("comm")).unwrap_or_else(|| "?".into());
            out.push(PortUser { pid, name });
        }
    }
    out
}

fn port_info(tty: &str) -> PortInfo {
    let dev = format!("/dev/{tty}");
    PortInfo { by_id: by_id_link(&dev), users: port_users(&dev), dev }
}

/// The parent hub path of a USB device path: `3-1.2.1` -> `3-1.2`.
fn parent_path(usb_path: &str) -> Option<&str> {
    usb_path.rsplit_once('.').map(|(p, _)| p)
}

/// Find a USB audio card attached to a device directly under `hub`.
fn audio_near(hub: &str) -> Option<AudioInfo> {
    let mut found = Vec::new();
    for entry in fs::read_dir(SYS_USB).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Interfaces look like `3-1.2.2:1.0`; we want siblings of the CP2105.
        let Some((dev, _iface)) = name.split_once(':') else { continue };
        if parent_path(dev) != Some(hub) {
            continue;
        }
        let sound = entry.path().join("sound");
        let Ok(cards) = fs::read_dir(&sound) else { continue };
        for card in cards.flatten() {
            let cname = card.file_name().to_string_lossy().to_string();
            let Some(idx) = cname.strip_prefix("card").and_then(|i| i.parse::<u32>().ok()) else { continue };
            let card_id = read_trim(card.path().join("id")).unwrap_or_else(|| format!("{idx}"));
            let mfr = read_trim(format!("{SYS_USB}/{dev}/manufacturer")).unwrap_or_default();
            let product = read_trim(format!("{SYS_USB}/{dev}/product")).unwrap_or_else(|| card_id.clone());
            let name = format!("{mfr} {product}").trim().to_string();
            found.push(AudioInfo { card_index: idx, card_id, name, usb_path: dev.to_string(), ..Default::default() });
        }
    }
    found.sort_by_key(|a| a.card_index);
    let mut audio = found.into_iter().next()?;
    let (mut sink, mut source) = pulse_nodes(audio.card_index);
    if sink.is_none() && source.is_none() {
        (sink, source) = pipewire_nodes(&audio.card_id);
    }
    audio.sink = sink;
    audio.source = source;
    Some(audio)
}

/// Look up PipeWire/PulseAudio sink and source names for an ALSA card using
/// `pactl` (works for both PulseAudio and pipewire-pulse).
fn pulse_nodes(card: u32) -> (Option<(String, String)>, Option<(String, String)>) {
    let find = |kind: &str| -> Option<(String, String)> {
        let out = Command::new("pactl").args(["list", kind]).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        parse_pactl(&text, card).into_iter().find(|(n, _)| !n.ends_with(".monitor"))
    };
    (find("sinks"), find("sources"))
}

/// Same lookup through `pw-cli` for PipeWire systems without `pactl`.
fn pipewire_nodes(card_id: &str) -> (Option<(String, String)>, Option<(String, String)>) {
    let Ok(out) = Command::new("pw-cli").args(["ls", "Node"]).output() else { return (None, None) };
    let nodes = parse_pw_cli(&String::from_utf8_lossy(&out.stdout), card_id);
    let pick = |class: &str| nodes.iter().find(|n| n.2 == class).map(|n| (n.0.clone(), n.1.clone()));
    (pick("Audio/Sink"), pick("Audio/Source"))
}

/// A PipeWire audio node (for choosing speakers/microphones).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioNode {
    pub name: String,
    pub description: String,
    /// `Audio/Sink` or `Audio/Source`.
    pub class: String,
}

/// All PipeWire audio sinks and sources.
pub fn list_audio_nodes() -> Vec<AudioNode> {
    let Ok(out) = Command::new("pw-cli").args(["ls", "Node"]).output() else { return Vec::new() };
    parse_pw_nodes(&String::from_utf8_lossy(&out.stdout))
}

pub(crate) fn parse_pw_nodes(text: &str) -> Vec<AudioNode> {
    let mut out = Vec::new();
    let mut cur: Option<AudioNode> = None;
    let unquote = |v: &str| v.trim().trim_matches('"').to_string();
    let flush = |n: Option<AudioNode>, out: &mut Vec<AudioNode>| {
        if let Some(n) = n.filter(|n| n.class == "Audio/Sink" || n.class == "Audio/Source") {
            out.push(n);
        }
    };
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("id ") && t.contains("type PipeWire:Interface:Node") {
            flush(cur.take(), &mut out);
            cur = Some(AudioNode { name: String::new(), description: String::new(), class: String::new() });
        } else if let (Some(c), Some((k, v))) = (cur.as_mut(), t.split_once(" = ")) {
            match k {
                "node.name" => c.name = unquote(v),
                "node.description" => c.description = unquote(v),
                "media.class" => c.class = unquote(v),
                _ => {}
            }
        }
    }
    flush(cur, &mut out);
    out
}

/// Parse `pw-cli ls Node` into `(node.name, node.description, media.class)`
/// for nodes whose `object.path` belongs to ALSA card `card_id`.
pub(crate) fn parse_pw_cli(text: &str, card_id: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut cur: Option<(String, String, String, bool)> = None;
    let needle = format!(":{card_id}:");
    let unquote = |v: &str| v.trim().trim_matches('"').to_string();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("id ") && t.contains("type PipeWire:Interface:Node") {
            if let Some((n, d, c, true)) = cur.take() {
                out.push((n, d, c));
            }
            cur = Some(Default::default());
        } else if let (Some(c), Some((k, v))) = (cur.as_mut(), t.split_once(" = ")) {
            match k {
                "node.name" => c.0 = unquote(v),
                "node.description" => c.1 = unquote(v),
                "media.class" => c.2 = unquote(v),
                "object.path" => c.3 = v.contains("alsa:") && v.contains(&needle),
                _ => {}
            }
        }
    }
    if let Some((n, d, c, true)) = cur {
        out.push((n, d, c));
    }
    out
}

/// Parse `pactl list sinks|sources` output and return `(name, description)`
/// of the nodes that belong to ALSA card `card`.
pub(crate) fn parse_pactl(text: &str, card: u32) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut name = None;
    let mut desc = String::new();
    let mut matches = false;
    let flush = |name: &mut Option<String>, desc: &mut String, matches: &mut bool, out: &mut Vec<(String, String)>| {
        if let Some(n) = name.take() {
            if *matches {
                out.push((n, std::mem::take(desc)));
            }
        }
        desc.clear();
        *matches = false;
    };
    for line in text.lines() {
        let t = line.trim();
        if !line.starts_with(char::is_whitespace) && (t.starts_with("Sink #") || t.starts_with("Source #")) {
            flush(&mut name, &mut desc, &mut matches, &mut out);
        } else if let Some(n) = t.strip_prefix("Name: ") {
            name = Some(n.to_string());
        } else if let Some(d) = t.strip_prefix("Description: ") {
            desc = d.to_string();
        } else if let Some((k, v)) = t.split_once(" = ") {
            if (k == "alsa.card" || k == "api.alsa.card") && v.trim_matches('"') == card.to_string() {
                matches = true;
            }
        }
    }
    flush(&mut name, &mut desc, &mut matches, &mut out);
    out
}

/// Find every connected FTDX3000 (CP2105 bridge), with paired audio.
pub fn detect_radios() -> Vec<DetectedRadio> {
    #[cfg(target_os = "linux")]
    return linux_detect_radios();
    #[cfg(not(target_os = "linux"))]
    return other::detect_radios();
}

/// All USB serial ports, for manual selection.
pub fn list_serial_ports() -> Vec<SerialPortEntry> {
    #[cfg(target_os = "linux")]
    return linux_list_serial_ports();
    #[cfg(not(target_os = "linux"))]
    return other::list_serial_ports();
}

#[cfg(target_os = "linux")]
fn linux_detect_radios() -> Vec<DetectedRadio> {
    let mut radios = Vec::new();
    let Ok(entries) = fs::read_dir(SYS_USB) else { return radios };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.contains(':') {
            continue;
        }
        let vid = read_trim(path.join("idVendor"));
        let pid = read_trim(path.join("idProduct"));
        if (vid.as_deref(), pid.as_deref()) != (Some(CP2105.0), Some(CP2105.1)) {
            continue;
        }
        let Some(cat_tty) = tty_of_interface(&path.join(format!("{name}:1.0"))) else { continue };
        let keying = tty_of_interface(&path.join(format!("{name}:1.1"))).map(|t| port_info(&t));
        let audio = parent_path(&name).and_then(audio_near);
        radios.push(DetectedRadio {
            serial_number: read_trim(path.join("serial")),
            cat: port_info(&cat_tty),
            keying,
            audio,
            usb_path: name,
        });
    }
    // Radios with paired audio first: that's almost certainly the FTDX3000.
    radios.sort_by_key(|r| (r.audio.is_none(), r.usb_path.clone()));
    radios
}

#[cfg(target_os = "linux")]
fn linux_list_serial_ports() -> Vec<SerialPortEntry> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/tty") else { return out };
    for entry in entries.flatten() {
        let tty = entry.file_name().to_string_lossy().to_string();
        if !(tty.starts_with("ttyUSB") || tty.starts_with("ttyACM")) {
            continue;
        }
        let dev_dir = fs::canonicalize(entry.path().join("device")).ok();
        // Walk up to the USB device directory to find product strings.
        let mut description = String::new();
        let mut cur = dev_dir;
        while let Some(d) = cur {
            if d.join("idVendor").exists() {
                let mfr = read_trim(d.join("manufacturer")).unwrap_or_default();
                let prod = read_trim(d.join("product")).unwrap_or_default();
                // cp210x: .../<iface>/ttyUSBn, so the interface name is one level up.
                let iface = fs::canonicalize(entry.path().join("device"))
                    .ok()
                    .and_then(|p| read_trim(p.parent()?.join("interface")))
                    .unwrap_or_default();
                description = format!("{mfr} {prod} {iface}").split_whitespace().collect::<Vec<_>>().join(" ");
                break;
            }
            cur = d.parent().map(Path::to_path_buf);
        }
        out.push(SerialPortEntry { port: port_info(&tty), description });
    }
    out.sort_by(|a, b| a.port.dev.cmp(&b.port.dev));
    out
}

/// macOS (and other non-Linux) detection through the serial library's USB
/// enumeration. On macOS only the `/dev/cu.*` call-out devices are used.
#[cfg(not(target_os = "linux"))]
mod other {
    use super::*;

    fn usable(name: &str) -> bool {
        !cfg!(target_os = "macos") || name.starts_with("/dev/cu.")
    }

    fn info(name: &str) -> PortInfo {
        PortInfo { dev: name.to_string(), by_id: None, users: Vec::new() }
    }

    pub fn detect_radios() -> Vec<DetectedRadio> {
        let ports = serialport::available_ports().unwrap_or_default();
        // CP2105 ports grouped by USB serial number: (interface, name).
        let mut groups: std::collections::BTreeMap<String, Vec<(u8, String)>> = Default::default();
        for p in ports {
            if let serialport::SerialPortType::UsbPort(u) = &p.port_type {
                if u.vid == 0x10c4 && u.pid == 0xea70 && usable(&p.port_name) {
                    let key = u.serial_number.clone().unwrap_or_default();
                    groups.entry(key).or_default().push((u.interface.unwrap_or(u8::MAX), p.port_name.clone()));
                }
            }
        }
        let (rx, tx) = crate::audio::radio_devices();
        let name_of = |d: &Option<crate::audio::AudioDevice>| d.as_ref().map(|d| d.to_string());
        let audio = rx.as_ref().map(|_| AudioInfo {
            card_id: name_of(&rx).unwrap_or_default(),
            name: name_of(&rx).unwrap_or_default(),
            source: name_of(&rx).map(|n| (n.clone(), n)),
            sink: name_of(&tx).map(|n| (n.clone(), n)),
            ..Default::default()
        });
        groups
            .into_iter()
            .filter_map(|(serial, mut ports)| {
                // Interface 0 is the Enhanced (CAT) port; without interface
                // numbers, the first name sorts first.
                ports.sort();
                let cat = ports.first()?.1.clone();
                let keying = ports.get(1).map(|p| info(&p.1));
                Some(DetectedRadio {
                    usb_path: serial.clone(),
                    serial_number: (!serial.is_empty()).then_some(serial),
                    cat: info(&cat),
                    keying,
                    audio: audio.clone(),
                })
            })
            .collect()
    }

    pub fn list_serial_ports() -> Vec<SerialPortEntry> {
        let mut out: Vec<SerialPortEntry> = serialport::available_ports()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| usable(&p.port_name))
            .map(|p| {
                let description = match &p.port_type {
                    serialport::SerialPortType::UsbPort(u) => {
                        let iface = match (u.vid, u.pid, u.interface) {
                            (0x10c4, 0xea70, Some(0)) => " Enhanced Com Port",
                            (0x10c4, 0xea70, Some(1)) => " Standard Com Port",
                            _ => "",
                        };
                        format!("{} {}{iface}", u.manufacturer.clone().unwrap_or_default(), u.product.clone().unwrap_or_default()).trim().to_string()
                    }
                    _ => String::new(),
                };
                SerialPortEntry { port: info(&p.port_name), description }
            })
            .collect();
        out.sort_by(|a, b| a.port.dev.cmp(&b.port.dev));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACTL: &str = r#"Sink #55
	State: SUSPENDED
	Name: alsa_output.usb-Burr-Brown_from_TI_USB_Audio_CODEC-00.analog-stereo
	Description: PCM2903B Audio CODEC Analog Stereo
	Properties:
		alsa.card = "1"
		device.bus_path = "pci-0000:3c:00.0-usb-0:1.2.2:1.0"

Sink #56
	Name: alsa_output.pci-0000_00_1f.3.analog-stereo
	Description: Built-in Audio
	Properties:
		alsa.card = "2"
"#;

    #[test]
    fn pactl_parsing() {
        let nodes = parse_pactl(PACTL, 1);
        assert_eq!(nodes.len(), 1);
        assert!(nodes[0].0.contains("Burr-Brown"));
        assert_eq!(nodes[0].1, "PCM2903B Audio CODEC Analog Stereo");
        assert_eq!(parse_pactl(PACTL, 2).len(), 1);
        assert!(parse_pactl(PACTL, 7).is_empty());
    }

    #[test]
    fn pw_cli_parsing() {
        let text = r#"	id 87, type PipeWire:Interface:Node/3
 		object.path = "alsa:acp:CODEC:4:playback"
 		node.description = "PCM2903B Audio CODEC Analog Stereo"
 		node.name = "alsa_output.usb-Burr-Brown_from_TI_USB_Audio_CODEC-00.analog-stereo"
 		media.class = "Audio/Sink"
	id 90, type PipeWire:Interface:Node/3
 		object.path = "alsa:acp:PCH:0:playback"
 		node.name = "alsa_output.pci"
 		media.class = "Audio/Sink"
"#;
        let n = parse_pw_cli(text, "CODEC");
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].2, "Audio/Sink");
        assert!(n[0].0.contains("Burr-Brown"));
    }

    #[test]
    fn parent() {
        assert_eq!(parent_path("3-1.2.1"), Some("3-1.2"));
        assert_eq!(parent_path("3-1"), None);
    }
}
