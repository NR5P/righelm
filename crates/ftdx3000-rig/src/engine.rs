//! The rig engine: the single owner of the radio's CAT port.
//!
//! Everything that wants to talk to the radio (the GUI, virtual serial ports,
//! the rigctld server) goes through a [`Rig`] handle. The engine thread:
//!
//! * serializes all traffic so commands from different programs never
//!   interleave on the wire,
//! * keeps at most one Read outstanding, so every answer (and every `?;`)
//!   can be attributed to the request that caused it,
//! * caches every answer (including unsolicited auto-information) so repeated
//!   reads from many programs are served without bothering the radio,
//! * polls meters and state in the background, and
//! * reconnects by itself when the USB cable is re-plugged.

use std::collections::{HashMap, VecDeque};
use std::io::{self, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, bounded, unbounded};
use ftdx3000_cat::command::{self, Request};
use ftdx3000_cat::model::{self, FTDX3000};
use ftdx3000_cat::{Framer, RadioModel, RadioState};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};

use crate::transport::{self, Link, SerialSettings};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    pub serial: SerialSettings,
    /// The radio model (`ftdx3000`, `ic7300`, ...); empty = find out from
    /// the radio when connecting.
    pub model: String,
    /// Control a radio owned by another instance on the network instead of
    /// a local serial port.
    pub remote: Option<crate::remote::RemoteTarget>,
    /// Turn on the radio's auto-information (`AI1;`) so changes made on the
    /// front panel arrive immediately instead of at the next poll.
    pub auto_info: bool,
    pub meter_poll_ms: u64,
    pub vfo_poll_ms: u64,
    /// Interval between reads of the slower-changing state items.
    pub misc_poll_ms: u64,
    pub read_timeout_ms: u64,
    /// Read every menu item after connecting.
    pub read_menu_on_connect: bool,
    /// Set CAT TIME OUT TIMER (FTDX3000 menu 039, and the same on other
    /// Yaesus) to 100 ms after connecting, like Hamlib does; the 10 ms
    /// default drops commands under load.
    pub set_cat_tot: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            serial: SerialSettings::default(),
            model: String::new(),
            remote: None,
            auto_info: true,
            meter_poll_ms: 150,
            vfo_poll_ms: 1000,
            misc_poll_ms: 150,
            read_timeout_ms: 500,
            read_menu_on_connect: true,
            set_cat_tot: true,
        }
    }
}

impl EngineConfig {
    /// The model chosen in the settings, if any (and known).
    pub fn wanted_model(&self) -> Option<&'static RadioModel> {
        if self.model.is_empty() { None } else { model::by_key(&self.model) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkState {
    Disconnected,
    Connecting(String),
    Connected { description: String },
    /// Port is open but the radio stopped answering (powered off?).
    NoResponse { description: String },
    /// Port failed (busy, unplugged, ...). Retrying automatically.
    Error(String),
}

impl LinkState {
    pub fn is_open(&self) -> bool {
        matches!(self, LinkState::Connected { .. } | LinkState::NoResponse { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatError {
    /// Radio answered `?;`.
    Rejected,
    Timeout,
    NotConnected,
    /// Not a valid command.
    Invalid,
}

impl std::fmt::Display for CatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            CatError::Rejected => "radio rejected the command (?;)",
            CatError::Timeout => "radio did not answer",
            CatError::NotConnected => "radio not connected",
            CatError::Invalid => "not a CAT command RigHelm knows",
        })
    }
}

impl std::error::Error for CatError {}

/// Who caused a piece of traffic (for the log and the console filter).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Source {
    Engine,
    Poll,
    Gui,
    Client(String),
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Engine => f.write_str("engine"),
            Source::Poll => f.write_str("poll"),
            Source::Gui => f.write_str("gui"),
            Source::Client(c) => f.write_str(c),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Tx,
    Rx,
    Info,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub at: Instant,
    pub dir: Dir,
    pub source: Option<Source>,
    pub text: String,
}

/// How old a cached answer may be for a read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// Engine decides: long for items the radio reports by itself (AI),
    /// short for everything else.
    Auto,
    Max(Duration),
    /// Always ask the radio.
    Radio,
}

/// A frame that changed (or confirmed) the radio state.
#[derive(Debug, Clone)]
pub struct Update {
    pub frame: String,
    /// The value differs from what was cached before (or is new).
    pub changed: bool,
    /// First value seen for this key since connecting.
    pub first: bool,
}

struct Cached {
    frame: String,
    at: Instant,
}

struct Shared {
    state: RwLock<RadioState>,
    cache: Mutex<HashMap<String, Cached>>,
    link: RwLock<LinkState>,
    log: Mutex<VecDeque<LogEntry>>,
    log_polls: AtomicBool,
    subscribers: Mutex<Vec<Sender<Update>>>,
    repaint: RwLock<Option<Box<dyn Fn() + Send + Sync>>>,
    /// Incremented whenever the state changes (cheap change detection).
    revision: AtomicU64,
    stats: Mutex<Stats>,
    /// Who sent the `TX1;`/`MX1;` that is in effect (CAT transmit only).
    tx_by: Mutex<Option<Source>>,
}

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub tx_frames: u64,
    pub rx_frames: u64,
    pub cache_hits: u64,
    pub timeouts: u64,
    pub rejected: u64,
}

const LOG_MAX: usize = 4000;

impl Shared {
    fn log(&self, dir: Dir, source: Option<Source>, text: impl Into<String>) {
        if source == Some(Source::Poll) && !self.log_polls.load(Ordering::Relaxed) {
            return;
        }
        let mut log = self.log.lock();
        if log.len() >= LOG_MAX {
            log.pop_front();
        }
        log.push_back(LogEntry { at: Instant::now(), dir, source, text: text.into() });
    }

    fn info(&self, text: impl Into<String>) {
        let text = text.into();
        log::info!("{text}");
        self.log(Dir::Info, None, text);
    }

    fn set_link(&self, l: LinkState) {
        let mut cur = self.link.write();
        if *cur != l {
            *cur = l;
            drop(cur);
            self.bump();
        }
    }

    fn bump(&self) {
        self.revision.fetch_add(1, Ordering::Relaxed);
        if let Some(f) = self.repaint.read().as_ref() {
            f();
        }
    }

