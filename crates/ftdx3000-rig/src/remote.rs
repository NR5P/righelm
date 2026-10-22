//! Remote operation over the LAN: one instance owns the radio (server), others
//! find it automatically and control it as if it were local (clients), with
//! receive audio streamed to the client and, optionally, the client's
//! microphone streamed back to the radio.
//!
//! Protocol
//! --------
//! * Discovery (UDP 4534): servers broadcast a beacon every 2 s and answer
//!   `FTDX3000-DISCOVER` probes; clients listen and probe.
//! * Sessions (TCP 4533): the server greets with a random nonce; the client
//!   proves it knows the password with HMAC-SHA256(password, nonce|mode).
//!   - `CAT` sessions then carry plain FTDX3000 CAT, served exactly like a
//!     virtual serial port (cache, per-session auto-information, TX block).
//!   - `AUDIO` sessions keep the TCP connection as a heartbeat and exchange
//!     20 ms PCM frames over UDP on the same port number.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use ftdx3000_cat::Framer;
use hmac::{Hmac, KeyInit, Mac};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::audio::{self, AudioDevice, Capture, Playback};
use crate::engine::{Rig, Source};
use crate::transport::Link;
use crate::vport::{self, SharedWriter, VirtualPortConfig};

pub const DEFAULT_PORT: u16 = 4533;
pub const DISCOVERY_PORT: u16 = 4534;
pub const AUDIO_RATE: u32 = 24_000;
const GREETING: &str = "FTDX3000-REMOTE";
const PROBE: &[u8] = b"FTDX3000-DISCOVER";
const PROTOCOL: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteServerConfig {
    pub enabled: bool,
    pub bind: String,
    /// Name shown to clients (defaults to the host name).
    pub name: String,
    pub password: String,
    /// Allow remote clients to transmit (PTT, TUNE, keyer...).
    pub allow_tx: bool,
    pub audio: bool,
    /// Accept microphone audio from clients while the radio transmits.
    pub tx_audio: bool,
    /// Radio receive audio source; `None` = the radio's codec, auto-detected.
    pub rx_device: Option<AudioDevice>,
    /// Radio transmit audio sink; `None` = the radio's codec, auto-detected.
    pub tx_device: Option<AudioDevice>,
}

impl Default for RemoteServerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: format!("0.0.0.0:{DEFAULT_PORT}"),
            name: String::new(),
            password: String::new(),
            allow_tx: true,
            audio: true,
            tx_audio: false,
            rx_device: None,
            tx_device: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteClientConfig {
    /// Receive audio from the server.
    pub audio: bool,
    pub speaker: AudioDevice,
    pub volume: f32,
    /// Send the microphone to the radio while transmitting.
    pub send_mic: bool,
    pub mic: AudioDevice,
    pub mic_gain: f32,
}

impl Default for RemoteClientConfig {
    fn default() -> Self {
        Self { audio: true, speaker: AudioDevice::Default, volume: 1.0, send_mic: false, mic: AudioDevice::Default, mic_gain: 1.0 }
    }
}

/// A remote radio to connect to instead of a serial port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RemoteTarget {
    /// `host:port`
    pub addr: String,
    pub password: String,
}

/// Random per-process id so an instance doesn't list itself.
pub fn instance_id() -> u64 {
    static ID: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *ID.get_or_init(random_u64)
}

fn random_u64() -> u64 {
    let mut b = [0u8; 8];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).is_err() {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        b = (t.as_nanos() as u64 ^ (std::process::id() as u64).rotate_left(32)).to_le_bytes();
    }
    u64::from_le_bytes(b)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

fn mac(password: &str, nonce: &str, mode: &str) -> Hmac<Sha256> {
    let mut m = Hmac::<Sha256>::new_from_slice(password.as_bytes()).expect("HMAC takes any key length");
    m.update(format!("ftdx3000:{nonce}:{mode}").as_bytes());
    m
}

/// This machine's non-loopback IPv4 addresses.
pub fn local_addresses() -> Vec<String> {
    let mut out = Vec::new();
    for i in if_addrs::get_if_addrs().unwrap_or_default() {
        if let if_addrs::IfAddr::V4(v4) = &i.addr {
            if !v4.ip.is_loopback() && !out.contains(&v4.ip.to_string()) {
                out.push(v4.ip.to_string());
            }
        }
    }
    out
}

pub fn hostname() -> String {
    gethostname::gethostname().into_string().ok().filter(|h| !h.is_empty()).unwrap_or_else(|| "ftdx3000".into())
}

// ---------------------------------------------------------------- discovery

/// A server found on the LAN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub id: u64,
    pub name: String,
    pub addr: SocketAddr,
    pub audio: bool,
    pub auth: bool,
    /// The server plays clients' microphones into the radio.
    pub mic: bool,
    /// The server's own radio link state ("connected", "no-response", ...).
    pub radio: String,
    pub last_seen: Instant,
}

fn beacon_text(cfg: &RemoteServerConfig, port: u16, radio: &str) -> String {
    let name = if cfg.name.is_empty() { hostname() } else { cfg.name.clone() };
    let name: String = name.chars().filter(|c| *c != ';' && *c != '=').collect();
    format!(
        "{GREETING} v{PROTOCOL};id={:016x};name={name};port={port};audio={};auth={};mic={};radio={radio}",
        instance_id(),
        cfg.audio as u8,
        (!cfg.password.is_empty()) as u8,
        (cfg.audio && cfg.tx_audio && tx_allowed(cfg)) as u8
    )
}

