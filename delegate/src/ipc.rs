//! Local IPC server: the React/Tauri UI talks to the Delegate over a
//! loopback-only WebSocket so key material never crosses a process boundary
//! the UI can inspect directly.
//!
//! Protocol: newline-agnostic JSON request/response pairs correlated by
//! `id`. Aetheria has no payments or subscriptions - every post is public,
//! so publishing is just "save locally, then sync to the real Freenet
//! network" (see `freenet_bridge.rs`).
//!
//! ## Locked/unlocked startup (as of 2026-08-02)
//!
//! Loading `DelegateKeys` needs a passphrase, and getting one used to block
//! `main()` on an `rpassword` stdin prompt *before* the IPC listener even
//! bound its port - fine for a CLI-launched delegate with a real terminal,
//! fatal for the Tauri sidecar case (no attached terminal to read from, see
//! `keys.rs`'s module docs). So startup is split in two: `serve()` binds and
//! starts accepting connections immediately with `Delegate::unlocked` still
//! `None` (the "locked" state), and every request except `Unlock` is refused
//! with a clear "delegate is locked" error until a passphrase actually
//! unlocks (or creates) the identity - see `handle_unlock`/`finish_unlock`.
//! Only once unlocked does the delegate connect to Freenet and publish/load
//! this identity's `PublisherProfileContract`/`ContentIndexContract` (what
//! used to happen unconditionally in `main.rs` before this split).
//!
//! The old CLI convenience paths (`AETHERIA_DEV_PASSPHRASE`, or an
//! interactive `rpassword` prompt on a real terminal) still work completely
//! unchanged - `try_legacy_auto_unlock`, spawned alongside the listener,
//! races to unlock automatically using the exact same `DelegateKeys::load_or_generate`
//! the old synchronous startup used, just without blocking the listener from
//! starting first. If neither applies (no env var, no interactive terminal -
//! the real Tauri sidecar case), it's a no-op and the delegate just waits for
//! a UI-driven `Unlock` request instead of hanging.

use crate::{
    contracts::{self, PublisherIdentity},
    db::LocalStore,
    freenet_bridge::FreenetBridge,
    keys::DelegateKeys,
    watcher::{self, EventSender, WatcherHandle},
};
use anyhow::{Context, Result};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    /// Must succeed before any other request - see this module's docs on the
    /// locked/unlocked startup split. Distinguishes "no identity.key yet"
    /// (creates a new one under `passphrase`, matching what the CLI's
    /// interactive `prompt_new_passphrase` would do, minus the confirm-field
    /// double-entry - the UI is responsible for that) from "identity.key
    /// exists" (unlocks it; a wrong passphrase is a retryable error, not a
    /// crash) purely by checking whether the identity file exists on disk -
    /// the caller doesn't need to say which case it thinks it's in.
    Unlock {
        passphrase: String,
    },
    /// The only other request answerable while locked (see `handle_request`'s
    /// dispatch gate) - lets the UI show the right unlock screen (a plain
    /// unlock form vs. a create-new-identity form with password
    /// confirmation) without guessing, and without exposing anything more
    /// sensitive than "does a file exist on disk".
    LockStatus,
    ListPosts,
    GetPost {
        post_id: String,
    },
    PublishPost {
        title: String,
        summary: String,
        markdown: String,
    },
    GetProfile,
    UpdateProfile {
        display_name: String,
        bio: String,
        /// A `data:<mime>;base64,<payload>` URL, or `None`/omitted to leave
        /// the avatar unchanged from whatever's already stored.
        #[serde(default)]
        avatar_data_url: Option<String>,
    },
    /// Reader-role action: fetches `author_pubkey`'s real, signed
    /// `PublisherProfileContract` off the network to validate they actually
    /// exist before saving anything locally (see
    /// `contracts::fetch_remote_profile`) - fails clearly rather than
    /// blindly following an unverified pubkey.
    FollowPublisher {
        /// Hex-encoded 32-byte Ed25519 master signing pubkey.
        author_pubkey: String,
    },
    UnfollowPublisher {
        author_pubkey: String,
    },
    /// Locally-cached list of followed publishers - no network call.
    ListFollowedPublishers,
    /// Every followed publisher's posts (no own posts) - backs the Home tab.
    GetFollowingFeed,
    /// The most recent posts from *every* publisher on the network (not just
    /// followed ones), via the shared `GlobalDirectoryContract` - backs the
    /// Latest tab. See `contracts::fetch_global_directory`'s module docs for
    /// why this exists and its 1000-entry cap.
    GetLatestFeed,
    /// A publisher's profile (real, network-fetched and signature-verified)
    /// plus their recent posts, for viewing someone else's profile page
    /// (with a Follow/Unfollow button) - reached by clicking an author's
    /// name in any feed.
    GetPublisherProfile {
        author_pubkey: String,
    },
    /// Fetches and decodes a post from *another* publisher by its
    /// `PostDataContract` id.
    GetRemotePost {
        post_contract_id: String,
    },
    /// Fetches an avatar image (this delegate's own, or any other
    /// publisher's whose key it already knows via a feed item / profile) by
    /// its `PostDataContract` id and returns it as a `data:` URL ready to
    /// drop into an `<img src>` - see `handle_get_remote_avatar`.
    GetRemoteAvatar {
        avatar_freenet_key: String,
    },
    /// Is this delegate's Freenet node actually part of the network right
    /// now? Answered by asking the node itself how many peers it holds ring
    /// connections to (see `FreenetBridge::query_node_status`), alongside
    /// this delegate's own observed contract-operation health.
    ///
    /// The third request answerable **while locked** (with `Unlock` and
    /// `LockStatus`), because there is a genuinely different, honest answer
    /// to give in that state - see `handle_get_network_status`.
    ///
    /// Exists because every previous way a Freenet connection could be
    /// broken - a stale port squatter, a node too old to talk to the
    /// network, a VPN breaking NAT hole-punching - produced the exact same
    /// visible symptom in the UI: empty feeds, indistinguishable from a
    /// network that genuinely has nothing to show.
    GetNetworkStatus,
}

#[derive(Deserialize)]
struct Envelope {
    id: String,
    #[serde(flatten)]
    request: Request,
}

#[derive(Serialize)]
struct Response<'a> {
    id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Everything that only exists once the delegate is unlocked - split out of
