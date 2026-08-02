// Copies the release build's distributable outputs into a top-level
// `builds/` folder after `tauri build` finishes, so they're easy to find
// without digging through src-tauri's nested target/release/bundle tree.
// Overwrites on every run rather than keeping history - this is meant to be
// "where's the latest build", not a build archive.

import { existsSync, mkdirSync, copyFileSync, readdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = dirname(dirname(fileURLToPath(import.meta.url)));
const releaseDir = join(appDir, "src-tauri", "target", "release");
const bundleDir = join(releaseDir, "bundle");
const buildsDir = join(appDir, "..", "builds");

function findFiles(dir, predicate) {
  if (!existsSync(dir)) return [];
  const found = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) found.push(...findFiles(full, predicate));
    else if (predicate(entry.name)) found.push(full);
  }
  return found;
}

const targets = [
  // The raw app binary - runs standalone, no install needed.
  join(releaseDir, "aetheria.exe"),
  // Installers, wherever tauri's bundler put them this run.
  ...findFiles(bundleDir, (name) => name.endsWith(".msi")),
  ...findFiles(bundleDir, (name) => name.endsWith("-setup.exe")),
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
  copyFileSync(src, join(buildsDir, destName));
  console.log(`copied ${destName} -> builds/`);
}