fn parse_beacon(text: &str, from: SocketAddr) -> Option<Found> {
    let rest = text.strip_prefix(GREETING)?.trim_start();
    let mut fields: HashMap<&str, &str> = HashMap::new();
    let mut parts = rest.split(';');
    let _version = parts.next()?;
    for p in parts {
        if let Some((k, v)) = p.split_once('=') {
            fields.insert(k, v);
        }
    }
    let port: u16 = fields.get("port")?.parse().ok()?;
    Some(Found {
        id: u64::from_str_radix(fields.get("id")?, 16).ok()?,
        name: fields.get("name").unwrap_or(&"?").to_string(),
        addr: SocketAddr::new(from.ip(), port),
        audio: fields.get("audio") == Some(&"1"),
        auth: fields.get("auth") == Some(&"1"),
        mic: fields.get("mic") == Some(&"1"),
        radio: fields.get("radio").unwrap_or(&"?").to_string(),
        last_seen: Instant::now(),
    })
}

/// Broadcast addresses of all IPv4 interfaces, plus the limited broadcast.
fn broadcast_targets() -> Vec<SocketAddr> {
    let mut out = vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), DISCOVERY_PORT)];
    for i in if_addrs::get_if_addrs().unwrap_or_default() {
        if let if_addrs::IfAddr::V4(v4) = &i.addr {
            if let Some(b) = v4.broadcast.filter(|_| !v4.ip.is_loopback()) {
                let a = SocketAddr::new(IpAddr::V4(b), DISCOVERY_PORT);
                if !out.contains(&a) {
                    out.push(a);
                }
            }
        }
    }
    out
}

fn discovery_socket(port: u16) -> io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    s.set_reuse_address(true)?;
    // Lets two copies on one machine (and avahi etc.) share the port.
    #[cfg(unix)]
    s.set_reuse_port(true)?;
    s.set_broadcast(true)?;
    s.bind(&SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port).into())?;
    let s: UdpSocket = s.into();
    s.set_read_timeout(Some(Duration::from_millis(500)))?;
    Ok(s)
}

/// Background LAN discovery for clients.
pub struct Discovery {
    found: Arc<Mutex<HashMap<u64, Found>>>,
    stop: Arc<AtomicBool>,
}

