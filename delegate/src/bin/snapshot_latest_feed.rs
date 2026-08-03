//! Read-only tool: connects to a local Freenet node (no keys, no writes,
//! never touches the delegate's own identity/SQLite) and dumps the shared
//! `GlobalDirectoryContract`'s current entries as JSON to stdout. Feeds the
//! marketing website's "Latest posts" snapshot - see CLAUDE.md's "Dev
//! scripts" section and `website/`'s own docs for how the snapshot gets
//! from here into the deployed site.
//!
//! A `SubscriberOnly` entry's `title`/`summary` are unencrypted metadata
//! (same convention the app itself uses for locked teasers) - this never
//! sees, let alone outputs, actual post content, encrypted or not. Every
//! subscriber-only entry is reported `locked: true` unconditionally: unlike
//! the real app (which unlocks a viewer's own posts), this tool has no
//! identity of its own, so nothing it shows is ever "mine".
//!
//! Run (from `delegate/`): `cargo run --release --bin snapshot-latest-feed > out.json`
//! against a real reachable Freenet node (`FreenetBridge::connect_local`'s
//! usual retry-with-backoff applies - see `freenet_bridge.rs`).

use aetheria_delegate::{contracts, freenet_bridge::FreenetBridge};
use anyhow::Result;
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
struct Snapshot {
    generated_at: u64,
    entries: Vec<SnapshotEntry>,
}

#[derive(Serialize)]
struct SnapshotEntry {
    post_id: String,
    author_pubkey: String,
    author_display_name: String,
    title: String,
    summary: String,
    post_contract_id: String,
    access_level: &'static str,
    locked: bool,
    published_at: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let freenet = FreenetBridge::connect_local().await?;
    let entries = contracts::fetch_global_directory(&freenet).await?;

    let mut out: Vec<SnapshotEntry> = entries
        .into_iter()
        .map(|e| {
            let (access_level, locked) = match e.access_level {
                aetheria_types::AccessTier::Public => ("public", false),
                aetheria_types::AccessTier::SubscriberOnly { .. } => ("subscriber", true),
            };
            SnapshotEntry {
                post_id: hex_encode(&e.post_id),
                author_pubkey: hex_encode(&e.author_pubkey),
                author_display_name: e.author_display_name,
                title: e.title,
                summary: e.summary,
                post_contract_id: e.post_contract_id,
                access_level,
                locked,
                published_at: e.published_at,
            }
        })
        .collect();
    out.sort_by(|a, b| b.published_at.cmp(&a.published_at));

    let snapshot = Snapshot {
        generated_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        entries: out,
    };
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