/// `Delegate` so "locked" is representable as `Delegate::unlocked == None`
/// rather than needing placeholder/dummy values for a `FreenetBridge` (which
/// has no cheap empty state - it's a live websocket connection) or the other
/// fields here.
struct Unlocked {
    keys: DelegateKeys,
    freenet: FreenetBridge,
    identity: PublisherIdentity,
    /// The live "someone you follow just published" watcher (`watcher.rs`),
    /// started once per unlock. Held here so `follow_publisher`/
    /// `unfollow_publisher` can tell it the followed set changed - it owns
    /// its own Freenet connection and its own task, and never touches this
    /// `Delegate` (it shares only the `LocalStore` behind an `Arc`), so it
    /// can't deadlock against an in-flight IPC request.
    watcher: WatcherHandle,
}

struct Delegate {
    /// Shared rather than owned since `watcher.rs`'s background task reads
    /// and writes the same tables (followed publishers, the durable remote
    /// post cache, the notification claims) from outside any IPC request.
    /// `LocalStore` already guards its `rusqlite::Connection` with a mutex of
    /// its own, so sharing it is a matter of ownership, not new locking.
    db: Arc<LocalStore>,
    identity_key_path: PathBuf,
    unlocked: Option<Unlocked>,
    /// Broadcast side of the server-push channel - see `serve`'s per-
    /// connection forwarder and `watcher.rs`'s module docs.
    events: EventSender,
}

impl Delegate {
    /// Panics if the delegate is still locked. Safe to call unconditionally
    /// from any handler below `handle_message`'s dispatch gate, which refuses
    /// every request except `Unlock` while `unlocked` is `None` - so no
    /// handler that calls this is ever reached in a locked state.
    fn unlocked(&self) -> &Unlocked {
        self.unlocked
            .as_ref()
            .expect("Delegate::unlocked() called while locked - dispatch gate should have refused this")
    }

}

/// How many server-push events can be buffered per connected UI before the
/// slowest one starts missing them. A UI that falls this far behind on
/// "somebody published" events has bigger problems than the events; the
/// `Lagged` case below is logged rather than silently ignored, and the posts
/// themselves are never lost (they're in the durable cache and the feeds).
const EVENT_BUFFER: usize = 64;

