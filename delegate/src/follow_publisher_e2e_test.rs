//! End-to-end verification (ignored by default - requires a live local
//! Freenet node) that following another publisher and reading their public
//! posts works for real over the real network, independent of any local
//! shared state - same rigor and same reason for existing as
//! `subscriber_registry_e2e_test.rs`: two genuinely independent identities,
//! never sharing a Rust value except what a real "paste their pubkey"
//! interaction would legitimately convey (here: just the hex-encoded
//! Ed25519 `author_pubkey`, the same string a user would copy/paste into
//! the Following tab's input field).
//!
//! Run with:
//! `cargo test follow_publisher_e2e -- --ignored --nocapture`
//! (needs a real `freenet` node on `ws://127.0.0.1:7509`, see CLAUDE.md).

use crate::contracts;
use crate::db::LocalStore;
use crate::freenet_bridge::FreenetBridge;
use crate::keys::DelegateKeys;

fn temp_db(label: &str) -> (LocalStore, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "aetheria-followe2e-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.sqlite");
    (LocalStore::open(&path).unwrap(), dir)
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[tokio::test]
#[ignore = "requires a live local Freenet node (ws://127.0.0.1:7509) - see CLAUDE.md"]
async fn following_a_publisher_and_reading_their_public_post_round_trips_over_the_real_network() {
    // --- "Publisher B" side: a completely independent identity from
    // whatever this delegate's own real identity is (see CLAUDE.md - there's
    // already one real identity's content on the network; this mints a
    // second, clearly test-labeled one so there's something real to follow
    // that isn't the delegate's own). ---
    let publisher_keys = DelegateKeys::generate();
    let (publisher_db, publisher_dir) = temp_db("publisher");
    let freenet_publisher = FreenetBridge::connect_local()
        .await
        .expect("connect to local Freenet node (publisher side)");

    let identity = contracts::ensure_publisher_identity(&freenet_publisher, &publisher_db, &publisher_keys)
        .await
        .expect("publish test publisher's ContentIndexContract + PublisherProfileContract");

    contracts::publish_profile_to_network(
        &freenet_publisher,
        &publisher_keys,
        &identity,
        "Test Publisher B (follow_publisher_e2e_test)",
        "An independent test identity minted only to verify the Following feature.",
        None,
    )
    .await
    .expect("publish a real display name for the test publisher");

    let markdown = "# Hello from Publisher B\n\nThis is a real public post on the real network, \
        published only so a *different* identity's follow/read path can be verified end to end.";
    let post_contract_id = contracts::publish_post_to_network(
        &freenet_publisher,
        &publisher_keys,
        &identity,
        rand_post_id(),
        "Hello from Publisher B",
        "A public test post for follow_publisher_e2e_test.",
        aetheria_types::AccessTier::Public,
        123, // epoch_id, irrelevant for a public post
        now_unix(),
        markdown.as_bytes().to_vec(),
        [0u8; 12], // public-post convention: plaintext bytes, all-zero nonce
    )
    .await
    .expect("publish a real public PostDataContract");

    let publisher_author_pubkey = publisher_keys.master_signing_verifying_bytes();
    println!(
        "Test publisher author_pubkey (hex): {}",
        hex_encode(&publisher_author_pubkey)
    );
    println!("Post contract id: {post_contract_id}");

    // --- "Reader A" side: a second, independent identity/connection/db,
    // standing in for a completely different person's delegate. Only the
    // publisher's hex author_pubkey crosses the boundary here - exactly what
    // a user would paste into the Following tab. ---
    let reader_keys = DelegateKeys::generate();
    let (reader_db, reader_dir) = temp_db("reader");
    let freenet_reader = FreenetBridge::connect_local()
        .await
        .expect("connect to local Freenet node (reader side, independent connection)");
    assert_ne!(
        reader_keys.master_signing_verifying_bytes(),
        publisher_author_pubkey,
        "sanity check: reader and publisher must be genuinely different identities"
    );

    // 1. Follow: fetch + verify the real signed profile (no local DB
    //    dependency yet - this is the network-only half `ipc.rs`'s
    //    `handle_follow_publisher` calls before ever touching `reader_db`).
    let fetched_profile = contracts::fetch_remote_profile(&freenet_reader, publisher_author_pubkey)
        .await
        .expect("fetch remote profile over the real network")
        .expect("profile should exist - it was just published above");
    assert_eq!(
        fetched_profile.display_name,
        "Test Publisher B (follow_publisher_e2e_test)"
    );
    reader_db
        .follow_publisher(
            &publisher_author_pubkey,
            &fetched_profile.display_name,
            fetched_profile.avatar_freenet_key.as_deref(),
            now_unix(),
        )
        .expect("save the follow locally");

    let followed = reader_db
        .list_followed_publishers()
        .expect("list followed publishers");
    assert_eq!(followed.len(), 1);
    assert_eq!(followed[0].author_pubkey, publisher_author_pubkey);

    // 2. Fetch their real post list over the network, independent of any
    //    value shared in-process with the publisher side above.
    let posts = contracts::fetch_remote_posts(&freenet_reader, publisher_author_pubkey)
        .await
        .expect("fetch remote posts over the real network");
    let header = posts
        .iter()
        .find(|p| p.post_contract_id == post_contract_id)
        .expect("the post just published should be in the fetched index");
    assert_eq!(header.title, "Hello from Publisher B");
    assert_eq!(header.access_level, aetheria_types::AccessTier::Public);

    // 3. Fetch and decode the actual post payload - the same path
    //    `ipc.rs::handle_get_remote_post` takes for a `Public` post.
    let payload = contracts::fetch_remote_post_payload(&freenet_reader, &header.post_contract_id)
        .await
        .expect("fetch the real PostDataContract payload");
    assert_eq!(payload.nonce, [0u8; 12], "public posts use the all-zero-nonce convention");
    let recovered_markdown =
        String::from_utf8(payload.cipher_text).expect("public post payload is valid UTF-8 markdown");
    assert_eq!(
        recovered_markdown, markdown,
        "reader must recover the exact markdown the publisher published, over the real network, \
         between two genuinely independent identities"
    );

    println!(
        "PASS: reader independently followed a stranger's pubkey and recovered their real public \
         post over the real network"
    );

    std::fs::remove_dir_all(&publisher_dir).ok();
    std::fs::remove_dir_all(&reader_dir).ok();
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn rand_post_id() -> [u8; 16] {
    use rand::RngCore;
    let mut id = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut id);
    id
}
