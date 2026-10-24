#!/bin/sh
# Build RigHelm.app and install it in /Applications (or the folder given).
set -e
cd "$(dirname "$0")/.."
dest="${1:-/Applications}"
cargo build --release
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
app="$work/RigHelm.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
install -m 755 target/release/righelm target/release/righelmd target/release/righelm-sim "$app/Contents/MacOS/"

# Icon: every size iconutil wants, from the PNGs (1024 from the 512).
set_dir="$work/righelm.iconset"
mkdir -p "$set_dir"
for n in 16 32 128 256 512; do
    sips -z $n $n assets/righelm-512.png --out "$set_dir/icon_${n}x${n}.png" >/dev/null
    d=$((n * 2))
    sips -z $d $d assets/righelm-512.png --out "$set_dir/icon_${n}x${n}@2x.png" >/dev/null
done
iconutil -c icns "$set_dir" -o "$app/Contents/Resources/righelm.icns"

# The usage strings matter: without them macOS refuses the microphone
# (receive audio, remote mic) and the LAN (remote discovery) to an app.
cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>RigHelm</string>
    <key>CFBundleDisplayName</key><string>RigHelm</string>
    <key>CFBundleIdentifier</key><string>net.nr5p.righelm</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleExecutable</key><string>righelm</string>
    <key>CFBundleIconFile</key><string>righelm</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSMicrophoneUsageDescription</key><string>RigHelm records the radio's receive audio for the waterfall and LAN remote, and your microphone for transmitting.</string>
    <key>NSLocalNetworkUsageDescription</key><string>RigHelm finds and connects to other copies of RigHelm on your network for remote control.</string>
</dict>
</plist>
EOF

# Ad-hoc signature, so macOS remembers the permissions you grant it.
codesign --force --deep --sign - "$app"

rm -rf "$dest/RigHelm.app"
cp -R "$app" "$dest/"
echo "Installed $dest/RigHelm.app ($version)."
