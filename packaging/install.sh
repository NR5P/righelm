#!/bin/sh
# Install Ferrite (binaries, icon, desktop entry) for the current user.
set -e
cd "$(dirname "$0")/.."
cargo build --release
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"
install -m 755 target/release/ferrite target/release/ferrited target/release/ferrite-sim "$HOME/.local/bin/"
for n in 32 48 64 128 256 512; do
    d="$HOME/.local/share/icons/hicolor/${n}x${n}/apps"
    mkdir -p "$d"
    install -m 644 "assets/ferrite-$n.png" "$d/ferrite.png"
done
mkdir -p "$HOME/.local/share/icons/hicolor/scalable/apps"
install -m 644 assets/ferrite.svg "$HOME/.local/share/icons/hicolor/scalable/apps/ferrite.svg"
sed "s|^Exec=ferrite|Exec=$HOME/.local/bin/ferrite|" packaging/ferrite.desktop > "$HOME/.local/share/applications/ferrite.desktop"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
echo "Installed Ferrite to ~/.local (binaries in ~/.local/bin)."
