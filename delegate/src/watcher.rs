//! Live "someone you follow just published" watcher - the delegate half of
//! Aetheria's desktop notifications.
//!
//! ## Why this exists
//!
//! Until this module, a reader only ever learned about a new post by having
//! the app open and hitting Refresh: every feed in `ipc.rs` is a
//! request/response pull. There was no push of any kind. This is the piece
//! that makes "your subscribers get told when you publish" true rather than
//! aspirational, and it does it **without any server**: no notification
//! service, no relay, no mailer, nothing new that has to be online for the
//! feature to work. The only moving parts are the user's own local delegate
//! and the Freenet node it already talks to.
//!
//! ## How
//!
//! `FreenetBridge::subscribe` (real as of this module - it was a `todo!()`
//! before, see CLAUDE.md's "Known stub" section) asks the local node to push
//! every change to a contract back down the client connection as a
//! `ContractResponse::UpdateNotification`. For each publisher this delegate
//! follows, that contract is their `ContentIndexContract` - the same key
//! `contracts::fetch_remote_posts` GETs, derived locally from their
//! `author_pubkey` with no discovery call (see `contracts.rs`'s module docs
//! on `ContractKey::from_params_and_code`). A publisher folding a new
//! `PostMetadataHeader` into their index (`ipc.rs::handle_publish_post`) is
//! precisely the event a follower wants to hear about, so their index is the
//! right thing to watch: the network-wide `GlobalDirectoryContract` would
//! also see the post, but it sees *everyone's*, and toasting for the whole
//! network is spam, not a flare gun.
//!
//! ## Dedicated connection
//!
//! This task opens its **own** `FreenetBridge` rather than sharing the one in
//! `ipc.rs`'s `Unlocked`. Two reasons, both structural rather than stylistic:
//!
//! 1. Every method on the shared bridge is a request/response round trip that
//!    logs-and-skips anything else that arrives mid-flight. A push landing
//!    during someone's GET would be discarded by that GET's own loop.
//! 2. Waiting for a push means holding the connection's mutex indefinitely,
//!    which would block every IPC request that needs the network.
//!
//! Two client connections to the same local node is a normal thing to do -
//! `fdev` opens its own alongside whatever else is running.
//!
//! ## Not trusting the push
//!
//! A pushed `ContentIndexState` gets exactly the same treatment as a fetched
//! one: every `PostMetadataHeader` in it is Ed25519-verified against the
//! publisher's own `author_pubkey` (`contracts::decode_verified_content_index`)
//! and silently dropped if it doesn't check out. Nothing about a notification
//! arriving unsolicited makes its contents more trustworthy.
//!
//! ## Two ways a new post is noticed, one way it is announced
//!
//! The real gateway network is documented throughout CLAUDE.md as flaky, and
//! a subscription is exactly the kind of thing it can quietly drop (a node
//! restart, a peer disconnect, a subscription the node reports as not
//! established at all). So the push path is backed by a plain poll of every
//! followed publisher's index every `POLL_INTERVAL`. Both paths funnel into
//! `record_posts`, and `LocalStore::claim_post_notification` makes "announce
//! this post" a single atomic claim, so the two can never double-toast. The
//! push is what makes it feel instant; the poll is what makes it eventually
//! reliable.

use crate::{contracts, db::FollowedPublisherRow, db::LocalStore, freenet_bridge::FreenetBridge};
use aetheria_types::{AccessTier, PostMetadataHeader};
use freenet_stdlib::prelude::{ContractInstanceId, UpdateData};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, mpsc};

/// Server-push events the delegate sends to the UI unprompted, over the same
/// IPC WebSocket the UI already uses for request/response (see `ipc.rs`).
/// Every event carries an `"event"` field and no `"id"`, which is how
/// `app/src/lib/delegate.ts` tells a push apart from a reply.
pub type EventSender = broadcast::Sender<serde_json::Value>;

