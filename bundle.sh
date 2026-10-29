#!/bin/sh
# Builds Artoo (release) and wraps it in target/Artoo.app with its icon.
#   ./bundle.sh           builds target/Artoo.app
#   ./bundle.sh --open    builds it and opens it
#   ARTOO_BIN=<file>   wraps that binary instead of building one (CI: a universal binary)
set -eu
cd "$(dirname "$0")"
bin=${ARTOO_BIN:-}
if [ -z "$bin" ]; then
    if ! command -v cargo >/dev/null 2>&1 && [ -f "$HOME/.cargo/env" ]; then
        . "$HOME/.cargo/env"
    fi
    cargo build --release
    bin=target/release/artoo
fi
app=target/Artoo.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/Artoo"
cp assets/Artoo.icns "$app/Contents/Resources/Artoo.icns"
# What the bundled third-party assets ask to travel with them: IntelliJ's icons (Apache 2.0,
# its licence and notice) and GitLab's emoji list (MIT).
mkdir -p "$app/Contents/Resources/Licenses"
cp assets/icons/intellij/LICENSE "$app/Contents/Resources/Licenses/intellij-icons-LICENSE"
cp assets/icons/intellij/NOTICE "$app/Contents/Resources/Licenses/intellij-icons-NOTICE"
cp assets/emoji/LICENSE "$app/Contents/Resources/Licenses/gitlab-emoji-LICENSE"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Artoo</string>
<key>CFBundleDisplayName</key><string>Artoo</string>
<key>CFBundleIdentifier</key><string>io.github.thernal.artoo</string>
<key>CFBundleExecutable</key><string>Artoo</string>
<key>CFBundleIconFile</key><string>Artoo</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>LSMinimumSystemVersion</key><string>12.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
# Ad-hoc signed: Apple silicon runs nothing unsigned. Not notarized — see README → Releases.
codesign --force --sign - "$app"
echo "built $app"
[ "${1:-}" = "--open" ] && open "$app"
exit 0
