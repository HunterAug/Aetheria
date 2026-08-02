//! `ContentIndexContract` — Layer 3 Freenet WASM contract.
//!
//! Append-only CRDT table of contents for a publication. Peers merge posts
//! by `post_id` (first-write-wins, since a post header is immutable once
//! signed) and the merged `last_sequence_num` is the max seen across peers.
//!
//! Per-post signature authenticity (the header's `signature` field) should be
//! checked against the publisher's key from the linked `PublisherProfileContract`
//! via `RelatedContracts` — left as a TODO until the related-contract lookup
//! API is wired up against a running Freenet node.
//!
//! See `docs/Decentralized_Substack_Design_Doc.pdf` section 3.2.

use aetheria_types::PostMetadataHeader;
use freenet_stdlib::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContentIndexState {
    pub publication_id: [u8; 32],
    pub posts: Vec<PostMetadataHeader>,
    pub last_sequence_num: u64,
}

impl ContentIndexState {
    fn merge(mut self, other: ContentIndexState) -> ContentIndexState {
        let mut by_id: BTreeMap<[u8; 16], PostMetadataHeader> = self
            .posts
            .drain(..)
            .map(|p| (p.post_id, p))
            .collect();

        for post in other.posts {
            by_id.entry(post.post_id).or_insert(post);
        }

        let mut posts: Vec<_> = by_id.into_values().collect();
        posts.sort_by_key(|p| p.published_at);

        ContentIndexState {
            publication_id: self.publication_id,
            posts,
            last_sequence_num: self.last_sequence_num.max(other.last_sequence_num),
        }
    }
}

struct ContentIndexContract;

#[contract]
impl ContractInterface for ContentIndexContract {
    fn validate_state(
        _parameters: Parameters<'static>,
        state: State<'static>,
        _related: RelatedContracts<'static>,
    ) -> Result<ValidateResult, ContractError> {
        let parsed: ContentIndexState = ciborium::from_reader(state.as_ref())
            .map_err(|e| ContractError::Deser(e.to_string()))?;

        // Structural check: sequence numbers must not regress below the
        // count of posts recorded so far. Full signature verification
        // against the publisher key is a TODO (see module docs).
        if (parsed.posts.len() as u64) > parsed.last_sequence_num + 1 {
            return Ok(ValidateResult::Invalid);
        }
        Ok(ValidateResult::Valid)
    }

    fn update_state(
        _parameters: Parameters<'static>,
        state: State<'static>,
        data: Vec<UpdateData<'static>>,
    ) -> Result<UpdateModification<'static>, ContractError> {
        let mut current: ContentIndexState =
            ciborium::from_reader(state.as_ref()).unwrap_or_default();

        for update in data {
            let incoming: ContentIndexState = match update {
                UpdateData::State(s) => ciborium::from_reader(s.as_ref())
                    .map_err(|e| ContractError::Deser(e.to_string()))?,
                UpdateData::Delta(d) => ciborium::from_reader(d.as_ref())
                    .map_err(|e| ContractError::Deser(e.to_string()))?,
                _ => continue,
            };
            current = current.merge(incoming);
        }

        let mut buf = Vec::new();
        ciborium::into_writer(&current, &mut buf)
            .map_err(|e| ContractError::Deser(e.to_string()))?;
        Ok(UpdateModification::valid(State::from(buf)))
    }

    fn summarize_state(
        _parameters: Parameters<'static>,
        state: State<'static>,
    ) -> Result<StateSummary<'static>, ContractError> {
        let parsed: ContentIndexState = ciborium::from_reader(state.as_ref())
            .map_err(|e| ContractError::Deser(e.to_string()))?;
        let post_ids: Vec<[u8; 16]> = parsed.posts.iter().map(|p| p.post_id).collect();

        let mut buf = Vec::new();
        ciborium::into_writer(&(post_ids, parsed.last_sequence_num), &mut buf)
            .map_err(|e| ContractError::Deser(e.to_string()))?;
        Ok(StateSummary::from(buf))
    }

    fn get_state_delta(
        _parameters: Parameters<'static>,
        state: State<'static>,
        summary: StateSummary<'static>,
    ) -> Result<StateDelta<'static>, ContractError> {
        let current: ContentIndexState = ciborium::from_reader(state.as_ref())
            .map_err(|e| ContractError::Deser(e.to_string()))?;
        let (known_ids, _last_seq): (Vec<[u8; 16]>, u64) =
            ciborium::from_reader(summary.as_ref())
                .map_err(|e| ContractError::Deser(e.to_string()))?;

        let delta = ContentIndexState {
            publication_id: current.publication_id,
            posts: current
                .posts
                .into_iter()
                .filter(|p| !known_ids.contains(&p.post_id))
                .collect(),
            last_sequence_num: current.last_sequence_num,
        };

        let mut buf = Vec::new();
        ciborium::into_writer(&delta, &mut buf)
            .map_err(|e| ContractError::Deser(e.to_string()))?;
        Ok(StateDelta::from(buf))
    }
}