/// How long to wait before rebuilding a subscription connection that failed
/// or died. Deliberately unhurried: the poll backstop below is what bounds
/// how stale things can get, so there's no reason to hammer a node that just
/// told us it isn't ready.
const RECONNECT_DELAY: Duration = Duration::from_secs(15);

/// Backstop cadence for "did we miss a push?" - see this module's docs. Also
/// when subscriptions that failed to establish get retried. Three minutes is
/// a compromise: short enough that a dropped push is a delay rather than a
/// loss, long enough that following a dozen publishers doesn't keep the
/// gateway network permanently busy on this one machine (each publisher costs
/// one GET, and a GET on this network can take seconds and retry four times -
/// see `freenet_bridge.rs`).
const POLL_INTERVAL: Duration = Duration::from_secs(180);

/// Handle to the running watcher task, held by `ipc.rs`'s `Unlocked`.
pub struct WatcherHandle {
    refresh_tx: mpsc::Sender<()>,
}

impl WatcherHandle {
    /// Tells the watcher the followed-publisher set may have changed, so a
    /// brand-new follow starts being watched immediately instead of at the
    /// next poll tick. Deliberately fire-and-forget: a full channel already
    /// means a refresh is queued, and a closed one means the watcher task is
    /// gone (which is logged where it happens, not here). Never blocks an IPC
    /// handler on the watcher's own progress.
    pub fn refresh(&self) {
        let _ = self.refresh_tx.try_send(());
    }
}

/// Starts the watcher. Called once per successful unlock (`ipc.rs`'s
/// `finish_unlock`), never before - there is no followed-publisher list, and
/// no identity, until then.
pub fn spawn(db: Arc<LocalStore>, events: EventSender) -> WatcherHandle {
    let (refresh_tx, refresh_rx) = mpsc::channel(8);
    tokio::spawn(run(db, events, refresh_rx));
    WatcherHandle { refresh_tx }
}

/// Per-connection state: which publishers this connection has an established
/// subscription for, and which contract each notification maps back to.
#[derive(Default)]
struct Subscriptions {
    /// Publishers with a live subscription on the current connection.
    publishers: HashSet<[u8; 32]>,
    /// Reverse lookup for an incoming notification's `ContractKey`.
    by_contract: HashMap<ContractInstanceId, [u8; 32]>,
}

async fn run(db: Arc<LocalStore>, events: EventSender, mut refresh_rx: mpsc::Receiver<()>) {
    // Publishers whose existing backlog has already been absorbed silently -
    // see `record_posts`. Kept across reconnects (unlike `Subscriptions`)
    // because it's about what the *user* has been told, not about the state
    // of any one socket. The durable half of this lives in the
    // `notified_posts` table; this set only avoids re-deciding it every loop.
    let mut primed: HashSet<[u8; 32]> = HashSet::new();

    'connection: loop {
        let bridge = match FreenetBridge::connect_local().await {
            Ok(bridge) => bridge,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "post watcher could not open its Freenet connection - retrying"
                );
                tokio::time::sleep(RECONNECT_DELAY).await;
                continue 'connection;
            }
        };
        tracing::info!("post watcher connected - subscribing to followed publishers");

        let mut subs = Subscriptions::default();
        sync_subscriptions(&bridge, &db, &events, &mut primed, &mut subs).await;

        let mut poll = tokio::time::interval(POLL_INTERVAL);
        // `interval`'s first tick is immediate; `sync_subscriptions` above
        // just did the equivalent work, so drop it rather than doing a full
        // pass over every followed publisher twice in a row on startup.
        poll.tick().await;

        loop {
            tokio::select! {
                // The only branch that borrows the bridge in its *future*
                // (immutably - the connection is behind a mutex), so the
                // other branches are free to use it in their bodies:
                // `tokio::select!` drops the losing futures before running
                // the winning branch's body.
                notification = bridge.next_update_notification() => {
                    match notification {
                        Ok((key, update)) => {
                            handle_notification(&bridge, &db, &events, &mut primed, &subs, *key.id(), update).await;
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                "post watcher's subscription connection dropped - reconnecting"
                            );
                            tokio::time::sleep(RECONNECT_DELAY).await;
                            continue 'connection;
                        }
                    }
                }

                Some(()) = refresh_rx.recv() => {
                    // Coalesce a burst (following several publishers quickly)
                    // into the single pass below.
                    while refresh_rx.try_recv().is_ok() {}

                    if unfollowed_anyone(&db, &subs) {
                        // freenet-stdlib 0.8.5 has no `Unsubscribe` request
                        // (see `FreenetBridge::subscribe`'s docs), so the
                        // only way to stop being pushed an unfollowed
                        // publisher's posts is to drop the connection that
                        // carries the subscription and rebuild it from the
                        // current follow list.
                        tracing::info!(
                            "a publisher was unfollowed - rebuilding the watcher's subscriptions"
                        );
                        continue 'connection;
                    }
                    sync_subscriptions(&bridge, &db, &events, &mut primed, &mut subs).await;
                }

                _ = poll.tick() => {
                    // Retries any subscription that never established, then
                    // does the poll backstop pass - see this module's docs.
                    sync_subscriptions(&bridge, &db, &events, &mut primed, &mut subs).await;
                    poll_followed(&bridge, &db, &events, &mut primed).await;
                }
            }
        }
    }
}