impl Discovery {
    pub fn start() -> io::Result<Discovery> {
        let sock = discovery_socket(DISCOVERY_PORT).or_else(|_| discovery_socket(0))?;
        let found = Arc::new(Mutex::new(HashMap::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (found2, stop2) = (found.clone(), stop.clone());
        thread::Builder::new().name("discovery".into()).spawn(move || {
            let mut last_probe = Instant::now() - Duration::from_secs(60);
            let mut buf = [0u8; 512];
            while !stop2.load(Ordering::Relaxed) {
                if last_probe.elapsed() > Duration::from_secs(5) {
                    for t in broadcast_targets() {
                        let _ = sock.send_to(PROBE, t);
                    }
                    last_probe = Instant::now();
                }
                if let Ok((n, from)) = sock.recv_from(&mut buf) {
                    if let Some(f) = parse_beacon(&String::from_utf8_lossy(&buf[..n]), from) {
                        if f.id != instance_id() {
                            found2.lock().insert(f.id, f);
                        }
                    }
                }
                found2.lock().retain(|_, f| f.last_seen.elapsed() < Duration::from_secs(8));
            }
        })?;
        Ok(Discovery { found, stop })
    }

    pub fn servers(&self) -> Vec<Found> {
        let mut v: Vec<Found> = self.found.lock().values().cloned().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }
}

impl Drop for Discovery {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

// ------------------------------------------------------------------- server

/// UDP packet: "FX" ver type session:u64 seq:u32 payload(s16le).
const PKT_HELLO: u8 = 1;
const PKT_RX: u8 = 2;
const PKT_TX: u8 = 3;
const HEADER: usize = 16;

fn packet(kind: u8, session: u64, seq: u32, samples: &[i16]) -> Vec<u8> {
    let mut p = Vec::with_capacity(HEADER + samples.len() * 2);
    p.extend_from_slice(b"FX");
    p.push(1);
    p.push(kind);
    p.extend_from_slice(&session.to_le_bytes());
    p.extend_from_slice(&seq.to_le_bytes());
    for s in samples {
        p.extend_from_slice(&s.to_le_bytes());
    }
    p
}

fn parse_packet(p: &[u8]) -> Option<(u8, u64, u32, Vec<i16>)> {
    if p.len() < HEADER || &p[0..2] != b"FX" || p[2] != 1 {
        return None;
    }
    let session = u64::from_le_bytes(p[4..12].try_into().ok()?);
    let seq = u32::from_le_bytes(p[12..16].try_into().ok()?);
    let samples = p[HEADER..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    Some((p[3], session, seq, samples))
}

#[derive(Debug, Clone)]
struct AudioSession {
    peer: IpAddr,
    udp: Option<SocketAddr>,
    last_seen: Instant,
}

#[derive(Debug, Clone, Default)]
pub struct ServerStatus {
    pub tx_audio_packets: u64,
    pub cat_clients: Vec<String>,
    pub audio_clients: Vec<String>,
    pub rx_audio: Option<String>,
    pub error: Option<String>,
}

pub struct RemoteServer {
    pub addr: SocketAddr,
    stop: Arc<AtomicBool>,
    /// Held by every background thread; when only ours is left they're gone.
    alive: Arc<()>,
    status: Arc<Mutex<ServerStatus>>,
}

impl RemoteServer {
    pub fn start(rig: Rig, cfg: RemoteServerConfig) -> io::Result<RemoteServer> {
        let listener = TcpListener::bind(&cfg.bind)?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let udp = UdpSocket::bind(addr)?;
        udp.set_read_timeout(Some(Duration::from_millis(200)))?;
        let stop = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(());
        let status = Arc::new(Mutex::new(ServerStatus::default()));
        let sessions: Arc<Mutex<HashMap<u64, AudioSession>>> = Arc::new(Mutex::new(HashMap::new()));
        if cfg.password.is_empty() {
            rig.note("remote server has no password: remote clients can listen and tune but not transmit");
        }
        rig.note(format!("remote server listening on {addr} (TCP control, UDP audio)"));

        // Discovery: beacon + probe responder.
        {
            let (rig, cfg, stop) = (rig.clone(), cfg.clone(), stop.clone());
            let sock = discovery_socket(DISCOVERY_PORT).or_else(|_| discovery_socket(0))?;
            let alive_guard = alive.clone();
            thread::Builder::new().name("remote-beacon".into()).spawn(move || {
                let _alive_guard = alive_guard;
                let mut last = Instant::now() - Duration::from_secs(10);
                let mut buf = [0u8; 256];
                while !stop.load(Ordering::Relaxed) {
                    let radio = match rig.link() {
                        crate::LinkState::Connected { .. } => "connected",
                        crate::LinkState::NoResponse { .. } => "not-answering",
                        _ => "disconnected",
                    };
                    let text = beacon_text(&cfg, addr.port(), radio);
                    if last.elapsed() > Duration::from_secs(2) {
                        for t in broadcast_targets() {
                            let _ = sock.send_to(text.as_bytes(), t);
                        }
                        last = Instant::now();
                    }
                    if let Ok((n, from)) = sock.recv_from(&mut buf) {
                        if &buf[..n] == PROBE {
                            let _ = sock.send_to(text.as_bytes(), from);
                        }
                    }
                }
            })?;
        }

        // Control connections.
        {
            let gate = Arc::new(Gate::default());
            let (rig, cfg, stop, status, sessions) = (rig.clone(), cfg.clone(), stop.clone(), status.clone(), sessions.clone());
            let alive_guard = alive.clone();
            thread::Builder::new().name("remote-accept".into()).spawn(move || {
                let _alive_guard = alive_guard;
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, peer)) => {
                            let _ = stream.set_nonblocking(false);
                            let (rig, cfg, stop, status, sessions, gate) =
                                (rig.clone(), cfg.clone(), stop.clone(), status.clone(), sessions.clone(), gate.clone());
                            let _ = thread::Builder::new().name(format!("remote-{peer}")).spawn(move || {
                                if let Err(e) = serve_session(&rig, &cfg, stream, peer, &stop, &status, &sessions, &gate) {
                                    log::debug!("remote session {peer}: {e}");
                                }
                            });
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(100)),
                        Err(_) => thread::sleep(Duration::from_millis(100)),
                    }
                }
            })?;
        }

        // Audio: receive hellos and TX audio; stream RX audio to sessions.
        // The radio's sound card is looked up whenever a stream starts (it may
        // be switched on after us), and never replaced by the system default:
        // that would be this computer's own microphone and speakers.
        if cfg.audio {
            let udp = Arc::new(udp);
            {
                let (udp, stop, sessions, rig, cfg, status) = (udp.clone(), stop.clone(), sessions.clone(), rig.clone(), cfg.clone(), status.clone());
                let alive_guard = alive.clone();
                thread::Builder::new().name("remote-udp-in".into()).spawn(move || {
                    let _alive_guard = alive_guard;
                    let mut buf = [0u8; 4096];
                    let mut tx_play: Option<(Playback, Instant)> = None;
                    let mut next_open = Instant::now();
                    // One client's microphone at a time: never interleave two.
                    let mut talker: Option<(u64, Instant)> = None;
                    while !stop.load(Ordering::Relaxed) {
                        if let Some((_, at)) = &tx_play {
                            if at.elapsed() > Duration::from_secs(1) {
                                tx_play = None;
                            }
                        }
                        let Ok((n, from)) = udp.recv_from(&mut buf) else { continue };
                        let Some((kind, session, _seq, samples)) = parse_packet(&buf[..n]) else { continue };
                        let mut s = sessions.lock();
                        let Some(sess) = s.get_mut(&session) else { continue };
                        if from.ip() != sess.peer {
                            continue;
                        }
                        sess.udp = Some(from);
                        sess.last_seen = Instant::now();
                        drop(s);
                        if kind == PKT_TX && cfg.tx_audio && tx_allowed(&cfg) && crate::txaudio::mic_tx_allowed(&rig, true) {
                            if talker.is_some_and(|(id, at)| id != session && at.elapsed() < Duration::from_millis(500)) {
                                continue;
                            }
                            talker = Some((session, Instant::now()));
                            if tx_play.is_none() && Instant::now() >= next_open {
                                next_open = Instant::now() + Duration::from_secs(2);
                                match cfg.tx_device.clone().or_else(|| audio::radio_devices().1) {
                                    Some(dev) => match Playback::start(&dev, AUDIO_RATE, Duration::from_millis(120)) {
                                        Ok(p) => tx_play = Some((p, Instant::now())),
                                        Err(e) => status.lock().error = Some(format!("TX audio {dev}: {e}")),
                                    },
                                    None => status.lock().error = Some("TX audio: the radio's sound card was not found".into()),
                                }
                            }
                            if let Some((p, at)) = &mut tx_play {
                                p.push(samples);
                                *at = Instant::now();
                                status.lock().tx_audio_packets += 1;
                            }
                        }
                    }
                })?;
            }
            {
                let (stop, sessions, status, cfg) = (stop.clone(), sessions.clone(), status.clone(), cfg.clone());
                let alive_guard = alive.clone();
                thread::Builder::new().name("remote-udp-out".into()).spawn(move || {
                    let _alive_guard = alive_guard;
                    let mut capture: Option<Capture> = None;
                    let mut seq: u32 = 0;
                    // Never send faster than real time, whatever the device does.
                    let mut pacer = audio::Pacer::new(AUDIO_RATE, Duration::from_millis(60));
                    let mut idle_since = Instant::now();
                    while !stop.load(Ordering::Relaxed) {
                        let targets: Vec<(u64, SocketAddr)> = {
                            let mut s = sessions.lock();
                            s.retain(|_, a| a.last_seen.elapsed() < Duration::from_secs(10));
                            s.iter().filter_map(|(id, a)| a.udp.map(|u| (*id, u))).collect()
                        };
                        if targets.is_empty() {
                            if capture.is_some() && idle_since.elapsed() > Duration::from_secs(5) {
                                capture = None;
                                status.lock().rx_audio = None;
                            }
                            thread::sleep(Duration::from_millis(100));
                            continue;
                        }
                        idle_since = Instant::now();
                        if capture.as_mut().is_none_or(|c| !c.alive()) {
                            let Some(rx_dev) = cfg.rx_device.clone().or_else(|| audio::radio_devices().0) else {
                                status.lock().error = Some("RX audio: the radio's sound card was not found (is the radio on?)".into());
                                thread::sleep(Duration::from_secs(2));
                                continue;
                            };
                            match Capture::start(&rx_dev, AUDIO_RATE) {
                                Ok(c) => {
                                    let mut st = status.lock();
                                    st.rx_audio = Some(rx_dev.to_string());
                                    st.error = None;
                                    capture = Some(c);
                                }
                                Err(e) => {
                                    status.lock().error = Some(format!("RX audio: {e}"));
                                    thread::sleep(Duration::from_secs(2));
                                    continue;
                                }
                            }
                        }
                        let Some(c) = &capture else { continue };
                        if let Ok(frame) = c.frames.recv_timeout(Duration::from_millis(200)) {
                            if !pacer.admit(frame.len()) {
                                // Ahead of real time: wait instead of flooding.
                                thread::sleep(Duration::from_millis(5));
                                continue;
                            }
                            seq = seq.wrapping_add(1);
                            for (id, to) in &targets {
                                let _ = udp.send_to(&packet(PKT_RX, *id, seq, &frame), to);
                            }
                        }
                    }
                })?;
            }
        }
        Ok(RemoteServer { addr, stop, alive, status })
    }