pub async fn serve(port: u16, db: LocalStore, identity_key_path: PathBuf) -> Result<()> {
    let (events, _) = tokio::sync::broadcast::channel(EVENT_BUFFER);
    let delegate = Arc::new(Mutex::new(Delegate {
        db: Arc::new(db),
        identity_key_path: identity_key_path.clone(),
        unlocked: None,
        events: events.clone(),
    }));

    tokio::spawn(try_legacy_auto_unlock(delegate.clone(), identity_key_path));

    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "delegate IPC listening (loopback only)");

    while let Ok((stream, peer)) = listener.accept().await {
        tracing::debug!(%peer, "UI connection accepted");
        let delegate = delegate.clone();
        let mut event_rx = events.subscribe();
        tokio::spawn(async move {
            let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
                return;
            };
            let (write, mut read) = ws.split();
            // Shared because this connection now has two writers: the
            // request/response loop below, and the push forwarder. The
            // protocol stays exactly as it was for replies - a push is
            // distinguished purely by carrying `"event"` and no `"id"`, so a
            // client that doesn't know about pushes (or an older UI build)
            // simply ignores them instead of mis-resolving a request.
            let write = Arc::new(Mutex::new(write));

            let forward_to = write.clone();
            let forwarder = tokio::spawn(async move {
                loop {
                    match event_rx.recv().await {
                        Ok(event) => {
                            let text = event.to_string();
                            if forward_to
                                .lock()
                                .await
                                .send(Message::Text(text.into()))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                            tracing::warn!(missed, "a UI connection missed server-push events");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });

            while let Some(Ok(msg)) = read.next().await {
                let Message::Text(text) = msg else { continue };
                let reply = handle_message(&delegate, &text).await;
                if write
                    .lock()
                    .await
                    .send(Message::Text(reply.into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            // The read half is gone, so nothing can be replied to and nobody
            // is reading pushes on this socket either.
            forwarder.abort();
        });
    }

    Ok(())
}

/// Races the IPC-driven `Unlock` request to bring the delegate out of its
/// locked startup state, using the exact same passphrase sources the old
/// (pre-restructuring) synchronous startup used - see this module's docs.
/// Whichever unlock path finishes first wins; `finish_unlock` no-ops if the
/// delegate is already unlocked by the time it acquires the lock.
async fn try_legacy_auto_unlock(delegate: Arc<Mutex<Delegate>>, identity_key_path: PathBuf) {
    let should_attempt =
        std::env::var("AETHERIA_DEV_PASSPHRASE").is_ok() || std::io::stdin().is_terminal();
    if !should_attempt {
        tracing::info!(
            "no AETHERIA_DEV_PASSPHRASE and no interactive terminal attached - delegate stays \
             locked until a UI sends an `unlock` request"
        );
        return;
    }

    let result =
        tokio::task::spawn_blocking(move || DelegateKeys::load_or_generate(&identity_key_path))
            .await;
    let keys = match result {
        Ok(Ok(keys)) => keys,
        Ok(Err(e)) => {
            tracing::error!(
                error = %e,
                "startup unlock failed - delegate stays locked, waiting for an `unlock` IPC request"
            );
            return;
        }
        Err(e) => {
            tracing::error!(error = %e, "startup unlock task panicked - delegate stays locked");
            return;
        }
    };

    let mut d = delegate.lock().await;
    if d.unlocked.is_some() {
        return; // Raced with a UI-driven `unlock` that got there first.
    }
    if let Err(e) = finish_unlock(&mut d, keys).await {
        tracing::error!(
            error = %e,
            "finishing startup after automatic unlock failed - delegate stays locked"
        );
    }
}

/// Handles a passphrase from either unlock path (see `Request::Unlock`'s
/// docs for the new-vs-existing distinction) once the raw `DelegateKeys` are
/// in hand: connects to Freenet (with `FreenetBridge::connect_local`'s own
/// retry-with-backoff, see that function's docs) and publishes/loads this
/// identity's `PublisherProfileContract`/`ContentIndexContract` - the same
/// work `main.rs` used to do unconditionally before this module's
/// locked/unlocked split.
async fn finish_unlock(delegate: &mut Delegate, keys: DelegateKeys) -> Result<()> {
    tracing::info!(
        publisher_pubkey = %hex::encode(keys.master_signing_verifying_bytes()),
        "delegate identity ready"
    );
    let freenet = FreenetBridge::connect_local()
        .await
        .context("connecting to the Freenet node")?;
    let identity = contracts::ensure_publisher_identity(&freenet, &delegate.db, &keys)
        .await
        .context("publishing/loading this delegate's PublisherProfileContract and ContentIndexContract")?;
    tracing::info!(
        content_index = %identity.content_index_key.encoded_contract_id(),
        publisher_profile = %identity.profile_key.encoded_contract_id(),
        "Freenet publisher identity ready"
    );
    // Started here rather than in `serve()` because there is no followed-
    // publisher list to watch (and no Freenet identity at all) until an
    // identity is actually unlocked. It opens its own connection to the node
    // and runs independently from this point on - a failure inside it is
    // logged by the task itself and never propagates back into unlock.
    let watcher = watcher::spawn(delegate.db.clone(), delegate.events.clone());
    delegate.unlocked = Some(Unlocked {
        keys,
        freenet,
        identity,
        watcher,
    });
    Ok(())
}

/// First run (no identity file yet) creates a fresh identity under
/// `passphrase`; an existing file is unlocked (or migrated, if legacy
/// plaintext) with it. A wrong passphrase against an existing file surfaces
/// as a plain, retryable `Err` - not a crash - matching
/// `DelegateKeys::unlock_existing`'s contract.
async fn handle_unlock(delegate: &mut Delegate, passphrase: &str) -> Result<serde_json::Value> {
    if delegate.unlocked.is_some() {
        return Ok(serde_json::json!({ "created_new_identity": false, "already_unlocked": true }));
    }

    let is_new = !delegate.identity_key_path.exists();
    let path = delegate.identity_key_path.clone();
    let keys = if is_new {
        anyhow::ensure!(!passphrase.is_empty(), "passphrase cannot be empty");
        DelegateKeys::create_new(&path, passphrase)?
    } else {
        DelegateKeys::unlock_existing(&path, passphrase)?
    };

    finish_unlock(delegate, keys).await?;
    Ok(serde_json::json!({ "created_new_identity": is_new, "already_unlocked": false }))
}

/// Honest, live answer to "is Freenet actually working right now?".
///
/// **Why this is answerable while locked**: the `FreenetBridge` genuinely
/// does not exist until `finish_unlock` builds one - the connection is
/// established as part of unlocking, not before it (see `Unlocked`, which
/// exists precisely because a `FreenetBridge` has no meaningful empty
/// state). Rather than force a connection to exist earlier than it
/// structurally can, this reports `state: "locked"` in that window, which is
/// the truthful answer: there is no Freenet connection yet *because nothing
/// has unlocked one*, which is different from one being broken. The UI only
/// renders its indicator after unlock anyway, so in practice this branch is
/// for any other caller (a script, a future pre-unlock diagnostic screen)
/// that asks early.
///
/// `state` is the single field a UI should switch on:
/// - `"connected"`  - the node reports at least one peer connection.
/// - `"isolated"`   - the node is up and answering, but is connected to
///                    **zero** peers. Feeds will look empty and nothing will
///                    publish. This is the state a VPN or a restrictive
///                    firewall produces, and the one this whole feature
///                    exists to stop being invisible.
/// - `"unknown"`    - the node did not answer the status query at all
///                    (`query_error` says why). Usually means the local node
///                    process died or its API socket dropped.
/// - `"locked"`     - see above.
///
/// `last_successful_operation_secs_ago` / `last_error` are a *second,
/// independent* signal from the first: they come from this delegate's own
/// observed contract-operation outcomes (`FreenetBridge::record_success`/
/// `record_failure`), not from the node's self-report. Kept alongside the
/// peer count rather than folded into `state` because they can genuinely
/// disagree in an informative way - e.g. a node with healthy peer
/// connections whose operations are all still timing out, which is the
/// documented gateway-network flakiness rather than a connectivity problem.
async fn handle_get_network_status(delegate: &Delegate) -> Result<serde_json::Value> {
    let Some(unlocked) = delegate.unlocked.as_ref() else {
        return Ok(serde_json::json!({
            "state": "locked",
            "freenet_connected": false,
            "peer_count": serde_json::Value::Null,
            "node_peer_id": serde_json::Value::Null,
            "last_successful_operation_secs_ago": serde_json::Value::Null,
            "last_error": serde_json::Value::Null,
            "query_error": serde_json::Value::Null,
        }));
    };

    let status = unlocked.freenet.query_node_status().await;
    let state = match status.peer_count {
        Some(0) => "isolated",
        Some(_) => "connected",
        None => "unknown",
    };

    Ok(serde_json::json!({
        "state": state,
        "freenet_connected": state == "connected",
        "peer_count": status.peer_count,
        "node_peer_id": status.node_peer_id,
        "last_successful_operation_secs_ago": status.last_success_secs_ago,
        "last_error": status.last_error,
        "query_error": status.query_error,
    }))
}

async fn handle_message(delegate: &Arc<Mutex<Delegate>>, text: &str) -> String {
    let envelope: Envelope = match serde_json::from_str(text) {
        Ok(e) => e,
        Err(e) => {
            return serde_json::to_string(&Response {
                id: "unknown",
                result: None,
                error: Some(format!("invalid request: {e}")),
            })
            .unwrap();
        }
    };

    let mut d = delegate.lock().await;
    let outcome = handle_request(&mut d, envelope.request).await;

    let response = match outcome {
        Ok(result) => Response {
            id: &envelope.id,
            result: Some(result),
            error: None,
        },
        Err(e) => Response {
            id: &envelope.id,
            result: None,
            error: Some(e.to_string()),
        },
    };
    serde_json::to_string(&response).unwrap()
}

/// Dispatch gate: `Unlock` is the only request handled while locked; every
/// other request is refused with a clear, retryable error instead of being
/// dispatched at all - so every handler below can assume `delegate.unlocked`
/// is `Some` (see `Delegate::unlocked()`).
async fn handle_request(delegate: &mut Delegate, request: Request) -> Result<serde_json::Value> {
    match request {
        Request::Unlock { passphrase } => handle_unlock(delegate, &passphrase).await,
        Request::LockStatus => Ok(serde_json::json!({
            "locked": delegate.unlocked.is_none(),
            "has_existing_identity": delegate.identity_key_path.exists(),
        })),
        Request::GetNetworkStatus => handle_get_network_status(delegate).await,
        other => {
            anyhow::ensure!(
                delegate.unlocked.is_some(),
                "delegate is locked - send `unlock` first"
            );
            match other {
                Request::Unlock { .. } | Request::LockStatus | Request::GetNetworkStatus => {
                    unreachable!("handled above")
                }
                Request::ListPosts => handle_list_posts(delegate),
                Request::GetPost { post_id } => handle_get_post(delegate, &post_id),
                Request::PublishPost {
                    title,
                    summary,
                    markdown,
                } => handle_publish_post(delegate, &title, &summary, &markdown).await,
                Request::GetProfile => handle_get_profile(delegate),
                Request::UpdateProfile {
                    display_name,
                    bio,
                    avatar_data_url,
                } => {
                    handle_update_profile(delegate, &display_name, &bio, avatar_data_url.as_deref())
                        .await
                }
                Request::FollowPublisher { author_pubkey } => {
                    handle_follow_publisher(delegate, &author_pubkey).await
                }
                Request::UnfollowPublisher { author_pubkey } => {
                    handle_unfollow_publisher(delegate, &author_pubkey)
                }
                Request::ListFollowedPublishers => handle_list_followed_publishers(delegate),
                Request::GetFollowingFeed => handle_get_following_feed(delegate).await,
                Request::GetLatestFeed => handle_get_latest_feed(delegate).await,
                Request::GetPublisherProfile { author_pubkey } => {
                    handle_get_publisher_profile(delegate, &author_pubkey).await
                }
                Request::GetRemotePost { post_contract_id } => {
                    handle_get_remote_post(delegate, &post_contract_id).await
                }
                Request::GetRemoteAvatar { avatar_freenet_key } => {
                    handle_get_remote_avatar(delegate, &avatar_freenet_key).await
                }
            }
        }
    }
}

fn handle_list_posts(delegate: &Delegate) -> Result<serde_json::Value> {
    let posts = delegate.db.list_posts()?;
    let json: Vec<_> = posts
        .into_iter()
        .map(|p| {
            // `post_contract_id` is `None` for a post whose network publish
            // failed (or hasn't been retried yet) - see
            // `handle_publish_post`. That's a legitimate, non-error state:
            // the post is real and locally saved, just not yet distributed
            // to the network, so it still shows up here rather than being
            // hidden or treated as corrupt.
            serde_json::json!({
                "post_id": hex::encode(p.post_id),
                "title": p.title,
                "summary": p.summary,
                "published_at": p.published_at,
                "network_synced": p.post_contract_id.is_some(),
                "post_contract_id": p.post_contract_id,
            })
        })
        .collect();
    Ok(serde_json::json!(json))
}

fn handle_get_post(delegate: &Delegate, post_id_hex: &str) -> Result<serde_json::Value> {
    let post_id: [u8; 16] = hex::decode_array(post_id_hex)?;
    let row = delegate
        .db
        .get_post(&post_id)?
        .ok_or_else(|| anyhow::anyhow!("post not found"))?;

    Ok(serde_json::json!({
        "post_id": post_id_hex,
        "title": row.title,
        "markdown": row.markdown,
        "network_synced": row.post_contract_id.is_some(),
        "post_contract_id": row.post_contract_id,
    }))
}

async fn handle_publish_post(
    delegate: &Delegate,
    title: &str,
    summary: &str,
    markdown: &str,
) -> Result<serde_json::Value> {
    let mut post_id = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut post_id);
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    // Local SQLite write stays exactly as it was before Freenet was wired
    // up - it's the fast local cache `list_posts`/`get_post` read from, and
    // must keep working even though publishing now also reaches the network.
    delegate.db.insert_post(&post_id, title, summary, now, markdown)?;

    // The local SQLite write above already committed - this post is real
    // and already in the local feed (`list_posts`/`get_post` will show it)
    // regardless of what happens next. Freenet is additive on top of that,
    // per the design philosophy documented in CLAUDE.md and at the top of
    // this file, and the real gateway network is known to be flaky enough
    // that even `freenet_bridge.rs`'s client-side retries sometimes come up
    // empty (see CLAUDE.md's "Working end-to-end" section) - that is
    // expected, not a bug to propagate as a hard failure. So: don't let a
    // network-publish error fail the whole IPC response and don't let it
    // invalidate the local write either. Catch it, log it, and report
    // honestly which side succeeded so the UI can show something like
    // "saved locally, not yet synced to the network" instead of a bare
    // failure - while `list_posts`/`get_post` keep working unconditionally
    // for the post either way.
    let (post_contract_id, network_synced, network_error) = match contracts::publish_post_to_network(
        &delegate.unlocked().freenet,
        &delegate.unlocked().keys,
        &delegate.unlocked().identity,
        post_id,
        title,
        summary,
        now,
        markdown.as_bytes().to_vec(),
    )
    .await
    {
        Ok(contract_id) => {
            delegate.db.set_post_contract_id(&post_id, &contract_id)?;
            (Some(contract_id), true, None)
        }
        Err(e) => {
            tracing::warn!(
                post_id = %hex::encode(post_id),
                error = %e,
                "network publish failed after retries; post saved locally only, not yet synced to the network"
            );
            (None, false, Some(e.to_string()))
        }
    };

    // Best-effort, same "don't let this fail the whole publish" philosophy
    // as everything else in this function - the post is already real and
    // live via ContentIndexContract regardless of whether it also makes it
    // into the Latest feed's shared directory. Only attempted once the main
    // publish actually produced a real post_contract_id to point at.
    if let Some(contract_id) = &post_contract_id {
        if let Err(e) = contracts::publish_to_global_directory(
            &delegate.unlocked().freenet,
            &delegate.unlocked().keys,
            post_id,
            &own_display_name(delegate)?,
            title,
            summary,
            contract_id.clone(),
            now,
        )
        .await
        {
            tracing::warn!(
                post_id = %hex::encode(post_id),
                error = %e,
                "publishing to the global Latest-feed directory failed after retries - post is \
                 still live via ContentIndexContract, just won't show up in Latest yet"
            );
        }
    }

    Ok(serde_json::json!({
        "post_id": hex::encode(post_id),
        "post_contract_id": post_contract_id,
        "network_synced": network_synced,
        "network_error": network_error,
    }))
}

/// This delegate's own display name, falling back to the same
/// "Untitled Publication" placeholder used everywhere else a blank name
/// would otherwise render as empty.
fn own_display_name(delegate: &Delegate) -> Result<String> {
    Ok(delegate
        .db
        .get_profile()?
        .map(|p| p.display_name)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Untitled Publication".to_string()))
}

fn handle_get_profile(delegate: &Delegate) -> Result<serde_json::Value> {
    let avatar_freenet_key = contracts::known_avatar_key(&delegate.db)?;
    // Every other publisher has to be followed by pasting this hex pubkey
    // (see `Following.tsx`) - it needs to be shown *somewhere* in the UI so
    // a publisher can actually hand it to someone, and Profile is the only
    // screen showing this delegate's own identity.
    let author_pubkey = hex::encode(delegate.unlocked().keys.master_signing_verifying_bytes());
    match delegate.db.get_profile()? {
        Some(p) => Ok(serde_json::json!({
            "display_name": p.display_name,
            "bio": p.bio,
            "avatar_data_url": match (&p.avatar_bytes, &p.avatar_mime) {
                (Some(bytes), Some(mime)) => Some(encode_data_url(mime, bytes)),
                _ => None,
            },
            "avatar_freenet_key": avatar_freenet_key,
            "author_pubkey": author_pubkey,
        })),
        // No local row yet (fresh install, Settings never saved) - blank on
        // purpose, matching the blank title `ensure_publisher_identity`
        // publishes on first run. The UI treats an empty `display_name` as
        // "not configured yet" and prompts for one rather than silently
        // shipping with a placeholder nobody remembers to change.
        None => Ok(serde_json::json!({
            "display_name": "",
            "bio": "",
            "avatar_data_url": null,
            "avatar_freenet_key": avatar_freenet_key,
            "author_pubkey": author_pubkey,
        })),
    }
}

async fn handle_update_profile(
    delegate: &Delegate,
    display_name: &str,
    bio: &str,
    avatar_data_url: Option<&str>,
) -> Result<serde_json::Value> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    // Resolve this call's avatar bytes/mime/network-key: either a fresh
    // upload (published/updated on the network right away, best-effort), or
    // - if the user didn't touch the avatar this save - whatever's already
    // cached locally and already registered on the network.
    let mut network_error: Option<String> = None;
    let (avatar_bytes, avatar_mime, avatar_freenet_key): (
        Option<Vec<u8>>,
        Option<String>,
        Option<String>,
    ) = if let Some(data_url) = avatar_data_url {
        let (mime, bytes) = decode_data_url(data_url)?;
        let key = match contracts::publish_avatar_to_network(
            &delegate.unlocked().freenet,
            &delegate.db,
            &delegate.unlocked().identity,
            delegate.unlocked().keys.master_signing_verifying_bytes(),
            bytes.clone(),
        )
        .await
        {
            Ok(key) => Some(key),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "avatar network publish failed after retries; avatar saved locally only, not yet synced to the network"
                );
                network_error = Some(e.to_string());
                contracts::known_avatar_key(&delegate.db)?
            }
        };
        (Some(bytes), Some(mime), key)
    } else {
        let existing = delegate.db.get_profile()?;
        let key = contracts::known_avatar_key(&delegate.db)?;
        match existing {
            Some(p) => (p.avatar_bytes, p.avatar_mime, key),
            None => (None, None, key),
        }
    };

    // Local write commits unconditionally - same "network is additive, never
    // blocks the local save" philosophy as `handle_publish_post` above: a
    // hiccup on the flaky real gateway network must not lose the user's edits.
    delegate.db.set_profile(
        display_name,
        bio,
        avatar_bytes.as_deref(),
        avatar_mime.as_deref(),
        now,
    )?;

    let profile_synced = match contracts::publish_profile_to_network(
        &delegate.unlocked().freenet,
        &delegate.unlocked().keys,
        &delegate.unlocked().identity,
        display_name,
        bio,
        avatar_freenet_key.clone(),
    )
    .await
    {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "profile network publish failed after retries; changes saved locally only, not yet synced to the network"
            );
            network_error.get_or_insert(e.to_string());
            false
        }
    };

    Ok(serde_json::json!({
        "display_name": display_name,
        "bio": bio,
        "avatar_data_url": match (&avatar_bytes, &avatar_mime) {
            (Some(bytes), Some(mime)) => Some(encode_data_url(mime, bytes)),
            _ => None,
        },
        "avatar_freenet_key": avatar_freenet_key,
        "network_synced": profile_synced,
        "network_error": network_error,
    }))
}

