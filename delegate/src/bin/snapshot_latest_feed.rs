//! Read-only tool: connects to a local Freenet node (no keys, no writes,
//! never touches the delegate's own identity/SQLite) and dumps the shared
//! `GlobalDirectoryContract`'s current entries as JSON to stdout. Feeds the
//! marketing website's "Latest posts" snapshot - see CLAUDE.md's "Dev
//! scripts" section and `website/`'s own docs for how the snapshot gets
//! from here into the deployed site.
//!
//! Every post is public - this never sees, let alone outputs, anything
//! access-restricted.
//!
//! Takes an optional path to the *previous* snapshot JSON as its one CLI
//! arg and merges its entries with whatever the network returns this time,
//! keyed by `post_contract_id` (freshly-fetched data wins on conflict,
//! otherwise the old cached entry is kept as-is - it was already Ed25519-
//! verified the run it was first fetched). This is deliberate accumulation,
//! not just a mirror of current network state: `GlobalDirectoryContract`
//! itself is capped at 1000 entries and evicts its oldest on overflow (see
//! `contracts.rs::GLOBAL_DIRECTORY_MAX_ENTRIES`), and Freenet may prune a
//! contract's state independently of that - the website's own history,
//! capped separately at `SNAPSHOT_MAX_ENTRIES`, should keep showing a post
//! that's since fallen out of the live network view rather than having it
//! silently vanish from the page. No previous-snapshot arg means no merge
//! (empty prior history), same as this tool's original behavior.
//!
//! Run (from `delegate/`):
//! `cargo run --release --bin snapshot-latest-feed [path/to/previous.json] > out.json`
//! against a real reachable Freenet node (`FreenetBridge::connect_local`'s
//! usual retry-with-backoff applies - see `freenet_bridge.rs`). Since this
//! reads the previous snapshot from a path, that path must differ from
//! wherever stdout is being redirected to - a shell `>` truncates its
//! target before this process ever gets to read it.

use aetheria_delegate::{contracts, freenet_bridge::FreenetBridge};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// The website's own accumulated-history cap - distinct from
/// `contracts.rs::GLOBAL_DIRECTORY_MAX_ENTRIES` (the network contract's own
/// 1000-entry cap). Keep the two in sync only in spirit, not value: this one
/// is deliberately much smaller since it's rendered directly on a marketing
/// page, not stored in a contract.
const SNAPSHOT_MAX_ENTRIES: usize = 100;

#[derive(Serialize, Deserialize)]
struct Snapshot {
    generated_at: u64,
    entries: Vec<SnapshotEntry>,
}

#[derive(Serialize, Deserialize, Clone)]
struct SnapshotEntry {
    post_id: String,
    author_pubkey: String,
    author_display_name: String,
    title: String,
    summary: String,
    post_contract_id: String,
    published_at: u64,
}

fn load_previous(path: &str) -> Vec<SnapshotEntry> {
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    match serde_json::from_slice::<Snapshot>(&bytes) {
        Ok(snapshot) => snapshot.entries,
        Err(e) => {
            eprintln!("==> ignoring unreadable previous snapshot at {path}: {e}");
            Vec::new()
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let previous = std::env::args().nth(1).map(|p| load_previous(&p)).unwrap_or_default();

    let freenet = FreenetBridge::connect_local().await?;
    let fresh = contracts::fetch_global_directory(&freenet).await?;

    let mut by_id: BTreeMap<String, SnapshotEntry> = BTreeMap::new();
    for e in previous {
        by_id.insert(e.post_contract_id.clone(), e);
    }
    for e in fresh {
        by_id.insert(
            e.post_contract_id.clone(),
            SnapshotEntry {
                post_id: hex_encode(&e.post_id),
                author_pubkey: hex_encode(&e.author_pubkey),
                author_display_name: e.author_display_name,
                title: e.title,
                summary: e.summary,
                post_contract_id: e.post_contract_id,
                published_at: e.published_at,
            },
        );
    }

    let mut out: Vec<SnapshotEntry> = by_id.into_values().collect();
    out.sort_by(|a, b| b.published_at.cmp(&a.published_at));
    out.truncate(SNAPSHOT_MAX_ENTRIES);

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
