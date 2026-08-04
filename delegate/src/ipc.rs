//! Local IPC server: the React/Tauri UI talks to the Delegate over a
//! loopback-only WebSocket so key material and ciphertext never cross a
//! process boundary the UI can inspect directly.
//!
//! Protocol: newline-agnostic JSON request/response pairs correlated by
//! `id`. This is a Phase 2 prototype covering only the publisher's own
//! publish -> feed -> read loop entirely against the local SQLite cache;
//! there is no Freenet broadcast or subscriber decryption yet (see
//! `freenet_bridge.rs` and `nwc.rs`).
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
    crypto,
    db::LocalStore,
    freenet_bridge::FreenetBridge,
    keys::DelegateKeys,
    nwc::NwcClient,
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
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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
        /// "public" or "subscriber".
        access: String,
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
    /// Connect this delegate's Lightning wallet via Nostr Wallet Connect
    /// (NIP-47) - a `nostr+walletconnect://...` URI exported from a wallet
    /// such as Alby, Mutiny, Phoenix, or Umbrel. One wallet connection
    /// serves both roles `Subscribe` needs (see `nwc.rs`'s module docs):
    /// receiving payment (as a publisher) and paying (as a reader).
    ConnectWallet {
        uri: String,
    },
    /// `author_pubkey`'s subscription tiers, this delegate's own
    /// subscriber-role pubkey (what a bundle addressed to "you" would be
    /// keyed on), whether a wallet is currently connected, and whether
    /// `author_pubkey` is actually subscribable right now - everything
    /// `Subscriptions.tsx` needs to render before a reader clicks Subscribe.
    /// Omitted (or equal to this delegate's own pubkey) means "myself" -
    /// always `subscribable: true` and unchanged from before this field
    /// existed. Any other target is real (profile-fetched where possible)
    /// but always `subscribable: false` - see `Subscribe`'s docs for why.
    GetSubscriptionInfo {
        #[serde(default)]
        author_pubkey: Option<String>,
    },
    /// Reader-role action: pay for `tier_id` via the connected wallet, then
    /// (publisher-role, same delegate - see this milestone's single-identity
    /// note in CLAUDE.md) verify settlement, derive the ECDH shared secret,
    /// and append a fresh `EncryptedKeyBundle` to the `SubscriberRegistryContract`.
    /// `author_pubkey` omitted (or equal to this delegate's own pubkey) is
    /// the only target that actually works today - anything else fails
    /// immediately with a clear error, since there's no channel yet for a
    /// reader to learn a stranger's secp256k1 identity key (needed for the
    /// ECDH exchange) - see CLAUDE.md's "Known stub" section. Deliberately a
    /// real, explicit `Err` here rather than a silently-ignored or
    /// fake-succeeding target, matching this codebase's honest-gap
    /// convention.
    Subscribe {
        tier_id: u8,
        #[serde(default)]
        author_pubkey: Option<String>,
    },
    /// Publisher-role view: subscriber grants this delegate has issued,
    /// from the local bookkeeping table (`LocalStore::record_subscriber`) -
    /// not a live network re-fetch of the registry contract.
    ListSubscribers,
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
    /// Fetches and decodes a `Public` post from *another* publisher by its
    /// `PostDataContract` id. Refuses (rather than silently failing to
    /// decrypt) if the fetched payload turns out to be `SubscriberOnly` -
    /// see `contracts::fetch_remote_post_payload`'s module docs on why that
    /// gap is deliberate for this milestone.
    GetRemotePost {
        post_contract_id: String,
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
    nwc: NwcClient,
    identity: PublisherIdentity,
    /// Optional 2% platform fee wallet (design doc §6.3) - disconnected
    /// unless `AETHERIA_PLATFORM_FEE_NWC` was set at startup (see
    /// `connect_platform_fee_wallet`). `handle_subscribe` checks
    /// `is_connected()` before doing anything with it.
    platform_fee: NwcClient,
}

