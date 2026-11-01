#!/usr/bin/env bash
# Gera "EPMA System Data.app" em target/release.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release

APP="target/release/EPMA System Data.app"
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/release/epma-system-data "$APP/Contents/MacOS/"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>EPMA System Data</string>
    <key>CFBundleDisplayName</key><string>EPMA System Data</string>
    <key>CFBundleIdentifier</key><string>com.epma.system-data</string>
    <key>CFBundleExecutable</key><string>epma-system-data</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>${VERSION}</string>
    <key>CFBundleVersion</key><string>${VERSION}</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF

echo "Pronto: $APP"
