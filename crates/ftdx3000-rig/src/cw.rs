//! CW from the computer: a straight key or iambic paddles (touchpad, mouse,
//! keyboard) and typed text, keyed in real time.
//!
//! The radio is keyed on its Standard USB serial port's RTS line (menu 065
//! PC KEYING = RTS) with break-in on. For a remote radio the key goes over
//! the network to the server, which keys its own port; the timing is made
//! here, so only the network's jitter (about a millisecond on a LAN) adds.

use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, unbounded};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::audio::{AudioDevice, Playback};
use crate::engine::{Freshness, Rig, Source};

/// Longest continuous key-down before the keyer lets go by itself.
pub const MAX_KEY_DOWN: Duration = Duration::from_secs(6);
/// Disarm after this long without keying.
pub const IDLE_DISARM: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum KeyerMode {
    /// Key down while held.
    Straight,
    /// Iambic paddles without element memory past the squeeze.
    IambicA,
    /// Iambic paddles: a squeeze released mid-element adds the opposite one.
    #[default]
    IambicB,
}

impl KeyerMode {
    pub fn label(self) -> &'static str {
        match self {
            KeyerMode::Straight => "Straight key",
            KeyerMode::IambicA => "Iambic A",
            KeyerMode::IambicB => "Iambic B",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CwConfig {
    pub mode: KeyerMode,
    /// Play a sidetone on this computer's speakers (useful for a remote radio).
    pub sidetone: bool,
    pub sidetone_hz: u32,
    pub sidetone_volume: f32,
}

impl Default for CwConfig {
    fn default() -> Self {
        Self { mode: KeyerMode::IambicB, sidetone: false, sidetone_hz: 700, sidetone_volume: 0.3 }
    }
}

/// The keying output.
pub trait KeyLine: Send {
    fn set(&mut self, down: bool) -> io::Result<()>;
    /// Called a few times a second, so a remote end knows we're still here.
    fn keepalive(&mut self, _down: bool) -> io::Result<()> {
        Ok(())
    }
}

// ------------------------------------------------------------ local port

/// RTS on the radio's Standard COM port. Restores menu 065 when dropped.
pub struct LocalKeyLine {
    port: Box<dyn serialport::SerialPort>,
    rig: Rig,
    src: Source,
    restore: Option<String>,
}

impl LocalKeyLine {
    /// Opening a serial port raises RTS and DTR, which would key the radio,
    /// so PC KEYING (menu 065) is OFF while the port opens and RTS after.
    pub fn open(rig: &Rig, src: Source) -> Result<LocalKeyLine, String> {
        let model = rig.model();
        let Some((item, off, rts)) = model.roles.pc_keying else {
            return Err(format!("RigHelm can't key the {} from the computer (no PC keying setting on RTS)", model.full_name()));
        };
        let path = crate::detect::detect_radios()
            .into_iter()
            .find_map(|r| r.keying)
            .map(|p| p.stable_path().to_string())
            .ok_or("the radio's keying port (its Standard COM port) was not found")?;
        let name = model.menu_item(item).map_or("PC KEYING", |i| i.name);
        let read_cmd = ftdx3000_cat::menu::read_cmd(item);
        let read = |what: &str| {
            rig.read(&read_cmd, Freshness::Radio, src.clone(), Duration::from_secs(2)).map_err(|e| format!("menu {item:03} {name} ({what}): {e}"))?;
            Ok::<Option<i32>, String>(crate::txaudio::menu_code(rig, item))
        };
        let before = read("read")?;
        rig.send_from(crate::txaudio::menu_cmd(rig, item, off), src.clone());
        if read("off")? != Some(off) {
            return Err(format!("the radio did not accept menu {item:03} {name} = OFF"));
        }
        let mut port = serialport::new(&path, 9600).timeout(Duration::from_millis(100)).open().map_err(|e| format!("keying port {path}: {e}"))?;
        port.write_request_to_send(false).map_err(|e| format!("keying port {path}: {e}"))?;
        let _ = port.write_data_terminal_ready(false);
        let restore = before.filter(|&b| b != rts).map(|b| crate::txaudio::menu_cmd(rig, item, b));
        let mut line = LocalKeyLine { port, rig: rig.clone(), src: src.clone(), restore };
        rig.send_from(crate::txaudio::menu_cmd(rig, item, rts), src.clone());
        if read("RTS")? != Some(rts) {
            line.restore = None;
            return Err(format!("the radio did not accept menu {item:03} {name} = RTS"));
        }
        Ok(line)
    }
}

impl KeyLine for LocalKeyLine {
    fn set(&mut self, down: bool) -> io::Result<()> {
        self.port.write_request_to_send(down).map_err(io::Error::other)
    }
}

impl Drop for LocalKeyLine {
    fn drop(&mut self) {
        let _ = self.port.write_request_to_send(false);
        if let Some(cmd) = self.restore.take() {
            self.rig.send_from(cmd, self.src.clone());
        }
    }
}

// ------------------------------------------------------------ remote

/// Key a remote radio: `D` = down, `U` = up, `.` = still here (idle). While
/// held, `D` is repeated; the server lets go if it stops hearing from us.
pub struct RemoteKeyLine {
    stream: TcpStream,
}

impl RemoteKeyLine {
    pub fn open(target: &crate::remote::RemoteTarget) -> Result<RemoteKeyLine, String> {
        let stream = crate::remote::open_key_session(target).map_err(|e| format!("remote key: {e}"))?;
        Ok(RemoteKeyLine { stream })
    }
}

impl KeyLine for RemoteKeyLine {
    fn set(&mut self, down: bool) -> io::Result<()> {
        self.stream.write_all(if down { b"D" } else { b"U" })
    }

    fn keepalive(&mut self, down: bool) -> io::Result<()> {
        self.stream.write_all(if down { b"D" } else { b"." })
    }
}

impl Drop for RemoteKeyLine {
    fn drop(&mut self) {
        let _ = self.stream.write_all(b"U");
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

/// Server side of a remote key session: key `line` as the client says,
/// letting go if the client goes quiet while down or holds too long.
pub(crate) fn serve_key(line: &mut dyn KeyLine, mut stream: TcpStream, stop: &AtomicBool) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(50)))?;
    let mut buf = [0u8; 64];
    let (mut down, mut down_since, mut last) = (false, Instant::now(), Instant::now());
    let result = loop {
        if stop.load(Ordering::Relaxed) {
            break Ok(());
        }
        match stream.read(&mut buf) {
            Ok(0) => break Ok(()),
            Ok(n) => {
                last = Instant::now();
                for b in &buf[..n] {
                    match b {
                        b'D' if !down => {
                            line.set(true)?;
                            (down, down_since) = (true, Instant::now());
                        }
                        b'U' if down => {
                            line.set(false)?;
                            down = false;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {}
            Err(e) => break Err(e),
        }
        if down && (last.elapsed() > Duration::from_millis(1500) || down_since.elapsed() > MAX_KEY_DOWN) {
            line.set(false)?;
            down = false;
        }
        if last.elapsed() > Duration::from_secs(10) {
            break Ok(());
        }
    };
    let _ = line.set(false);
    result
}

// ------------------------------------------------------------ keyer

/// What the operator does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// Straight key (or both paddles in straight mode) pressed/released.
    Straight(bool),
    /// Paddle state.
    Paddles { dit: bool, dah: bool },
    /// Send this text.
    Text(String),
    /// Stop sending and let go of the key.
    Stop,
}

#[derive(Debug, Clone, Default)]
pub struct CwStatus {
    /// The key line is open and the keyer takes input.
    pub ready: bool,
    pub down: bool,
    /// Typed text is being sent.
    pub sending_text: bool,
    pub error: Option<String>,
    pub note: Option<String>,
}

/// The armed keyer. Dropping it lets go of the key and closes the line.
pub struct CwKeyer {
    tx: Sender<Input>,
    stop: Arc<AtomicBool>,
    wpm: Arc<AtomicU32>,
    status: Arc<Mutex<CwStatus>>,
    last_activity: Arc<Mutex<Instant>>,
    pub armed_at: Instant,
}

impl CwKeyer {
    /// `open` runs on the keyer thread (it may take a moment).
    pub fn start(open: impl FnOnce() -> Result<Box<dyn KeyLine>, String> + Send + 'static, cfg: CwConfig, wpm: u32) -> CwKeyer {
        let (tx, rx) = unbounded();
        let stop = Arc::new(AtomicBool::new(false));
        let wpm = Arc::new(AtomicU32::new(wpm.clamp(4, 60)));
        let status = Arc::new(Mutex::new(CwStatus::default()));
        let last_activity = Arc::new(Mutex::new(Instant::now()));
        let (stop2, wpm2, status2, activity2) = (stop.clone(), wpm.clone(), status.clone(), last_activity.clone());
        thread::Builder::new()
            .name("cw-keyer".into())
            .spawn(move || {
                let line = match open() {
                    Ok(l) => l,
                    Err(e) => {
                        status2.lock().error = Some(e);
                        return;
                    }
                };
                status2.lock().ready = true;
                let tone = cfg.sidetone.then(|| Sidetone::start(cfg.sidetone_hz, cfg.sidetone_volume));
                let mut k = Keyer { line, down: false, down_since: Instant::now(), tone, status: status2.clone() };
                if let Err(e) = k.run(&rx, cfg.mode, &wpm2, &stop2, &activity2) {
                    status2.lock().error = Some(format!("keying stopped: {e}"));
                }
                let _ = k.key(false);
                status2.lock().ready = false;
            })
            .expect("spawn cw keyer");
        CwKeyer { tx, stop, wpm, status, last_activity, armed_at: Instant::now() }
    }

    pub fn input(&self, i: Input) {
        let _ = self.tx.send(i);
    }

    pub fn set_wpm(&self, wpm: u32) {
        self.wpm.store(wpm.clamp(4, 60), Ordering::Relaxed);
    }

    pub fn status(&self) -> CwStatus {
        self.status.lock().clone()
    }

    pub fn idle_for(&self) -> Duration {
        self.last_activity.lock().elapsed()
    }
}

impl Drop for CwKeyer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.tx.send(Input::Stop);
    }
}

struct Keyer {
    line: Box<dyn KeyLine>,
    down: bool,
    down_since: Instant,
    tone: Option<Sidetone>,
    status: Arc<Mutex<CwStatus>>,
}

impl Keyer {
    fn key(&mut self, down: bool) -> io::Result<()> {
        if down != self.down {
            self.line.set(down)?;
            self.down = down;
            if down {
                self.down_since = Instant::now();
            }
            if let Some(t) = &self.tone {
                t.on.store(down, Ordering::Relaxed);
            }
            self.status.lock().down = down;
        }
        Ok(())
    }

    fn run(&mut self, rx: &Receiver<Input>, mode: KeyerMode, wpm: &AtomicU32, stop: &AtomicBool, activity: &Mutex<Instant>) -> io::Result<()> {
        // Timed key states still to play: (down, length in dits).
        let mut plan: VecDeque<(bool, u32)> = VecDeque::new();
        let mut text_plan = false; // `plan` holds typed text
        let mut until: Option<Instant> = None;
        let (mut dit_held, mut dah_held, mut dit_mem, mut dah_mem) = (false, false, false, false);
        let mut last_dah = false;
        let mut last_keepalive = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            let wake = until.unwrap_or_else(|| Instant::now() + Duration::from_millis(100));
            match rx.recv_deadline(wake) {
                Ok(input) => {
                    *activity.lock() = Instant::now();
                    match input {
                        Input::Straight(d) if mode == KeyerMode::Straight => {
                            plan.clear();
                            until = None;
                            self.key(d)?;
                        }
                        Input::Straight(_) => {}
                        Input::Paddles { dit, dah } if mode == KeyerMode::Straight => {
                            plan.clear();
                            until = None;
                            self.key(dit || dah)?;
                        }
                        Input::Paddles { dit, dah } => {
                            if text_plan && (dit || dah) {
                                // Touching the paddles interrupts typed text.
                                plan.clear();
                                text_plan = false;
                                until = None;
                                self.key(false)?;
                            }
                            dit_mem |= dit && !dit_held;
                            dah_mem |= dah && !dah_held;
                            (dit_held, dah_held) = (dit, dah);
                        }
                        Input::Text(t) => {
                            if plan.is_empty() || text_plan {
                                plan.extend(morse_plan(&t));
                                text_plan = true;
                            }
                        }
                        Input::Stop => {
                            plan.clear();
                            until = None;
                            (dit_mem, dah_mem, dit_held, dah_held) = (false, false, false, false);
                            self.key(false)?;
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            let now = Instant::now();
            if until.is_some_and(|t| now < t) {
                continue;
            }
            until = None;
            if plan.is_empty() {
                text_plan = false;
                if mode != KeyerMode::Straight && !self.down {
                    // Mode A forgets a paddle let go during the last element.
                    if mode == KeyerMode::IambicA {
                        (dit_mem, dah_mem) = (false, false);
                    }
                    let (dit, dah) = (dit_mem || dit_held, dah_mem || dah_held);
                    let next_dah = match (dit, dah) {
                        (true, true) => Some(!last_dah),
                        (true, false) => Some(false),
                        (false, true) => Some(true),
                        _ => None,
                    };
                    if let Some(d) = next_dah {
                        if d { dah_mem = false } else { dit_mem = false }
                        last_dah = d;
                        plan.push_back((true, if d { 3 } else { 1 }));
                        plan.push_back((false, 1));
                        *activity.lock() = now;
                    }
                }
            }
            if let Some((down, dits)) = plan.pop_front() {
                self.key(down)?;
                let dit = Duration::from_secs_f64(1.2 / wpm.load(Ordering::Relaxed) as f64);
                until = Some(now + dit * dits);
            }
            if self.down && self.down_since.elapsed() > MAX_KEY_DOWN {
                plan.clear();
                until = None;
                self.key(false)?;
                self.status.lock().note = Some("key held too long: released".into());
            }
            if last_keepalive.elapsed() > Duration::from_millis(400) {
                self.line.keepalive(self.down)?;
                last_keepalive = now;
            }
            self.status.lock().sending_text = text_plan;
        }
        Ok(())
    }
}

/// Morse for one character, `.` and `-`.
fn morse(c: char) -> Option<&'static str> {
    Some(match c.to_ascii_uppercase() {
        'A' => ".-",
        'B' => "-...",
        'C' => "-.-.",
        'D' => "-..",
        'E' => ".",
        'F' => "..-.",
        'G' => "--.",
        'H' => "....",
        'I' => "..",
        'J' => ".---",
        'K' => "-.-",
        'L' => ".-..",
        'M' => "--",
        'N' => "-.",
        'O' => "---",
        'P' => ".--.",
        'Q' => "--.-",
        'R' => ".-.",
        'S' => "...",
        'T' => "-",
        'U' => "..-",
        'V' => "...-",
        'W' => ".--",
        'X' => "-..-",
        'Y' => "-.--",
        'Z' => "--..",
        '0' => "-----",
        '1' => ".----",
        '2' => "..---",
        '3' => "...--",
        '4' => "....-",
        '5' => ".....",
        '6' => "-....",
        '7' => "--...",
        '8' => "---..",
        '9' => "----.",
        '.' => ".-.-.-",
        ',' => "--..--",
        '?' => "..--..",
        '/' => "-..-.",
        '=' => "-...-",
        '+' => ".-.-.",
        '-' => "-....-",
        '(' => "-.--.",
        ')' => "-.--.-",
        '\'' => ".----.",
        '"' => ".-..-.",
        ':' => "---...",
        '@' => ".--.-.",
        '!' => "-.-.--",
        _ => return None,
    })
}

/// Key states for `text`: (down, dits). Element gap 1, letter gap 3, word gap 7.
pub fn morse_plan(text: &str) -> Vec<(bool, u32)> {
    let mut out: Vec<(bool, u32)> = Vec::new();
    let mut gap = 0;
    for c in text.chars() {
        if c.is_whitespace() {
            gap = 7;
            continue;
        }
        let Some(code) = morse(c) else { continue };
        if !out.is_empty() {
            out.push((false, gap.max(3)));
        }
        for (i, e) in code.chars().enumerate() {
            if i > 0 {
                out.push((false, 1));
            }
            out.push((true, if e == '-' { 3 } else { 1 }));
        }
        gap = 3;
    }
    if !out.is_empty() {
        out.push((false, 3));
    }
    out
}

// ------------------------------------------------------------ sidetone

/// A tone on this computer's speakers while the key is down.
struct Sidetone {
    on: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

impl Sidetone {
    fn start(hz: u32, volume: f32) -> Sidetone {
        let on = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (on2, stop2) = (on.clone(), stop.clone());
        let _ = thread::Builder::new().name("cw-sidetone".into()).spawn(move || {
            let rate = 24_000u32;
            let Ok(play) = Playback::start(&AudioDevice::Default, rate, Duration::from_millis(60)) else { return };
            let chunk = (rate / 200) as usize; // 5 ms
            let step = std::f32::consts::TAU * hz as f32 / rate as f32;
            let (mut phase, mut amp) = (0f32, 0f32);
            let ramp = 1.0 / (rate as f32 * 0.004); // 4 ms rise/fall: no clicks
            let start = Instant::now();
            let mut sent = 0u64;
            while !stop2.load(Ordering::Relaxed) {
                let target = if on2.load(Ordering::Relaxed) { 1.0 } else { 0.0 };
                let frame: Vec<i16> = (0..chunk)
                    .map(|_| {
                        amp = if amp < target { (amp + ramp).min(target) } else { (amp - ramp).max(target) };
                        phase = (phase + step) % std::f32::consts::TAU;
                        (phase.sin() * amp * volume.clamp(0.0, 1.0) * 32767.0) as i16
                    })
                    .collect();
                play.push(frame);
                sent += chunk as u64;
                // Stay about one chunk ahead of real time.
                let due = Duration::from_secs_f64(sent as f64 / rate as f64);
                if let Some(wait) = due.checked_sub(start.elapsed() + Duration::from_millis(5)) {
                    thread::sleep(wait);
                }
            }
        });
        Sidetone { on, stop }
    }
}

impl Drop for Sidetone {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Read one line (the server's READY/ERR answer) with a deadline.
pub(crate) fn read_reply(stream: &TcpStream, timeout: Duration) -> io::Result<String> {
    stream.set_read_timeout(Some(timeout))?;
    let mut line = String::new();
    BufReader::new(stream.try_clone()?).take(256).read_line(&mut line)?;
    Ok(line.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    type Log = Arc<Mutex<Vec<(bool, Instant)>>>;

    /// Records key transitions with timestamps.
    struct Recorder(Log);

    impl KeyLine for Recorder {
        fn set(&mut self, down: bool) -> io::Result<()> {
            self.0.lock().push((down, Instant::now()));
            Ok(())
        }
    }

    fn keyer(mode: KeyerMode, wpm: u32) -> (CwKeyer, Log) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let l2 = log.clone();
        let k = CwKeyer::start(move || Ok(Box::new(Recorder(l2)) as Box<dyn KeyLine>), CwConfig { mode, ..Default::default() }, wpm);
        thread::sleep(Duration::from_millis(50));
        (k, log)
    }

    /// Key-down lengths in dits.
    fn downs(log: &[(bool, Instant)], dit: f64) -> Vec<u32> {
        log.windows(2).filter(|w| w[0].0 && !w[1].0).map(|w| ((w[1].1 - w[0].1).as_secs_f64() / dit).round() as u32).collect()
    }

    #[test]
    fn text_is_sent_with_correct_timing() {
        assert_eq!(morse_plan("E"), vec![(true, 1), (false, 3)]);
        assert_eq!(morse_plan("A B"), vec![(true, 1), (false, 1), (true, 3), (false, 7), (true, 3), (false, 1), (true, 1), (false, 1), (true, 1), (false, 1), (true, 1), (false, 3)]);
        let (k, log) = keyer(KeyerMode::IambicB, 40); // 30 ms dits
        k.input(Input::Text("PARIS".into()));
        thread::sleep(Duration::from_millis(1600));
        let log = log.lock().clone();
        assert_eq!(downs(&log, 0.030), vec![1, 3, 3, 1, 1, 3, 1, 3, 1, 1, 1, 1, 1, 1]);
        assert!(!log.last().unwrap().0, "key left up");
    }

    #[test]
    fn squeeze_alternates_and_release_stops() {
        let (k, log) = keyer(KeyerMode::IambicB, 40);
        k.input(Input::Paddles { dit: true, dah: true });
        thread::sleep(Duration::from_millis(500));
        k.input(Input::Paddles { dit: false, dah: false });
        thread::sleep(Duration::from_millis(300));
        let log = log.lock().clone();
        let d = downs(&log, 0.030);
        assert!(d.len() >= 3, "{d:?}");
        assert!(d.windows(2).all(|w| w[0] != w[1]), "alternates: {d:?}");
        assert!(!log.last().unwrap().0);
    }

    #[test]
    fn straight_key_follows_and_is_limited() {
        let (k, log) = keyer(KeyerMode::Straight, 20);
        k.input(Input::Straight(true));
        thread::sleep(Duration::from_millis(80));
        k.input(Input::Straight(false));
        thread::sleep(Duration::from_millis(50));
        assert_eq!(log.lock().iter().map(|e| e.0).collect::<Vec<_>>(), vec![true, false]);
        drop(k);
        thread::sleep(Duration::from_millis(50));
    }

    #[test]
    fn dropping_the_keyer_lets_go() {
        let (k, log) = keyer(KeyerMode::Straight, 20);
        k.input(Input::Straight(true));
        thread::sleep(Duration::from_millis(50));
        drop(k);
        thread::sleep(Duration::from_millis(150));
        assert_eq!(log.lock().last().map(|e| e.0), Some(false));
    }
}
