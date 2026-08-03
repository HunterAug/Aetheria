//! `GlobalDirectoryContract` — Layer 3 Freenet WASM contract.
//!
//! Not part of the original design doc's contract set (§3): a single,
//! globally-shared, well-known-key CRDT list of the most recent posts across
//! *every* publisher, capped at `MAX_ENTRIES` (newest kept, oldest evicted).
//! Backs the app's "Latest" feed - unlike `ContentIndexContract` (one
//! instance per publication, keyed on that publisher's pubkey), every
//! delegate derives the *same* instance key for this contract from empty
//! `Parameters` (see `delegate/src/contracts.rs::global_directory_key`), so
//! there's exactly one of these on the whole network and any publisher's
//! delegate can append to it with no discovery step.
//!
//! Merge semantics mirror `ContentIndexContract`'s (dedupe by `post_id`,
//! first-write-wins since an entry is immutable once signed) with two
//! differences: sorted **newest-first** (`ContentIndexContract` sorts
//! oldest-first, since it's a per-publication chronological log; this is a
//! "what's fresh right now" feed) and truncated to `MAX_ENTRIES` after every
//! merge, so the shared state can't grow without bound as more publishers
//! use it - the closest thing this contract has to the design doc §7 Sybil-
//! spam mitigation it never specifies (proof-of-work/payment gating is still
//! a TODO, see CLAUDE.md).
//!
//! Per-entry signature authenticity (the `signature` field, keyed on each
//! entry's own `author_pubkey` rather than one publisher's key like
//! `ContentIndexContract`) is checked delegate-side
//! (`contracts.rs::fetch_global_directory`), not here - same reasoning as
//! `ContentIndexContract`'s module docs: no related-contract-lookup key
//! server is wired up against a running node yet.

use aetheria_types::GlobalDirectoryEntry;
use freenet_stdlib::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Newest entries are kept, oldest evicted once a merge exceeds this - see
/// module docs for why this is the only spam mitigation this contract has
/// today.
const MAX_ENTRIES: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalDirectoryState {
    pub entries: Vec<GlobalDirectoryEntry>,
}

impl GlobalDirectoryState {
    fn merge(mut self, other: GlobalDirectoryState) -> GlobalDirectoryState {
        let mut by_id: BTreeMap<[u8; 16], GlobalDirectoryEntry> = self
            .entries
            .drain(..)
            .map(|e| (e.post_id, e))
            .collect();

        for entry in other.entries {
            by_id.entry(entry.post_id).or_insert(entry);
        }

        let mut entries: Vec<_> = by_id.into_values().collect();
        entries.sort_by(|a, b| b.published_at.cmp(&a.published_at));
        entries.truncate(MAX_ENTRIES);

        GlobalDirectoryState { entries }
    }
}

struct GlobalDirectoryContract;

#[contract]
impl ContractInterface for GlobalDirectoryContract {
    fn validate_state(
        _parameters: Parameters<'static>,
        state: State<'static>,
        _related: RelatedContracts<'static>,
    ) -> Result<ValidateResult, ContractError> {
        let parsed: GlobalDirectoryState = ciborium::from_reader(state.as_ref())
            .map_err(|e| ContractError::Deser(e.to_string()))?;

        // Structural check only, matching ContentIndexContract's approach -
        // full per-entry signature verification is a delegate-side concern
        // (see module docs).
        if parsed.entries.len() > MAX_ENTRIES {
            return Ok(ValidateResult::Invalid);
        }
        Ok(ValidateResult::Valid)
    }

    fn update_state(
        _parameters: Parameters<'static>,
        state: State<'static>,
        data: Vec<UpdateData<'static>>,
    ) -> Result<UpdateModification<'static>, ContractError> {
        let mut current: GlobalDirectoryState =
            ciborium::from_reader(state.as_ref()).unwrap_or_default();

        for update in data {
            let incoming: GlobalDirectoryState = match update {
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
        let parsed: GlobalDirectoryState = ciborium::from_reader(state.as_ref())
            .map_err(|e| ContractError::Deser(e.to_string()))?;
        let post_ids: Vec<[u8; 16]> = parsed.entries.iter().map(|e| e.post_id).collect();

        let mut buf = Vec::new();
        ciborium::into_writer(&post_ids, &mut buf)
            .map_err(|e| ContractError::Deser(e.to_string()))?;
        Ok(StateSummary::from(buf))
    }

    fn get_state_delta(
        _parameters: Parameters<'static>,
        state: State<'static>,
        summary: StateSummary<'static>,
    ) -> Result<StateDelta<'static>, ContractError> {
        let current: GlobalDirectoryState = ciborium::from_reader(state.as_ref())
            .map_err(|e| ContractError::Deser(e.to_string()))?;
        let known_ids: Vec<[u8; 16]> = ciborium::from_reader(summary.as_ref())
            .map_err(|e| ContractError::Deser(e.to_string()))?;

        let delta = GlobalDirectoryState {
            entries: current
                .entries
                .into_iter()
                .filter(|e| !known_ids.contains(&e.post_id))
                .collect(),
        };

        let mut buf = Vec::new();
        ciborium::into_writer(&delta, &mut buf)
            .map_err(|e| ContractError::Deser(e.to_string()))?;
        Ok(StateDelta::from(buf))
    }
}
