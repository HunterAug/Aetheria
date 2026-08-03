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
use aetheria_types::{
    AccessTier, EncryptedKeyBundle, EncryptedPostPayload, GlobalDirectoryEntry, PostMetadataHeader,
    Tier,
};
use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Signer, VerifyingKey};
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
const SUBSCRIBER_REGISTRY_CONTRACT_WASM: &[u8] = include_bytes!(
    "../../contracts/subscriber-registry-contract/build/freenet/subscriber_registry_contract"
);
const GLOBAL_DIRECTORY_CONTRACT_WASM: &[u8] = include_bytes!(
    "../../contracts/global-directory-contract/build/freenet/global_directory_contract"
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

/// Mirror of `subscriber_registry_contract::SubscriberRegistryState` - see
/// module docs for why this is hand-copied rather than imported.
/// `EncryptedKeyBundle` itself *is* imported directly from `aetheria-types`
/// (unlike this struct) - it's a plain data type with no `#[contract]`
/// macro involvement, so there's no WASM-export collision risk in sharing it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SubscriberRegistryState {
    publication_id: [u8; 32],
    bundles: Vec<EncryptedKeyBundle>,
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

/// Mirror of `global_directory_contract::GlobalDirectoryState` - see module
/// docs for why this is hand-copied rather than imported.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct GlobalDirectoryState {
    entries: Vec<GlobalDirectoryEntry>,
}

/// Newest kept, oldest evicted - must match
/// `global_directory_contract::MAX_ENTRIES` exactly, since this delegate-side
/// copy is what actually performs the truncation before every PUT/UPDATE (the
/// WASM contract's own `validate_state` only rejects an already-too-long
/// state, it doesn't truncate one itself - see that crate's module docs).
const GLOBAL_DIRECTORY_MAX_ENTRIES: usize = 1000;

