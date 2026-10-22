# RigHelm

![RigHelm](assets/righelm-128.png)

**RigHelm** — the Linux alternative to Win4Yaesu for the Yaesu FTDX3000. Rig control,
sharing and LAN remote, written in Rust.

It owns the radio's CAT
port and shares it with any number of other programs, on this computer or across your LAN.

- **Full radio control.** VFO A/B, split, bands, modes, filters and DSP, receiver and
  transmitter settings, clarifier, CW keyer and keyer memories, voice memories, tuner,
  antennas, rotator, live meters, all 196 menu items (with backup and restore), memory
  channels, macros, and a raw CAT console.
- **Audio waterfall.** A scrolling spectrogram of the radio's receive audio (up to 6 kHz),
  with the filter passband and CW pitch overlaid. Hover shows a signal's radio frequency;
  in CW, click a trace to zero-beat it. It shares the radio's sound card with WSJT-X, and
  works from a remote computer too. The FTDX3000 doesn't send its own scope over CAT, so a
  wide panadapter would need an SDR on an IF tap.
- **Auto-detect.** Finds the radio's CP2105 USB bridge (Enhanced = CAT, Standard = keying)
  and the radio's own USB audio codec. The two are paired because they sit on the same
  internal USB hub of the radio. The baud rate and handshake are probed on connect.
- **Port sharing.** Linux has the same one-program-per-port problem as Windows, so this
  program opens the radio exclusively and offers **virtual serial ports** and a
  **rigctld-compatible server** to everything else.
- **Remote over the LAN.** Run a second copy on another computer: it finds the radio
  automatically, controls everything, plays the radio's audio, and can send your
  microphone back.

## Build and install

```bash
cargo build --release
```

To install the binaries, icon and a desktop launcher for your user:

```bash
./packaging/install.sh
```

Binaries in `target/release/`:

| binary          | what                                                         |
|-----------------|--------------------------------------------------------------|
| `righelm`       | the desktop app                                              |
| `righelmd`      | headless server (same config, no window), `--detect` lists hardware |
| `righelm-sim`   | a simulated FTDX3000 on a PTY for testing without the radio  |

Your user must be in the `dialout` group to open the radio's serial port.

## Radio settings

| menu | setting           | value |
|------|-------------------|-------|
| 037  | CAT SELECT        | USB |
| 038  | CAT RATE          | 38400 (any rate works; it's auto-detected) |
| 039  | CAT TIME OUT TIMER| set to 100 ms automatically on connect |

For transmit audio from a computer (WSJT-X or remote mic), set **075 DATA IN SELECT** and/or
**103 SSB MIC SEL** to **USB**.

## Sharing the radio with other programs

Only one program at a time can sensibly talk to a serial port. Linux will even let two open
it, and then they steal each other's answers. Most programs also lock it exclusively
(`TIOCEXCL`). This app keeps the real port and shares it in two ways.

### 1. Virtual serial ports

Each one behaves exactly like an FTDX3000 on a serial cable (default `vcom1..3` in
`$XDG_RUNTIME_DIR/ftdx3000/`, e.g. `/run/user/1000/ftdx3000/vcom1`).

- **WSJT-X / JTDX**: Rig *Yaesu FTDX-3000*, Serial port `/run/user/1000/ftdx3000/vcom1`
  (type it in), Handshake *None*, PTT *CAT*.
- **fldigi**: Hamlib, FTDX-3000, device as above.
- **flrig**: FTdx3000, port as above.
- **Wine programs** (N1MM, Log4OM, CW Skimmer...): *Ports & sharing → Wine COM port
  mapping* links `COMn` in a Wine prefix to a virtual port.

How the sharing works:

- **Reads are cached.** Five programs polling `FA;` cost the radio one read.
- **Sets go out in order**, one at a time, never interleaved.
- **Auto-information is emulated per program.** One program turning `AI1;` on doesn't flood
  the others.
- **Block TX** per port, for a skimmer that should never key the radio. This blocks every way
  to transmit: PTT/MOX, tuner start, keyer and voice playback, and VOX.
- Virtual ports carry CAT only, so use CAT PTT in other programs (RTS/DTR on a PTY does
  nothing).

### 2. rigctld-compatible server

On `127.0.0.1:4532`. Anything with Hamlib can use rig **Hamlib NET rigctl** with no serial
port at all. Tested against Hamlib 4.6's own `rigctl -m 2`.

## Remote operation over the LAN

On the computer with the radio, open **Remote (LAN) → Let other computers use this
radio**, set a password, and enable it. On any other computer, start the app and open
**Remote (LAN)**. The radio appears under *Radios found on your network*. Click
*Connect*.

- Everything in the remote app controls the remote radio, and the remote app's own virtual
  ports and rigctld server work too. You can run WSJT-X on the remote computer against
  its local `vcom1`.
- **Receive audio** streams to the remote computer's speakers (24 kHz, 16-bit PCM over UDP,
  about 400 kbit/s, with latency held to roughly 150 ms).
- **Send my microphone while transmitting** streams the mic back to the radio. It only
  flows while the radio is actually transmitting, and only if the server allows it.
- The server uses TCP+UDP 4533 for control and audio and UDP 4534 for discovery. If you run
  a firewall:

  ```bash
  sudo ufw allow 4533
  ```

  ```bash
  sudo ufw allow 4534/udp
  ```

- **Transmit needs a password.** A server without one is receive-only: remote users can
  listen and tune, but every transmit command (PTT, MOX, TUNE, keyer/voice playback, VOX)
  is refused.
- The password is checked with an HMAC-SHA256 challenge-response and never sent in clear.
  Five wrong guesses lock that address out for a minute.
- Traffic is not encrypted, and a fake server on your LAN could collect a response to try
  passwords against offline. Use it on your home network, not across the internet.
- **Un-keying.** If a remote client, rigctld client, or program on a virtual port keyed the
  radio and then disappears (crash, closed laptop, Wi-Fi drop), the radio is un-keyed. A
  silently dropped network connection is detected within 5 seconds.
- **No stale commands.** Commands sent while the radio is disconnected are discarded, not
  replayed later.

## Testing without the radio

```bash
cargo run --release --bin righelm-sim
```

Then choose `/run/user/1000/ftdx3000/sim` as the CAT port. The simulator answers every
command in the CAT reference, supports auto-information, and rejects bad commands with
`?;`.

```bash
cargo test
```

Runs the protocol, engine, virtual port (with real PTYs), rigctld, and remote control/audio
tests.

## Files

- Config: `~/.config/ftdx3000/config.toml` (override with `FTDX3000_CONFIG`).
- Menu backups default to `~/ftdx3000-menu-backup.txt`. Restore skips the CAT settings
  (037-040) unless you tick the box.

## Notes and limits

- Meter calibrations (SWR, ID, VDD) are approximate until checked against the radio.
- The FTDX3000 does not send its band scope over CAT, so there is no panadapter.
- DATA-mode filter widths follow menu 067: PSK widths when DATA MODE = PSK, SSB widths
  when OTHER.
