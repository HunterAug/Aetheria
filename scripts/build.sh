#!/usr/bin/env bash
# Full rebuild pipeline: delegate release binary -> copy into the Tauri
# sidecar dir -> full installer (npm run build:desktop). Ends with a
# byte-for-byte comparison of the freshly-built delegate binary against both
# the Tauri sidecar copy and builds/aetheria-delegate.exe, and refuses to
# report success if they differ.
#
# This check exists because of a real incident: an installer was handed over
# with a stale delegate binary (built before a feature landed, never
# rebuilt after) - the running app looked fine on the surface but silently
# lacked the new IPC ops. A file comparison catches that class of mistake
# unconditionally, without needing to know anything about which features
# exist - unlike an IPC smoke test (checking for a specific recognized op),
# which would need updating every time a new feature lands.
#
# Usage: scripts/build.sh
set -euo pipefail
cd "$(dirname "$0")/.."

# cargo isn't always on PATH in a fresh shell on this machine (see
# CLAUDE.md's environment notes) - add it rather than requiring every
# caller to remember to.
export PATH="$PATH:/c/Users/WebDev/.cargo/bin"

DELEGATE_BIN="delegate/target/release/aetheria-delegate.exe"
SIDECAR_BIN="app/src-tauri/binaries/aetheria-delegate-x86_64-pc-windows-msvc.exe"
BUILDS_BIN="builds/aetheria-delegate.exe"

echo "==> cargo build --release (delegate)"
(cd delegate && cargo build --release)

echo "==> copying delegate binary into the Tauri sidecar dir"
mkdir -p "$(dirname "$SIDECAR_BIN")"
cp "$DELEGATE_BIN" "$SIDECAR_BIN"

echo "==> npm run build:desktop (frontend + Tauri shell + installers)"
(cd app && npm run build:desktop)

echo "==> verifying the bundled binaries actually match today's build"
if ! cmp -s "$DELEGATE_BIN" "$SIDECAR_BIN"; then
  echo "error: $SIDECAR_BIN does not match $DELEGATE_BIN - installer would ship a stale delegate!" >&2
  exit 1
fi
if ! cmp -s "$DELEGATE_BIN" "$BUILDS_BIN"; then
  echo "error: $BUILDS_BIN does not match $DELEGATE_BIN - installer would ship a stale delegate!" >&2
  exit 1
fi
echo "==> confirmed: builds/aetheria-delegate.exe is byte-identical to today's release build."

echo ""
echo "Installer ready:"
echo "  builds/Aetheria_0.1.0_x64-setup.exe"
echo "  builds/Aetheria_0.1.0_x64_en-US.msi"