    pub fn status(&self) -> ServerStatus {
        self.status.lock().clone()
    }
}

impl Drop for RemoteServer {
    /// Stops the server and waits for its threads to let go of the ports,
    /// so it can be restarted on the same port straight away.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let t0 = Instant::now();
        while Arc::strong_count(&self.alive) > 1 && t0.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(20));
        }
    }
}

/// Shared protection for the control port.
#[derive(Default)]
struct Gate {
    active: std::sync::atomic::AtomicUsize,
    /// Wrong-password count and time of the last failure, per address.
    failures: Mutex<HashMap<IpAddr, (u32, Instant)>>,
}

const MAX_SESSIONS: usize = 16;
const MAX_FAILURES: u32 = 5;
const LOCKOUT: Duration = Duration::from_secs(60);
/// The client engine polls several times a second; silence means it's gone.
const CAT_IDLE_TIMEOUT: Duration = Duration::from_secs(5);

impl Gate {
    fn locked_out(&self, ip: IpAddr) -> bool {
        let mut f = self.failures.lock();
        f.retain(|_, (_, at)| at.elapsed() < LOCKOUT);
        f.get(&ip).is_some_and(|(n, _)| *n >= MAX_FAILURES)
    }

    fn failed(&self, ip: IpAddr) {
        let mut f = self.failures.lock();
        let e = f.entry(ip).or_insert((0, Instant::now()));
        e.0 += 1;
        e.1 = Instant::now();
    }
}

#[allow(clippy::too_many_arguments)]
fn serve_session(
    rig: &Rig,
    cfg: &RemoteServerConfig,
    mut stream: TcpStream,
    peer: SocketAddr,
    stop: &Arc<AtomicBool>,
    status: &Arc<Mutex<ServerStatus>>,
    sessions: &Arc<Mutex<HashMap<u64, AudioSession>>>,
    gate: &Arc<Gate>,
) -> io::Result<()> {
    stream.set_nodelay(true)?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    socket2::SockRef::from(&stream).set_keepalive(true)?;
    if gate.active.load(Ordering::Relaxed) >= MAX_SESSIONS {
        stream.write_all(b"DENIED too many sessions\n")?;
        return Ok(());
    }
    if gate.locked_out(peer.ip()) {
        stream.write_all(b"DENIED too many wrong passwords, try again later\n")?;
        return Ok(());
    }
    gate.active.fetch_add(1, Ordering::Relaxed);
    struct Active<'a>(&'a Gate);
    impl Drop for Active<'_> {
        fn drop(&mut self) {
            self.0.active.fetch_sub(1, Ordering::Relaxed);
        }
    }
    let _active = Active(gate);

    let nonce = hex(&random_u64().to_le_bytes());
    let name = if cfg.name.is_empty() { hostname() } else { cfg.name.clone() };
    stream.write_all(format!("{GREETING} {PROTOCOL} {nonce} {name}\n").as_bytes())?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    // Bounded: an unauthenticated peer can't make us buffer more than this.
    (&mut reader).take(256).read_line(&mut line)?;
    let mut it = line.split_whitespace();
    let (Some("HELLO"), Some(mode), Some(proof)) = (it.next(), it.next(), it.next()) else {
        stream.write_all(b"DENIED bad hello\n")?;
        return Ok(());
    };
    let mode = mode.to_string();
    if !cfg.password.is_empty() {
        let ok = unhex(proof).is_some_and(|p| mac(&cfg.password, &nonce, &mode).verify_slice(&p).is_ok());
        if !ok {
            gate.failed(peer.ip());
            rig.note(format!("remote: {peer} gave a wrong password"));
            thread::sleep(Duration::from_millis(500));
            stream.write_all(b"DENIED wrong password\n")?;
            return Ok(());
        }
    }
    match mode.as_str() {
        "CAT" => {}
        "KEY" if tx_allowed(cfg) => {}
        "KEY" => {
            stream.write_all(b"DENIED this server does not allow transmit\n")?;
            return Ok(());
        }
        "AUDIO" if cfg.audio => {}
        "AUDIO" => {
            stream.write_all(b"DENIED audio is turned off on this server\n")?;
            return Ok(());
        }
        _ => {
            stream.write_all(b"DENIED unsupported mode\n")?;
            return Ok(());
        }
    }
    let session = random_u64();
    stream.write_all(format!("OK {session:016x} {} {AUDIO_RATE}\n", stream.local_addr()?.port()).as_bytes())?;
    let who = format!("remote {}", peer.ip());
    match mode.as_str() {
        "CAT" => {
            rig.note(format!("remote control session from {peer}"));
            status.lock().cat_clients.push(peer.to_string());
            let r = serve_cat(rig, cfg, stream, reader, &who, stop);
            status.lock().cat_clients.retain(|c| *c != peer.to_string());
            rig.note(format!("remote control session from {peer} ended"));
            r
        }
        "KEY" => {
            // Key the radio for a remote CW operator.
            match crate::cw::LocalKeyLine::open(rig, Source::Client(who.clone())) {
                Ok(mut line) => {
                    stream.write_all(b"READY\n")?;
                    rig.note(format!("remote CW key from {peer}"));
                    let r = crate::cw::serve_key(&mut line, stream, stop);
                    rig.note(format!("remote CW key from {peer} ended"));
                    r
                }
                Err(e) => {
                    stream.write_all(format!("ERR {e}\n").as_bytes())?;
                    Ok(())
                }
            }
        }
        "AUDIO" if cfg.audio => {
            rig.note(format!("remote audio session from {peer}"));
            sessions.lock().insert(session, AudioSession { peer: peer.ip(), udp: None, last_seen: Instant::now() });
            status.lock().audio_clients.push(peer.to_string());
            // Heartbeat: the session lives while the TCP connection does.
            stream.set_read_timeout(Some(Duration::from_secs(1)))?;
            let mut buf = [0u8; 64];
            let mut last = Instant::now();
            while !stop.load(Ordering::Relaxed) && last.elapsed() < Duration::from_secs(10) {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(_) => {
                        last = Instant::now();
                        if let Some(s) = sessions.lock().get_mut(&session) {
                            s.last_seen = Instant::now();
                        }
                    }
                    Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {}
                    Err(_) => break,
                }
            }
            sessions.lock().remove(&session);
            status.lock().audio_clients.retain(|c| *c != peer.to_string());
            rig.note(format!("remote audio session from {peer} ended"));
            Ok(())
        }
        _ => {
            stream.write_all(b"DENIED unsupported mode\n")?;
            Ok(())
        }
    }
}

