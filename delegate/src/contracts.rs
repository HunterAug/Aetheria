//! Publishes this delegate's own `PublisherProfileContract` and
//! `ContentIndexContract` on the real Freenet network (design doc §3.1-3.2),
//! and mints a fresh `PostDataContract` instance (§3.3) per published post.
//!
//! Compiled contract WASM is embedded at delegate-compile-time via
//! `include_bytes!`, pointing at `fdev build`'s output under each contract
//! crate's (gitignored) `build/freenet/` directory - see CLAUDE.md for the
//! `fdev build` invocation and its `CARGO_TARGET_DIR` workaround. This keeps
//! the delegate's *runtime* free of any dependency on `fdev` being
//! installed, at the cost of a manual build step: if a contract's source
//! changes, `fdev build` must be re-run for it before rebuilding the
//! delegate, or the embedded bytes go stale silently.
//!
//! `ContentIndexState` and the wire shape of `PublisherProfile` below are
//! hand-mirrored copies of `content-index-contract`'s and
//! `publisher-profile-contract`'s own state structs (same field names -
//! ciborium encodes named structs as CBOR maps keyed by field name, so names
//! matching is what matters, not declaration order). They aren't imported
//! from those crates directly because the `#[contract]` macro's `#[no_mangle]
//! extern "C"` WASM exports assume being the only such crate linked into a
//! binary (see the `freenet-main-contract` feature note in each contract's
//! Cargo.toml) - pulling more than one of these crates into one native
//! binary risks colliding exports. Keep these two structs in sync by hand if
//! either contract's state shape changes.

use crate::{db::LocalStore, freenet_bridge::FreenetBridge, keys::DelegateKeys};
use aetheria_types::{AccessTier, EncryptedPostPayload, PostMetadataHeader, Tier};
use anyhow::{Context, Result};
use ed25519_dalek::Signer;
use freenet_stdlib::prelude::{CodeHash, ContractCode, ContractInstanceId, ContractKey, Parameters};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const POST_DATA_CONTRACT_WASM: &[u8] =
    include_bytes!("../../contracts/post-data-contract/build/freenet/post_data_contract");
const CONTENT_INDEX_CONTRACT_WASM: &[u8] =
    include_bytes!("../../contracts/content-index-contract/build/freenet/content_index_contract");
const PUBLISHER_PROFILE_CONTRACT_WASM: &[u8] = include_bytes!(
    "../../contracts/publisher-profile-contract/build/freenet/publisher_profile_contract"
);

fn load_code(bytes: &'static [u8]) -> Result<Arc<ContractCode<'static>>> {
    let (code, _version) = ContractCode::load_versioned_from_bytes(bytes.to_vec()).context(
        "parsing embedded contract package - rebuild it with `fdev build` (see CLAUDE.md)",
    )?;
    Ok(Arc::new(code))
}

/// Mirror of `content_index_contract::ContentIndexState` - see module docs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ContentIndexState {
    publication_id: [u8; 32],
    posts: Vec<PostMetadataHeader>,
    last_sequence_num: u64,
}

/// Mirror of `publisher_profile_contract::PublisherProfile` - see module docs.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PublisherProfile {
    author_pubkey: [u8; 32],
    title: String,
    description: String,
    avatar_freenet_key: Option<String>,
    subscription_tiers: Vec<Tier>,
    content_index_contract_id: String,
    updated_at: u64,
    #[serde(with = "BigArray")]
    signature: [u8; 64],
}

impl PublisherProfile {
    fn signable_bytes(&self) -> Vec<u8> {
        let mut unsigned = self.clone();
        unsigned.signature = [0u8; 64];
        let mut buf = Vec::new();
        ciborium::into_writer(&unsigned, &mut buf).expect("cbor serialization is infallible");
        buf
    }
}

fn header_signable_bytes(header: &PostMetadataHeader) -> Vec<u8> {
    let mut unsigned = header.clone();
    unsigned.signature = [0u8; 64];
    let mut buf = Vec::new();
    ciborium::into_writer(&unsigned, &mut buf).expect("cbor serialization is infallible");
    buf
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_secs()
}

fn contract_key_from_registration(instance_id: [u8; 32], code_hash: [u8; 32]) -> ContractKey {
    ContractKey::from_id_and_code(ContractInstanceId::new(instance_id), CodeHash::new(code_hash))
}

/// This delegate's own identity on the Freenet network: the two contracts
/// every publisher needs before they can publish a single post.
pub struct PublisherIdentity {
    pub content_index_key: ContractKey,
    #[allow(dead_code)]
    pub profile_key: ContractKey,
    post_data_code: Arc<ContractCode<'static>>,
}