/// Whether the current connection holds a subscription for anyone who is no
/// longer followed. A local-DB read failure answers `false` (keep the
/// connection) rather than tearing a working watcher down over a transient
/// SQLite hiccup.
fn unfollowed_anyone(db: &LocalStore, subs: &Subscriptions) -> bool {
    let followed: HashSet<[u8; 32]> = match db.list_followed_publishers() {
        Ok(rows) => rows.into_iter().map(|r| r.author_pubkey).collect(),
        Err(e) => {
            tracing::warn!(error = %e, "reading followed publishers failed in the post watcher");
            return false;
        }
    };
    subs.publishers.iter().any(|pk| !followed.contains(pk))
}

/// Brings the current connection's subscriptions in line with the followed
/// list: primes any publisher this delegate hasn't accounted for yet (so
/// their backlog can't arrive as a burst of toasts), then subscribes.
/// Additive only - removals are handled by rebuilding the connection, see
/// the `refresh_rx` branch above.
///
/// Every failure here is per-publisher and non-fatal: one unreachable
/// publisher must not stop the others from being watched, exactly like
/// `ipc.rs`'s feed handlers treat a failed per-publisher fetch.
async fn sync_subscriptions(
    bridge: &FreenetBridge,
    db: &LocalStore,
    events: &EventSender,
    primed: &mut HashSet<[u8; 32]>,
    subs: &mut Subscriptions,
) {
    let followed = match db.list_followed_publishers() {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "reading followed publishers failed in the post watcher");
            return;
        }
    };

    for f in &followed {
        if !primed.contains(&f.author_pubkey) {
            prime(bridge, db, events, f, primed).await;
        }
        if subs.publishers.contains(&f.author_pubkey) {
            continue;
        }
        let key = match contracts::content_index_key_for(f.author_pubkey) {
            Ok(key) => key,
            Err(e) => {
                tracing::warn!(error = %e, "deriving a followed publisher's ContentIndexContract key failed");
                continue;
            }
        };
        match bridge.subscribe(*key.id()).await {
            Ok(true) => {
                tracing::info!(
                    publisher = %f.display_name,
                    contract = %key.encoded_contract_id(),
                    "watching a followed publisher's index for new posts"
                );
                subs.publishers.insert(f.author_pubkey);
                subs.by_contract.insert(*key.id(), f.author_pubkey);
            }
            // Not an error: the node accepted the request but isn't watching
            // that contract, typically because nobody on the network is
            // holding it right now. Left out of `subs` so the next poll tick
            // tries again; the poll backstop covers this publisher meanwhile.
            Ok(false) => tracing::info!(
                publisher = %f.display_name,
                "the node reported no subscription for this publisher's index yet - will retry"
            ),
            Err(e) => tracing::warn!(
                publisher = %f.display_name,
                error = %e,
                "subscribing to a followed publisher's index failed - will retry"
            ),
        }
    }
}