/// Fetches and verifies `author_pubkey_hex`'s real `PublisherProfileContract`
/// before saving anything - `contracts::fetch_remote_profile` refuses to
/// return a profile whose signature doesn't check out, so a successful
/// return here means a real, self-consistent publisher was found. Fails
/// clearly (rather than saving an unverified pubkey) if none exists yet.
async fn handle_follow_publisher(
    delegate: &Delegate,
    author_pubkey_hex: &str,
) -> Result<serde_json::Value> {
    let author_pubkey: [u8; 32] = hex::decode_array(author_pubkey_hex)?;
    anyhow::ensure!(
        author_pubkey != delegate.unlocked().keys.master_signing_verifying_bytes(),
        "that's your own publication - you're already in your own Home feed"
    );

    let profile = contracts::fetch_remote_profile(&delegate.unlocked().freenet, author_pubkey)
        .await
        .context("looking up that publisher's profile on the network")?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no publisher profile found for that pubkey - double check it's correct and \
                 that publisher has actually published something"
            )
        })?;

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    // Same blank-title fallback `ReaderFeed.tsx`/`ensure_publisher_identity`
    // already use for this delegate's own identity - a freshly-created
    // publisher with no title set yet shouldn't render as an empty name.
    let display_name = if profile.display_name.trim().is_empty() {
        "Untitled Publication".to_string()
    } else {
        profile.display_name.clone()
    };
    delegate.db.follow_publisher(
        &author_pubkey,
        &display_name,
        profile.avatar_freenet_key.as_deref(),
        now,
    )?;
    // Start watching them for new posts right away rather than at the
    // watcher's next poll tick - following someone and then having their
    // next post arrive silently for three minutes would be a strange first
    // impression of the feature.
    delegate.unlocked().watcher.refresh();

    Ok(serde_json::json!({
        "author_pubkey": author_pubkey_hex,
        "display_name": display_name,
        "bio": profile.bio,
        "avatar_freenet_key": profile.avatar_freenet_key,
        "followed_at": now,
    }))
}