struct Delegate {
    db: LocalStore,
    identity_key_path: PathBuf,
    unlocked: Option<Unlocked>,
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

    fn unlocked_mut(&mut self) -> &mut Unlocked {
        self.unlocked
            .as_mut()
            .expect("Delegate::unlocked_mut() called while locked - dispatch gate should have refused this")
    }
}

pub async fn serve(port: u16, db: LocalStore, identity_key_path: PathBuf) -> Result<()> {
    let delegate = Arc::new(Mutex::new(Delegate {
        db,
        identity_key_path: identity_key_path.clone(),
        unlocked: None,
    }));

    tokio::spawn(try_legacy_auto_unlock(delegate.clone(), identity_key_path));

    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "delegate IPC listening (loopback only)");

    while let Ok((stream, peer)) = listener.accept().await {
        tracing::debug!(%peer, "UI connection accepted");
        let delegate = delegate.clone();
        tokio::spawn(async move {
            let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
                return;
            };
            let (mut write, mut read) = ws.split();
            while let Some(Ok(msg)) = read.next().await {
                let Message::Text(text) = msg else { continue };
                let reply = handle_message(&delegate, &text).await;
                if write.send(Message::Text(reply.into())).await.is_err() {
                    break;
                }
            }
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
    let nwc = NwcClient::disconnected();
    let platform_fee = connect_platform_fee_wallet().await;
    delegate.unlocked = Some(Unlocked {
        keys,
        freenet,
        nwc,
        identity,
        platform_fee,
    });
    Ok(())
}

/// Optional 2% platform fee (design doc §6.3's "Optional App Split"): if
/// `AETHERIA_PLATFORM_FEE_NWC` is set to a real `nostr+walletconnect://...`
/// URI, `handle_subscribe` requests a small fee invoice from this wallet
/// alongside the main subscription payment, best-effort. Unset by default -
/// a fork of this app run by someone else shouldn't silently try to pay a
/// stranger's wallet. **Never hardcode a real connection string here or
/// anywhere else in this repo** - it's a real secret (scoped receive-only:
/// make_invoice/lookup_invoice/get_info/get_balance, no pay_invoice, so even
/// a leaked string can't be used to spend funds, but it's still not
/// something to commit to git history). Moved here from `main.rs` as part of
/// the locked/unlocked startup split - this now runs once per successful
/// unlock rather than once at process start.
async fn connect_platform_fee_wallet() -> NwcClient {
    let mut client = NwcClient::disconnected();
    match std::env::var("AETHERIA_PLATFORM_FEE_NWC") {
        Ok(uri) if !uri.trim().is_empty() => match client.connect(&uri).await {
            Ok(()) => {
                tracing::info!("platform fee wallet connected - 2% subscription split enabled");
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "AETHERIA_PLATFORM_FEE_NWC is set but failed to connect - platform fee split disabled this run"
                );
            }
        },
        _ => {}
    }
    client
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
/// is `Some` (see `Delegate::unlocked()`/`unlocked_mut()`).
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
                    access,
                } => handle_publish_post(delegate, &title, &summary, &markdown, &access).await,
                Request::GetProfile => handle_get_profile(delegate),
                Request::UpdateProfile {
                    display_name,
                    bio,
                    avatar_data_url,
                } => {
                    handle_update_profile(delegate, &display_name, &bio, avatar_data_url.as_deref())
                        .await
                }
                Request::ConnectWallet { uri } => handle_connect_wallet(delegate, &uri).await,
                Request::GetSubscriptionInfo { author_pubkey } => {
                    handle_get_subscription_info(delegate, author_pubkey.as_deref())
                }
                Request::Subscribe { tier_id, author_pubkey } => {
                    handle_subscribe(delegate, tier_id, author_pubkey.as_deref()).await
                }
                Request::ListSubscribers => handle_list_subscribers(delegate),
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
                "access_level": p.access_level,
                "epoch_id": p.epoch_id,
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

    let markdown = match row.access_level.as_str() {
        "public" => row
            .markdown_plain
            .ok_or_else(|| anyhow::anyhow!("public post missing plaintext"))?,
        "subscriber" => {
            let key = delegate
                .db
                .get_epoch_key(row.epoch_id)?
                .ok_or_else(|| anyhow::anyhow!("epoch key not available locally"))?;
            let cipher_text = row
                .cipher_text
                .ok_or_else(|| anyhow::anyhow!("subscriber post missing ciphertext"))?;
            let nonce: [u8; 12] = row
                .nonce
                .ok_or_else(|| anyhow::anyhow!("subscriber post missing nonce"))?
                .try_into()
                .map_err(|_| anyhow::anyhow!("corrupt nonce length"))?;
            let bytes = crypto::decrypt_payload(&key, &nonce, &cipher_text)?;
            String::from_utf8(bytes)?
        }
        other => anyhow::bail!("unknown access_level {other}"),
    };

    Ok(serde_json::json!({
        "post_id": post_id_hex,
        "title": row.title,
        "markdown": markdown,
        "network_synced": row.post_contract_id.is_some(),
        "post_contract_id": row.post_contract_id,
    }))
}

