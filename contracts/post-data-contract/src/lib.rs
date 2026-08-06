//! `PostDataContract` — Layer 3 Freenet WASM contract.
//!
//! Holds a single article's markdown payload. Every post is public, so
//! `content` is plain bytes - no encryption. The contract is write-once:
//! once a payload is set it cannot be replaced, matching the append-only
//! publication model described in
//! `docs/Decentralized_Substack_Design_Doc.pdf` section 3.3.

use aetheria_types::PostPayload;
use freenet_stdlib::prelude::*;

struct PostDataContract;

#[contract]
impl ContractInterface for PostDataContract {
    fn validate_state(
        _parameters: Parameters<'static>,
        state: State<'static>,
        _related: RelatedContracts<'static>,
    ) -> Result<ValidateResult, ContractError> {
        let payload: PostPayload = ciborium::from_reader(state.as_ref())
            .map_err(|e| ContractError::Deser(e.to_string()))?;

        if payload.content.is_empty() {
            return Ok(ValidateResult::Invalid);
        }
        Ok(ValidateResult::Valid)
    }

    fn update_state(
        _parameters: Parameters<'static>,
        state: State<'static>,
        data: Vec<UpdateData<'static>>,
    ) -> Result<UpdateModification<'static>, ContractError> {
        // Write-once: an already-populated payload cannot be overwritten.
        let existing: Option<PostPayload> = ciborium::from_reader(state.as_ref()).ok();
        if existing.is_some() {
            return Ok(UpdateModification::valid(state));
        }

        for update in data {
            if let UpdateData::State(s) = update {
                let payload: PostPayload = ciborium::from_reader(s.as_ref())
                    .map_err(|e| ContractError::Deser(e.to_string()))?;
                let mut buf = Vec::new();
                ciborium::into_writer(&payload, &mut buf)
                    .map_err(|e| ContractError::Deser(e.to_string()))?;
                return Ok(UpdateModification::valid(State::from(buf)));
            }
        }

        Err(ContractError::InvalidUpdate)
    }

    fn summarize_state(
        _parameters: Parameters<'static>,
        state: State<'static>,
    ) -> Result<StateSummary<'static>, ContractError> {
        // The whole payload is small relative to a summary/delta round-trip
        // for a single post, so the summary is just a presence flag.
        let present = !state.as_ref().is_empty();
        Ok(StateSummary::from(vec![present as u8]))
    }

    fn get_state_delta(
        _parameters: Parameters<'static>,
        state: State<'static>,
        summary: StateSummary<'static>,
    ) -> Result<StateDelta<'static>, ContractError> {
        let known = summary.as_ref().first().copied().unwrap_or(0) == 1;
        if known {
            Ok(StateDelta::from(Vec::new()))
        } else {
            Ok(StateDelta::from(state.as_ref().to_vec()))
        }
    }
}