fn handle_unfollow_publisher(delegate: &Delegate, author_pubkey_hex: &str) -> Result<serde_json::Value> {
    let author_pubkey: [u8; 32] = hex::decode_array(author_pubkey_hex)?;
    delegate.db.unfollow_publisher(&author_pubkey)?;
    // Stops the notifications too - the watcher rebuilds its Freenet
    // subscriptions from the (now shorter) followed list, since Freenet's
    // client protocol has no unsubscribe (see `FreenetBridge::subscribe`).
    delegate.unlocked().watcher.refresh();
    Ok(serde_json::json!({ "author_pubkey": author_pubkey_hex }))
}

fn handle_list_followed_publishers(delegate: &Delegate) -> Result<serde_json::Value> {
    let rows = delegate.db.list_followed_publishers()?;
    let json: Vec<_> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "author_pubkey": hex::encode(r.author_pubkey),
                "display_name": r.display_name,
                "avatar_freenet_key": r.avatar_freenet_key,
                "followed_at": r.followed_at,
            })
        })
        .collect();
    Ok(serde_json::json!(json))
}

/// Shared shape for one feed card, used by every feed-producing handler
/// below (`followed_feed_items`, `handle_get_latest_feed`,
/// `handle_get_publisher_profile`) so the rendering stays in one place.
#[allow(clippy::too_many_arguments)]
fn feed_item_json(
    post_id: [u8; 16],
    title: &str,
    summary: &str,
    published_at: u64,
    author_pubkey: [u8; 32],
    author_display_name: &str,
    author_avatar_freenet_key: Option<&str>,
    is_own: bool,
    post_contract_id: Option<String>,
) -> serde_json::Value {
    serde_json::json!({
        "post_id": hex::encode(post_id),
        "title": title,
        "summary": summary,
        "published_at": published_at,
        "author_pubkey": hex::encode(author_pubkey),
        "author_display_name": author_display_name,
        "author_avatar_freenet_key": author_avatar_freenet_key,
        "is_own": is_own,
        "post_contract_id": post_contract_id,
    })
}

