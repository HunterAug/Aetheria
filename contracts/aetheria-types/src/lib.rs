//! Shared data types referenced by more than one Aetheria Freenet contract.
//!
//! Aetheria has no payments or subscriptions: every post is public, so
//! there's no access-tier or encryption-key machinery here. See
//! `docs/Decentralized_Substack_Design_Doc.pdf` section 3 for the original
//! contract schemas this crate implements the (subscription-free) parts of.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

// serde's built-in array support only covers lengths 0-32; the 64-byte
// signature field below needs `BigArray`.

/// One entry in a publication's append-only `ContentIndexContract` log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostMetadataHeader {
    /// UUID v4 identifier.
    pub post_id: [u8; 16],
    /// Article title.
    pub title: String,
    /// Teaser/preview snippet.
    pub summary: String,
    /// Freenet contract ID containing the post's payload.
    pub post_contract_id: String,
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
    pub blob_contract_id: String,
}

/// An article's payload, as stored in a `PostDataContract`. Every post is
/// public - `content` is the literal Markdown (or, for an avatar instance,
/// image) bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostPayload {
    pub post_id: [u8; 16],
    pub content: Vec<u8>,
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
    pub published_at: u64,
    /// Ed25519 signature over the entry bytes, by `author_pubkey`'s matching
    /// signing key - verified independently per-entry by whoever fetches
    /// this contract, since (unlike a per-publication index) many different
    /// authors write into the one shared state here.
    #[serde(with = "BigArray")]
    pub signature: [u8; 64],
}