fn serve_cat(rig: &Rig, cfg: &RemoteServerConfig, stream: TcpStream, mut reader: BufReader<TcpStream>, who: &str, stop: &Arc<AtomicBool>) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    let writer: SharedWriter = Arc::new(Mutex::new(Box::new(stream.try_clone()?)));
    let client = Arc::new(vport::ClientCtx::default());
    let connected = Arc::new(AtomicBool::new(true));
    let session_stop = Arc::new(AtomicBool::new(false));
    let _pusher = vport::ai_pusher(rig.clone(), writer.clone(), client.clone(), connected.clone(), session_stop.clone(), || {})?;
    let vcfg = VirtualPortConfig { name: who.into(), label: who.into(), enabled: true, block_tx: !tx_allowed(cfg), com_port: String::new() };
    let source = Source::Client(who.into());
    let mut framer = Framer::new();
    let mut buf = [0u8; 1024];
    let mut last_rx = Instant::now();
    let result = loop {
        if stop.load(Ordering::Relaxed) {
            break Ok(());
        }
        if last_rx.elapsed() > CAT_IDLE_TIMEOUT {
            break Err(io::Error::new(io::ErrorKind::TimedOut, "client went silent"));
        }
        let n = match reader.read(&mut buf) {
            Ok(0) => break Ok(()),
            Ok(n) => n,
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => continue,
            Err(e) => break Err(e),
        };
        last_rx = Instant::now();
        let mut failed = None;
        for f in framer.push(&buf[..n]) {
            if let Some(reply) = vport::handle(rig, &vcfg, &source, &client, &f) {
                if let Err(e) = writer.lock().write_all(reply.as_bytes()) {
                    failed = Some(e);
                    break;
                }
            }
        }
        if let Some(e) = failed {
            break Err(e);
        }
    };
    // Every exit path: stop pushing and release the transmitter if this
    // client was holding it.
    session_stop.store(true, Ordering::Relaxed);
    connected.store(false, Ordering::Relaxed);
    client.disconnected(rig, &source);
    let _ = stream.shutdown(std::net::Shutdown::Both);
    result
}

/// Transmitting needs both the setting and a password: an open server is
/// receive-only.
fn tx_allowed(cfg: &RemoteServerConfig) -> bool {
    cfg.allow_tx && !cfg.password.is_empty()
}

// ------------------------------------------------------------------- client