/// Absorbs everything a publisher has *already* published without announcing
/// any of it, then marks them primed.
///
/// Without this, the first look at a publisher's index would be
/// indistinguishable from them publishing their whole back catalogue at that
/// instant: following someone with forty posts would fire forty toasts, and
/// so would every app restart. Only a post that shows up *after* this pass is
/// news. A failed fetch deliberately leaves the publisher unprimed so the
/// next pass tries again, rather than treating "we couldn't see their index"
/// as "they have nothing".
async fn prime(
    bridge: &FreenetBridge,
    db: &LocalStore,
    events: &EventSender,
    f: &FollowedPublisherRow,
    primed: &mut HashSet<[u8; 32]>,
) {
    match contracts::fetch_remote_posts(bridge, f.author_pubkey).await {
        Ok(headers) => {
            let count = headers.len();
            record_posts(db, events, f, &headers, false);
            primed.insert(f.author_pubkey);
            tracing::info!(
                publisher = %f.display_name,
                existing_posts = count,
                "primed a followed publisher - only posts published from now on will notify"
            );
        }
        Err(e) => tracing::warn!(
            publisher = %f.display_name,
            error = %e,
            "could not read a followed publisher's index to prime it - staying silent for them until it works"
        ),
    }
}

/// The poll backstop: re-reads every followed publisher's index and announces
/// anything new. Same treatment a push gets, just arrived at differently -
/// see this module's docs on why both exist.
async fn poll_followed(
    bridge: &FreenetBridge,
    db: &LocalStore,
    events: &EventSender,
    primed: &mut HashSet<[u8; 32]>,
) {
    let followed = match db.list_followed_publishers() {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "reading followed publishers failed in the post watcher");
            return;
        }
    };
    for f in &followed {
        if !primed.contains(&f.author_pubkey) {
            prime(bridge, db, events, f, primed).await;
            continue;
        }
        match contracts::fetch_remote_posts(bridge, f.author_pubkey).await {
            Ok(headers) => record_posts(db, events, f, &headers, true),
            Err(e) => tracing::debug!(
                publisher = %f.display_name,
                error = %e,
                "polling a followed publisher's index failed - trying again next interval"
            ),
        }
    }
}

/// Handles one real push from the node.
async fn handle_notification(
    bridge: &FreenetBridge,
    db: &LocalStore,
    events: &EventSender,
    primed: &mut HashSet<[u8; 32]>,
    subs: &Subscriptions,
    contract: ContractInstanceId,
    update: UpdateData<'static>,
) {
    let Some(&author_pubkey) = subs.by_contract.get(&contract) else {
        // A push for something this connection didn't subscribe to. Nothing
        // in this delegate subscribes to anything else, so this means the
        // node sent something unexpected - worth a log line, not a panic and
        // not a reason to drop a working connection.
        tracing::debug!(%contract, "ignoring an update notification for an unwatched contract");
        return;
    };

    let followed = match db.list_followed_publishers() {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error = %e, "reading followed publishers failed in the post watcher");
            return;
        }
    };
    let Some(f) = followed.into_iter().find(|r| r.author_pubkey == author_pubkey) else {
        // Unfollowed between the push being sent and being handled.
        return;
    };

    // A publisher who was never primed (their index wasn't readable when
    // they were followed) must not have their whole backlog announced just
    // because a push finally arrived - prime them from a full fetch instead
    // and let the *next* change be the news.
    if !primed.contains(&author_pubkey) {
        prime(bridge, db, events, &f, primed).await;
        return;
    }

    // `ContentIndexContract` merges a full state and a delta identically
    // (both decode to a `ContentIndexState` - see that contract's
    // `update_state`), so either form is usable as-is. The `Related*`
    // variants describe a *different* contract's state and are meaningless
    // here.
    let bytes: &[u8] = match &update {
        UpdateData::State(state) => state.as_ref(),
        UpdateData::Delta(delta) => delta.as_ref(),
        UpdateData::StateAndDelta { state, .. } => state.as_ref(),
        other => {
            tracing::debug!(
                kind = ?std::mem::discriminant(other),
                "ignoring an update notification carrying a related contract's state"
            );
            return;
        }
    };

    match contracts::decode_verified_content_index(bytes, author_pubkey) {
        Ok(headers) => {
            tracing::info!(
                publisher = %f.display_name,
                posts_in_update = headers.len(),
                "received a live index update from a followed publisher"
            );
            record_posts(db, events, &f, &headers, true);
        }
        Err(e) => tracing::warn!(
            publisher = %f.display_name,
            error = %e,
            "could not decode a pushed index update - ignoring it"
        ),
    }
}

