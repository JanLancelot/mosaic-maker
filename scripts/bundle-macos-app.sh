#!/bin/sh
# Usage: bundle-macos-app.sh <zip> <mosaic-app binary>...
# Wraps the given builds of mosaic-app (one per architecture) into a single
# "Mosaic Maker.app", ad-hoc signed, and zips it to <zip>. The app isn't
# notarized, so Gatekeeper asks before its first launch.
set -eu

zip="$1"
shift
root=$(cd "$(dirname "$0")/.." && pwd)
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/crates/mosaic-app/Cargo.toml")

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
app="$work/Mosaic Maker.app"
mkdir -p "$app/Contents/MacOS"

lipo -create "$@" -output "$app/Contents/MacOS/mosaic-maker"
cat >"$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Mosaic Maker</string>
    <key>CFBundleDisplayName</key>
    <string>Mosaic Maker</string>
    <key>CFBundleIdentifier</key>
    <string>io.github.janlancelot.mosaic-maker</string>
    <key>CFBundleExecutable</key>
    <string>mosaic-maker</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundleVersion</key>
    <string>$version</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

# Without a signature on the bundle as a whole, a downloaded copy is reported
# as damaged rather than as from an unidentified developer.
codesign --force --sign - "$app"

rm -f "$zip"
ditto -c -k --keepParent "$app" "$zip"