/// Connect, greet and authenticate. Returns the stream positioned after `OK`,
/// the session id, server UDP port and audio rate.
fn handshake(addr: &str, password: &str, mode: &str) -> io::Result<(TcpStream, String, u64, u16, u32)> {
    let mut addrs: Vec<SocketAddr> = std::net::ToSocketAddrs::to_socket_addrs(addr)?.collect();
    // "localhost" often resolves to ::1 first; servers usually listen on IPv4.
    addrs.sort_by_key(|a| a.is_ipv6());
    let mut last_err = io::Error::new(io::ErrorKind::NotFound, format!("cannot resolve {addr}"));
    let mut connected = None;
    for a in addrs {
        match TcpStream::connect_timeout(&a, Duration::from_secs(3)) {
            Ok(s) => {
                connected = Some((s, a));
                break;
            }
            Err(e) => last_err = e,
        }
    }
    let (stream, sock_addr) = connected.ok_or(last_err)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut it = line.split_whitespace();
    let (Some(GREETING), Some(_ver), Some(nonce)) = (it.next(), it.next(), it.next()) else {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not an FTDX3000 rig control server"));
    };
    let server_name = it.collect::<Vec<_>>().join(" ");
    let proof = hex(&mac(password, nonce, mode).finalize().into_bytes());
    (&stream).write_all(format!("HELLO {mode} {proof}\n").as_bytes())?;
    line.clear();
    reader.read_line(&mut line)?;
    let mut it = line.split_whitespace();
    match it.next() {
        Some("OK") => {
            let session = it.next().and_then(|s| u64::from_str_radix(s, 16).ok()).unwrap_or(0);
            let udp_port = it.next().and_then(|s| s.parse().ok()).unwrap_or(sock_addr.port());
            let rate = it.next().and_then(|s| s.parse().ok()).unwrap_or(AUDIO_RATE);
            if !(8_000..=48_000).contains(&rate) {
                return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unsupported audio rate {rate}")));
            }
            // Anything the BufReader read ahead would be lost; the server
            // sends nothing until we do, so there is none.
            Ok((stream, server_name, session, udp_port, rate))
        }
        Some("DENIED") => Err(io::Error::new(io::ErrorKind::PermissionDenied, line.trim().trim_start_matches("DENIED ").to_string())),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, format!("unexpected reply: {}", line.trim()))),
    }
}

/// Open a CW key session: the server arms its keying port, then answers.
pub fn open_key_session(target: &RemoteTarget) -> io::Result<TcpStream> {
    let (stream, _, _, _, _) = handshake(&target.addr, &target.password, "KEY")?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let reply = crate::cw::read_reply(&stream, Duration::from_secs(6))?;
    match reply.split_once(' ').map_or((reply.as_str(), ""), |(a, b)| (a, b)) {
        ("READY", _) => Ok(stream),
        ("ERR", e) => Err(io::Error::other(e.to_string())),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, format!("unexpected reply: {reply}"))),
    }
}

/// TCP reads that report end-of-stream as an error, so the engine notices.
struct TcpReader(TcpStream);

impl Read for TcpReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.read(buf)? {
            0 => Err(io::Error::new(io::ErrorKind::UnexpectedEof, "server closed the connection")),
            n => Ok(n),
        }
    }
}

/// Open a remote CAT link for the engine.
pub fn connect_cat(target: &RemoteTarget) -> io::Result<Link> {
    let (stream, name, _, _, _) = handshake(&target.addr, &target.password, "CAT")?;
    stream.set_read_timeout(Some(Duration::from_millis(50)))?;
    Ok(Link {
        reader: Box::new(TcpReader(stream.try_clone()?)),
        writer: Box::new(stream),
        description: format!("remote radio \"{name}\" at {}", target.addr),
    })
}

#[derive(Debug, Clone, Default)]
pub struct AudioClientStatus {
    pub connected: bool,
    pub server: String,
    pub rx_level_db: f32,
    pub packets: u64,
    pub lost: u64,
    pub sending_mic: bool,
    pub mic_level_db: f32,
    pub error: Option<String>,
}

/// Receives the remote radio's audio (and sends the microphone back).
pub struct AudioClient {
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<AudioClientStatus>>,
    config: Arc<Mutex<RemoteClientConfig>>,
    generation: Arc<AtomicU64>,
}