fn global_directory_entry_signable_bytes(entry: &GlobalDirectoryEntry) -> Vec<u8> {
    let mut unsigned = entry.clone();
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
                // Blank on purpose: a fresh identity has no display name yet -
                // the UI prompts for one (see App.tsx's first-run check on
                // `display_name.trim() === ""`) rather than this publishing a
                // placeholder like "Untitled Publication" that a new user
                // might not think to go change.
                title: String::new(),
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

/// Derives a stable 16-byte identifier for this publisher's avatar
/// `PostDataContract` instance from their pubkey (not a random post_id, the
/// way real posts get one) so repeated avatar edits update the *same*
/// contract instance rather than minting a fresh one every time - that's
/// what lets `avatar_freenet_key` keep pointing at the right place across
/// profile edits.
fn avatar_post_id(author_pubkey: &[u8; 32]) -> [u8; 16] {
    author_pubkey[0..16].try_into().expect("pubkey is 32 bytes, slice is 16")
}

/// Publishes (first call) or updates (every later call) this publisher's
/// avatar image through the same already-compiled `PostDataContract` code
/// used for post bodies, rather than inventing a new contract type - its
/// schema (`EncryptedPostPayload`) is a generic blob container, and
/// `post_id` doesn't have to literally be a post (see the module docs and
/// CLAUDE.md for the reasoning). Returns the (possibly new) contract's
/// encoded id, storing/reusing its registration under db role `"avatar"`
/// the same way `content_index`/`publisher_profile` do, so a restart finds
/// the same instance instead of re-publishing.
pub async fn publish_avatar_to_network(
    freenet: &FreenetBridge,
    db: &LocalStore,
    identity: &PublisherIdentity,
    author_pubkey: [u8; 32],
    avatar_bytes: Vec<u8>,
) -> Result<String> {
    let avatar_id = avatar_post_id(&author_pubkey);
    let payload = EncryptedPostPayload {
        post_id: avatar_id,
        cipher_text: avatar_bytes,
        // Avatars are public by definition (shown on every post) - same
        // all-zero-nonce "plaintext" convention public posts use (see
        // `publish_post_to_network` above and `ipc.rs`).
        nonce: [0u8; 12],
        auth_tag: [0u8; 16],
        attachments: Vec::new(),
    };
    let mut buf = Vec::new();
    ciborium::into_writer(&payload, &mut buf)?;

    match db.get_contract_registration("avatar")? {
        Some((instance_id, code_hash)) => {
            let key = contract_key_from_registration(instance_id, code_hash);
            freenet
                .update_state(key, buf)
                .await
                .context("updating avatar PostDataContract")?;
            Ok(key.encoded_contract_id())
        }
        None => {
            let params = Parameters::from(avatar_id.to_vec());
            let key = freenet
                .put_new(identity.post_data_code.clone(), params, buf)
                .await
                .context("publishing avatar PostDataContract")?;
            db.set_contract_registration("avatar", key.id().as_bytes(), key.code_hash().as_ref())?;
            Ok(key.encoded_contract_id())
        }
    }
}

/// Best-effort, network-free lookup of a previously-published avatar
/// contract's encoded id (or `None` if an avatar has never been published) -
/// used when a profile save doesn't touch the avatar this call, so the
/// existing key can still be threaded through to `publish_profile_to_network`
/// instead of being silently dropped.
pub fn known_avatar_key(db: &LocalStore) -> Result<Option<String>> {
    Ok(db
        .get_contract_registration("avatar")?
        .map(|(instance_id, code_hash)| {
            contract_key_from_registration(instance_id, code_hash).encoded_contract_id()
        }))
}

/// Pushes an updated, freshly-signed `PublisherProfile` to the network,
/// overwriting `title`/`description`/`avatar_freenet_key`/`updated_at` while
/// preserving whatever `subscription_tiers` the currently-stored state has
/// (nothing sets tiers yet - see `ensure_publisher_identity`'s TODO - but
/// fetching-then-resending mirrors the same pattern `publish_post_to_network`
/// uses for `ContentIndexContract` rather than silently clobbering a field
/// this call doesn't know about).
pub async fn publish_profile_to_network(
    freenet: &FreenetBridge,
    keys: &DelegateKeys,
    identity: &PublisherIdentity,
    title: &str,
    description: &str,
    avatar_freenet_key: Option<String>,
) -> Result<()> {
    let current = freenet
        .get_state(*identity.profile_key.id())
        .await
        .context("fetching current PublisherProfileContract state")?;
    let subscription_tiers = current
        .as_deref()
        .and_then(|bytes| ciborium::from_reader::<PublisherProfile, _>(bytes).ok())
        .map(|p| p.subscription_tiers)
        .unwrap_or_default();

    let mut profile = PublisherProfile {
        author_pubkey: keys.master_signing_verifying_bytes(),
        title: title.to_string(),
        description: description.to_string(),
        avatar_freenet_key,
        subscription_tiers,
        content_index_contract_id: identity.content_index_key.encoded_contract_id(),
        updated_at: now_unix(),
        signature: [0u8; 64],
    };
    let signature = keys.master_signing.sign(&profile.signable_bytes());
    profile.signature = signature.to_bytes();

    let mut buf = Vec::new();
    ciborium::into_writer(&profile, &mut buf)?;
    freenet
        .update_state(identity.profile_key, buf)
        .await
        .context("updating PublisherProfileContract")
}

// ---------------------------------------------------------------------
// SubscriberRegistryContract - publisher-side key delivery (Workflow B,
// design doc §5.2/6.1). Added as part of the NWC/ECDH subscription task;
// deliberately kept separate from `ensure_publisher_identity` above (which
// eagerly mints content_index/profile on every fresh identity) - a
// publisher's registry is only minted lazily, the first time someone
// actually subscribes, since publishing an empty one on every delegate
// startup before anyone has paid would just be dead weight on the network.
// ---------------------------------------------------------------------

/// Deterministically computes a publisher's `SubscriberRegistryContract`
/// instance key from their Ed25519 pubkey alone - **no network round trip,
/// no discovery pointer field needed**. This works because
/// `ContractKey::from_params_and_code` (`freenet_stdlib`) is a pure hash of
/// `(code, params)` - the exact computation `FreenetBridge::put_new` does
/// internally when it mints a new instance (see `WrappedContract::new`) - so
/// any delegate holding the same compiled contract code (embedded in every
/// Aetheria build, same as `PUBLISHER_PROFILE_CONTRACT_WASM` etc. above) and
/// the publisher's public pubkey can independently arrive at the same key a
/// publisher's own `ensure_subscriber_registry` call would produce or reuse.
///
/// Uses the same `author_pubkey` (Ed25519 master signing key) as `Parameters`
/// that `ensure_publisher_identity` uses for `content_index`/
/// `publisher_profile`/`avatar` above, for the same reason: it's the one
/// pubkey a publisher's identity is already keyed on everywhere else, and
/// the one a reader can read straight off a `PublisherProfileContract`.
pub fn subscriber_registry_key_for(publisher_author_pubkey: [u8; 32]) -> Result<ContractKey> {
    let code = load_code(SUBSCRIBER_REGISTRY_CONTRACT_WASM)?;
    let params = Parameters::from(publisher_author_pubkey.to_vec());
    Ok(ContractKey::from_params_and_code(&params, &*code))
}

/// Mint-once (first subscriber ever) + reuse pattern, same shape as
/// `ensure_publisher_identity`'s content_index/profile handling above, but
/// called lazily from the subscription path instead of on every startup.
pub async fn ensure_subscriber_registry(
    freenet: &FreenetBridge,
    db: &LocalStore,
    keys: &DelegateKeys,
) -> Result<ContractKey> {
    let author_pubkey = keys.master_signing_verifying_bytes();
    match db.get_contract_registration("subscriber_registry")? {
        Some((instance_id, code_hash)) => Ok(contract_key_from_registration(instance_id, code_hash)),
        None => {
            let code = load_code(SUBSCRIBER_REGISTRY_CONTRACT_WASM)?;
            let params = Parameters::from(author_pubkey.to_vec());
            let initial = SubscriberRegistryState {
                publication_id: author_pubkey,
                bundles: Vec::new(),
            };
            let mut buf = Vec::new();
            ciborium::into_writer(&initial, &mut buf)?;
            let key = freenet
                .put_new(code, params, buf)
                .await
                .context("publishing initial SubscriberRegistryContract")?;
            db.set_contract_registration(
                "subscriber_registry",
                key.id().as_bytes(),
                key.code_hash().as_ref(),
            )?;
            tracing::info!(
                contract_key = %key.encoded_contract_id(),
                "published SubscriberRegistryContract"
            );
            debug_assert_eq!(
                key.id(),
                subscriber_registry_key_for(author_pubkey)?.id(),
                "put_new's returned key must match the pure local computation - if this ever \
                 fires, subscriber_registry_key_for's discovery-free design assumption is wrong"
            );
            Ok(key)
        }
    }
}

/// Publisher-side half of Workflow B once a payment's been verified:
/// encrypts and appends a per-subscriber `EncryptedKeyBundle`
/// (`crypto::wrap_epoch_key` already produced the ciphertext - this just
/// publishes it). Fetches current state first (a fresh read, not a locally
/// cached copy) so a concurrent bundle from a different subscriber isn't
/// clobbered by a stale full-state resend - same read-modify-write shape
/// `publish_post_to_network` uses for `ContentIndexContract` above.
pub async fn publish_key_bundle_to_network(
    freenet: &FreenetBridge,
    db: &LocalStore,
    keys: &DelegateKeys,
    bundle: EncryptedKeyBundle,
) -> Result<ContractKey> {
    let registry_key = ensure_subscriber_registry(freenet, db, keys).await?;

    let current = freenet
        .get_state(*registry_key.id())
        .await
        .context("fetching current SubscriberRegistryContract state")?;
    let mut state: SubscriberRegistryState = match current {
        Some(bytes) => ciborium::from_reader(bytes.as_slice())
            .context("decoding SubscriberRegistryContract state")?,
        None => SubscriberRegistryState {
            publication_id: keys.master_signing_verifying_bytes(),
            bundles: Vec::new(),
        },
    };
    state.bundles.retain(|b| {
        !(b.subscriber_pubkey == bundle.subscriber_pubkey && b.epoch_id == bundle.epoch_id)
    });
    state.bundles.push(bundle);

    let mut buf = Vec::new();
    ciborium::into_writer(&state, &mut buf)?;
    freenet
        .update_state(registry_key, buf)
        .await
        .context("updating SubscriberRegistryContract")?;
    Ok(registry_key)
}

/// Reader-side lookup: locates `publisher_author_pubkey`'s
/// `SubscriberRegistryContract` (recomputing its key locally - see
/// `subscriber_registry_key_for`, no discovery call needed to find *which*
/// contract to ask) and looks for a bundle issued to `subscriber_pubkey` for
/// `epoch_id`. Purely network-facing - no local DB dependency - since the
/// caller may be a completely different delegate/identity than whichever one
/// published the bundle.
///
/// Not called from `ipc.rs` yet: this milestone's single-identity
/// architecture means `handle_subscribe` never needs to *fetch* a bundle
/// from someone else (subscriber and publisher are the same delegate, so the
/// epoch key is already sitting in its own local `epoch_keys` table the
/// moment `handle_subscribe` generates it). Verified directly instead, by
/// `subscriber_registry_e2e_test.rs`'s independent-identity round trip -
/// wiring this into a real "browse and subscribe to someone else's
/// publication" reader UI is future work once that feature exists at all.
#[allow(dead_code)]
pub async fn fetch_key_bundle(
    freenet: &FreenetBridge,
    publisher_author_pubkey: [u8; 32],
    subscriber_pubkey: [u8; 33],
    epoch_id: u32,
) -> Result<Option<EncryptedKeyBundle>> {
    let registry_key = subscriber_registry_key_for(publisher_author_pubkey)?;
    let current = freenet
        .get_state(*registry_key.id())
        .await
        .context("fetching SubscriberRegistryContract state")?;
    let Some(bytes) = current else {
        return Ok(None);
    };
    let state: SubscriberRegistryState =
        ciborium::from_reader(bytes.as_slice()).context("decoding SubscriberRegistryContract state")?;
    Ok(state
        .bundles
        .into_iter()
        .find(|b| b.subscriber_pubkey == subscriber_pubkey && b.epoch_id == epoch_id))
}

// ---------------------------------------------------------------------
// Following another publisher - reader-side discovery-free lookups.
//
// Every contract key in this app is `ContractKey::from_params_and_code(params,
// code)`, a pure hash with no discovery/pointer field needed (see
// `subscriber_registry_key_for`'s module docs above, and CLAUDE.md). The same
// trick applies here: given nothing but a publisher's Ed25519 `author_pubkey`
// (the same one shown as `publisher_pubkey` in `GetSubscriptionInfo`, and the
// one every other contract in this file already keys `Parameters` on), any
// delegate can independently compute that publisher's `PublisherProfileContract`
// and `ContentIndexContract` keys - the exact same keys `ensure_publisher_identity`
// derives (and, the first time, mints) for *this* delegate's own identity.
// ---------------------------------------------------------------------

fn publisher_profile_key_for(author_pubkey: [u8; 32]) -> Result<ContractKey> {
    let code = load_code(PUBLISHER_PROFILE_CONTRACT_WASM)?;
    let params = Parameters::from(author_pubkey.to_vec());
    Ok(ContractKey::from_params_and_code(&params, &*code))
}

fn content_index_key_for(author_pubkey: [u8; 32]) -> Result<ContractKey> {
    let code = load_code(CONTENT_INDEX_CONTRACT_WASM)?;
    let params = Parameters::from(author_pubkey.to_vec());
    Ok(ContractKey::from_params_and_code(&params, &*code))
}

/// A remote publisher's profile, as fetched and *verified* (not merely
/// trusted) from the real network - see `fetch_remote_profile`.
pub struct RemoteProfile {
    /// Redundant with the caller-supplied argument `fetch_remote_profile` was
    /// called with (already checked equal to it before returning) - kept on
    /// the struct anyway so a caller holding only a `RemoteProfile` (not the
    /// original hex string) still has the pubkey to hand to
    /// `LocalStore::follow_publisher` or similar.
    #[allow(dead_code)]
    pub author_pubkey: [u8; 32],
    pub display_name: String,
    pub bio: String,
    pub avatar_freenet_key: Option<String>,
}

/// Fetches `author_pubkey`'s `PublisherProfileContract` off the real network
/// (no discovery call - the key is a pure local computation, see module docs
/// above) and checks its Ed25519 signature before returning anything -
/// `follow_publisher` uses this to refuse saving a follow for a pubkey with
/// no real (or tampered) profile, rather than trusting arbitrary bytes some
/// peer handed back for that contract slot. `Ok(None)` means "no profile
/// published at that key" (a real, distinct outcome from a network error,
/// same convention as `FreenetBridge::get_state`); a signature mismatch is a
/// hard error, not a `None`, since *something* did answer - it just wasn't
/// trustworthy.
pub async fn fetch_remote_profile(
    freenet: &FreenetBridge,
    author_pubkey: [u8; 32],
) -> Result<Option<RemoteProfile>> {
    let key = publisher_profile_key_for(author_pubkey)?;
    let Some(bytes) = freenet
        .get_state(*key.id())
        .await
        .context("fetching remote PublisherProfileContract state")?
    else {
        return Ok(None);
    };
    let profile: PublisherProfile = ciborium::from_reader(bytes.as_slice())
        .context("decoding remote PublisherProfileContract state")?;
    anyhow::ensure!(
        profile.author_pubkey == author_pubkey,
        "remote profile's author_pubkey field doesn't match the pubkey it was fetched for"
    );

    let verifying_key = VerifyingKey::from_bytes(&author_pubkey)
        .context("author_pubkey is not a valid Ed25519 verifying key")?;
    let signature = Signature::from_bytes(&profile.signature);
    verifying_key
        .verify_strict(&profile.signable_bytes(), &signature)
        .context("remote PublisherProfileContract signature verification failed - refusing to trust it")?;

    Ok(Some(RemoteProfile {
        author_pubkey,
        display_name: profile.title,
        bio: profile.description,
        avatar_freenet_key: profile.avatar_freenet_key,
    }))
}

/// Fetches `author_pubkey`'s `ContentIndexContract` post list off the real
/// network. `Ok(vec![])` covers both "no index published yet" and "published
/// but empty" - neither is an error, both mean "nothing to show from them
/// right now". Each returned header's signature is checked against
/// `author_pubkey`; a header that fails verification is dropped (logged, not
/// propagated as a hard error) rather than failing the whole fetch - one bad
/// or tampered entry in someone else's index shouldn't hide every other real
/// post of theirs.
pub async fn fetch_remote_posts(
    freenet: &FreenetBridge,
    author_pubkey: [u8; 32],
) -> Result<Vec<PostMetadataHeader>> {
    let key = content_index_key_for(author_pubkey)?;
    let Some(bytes) = freenet
        .get_state(*key.id())
        .await
        .context("fetching remote ContentIndexContract state")?
    else {
        return Ok(Vec::new());
    };
    let state: ContentIndexState = ciborium::from_reader(bytes.as_slice())
        .context("decoding remote ContentIndexContract state")?;

    let verifying_key = VerifyingKey::from_bytes(&author_pubkey)
        .context("author_pubkey is not a valid Ed25519 verifying key")?;
    Ok(state
        .posts
        .into_iter()
        .filter(|header| {
            let signature = Signature::from_bytes(&header.signature);
            let ok = verifying_key
                .verify_strict(&header_signable_bytes(header), &signature)
                .is_ok();
            if !ok {
                tracing::warn!(
                    post_id = %hex_encode(&header.post_id),
                    author_pubkey = %hex_encode(&author_pubkey),
                    "dropping remote post header with an invalid signature"
                );
            }
            ok
        })
        .collect())
}

/// Fetches a specific `PostDataContract` instance's raw payload by its
/// encoded (base58) contract id - the exact string `PostMetadataHeader::post_contract_id`
/// stores and `ContractKey::encoded_contract_id` produces elsewhere in this
/// file. `ContractInstanceId::from_base58` (found in `freenet_stdlib::prelude`
/// via the crate's own source under `contract_interface/key.rs` - the
/// `Display`/`encode` half of this round trip is what `encoded_contract_id`
/// already used) is the inverse of that encoding; `FreenetBridge::get_state`
/// only ever needs the instance id, not the full `ContractKey` (code hash
/// included), for a GET, so no code hash needs to be recovered here at all.
///
/// Deliberately does not attempt decryption - a `SubscriberOnly` post's
/// `cipher_text` here is genuine AES-256-GCM ciphertext this delegate has no
/// way to unwrap for someone else's publication (see CLAUDE.md's "Known
/// stub" section on ECDH discovery for other publishers). Callers must check
/// `PostMetadataHeader::access_level` themselves before calling this for
/// content they intend to render as plaintext; a `Public` post's `cipher_text`
/// is the literal markdown bytes with an all-zero nonce, per this app's
/// existing convention (see `publish_post_to_network`).
pub async fn fetch_remote_post_payload(
    freenet: &FreenetBridge,
    post_contract_id: &str,
) -> Result<EncryptedPostPayload> {
    let instance_id = ContractInstanceId::from_base58(post_contract_id)
        .map_err(|e| anyhow::anyhow!("decoding post_contract_id {post_contract_id:?}: {e}"))?;
    let bytes = freenet
        .get_state(instance_id)
        .await
        .context("fetching remote PostDataContract state")?
        .ok_or_else(|| anyhow::anyhow!("post contract not found on the network"))?;
    ciborium::from_reader(bytes.as_slice()).context("decoding remote EncryptedPostPayload")
}

// ---------------------------------------------------------------------
// GlobalDirectoryContract - backs the "Latest" feed (everyone's posts, not
// just followed publishers'). No spec for this in the design doc; added
// because there's otherwise no way to discover a publisher you haven't
// already been given the pubkey for. See global-directory-contract's module
// docs for the full design rationale (bounded to 1000 entries, the closest
// thing to the design doc §7 Sybil-spam mitigation this pass implements).
// ---------------------------------------------------------------------

/// Deterministically computes the network's one `GlobalDirectoryContract`
/// instance key from **empty** `Parameters` - unlike every other key
/// derivation in this file (which scopes to one publisher via their
/// pubkey), this is a single well-known singleton shared by the whole
/// network, so every delegate holding the same compiled code independently
/// arrives at the identical key with no discovery/pointer field, same trick
/// as `subscriber_registry_key_for`.
pub fn global_directory_key() -> Result<ContractKey> {
    let code = load_code(GLOBAL_DIRECTORY_CONTRACT_WASM)?;
    let params = Parameters::from(Vec::new());
    Ok(ContractKey::from_params_and_code(&params, &*code))
}

/// Signs and appends one post's entry to the shared `GlobalDirectoryContract`,
/// bootstrapping it with a fresh PUT if nothing has ever been published there
/// yet (checked via a GET first, not blindly PUT - `put_new` targeting an
/// already-existing key is unexplored territory in this codebase, unlike
/// every other contract here which has exactly one authoritative publisher;
/// this is the one contract multiple independent delegates might race to
/// bootstrap, so checking first minimizes but doesn't eliminate that race -
/// an actual double-PUT collision is left to Freenet's own conflict handling
/// for that key, and the losing delegate's *next* publish will still merge
/// its entry in via the update path below).
#[allow(clippy::too_many_arguments)]
pub async fn publish_to_global_directory(
    freenet: &FreenetBridge,
    keys: &DelegateKeys,
    post_id: [u8; 16],
    author_display_name: &str,
    title: &str,
    summary: &str,
    post_contract_id: String,
    access_level: AccessTier,
    epoch_id: u32,
    published_at: u64,
) -> Result<()> {
    let mut entry = GlobalDirectoryEntry {
        post_id,
        author_pubkey: keys.master_signing_verifying_bytes(),
        author_display_name: author_display_name.to_string(),
        title: title.to_string(),
        summary: summary.to_string(),
        post_contract_id,
        access_level,
        epoch_id,
        published_at,
        signature: [0u8; 64],
    };
    let signature = keys.master_signing.sign(&global_directory_entry_signable_bytes(&entry));
    entry.signature = signature.to_bytes();

    let key = global_directory_key()?;
    let current = freenet
        .get_state(*key.id())
        .await
        .context("fetching current GlobalDirectoryContract state")?;

    match current {
        Some(bytes) => {
            let mut state: GlobalDirectoryState = ciborium::from_reader(bytes.as_slice())
                .context("decoding GlobalDirectoryContract state")?;
            state.entries.retain(|e| e.post_id != entry.post_id);
            state.entries.push(entry);
            state.entries.sort_by(|a, b| b.published_at.cmp(&a.published_at));
            state.entries.truncate(GLOBAL_DIRECTORY_MAX_ENTRIES);

            let mut buf = Vec::new();
            ciborium::into_writer(&state, &mut buf)?;
            freenet
                .update_state(key, buf)
                .await
                .context("updating GlobalDirectoryContract")
        }
        None => {
            let code = load_code(GLOBAL_DIRECTORY_CONTRACT_WASM)?;
            let params = Parameters::from(Vec::new());
            let state = GlobalDirectoryState { entries: vec![entry] };
            let mut buf = Vec::new();
            ciborium::into_writer(&state, &mut buf)?;
            freenet
                .put_new(code, params, buf)
                .await
                .context("publishing initial GlobalDirectoryContract")?;
            Ok(())
        }
    }
}

/// Fetches the shared `GlobalDirectoryContract` and verifies each entry's
/// signature independently against *its own* `author_pubkey` - unlike
/// `fetch_remote_posts` (one publisher, one key to check every header
/// against), this contract holds entries from many different authors, so
/// there's no single verifying key to check the whole state against. An
/// entry that fails verification is dropped (logged, not propagated as a
/// hard error) - same "one bad entry doesn't hide everyone else's real
/// posts" philosophy as `fetch_remote_posts`.
pub async fn fetch_global_directory(freenet: &FreenetBridge) -> Result<Vec<GlobalDirectoryEntry>> {
    let key = global_directory_key()?;
    let Some(bytes) = freenet
        .get_state(*key.id())
        .await
        .context("fetching GlobalDirectoryContract state")?
    else {
        return Ok(Vec::new());
    };
    let state: GlobalDirectoryState = ciborium::from_reader(bytes.as_slice())
        .context("decoding GlobalDirectoryContract state")?;

    Ok(state
        .entries
        .into_iter()
        .filter(|entry| {
            let Ok(verifying_key) = VerifyingKey::from_bytes(&entry.author_pubkey) else {
                tracing::warn!(
                    author_pubkey = %hex_encode(&entry.author_pubkey),
                    "dropping global directory entry with an invalid author_pubkey"
                );
                return false;
            };
            let signature = Signature::from_bytes(&entry.signature);
            let ok = verifying_key
                .verify_strict(&global_directory_entry_signable_bytes(entry), &signature)
                .is_ok();
            if !ok {
                tracing::warn!(
                    post_id = %hex_encode(&entry.post_id),
                    author_pubkey = %hex_encode(&entry.author_pubkey),
                    "dropping global directory entry with an invalid signature"
                );
            }
            ok
        })
        .collect())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
