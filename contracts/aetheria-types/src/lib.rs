//! Shared data types referenced by more than one Aetheria Freenet contract.
//!
//! See `docs/Decentralized_Substack_Design_Doc.pdf` sections 3-4 for the
//! authoritative schema and cryptographic flow this crate implements.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

// serde's built-in array support only covers lengths 0-32; the 64-byte
// signature and 33-byte compressed-pubkey fields below need `BigArray`.

/// A subscription pricing tier offered by a publisher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tier {
    pub tier_id: u8,
    pub name: String,
    pub price_sats_per_month: u64,
    pub features: Vec<String>,
}

/// Access restriction applied to a post at publish time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessTier {
    Public,
    SubscriberOnly { required_tier_id: u8 },
}

/// One entry in a publication's append-only `ContentIndexContract` log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostMetadataHeader {
    /// UUID v4 identifier.
    pub post_id: [u8; 16],
    /// Article title (unencrypted).
    pub title: String,
    /// Unencrypted teaser/preview snippet.
    pub summary: String,
    /// Freenet contract ID containing the encrypted payload.
    pub post_contract_id: String,
    pub access_level: AccessTier,
    /// Epoch required for decryption key derivation.
    pub epoch_id: u32,
    pub published_at: u64,
    /// Ed25519 signature over the header bytes, by the publisher's master key.
    #[serde(with = "BigArray")]
    pub signature: [u8; 64],
}

/// Reference to a media file embedded in a post.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaAttachment {
    pub attachment_id: String,
    pub mime_type: String,
    pub encrypted_blob_contract_id: String,
}

/// AES-256-GCM encrypted article payload stored in a `PostDataContract`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedPostPayload {
    pub post_id: [u8; 16],
    /// AES-256-GCM encrypted Markdown payload.
    pub cipher_text: Vec<u8>,
    /// Unique GCM nonce.
    pub nonce: [u8; 12],
    /// Cryptographic authentication tag.
    pub auth_tag: [u8; 16],
    pub attachments: Vec<MediaAttachment>,
}

/// One entry in the network-wide `GlobalDirectoryContract`'s bounded
/// recent-posts list (backs the "Latest" feed - everyone's posts, not just
/// followed publishers'). Not part of the original design doc's contract
/// set: unlike `PostMetadataHeader` (scoped to one publication's own
/// `ContentIndexContract`), this is a single shared, globally-writable
/// contract many different publishers' delegates append to, so each entry
/// has to carry its own author identity rather than relying on the
/// containing contract's `publication_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalDirectoryEntry {
    pub post_id: [u8; 16],
    /// Ed25519 pubkey of whoever published this post.
    pub author_pubkey: [u8; 32],
    pub author_display_name: String,
    pub title: String,
    pub summary: String,
    pub post_contract_id: String,
    pub access_level: AccessTier,
    pub epoch_id: u32,
    pub published_at: u64,
    /// Ed25519 signature over the entry bytes, by `author_pubkey`'s matching
    /// signing key - verified independently per-entry by whoever fetches
    /// this contract, since (unlike a per-publication index) many different
    /// authors write into the one shared state here.
    #[serde(with = "BigArray")]
    pub signature: [u8; 64],
}

/// Encrypted Kepoch bundle for a single subscriber, produced via
/// ECDH(SKpub, PKsub,i) and stored in the `SubscriberRegistryContract`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedKeyBundle {
    /// Secp256k1 public key of the subscriber this bundle was encrypted for.
    #[serde(with = "BigArray")]
    pub subscriber_pubkey: [u8; 33],
    pub epoch_id: u32,
    /// AES-256-GCM(Si, nonce_k, Kepoch).
    pub cipher_text: Vec<u8>,
    pub nonce: [u8; 12],
    pub auth_tag: [u8; 16],
    /// Unix timestamp this bundle was appended to the registry.
    pub issued_at: u64,
}
