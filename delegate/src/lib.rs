//! Library target for `aetheria-delegate`, split out from the `main.rs`
//! daemon binary so a second, much smaller binary
//! (`src/bin/snapshot_latest_feed.rs`, a read-only tool feeding the
//! marketing website's "Latest posts" snapshot - see CLAUDE.md's "Dev
//! scripts" section) can reuse the same Freenet bridge/contracts code
//! without duplicating it. `main.rs` is now a thin wrapper around this.

pub mod contracts;
pub mod crypto;
pub mod db;
pub mod freenet_bridge;
pub mod ipc;
pub mod keys;
pub mod nwc;

#[cfg(test)]
mod follow_publisher_e2e_test;
#[cfg(test)]
mod subscriber_registry_e2e_test;

/// Local WebSocket port the UI (React/Tauri) connects to for IPC.
/// Port 3000 is reserved for the frontend dev server on this machine, so the
/// delegate listens elsewhere.
pub const IPC_PORT: u16 = 47_021;
