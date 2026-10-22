#!/bin/sh
# Build RigHelm.app and a .dmg to drag it into Applications; sign and
# notarize them when the credentials are there.
#
#   packaging/macos/package.sh BIN_DIR VERSION OUT_DIR
#
# BIN_DIR holds righelm, righelmd and righelm-sim (universal binaries).
#
# Signing (otherwise the app is ad-hoc signed and macOS will block it):
#   SIGN_IDENTITY   e.g. "Developer ID Application: Nathan Rigg (G4GK5372G8)"
# Notarization, one of:
#   NOTARY_PROFILE  a login saved with `xcrun notarytool store-credentials`
#   NOTARY_APPLE_ID, NOTARY_PASSWORD (app-specific), NOTARY_TEAM_ID
#   NOTARY_KEY (path to an App Store Connect API .p8), NOTARY_KEY_ID, NOTARY_ISSUER
set -eu

BIN=$1 VERSION=${2#v} OUT=$3
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

app="$WORK/dmg/RigHelm.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
sed "s/__VERSION__/$VERSION/g" "$HERE/Info.plist" > "$app/Contents/Info.plist"
printf 'APPL????' > "$app/Contents/PkgInfo"
cp "$HERE/RigHelm.icns" "$app/Contents/Resources/"
# The server and simulator ride along for command-line use.
for b in righelm righelmd righelm-sim; do
    install -m 755 "$BIN/$b" "$app/Contents/MacOS/$b"
done
cp "$ROOT/README.md" "$ROOT/LICENSE" "$WORK/dmg/"
ln -s /Applications "$WORK/dmg/Applications"

# Inside out: the helpers, then the app (its own executable last).
id=${SIGN_IDENTITY:--}
if [ "$id" = - ]; then
    echo "No SIGN_IDENTITY: ad-hoc signing (macOS will block this build)." >&2
    sign() { codesign --force --sign - "$@"; }
else
    sign() { codesign --force --sign "$id" --options runtime --timestamp --entitlements "$HERE/entitlements.plist" "$@"; }
fi
sign "$app/Contents/MacOS/righelmd"
sign "$app/Contents/MacOS/righelm-sim"
sign "$app"
codesign --verify --deep --strict "$app"

mkdir -p "$OUT"
dmg="$OUT/righelm-v$VERSION-macos.dmg"
rm -f "$dmg"
hdiutil create -quiet -volname "RigHelm $VERSION" -srcfolder "$WORK/dmg" -fs HFS+ -format UDZO "$dmg"
[ "$id" = - ] || codesign --force --sign "$id" --timestamp "$dmg"

if [ -n "${NOTARY_PROFILE:-}" ]; then
    set -- --keychain-profile "$NOTARY_PROFILE"
elif [ -n "${NOTARY_APPLE_ID:-}" ]; then
    set -- --apple-id "$NOTARY_APPLE_ID" --password "$NOTARY_PASSWORD" --team-id "$NOTARY_TEAM_ID"
elif [ -n "${NOTARY_KEY:-}" ]; then
    set -- --key "$NOTARY_KEY" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER"
else
    set --
fi
if [ "$id" != - ] && [ $# -gt 0 ]; then
    # Apple checks the app inside the .dmg; stapling lets it open offline too.
    xcrun notarytool submit "$dmg" "$@" --wait --timeout 30m --output-format json > "$WORK/notary.json" || true
    cat "$WORK/notary.json"
    if ! grep -q '"status" *: *"Accepted"' "$WORK/notary.json"; then
        sub=$(sed -n 's/.*"id" *: *"\([^"]*\)".*/\1/p' "$WORK/notary.json" | head -1)
        [ -n "$sub" ] && xcrun notarytool log "$sub" "$@" || true
        echo "Notarization failed." >&2
        exit 1
    fi
    xcrun stapler staple "$dmg"
    spctl --assess --type open --context context:primary-signature -v "$dmg"
else
    echo "Not notarized." >&2
fi
echo "$dmg"
