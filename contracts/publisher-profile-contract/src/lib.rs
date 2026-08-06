//! `PublisherProfileContract` — Layer 3 Freenet WASM contract.
//!
//! Holds a single publisher's public identity, publication metadata, and a
//! pointer to their `ContentIndexContract`. State updates are only accepted
//! when signed by the publisher's Ed25519 master key and newer than the
//! currently stored state (last-writer-wins on `updated_at`).
//!
//! See `docs/Decentralized_Substack_Design_Doc.pdf` section 3.1.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use freenet_stdlib::prelude::*;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublisherProfile {
    /// Ed25519 public key.
    pub author_pubkey: [u8; 32],
    pub title: String,
    pub description: String,
    /// Freenet key pointing to an avatar media contract.
    pub avatar_freenet_key: Option<String>,
    /// Freenet contract ID for this publication's article index.
    pub content_index_contract_id: String,
    pub updated_at: u64,
    /// Ed25519 signature over the state with `signature` zeroed.
    /// serde's built-in array support only covers lengths 0-32, so this
    /// 64-byte signature needs `BigArray`.
    #[serde(with = "BigArray")]
    pub signature: [u8; 64],
}

impl PublisherProfile {
    fn signable_bytes(&self) -> Vec<u8> {
        let mut unsigned = self.clone();
        unsigned.signature = [0u8; 64];
        let mut buf = Vec::new();
        ciborium::into_writer(&unsigned, &mut buf).expect("serialization is infallible");
        buf
    }

    fn verify_signature(&self) -> bool {
        let Ok(verifying_key) = VerifyingKey::from_bytes(&self.author_pubkey) else {
            return false;
        };
        let signature = Signature::from_bytes(&self.signature);
        verifying_key
            .verify(&self.signable_bytes(), &signature)
            .is_ok()
    }
}

struct PublisherProfileContract;

#[contract]
impl ContractInterface for PublisherProfileContract {
    fn validate_state(
        _parameters: Parameters<'static>,
        state: State<'static>,
        _related: RelatedContracts<'static>,
    ) -> Result<ValidateResult, ContractError> {
        let profile: PublisherProfile = ciborium::from_reader(state.as_ref())
            .map_err(|e| ContractError::Deser(e.to_string()))?;

        if !profile.verify_signature() {
            return Ok(ValidateResult::Invalid);
        }
        Ok(ValidateResult::Valid)
    }

    fn update_state(
        _parameters: Parameters<'static>,
        state: State<'static>,
        data: Vec<UpdateData<'static>>,
    ) -> Result<UpdateModification<'static>, ContractError> {
        let mut current: Option<PublisherProfile> = ciborium::from_reader(state.as_ref()).ok();

        for update in data {
            let candidate: PublisherProfile = match update {
                UpdateData::State(s) => ciborium::from_reader(s.as_ref())
                    .map_err(|e| ContractError::Deser(e.to_string()))?,
                _ => continue,
            };

            if !candidate.verify_signature() {
                continue;
            }

            let should_replace = match &current {
                Some(existing) => candidate.updated_at > existing.updated_at,
                None => true,
            };
            if should_replace {
                current = Some(candidate);
            }
        }

        let Some(final_profile) = current else {
            return Err(ContractError::InvalidUpdate);
        };

        let mut buf = Vec::new();
        ciborium::into_writer(&final_profile, &mut buf)
            .map_err(|e| ContractError::Deser(e.to_string()))?;
        Ok(UpdateModification::valid(State::from(buf)))
    }

    fn summarize_state(
        _parameters: Parameters<'static>,
        state: State<'static>,
    ) -> Result<StateSummary<'static>, ContractError> {
        // Profile state is small; the summary is the full state.
        Ok(StateSummary::from(state.as_ref().to_vec()))
    }

    fn get_state_delta(
        _parameters: Parameters<'static>,
        state: State<'static>,
        _summary: StateSummary<'static>,
    ) -> Result<StateDelta<'static>, ContractError> {
        Ok(StateDelta::from(state.as_ref().to_vec()))
    }
}
