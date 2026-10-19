#!/usr/bin/env bash
# Build a standalone Screendream.app (and optionally a .dmg) for macOS.
#
#   scripts/bundle-macos.sh           # target/release/bundle/osx/Screendream.app
#   scripts/bundle-macos.sh --dmg     # ...plus target/release/Screendream.dmg
#
# Set SIGN_IDENTITY="Developer ID Application: ..." to sign with a real certificate;
# otherwise the bundle is ad-hoc signed, which is enough to run it on this Mac.
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v cargo-bundle >/dev/null; then
    echo "Installing cargo-bundle..."
    cargo install cargo-bundle
fi

cargo bundle --release --format osx

APP="target/release/bundle/osx/Screendream.app"
IDENTITY="${SIGN_IDENTITY:--}"

# A stable signature lets macOS remember the Screen Recording permission across launches.
codesign --force --deep --options runtime --sign "$IDENTITY" "$APP"
codesign --verify --verbose=2 "$APP"
echo "Built $APP"

if [[ "${1:-}" == "--dmg" ]]; then
    DMG="target/release/Screendream.dmg"
    rm -f "$DMG"
    hdiutil create -volname Screendream -srcfolder "$APP" -ov -format UDZO "$DMG"
    echo "Built $DMG"
fi
