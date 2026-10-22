//! Configuration file and the set of running services (engine, virtual ports,
//! rigctld server), shared by the GUI and the headless daemon.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::audio::AudioDevice;
use crate::engine::{EngineConfig, Rig};
use crate::remote::{AudioClient, Discovery, RemoteClientConfig, RemoteServer, RemoteServerConfig, RemoteTarget};
use crate::rigctld::{RigctldConfig, RigctldServer};
use crate::vport::{VirtualPort, VirtualPortConfig, VirtualPortStatus};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub engine: EngineConfig,
    /// Connect automatically at start-up.
    pub auto_connect: bool,
    /// Directory for virtual port links (default `$XDG_RUNTIME_DIR/ftdx3000`).
    pub link_dir: Option<PathBuf>,
    pub virtual_ports: Vec<VirtualPortConfig>,
    pub rigctld: RigctldConfig,
    /// User-defined buttons that send a sequence of CAT commands.
    pub macros: Vec<Macro>,
    /// Let other computers on the LAN control this radio.
    pub remote_server: RemoteServerConfig,
    /// Audio settings used when controlling a remote radio.
    pub remote_client: RemoteClientConfig,
    /// Last remote radio used (address and password), for reconnecting.
    pub last_remote: Option<RemoteTarget>,
    /// Display preferences for the desktop app.
    pub ui: UiPrefs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiPrefs {
    /// Show the rotator panel (for a Yaesu rotator on the radio's ROT jack).
    pub show_rotator: bool,
    pub show_macros: bool,
    /// Tuning step in Hz for the mouse wheel and arrow keys.
    pub tune_step: i64,
    pub show_waterfall: bool,
    /// Highest audio frequency shown on the waterfall.
    pub wf_max_hz: u32,
    /// 1 = fast, 2 = normal, 4 = slow.
    pub wf_speed: u32,
    /// Follow the noise floor automatically.
    pub wf_auto: bool,
    pub wf_floor_db: i32,
    pub wf_range_db: i32,
    pub wf_height: u32,
}

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            show_rotator: false,
            show_macros: true,
            tune_step: 10,
            show_waterfall: true,
            wf_max_hz: 4000,
            wf_speed: 2,
            wf_auto: true,
            wf_floor_db: -110,
            wf_range_db: 60,
            wf_height: 180,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Macro {
    pub label: String,
    /// One or more CAT commands, e.g. `"FA14074000;MD0C;"`.
    pub commands: String,
}

fn default_macros() -> Vec<Macro> {
    let m = |l: &str, c: &str| Macro { label: l.into(), commands: c.into() };
    vec![
        m("FT8 40m", "FA07074000;MD0C;"),
        m("FT8 20m", "FA14074000;MD0C;"),
        m("FT8 15m", "FA21074000;MD0C;"),
        m("CW 20m", "FA14025000;MD03;"),
        m("SSB 20m", "FA14200000;MD02;"),
    ]
}

impl Default for Config {
    fn default() -> Self {
        Self {
            engine: EngineConfig::default(),
            auto_connect: true,
            link_dir: None,
            virtual_ports: vec![
                VirtualPortConfig { name: "vcom1".into(), label: "WSJT-X".into(), ..Default::default() },
                VirtualPortConfig { name: "vcom2".into(), label: "Logger".into(), ..Default::default() },
                VirtualPortConfig { name: "vcom3".into(), label: "CW Skimmer".into(), block_tx: true, ..Default::default() },
            ],
            rigctld: RigctldConfig::default(),
            macros: default_macros(),
            remote_server: RemoteServerConfig::default(),
            remote_client: RemoteClientConfig::default(),
            last_remote: None,
            ui: UiPrefs::default(),
        }
    }
}

impl Config {
    /// `$FTDX3000_CONFIG`, else `~/.config/ftdx3000/config.toml`.
    pub fn path() -> PathBuf {
        if let Some(p) = std::env::var_os("FTDX3000_CONFIG") {
            return PathBuf::from(p);
        }
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("ftdx3000").join("config.toml")
    }

