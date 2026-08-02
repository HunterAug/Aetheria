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

use anyhow::Result;

/// Local WebSocket port the UI (React/Tauri) connects to for IPC.
/// Port 3000 is reserved for the frontend dev server on this machine, so the
/// delegate listens elsewhere.
pub const IPC_PORT: u16 = 47_021;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let data_dir = dirs_local_data_dir();
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

fn dirs_local_data_dir() -> std::path::PathBuf {
    // TODO: use a proper platform data-dir crate (e.g. `directories`) once
    // dependencies are finalized; for now default to a repo-local folder.
    std::path::PathBuf::from("delegate/data")
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
