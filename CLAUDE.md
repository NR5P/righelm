# Notes for Claude

## Pending checks on macOS and Linux

These were added and tested on Windows only. When a session starts on macOS
or Linux, remind the user of them before anything else, and offer to run
them. Remove each item from this file (and commit) once it has been checked
on both systems; remove this section when it's empty.

- **Band scope: "which jack" question.** The first time an RTL-SDR dongle is
  seen, the band scope asks whether it's on IF OUT or RX OUT
  (`feed_question` in `crates/righelm/src/scope.rs`, `ScopeConfig::feed_confirmed`).
  Existing configs don't have the flag, so it is asked once after updating.
  Check: the question shows with the dongle plugged in, both buttons set the
  feed and save it, it doesn't come back after a restart, and changing the
  feed in Settings → Band scope also stops it being asked.
- **Band scope across band changes on RX OUT.** Windows got a fix for the
  dongle stalling after a retune (`Dongle::restart` in
  `crates/ftdx3000-rig/src/scope.rs`); macOS and Linux keep the old stop/start.
  Run `cargo run --release --example functions_check` with RigHelm closed and
  the dongle on RX OUT: every "band scope after moving to ..." line must pass.
