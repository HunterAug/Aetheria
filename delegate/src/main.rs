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

mod crypto;
mod db;
mod freenet_bridge;
mod ipc;
mod keys;
mod nwc;

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
    let keys = keys::DelegateKeys::load_or_generate(&data_dir.join("identity.key"))?;

    tracing::info!(
        publisher_pubkey = %hex_encode(&keys.master_signing_verifying_bytes()),
        "delegate identity ready"
    );

    let freenet = freenet_bridge::FreenetBridge::connect_local().await?;
    let nwc = nwc::NwcClient::disconnected();

    ipc::serve(IPC_PORT, db, keys, freenet, nwc).await
}

/// Platform-appropriate app data directory (e.g. `%APPDATA%\aetheria\aetheria-delegate\data`
/// on Windows). Deliberately *not* a path relative to the process's working
/// directory - a relative path meant different locations depending on
/// whether the daemon was launched from `delegate/`, the repo root, or (once
/// Tauri spawns it) some other directory entirely, which silently forked the
/// SQLite cache and identity key across multiple stray folders.
fn local_data_dir() -> Result<std::path::PathBuf> {
    let dirs = ProjectDirs::from("com", "aetheria", "aetheria-delegate")
        .context("could not determine a platform data directory")?;
    Ok(dirs.data_dir().to_path_buf())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
