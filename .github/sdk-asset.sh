#!/bin/sh
# sdk-asset.sh <deps> <host-deps> <target> <build> <version> <out> (#175): what a plugin is built against, for one
# target, as artoo-sdk-<version>-<target>.tar.zst in <out>. Inside: `deps/` for rustc (`--extern artoo_plugin_sdk=…`,
# `-L dependency=…`) and `sdk.txt`, the version, the target, the compiler a plugin must be built with and the identity
# of this SDK build (`ARTOO_SDK_BUILD`).
#
# <deps> is the build's `deps` folder and <host-deps> the one its proc macros were built into, the same one unless the
# build was given `--target`. Proc macros run on the machine that builds the plugin: macOS' two assets hold the
# runner's, so a plugin is built on Apple silicon for either architecture.
set -e
if [ $# -ne 6 ]; then
  echo "usage: sdk-asset.sh <deps> <host-deps> <target> <build> <version> <out>" >&2
  exit 2
fi
deps=$1 hosts=$2 target=$3 build=$4 version=$5 out=$6
if [ -z "$build" ]; then
  echo "::error::no SDK build for $target" >&2
  exit 1
fi
name=artoo-sdk-$version-$target
root=$(mktemp -d)
mkdir -p "$root/$name/deps" "$out"
cp "$deps"/*.rmeta "$root/$name/deps/"
# A crate built as a static library too has no `.rmeta`, only its `.rlib`.
for rlib in "$deps"/*.rlib; do
  if [ ! -f "${rlib%.rlib}.rmeta" ]; then
    cp "$rlib" "$root/$name/deps/"
  fi
done
# The SDK, and on Windows what a plugin links against it with.
for file in libartoo_plugin_sdk.dylib libartoo_plugin_sdk.so artoo_plugin_sdk.dll artoo_plugin_sdk.dll.lib; do
  if [ -f "$deps/$file" ]; then
    cp "$deps/$file" "$root/$name/deps/"
  fi
done
# The proc macros, the only other shared libraries there.
for file in "$hosts"/*.dylib "$hosts"/*.so "$hosts"/*.dll; do
  case "$file" in
    *artoo_plugin_sdk*) continue ;;
  esac
  if [ -f "$file" ]; then
    cp "$file" "$root/$name/deps/"
  fi
done
{
  echo "version=$version"
  echo "target=$target"
  echo "rustc=$(rustc --version)"
  echo "build=$build"
} > "$root/$name/sdk.txt"
tar -C "$root" --zstd -cf "$out/$name.tar.zst" "$name"
echo "$name.tar.zst: $(du -h "$out/$name.tar.zst" | cut -f1), SDK build $build"
