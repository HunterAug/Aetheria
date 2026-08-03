//! Aetheria Local Delegate — Layer 2 of the architecture.
//!
//! A native Rust daemon that runs alongside the desktop UI. It owns the
//! user's private keys, talks to a local Freenet node over its native host
//! protocol, drives the NWC (NIP-47) payment flow, and maintains a local
//! SQLite cache of decrypted content so the UI never has to touch key
//! material or ciphertext directly.
//!
//! This is a Phase 2 scaffold: modules define the intended shape of each
//! subsystem but most bodies are `todo!()` pending the real Freenet
//! websocket API and NWC relay wiring.

mod contracts;
mod crypto;
mod db;
mod freenet_bridge;
mod ipc;
mod keys;
mod nwc;
#[cfg(test)]
mod follow_publisher_e2e_test;
#[cfg(test)]
mod subscriber_registry_e2e_test;

use anyhow::{Context, Result};
use directories::ProjectDirs;

/// Local WebSocket port the UI (React/Tauri) connects to for IPC.
/// Port 3000 is reserved for the frontend dev server on this machine, so the
/// delegate listens elsewhere.
pub const IPC_PORT: u16 = 47_021;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let data_dir = local_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;

    let db = db::LocalStore::open(&data_dir.join("aetheria.sqlite"))?;

    // Loading `DelegateKeys` needs a passphrase, which the delegate can no
    // longer assume it can get by blocking here (a Tauri sidecar has no
    // attached terminal for `rpassword` to prompt on - see keys.rs's module
    // docs). So `main()`'s job is now just standing up the IPC listener;
    // everything that used to happen here after key loading (connecting to
    // Freenet, publishing/loading this identity's contracts, connecting the
    // NWC wallets) moved into `ipc.rs`'s `finish_unlock`, which runs once a
    // passphrase actually arrives - either the same legacy env-var/stdin
    // paths this used to use synchronously (see `ipc.rs::try_legacy_auto_unlock`,
    // spawned alongside the listener below), or a real `unlock` IPC request
    // from the UI. See `ipc.rs`'s module docs for the full picture.
    ipc::serve(IPC_PORT, db, data_dir.join("identity.key")).await
}

/// Platform-appropriate app data directory (e.g. `%APPDATA%\aetheria\aetheria-delegate\data`
/// on Windows). Deliberately *not* a path relative to the process's working
/// directory - a relative path meant different locations depending on
/// whether the daemon was launched from `delegate/`, the repo root, or (once
/// Tauri spawns it) some other directory entirely, which silently forked the
/// SQLite cache and identity key across multiple stray folders.
fn local_data_dir() -> Result<std::path::PathBuf> {
    // Dev/test escape hatch, same spirit as AETHERIA_DEV_PASSPHRASE - lets a
    // "fresh machine" test point at an empty scratch directory instead of
    // this machine's real identity/SQLite cache, without needing a second
    // Windows user profile (`directories::ProjectDirs` resolves via the OS
    // known-folder API on Windows, which ignores %APPDATA%/%LOCALAPPDATA%
    // env var overrides, so redirecting it needs an explicit escape hatch
    // like this one rather than just setting those env vars). Unset for any
    // normal run.
    if let Ok(dir) = std::env::var("AETHERIA_DATA_DIR_OVERRIDE") {
        tracing::warn!(
            dir,
            "AETHERIA_DATA_DIR_OVERRIDE is set - using it instead of the real platform data \
             directory. Dev/test only."
        );
        return Ok(std::path::PathBuf::from(dir));
    }

    let dirs = ProjectDirs::from("com", "aetheria", "aetheria-delegate")
        .context("could not determine a platform data directory")?;
    Ok(dirs.data_dir().to_path_buf())
}