async fn handle_publish_post(
    delegate: &Delegate,
    title: &str,
    summary: &str,
    markdown: &str,
    access: &str,
) -> Result<serde_json::Value> {
    anyhow::ensure!(
        access == "public" || access == "subscriber",
        "access must be \"public\" or \"subscriber\""
    );

    let mut post_id = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut post_id);
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let epoch_id = current_epoch_id(now);

    // Local SQLite write stays exactly as it was before Freenet was wired
    // up - it's the fast local cache `list_posts`/`get_post` read from, and
    // must keep working even though publishing now also reaches the
    // network. `access_tier`/`network_cipher_text`/`network_nonce` are the
    // extra pieces the network side needs alongside what's already stored.
    let (access_tier, network_cipher_text, network_nonce) = if access == "public" {
        delegate.db.insert_post(
            &post_id,
            title,
            summary,
            access,
            epoch_id,
            now,
            Some(markdown),
            None,
            None,
        )?;
        // Matches PostDataContract's own convention for public posts: plain
        // bytes in `cipher_text`, all-zero nonce (see its module docs).
        (aetheria_types::AccessTier::Public, markdown.as_bytes().to_vec(), [0u8; 12])
    } else {
        let key = delegate
            .db
            .get_or_create_epoch_key(epoch_id, crypto::generate_epoch_key, now)?;
        let encrypted = crypto::encrypt_payload(&key, markdown.as_bytes())?;
        delegate.db.insert_post(
            &post_id,
            title,
            summary,
            access,
            epoch_id,
            now,
            None,
            Some(&encrypted.cipher_text),
            Some(&encrypted.nonce),
        )?;
        // TODO(Phase 3): real tier selection once the UI exposes more than
        // one subscription tier (design doc §3.1) - defaults to tier 0.
        (
            aetheria_types::AccessTier::SubscriberOnly { required_tier_id: 0 },
            encrypted.cipher_text,
            encrypted.nonce,
        )
    };

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
        access_tier.clone(),
        epoch_id,
        now,
        network_cipher_text,
        network_nonce,
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
            access_tier,
            epoch_id,
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
    match delegate.db.get_profile()? {
        Some(p) => Ok(serde_json::json!({
            "display_name": p.display_name,
            "bio": p.bio,
            "avatar_data_url": match (&p.avatar_bytes, &p.avatar_mime) {
                (Some(bytes), Some(mime)) => Some(encode_data_url(mime, bytes)),
                _ => None,
            },
            "avatar_freenet_key": avatar_freenet_key,
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

/// Single hardcoded subscription tier, per this task's scope note: real
/// multi-tier configuration in Settings is a TODO, not required for this
/// milestone - see CLAUDE.md's "Known stub" section.
fn default_tiers() -> Vec<aetheria_types::Tier> {
    vec![aetheria_types::Tier {
        tier_id: 0,
        name: "Supporter".to_string(),
        price_sats_per_month: 5_000,
        features: vec!["Full access to subscriber-only posts".to_string()],
    }]
}

async fn handle_connect_wallet(delegate: &mut Delegate, uri: &str) -> Result<serde_json::Value> {
    delegate.unlocked_mut().nwc.connect(uri).await?;
    Ok(serde_json::json!({ "connected": true }))
}

/// `author_pubkey` omitted, or equal to this delegate's own pubkey, means
/// "myself" - unchanged from before `Subscriptions.tsx` existed. Any other
/// target reports `subscribable: false` (see `handle_subscribe`'s docs for
/// why) and an empty tier list - real tier data is never published by any
/// publisher yet regardless of target (`ensure_publisher_identity` always
/// publishes `subscription_tiers: vec![]`, see contracts.rs), so there's
/// nothing honest to show there either way.
fn handle_get_subscription_info(
    delegate: &Delegate,
    author_pubkey: Option<&str>,
) -> Result<serde_json::Value> {
    let self_pubkey = delegate.unlocked().keys.master_signing_verifying_bytes();
    let target: [u8; 32] = match author_pubkey {
        Some(pubkey_hex) => hex::decode_array(pubkey_hex)?,
        None => self_pubkey,
    };
    let is_self = target == self_pubkey;
    Ok(serde_json::json!({
        "publisher_pubkey": hex::encode(target),
        "subscriber_pubkey": hex::encode(delegate.unlocked().keys.identity_public_compressed()),
        "tiers": if is_self { default_tiers() } else { Vec::new() },
        "wallet_connected": delegate.unlocked().nwc.is_connected(),
        "subscribable": is_self,
    }))
}

/// Reader-role action (design doc §5.2, Workflow B) - pays for `tier_id` via
/// the connected wallet, then (publisher-role - see this milestone's
/// single-identity note in CLAUDE.md, both roles are the same delegate for
/// now) verifies settlement and appends a fresh `EncryptedKeyBundle`.
///
/// Local-first/network-best-effort still applies to the *registry publish*
/// step (`contracts::publish_key_bundle_to_network`) - the epoch key and the
/// local subscriber-grant record are committed unconditionally once payment
/// is verified, exactly like `handle_publish_post`'s post row and
/// `handle_update_profile`'s profile row. It does *not* apply to the
/// Lightning payment itself: `make_invoice`/`pay_invoice`/`wait_for_preimage`
/// are real money movement (once a real wallet is connected), so a failure
/// there fails the whole call rather than silently granting free access.
///
/// `author_pubkey` omitted, or equal to this delegate's own pubkey, is the
/// only target this actually supports - subscribing to anyone else fails
/// immediately, clearly, and with no side effects, because there is
/// genuinely no channel yet for a reader to learn a stranger's secp256k1
/// identity key (the ECDH exchange needs it, and `PublisherProfileContract`
/// only ever carries the unrelated Ed25519 `author_pubkey`). The design
/// doc's own plan for this (an encrypted peer-to-peer message between
/// delegates, §5.2 step 2) was never built - see CLAUDE.md's "Known stub"
/// section. Real `Err`, not a silent no-op or fake success, matching this
/// codebase's honest-gap convention elsewhere (`FreenetBridge::subscribe`'s
/// `todo!()`, etc.).
async fn handle_subscribe(
    delegate: &Delegate,
    tier_id: u8,
    author_pubkey: Option<&str>,
) -> Result<serde_json::Value> {
    if let Some(pubkey_hex) = author_pubkey {
        let target: [u8; 32] = hex::decode_array(pubkey_hex)?;
        anyhow::ensure!(
            target == delegate.unlocked().keys.master_signing_verifying_bytes(),
            "subscribing to another publisher isn't supported yet - there's no way for your \
             delegate to securely learn their encryption key (see CLAUDE.md's Known stub section)"
        );
    }

    let tier = default_tiers()
        .into_iter()
        .find(|t| t.tier_id == tier_id)
        .ok_or_else(|| anyhow::anyhow!("unknown tier_id {tier_id}"))?;
    anyhow::ensure!(
        delegate.unlocked().nwc.is_connected(),
        "connect a Lightning wallet first (Nostr Wallet Connect)"
    );

    let amount_msat = tier.price_sats_per_month.saturating_mul(1000);
    let description = format!(
        "Aetheria Subscription: tier {} ({})",
        tier.tier_id, tier.name
    );

    // Publisher role: mint an invoice against the connected wallet.
    let invoice = delegate
        .unlocked()
        .nwc
        .make_invoice(amount_msat, &description)
        .await
        .context("requesting a Lightning invoice via NWC")?;

    // Reader role: pay it, via the *same* connected wallet in this
    // milestone's single-identity architecture (see nwc.rs's module docs -
    // a real deployment has two different people each connecting their own
    // wallet; nothing here assumes they're the same wallet, it just happens
    // to be true today because there's only one identity to test with).
    let claimed_preimage = delegate
        .unlocked()
        .nwc
        .pay_invoice(&invoice.bolt11)
        .await
        .context("paying the Lightning invoice via NWC")?;

    // Publisher role again: verify settlement independently (design doc
    // §5.2 step 5) rather than trusting the payer's own claim.
    let confirmed_preimage = delegate
        .unlocked()
        .nwc
        .wait_for_preimage(&invoice.payment_hash, Duration::from_secs(30), Duration::from_secs(2))
        .await
        .context("verifying invoice settlement via NIP-47 lookup_invoice")?;
    anyhow::ensure!(
        confirmed_preimage == claimed_preimage,
        "preimage mismatch between pay_invoice's response and lookup_invoice's - refusing to \
         grant access"
    );

    // Optional 2% platform fee (design doc §6.3), best-effort: the reader's
    // subscription is already paid for and verified above, so a hiccup
    // collecting this small secondary fee must never block or reverse the
    // access grant that follows - same "local decision is real regardless
    // of what a network/secondary side-effect does" philosophy as
    // everywhere else in this file. Skipped entirely if no platform fee
    // wallet is configured (see `main.rs::connect_platform_fee_wallet`).
    let (platform_fee_synced, platform_fee_error) = if delegate.unlocked().platform_fee.is_connected() {
        const PLATFORM_FEE_BASIS_POINTS: u64 = 200; // 2.00%
        let fee_amount_msat = amount_msat.saturating_mul(PLATFORM_FEE_BASIS_POINTS) / 10_000;
        if fee_amount_msat == 0 {
            (false, Some("tier price too small to produce a nonzero fee".to_string()))
        } else {
            match collect_platform_fee(delegate, fee_amount_msat, tier.tier_id).await {
                Ok(()) => (true, None),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "platform fee collection failed - subscriber access is unaffected"
                    );
                    (false, Some(e.to_string()))
                }
            }
        }
    } else {
        (false, None)
    };

    // ECDH key delivery (crypto.rs, design doc §4.2).
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let epoch_id = current_epoch_id(now);
    let epoch_key = delegate
        .db
        .get_or_create_epoch_key(epoch_id, crypto::generate_epoch_key, now)?;

    let subscriber_pubkey = delegate.unlocked().keys.identity_public_compressed();
    let subscriber_public = k256::PublicKey::from_sec1_bytes(&subscriber_pubkey)
        .context("decoding own compressed secp256k1 pubkey")?;
    let shared_secret =
        crypto::derive_shared_secret(&delegate.unlocked().keys.identity_secret, &subscriber_public);
    let wrapped = crypto::wrap_epoch_key(&shared_secret, &epoch_key)?;

    let bundle = aetheria_types::EncryptedKeyBundle {
        subscriber_pubkey,
        epoch_id,
        cipher_text: wrapped.cipher_text,
        nonce: wrapped.nonce,
        auth_tag: [0u8; 16],
        issued_at: now,
    };

    // Local write commits unconditionally, same philosophy as every other
    // handler in this file - the delegate already decided to grant access
    // (payment verified above), that's real regardless of what the network
    // publish below does.
    delegate
        .db
        .record_subscriber(&bundle.subscriber_pubkey, epoch_id, now)?;

    let (registry_synced, network_error) =
        match contracts::publish_key_bundle_to_network(
            &delegate.unlocked().freenet,
            &delegate.db,
            &delegate.unlocked().keys,
            bundle,
        )
        .await
        {
            Ok(_key) => (true, None),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "SubscriberRegistryContract publish failed after retries; access granted \
                     locally only, not yet synced to the network"
                );
                (false, Some(e.to_string()))
            }
        };

    Ok(serde_json::json!({
        "tier_id": tier_id,
        "epoch_id": epoch_id,
        "preimage": confirmed_preimage,
        "network_synced": registry_synced,
        "network_error": network_error,
        "platform_fee_synced": platform_fee_synced,
        "platform_fee_error": platform_fee_error,
    }))
}

