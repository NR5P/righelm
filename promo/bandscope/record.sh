#!/bin/sh
# Records the band scope video: RigHelm plays bandscope.demo at 1080p in a
# private headless GNOME Shell, with the simulated radio (nothing is sent to
# a real one) and the real RTL-SDR dongle on RX OUT. Then adds the soundtrack.
#
#   ./record.sh                -> righelm-bandscope.mp4
#   VIDEO_ONLY=1 ./record.sh   -> work/raw.mp4 only (no soundtrack)
set -e
cd "$(dirname "$0")"
HERE=$PWD
W=$HERE/work
ROOT=$HERE/../..
mkdir -p "$W/links"
(cd "$ROOT" && cargo build --release)
# Keep the last take until the new one is good.
[ -f righelm-bandscope.mp4 ] && mv righelm-bandscope.mp4 righelm-bandscope.previous.mp4
cat > "$W/config.toml" <<CONF
auto_connect = true
link_dir = "$W/links"
virtual_ports = []
[engine.serial]
path = "$W/links/sim"
[rigctld]
enabled = false
[remote_server]
enabled = false
[scope]
feed = "RxOut"
ppm = ${PPM:--2}
ppm_fine = ${PPM_FINE:--0.47}
calibrated = true
[ui]
show_waterfall = false
show_macros = false
scope_span = 0
scope_fps = 30
scope_height = 300
CONF
"$ROOT/target/release/righelm-sim" "$W/links/sim" > "$W/sim.log" 2>&1 &
SIM=$!
trap 'kill $SIM 2>/dev/null' EXIT
sleep 1
FTDX3000_CONFIG="$W/config.toml" RIGHELM_DEMO="$HERE/bandscope.demo" RIGHELM_DEMO_OUT="$W/raw.mp4" \
  dbus-run-session ./headless.sh "$ROOT/target/release/righelm" > "$W/app.log" 2>&1
[ -n "$VIDEO_ONLY" ] && exit 0
python3 soundtrack.py "$W/raw.events.txt" "$W/raw.mp4" "$W/soundtrack.wav"
ffmpeg -y -loglevel error -i "$W/raw.mp4" -i "$W/soundtrack.wav" -map 0:v -map 1:a \
  -af loudnorm=I=-16:TP=-1.5:LRA=11 -ar 48000 \
  -c:v libx264 -preset slow -crf 17 -pix_fmt yuv420p -c:a aac -b:a 192k -movflags +faststart -shortest \
  righelm-bandscope.mp4
echo "video -> $HERE/righelm-bandscope.mp4"
