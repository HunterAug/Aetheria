// Copies the release build's distributable outputs into a top-level
// `builds/` folder after `tauri build` finishes, so they're easy to find
// without digging through src-tauri's nested target/release/bundle tree.
// Overwrites on every run rather than keeping history - this is meant to be
// "where's the latest build", not a build archive.

import {
  existsSync,
  mkdirSync,
  copyFileSync,
  cpSync,
  readdirSync,
} from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = dirname(dirname(fileURLToPath(import.meta.url)));
const releaseDir = join(appDir, "src-tauri", "target", "release");
const bundleDir = join(releaseDir, "bundle");
const buildsDir = join(appDir, "..", "builds");

// Windows binaries carry a `.exe` suffix; macOS/Linux don't. Everything else
// about this script is platform-agnostic already.
const exeSuffix = process.platform === "win32" ? ".exe" : "";

function findFiles(dir, predicate) {
  if (!existsSync(dir)) return [];
  const found = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    // A macOS `.app` bundle is itself a directory - treat it as a leaf if it
    // matches the predicate (copied recursively below) rather than recursing
    // into its contents.
    if (entry.isDirectory() && !predicate(entry.name)) {
      found.push(...findFiles(full, predicate));
    } else if (predicate(entry.name)) {
      found.push(full);
    }
  }
  return found;
}

// Installer/package formats Tauri's bundler produces, by platform: NSIS/MSI
// on Windows, dmg/app on macOS, deb/AppImage/rpm on Linux. Checked broadly
// (not gated on `process.platform`) since matching by extension is simpler
// than tracking which formats `tauri.conf.json`'s `bundle.targets: "all"`
// picks for a given OS.
const installerSuffixes = [
  ".msi",
  "-setup.exe",
  ".dmg",
  ".app",
  ".deb",
  ".AppImage",
  ".rpm",
];

const targets = [
  // The raw app binary - but it only runs standalone if its sidecars sit in
  // the same folder (Tauri's Command::sidecar() resolves them relative to
  // the running exe's own directory, not any fixed path), so the
  // plain-named sidecar copies cargo leaves next to it have to come along
  // too. Confirmed by reproducing "immediately fails" with just aetheria.exe
  // copied alone (2026-08-02) - `NotFound` on the sidecar spawn.
  join(releaseDir, `aetheria${exeSuffix}`),
  join(releaseDir, `aetheria-delegate${exeSuffix}`),
  join(releaseDir, `freenet${exeSuffix}`),
  // Installers, wherever tauri's bundler put them this run.
  ...findFiles(bundleDir, (name) =>
    installerSuffixes.some((suffix) => name.endsWith(suffix)),
  ),
];

const existing = targets.filter((path) => existsSync(path));
if (existing.length === 0) {
  console.error(
    "copy-build-artifacts: no build outputs found under src-tauri/target/release - did `tauri build` run first?",
  );
  process.exit(1);
}

mkdirSync(buildsDir, { recursive: true });
for (const src of existing) {
  const destName = src.split(/[\\/]/).pop();
  const dest = join(buildsDir, destName);
  // `.app` bundles are directories - copyFileSync can't handle those.
  if (src.endsWith(".app")) {
    cpSync(src, dest, { recursive: true });
  } else {
    copyFileSync(src, dest);
  }
  console.log(`copied ${destName} -> builds/`);
}