    /// Record an answer frame. Returns true if the value changed.
    fn record(&self, key: &str, frame: &str) -> bool {
        self.record_as(key, frame, frame)
    }

    /// Cache `frame` under `key` but update the typed state from `state_frame`
    /// (they differ only for memory reads, see `on_frame`).
    fn record_as(&self, key: &str, frame: &str, state_frame: &str) -> bool {
        let (changed, first) = {
            let mut cache = self.cache.lock();
            let prev = cache.insert(key.to_string(), Cached { frame: frame.to_string(), at: Instant::now() });
            let first = prev.is_none();
            (first || prev.is_some_and(|c| c.frame != frame), first)
        };
        if changed {
            self.state.write().apply(state_frame);
        }
        let update = Update { frame: frame.to_string(), changed, first };
        self.subscribers.lock().retain(|s| s.send(update.clone()).is_ok());
        if changed {
            self.bump();
        }
        changed
    }

    fn cached(&self, key: &str, max_age: Duration) -> Option<String> {
        let cache = self.cache.lock();
        cache.get(key).filter(|c| c.at.elapsed() <= max_age).map(|c| c.frame.clone())
    }

    fn invalidate(&self, key: &str) {
        self.cache.lock().remove(key);
    }
}

type Reply = Sender<Result<String, CatError>>;

enum Job {
    Connect(EngineConfig),
    ConnectLink(Link, EngineConfig),
    Disconnect,
    /// `at`: when it was queued (commands from before a connection never go out on it).
    Set { cmd: String, src: Source, at: Instant },
    Read { key: String, cmd: String, fresh: Freshness, src: Source, reply: Option<Reply> },
    Raw { cmd: String, reply: Sender<Vec<String>> },
    Refresh(Vec<String>),
    Shutdown,
}

enum Msg {
    Job(Job),
    Rx { generation: u64, data: Vec<u8> },
    RxError { generation: u64, error: String },
}

/// Cloneable handle to the engine.
#[derive(Clone)]
pub struct Rig {
    shared: Arc<Shared>,
    tx: Sender<Msg>,
}

impl Rig {
    /// Start the engine thread (not connected yet).
    pub fn start() -> Rig {
        let shared = Arc::new(Shared {
            state: RwLock::new(RadioState::new()),
            cache: Mutex::new(HashMap::new()),
            link: RwLock::new(LinkState::Disconnected),
            log: Mutex::new(VecDeque::new()),
            log_polls: AtomicBool::new(false),
            subscribers: Mutex::new(Vec::new()),
            repaint: RwLock::new(None),
            revision: AtomicU64::new(0),
            stats: Mutex::new(Stats::default()),
            tx_by: Mutex::new(None),
        });
        let (tx, rx) = unbounded();
        let engine = Engine::new(shared.clone(), rx, tx.clone());
        thread::Builder::new().name("rig-engine".into()).spawn(move || engine.run()).expect("spawn engine");
        Rig { shared, tx }
    }

    fn job(&self, job: Job) {
        let _ = self.tx.send(Msg::Job(job));
    }

    /// Open the configured serial port (probing the baud rate if needed).
    pub fn connect(&self, config: EngineConfig) {
        self.job(Job::Connect(config));
    }

    /// Use an already-open link (simulator, tests).
    pub fn connect_link(&self, link: Link, config: EngineConfig) {
        self.job(Job::ConnectLink(link, config));
    }

    pub fn disconnect(&self) {
        self.job(Job::Disconnect);
    }

    pub fn shutdown(&self) {
        self.job(Job::Shutdown);
    }

    pub fn link(&self) -> LinkState {
        self.shared.link.read().clone()
    }

    pub fn state(&self) -> parking_lot::RwLockReadGuard<'_, RadioState> {
        self.shared.state.read()
    }

    /// The radio connected (or last connected; FTDX3000 before any).
    pub fn model(&self) -> &'static RadioModel {
        self.shared.state.read().model
    }

    /// Who keyed the radio by CAT (`TX1;`/`MX1;`), if it is CAT-keyed.
    /// `None` while transmitting means the radio's own PTT, key or VOX.
    pub fn tx_source(&self) -> Option<Source> {
        if self.shared.state.read().tx == 1 || self.shared.state.read().mox { self.shared.tx_by.lock().clone() } else { None }
    }

    pub fn revision(&self) -> u64 {
        self.shared.revision.load(Ordering::Relaxed)
    }

    pub fn stats(&self) -> Stats {
        self.shared.stats.lock().clone()
    }

    pub fn set_repaint(&self, f: impl Fn() + Send + Sync + 'static) {
        *self.shared.repaint.write() = Some(Box::new(f));
    }

    pub fn set_log_polls(&self, on: bool) {
        self.shared.log_polls.store(on, Ordering::Relaxed);
    }

    pub fn log_snapshot(&self, max: usize) -> Vec<LogEntry> {
        let log = self.shared.log.lock();
        log.iter().skip(log.len().saturating_sub(max)).cloned().collect()
    }

    pub fn clear_log(&self) {
        self.shared.log.lock().clear();
    }

    pub fn note(&self, text: impl Into<String>) {
        self.shared.info(text);
    }

    /// Receive every answer frame (radio answers, auto-information and
    /// optimistic updates from Set commands).
    pub fn subscribe(&self) -> Receiver<Update> {
        let (tx, rx) = unbounded();
        self.shared.subscribers.lock().push(tx);
        rx
    }

    /// Cached answer for a key, if any (no age limit).
    pub fn cached(&self, key: &str) -> Option<String> {
        self.shared.cached(key, Duration::MAX)
    }

    /// Queue a Set command from the GUI.
    pub fn send(&self, cmd: impl AsRef<str>) {
        self.send_from(cmd, Source::Gui);
    }

    /// Queue one or more Set commands (`"AB;SV;"` works).
    pub fn send_from(&self, cmd: impl AsRef<str>, src: Source) {
        let mut f = Framer::new();
        for frame in f.push(cmd.as_ref().as_bytes()) {
            self.job(Job::Set { cmd: command::normalize(&frame), src: src.clone(), at: Instant::now() });
        }
    }

    /// Ask the engine to re-read keys soon (e.g. after opening a panel).
    pub fn refresh(&self, read_cmds: &[&str]) {
        self.job(Job::Refresh(read_cmds.iter().map(|s| command::normalize(s)).collect()));
    }

    /// Re-read every menu item.
    pub fn refresh_menu(&self) {
        let cmds = self.model().menu.iter().map(|m| ftdx3000_cat::menu::read_cmd(m.num)).collect();
        self.job(Job::Refresh(cmds));
    }

    /// Blocking read. `cmd` is a Read command like `"FA;"` or `"EX038;"`.
    pub fn read(&self, cmd: &str, fresh: Freshness, src: Source, timeout: Duration) -> Result<String, CatError> {
        let cmd = command::normalize(cmd);
        let Request::Read { key } = command::classify(&cmd) else { return Err(CatError::Invalid) };
        if let Freshness::Max(age) = fresh {
            if let Some(hit) = self.shared.cached(&key, age) {
                self.shared.stats.lock().cache_hits += 1;
                return Ok(hit);
            }
        }
        let (reply, rx) = bounded(1);
        self.job(Job::Read { key, cmd, fresh, src, reply: Some(reply) });
        rx.recv_timeout(timeout).unwrap_or(Err(CatError::Timeout))
    }

    /// Send anything (console). Returns every frame received in the next
    /// `window` (all frames are also processed normally).
    pub fn raw(&self, cmd: &str, window: Duration) -> Vec<String> {
        let (reply, rx) = bounded(1);
        self.job(Job::Raw { cmd: cmd.to_string(), reply });
        rx.recv_timeout(window + Duration::from_secs(1)).unwrap_or_default()
    }
}

