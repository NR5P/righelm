#!/bin/sh
# Install RigHelm (binaries, icon, desktop entry) for the current user.
set -e
cd "$(dirname "$0")/.."
cargo build --release
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"
install -m 755 target/release/righelm target/release/righelmd target/release/righelm-sim "$HOME/.local/bin/"
for n in 32 48 64 128 256 512; do
    d="$HOME/.local/share/icons/hicolor/${n}x${n}/apps"
    mkdir -p "$d"
    install -m 644 "assets/righelm-$n.png" "$d/righelm.png"
done
mkdir -p "$HOME/.local/share/icons/hicolor/scalable/apps"
install -m 644 assets/righelm.svg "$HOME/.local/share/icons/hicolor/scalable/apps/righelm.svg"
# Absolute paths: the launcher works even if ~/.local/bin isn't on the
# desktop's PATH or the icon theme cache is stale.
sed -e "s|^Exec=righelm|Exec=$HOME/.local/bin/righelm|" \
    -e "s|^Icon=righelm|Icon=$HOME/.local/share/icons/hicolor/256x256/apps/righelm.png|" \
    packaging/righelm.desktop > "$HOME/.local/share/applications/righelm.desktop"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
# -t: the per-user hicolor directory usually has no index.theme.
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
echo "Installed RigHelm to ~/.local (binaries in ~/.local/bin)."
