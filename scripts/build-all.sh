#!/usr/bin/env bash
# Full release-build pipeline, all three platforms, one local command:
#   1. Builds Windows locally (scripts/build.sh, unchanged).
#   2. Triggers build-desktop.yml on GitHub Actions - it now only builds
#      macOS + Linux (see that workflow's own header comment for why
#      Windows isn't in its matrix anymore: this script's step 1 already
#      covers it, and building it twice would just be redundant CI time).
#   3. Waits for that run, downloads its installers, and stages them into
#      website/public/downloads/ next to the Windows one.
#
# Deliberately does NOT `git add`/commit/push anything - website/public/
# downloads/ is left with new/updated files for review. Committing and
# pushing (and, separately, whether/how to update download/page.tsx to link
# to the new platforms) is a decision made when actually ready to ship a
# release, not something every local build should do unattended.
#
# On demand only, same reasoning as build-desktop.yml's on: workflow_dispatch
# (no schedule, no push trigger): a full macOS+Linux build compiles a real
# Freenet node from source on two fresh runners, real CI minutes - this
# should run when there's something to release, not on every commit.
#
# Requires the `gh` CLI, authenticated (`gh auth login` once, interactively -
# this script never handles a token itself).
#
# Usage: scripts/build-all.sh
set -euo pipefail
cd "$(dirname "$0")/.."

GH="gh"
if ! command -v "$GH" >/dev/null 2>&1; then
  # winget's install location - not yet on PATH in every shell right after
  # install (see root CLAUDE.md's PATH-related gotchas for the general
  # pattern of a fresh install not being visible to an already-open shell).
  GH="/c/Program Files/GitHub CLI/gh.exe"
fi
if ! "$GH" --version >/dev/null 2>&1; then
  echo "error: gh CLI not found. Install it and run 'gh auth login' first." >&2
  exit 1
fi
if ! "$GH" auth status >/dev/null 2>&1; then
  echo "error: gh CLI is not authenticated. Run 'gh auth login' first." >&2
  exit 1
fi

REPO="HunterAug/Aetheria"
DOWNLOADS_DIR="website/public/downloads"
mkdir -p "$DOWNLOADS_DIR"

echo "==> [1/3] building Windows locally"
scripts/build.sh
cp builds/Aetheria_0.1.0_x64-setup.exe "$DOWNLOADS_DIR/Aetheria-Setup-x64.exe"
echo "==> staged Windows installer -> $DOWNLOADS_DIR/Aetheria-Setup-x64.exe"

echo "==> [2/3] triggering build-desktop.yml on GitHub Actions (macOS + Linux)"
BEFORE_RUN_ID="$("$GH" run list -R "$REPO" --workflow=build-desktop.yml --limit 1 --json databaseId --jq '.[0].databaseId // empty')"
"$GH" workflow run build-desktop.yml -R "$REPO"

echo "==> waiting for the new run to register..."
RUN_ID=""
for i in $(seq 1 30); do
  RUN_ID="$("$GH" run list -R "$REPO" --workflow=build-desktop.yml --limit 1 --json databaseId --jq '.[0].databaseId // empty')"
  if [ -n "$RUN_ID" ] && [ "$RUN_ID" != "$BEFORE_RUN_ID" ]; then
    break
  fi
  sleep 2
done
if [ -z "$RUN_ID" ] || [ "$RUN_ID" = "$BEFORE_RUN_ID" ]; then
  echo "error: couldn't find the newly triggered run - check https://github.com/$REPO/actions/workflows/build-desktop.yml manually" >&2
  exit 1
fi

echo "==> run https://github.com/$REPO/actions/runs/$RUN_ID started - watching."
echo "    This compiles a real Freenet node from source on each platform;"
echo "    can take 20-40+ minutes. Safe to Ctrl-C and re-check later with:"
echo "    gh run watch $RUN_ID -R $REPO"
"$GH" run watch "$RUN_ID" -R "$REPO" --exit-status

echo "==> [3/3] downloading build artifacts"
TMP_DIR="$(mktemp -d)"
"$GH" run download "$RUN_ID" -R "$REPO" -D "$TMP_DIR"

stage() {
  local pattern="$1" dest="$2"
  local found
  found="$(find "$TMP_DIR" -iname "$pattern" | head -n1)"
  if [ -z "$found" ]; then
    echo "warning: no file matching '$pattern' found in downloaded artifacts - skipping $dest" >&2
    return
  fi
  cp -r "$found" "$DOWNLOADS_DIR/$dest"
  echo "==> staged $dest"
}

stage "*.dmg" "Aetheria-Setup-macos-arm64.dmg"
stage "*.AppImage" "Aetheria-x86_64.AppImage"
stage "*.deb" "Aetheria-amd64.deb"
stage "*.rpm" "Aetheria-x86_64.rpm"

rm -rf "$TMP_DIR"

echo ""
echo "All platforms built. Nothing was committed or pushed - review, then:"
echo "  git status $DOWNLOADS_DIR"
echo "  git add $DOWNLOADS_DIR && git commit -m '...' && git push"
