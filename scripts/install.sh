#!/bin/sh
# Builds NBSP as an RPM and installs it with dnf. Re-run it to update.
# Uninstall with: sudo dnf remove nbsp
set -eu
cd "$(dirname "$0")/.."

[ -f src-tauri/lib/libpdfium.so ] || ./scripts/fetch-pdfium.sh
bun install --frozen-lockfile
bun run tauri build

rpm=$(ls -t src-tauri/target/release/bundle/rpm/*.rpm | head -n 1)
new=$(rpm -qp --qf '%{VERSION}-%{RELEASE}' "$rpm")
old=$(rpm -q --qf '%{VERSION}-%{RELEASE}' nbsp 2>/dev/null || true)

# dnf skips a package whose version is already installed, so rebuilds of the same version reinstall.
if [ "$new" = "$old" ]; then
  sudo dnf reinstall -y "$rpm"
else
  sudo dnf install -y "$rpm"
fi

echo "Installed NBSP $new. Run 'nbsp --help', or open it from the app menu."
