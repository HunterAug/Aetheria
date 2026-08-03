#!/usr/bin/env bash
# Starts the local dev stack for manual/browser testing: confirms the real
# Freenet node is reachable (starts the persistent dev service if it isn't -
# see CLAUDE.md, this is a standing part of the dev environment, not
# something dev-down.sh tears down by default), then starts the delegate
# release binary against the real local identity, using the same
# AETHERIA_DEV_PASSPHRASE convention documented in CLAUDE.md.
#
# Deliberately does NOT start the Vite dev server - use the Browser tool's
# preview_start ("aetheria-frontend", see .claude/launch.json) for that. It
# already runs `npm run dev` and gives a managed browser tab + log access;
# this script would just be a worse duplicate of it.
#
# Usage: scripts/dev-up.sh
set -euo pipefail
cd "$(dirname "$0")/.."

FREENET_EXE="C:\\Users\\WebDev\\AppData\\Local\\Freenet\\bin\\freenet.exe"
DELEGATE_EXE="delegate/target/release/aetheria-delegate.exe"
PIDFILE=".dev-delegate.pid"

port_listening() {
  powershell -NoProfile -Command "netstat -ano | Select-String ':$1' | Select-String 'LISTENING'" | grep -q .
}

if [ ! -f "$DELEGATE_EXE" ]; then
  echo "error: delegate release binary not found at $DELEGATE_EXE" >&2
  echo "       run scripts/build.sh first (or 'cargo build --release' in delegate/)." >&2
  exit 1
fi

if [ -f "$PIDFILE" ] && powershell -NoProfile -Command "Get-Process -Id $(cat "$PIDFILE") -ErrorAction SilentlyContinue" | grep -q .; then
  echo "delegate already running (pid $(cat "$PIDFILE")), nothing to do."
  exit 0
fi
rm -f "$PIDFILE"

if ! port_listening 7509; then
  echo "==> Freenet not running, starting the persistent dev service..."
  powershell -NoProfile -Command "& '$FREENET_EXE' service start" || true
  for _ in $(seq 1 30); do
    port_listening 7509 && break
    sleep 1
  done
  if ! port_listening 7509; then
    echo "error: Freenet did not bind port 7509 within 30s" >&2
    exit 1
  fi
  echo "==> Freenet up."
else
  echo "==> Freenet already running."
fi

echo "==> starting delegate (real identity, AETHERIA_DEV_PASSPHRASE)..."
export AETHERIA_DEV_PASSPHRASE="aetheria-dev-local-only"
export RUST_LOG=info
"./$DELEGATE_EXE" < /dev/null > delegate.log 2>&1 &

for _ in $(seq 1 20); do
  port_listening 47021 && break
  sleep 0.5
done
if ! port_listening 47021; then
  echo "error: delegate did not bind port 47021 within 10s - see delegate.log" >&2
  exit 1
fi

# git-bash's $! is an MSYS-internal pseudo-PID, not the real Windows PID
# Stop-Process/Get-Process need (confirmed via `ps -W`'s WINPID column) - look
# the real one up by process name instead of trusting $!.
WINPID="$(powershell -NoProfile -Command "(Get-Process aetheria-delegate -ErrorAction SilentlyContinue).Id")"
if [ -z "$WINPID" ]; then
  echo "error: delegate port is open but no aetheria-delegate.exe process found - can't record a pidfile" >&2
  exit 1
fi
echo "$WINPID" > "$PIDFILE"

echo "==> delegate running (pid $WINPID), IPC on 47021. Logs: delegate.log"
echo "==> start the Vite dev server separately (preview_start 'aetheria-frontend'), then open http://localhost:5173"
