#!/usr/bin/env bash
# Stops the delegate started by dev-up.sh. Leaves the persistent Freenet dev
# service running by default - it's a standing part of the dev environment
# that predates any given session, not something to tear down casually (see
# CLAUDE.md). Pass --with-freenet to also stop it.
#
# Usage: scripts/dev-down.sh [--with-freenet]
set -euo pipefail
cd "$(dirname "$0")/.."

PIDFILE=".dev-delegate.pid"
FREENET_EXE="C:\\Users\\WebDev\\AppData\\Local\\Freenet\\bin\\freenet.exe"

if [ -f "$PIDFILE" ]; then
  PID="$(cat "$PIDFILE")"
  powershell -NoProfile -Command "Stop-Process -Id $PID -Force -ErrorAction SilentlyContinue"
  rm -f "$PIDFILE"
  echo "==> delegate (pid $PID) stopped."
else
  # No pidfile (e.g. started outside this script) - fall back to killing by
  # process name, matching how this session has always cleaned up manually.
  if powershell -NoProfile -Command "Get-Process aetheria-delegate -ErrorAction SilentlyContinue" | grep -q .; then
    powershell -NoProfile -Command "Get-Process aetheria-delegate -ErrorAction SilentlyContinue | Stop-Process -Force"
    echo "==> delegate stopped (no pidfile - killed by process name)."
  else
    echo "==> delegate wasn't running."
  fi
fi

if [ "${1:-}" = "--with-freenet" ]; then
  powershell -NoProfile -Command "& '$FREENET_EXE' service stop"
  echo "==> Freenet service stopped."
fi