    pub fn load() -> Config {
        match std::fs::read_to_string(Self::path()) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                log::warn!("ignoring invalid config {}: {e}", Self::path().display());
                Config::default()
            }),
            Err(_) => Config::default(),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    pub fn link_dir(&self) -> PathBuf {
        self.link_dir.clone().unwrap_or_else(default_link_dir)
    }

    /// Fill in the CAT port from auto-detection if none is configured or
    /// the configured one no longer exists.
    pub fn autodetect_port(&mut self) -> Option<crate::detect::DetectedRadio> {
        let radios = crate::detect::detect_radios();
        let first = radios.into_iter().next()?;
        let path = &self.engine.serial.path;
        if path.is_empty() || !std::path::Path::new(path).exists() {
            self.engine.serial.path = first.cat.stable_path().to_string();
        }
        Some(first)
    }
}

/// Default directory for the stable virtual-port links (Unix).
pub fn default_link_dir() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(d) => PathBuf::from(d).join("ftdx3000"),
        // macOS has no XDG runtime dir and its temp dir is a long random
        // path; keep the names short enough to type into WSJT-X.
        None if cfg!(unix) => PathBuf::from(format!("/tmp/ftdx3000-{}", std::env::var("USER").unwrap_or_else(|_| "user".into()))),
        None => std::env::temp_dir().join("ftdx3000"),
    }
}

/// Everything that runs while the program is up.
pub struct Services {
    pub rig: Rig,
    pub config: Config,
    ports: Vec<(VirtualPortConfig, Result<VirtualPort, String>)>,
    rigctld: Option<Result<RigctldServer, String>>,
    remote_server: Option<(RemoteServerConfig, Result<RemoteServer, String>)>,
    audio_client: Option<AudioClient>,
    pub discovery: Option<Discovery>,
    /// The radio's receive audio, local or from the remote server.
    pub rx_tap: crate::spectrum::AudioTap,
    local_rx: Option<crate::spectrum::LocalRxAudio>,
}

/// The radio codec's capture and playback devices, if detected.
pub fn radio_audio_devices() -> (Option<AudioDevice>, Option<AudioDevice>) {
    crate::audio::radio_devices()
}

impl Services {
    pub fn start(config: Config) -> Services {
        let rig = Rig::start();
        let mut s = Services {
            rig,
            config,
            ports: Vec::new(),
            rigctld: None,
            remote_server: None,
            audio_client: None,
            discovery: None,
            rx_tap: Default::default(),
            local_rx: None,
        };
        s.apply_local_rx();
        s.discovery = Discovery::start().map_err(|e| log::warn!("LAN discovery unavailable: {e}")).ok();
        s.apply_sharing();
        let have_target = s.config.engine.remote.is_some() || !s.config.engine.serial.path.is_empty();
        if s.config.auto_connect && have_target {
            s.connect();
        }
        s
    }

    /// (Re)start virtual ports, rigctld and the remote server to match the config.
    pub fn apply_sharing(&mut self) {
        self.apply_remote_server();
        let dir = self.config.link_dir();
        // Keep ports whose config didn't change; restart the rest.
        let wanted: Vec<VirtualPortConfig> = self.config.virtual_ports.iter().filter(|p| p.enabled).cloned().collect();
        self.ports.retain(|(cfg, _)| wanted.contains(cfg));
        for cfg in wanted {
            if self.ports.iter().any(|(c, _)| *c == cfg) {
                continue;
            }
            let port = VirtualPort::start(self.rig.clone(), cfg.clone(), dir.clone()).map_err(|e| e.to_string());
            if let Err(e) = &port {
                self.rig.note(format!("virtual port {} failed: {e}", cfg.name));
            }
            self.ports.push((cfg, port));
        }
        let rc = &self.config.rigctld;
        let restart = match &self.rigctld {
            Some(Ok(s)) => !rc.enabled || s.addr.to_string() != rc.bind,
            Some(Err(_)) => true,
            None => rc.enabled,
        };
        if restart {
            self.rigctld = None;
            if rc.enabled {
                let r = RigctldServer::start(self.rig.clone(), rc.clone()).map_err(|e| e.to_string());
                if let Err(e) = &r {
                    self.rig.note(format!("rigctld server on {} failed: {e}", rc.bind));
                }
                self.rigctld = Some(r);
            }
        }
    }

