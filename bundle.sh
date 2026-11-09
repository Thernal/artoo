#!/bin/sh
# Builds Artoo (release) and wraps it in target/Artoo.app with its icon.
#   ./bundle.sh           builds target/Artoo.app
#   ./bundle.sh --open    builds it and opens it
#   ARTOO_BIN=<file>   wraps that binary instead of building one (CI: a universal binary)
#   ARTOO_LIBS=<dir>   the shared libraries it loads (the plugins' SDK, libstd), when ARTOO_BIN is given
set -eu
cd "$(dirname "$0")"
bin=${ARTOO_BIN:-}
if [ -z "$bin" ]; then
    if ! command -v cargo >/dev/null 2>&1 && [ -f "$HOME/.cargo/env" ]; then
        . "$HOME/.cargo/env"
    fi
    cargo build --release
    bin=target/release/artoo
    libs=target/release/deps
    std=$(rustc --print target-libdir)
else
    libs=${ARTOO_LIBS:?ARTOO_LIBS: the folder of the libraries ARTOO_BIN loads}
    std=$libs
fi
app=target/Artoo.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/Artoo"
# The plugins' SDK and the libstd it links (#175), where the app's rpath looks in a bundle: `../Frameworks`. The
# toolchain's folder, which a build run in place uses, is taken out.
mkdir -p "$app/Contents/Frameworks"
cp "$libs/libartoo_plugin_sdk.dylib" "$app/Contents/Frameworks/"
for lib in $(otool -L "$bin" | sed -n 's|^[[:space:]]*@rpath/\(libstd-[^ ]*\.dylib\).*|\1|p' | sort -u); do
    cp "$std/$lib" "$app/Contents/Frameworks/"
done
otool -l "$bin" | awk '/LC_RPATH/{r=1} r && $1=="path"{print $2; r=0}' | grep '^/' | sort -u | while read -r dir; do
    install_name_tool -delete_rpath "$dir" "$app/Contents/MacOS/Artoo"
done
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
# Ad-hoc signed: Apple silicon runs nothing unsigned. Not notarized — see README → Releases. The libraries first: a
# bundle's signature seals what is in it as it is signed.
codesign --force --sign - "$app"/Contents/Frameworks/*.dylib
codesign --force --sign - "$app"
echo "built $app"
[ "${1:-}" = "--open" ] && open "$app"
exit 0