struct Outstanding {
    key: String,
    cmd: String,
    src: Source,
    deadline: Instant,
    retried: bool,
}

struct PollItem {
    cmd: String,
    key: String,
    next: Instant,
    every: Duration,
}

/// Whether a poll item runs in the current TX/RX state: the S-meter while
/// receiving, TX meters while transmitting; `ID` only in no-response mode.
fn poll_eligible(p: &PollItem, transmitting: bool) -> bool {
    if p.key == "ID" {
        false
    } else if p.key.starts_with("SM") {
        !transmitting
    } else if p.key.starts_with("RM") {
        transmitting
    } else {
        true
    }
}

struct RawCapture {
    until: Instant,
    frames: Vec<String>,
    reply: Sender<Vec<String>>,
}

/// Readable state items polled in the background, slowest group.
const MISC_POLL: &[&str] = &[
    "AC;", "AG0;", "AN0;", "BC0;", "BI;", "BP00;", "BP01;", "BY;", "CN0;", "CO00;", "CO01;", "CO02;",
    "CS;", "CT0;", "DA;", "FR;", "FS;", "FT;", "GT0;", "IS0;", "KP;", "KR;", "KS;", "LK;", "MC;", "MG;",
    "ML0;", "ML1;", "MS;", "MX;", "NA0;", "NB0;", "NL0;", "NR0;", "OS0;", "PA0;", "PC;", "PL;", "PR0;",
    "PR1;", "RA0;", "RF0;", "RG0;", "RI0;", "RL0;", "RT;", "SC;", "SD;", "SF;", "SH0;", "SQ0;", "TS;",
    "UL;", "VD;", "VF0;", "VG;", "VS;", "VX;", "XT;", "RO;",
];

const VFO_POLL: &[&str] = &["FA;", "FB;", "MD0;", "IF;", "OI;"];

/// Keys that change as a side effect of VFO/band/memory operations.
const VFO_KEYS: &[&str] =
    &["FA", "FB", "IF", "OI", "MD0", "SH0", "NA0", "RF0", "FT", "FR", "VS", "MC", "PA0", "RA0", "AN0", "GT0"];

struct Engine {
    shared: Arc<Shared>,
    rx: Receiver<Msg>,
    self_tx: Sender<Msg>,
    config: EngineConfig,
    want_connected: bool,
    writer: Option<Box<dyn Write + Send>>,
    description: String,
    generation: u64,
    reader_stop: Option<Arc<AtomicBool>>,
    framer: Framer,
    sets: VecDeque<(String, Source)>,
    reads: VecDeque<(String, String, Freshness, Source)>,
    waiters: HashMap<String, Vec<Reply>>,
    outstanding: Option<Outstanding>,
    polls: Vec<PollItem>,
    once: VecDeque<String>,
    last_write: Instant,
    hold_until: Instant,
    fails: u32,
    next_reconnect: Option<Instant>,
    raw: Vec<RawCapture>,
    /// Our own "TX1;"/"MX1;" is active: un-key before closing.
    cat_keyed: bool,
    /// Sets written since the last answer: a `?;` is theirs before it is
    /// the outstanding read's (the radio answers in order).
    unconfirmed: VecDeque<(String, String, Instant)>,
    /// The radio may still be keyed by CAT and the un-key could not be
    /// written: send `TX0;` first thing on the next connection.
    pending_unkey: bool,
    /// When the current link was attached.
    attached_at: Instant,
}

const POST_WRITE_GAP: Duration = Duration::from_millis(5);
/// Hamlib: the FTDX3000 needs time after a mode change before a frequency change.
const MODE_SETTLE: Duration = Duration::from_millis(300);
const RECONNECT_EVERY: Duration = Duration::from_secs(2);

impl Engine {
    fn new(shared: Arc<Shared>, rx: Receiver<Msg>, self_tx: Sender<Msg>) -> Self {
        let now = Instant::now();
        Self {
            shared,
            rx,
            self_tx,
            config: EngineConfig::default(),
            want_connected: false,
            writer: None,
            description: String::new(),
            generation: 0,
            reader_stop: None,
            framer: Framer::new(),
            sets: VecDeque::new(),
            reads: VecDeque::new(),
            waiters: HashMap::new(),
            outstanding: None,
            polls: Vec::new(),
            once: VecDeque::new(),
            last_write: now,
            hold_until: now,
            fails: 0,
            next_reconnect: None,
            raw: Vec::new(),
            cat_keyed: false,
            pending_unkey: false,
            attached_at: now,
            unconfirmed: VecDeque::new(),
        }
    }

