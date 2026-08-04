//! Aetheria Local Delegate — Layer 2 of the architecture.
//!
//! A native Rust daemon that runs alongside the desktop UI. It owns the
//! user's private keys, talks to a local Freenet node over its native host
//! protocol, drives the NWC (NIP-47) payment flow, and maintains a local
//! SQLite cache of decrypted content so the UI never has to touch key
//! material or ciphertext directly.
//!
//! Thin wrapper around the `aetheria_delegate` library crate (see
//! `src/lib.rs`) - the actual subsystems live there so
//! `src/bin/snapshot_latest_feed.rs` can reuse them too.

use aetheria_delegate::{db, ipc, IPC_PORT};
use anyhow::{Context, Result};
use directories::ProjectDirs;

/// Dev/test escape hatch, same spirit as `AETHERIA_DATA_DIR_OVERRIDE` below
/// and `AETHERIA_FREENET_WS_URL` in `freenet_bridge.rs`: run this delegate's
/// IPC listener somewhere other than the standard 47021.
///
/// Exists because verifying anything about *two* Aetheria users on one
/// machine (which is what "your followers get notified when you publish"
/// inherently needs) means running two delegates at once, and the second one
/// can't have the first one's port. Unset for any normal run - the UI in
/// `app/src/lib/delegate.ts` only ever dials 47021.
const IPC_PORT_ENV: &str = "AETHERIA_IPC_PORT";

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
    ipc::serve(ipc_port(), db, data_dir.join("identity.key")).await
}

fn ipc_port() -> u16 {
    match std::env::var(IPC_PORT_ENV).ok().and_then(|p| p.parse().ok()) {
        Some(port) => {
            tracing::warn!(
                port,
                "{IPC_PORT_ENV} is set - listening there instead of the standard IPC port. \
                 Dev/test only."
            );
            port
        }
        None => IPC_PORT,
    }
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