impl AudioClient {
    /// `tap` also receives the incoming radio audio (for the waterfall).
    pub fn start(rig: Rig, target: RemoteTarget, config: RemoteClientConfig, tap: Option<crate::spectrum::AudioTap>) -> AudioClient {
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(AudioClientStatus::default()));
        let config = Arc::new(Mutex::new(config));
        let generation = Arc::new(AtomicU64::new(0));
        let (stop2, status2, config2, gen2, tap2) = (stop.clone(), status.clone(), config.clone(), generation.clone(), tap);
        thread::Builder::new()
            .name("remote-audio".into())
            .spawn(move || {
                while !stop2.load(Ordering::Relaxed) {
                    if let Err(e) = audio_session(&rig, &target, &config2, &status2, &stop2, &gen2, tap2.as_ref()) {
                        let mut s = status2.lock();
                        s.connected = false;
                        s.error = Some(e.to_string());
                    }
                    for _ in 0..30 {
                        if stop2.load(Ordering::Relaxed) {
                            break;
                        }
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            })
            .expect("spawn audio client");
        AudioClient { stop, status, config, generation }
    }

    pub fn status(&self) -> AudioClientStatus {
        self.status.lock().clone()
    }

    /// Change volume/mic settings; device changes restart the streams.
    pub fn update(&self, cfg: RemoteClientConfig) {
        let mut cur = self.config.lock();
        if cur.speaker != cfg.speaker || cur.mic != cfg.mic || cur.send_mic != cfg.send_mic || cur.audio != cfg.audio {
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
        *cur = cfg;
    }
}

impl Drop for AudioClient {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn audio_session(
    rig: &Rig,
    target: &RemoteTarget,
    config: &Arc<Mutex<RemoteClientConfig>>,
    status: &Arc<Mutex<AudioClientStatus>>,
    stop: &Arc<AtomicBool>,
    generation: &Arc<AtomicU64>,
    tap: Option<&crate::spectrum::AudioTap>,
) -> io::Result<()> {
    if !config.lock().audio && !config.lock().send_mic {
        thread::sleep(Duration::from_millis(500));
        return Ok(());
    }
    let my_gen = generation.load(Ordering::Relaxed);
    let (mut control, name, session, udp_port, rate) = handshake(&target.addr, &target.password, "AUDIO")?;
    let server_ip = control.peer_addr()?.ip();
    let server_udp = SocketAddr::new(server_ip, udp_port);
    let any: IpAddr = if server_ip.is_ipv6() { Ipv6Addr::UNSPECIFIED.into() } else { Ipv4Addr::UNSPECIFIED.into() };
    let udp = UdpSocket::bind(SocketAddr::new(any, 0))?;
    udp.set_read_timeout(Some(Duration::from_millis(100)))?;
    let cfg = config.lock().clone();
    let mut speaker = Playback::start(&cfg.speaker, rate, Duration::from_millis(150))?;
    // A microphone that won't open must not cost the receive audio.
    let (mut mic, mic_error) = if cfg.send_mic {
        match audio::computer_mic(&cfg.mic).and_then(|d| Capture::start(&d, rate).map_err(|e| format!("microphone {d}: {e}"))) {
            Ok(c) => (Some(c), None),
            Err(e) => (None, Some(e)),
        }
    } else {
        (None, None)
    };
    {
        let mut s = status.lock();
        *s = AudioClientStatus { connected: true, server: name.clone(), error: mic_error, mic_level_db: -100.0, ..Default::default() };
    }
    rig.note(format!("receiving audio from \"{name}\""));
    let mut last_hello = Instant::now() - Duration::from_secs(5);
    let mut last_seq: Option<u32> = None;
    let mut buf = [0u8; 4096];
    let mut tx_seq: u32 = 0;
    let frame = audio::frame_len(rate);
    let result = loop {
        if stop.load(Ordering::Relaxed) || generation.load(Ordering::Relaxed) != my_gen {
            break Ok(());
        }
        if last_hello.elapsed() > Duration::from_secs(1) {
            udp.send_to(&packet(PKT_HELLO, session, 0, &[]), server_udp)?;
            if let Err(e) = control.write_all(b"\n") {
                break Err(e);
            }
            last_hello = Instant::now();
        }
        if !speaker.alive() {
            break Err(io::Error::other("speaker playback stopped"));
        }
        // Microphone: only while this operator transmits in a phone mode.
        if let Some(m) = &mic {
            let transmitting = crate::txaudio::mic_tx_allowed(rig, false);
            let gain = config.lock().mic_gain;
            let mut level = None;
            while let Ok(mut f) = m.frames.try_recv() {
                audio::apply_gain(&mut f, gain);
                level = Some(audio::level_dbfs(&f));
                if transmitting {
                    tx_seq = tx_seq.wrapping_add(1);
                    let _ = udp.send_to(&packet(PKT_TX, session, tx_seq, &f), server_udp);
                }
            }
            let mut s = status.lock();
            s.sending_mic = transmitting;
            if let Some(l) = level {
                s.mic_level_db = l;
            }
        }
        match udp.recv_from(&mut buf) {
            Ok((n, from)) if from.ip() == server_ip => {
                let Some((PKT_RX, sid, seq, mut samples)) = parse_packet(&buf[..n]) else { continue };
                if sid != session {
                    continue;
                }
                let mut lost = 0;
                match last_seq {
                    Some(l) if seq <= l && l.wrapping_sub(seq) < 1000 => continue, // late duplicate
                    Some(l) => {
                        let gap = seq.wrapping_sub(l).saturating_sub(1);
                        if (1..=5).contains(&gap) {
                            // Conceal short losses with silence to keep timing.
                            for _ in 0..gap {
                                speaker.push(vec![0; frame]);
                            }
                        }
                        lost = gap as u64;
                    }
                    None => {}
                }
                last_seq = Some(seq);
                if let Some(t) = tap {
                    t.publish(rate, &samples);
                }
                // Sending the mic without listening: keep quiet.
                let vol = if config.lock().audio { config.lock().volume } else { 0.0 };
                let level = audio::level_dbfs(&samples);
                audio::apply_gain(&mut samples, vol);
                speaker.push(samples);
                let mut s = status.lock();
                s.packets += 1;
                s.lost += lost;
                s.rx_level_db = level;
            }
            Ok(_) => {}
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {}
            Err(e) => break Err(e),
        }
        if mic.as_mut().is_some_and(|m| !m.alive()) {
            mic = None;
            status.lock().error = Some("microphone capture stopped".into());
        }
    };
    status.lock().connected = false;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineConfig;
    use crate::sim::SimRadio;

    #[test]
    fn restarts_on_the_same_port_immediately() {
        let rig = Rig::start();
        let cfg = RemoteServerConfig { bind: "127.0.0.1:0".into(), audio: true, rx_device: Some(AudioDevice::Alsa("null".into())), ..Default::default() };
        let first = RemoteServer::start(rig.clone(), cfg.clone()).unwrap();
        let addr = first.addr;
        drop(first);
        let again = RemoteServer::start(rig.clone(), RemoteServerConfig { bind: addr.to_string(), ..cfg });
        assert!(again.is_ok(), "{:?}", again.err());
        rig.shutdown();
    }

    #[test]
    fn beacon_round_trip() {
        let cfg = RemoteServerConfig { name: "shack".into(), password: "x".into(), ..Default::default() };
        let text = beacon_text(&cfg, 4533, "connected");
        let f = parse_beacon(&text, "192.168.1.5:4534".parse().unwrap()).unwrap();
        assert_eq!(f.name, "shack");
        assert_eq!(f.addr, "192.168.1.5:4533".parse().unwrap());
        assert!(f.auth);
        assert!(f.audio);
        assert_eq!(f.radio, "connected");
        assert_eq!(f.id, instance_id());
    }

    #[test]
    fn packets() {
        let p = packet(PKT_RX, 42, 7, &[1, -2, 3]);
        assert_eq!(parse_packet(&p), Some((PKT_RX, 42, 7, vec![1, -2, 3])));
        assert_eq!(parse_packet(b"nope"), None);
    }

    fn sim_rig() -> Rig {
        let sim = SimRadio::new();
        let rig = Rig::start();
        rig.connect_link(sim.link(), EngineConfig { read_menu_on_connect: false, ..Default::default() });
        rig
    }

    #[test]
    fn remote_control_end_to_end() {
        let server_rig = sim_rig();
        let cfg = RemoteServerConfig { bind: "127.0.0.1:0".into(), password: "secret".into(), audio: false, ..Default::default() };
        let server = RemoteServer::start(server_rig.clone(), cfg).unwrap();
        let addr = server.addr.to_string();

        // Wrong password is refused.
        let bad = connect_cat(&RemoteTarget { addr: addr.clone(), password: "nope".into() });
        assert_eq!(bad.err().map(|e| e.kind()), Some(io::ErrorKind::PermissionDenied));

        // A client engine drives the server's radio.
        let client = Rig::start();
        let link = connect_cat(&RemoteTarget { addr: addr.clone(), password: "secret".into() }).unwrap();
        client.connect_link(link, EngineConfig { read_menu_on_connect: false, ..Default::default() });
        let fa = client.read("FA;", crate::Freshness::Radio, Source::Gui, Duration::from_secs(3)).unwrap();
        assert_eq!(fa, "FA14074000;");
        client.send("FA07030000;");
        let t0 = Instant::now();
        while server_rig.state().freq_a != 7_030_000 && t0.elapsed() < Duration::from_secs(3) {
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(server_rig.state().freq_a, 7_030_000);
        // Front-panel changes on the server reach the client (auto information).
        server_rig.send("MD03;");
        let t0 = Instant::now();
        while client.state().mode_a != Some(ftdx3000_cat::Mode::Cw) && t0.elapsed() < Duration::from_secs(5) {
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(client.state().mode_a, Some(ftdx3000_cat::Mode::Cw));
        client.shutdown();
        server_rig.shutdown();
    }

    #[test]
    fn audio_streams_end_to_end() {
        // ALSA's "null" device captures silence and swallows playback, so the
        // whole pipeline runs without touching real sound hardware.
        if !std::path::Path::new("/usr/bin/arecord").exists() {
            return;
        }
        let server_rig = sim_rig();
        let null = AudioDevice::Alsa("null".into());
        let cfg = RemoteServerConfig {
            bind: "127.0.0.1:0".into(),
            audio: true,
            rx_device: Some(null.clone()),
            tx_device: Some(null.clone()),
            ..Default::default()
        };
        // Transmit (including mic audio) needs a password on the server.
        let cfg = RemoteServerConfig { tx_audio: true, password: "pw".into(), ..cfg };
        let server = RemoteServer::start(server_rig.clone(), cfg).unwrap();
        let target = RemoteTarget { addr: server.addr.to_string(), password: "pw".into() };
        // The client mirrors the remote radio's state over its own CAT session.
        let client_rig = Rig::start();
        client_rig.connect_link(connect_cat(&target).unwrap(), EngineConfig { read_menu_on_connect: false, ..Default::default() });
        let client = AudioClient::start(
            client_rig.clone(),
            target,
            RemoteClientConfig { speaker: null.clone(), mic: null, send_mic: true, ..Default::default() },
            None,
        );
        let t0 = Instant::now();
        while client.status().packets < 25 && t0.elapsed() < Duration::from_secs(6) {
            thread::sleep(Duration::from_millis(50));
        }
        let st = client.status();
        assert!(st.connected, "{st:?}");
        assert!(st.packets >= 25, "only {} audio packets in 6 s: {st:?}", st.packets);
        // Paced to real time: ~50 packets/s, not a flood.
        let rate = st.packets as f64 / t0.elapsed().as_secs_f64();
        assert!(rate < 80.0, "{rate:.0} packets/s");
        assert_eq!(server.status().audio_clients.len(), 1);

        // Microphone audio reaches the radio only while transmitting.
        thread::sleep(Duration::from_millis(300));
        assert_eq!(server.status().tx_audio_packets, 0, "mic must not be sent while receiving");
        client_rig.send("TX1;");
        let t0 = Instant::now();
        while server.status().tx_audio_packets < 10 && t0.elapsed() < Duration::from_secs(5) {
            thread::sleep(Duration::from_millis(50));
        }
        assert!(server.status().tx_audio_packets >= 10, "{:?}", server.status());
        client_rig.send("TX0;");
        drop(client);
        server_rig.shutdown();
        client_rig.shutdown();
    }

    #[test]
    fn open_server_is_receive_only_and_unkeys_lost_clients() {
        let sim = SimRadio::new();
        let server_rig = Rig::start();
        server_rig.connect_link(sim.link(), EngineConfig { read_menu_on_connect: false, ..Default::default() });
        // No password: tuning works, transmitting doesn't.
        let open = RemoteServer::start(server_rig.clone(), RemoteServerConfig { bind: "127.0.0.1:0".into(), audio: false, ..Default::default() }).unwrap();
        let mut c = TcpStream::connect(open.addr).unwrap();
        let mut line = String::new();
        BufReader::new(c.try_clone().unwrap()).read_line(&mut line).unwrap();
        c.write_all(b"HELLO CAT -\nFA07020000;TX1;AC002;").unwrap();
        thread::sleep(Duration::from_millis(400));
        assert_eq!(sim.core().freq_a, 7_020_000);
        assert_eq!(sim.core().tx, 0);
        drop(c);

        // With a password, a client that keys and then vanishes is un-keyed.
        let secured = RemoteServer::start(
            server_rig.clone(),
            RemoteServerConfig { bind: "127.0.0.1:0".into(), audio: false, password: "pw".into(), ..Default::default() },
        )
        .unwrap();
        let link = connect_cat(&RemoteTarget { addr: secured.addr.to_string(), password: "pw".into() }).unwrap();
        let mut w = link.writer;
        w.write_all(b"TX1;").unwrap();
        let t0 = Instant::now();
        while sim.core().tx != 1 && t0.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(sim.core().tx, 1);
        drop(w);
        drop(link.reader);
        let t0 = Instant::now();
        while sim.core().tx != 0 && t0.elapsed() < Duration::from_secs(3) {
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(sim.core().tx, 0, "radio left transmitting after the remote client vanished");
        server_rig.shutdown();
    }
}