/// Requests a `fee_amount_msat` invoice from the platform fee wallet and
/// pays it via the reader's own connected wallet (`delegate.unlocked().nwc` - same
/// dual-role-one-wallet caveat as the main subscription payment in this
/// milestone's single-identity architecture, see `nwc.rs`'s module docs).
/// No settlement re-verification via `lookup_invoice` here, unlike the main
/// payment above - this is a best-effort side collection, not something
/// worth gating subscriber access on either way.
async fn collect_platform_fee(delegate: &Delegate, fee_amount_msat: u64, tier_id: u8) -> Result<()> {
    let description = format!("Aetheria platform fee: tier {tier_id} subscription");
    let invoice = delegate
        .unlocked()
        .platform_fee
        .make_invoice(fee_amount_msat, &description)
        .await
        .context("requesting platform fee invoice via NWC")?;
    delegate
        .unlocked()
        .nwc
        .pay_invoice(&invoice.bolt11)
        .await
        .context("paying platform fee invoice via NWC")?;
    Ok(())
}

fn handle_list_subscribers(delegate: &Delegate) -> Result<serde_json::Value> {
    let rows = delegate.db.list_subscribers()?;
    let json: Vec<_> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "subscriber_pubkey": hex::encode(&r.subscriber_pubkey),
                "epoch_id": r.epoch_id,
                "issued_at": r.issued_at,
            })
        })
        .collect();
    Ok(serde_json::json!(json))
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
/// `handle_get_publisher_profile`) so the `locked` rule - a `SubscriberOnly`
/// post is only ever unlocked when `is_own` - stays in one place. A
/// publisher can always read their own posts (see `handle_get_post`),
/// regardless of which feed they're appearing in.
#[allow(clippy::too_many_arguments)]
fn feed_item_json(
    post_id: [u8; 16],
    title: &str,
    summary: &str,
    access_level: &aetheria_types::AccessTier,
    epoch_id: u32,
    published_at: u64,
    author_pubkey: [u8; 32],
    author_display_name: &str,
    is_own: bool,
    post_contract_id: Option<String>,
) -> serde_json::Value {
    let (access_level_str, locked) = match access_level {
        aetheria_types::AccessTier::Public => ("public", false),
        aetheria_types::AccessTier::SubscriberOnly { .. } => ("subscriber", !is_own),
    };
    serde_json::json!({
        "post_id": hex::encode(post_id),
        "title": title,
        "summary": summary,
        "access_level": access_level_str,
        "epoch_id": epoch_id,
        "published_at": published_at,
        "author_pubkey": hex::encode(author_pubkey),
        "author_display_name": author_display_name,
        "is_own": is_own,
        "locked": locked,
        "post_contract_id": post_contract_id,
    })
}