/// Renders a durably-cached post header (`db::CachedRemotePost`) in the same
/// shape `feed_item_json` produces for a live one - the two are
/// interchangeable to the UI by design, since the whole point of the cache
/// is that "seen once" and "seen just now" should look identical once
/// something's in a feed.
///
/// `author_avatar_freenet_key` isn't stored on `CachedRemotePost` itself (the
/// cache exists for post headers, and an avatar can change independently of
/// any given post) - passed in by the caller, which already has it on hand
/// from whatever it just resolved the author's identity through (a followed
/// publisher's cached row, a just-fetched or cached profile, or this
/// delegate's own).
fn cached_post_feed_item(
    row: &crate::db::CachedRemotePost,
    author_avatar_freenet_key: Option<&str>,
    is_own: bool,
) -> serde_json::Value {
    serde_json::json!({
        "post_id": hex::encode(row.post_id),
        "title": row.title,
        "summary": row.summary,
        "published_at": row.published_at,
        "author_pubkey": hex::encode(row.author_pubkey),
        "author_display_name": row.author_display_name,
        "author_avatar_freenet_key": author_avatar_freenet_key,
        "is_own": is_own,
        "post_contract_id": row.post_contract_id,
    })
}

/// Every followed publisher's posts - a live fetch merged with the durable
/// local cache (see `db.rs`'s module docs on `cached_remote_posts`), so a
/// publisher's older posts don't vanish from Home just because this
/// refresh's live fetch failed or the network's current copy of their index
/// happens to be thinner than what's actually been seen before. Best-effort
/// per publisher on the live half: a fetch failure for one followed
/// publisher (the real gateway network is known to be flaky, see CLAUDE.md)
/// is logged and that publisher's live fetch is skipped for this refresh,
/// not treated as a reason to fail the whole feed for every other publisher
/// - their cached posts still appear via the merge below regardless.
async fn followed_feed_items(delegate: &Delegate) -> Vec<serde_json::Value> {
    let followed = match delegate.db.list_followed_publishers() {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "reading followed publishers from local db failed");
            return Vec::new();
        }
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    for f in &followed {
        match contracts::fetch_remote_posts(&delegate.unlocked().freenet, f.author_pubkey).await {
            Ok(posts) => {
                for header in posts {
                    if let Err(e) = delegate.db.cache_remote_post(
                        &header.post_id,
                        &f.author_pubkey,
                        &f.display_name,
                        &header.title,
                        &header.summary,
                        &header.post_contract_id,
                        header.published_at,
                        now,
                    ) {
                        tracing::warn!(error = %e, "caching a followed publisher's post failed");
                    }
                    seen.insert(header.post_contract_id.clone());
                    items.push(feed_item_json(
                        header.post_id,
                        &header.title,
                        &header.summary,
                        header.published_at,
                        f.author_pubkey,
                        &f.display_name,
                        f.avatar_freenet_key.as_deref(),
                        false,
                        Some(header.post_contract_id),
                    ));
                }
            }
            Err(e) => {
                tracing::warn!(
                    author_pubkey = %hex::encode(f.author_pubkey),
                    error = %e,
                    "fetching a followed publisher's posts failed - falling back to their cached posts"
                );
            }
        }
    }

    for f in &followed {
        match delegate.db.list_cached_remote_posts_by_author(&f.author_pubkey) {
            Ok(cached) => {
                for row in cached {
                    if seen.contains(&row.post_contract_id) {
                        continue;
                    }
                    items.push(cached_post_feed_item(&row, f.avatar_freenet_key.as_deref(), false));
                }
            }
            Err(e) => tracing::warn!(
                author_pubkey = %hex::encode(f.author_pubkey),
                error = %e,
                "reading cached posts for a followed publisher failed"
            ),
        }
    }
    items
}

fn sort_feed_items_desc(items: &mut [serde_json::Value]) {
    items.sort_by(|a, b| {
        let pa = a["published_at"].as_u64().unwrap_or(0);
        let pb = b["published_at"].as_u64().unwrap_or(0);
        pb.cmp(&pa)
    });
}

async fn handle_get_following_feed(delegate: &Delegate) -> Result<serde_json::Value> {
    let mut items = followed_feed_items(delegate).await;
    sort_feed_items_desc(&mut items);
    Ok(serde_json::json!(items))
}

