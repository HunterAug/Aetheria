//! End-to-end verification (ignored by default - requires a live local
//! Freenet node) that the ECDH subscriber key-delivery mechanism
//! (`crypto.rs`) and the `SubscriberRegistryContract` wiring (`contracts.rs`)
//! work for real over the real network, independent of any NWC/Lightning
//! payment.
//!
//! Compiled as a unit-test module (declared `#[cfg(test)]` from `main.rs`),
//! not an integration test under `tests/` - `aetheria-delegate` is a
//! binary-only crate with no lib target, so a `tests/*.rs` file couldn't
//! reach `contracts`/`crypto`/`keys`/`db` (all private modules) at all.
//! Compiling in-crate is what `db.rs`'s and `keys.rs`'s own `#[cfg(test)]`
//! blocks already do, just split into its own file for a test this size.
//!
//! Simulates two independent identities - "publisher" and "subscriber" -
//! never sharing any Rust value directly except what a real Workflow B
//! exchange would legitimately convey between them:
//!   - the publisher's Ed25519 `author_pubkey`, used only to *locate* their
//!     `SubscriberRegistryContract` (`contracts::subscriber_registry_key_for`
//!     is a pure local hash, no discovery call) - publicly readable off
//!     their real `PublisherProfileContract` in production;
//!   - the publisher's secp256k1 identity public key, used for ECDH - in
//!     production this would arrive via the peer-message channel design doc
//!     §5.2 step 2 describes ("Reader's Delegate sends ... PK_sub ... to the
//!     Publisher's Delegate"), which isn't built yet (same Phase-4 bucket as
//!     `FreenetBridge::subscribe` - see CLAUDE.md), so this test supplies it
//!     directly, standing in for that not-yet-built channel rather than
//!     inventing a new one just for this test. The production `ipc.rs`
//!     `Subscribe` handler sidesteps this entirely: in this milestone's
//!     single-identity architecture, subscriber and publisher are always the
//!     same delegate, so there's nothing to discover there. This test is
//!     deliberately more rigorous than that handler can exercise, using two
//!     genuinely different secp256k1 identities.
//!
//! Run with:
//! `cargo test subscriber_registry_e2e -- --ignored --nocapture`
//! (needs a real `freenet` node on `ws://127.0.0.1:7509/...`, see CLAUDE.md).

use crate::contracts;
use crate::crypto;
use crate::db::LocalStore;
use crate::freenet_bridge::FreenetBridge;
use crate::keys::DelegateKeys;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::SecretKey as K256SecretKey;

fn temp_db(label: &str) -> (LocalStore, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "aetheria-e2e-{label}-{}-{}",
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

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[tokio::test]
#[ignore = "requires a live local Freenet node (ws://127.0.0.1:7509) - see CLAUDE.md"]
async fn ecdh_key_bundle_round_trips_over_the_real_network() {
    // --- Publisher side setup ---
    let publisher_keys = DelegateKeys::generate();
    let (publisher_db, publisher_dir) = temp_db("publisher");
    let freenet_publisher = FreenetBridge::connect_local()
        .await
        .expect("connect to local Freenet node");

    // --- Subscriber side setup: a totally independent secp256k1 identity,
    // standing in for a different person's Delegate. Only its secret key
    // and the publisher's already-public info are used from here on. ---
    let subscriber_secret = K256SecretKey::random(&mut rand::rngs::OsRng);
    let subscriber_pubkey_compressed: [u8; 33] = subscriber_secret
        .public_key()
        .to_encoded_point(true)
        .as_bytes()
        .try_into()
        .unwrap();

    let epoch_id: u32 = 999_001; // arbitrary, distinguishable in a fresh registry
    let epoch_key = crypto::generate_epoch_key();

    // Publisher: Si = ECDH(SKpub, PKsub) -> wrap Kepoch -> publish bundle.
    let shared_secret_publisher_side = crypto::derive_shared_secret(
        &publisher_keys.identity_secret,
        &subscriber_secret.public_key(),
    );
    let wrapped =
        crypto::wrap_epoch_key(&shared_secret_publisher_side, &epoch_key).expect("wrap epoch key");

    let bundle = aetheria_types::EncryptedKeyBundle {
        subscriber_pubkey: subscriber_pubkey_compressed,
        epoch_id,
        cipher_text: wrapped.cipher_text,
        nonce: wrapped.nonce,
        auth_tag: [0u8; 16],
        issued_at: 1_700_000_000,
    };

    let registry_key = contracts::publish_key_bundle_to_network(
        &freenet_publisher,
        &publisher_db,
        &publisher_keys,
        bundle,
    )
    .await
    .expect("publish key bundle to the real SubscriberRegistryContract");

    println!(
        "SubscriberRegistryContract key: {}",
        registry_key.encoded_contract_id()
    );
    println!(
        "publisher author_pubkey (hex): {}",
        hex_encode(&publisher_keys.master_signing_verifying_bytes())
    );
    println!(
        "subscriber_pubkey (hex): {}",
        hex_encode(&subscriber_pubkey_compressed)
    );

    // --- Subscriber side: an independent connection (simulating a separate
    // process/delegate), only knowing the publisher's Ed25519 pubkey (to
    // locate the registry - no discovery call, see
    // contracts::subscriber_registry_key_for) and secp256k1 public key (to
    // compute the same shared secret). ---
    let freenet_subscriber = FreenetBridge::connect_local()
        .await
        .expect("connect to local Freenet node (second, independent connection)");

    let fetched = contracts::fetch_key_bundle(
        &freenet_subscriber,
        publisher_keys.master_signing_verifying_bytes(),
        subscriber_pubkey_compressed,
        epoch_id,
    )
    .await
    .expect("fetch key bundle from the real network")
    .expect("bundle should exist - it was just published above");

    let shared_secret_subscriber_side = crypto::derive_shared_secret_as_subscriber(
        &subscriber_secret,
        &publisher_keys.identity_secret.public_key(),
    );
    assert_eq!(
        shared_secret_publisher_side, shared_secret_subscriber_side,
        "ECDH must be symmetric - both sides should derive the same shared secret"
    );

    let recovered_epoch_key = crypto::unwrap_epoch_key(
        &shared_secret_subscriber_side,
        &fetched.nonce,
        &fetched.cipher_text,
    )
    .expect("subscriber should be able to decrypt the epoch key with their own secret");

    assert_eq!(
        recovered_epoch_key, epoch_key,
        "the epoch key the subscriber recovered must match the one the publisher generated"
    );

    println!(
        "PASS: subscriber independently recovered the correct epoch key over the real network"
    );

    std::fs::remove_dir_all(&publisher_dir).ok();
}