/// Renders a durably-cached post header (`db::CachedRemotePost`) in the same
/// shape `feed_item_json` produces for a live one - the two are
/// interchangeable to the UI by design, since the whole point of the cache
/// is that "seen once" and "seen just now" should look identical once
/// something's in a feed.
fn cached_post_feed_item(row: &crate::db::CachedRemotePost, is_own: bool) -> serde_json::Value {
    let locked = row.access_level == "subscriber" && !is_own;
    serde_json::json!({
        "post_id": hex::encode(row.post_id),
        "title": row.title,
        "summary": row.summary,
        "access_level": row.access_level,
        "epoch_id": row.epoch_id,
        "published_at": row.published_at,
        "author_pubkey": hex::encode(row.author_pubkey),
        "author_display_name": row.author_display_name,
        "is_own": is_own,
        "locked": locked,
        "post_contract_id": row.post_contract_id,
    })
}

fn access_level_str(access_level: &aetheria_types::AccessTier) -> &'static str {
    match access_level {
        aetheria_types::AccessTier::Public => "public",
        aetheria_types::AccessTier::SubscriberOnly { .. } => "subscriber",
    }
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
                        access_level_str(&header.access_level),
                        header.epoch_id,
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
                        &header.access_level,
                        header.epoch_id,
                        header.published_at,
                        f.author_pubkey,
                        &f.display_name,
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
                    items.push(cached_post_feed_item(&row, false));
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
                    access_level_str(&e.access_level),
                    e.epoch_id,
                    e.published_at,
                    now,
                ) {
                    tracing::warn!(error = %err, "caching a global directory entry failed");
                }
                seen.insert(e.post_contract_id.clone());
                let is_own = e.author_pubkey == self_pubkey;
                items.push(feed_item_json(
                    e.post_id,
                    &e.title,
                    &e.summary,
                    &e.access_level,
                    e.epoch_id,
                    e.published_at,
                    e.author_pubkey,
                    &e.author_display_name,
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
                items.push(cached_post_feed_item(&row, is_own));
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
                    access_level_str(&header.access_level),
                    header.epoch_id,
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
                    &header.access_level,
                    header.epoch_id,
                    header.published_at,
                    author_pubkey,
                    &display_name,
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
                post_items.push(cached_post_feed_item(&row, is_own));
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

/// Reader-role: opens a `Public` post from another publisher by its
/// `PostDataContract` id. The feed already tells the UI which posts are
/// `locked` (see `followed_feed_items`), but this re-checks the fetched
/// payload's nonce independently rather than trusting the caller's claim -
/// a `SubscriberOnly` payload's nonce is genuine random AES-256-GCM output,
/// never all-zero (see `publish_post_to_network`'s convention), so this is a
/// real distinguishing check, not a formality.
///
/// Once a post's plaintext markdown has been recovered here, it's cached
/// durably (`db.rs`'s `cached_post_payloads`) - the whole point of this
/// feature (see CLAUDE.md's positioning notes): once you've actually opened
/// something, it's yours to keep reading regardless of whether the network
/// can still produce it later. A live fetch failure falls back to that
/// cached copy rather than erroring if one exists.
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
            anyhow::ensure!(
                payload.nonce == [0u8; 12],
                "this post is subscriber-only content from another publisher and can't be opened yet - \
                 there's no mechanism yet for a reader to learn a stranger's ECDH key (see CLAUDE.md's \
                 Known stub section)"
            );
            let markdown = String::from_utf8(payload.cipher_text)
                .context("decoding remote public post payload as UTF-8 markdown")?;
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

/// Bucket the current time into a coarse ~30-day "billing epoch".
///
/// TODO(Phase 3): replace with a real calendar-month epoch once the
/// subscription renewal scheduler (design doc §6.2) is implemented.
fn current_epoch_id(now_unix_secs: u64) -> u32 {
    const THIRTY_DAYS_SECS: u64 = 30 * 24 * 60 * 60;
    (now_unix_secs / THIRTY_DAYS_SECS) as u32
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