    fn run(mut self) {
        loop {
            let wait = self.next_wakeup().saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(wait.max(Duration::from_millis(1))) {
                Ok(msg) => {
                    if !self.handle(msg) {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            while let Ok(msg) = self.rx.try_recv() {
                if !self.handle(msg) {
                    self.close("shutdown");
                    return;
                }
            }
            self.housekeeping();
            self.pump();
        }
        self.close("shutdown");
    }

    fn next_wakeup(&self) -> Instant {
        let now = Instant::now();
        let mut t = now + Duration::from_millis(250);
        if let Some(o) = &self.outstanding {
            t = t.min(o.deadline);
        }
        if self.writer.is_some() && self.outstanding.is_none() {
            if !self.sets.is_empty() || !self.reads.is_empty() || !self.once.is_empty() {
                t = t.min(self.hold_until.max(now));
            }
            let tx = self.shared.state.read().transmitting();
            if let Some(p) = self.polls.iter().filter(|p| poll_eligible(p, tx)).map(|p| p.next).min() {
                t = t.min(p.max(self.hold_until));
            }
        }
        if let Some(r) = self.next_reconnect {
            t = t.min(r);
        }
        for r in &self.raw {
            t = t.min(r.until);
        }
        t
    }

    /// Returns false on shutdown.
    fn handle(&mut self, msg: Msg) -> bool {
        match msg {
            Msg::Job(Job::Shutdown) => return false,
            Msg::Job(Job::Connect(config)) => {
                self.close("reconnecting");
                self.config = config;
                self.want_connected = true;
                self.try_open();
            }
            Msg::Job(Job::ConnectLink(link, config)) => {
                self.close("reconnecting");
                self.config = config;
                self.want_connected = false; // can't reopen an in-process link
                // A link to another radio's own protocol (its simulator)
                // gets that radio's translator.
                let model = self.config.wanted_model().unwrap_or(&FTDX3000);
                let link = match ftdx3000_cat::dialect::for_model(model) {
                    Some(d) => transport::translated(link, d),
                    None => link,
                };
                self.attach(link, model);
            }
            Msg::Job(Job::Disconnect) => {
                self.want_connected = false;
                self.close("disconnected by user");
                self.shared.set_link(LinkState::Disconnected);
            }
            Msg::Job(Job::Set { cmd, src, at }) => {
                let unkey = cmd == "TX0;" || cmd == "MX0;";
                // Never hold commands for later: a PTT click while the cable
                // is out must not key the radio when it comes back. An
                // un-key is the exception: it is owed to the radio.
                if self.writer.is_none() {
                    if unkey {
                        self.pending_unkey = true;
                        self.shared.log(Dir::Info, Some(src), format!("{cmd} will be sent when the radio reconnects"));
                    } else {
                        self.shared.log(Dir::Info, Some(src), format!("dropped {cmd}: radio not connected"));
                    }
                    return true;
                }
                // Queued while the port was being opened (the probe blocks
                // this thread): just as stale.
                if at < self.attached_at && !unkey {
                    self.shared.log(Dir::Info, Some(src), format!("dropped {cmd}: sent before the radio connected"));
                    return true;
                }
                // Coalesce bursts (tuning knob, sliders): if the last queued
                // Set is for the same setting, replace it. Only the tail, so
                // the order relative to other commands never changes.
                let key = command::answer_key(&cmd).filter(|_| {
                    ftdx3000_cat::frame::split(&cmd).and_then(|(c, _)| command::lookup(&c)).is_some_and(|i| i.set_is_answer)
                });
                let queued = key
                    .as_ref()
                    .and_then(|k| self.sets.back_mut().filter(|(c, _)| command::answer_key(c).as_ref() == Some(k)));
                match queued {
                    Some(slot) => *slot = (cmd, src),
                    None => self.sets.push_back((cmd, src)),
                }
            }
            Msg::Job(Job::Read { key, cmd, fresh, src, reply }) => {
                if let Some(r) = reply {
                    if self.writer.is_none() || matches!(*self.shared.link.read(), LinkState::NoResponse { .. }) {
                        let _ = r.send(Err(CatError::NotConnected));
                        return true;
                    }
                    self.waiters.entry(key.clone()).or_default().push(r);
                }
                self.reads.push_back((key, cmd, fresh, src));
            }
            Msg::Job(Job::Raw { cmd, reply }) => {
                let window = Duration::from_millis(400);
                self.raw.push(RawCapture { until: Instant::now() + window, frames: Vec::new(), reply });
                if let Some(w) = self.writer.as_mut() {
                    let cmd = if cmd.trim().ends_with(';') { cmd.trim().to_string() } else { format!("{};", cmd.trim()) };
                    self.shared.log(Dir::Tx, Some(Source::Gui), &cmd);
                    let _ = w.write_all(cmd.as_bytes()).and_then(|_| w.flush());
                    self.last_write = Instant::now();
                }
            }
            Msg::Job(Job::Refresh(cmds)) => {
                for c in cmds {
                    if !self.once.contains(&c) {
                        self.once.push_back(c);
                    }
                }
            }
            Msg::Rx { generation, data } => {
                if generation == self.generation {
                    for frame in self.framer.push(&data) {
                        self.on_frame(frame);
                    }
                }
            }
            Msg::RxError { generation, error } => {
                if generation == self.generation && self.writer.is_some() {
                    self.close(&format!("port error: {error}"));
                    self.shared.set_link(LinkState::Error(error));
                    self.schedule_reconnect();
                }
            }
        }
        true
    }

    fn schedule_reconnect(&mut self) {
        if self.want_connected {
            self.next_reconnect = Some(Instant::now() + RECONNECT_EVERY);
        }
    }

    fn try_open(&mut self) {
        self.next_reconnect = None;
        if let Some(target) = self.config.remote.clone() {
            self.shared.set_link(LinkState::Connecting(format!("connecting to {}", target.addr)));
            match crate::remote::connect_cat(&target) {
                // The remote station translates for its radio already.
                Ok((link, model)) => self.attach(link, model),
                Err(e) => {
                    let msg = format!("remote {}: {e}", target.addr);
                    self.shared.info(format!("connect failed: {msg}"));
                    self.shared.set_link(LinkState::Error(msg));
                    // A wrong password won't fix itself.
                    if e.kind() != io::ErrorKind::PermissionDenied {
                        self.schedule_reconnect();
                    }
                }
            }
            return;
        }
        let s = self.config.serial.clone();
        if s.path.is_empty() {
            self.shared.set_link(LinkState::Error("no CAT port selected".into()));
            return;
        }
        self.shared.set_link(LinkState::Connecting(format!("probing {}", s.path)));
        let shared = self.shared.clone();
        let want = self.config.wanted_model();
        if !self.config.model.is_empty() && want.is_none() {
            self.shared.info(format!("unknown radio model \"{}\" in the settings; finding out from the radio", self.config.model));
        }
        let probe = transport::probe(&s, want, |baud, flow| {
            shared.info(format!("probing {} at {baud} bps ({flow:?} handshake)", s.path));
        });
        match probe {
            Ok((found, link)) => {
                self.shared.info(format!("radio answered {} at {} bps", found.id_answer, found.baud));
                let model = match found.model {
                    Some(m) => {
                        if want.is_some_and(|w| w != m) {
                            self.shared.info(format!("the radio is a {}, not the {} chosen in the settings", m.full_name(), want.unwrap().full_name()));
                        }
                        m
                    }
                    None if found.civ_addr.is_some() => {
                        let msg = format!("an Icom radio RigHelm doesn't know answered ({}); choose the model in Settings", found.id_answer);
                        self.shared.info(msg.clone());
                        self.shared.set_link(LinkState::Error(msg));
                        return;
                    }
                    None => {
                        self.shared.info(format!("warning: {} is not a radio RigHelm knows; treating it as an FTDX3000", found.id_answer));
                        &FTDX3000
                    }
                };
                self.shared.info(format!("radio: {}", model.full_name()));
                let link = match ftdx3000_cat::dialect::for_model(model) {
                    Some(d) => transport::translated(link, d),
                    None => link,
                };
                self.attach(link, model);
            }
            Err(e) => {
                let mut msg = e.to_string();
                let users = crate::detect::port_users(&s.path);
                if !users.is_empty() {
                    let names: Vec<String> = users.iter().map(|u| format!("{} (pid {})", u.name, u.pid)).collect();
                    msg = format!("{msg} — port in use by {}", names.join(", "));
                }
                self.shared.info(format!("connect failed: {msg}"));
                self.shared.set_link(LinkState::Error(msg));
                self.schedule_reconnect();
            }
        }
    }

    fn attach(&mut self, link: Link, model: &'static RadioModel) {
        // A different radio: forget everything about the last one.
        if self.shared.state.read().model != model {
            *self.shared.state.write() = RadioState::for_model(model);
            self.shared.cache.lock().clear();
            self.shared.bump();
        }
        // Nothing from before this connection may be sent on it.
        self.sets.clear();
        self.reads.clear();
        self.once.clear();
        self.unconfirmed.clear();
        self.attached_at = Instant::now();
        self.generation += 1;
        let generation = self.generation;
        let stop = Arc::new(AtomicBool::new(false));
        let tx = self.self_tx.clone();
        let mut reader = link.reader;
        let stop2 = stop.clone();
        thread::Builder::new()
            .name("rig-reader".into())
            .spawn(move || {
                let mut buf = [0u8; 512];
                while !stop2.load(Ordering::Relaxed) {
                    match reader.read(&mut buf) {
                        Ok(0) => {}
                        Ok(n) => {
                            if tx.send(Msg::Rx { generation, data: buf[..n].to_vec() }).is_err() {
                                break;
                            }
                        }
                        Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) => {}
                        Err(e) => {
                            let _ = tx.send(Msg::RxError { generation, error: e.to_string() });
                            break;
                        }
                    }
                }
            })
            .expect("spawn reader");
        self.reader_stop = Some(stop);
        self.writer = Some(link.writer);
        self.description = link.description.clone();
        self.framer.clear();
        self.fails = 0;
        self.shared.info(format!("connected: {}", link.description));
        self.shared.set_link(LinkState::Connected { description: link.description });
        self.setup_polls();
        if std::mem::take(&mut self.pending_unkey) {
            self.sets.push_front(("TX0;".into(), Source::Engine));
            self.shared.info("un-keying: the transmitter may have been left keyed when the link dropped");
        }
    }

    fn setup_polls(&mut self) {
        let now = Instant::now();
        let c = &self.config;
        let model = self.shared.state.read().model;
        let has = |cmd: &str| model.has(&cmd[..2]);
        let mut polls = Vec::new();
        let mut add = |cmd: &str, every: u64, offset: u64| {
            if !has(cmd) {
                return;
            }
            let cmd = cmd.to_string();
            let key = match command::classify(&cmd) {
                Request::Read { key } => key,
                _ => return,
            };
            polls.push(PollItem { cmd, key, next: now + Duration::from_millis(offset), every: Duration::from_millis(every) });
        };
        add("TX;", 250, 0);
        add("SM0;", c.meter_poll_ms, 0);
        for m in ["RM5;", "RM6;", "RM4;", "RM3;", "RM7;", "RM8;"] {
            add(m, c.meter_poll_ms, 0);
        }
        let vfo = if c.auto_info { c.vfo_poll_ms * 3 } else { c.vfo_poll_ms };
        for v in VFO_POLL {
            add(v, vfo, 0);
        }
        let misc: Vec<&&str> = MISC_POLL.iter().filter(|m| has(m)).collect();
        let misc_cycle = c.misc_poll_ms * misc.len() as u64;
        let misc_every = if c.auto_info { misc_cycle.max(15_000) } else { misc_cycle.max(5_000) };
        for (i, m) in misc.iter().enumerate() {
            add(m, misc_every, 200 + i as u64 * c.misc_poll_ms);
        }
        self.polls = polls;

        // Start-up sequence: identify, configure, then read everything once.
        self.once.clear();
        self.once.push_back("ID;".into());
        if has("PS;") {
            self.once.push_back("PS;".into());
        }
        if self.config.set_cat_tot {
            if let Some((item, v)) = model.roles.cat_tot {
                self.sets.push_back((format!("EX{item:03}{v};"), Source::Engine));
            }
        }
        if has("AI;") {
            let ai = if self.config.auto_info { "AI1;" } else { "AI0;" };
            self.sets.push_back((ai.into(), Source::Engine));
        }
        for v in VFO_POLL.iter().chain(MISC_POLL).filter(|v| has(v)) {
            self.once.push_back(v.to_string());
        }
        if has("KM;") {
            // RigHelm's KM has room for five (the FTDX3000's).
            for k in 1..=model.caps.keyer_memories.min(5) {
                self.once.push_back(format!("KM{k};"));
            }
        }
        if self.config.read_menu_on_connect {
            for m in model.menu {
                self.once.push_back(ftdx3000_cat::menu::read_cmd(m.num));
            }
        }
    }

    fn close(&mut self, why: &str) {
        if let Some(w) = self.writer.as_mut() {
            if self.cat_keyed || self.shared.state.read().tx == 1 || self.shared.state.read().mox {
                if w.write_all(b"TX0;MX0;").and_then(|_| w.flush()).is_ok() {
                    self.shared.info("un-keyed transmitter before closing");
                } else {
                    self.pending_unkey = true;
                }
            }
            // Leave the radio as other software expects to find it.
            let _ = w.write_all(b"AI0;").and_then(|_| w.flush());
        }
        if let Some(stop) = self.reader_stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        if self.writer.take().is_some() {
            self.shared.info(format!("closed CAT port: {why}"));
        }
        self.generation += 1;
        self.cat_keyed = false;
        self.unconfirmed.clear();
        self.outstanding = None;
        self.reads.clear();
        self.sets.clear();
        self.once.clear();
        self.polls.clear();
        for (_, ws) in self.waiters.drain() {
            for w in ws {
                let _ = w.send(Err(CatError::NotConnected));
            }
        }
        self.shared.cache.lock().clear();
    }

    fn housekeeping(&mut self) {
        let now = Instant::now();
        if let Some(t) = self.next_reconnect {
            if now >= t && self.writer.is_none() && self.want_connected {
                self.try_open();
            }
        }
        // Finish console captures.
        let mut i = 0;
        while i < self.raw.len() {
            if now >= self.raw[i].until {
                let r = self.raw.remove(i);
                let _ = r.reply.send(r.frames);
            } else {
                i += 1;
            }
        }
        // Outstanding read timeout.
        if let Some(o) = &self.outstanding {
            if now >= o.deadline {
                let o = self.outstanding.take().unwrap();
                if !o.retried && o.src != Source::Poll {
                    self.write_read(o.key, o.cmd, o.src, true);
                } else {
                    self.shared.stats.lock().timeouts += 1;
                    self.shared.log(Dir::Info, Some(o.src.clone()), format!("no answer to {}", o.cmd));
                    self.fail_waiters(&o.key, CatError::Timeout);
                    self.fails += 1;
                    if self.fails >= 3 {
                        self.no_response();
                    }
                }
            }
        }
    }

    fn no_response(&mut self) {
        if !matches!(*self.shared.link.read(), LinkState::NoResponse { .. }) {
            self.shared.info("radio stopped answering (powered off, or CAT RATE changed?)");
            self.shared.set_link(LinkState::NoResponse { description: self.description.clone() });
            self.shared.state.write().power_on = Some(false);
            self.shared.bump();
            // Fail queued client reads quickly; keep probing with ID.
            let keys: Vec<String> = self.waiters.keys().cloned().collect();
            for k in keys {
                self.fail_waiters(&k, CatError::Timeout);
            }
            self.reads.clear();
        }
    }

    fn fail_waiters(&mut self, key: &str, err: CatError) {
        if let Some(ws) = self.waiters.remove(key) {
            for w in ws {
                let _ = w.send(Err(err.clone()));
            }
        }
    }

    fn freshness(&self, key: &str, fresh: Freshness) -> Duration {
        match fresh {
            Freshness::Max(d) => d,
            Freshness::Radio => Duration::ZERO,
            Freshness::Auto => {
                let code = &key[..2.min(key.len())];
                let ai_on = self.shared.state.read().auto_info;
                match code {
                    "SM" | "RM" | "TX" | "BY" => Duration::from_millis(200),
                    "FA" | "FB" | "IF" | "OI" | "MD" => {
                        if ai_on { Duration::from_millis(1500) } else { Duration::from_millis(500) }
                    }
                    "ID" => Duration::MAX,
                    _ => {
                        let ai = command::lookup(code).is_some_and(|c| c.ai);
                        if ai_on && ai { Duration::from_secs(10) } else { Duration::from_millis(1000) }
                    }
                }
            }
        }
    }

    /// Write queued traffic while nothing is outstanding.
    fn pump(&mut self) {
        loop {
            if self.writer.is_none() || self.outstanding.is_some() {
                return;
            }
            let now = Instant::now();
            if now < self.hold_until {
                return;
            }
            let no_response = matches!(*self.shared.link.read(), LinkState::NoResponse { .. });

            if let Some((cmd, src)) = self.sets.pop_front() {
                self.write_set(cmd, src);
                continue;
            }
            if let Some((key, cmd, fresh, src)) = self.reads.pop_front() {
                let age = self.freshness(&key, fresh);
                if let Some(hit) = self.shared.cached(&key, age) {
                    self.shared.stats.lock().cache_hits += 1;
                    if let Some(ws) = self.waiters.remove(&key) {
                        for w in ws {
                            let _ = w.send(Ok(hit.clone()));
                        }
                    }
                    continue;
                }
                if !self.waiters.contains_key(&key) {
                    // A duplicate of a read whose answer already arrived.
                    continue;
                }
                self.write_read(key, cmd, src, false);
                return;
            }
            if no_response {
                // Only knock gently until the radio answers again.
                if let Some(p) = self.polls.iter_mut().find(|p| p.key == "ID") {
                    if now >= p.next {
                        p.next = now + Duration::from_secs(2);
                        self.write_read("ID".into(), "ID;".into(), Source::Poll, true);
                    }
                } else {
                    self.polls.push(PollItem { cmd: "ID;".into(), key: "ID".into(), next: now, every: Duration::from_secs(2) });
                }
                return;
            }
            if let Some(cmd) = self.once.pop_front() {
                if let Request::Read { key } = command::classify(&cmd) {
                    self.write_read(key, cmd, Source::Poll, false);
                    return;
                }
                continue;
            }
            if let Some(idx) = self.next_poll(now) {
                let p = &mut self.polls[idx];
                p.next = now + p.every;
                let (key, cmd) = (p.key.clone(), p.cmd.clone());
                // Skip if something else refreshed it recently.
                if self.shared.cached(&key, self.polls[idx].every / 2).is_some() && !key.starts_with("SM") && !key.starts_with("RM") {
                    continue;
                }
                self.write_read(key, cmd, Source::Poll, false);
            }
            return;
        }
    }

    fn next_poll(&self, now: Instant) -> Option<usize> {
        let tx = self.shared.state.read().transmitting();
        self.polls
            .iter()
            .enumerate()
            .filter(|(_, p)| p.next <= now && poll_eligible(p, tx))
            .min_by_key(|(_, p)| p.next)
            .map(|(i, _)| i)
    }

    fn write_raw(&mut self, cmd: &str) -> bool {
        let gap = self.last_write.elapsed();
        if gap < POST_WRITE_GAP {
            thread::sleep(POST_WRITE_GAP - gap);
        }
        let Some(w) = self.writer.as_mut() else { return false };
        let res = w.write_all(cmd.as_bytes()).and_then(|_| w.flush());
        self.last_write = Instant::now();
        self.shared.stats.lock().tx_frames += 1;
        match res {
            Ok(()) => true,
            Err(e) => {
                let msg = e.to_string();
                self.close(&format!("write failed: {msg}"));
                self.shared.set_link(LinkState::Error(msg));
                self.schedule_reconnect();
                false
            }
        }
    }

    fn write_read(&mut self, key: String, cmd: String, src: Source, retried: bool) {
        self.shared.log(Dir::Tx, Some(src.clone()), &cmd);
        if self.write_raw(&cmd) {
            let timeout = Duration::from_millis(self.config.read_timeout_ms);
            self.outstanding = Some(Outstanding { key, cmd, src, deadline: Instant::now() + timeout, retried });
        }
    }

    fn write_set(&mut self, cmd: String, src: Source) {
        let Some((code, params)) = ftdx3000_cat::frame::split(&cmd) else { return };
        let Some(info) = command::lookup(&code) else {
            self.shared.log(Dir::Info, Some(src), format!("refused unknown command {cmd}"));
            return;
        };
        self.shared.log(Dir::Tx, Some(src.clone()), &cmd);
        if !self.write_raw(&cmd) {
            return;
        }
        match code.as_str() {
            "TX" | "MX" => {
                self.cat_keyed = params == "1";
                *self.shared.tx_by.lock() = (params != "0").then(|| src.clone());
            }
            "MD" => self.hold_until = Instant::now() + MODE_SETTLE,
            "PS" if params == "1" && src != Source::Engine => {
                // Power-on needs a wake-up byte, then the command again 1-2 s later.
                self.hold_until = Instant::now() + Duration::from_millis(1200);
                self.sets.push_front(("PS1;".into(), Source::Engine));
            }
            _ => {}
        }
        let key = format!("{}{}", info.code, params.get(..info.read_params).unwrap_or(""));
        self.unconfirmed.push_back((key.clone(), cmd.clone(), Instant::now()));
        if info.set_is_answer && info.read {
            self.shared.record(&key, &cmd);
        } else if info.read {
            self.shared.invalidate(&key);
            self.bump_poll(&key, Duration::from_millis(50));
        }
        // Side effects on other keys.
        let affected: &[&str] = match code.as_str() {
            "FA" | "RC" | "RU" | "RD" | "RT" | "XT" => &["IF"],
            "FB" => &["OI"],
            "MD" => &["IF", "SH0", "NA0", "RF0"],
            "NA" => &["SH0"],
            "AB" | "BA" | "SV" | "BS" | "BU" | "BD" | "MA" | "AM" | "MC" | "CH" | "QR" | "QS" | "VM" | "UP" | "DN" | "VS" | "FR" | "FT" => VFO_KEYS,
            "TX" | "MX" | "AC" => &["TX"],
            _ => &[],
        };
        for k in affected {
            self.shared.invalidate(k);
            self.bump_poll(k, Duration::from_millis(80));
        }
        // Confirm optimistic frequency/mode updates (the radio ignores
        // frequency changes while transmitting, for example).
        if info.set_is_answer && matches!(code.as_str(), "FA" | "FB" | "MD") {
            self.bump_poll(&key, Duration::from_millis(250));
        }
    }

    /// Make a polled key due soon (or queue a one-shot read).
    fn bump_poll(&mut self, key: &str, delay: Duration) {
        let due = Instant::now() + delay;
        if let Some(p) = self.polls.iter_mut().find(|p| p.key == key) {
            p.next = p.next.min(due);
        } else {
            let cmd = format!("{key};");
            if !self.once.contains(&cmd) {
                self.once.push_front(cmd);
            }
        }
    }

    fn on_frame(&mut self, frame: String) {
        self.shared.stats.lock().rx_frames += 1;
        for r in &mut self.raw {
            r.frames.push(frame.clone());
        }
        let src = self.outstanding.as_ref().map(|o| o.src.clone());
        self.unconfirmed.retain(|(_, _, at)| at.elapsed() < Duration::from_secs(2));
        if frame == "?;" {
            self.shared.stats.lock().rejected += 1;
            if let Some((key, cmd, _)) = self.unconfirmed.pop_front() {
                // A rejected Set: undo its optimistic update by re-reading.
                self.shared.log(Dir::Rx, None, format!("?; (rejected {cmd})"));
                self.shared.invalidate(&key);
                if command::lookup(&key[..2.min(key.len())]).is_some_and(|i| i.read) {
                    self.bump_poll(&key, Duration::from_millis(20));
                }
            } else if let Some(o) = self.outstanding.take() {
                self.shared.log(Dir::Rx, Some(o.src.clone()), format!("?; (rejected {})", o.cmd));
                self.fail_waiters(&o.key, CatError::Rejected);
            } else {
                self.shared.log(Dir::Rx, None, "?; (radio rejected a command)");
            }
            self.fails = 0;
            return;
        }
        let Some(mut key) = command::answer_key(&frame) else {
            self.shared.log(Dir::Rx, None, format!("{frame} (unrecognized)"));
            return;
        };
        // An MR answer starts with the radio's *current* memory channel, not
        // the one asked for ("MR001;" -> "MR050..." when on channel 50). It
        // belongs to the outstanding MR read; file it under that channel.
        let mut state_frame = frame.clone();
        if let Some(o) = self.outstanding.as_ref().filter(|o| o.key.starts_with("MR") && key.starts_with("MR")) {
            if frame.len() > 5 {
                state_frame = format!("{}{}", o.key, &frame[5..]);
                key = o.key.clone();
            }
        }
        let solicited = self.outstanding.as_ref().is_some_and(|o| o.key == key);
        if solicited {
            // Everything written before this read was accepted.
            self.unconfirmed.clear();
        }
        self.shared.log(Dir::Rx, if solicited { src } else { None }, &frame);
        self.fails = 0;
        if matches!(*self.shared.link.read(), LinkState::NoResponse { .. }) {
            self.shared.info("radio is answering again");
            self.shared.set_link(LinkState::Connected { description: self.description.clone() });
            self.polls.retain(|p| p.key != "ID");
            self.setup_polls();
        }
        self.shared.record_as(&key, &frame, &state_frame);
        if solicited {
            self.outstanding = None;
        }
        if let Some(ws) = self.waiters.remove(&key) {
            for w in ws {
                let _ = w.send(Ok(frame.clone()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::SimRadio;

    fn rig_with_sim() -> (Rig, SimRadio) {
        let sim = SimRadio::new();
        let rig = Rig::start();
        let config = EngineConfig { read_menu_on_connect: false, ..Default::default() };
        rig.connect_link(sim.link(), config);
        (rig, sim)
    }

    #[test]
    fn reads_and_sets_through_engine() {
        let (rig, _sim) = rig_with_sim();
        let fa = rig.read("FA;", Freshness::Radio, Source::Gui, Duration::from_secs(2)).unwrap();
        assert_eq!(fa, "FA14074000;");
        rig.send("FA07074000;");
        // Optimistic update is visible right away...
        let fa = rig.read("FA;", Freshness::Auto, Source::Gui, Duration::from_secs(2)).unwrap();
        assert_eq!(fa, "FA07074000;");
        // ...and the radio agrees.
        let fa = rig.read("FA;", Freshness::Radio, Source::Gui, Duration::from_secs(2)).unwrap();
        assert_eq!(fa, "FA07074000;");
        assert_eq!(rig.state().freq_a, 7_074_000);
        assert_eq!(rig.read("ZZ;", Freshness::Radio, Source::Gui, Duration::from_secs(1)), Err(CatError::Invalid));
        rig.shutdown();
    }

    #[test]
    fn menu_read_and_rejection() {
        let (rig, _sim) = rig_with_sim();
        let ex = rig.read("EX038;", Freshness::Radio, Source::Gui, Duration::from_secs(2)).unwrap();
        assert_eq!(ex, "EX0381;");
        assert_eq!(rig.read("EX250;", Freshness::Radio, Source::Gui, Duration::from_secs(2)), Err(CatError::Rejected));
        rig.shutdown();
    }

    #[test]
    fn rejected_set_is_not_blamed_on_next_read() {
        let (rig, _sim) = rig_with_sim();
        let _ = rig.read("EX038;", Freshness::Radio, Source::Gui, Duration::from_secs(2)).unwrap();
        // Out-of-range menu value: the radio answers ?; to the Set...
        rig.send("EX0389;");
        // ...and the read right behind it still gets its own answer.
        let fa = rig.read("FA;", Freshness::Radio, Source::Gui, Duration::from_secs(2));
        assert_eq!(fa.as_deref(), Ok("FA14074000;"));
        // The optimistic EX value is rolled back by a re-read.
        let t0 = Instant::now();
        while rig.cached("EX038").as_deref() != Some("EX0381;") && t0.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(rig.cached("EX038").as_deref(), Some("EX0381;"));
        rig.shutdown();
    }

    #[test]
    fn memory_reads_are_filed_under_the_requested_channel() {
        let (rig, sim) = rig_with_sim();
        sim.core().front_panel("MC050;");
        // The simulator answers MR002 with "MR050..." like the radio does.
        let a = rig.read("MR002;", Freshness::Radio, Source::Gui, Duration::from_secs(2)).unwrap();
        assert!(a.starts_with("MR050"), "{a}");
        let m = rig.state().memories.get(&2).cloned().expect("channel 2 stored");
        assert_eq!(m.freq, 14_074_000);
        assert!(rig.state().memories.get(&50).is_none());
        assert_eq!(rig.read("MR099;", Freshness::Radio, Source::Gui, Duration::from_secs(2)), Err(CatError::Rejected));
        rig.shutdown();
    }

    #[test]
    fn sets_are_dropped_while_disconnected() {
        let rig = Rig::start();
        rig.send("TX1;");
        thread::sleep(Duration::from_millis(50));
        let sim = SimRadio::new();
        rig.connect_link(sim.link(), EngineConfig { read_menu_on_connect: false, ..Default::default() });
        let _ = rig.read("FA;", Freshness::Radio, Source::Gui, Duration::from_secs(2)).unwrap();
        thread::sleep(Duration::from_millis(100));
        assert_eq!(sim.core().tx, 0, "a command from before the connection must not be sent");
        rig.shutdown();
    }

    #[test]
    fn auto_information_updates_state() {
        let (rig, sim) = rig_with_sim();
        let _ = rig.read("FA;", Freshness::Radio, Source::Gui, Duration::from_secs(2)).unwrap();
        let updates = rig.subscribe();
        // Wait for AI1 to reach the simulator.
        let t0 = Instant::now();
        while !sim.auto_info() && t0.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(10));
        }
        sim.front_panel("FA21074000;");
        let t0 = Instant::now();
        loop {
            if let Ok(u) = updates.recv_timeout(Duration::from_millis(100)) {
                if u.frame == "FA21074000;" {
                    break;
                }
            }
            assert!(t0.elapsed() < Duration::from_secs(3), "no AI update");
        }
        assert_eq!(rig.state().freq_a, 21_074_000);
        rig.shutdown();
    }
}