/// Backs the Latest tab: the most recent posts from *every* publisher on
/// the network, via the shared `GlobalDirectoryContract` - see
/// `contracts::fetch_global_directory`'s module docs - merged with every
/// post this delegate has ever durably cached (`db.rs`'s `cached_remote_posts`
/// table). Own posts appear here too (unlocked, `is_own: true`) exactly like
/// anyone else's - this is a single global feed, not "everyone but me".
///
/// A live fetch failure (or the network's current copy of the shared
/// directory simply not including something it once did - this network is
/// sparse enough that content availability isn't guaranteed, see CLAUDE.md)
/// never empties this feed: it's logged and the durable cache is served on
/// its own, rather than propagating the error and showing nothing.
async fn handle_get_latest_feed(delegate: &Delegate) -> Result<serde_json::Value> {
    let self_pubkey = delegate.unlocked().keys.master_signing_verifying_bytes();
    // `GlobalDirectoryEntry` doesn't carry the author's avatar - it's a
    // network-wide contract every publisher's delegate appends a signed
    // entry to, and re-signing every past entry whenever an avatar changes
    // isn't something this contract supports (entries are immutable once
    // signed, see its module docs). So the avatar shown per-entry is
    // resolved locally instead: this delegate's own known avatar for its own
    // entries, or whatever profile snapshot this delegate has already cached
    // for anyone else (`cached_remote_profiles`, populated by
    // `handle_get_publisher_profile`) - `None` (renders as an initial in the
    // UI) for a publisher whose profile has never been viewed yet, same as
    // before this feature existed.
    let own_avatar_key = contracts::known_avatar_key(&delegate.db)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut items = Vec::new();
    let mut seen = HashSet::new();
    match contracts::fetch_global_directory(&delegate.unlocked().freenet).await {
        Ok(entries) => {
            for e in entries {
                if let Err(err) = delegate.db.cache_remote_post(
                    &e.post_id,
                    &e.author_pubkey,
                    &e.author_display_name,
                    &e.title,
                    &e.summary,
                    &e.post_contract_id,
                    e.published_at,
                    now,
                ) {
                    tracing::warn!(error = %err, "caching a global directory entry failed");
                }
                seen.insert(e.post_contract_id.clone());
                let is_own = e.author_pubkey == self_pubkey;
                let avatar_freenet_key = if is_own {
                    own_avatar_key.clone()
                } else {
                    delegate
                        .db
                        .get_cached_remote_profile(&e.author_pubkey)?
                        .and_then(|p| p.avatar_freenet_key)
                };
                items.push(feed_item_json(
                    e.post_id,
                    &e.title,
                    &e.summary,
                    e.published_at,
                    e.author_pubkey,
                    &e.author_display_name,
                    avatar_freenet_key.as_deref(),
                    is_own,
                    Some(e.post_contract_id),
                ));
            }
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                "fetching the global directory failed - falling back to cached posts"
            );
        }
    }

    match delegate.db.list_cached_remote_posts() {
        Ok(cached) => {
            for row in cached {
                if seen.contains(&row.post_contract_id) {
                    continue;
                }
                let is_own = row.author_pubkey == self_pubkey;
                let avatar_freenet_key = if is_own {
                    own_avatar_key.clone()
                } else {
                    delegate
                        .db
                        .get_cached_remote_profile(&row.author_pubkey)?
                        .and_then(|p| p.avatar_freenet_key)
                };
                items.push(cached_post_feed_item(&row, avatar_freenet_key.as_deref(), is_own));
            }
        }
        Err(err) => tracing::warn!(error = %err, "reading cached posts for the Latest feed failed"),
    }

    // The contract's own merge already sorts newest-first, but the network
    // may have returned a stale/partial copy - re-sort defensively rather
    // than trust that invariant blindly.
    sort_feed_items_desc(&mut items);
    Ok(serde_json::json!(items))
}

/// Backs viewing another publisher's profile page (with a Follow/Unfollow
/// button) - reached by clicking an author's name in any feed. Fetches and
/// *verifies* their real `PublisherProfileContract` (see
/// `contracts::fetch_remote_profile`'s module docs) rather than trusting
/// anything client-supplied, plus their recent posts for display.
///
/// A live fetch failure falls back to the durably-cached copy of this
/// profile (`db.rs`'s `cached_remote_profiles`) if one exists - only a
/// pubkey this delegate has *never* successfully seen a verified profile for
/// actually errors out. Posts follow the same live-plus-cache merge as
/// `handle_get_latest_feed`.
async fn handle_get_publisher_profile(
    delegate: &Delegate,
    author_pubkey_hex: &str,
) -> Result<serde_json::Value> {
    let author_pubkey: [u8; 32] = hex::decode_array(author_pubkey_hex)?;
    let self_pubkey = delegate.unlocked().keys.master_signing_verifying_bytes();
    let is_own = author_pubkey == self_pubkey;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let live_profile = match contracts::fetch_remote_profile(&delegate.unlocked().freenet, author_pubkey).await {
        Ok(profile) => profile,
        Err(e) => {
            tracing::warn!(error = %e, "fetching remote profile failed - falling back to cache if available");
            None
        }
    };
    let (display_name, bio, avatar_freenet_key) = if let Some(profile) = &live_profile {
        if let Err(e) = delegate.db.cache_remote_profile(
            &author_pubkey,
            &profile.display_name,
            &profile.bio,
            profile.avatar_freenet_key.as_deref(),
            now,
        ) {
            tracing::warn!(error = %e, "caching remote profile failed");
        }
        (
            profile.display_name.clone(),
            profile.bio.clone(),
            profile.avatar_freenet_key.clone(),
        )
    } else if let Some(cached) = delegate.db.get_cached_remote_profile(&author_pubkey)? {
        (cached.display_name, cached.bio, cached.avatar_freenet_key)
    } else {
        anyhow::bail!("no publisher profile found for that pubkey");
    };
    let display_name = if display_name.trim().is_empty() {
        "Untitled Publication".to_string()
    } else {
        display_name
    };
    let is_following = delegate
        .db
        .list_followed_publishers()?
        .iter()
        .any(|f| f.author_pubkey == author_pubkey);

    let mut post_items = Vec::new();
    let mut seen = HashSet::new();
    match contracts::fetch_remote_posts(&delegate.unlocked().freenet, author_pubkey).await {
        Ok(posts) => {
            for header in posts {
                if let Err(e) = delegate.db.cache_remote_post(
                    &header.post_id,
                    &author_pubkey,
                    &display_name,
                    &header.title,
                    &header.summary,
                    &header.post_contract_id,
                    header.published_at,
                    now,
                ) {
                    tracing::warn!(error = %e, "caching a publisher's post failed");
                }
                seen.insert(header.post_contract_id.clone());
                post_items.push(feed_item_json(
                    header.post_id,
                    &header.title,
                    &header.summary,
                    header.published_at,
                    author_pubkey,
                    &display_name,
                    avatar_freenet_key.as_deref(),
                    is_own,
                    Some(header.post_contract_id),
                ));
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "fetching this publisher's posts failed - falling back to cache");
        }
    }
    match delegate.db.list_cached_remote_posts_by_author(&author_pubkey) {
        Ok(cached) => {
            for row in cached {
                if seen.contains(&row.post_contract_id) {
                    continue;
                }
                post_items.push(cached_post_feed_item(&row, avatar_freenet_key.as_deref(), is_own));
            }
        }
        Err(e) => tracing::warn!(error = %e, "reading cached posts for a publisher's profile failed"),
    }
    sort_feed_items_desc(&mut post_items);

    Ok(serde_json::json!({
        "author_pubkey": author_pubkey_hex,
        "display_name": display_name,
        "bio": bio,
        "avatar_freenet_key": avatar_freenet_key,
        "is_own": is_own,
        "is_following": is_following,
        "posts": post_items,
    }))
}