    /// (Re)start the LAN remote server to match the config.
    pub fn apply_remote_server(&mut self) {
        let want = self.config.remote_server.clone();
        // A remote client can't also serve the radio it doesn't own.
        let enabled = want.enabled && self.config.engine.remote.is_none();
        let same = self.remote_server.as_ref().is_some_and(|(c, r)| *c == want && r.is_ok());
        if !enabled {
            self.remote_server = None;
            return;
        }
        if same {
            return;
        }
        self.remote_server = None;
        let (rx, tx) = radio_audio_devices();
        let r = RemoteServer::start(self.rig.clone(), want.clone(), rx, tx).map_err(|e| e.to_string());
        if let Err(e) = &r {
            self.rig.note(format!("remote server on {} failed: {e}", want.bind));
        }
        self.remote_server = Some((want, r));
    }

    pub fn remote_server_status(&self) -> Option<Result<(String, crate::remote::ServerStatus), String>> {
        self.remote_server.as_ref().map(|(_, r)| r.as_ref().map(|s| (s.addr.to_string(), s.status())).map_err(|e| e.clone()))
    }

    pub fn audio_client_status(&self) -> Option<crate::remote::AudioClientStatus> {
        self.audio_client.as_ref().map(|a| a.status())
    }

    /// Record the local radio's audio (idle until something subscribes to
    /// `rx_tap`), unless we're controlling a remote radio.
    fn apply_local_rx(&mut self) {
        if self.config.engine.remote.is_some() {
            self.local_rx = None;
        } else if self.local_rx.is_none() {
            let device = || radio_audio_devices().0;
            self.local_rx = Some(crate::spectrum::LocalRxAudio::start(self.rx_tap.clone(), device, crate::remote::AUDIO_RATE));
        }
    }

    /// Where the waterfall's audio comes from, or why there is none.
    pub fn rx_audio_status(&self) -> Option<String> {
        match &self.local_rx {
            Some(l) => l.status.lock().clone(),
            None if self.config.engine.remote.is_some() => {
                Some(if self.config.remote_client.audio { "remote radio audio".into() } else { "turn on \"Listen to the radio\" in Remote (LAN)".into() })
            }
            None => None,
        }
    }

    /// Push changed client audio settings (volume, mic) to the running stream.
    pub fn update_audio_client(&mut self) {
        let want_audio = self.config.engine.remote.is_some() && self.config.remote_client.audio;
        match (&self.audio_client, want_audio) {
            (Some(a), true) => a.update(self.config.remote_client.clone()),
            (None, true) => {
                if let Some(t) = self.config.engine.remote.clone() {
                    self.audio_client = Some(AudioClient::start(self.rig.clone(), t, self.config.remote_client.clone(), Some(self.rx_tap.clone())));
                }
            }
            (Some(_), false) => self.audio_client = None,
            (None, false) => {}
        }
    }

    /// Control a radio on another computer instead of a local one.
    pub fn connect_remote(&mut self, target: RemoteTarget) {
        self.config.engine.remote = Some(target.clone());
        self.config.last_remote = Some(target);
        self.audio_client = None;
        self.apply_remote_server();
        self.apply_local_rx();
        self.connect();
        self.update_audio_client();
    }

    /// Go back to the locally connected radio.
    pub fn use_local(&mut self) {
        self.config.engine.remote = None;
        self.audio_client = None;
        self.rig.disconnect();
        self.apply_remote_server();
        self.apply_local_rx();
        if !self.config.engine.serial.path.is_empty() {
            self.connect();
        }
    }

    pub fn port_status(&self) -> Vec<(VirtualPortConfig, Result<VirtualPortStatus, String>)> {
        self.ports
            .iter()
            .map(|(c, p)| (c.clone(), p.as_ref().map(|p| p.status()).map_err(|e| e.clone())))
            .collect()
    }

    pub fn rigctld_status(&self) -> Option<Result<(String, usize), String>> {
        self.rigctld.as_ref().map(|r| r.as_ref().map(|s| (s.addr.to_string(), s.clients())).map_err(|e| e.clone()))
    }

    pub fn connect(&mut self) {
        self.rig.connect(self.config.engine.clone());
        self.update_audio_client();
    }

    pub fn shutdown(&mut self) {
        self.ports.clear();
        self.rigctld = None;
        self.remote_server = None;
        self.audio_client = None;
        self.discovery = None;
        self.local_rx = None;
        self.rig.shutdown();
        // Give the engine a moment to un-key and send AI0.
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
}
