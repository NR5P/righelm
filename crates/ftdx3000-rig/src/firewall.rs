//! Firewall detection and advice for LAN remote.
//!
//! The server needs TCP+UDP 4533 (control, audio) and UDP 4534 (discovery)
//! open for incoming traffic; a client needs UDP 4534 to see discovery
//! replies. On Linux we detect ufw and firewalld (without root) and can open
//! the ports through the desktop's password prompt (pkexec). macOS and
//! Windows ask the user themselves the first time RigHelm listens.

use std::process::Command;

use crate::remote::{DEFAULT_PORT, DISCOVERY_PORT};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// This computer serves its radio to others.
    Server,
    /// This computer looks for radios on the network.
    Client,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirewallHelp {
    /// "ufw", "firewalld", "macOS firewall", ...
    pub firewall: &'static str,
    /// Commands to run in a terminal (empty if the system asks by itself).
    pub commands: Vec<String>,
    /// The same commands without `sudo`, runnable through pkexec.
    pkexec_script: Option<String>,
    pub note: String,
}

impl FirewallHelp {
    /// Can RigHelm open the ports itself (after a password prompt)?
    pub fn can_open(&self) -> bool {
        self.pkexec_script.is_some() && have("pkexec")
    }

    /// Run the commands through pkexec (shows the desktop's password
    /// dialog). Blocks until done.
    pub fn open_ports(&self) -> Result<(), String> {
        let script = self.pkexec_script.as_ref().ok_or("nothing to run")?;
        let out = Command::new("pkexec").args(["sh", "-c", script]).output().map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            Err(match out.status.code() {
                Some(126) => "cancelled".to_string(),
                Some(127) => "not authorized".to_string(),
                _ if err.is_empty() => format!("failed ({})", out.status),
                _ => err,
            })
        }
    }
}

fn have(cmd: &str) -> bool {
    Command::new("sh").args(["-c", &format!("command -v {cmd} >/dev/null")]).status().is_ok_and(|s| s.success())
}

fn ufw_enabled() -> bool {
    // For testing the UI on a machine without a firewall.
    if std::env::var("RIGHELM_FIREWALL_TEST").is_ok_and(|v| v == "ufw") {
        return true;
    }
    std::fs::read_to_string("/etc/ufw/ufw.conf").is_ok_and(|t| {
        t.lines().any(|l| l.trim().eq_ignore_ascii_case("ENABLED=yes"))
    })
}

fn firewalld_active() -> bool {
    Command::new("systemctl").args(["is-active", "--quiet", "firewalld"]).status().is_ok_and(|s| s.success())
}

/// What (if anything) the user must do so LAN remote works through the
/// firewall. `None` means no firewall was found.
pub fn check(role: Role) -> Option<FirewallHelp> {
    if cfg!(target_os = "macos") {
        return Some(FirewallHelp {
            firewall: "macOS firewall",
            commands: Vec::new(),
            pkexec_script: None,
            note: "If macOS asks whether RigHelm may accept incoming network connections, click Allow.".into(),
        });
    }
    if cfg!(windows) {
        return Some(FirewallHelp {
            firewall: "Windows Firewall",
            commands: Vec::new(),
            pkexec_script: None,
            note: "When Windows asks whether to allow RigHelm on your network, allow it for private networks.".into(),
        });
    }
    let (tcp_udp, udp) = (DEFAULT_PORT, DISCOVERY_PORT);
    if ufw_enabled() {
        let rules: Vec<String> = match role {
            Role::Server => vec![format!("ufw allow {tcp_udp}"), format!("ufw allow {udp}/udp")],
            Role::Client => vec![format!("ufw allow {udp}/udp")],
        };
        return Some(FirewallHelp {
            firewall: "ufw",
            commands: rules.iter().map(|r| format!("sudo {r}")).collect(),
            pkexec_script: Some(rules.join(" && ")),
            note: match role {
                Role::Server => format!("ufw is on: other computers can't reach this radio until TCP/UDP {tcp_udp} and UDP {udp} are allowed."),
                Role::Client => format!("ufw is on: replies from radios on your network (UDP {udp}) may be blocked, so none show up."),
            },
        });
    }
    if firewalld_active() {
        let ports = match role {
            Role::Server => format!("--add-port={tcp_udp}/tcp --add-port={tcp_udp}/udp --add-port={udp}/udp"),
            Role::Client => format!("--add-port={udp}/udp"),
        };
        let rules = [format!("firewall-cmd --permanent {ports}"), "firewall-cmd --reload".to_string()];
        return Some(FirewallHelp {
            firewall: "firewalld",
            commands: rules.iter().map(|r| format!("sudo {r}")).collect(),
            pkexec_script: Some(rules.join(" && ")),
            note: match role {
                Role::Server => format!("firewalld is on: other computers can't reach this radio until TCP/UDP {tcp_udp} and UDP {udp} are allowed."),
                Role::Client => format!("firewalld is on: replies from radios on your network (UDP {udp}) may be blocked."),
            },
        });
    }
    None
}