/// Reader-role: opens a post from another publisher by its
/// `PostDataContract` id.
///
/// Once a post's markdown has been recovered here, it's cached durably
/// (`db.rs`'s `cached_post_payloads`) - the whole point of this feature (see
/// CLAUDE.md's positioning notes): once you've actually opened something,
/// it's yours to keep reading regardless of whether the network can still
/// produce it later. A live fetch failure falls back to that cached copy
/// rather than erroring if one exists.
async fn handle_get_remote_post(
    delegate: &Delegate,
    post_contract_id: &str,
) -> Result<serde_json::Value> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match contracts::fetch_remote_post_payload(&delegate.unlocked().freenet, post_contract_id).await {
        Ok(payload) => {
            let markdown = String::from_utf8(payload.content)
                .context("decoding remote post payload as UTF-8 markdown")?;
            if let Err(e) = delegate.db.cache_post_payload(post_contract_id, &markdown, now) {
                tracing::warn!(error = %e, "caching a remote post's content failed");
            }
            Ok(serde_json::json!({
                "post_contract_id": post_contract_id,
                "markdown": markdown,
            }))
        }
        Err(e) => {
            if let Some(markdown) = delegate.db.get_cached_post_payload(post_contract_id)? {
                tracing::warn!(error = %e, "fetching remote post live failed - serving cached copy");
                return Ok(serde_json::json!({
                    "post_contract_id": post_contract_id,
                    "markdown": markdown,
                }));
            }
            Err(e)
        }
    }
}

/// Fetches an avatar image (own or another publisher's) by its
/// `PostDataContract` id and returns it as a ready-to-render `data:` URL.
///
/// Avatars are published `Public` (see `contracts::publish_avatar_to_network`'s
/// module docs), so - unlike `handle_get_remote_post` - there's no access
/// check here: anyone who knows the key can fetch it, same as the network
/// itself allows. The payload is raw image bytes with no mime metadata (the
/// upload path never sent one over the network, only kept it in this
/// delegate's own local `profile` row), so the mime type is recovered by
/// sniffing the image's own magic bytes instead - reliable for the common
/// formats a `<input type="file" accept="image/*">` upload actually produces,
/// and a fixed set is easier to keep correct than plumbing a new metadata
/// field through the avatar's `PostDataContract` payload for this alone.
///
/// A live fetch failure falls back to `db.rs`'s `cached_avatars` (same "seen
/// once, yours to keep" durability as `handle_get_remote_post`'s markdown
/// cache) rather than erroring if a previous fetch already succeeded.
async fn handle_get_remote_avatar(
    delegate: &Delegate,
    avatar_freenet_key: &str,
) -> Result<serde_json::Value> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match contracts::fetch_remote_post_payload(&delegate.unlocked().freenet, avatar_freenet_key).await {
        Ok(payload) => {
            let mime = sniff_image_mime(&payload.content);
            if let Err(e) = delegate.db.cache_avatar(avatar_freenet_key, mime, &payload.content, now) {
                tracing::warn!(error = %e, "caching a remote avatar failed");
            }
            Ok(serde_json::json!({
                "avatar_freenet_key": avatar_freenet_key,
                "avatar_data_url": encode_data_url(mime, &payload.content),
            }))
        }
        Err(e) => {
            if let Some((mime, bytes)) = delegate.db.get_cached_avatar(avatar_freenet_key)? {
                tracing::warn!(error = %e, "fetching remote avatar live failed - serving cached copy");
                return Ok(serde_json::json!({
                    "avatar_freenet_key": avatar_freenet_key,
                    "avatar_data_url": encode_data_url(&mime, &bytes),
                }));
            }
            Err(e)
        }
    }
}

/// Identifies an image's format from its own leading magic bytes rather than
/// trusting a filename/extension nobody supplied over the network - the
/// formats a browser `<input type="file" accept="image/*">` upload actually
/// produces in practice. Falls back to PNG (arbitrary but harmless: browsers
/// render an `<img>` from its real bytes regardless of a mismatched `data:`
/// mime prefix) rather than failing outright for a format outside this list.
fn sniff_image_mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "image/png"
    }
}

/// Minimal data-URL codec (`data:<mime>;base64,<payload>`) for the profile
/// avatar image over IPC - the UI reads/writes a `<input type="file">` as a
/// data URL, so this avoids inventing a second wire representation just for
/// this one field.
fn decode_data_url(data_url: &str) -> Result<(String, Vec<u8>)> {
    let rest = data_url
        .strip_prefix("data:")
        .ok_or_else(|| anyhow::anyhow!("avatar_data_url must start with \"data:\""))?;
    let (meta, payload) = rest
        .split_once(',')
        .ok_or_else(|| anyhow::anyhow!("avatar_data_url missing ',' separator"))?;
    let mime = meta
        .strip_suffix(";base64")
        .ok_or_else(|| anyhow::anyhow!("avatar_data_url must be base64-encoded"))?
        .to_string();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .context("decoding avatar image base64")?;
    Ok((mime, bytes))
}

fn encode_data_url(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// Minimal hex helpers so the delegate doesn't need a full `hex` crate
/// dependency for this small amount of encode/decode.
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn decode_array<const N: usize>(s: &str) -> anyhow::Result<[u8; N]> {
        anyhow::ensure!(s.len() == N * 2, "expected {} hex chars, got {}", N * 2, s.len());
        let mut out = [0u8; N];
        for i in 0..N {
            out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)?;
        }
        Ok(out)
    }
}
