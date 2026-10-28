#!/usr/bin/env bash
# Builds dist/WiFiSpanner-x86_64.AppImage from an existing release build.
# Run from the repository root after `cargo build --release`.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

target="${CARGO_TARGET_DIR:-target}/release"
appdir="dist/AppDir"

rm -rf "$appdir"
mkdir -p "$appdir/usr/bin" dist

cp "$target/wifispanner" "$appdir/usr/bin/wifispanner"
cp "$target/spike" "$appdir/usr/bin/wifispanner-spike"
cp packaging/wifispanner.desktop "$appdir/wifispanner.desktop"
mkdir -p "$appdir/usr/share/doc/wifispanner"
cp LICENSE NOTICE THIRD-PARTY-LICENSES.txt "$appdir/usr/share/doc/wifispanner/"
cp packaging/AppRun "$appdir/AppRun"
chmod +x "$appdir/AppRun"

# Downscale the 2000px logo for the icon; ImageMagick keeps the alpha channel.
if command -v magick >/dev/null 2>&1; then im=magick; else im=convert; fi
"$im" assets/logo.png -resize 256x256 "$appdir/wifispanner.png"
cp "$appdir/wifispanner.png" "$appdir/.DirIcon"

tool="dist/appimagetool-x86_64.AppImage"
if [ ! -x "$tool" ]; then
    curl -fsSL -o "$tool" \
        https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod +x "$tool"
fi

# --appimage-extract-and-run avoids needing FUSE on the build machine.
ARCH=x86_64 "$tool" --appimage-extract-and-run "$appdir" dist/WiFiSpanner-x86_64.AppImage
echo "built dist/WiFiSpanner-x86_64.AppImage"
