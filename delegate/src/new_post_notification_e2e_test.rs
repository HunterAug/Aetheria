//! End-to-end verification (ignored by default - requires a live local
//! Freenet node) that a post published by a *different* identity reaches this
//! delegate as a real server-push `new_post` event over the real IPC
//! protocol, without anyone asking for it.
//!
//! This is the delegate half of the desktop-notification feature end to end:
//! `FreenetBridge::subscribe` → the node's own `UpdateNotification` push →
//! `watcher.rs`'s signature check and new-post diff → the broadcast channel →
//! `ipc.rs`'s per-connection forwarder → a WebSocket frame a UI can act on.
//! The only link past this point is the frontend handing the event to Tauri's
//! notification command, which needs a real desktop session and can't be
//! asserted from a test.
//!
//! Same shape and rigor as `follow_publisher_e2e_test.rs`: a genuinely
//! independent publisher identity, nothing shared in-process with the reader
//! except the hex `author_pubkey` a user would paste into the Following tab,
//! and a real `ipc::serve` listener driven over a real WebSocket rather than
//! by calling handlers directly.
//!
//! Run with:
//! `cargo test new_post_notification_e2e -- --ignored --nocapture`
//! (needs a real `freenet` node on `ws://127.0.0.1:7509`, see CLAUDE.md).

use crate::contracts;
use crate::db::LocalStore;
use crate::freenet_bridge::FreenetBridge;
use crate::keys::DelegateKeys;
use futures_util::{SinkExt, StreamExt};
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

/// Deliberately not `IPC_PORT` (47021): this machine routinely has a real
/// delegate already bound there (see CLAUDE.md's dev scripts), and a test
/// that only passes when nothing else is running is a test that gets
/// disabled.
const TEST_IPC_PORT: u16 = 47_122;

/// Generous on purpose. A live subscription push arrives in about a second,
/// but `watcher.rs`'s polling backstop only fires every three minutes, and
/// this test is honest about passing either way - the elapsed time it prints
/// is what tells you which path delivered (single-digit seconds = the real
/// push; ~180s = the poll caught what the push missed).
const NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(300);

