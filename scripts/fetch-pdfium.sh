#!/bin/sh
# Downloads the pdfium build matching pdfium-render's `pdfium_latest` feature into src-tauri/lib.
set -e
version=7881
dir=$(dirname "$0")/../src-tauri/lib
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target=linux-x64 lib=libpdfium.so ;;
  Linux-aarch64) target=linux-arm64 lib=libpdfium.so ;;
  Darwin-x86_64) target=mac-x64 lib=libpdfium.dylib ;;
  Darwin-arm64) target=mac-arm64 lib=libpdfium.dylib ;;
  *) echo "no pdfium build for $(uname -sm)" >&2; exit 1 ;;
esac
tmp=$(mktemp -d)
curl -fsSL -o "$tmp/pdfium.tgz" "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/$version/pdfium-$target.tgz"
tar -xzf "$tmp/pdfium.tgz" -C "$tmp"
mkdir -p "$dir"
cp "$tmp/lib/$lib" "$dir/"
rm -rf "$tmp"
echo "pdfium $version -> $dir/$lib"