/// Files a batch of verified post headers: caches every one durably (so a
/// notified post is already in Home when the user clicks through, without
/// waiting for a refresh) and announces the ones that are genuinely new.
///
/// `announce == false` is the priming/silencing path: the posts are still
/// claimed, so they can never be announced later, they just don't produce an
/// event now.
fn record_posts(
    db: &LocalStore,
    events: &EventSender,
    f: &FollowedPublisherRow,
    headers: &[PostMetadataHeader],
    announce: bool,
) {
    let now = now_unix();
    for header in headers {
        if let Err(e) = db.cache_remote_post(
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
            tracing::warn!(error = %e, "caching a watched publisher's post failed");
        }

        // Nothing is listening right now (no UI attached - the app is closed
        // entirely, or mid-restart). Deliberately leave the post *unclaimed*
        // so the next pass announces it once a UI is back, instead of
        // burning the one chance to tell the user on a message nobody could
        // receive. Priming still claims unconditionally: its whole job is to
        // make sure these are never announced.
        if announce && events.receiver_count() == 0 {
            continue;
        }

        let newly_claimed = match db.claim_post_notification(&header.post_id, &f.author_pubkey, now)
        {
            Ok(claimed) => claimed,
            Err(e) => {
                tracing::warn!(error = %e, "recording a post notification claim failed");
                continue;
            }
        };
        if !announce || !newly_claimed {
            continue;
        }

        let locked = matches!(header.access_level, AccessTier::SubscriberOnly { .. });
        tracing::info!(
            publisher = %f.display_name,
            title = %header.title,
            "new post from a followed publisher - notifying"
        );
        let _ = events.send(serde_json::json!({
            "event": "new_post",
            "post_id": hex_encode(&header.post_id),
            "post_contract_id": header.post_contract_id,
            "title": header.title,
            "summary": header.summary,
            "access_level": access_level_str(&header.access_level),
            // Same meaning as `FeedItem.locked` in the UI: a subscriber-only
            // post from someone else is announced (that's the point - it's a
            // reason to subscribe) but can't be opened yet, see CLAUDE.md's
            // "Known stub" section.
            "locked": locked,
            "author_pubkey": hex_encode(&f.author_pubkey),
            "author_display_name": f.display_name,
            "published_at": header.published_at,
        }));
    }
}

fn access_level_str(access_level: &AccessTier) -> &'static str {
    match access_level {
        AccessTier::Public => "public",
        AccessTier::SubscriberOnly { .. } => "subscriber",
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The claim is what makes "announce once" true across the push path and
    /// the poll backstop both seeing the same post, so it's worth pinning
    /// directly rather than trusting the SQL by inspection.
    #[test]
    fn a_post_can_only_be_claimed_once() {
        let dir = std::env::temp_dir().join(format!("aetheria-watcher-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = LocalStore::open(&dir.join("claim-test.sqlite")).unwrap();

        let post_id = [7u8; 16];
        let author = [9u8; 32];
        assert!(db.claim_post_notification(&post_id, &author, 1).unwrap());
        assert!(!db.claim_post_notification(&post_id, &author, 2).unwrap());
        // A different post is unaffected by the first one's claim.
        assert!(db.claim_post_notification(&[8u8; 16], &author, 3).unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