fn temp_dir(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "aetheria-notify-e2e-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn rand_post_id() -> [u8; 16] {
    use rand::RngCore;
    let mut id = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut id);
    id
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[tokio::test]
#[ignore = "requires a live local Freenet node (ws://127.0.0.1:7509) - see CLAUDE.md"]
async fn a_followed_publishers_new_post_arrives_as_a_push_event_over_real_ipc() {
    let _ = tracing_subscriber::fmt::try_init();

    // --- Publisher: an independent, clearly test-labeled identity, exactly
    // like follow_publisher_e2e_test.rs mints. ---
    let publisher_keys = DelegateKeys::generate();
    let publisher_dir = temp_dir("publisher");
    let publisher_db = LocalStore::open(&publisher_dir.join("test.sqlite")).unwrap();
    let publisher_net = FreenetBridge::connect_local()
        .await
        .expect("connect to local Freenet node (publisher side)");
    let identity =
        contracts::ensure_publisher_identity(&publisher_net, &publisher_db, &publisher_keys)
            .await
            .expect("publish the test publisher's own contracts");
    contracts::publish_profile_to_network(
        &publisher_net,
        &publisher_keys,
        &identity,
        "Test Publisher N (new_post_notification_e2e_test)",
        "An independent test identity minted only to verify new-post notifications.",
        None,
    )
    .await
    .expect("publish a real display name for the test publisher");

    // A post published *before* the reader follows them. The point of this
    // one is that it must NOT produce a notification - `watcher.rs` primes a
    // publisher's existing posts silently, so following someone is never a
    // burst of toasts for their back catalogue.
    let backlog_title = "Old post that must not notify";
    contracts::publish_post_to_network(
        &publisher_net,
        &publisher_keys,
        &identity,
        rand_post_id(),
        backlog_title,
        "Published before the reader followed - priming must absorb this silently.",
        aetheria_types::AccessTier::Public,
        1,
        now_unix(),
        b"backlog".to_vec(),
        [0u8; 12],
    )
    .await
    .expect("publish the backlog post");

    let publisher_pubkey_hex = hex_encode(&publisher_keys.master_signing_verifying_bytes());
    println!("test publisher author_pubkey: {publisher_pubkey_hex}");

    // --- Reader: a real `ipc::serve` listener with its own fresh data dir,
    // driven the way the UI drives it (WebSocket, JSON, `id`-correlated). ---
    let reader_dir = temp_dir("reader");
    let reader_db = LocalStore::open(&reader_dir.join("aetheria.sqlite")).unwrap();
    let identity_key_path = reader_dir.join("identity.key");
    tokio::spawn(crate::ipc::serve(TEST_IPC_PORT, reader_db, identity_key_path));

    // The listener binds before it does anything else, but the OS still needs
    // a moment; retry rather than sleeping a fixed amount and hoping.
    let mut ws = None;
    for _ in 0..40 {
        match tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{TEST_IPC_PORT}")).await {
            Ok((socket, _)) => {
                ws = Some(socket);
                break;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(250)).await,
        }
    }
    let (mut write, mut read) = ws.expect("the test delegate's IPC listener never came up").split();

    // 1. Unlock: creates a fresh identity and publishes its contracts for
    //    real, exactly as a first run of the app does.
    write
        .send(Message::Text(
            serde_json::json!({"id": "1", "op": "unlock", "passphrase": "notification-e2e-test"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let unlock = next_reply(&mut read, "1").await;
    assert!(
        unlock.get("error").is_none(),
        "unlock failed: {unlock} (the real gateway network is flaky - see CLAUDE.md - retrying the \
         whole test usually clears it)"
    );

    // 2. Follow the test publisher, exactly as pasting their pubkey does.
    //    This is also what tells the watcher to subscribe to their index.
    write
        .send(Message::Text(
            serde_json::json!({"id": "2", "op": "follow_publisher", "author_pubkey": publisher_pubkey_hex})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let follow = next_reply(&mut read, "2").await;
    assert!(follow.get("error").is_none(), "follow failed: {follow}");
    // Whatever name the follow resolved to is what a notification must carry.
    // Deliberately not hardcoded to the string published above: on this
    // network the reader can legitimately still be seeing the publisher's
    // first (untitled) profile if the later profile update hasn't propagated
    // yet - a real property of the network, not something for this test to
    // paper over or to fail on.
    let followed_display_name = follow["result"]["display_name"].clone();
    println!("followed the test publisher as {followed_display_name}; waiting for the watcher to prime and subscribe");

    // Give the watcher time to prime (a GET of their index) and establish the
    // subscription before anything new is published - otherwise the new post
    // would just be part of the backlog it silently absorbs, and the test
    // would be measuring the polling backstop instead of the push.
    tokio::time::sleep(Duration::from_secs(10)).await;

    // 3. The event under test: a genuinely new post, published by a different
    //    identity over a different connection, with nobody asking for it.
    let new_title = "Brand new post that must notify";
    contracts::publish_post_to_network(
        &publisher_net,
        &publisher_keys,
        &identity,
        rand_post_id(),
        new_title,
        "Published after the follow - this is the one that must reach the UI unprompted.",
        aetheria_types::AccessTier::Public,
        1,
        now_unix(),
        b"brand new".to_vec(),
        [0u8; 12],
    )
    .await
    .expect("publish the new post");
    let published_at = Instant::now();
    println!("published the new post - waiting for a push event");

    // 4. Wait for the push. Anything else arriving on this socket would be a
    //    reply to a request, and this test has none outstanding.
    let event = tokio::time::timeout(NOTIFICATION_TIMEOUT, async {
        loop {
            let Some(Ok(Message::Text(text))) = read.next().await else {
                panic!("the IPC socket closed while waiting for a push event");
            };
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            if value.get("event").is_some() {
                return value;
            }
        }
    })
    .await
    .expect("no new_post push event arrived - neither the subscription nor the poll backstop \
             delivered it");

    println!(
        "push event arrived {:.1}s after publishing: {event}",
        published_at.elapsed().as_secs_f64()
    );
    assert_eq!(event["event"], "new_post");
    assert_eq!(
        event["title"], new_title,
        "the notification must be for the post published after the follow, not the backlog one \
         ({backlog_title:?}) that priming should have absorbed silently"
    );
    assert_eq!(event["author_pubkey"], publisher_pubkey_hex);
    assert_eq!(event["author_display_name"], followed_display_name);
    assert_eq!(event["locked"], false, "a public post is not locked");

    println!("PASS: a followed publisher's new post reached this delegate as an unprompted push");

    std::fs::remove_dir_all(&publisher_dir).ok();
    std::fs::remove_dir_all(&reader_dir).ok();
}

/// Reads until the reply with `id` shows up, skipping any server-push events
/// that happen to interleave (they carry no `id` at all - which is the whole
/// point of that distinction).
async fn next_reply<S>(read: &mut S, id: &str) -> serde_json::Value
where
    S: futures_util::Stream<
            Item = Result<Message, tokio_tungstenite::tungstenite::Error>,
        > + Unpin,
{
    let deadline = Duration::from_secs(180);
    tokio::time::timeout(deadline, async {
        loop {
            let Some(Ok(Message::Text(text))) = read.next().await else {
                panic!("the IPC socket closed while waiting for reply {id}");
            };
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            if value.get("id").and_then(|v| v.as_str()) == Some(id) {
                return value;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for reply {id}"))
}
