#!/usr/bin/env bash
# Construit XBlades-Linux-x86_64.AppImage (nécessite appimagetool dans le PATH).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --locked -p xblades-gui -p xblades-cli
A=packaging/AppDir; rm -rf "$A"; mkdir -p "$A/usr/bin"
cp target/release/xblades-gui target/release/xblades "$A/usr/bin/"
cat > "$A/AppRun" <<'R'
#!/bin/sh
D="$(dirname "$(readlink -f "$0")")/usr/bin"
if [ "$#" -gt 0 ]; then exec "$D/xblades" "$@"; fi
exec "$D/xblades-gui"
R
chmod +x "$A/AppRun"
cat > "$A/xblades.desktop" <<'D'
[Desktop Entry]
Type=Application
Name=XBlades Linux
Exec=xblades-gui
Icon=xblades
Categories=Settings;HardwareSettings;
D
cp gui/assets/logo.png "$A/xblades.png"
cp gui/assets/logo.png "$A/.DirIcon"
ARCH=x86_64 appimagetool "$A" XBlades-Linux-x86_64.AppImage
echo "OK. Règle udev : sudo cp packaging/70-xblades.rules /etc/udev/rules.d/ && sudo udevadm control --reload && sudo udevadm trigger"