/// On first run (no stored keys in `db`), publishes a fresh, empty
/// `ContentIndexContract` and a signed `PublisherProfileContract` pointing at
/// it, and remembers both keys in `db` for reuse. On every later run, just
/// reconstructs the same `ContractKey`s from what's stored - no network call.
pub async fn ensure_publisher_identity(
    freenet: &FreenetBridge,
    db: &LocalStore,
    keys: &DelegateKeys,
) -> Result<PublisherIdentity> {
    let author_pubkey = keys.master_signing_verifying_bytes();
    // Scopes the contract instance to this publisher: same code, params keyed
    // on their pubkey, so two publishers never collide on one instance id.
    let params = Parameters::from(author_pubkey.to_vec());

    let content_index_key = match db.get_contract_registration("content_index")? {
        Some((instance_id, code_hash)) => contract_key_from_registration(instance_id, code_hash),
        None => {
            let code = load_code(CONTENT_INDEX_CONTRACT_WASM)?;
            let initial = ContentIndexState {
                publication_id: author_pubkey,
                posts: Vec::new(),
                last_sequence_num: 0,
            };
            let mut buf = Vec::new();
            ciborium::into_writer(&initial, &mut buf)?;
            let key = freenet
                .put_new(code, params.clone(), buf)
                .await
                .context("publishing initial ContentIndexContract")?;
            db.set_contract_registration(
                "content_index",
                key.id().as_bytes(),
                key.code_hash().as_ref(),
            )?;
            tracing::info!(
                contract_key = %key.encoded_contract_id(),
                "published ContentIndexContract"
            );
            key
        }
    };

    let profile_key = match db.get_contract_registration("publisher_profile")? {
        Some((instance_id, code_hash)) => contract_key_from_registration(instance_id, code_hash),
        None => {
            let code = load_code(PUBLISHER_PROFILE_CONTRACT_WASM)?;
            let mut profile = PublisherProfile {
                author_pubkey,
                // TODO(later): real publication settings once the UI exposes
                // them - nothing populates title/description/tiers yet.
                title: "Untitled Publication".to_string(),
                description: String::new(),
                avatar_freenet_key: None,
                subscription_tiers: Vec::new(),
                content_index_contract_id: content_index_key.encoded_contract_id(),
                updated_at: now_unix(),
                signature: [0u8; 64],
            };
            let signature = keys.master_signing.sign(&profile.signable_bytes());
            profile.signature = signature.to_bytes();

            let mut buf = Vec::new();
            ciborium::into_writer(&profile, &mut buf)?;
            let key = freenet
                .put_new(code, params, buf)
                .await
                .context("publishing PublisherProfileContract")?;
            db.set_contract_registration(
                "publisher_profile",
                key.id().as_bytes(),
                key.code_hash().as_ref(),
            )?;
            tracing::info!(
                contract_key = %key.encoded_contract_id(),
                "published PublisherProfileContract"
            );
            key
        }
    };

    Ok(PublisherIdentity {
        content_index_key,
        profile_key,
        post_data_code: load_code(POST_DATA_CONTRACT_WASM)?,
    })
}

/// Publishes one post's payload to a fresh `PostDataContract` instance, then
/// folds a signed `PostMetadataHeader` for it into the publisher's
/// `ContentIndexContract`. Returns the new `PostDataContract`'s encoded
/// (base58) contract id, the same string format
/// `PostMetadataHeader::post_contract_id` stores.
#[allow(clippy::too_many_arguments)]
pub async fn publish_post_to_network(
    freenet: &FreenetBridge,
    keys: &DelegateKeys,
    identity: &PublisherIdentity,
    post_id: [u8; 16],
    title: &str,
    summary: &str,
    access_level: AccessTier,
    epoch_id: u32,
    published_at: u64,
    cipher_text: Vec<u8>,
    nonce: [u8; 12],
) -> Result<String> {
    let payload = EncryptedPostPayload {
        post_id,
        cipher_text,
        nonce,
        // The AES-256-GCM tag `aes_gcm::Aes256Gcm::encrypt` produces is
        // already appended to `cipher_text` (standard AEAD output) - this
        // field is part of the wire schema but carries no independent data;
        // `crypto.rs`/`db.rs` treat `cipher_text` the same way for the local
        // SQLite cache, so this mirrors that convention instead of inventing
        // a second one for the network path.
        auth_tag: [0u8; 16],
        attachments: Vec::new(),
    };
    let mut payload_buf = Vec::new();
    ciborium::into_writer(&payload, &mut payload_buf)?;

    // Parameters = post_id gives every post its own contract instance from
    // the one shared PostDataContract code, per design doc §3.3.
    let post_params = Parameters::from(post_id.to_vec());
    let post_key = freenet
        .put_new(identity.post_data_code.clone(), post_params, payload_buf)
        .await
        .context("publishing PostDataContract")?;
    let post_contract_id = post_key.encoded_contract_id();

    let mut header = PostMetadataHeader {
        post_id,
        title: title.to_string(),
        summary: summary.to_string(),
        post_contract_id: post_contract_id.clone(),
        access_level,
        epoch_id,
        published_at,
        signature: [0u8; 64],
    };
    let signature = keys.master_signing.sign(&header_signable_bytes(&header));
    header.signature = signature.to_bytes();

    let current = freenet
        .get_state(*identity.content_index_key.id())
        .await
        .context("fetching current ContentIndexContract state")?;
    let mut state: ContentIndexState = match current {
        Some(bytes) => {
            ciborium::from_reader(bytes.as_slice()).context("decoding ContentIndexContract state")?
        }
        None => ContentIndexState {
            publication_id: keys.master_signing_verifying_bytes(),
            posts: Vec::new(),
            last_sequence_num: 0,
        },
    };
    state.posts.retain(|p| p.post_id != header.post_id);
    state.posts.push(header);
    state.posts.sort_by_key(|p| p.published_at);
    state.last_sequence_num += 1;

    let mut state_buf = Vec::new();
    ciborium::into_writer(&state, &mut state_buf)?;
    freenet
        .update_state(identity.content_index_key, state_buf)
        .await
        .context("updating ContentIndexContract")?;

    Ok(post_contract_id)
}
