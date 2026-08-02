//! `SubscriberRegistryContract` — Layer 3 Freenet WASM contract.
//!
//! Append-only set of per-epoch encrypted key bundles (`Ekey,i`), one per
//! (subscriber public key, epoch) pair. The Publisher Delegate appends a new
//! bundle after verifying a Lightning payment preimage; it never removes an
//! entry, so past epochs stay decryptable by subscribers who paid for them
//! even after their subscription lapses (see design doc section 6.2).
//!
//! See `docs/Decentralized_Substack_Design_Doc.pdf` sections 3.4 and 4.2.

use aetheria_types::EncryptedKeyBundle;
use freenet_stdlib::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SubscriberRegistryState {
    pub publication_id: [u8; 32],
    pub bundles: Vec<EncryptedKeyBundle>,
}

impl SubscriberRegistryState {
    fn merge(mut self, other: SubscriberRegistryState) -> SubscriberRegistryState {
        let mut by_key: BTreeMap<([u8; 33], u32), EncryptedKeyBundle> = self
            .bundles
            .drain(..)
            .map(|b| ((b.subscriber_pubkey, b.epoch_id), b))
            .collect();

        for bundle in other.bundles {
            by_key
                .entry((bundle.subscriber_pubkey, bundle.epoch_id))
                .or_insert(bundle);
        }

        SubscriberRegistryState {
            publication_id: self.publication_id,
            bundles: by_key.into_values().collect(),
        }
    }
}

struct SubscriberRegistryContract;

#[contract]
impl ContractInterface for SubscriberRegistryContract {
    fn validate_state(
        _parameters: Parameters<'static>,
        state: State<'static>,
        _related: RelatedContracts<'static>,
    ) -> Result<ValidateResult, ContractError> {
        ciborium::from_reader::<SubscriberRegistryState, _>(state.as_ref())
            .map_err(|e| ContractError::Deser(e.to_string()))?;
        Ok(ValidateResult::Valid)
    }

    fn update_state(
        _parameters: Parameters<'static>,
        state: State<'static>,
        data: Vec<UpdateData<'static>>,
    ) -> Result<UpdateModification<'static>, ContractError> {
        let mut current: SubscriberRegistryState =
            ciborium::from_reader(state.as_ref()).unwrap_or_default();

        for update in data {
            let incoming: SubscriberRegistryState = match update {
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
        let parsed: SubscriberRegistryState = ciborium::from_reader(state.as_ref())
            .map_err(|e| ContractError::Deser(e.to_string()))?;
        // serde's built-in array support tops out at 32 bytes, so the
        // 33-byte compressed pubkey is represented as `Vec<u8>` in this
        // internal summary type (unlike the persisted state, which uses
        // `BigArray` via `EncryptedKeyBundle`).
        let keys: Vec<(Vec<u8>, u32)> = parsed
            .bundles
            .iter()
            .map(|b| (b.subscriber_pubkey.to_vec(), b.epoch_id))
            .collect();

        let mut buf = Vec::new();
        ciborium::into_writer(&keys, &mut buf)
            .map_err(|e| ContractError::Deser(e.to_string()))?;
        Ok(StateSummary::from(buf))
    }

    fn get_state_delta(
        _parameters: Parameters<'static>,
        state: State<'static>,
        summary: StateSummary<'static>,
    ) -> Result<StateDelta<'static>, ContractError> {
        let current: SubscriberRegistryState = ciborium::from_reader(state.as_ref())
            .map_err(|e| ContractError::Deser(e.to_string()))?;
        let known: Vec<(Vec<u8>, u32)> = ciborium::from_reader(summary.as_ref())
            .map_err(|e| ContractError::Deser(e.to_string()))?;

        let delta = SubscriberRegistryState {
            publication_id: current.publication_id,
            bundles: current
                .bundles
                .into_iter()
                .filter(|b| !known.contains(&(b.subscriber_pubkey.to_vec(), b.epoch_id)))
                .collect(),
        };

        let mut buf = Vec::new();
        ciborium::into_writer(&delta, &mut buf)
            .map_err(|e| ContractError::Deser(e.to_string()))?;
        Ok(StateDelta::from(buf))
    }
}
