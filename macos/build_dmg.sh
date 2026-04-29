#!/usr/bin/env bash
# Build genscribe.app + genscribe-<version>-arm64.dmg
# Apple Silicon only (aarch64-apple-darwin). Run on a macOS arm64 host.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"
VERSION="$(awk -F'"' '/^version *=/ {print $2; exit}' Cargo.toml)"
TARGET="aarch64-apple-darwin"

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "build_dmg.sh: must run on macOS" >&2; exit 1
fi
if [[ "$(uname -m)" != "arm64" ]]; then
    echo "build_dmg.sh: must run on Apple Silicon (arm64)" >&2; exit 1
fi

echo "==> Building release for $TARGET"
rustup target add "$TARGET" >/dev/null 2>&1 || true
cargo build --release --target "$TARGET"

DIST="$ROOT/dist"
APP="$DIST/genscribe.app"
rm -rf "$DIST"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/bin"

echo "==> Assembling app bundle"
cp "target/$TARGET/release/genscribe" "$APP/Contents/MacOS/genscribe"
sed "s/__VERSION__/$VERSION/g" macos/Info.plist > "$APP/Contents/Info.plist"
if [[ -f macos/AppIcon.icns ]]; then
    cp macos/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
fi

echo "==> Bundling helper binaries"
# Helpers must be arm64-only. Place them at macos/Resources/bin/ before running this script,
# or set GENSCRIBE_FFMPEG / GENSCRIBE_YTDLP to override paths to the binaries to bundle.
FFMPEG_SRC="${GENSCRIBE_FFMPEG:-macos/Resources/bin/ffmpeg}"
YTDLP_SRC="${GENSCRIBE_YTDLP:-macos/Resources/bin/yt-dlp}"

if [[ ! -x "$FFMPEG_SRC" ]]; then
    echo "warning: $FFMPEG_SRC missing or not executable — app will fall back to PATH" >&2
else
    cp "$FFMPEG_SRC" "$APP/Contents/Resources/bin/ffmpeg"
    chmod +x "$APP/Contents/Resources/bin/ffmpeg"
fi
if [[ ! -x "$YTDLP_SRC" ]]; then
    echo "warning: $YTDLP_SRC missing or not executable — app will fall back to PATH" >&2
else
    cp "$YTDLP_SRC" "$APP/Contents/Resources/bin/yt-dlp"
    chmod +x "$APP/Contents/Resources/bin/yt-dlp"
fi

echo "==> Ad-hoc codesigning"
codesign --force --deep --sign - "$APP"

echo "==> Creating DMG"
DMG="$DIST/genscribe-$VERSION-arm64.dmg"
STAGE="$DIST/dmg_stage"
rm -rf "$STAGE"; mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/genscribe.app"
ln -s /Applications "$STAGE/Applications"

hdiutil create -volname "genscribe" -srcfolder "$STAGE" -ov -format ULFO "$DMG" >/dev/null

echo "==> Done: $DMG"
ls -lh "$DMG"
