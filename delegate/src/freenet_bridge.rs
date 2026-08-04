//! Bridge to a local Freenet node over its native host protocol (a
//! WebSocket API exposed by `freenet-core` at a local port).
//!
//! Responsible for PUT/GET/UPDATE/SUBSCRIBE against the Aetheria contracts
//! (`PublisherProfileContract`, `ContentIndexContract`, `PostDataContract`;
//! `SubscriberRegistryContract` is untouched - no real NWC subscriber flow
//! yet, see `nwc.rs`).
//!
//! **Corrected 2026-08-02** from the CLAUDE.md note this module's docs used
//! to carry: the client WebSocket endpoint is
//! `ws://127.0.0.1:7509/v1/contract/command?encodingProtocol=native`, not
//! the root path `/`. Two separate mistakes stacked there:
//!
//! 1. Root path serves the HTML dashboard (`curl -i http://127.0.0.1:7509/`
//!    returns `200` with an HTML body for *any* request, upgrade headers
//!    included) and silently fails a `tokio-tungstenite` handshake with
//!    "HTTP error: 200 OK" instead of the `101 Switching Protocols` a WS
//!    upgrade needs. Confirmed with a raw `curl -i --max-time 3` upgrade
//!    request against the real running node: root path answers `200`
//!    unconditionally, `/v1/contract/command` answers `101`.
//! 2. The path alone gets a `101`, but the node replies to the first real
//!    request with a `HostResult` this crate's `bincode::deserialize` can't
//!    parse ("invalid value: integer `12`, expected `Ok` or `Err`") unless
//!    the query string carries `?encodingProtocol=native` - found by reading
//!    `fdev`'s own connect call (`fdev-0.3.280/src/commands/v1.rs` and
//!    `wasm_runtime/state/v1.rs`, both cached locally in the cargo registry
//!    alongside `freenet-stdlib`'s source), which builds exactly this URL
//!    before handing the stream to the same `WebApi::start`. Without it the
//!    node apparently defaults to a different (flatbuffers) wire encoding
//!    that `regular.rs`'s always-bincode `WebApi` can't decode.
//!
//! The earlier CLAUDE.md note came from reading `freenet_stdlib`'s
//! `client_api` source (which documents the wire *protocol*, not the URL) plus
//! a working `fdev` round trip (which builds this exact URL internally rather
//! than connecting at root) - neither actually exercised the literal
//! connect string a raw `WebApi::start` caller needs to supply itself.
//!
//! Uses `freenet_stdlib::client_api::WebApi` directly - no shelling out to
//! `fdev`. A `ContractRequest::Put` carries the full `ContractContainer`
//! (code + params), so the node never needs the code pre-cached; the
//! compiled WASM bytes are embedded in the delegate binary (see
//! `contracts.rs`) from a build produced by `fdev build` ahead of time,
//! keeping the runtime path free of any dependency on `fdev` being
//! installed or on `PATH`.

use anyhow::{Context, Result};
use freenet_stdlib::client_api::{
    ClientRequest, ContractRequest, ContractResponse, HostResponse, WebApi,
};
use freenet_stdlib::prelude::{
    ContractCode, ContractContainer, ContractInstanceId, ContractKey, ContractWasmAPIVersion,
    Parameters, RelatedContracts, State, UpdateData, WrappedContract, WrappedState,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

pub const NODE_WS_URL: &str = "ws://127.0.0.1:7509/v1/contract/command?encodingProtocol=native";

/// Dev/test escape hatch, same spirit (and same "unset for any normal run"
/// rule) as `AETHERIA_DATA_DIR_OVERRIDE` and `AETHERIA_FREENET_DATA_DIR_OVERRIDE`
/// in `main.rs` / the Tauri shell: point the delegate at a Freenet node on a
/// port other than the default 7509.
///
/// Added while verifying the new-post watcher, for a concrete reason worth
/// recording: the only way to test push notifications is to have a node whose
/// lifecycle the test controls, and on a machine that already runs one on
/// 7509 (the normal state of this dev box) a second node has to live
/// somewhere else. Set it to a full URL including the path and query string -
/// `ws://127.0.0.1:7609/v1/contract/command?encodingProtocol=native` - both of
/// which matter, see this module's docs above for what happens without them.
const NODE_WS_URL_ENV: &str = "AETHERIA_FREENET_WS_URL";

fn node_ws_url() -> String {
    match std::env::var(NODE_WS_URL_ENV) {
        Ok(url) if !url.trim().is_empty() => {
            tracing::warn!(
                url,
                "{NODE_WS_URL_ENV} is set - connecting there instead of the default local node. \
                 Dev/test only."
            );
            url
        }
        _ => NODE_WS_URL.to_string(),
    }
}

/// A single-attempt PUT against the real gateway-routed public network has
/// been observed (2026-08-02, this same node) to fail transiently - "put
/// failed after 1 peer attempt(s) ... awaited peer ... disconnected before
/// replying" and "put timed out after 1 peer attempt(s)" both cleared on a
/// bare retry with no code changes. `freenet-core` itself reports "0
/// infra-retries" for these, so retrying is left to the client. Applied to
/// GET/UPDATE too since both go over the same gateway-routed path.
const MAX_ATTEMPTS: u32 = 4;
const RETRY_DELAY: Duration = Duration::from_millis(1500);

/// `connect_local()`'s own retry budget - distinct from `MAX_ATTEMPTS`/
/// `RETRY_DELAY` above, which govern individual contract *operations* against
/// an already-established connection. This one covers the initial TCP+WS
/// handshake itself, whose failure mode is different: "nothing is listening
/// on 7509 yet" (connection refused), not "the gateway network is flaky".
/// That's the normal state for the first few seconds after the bundled
/// Freenet sidecar (see app/src-tauri/src/main.rs) is spawned - it needs a
/// moment to bind its WebSocket API - and also whenever someone launches the
/// CLI delegate slightly before starting Freenet by hand. A bounded retry
/// loop here absorbs that startup race without a fixed sleep in main.rs (which
/// would either be too short on a slow machine or waste time on a fast one),
/// and still fails outright - not hang forever - if no node shows up at all
/// within the window.
const CONNECT_MAX_ATTEMPTS: u32 = 20;
const CONNECT_RETRY_DELAY: Duration = Duration::from_millis(1500);

pub struct FreenetBridge {
    // A single shared connection, guarded so a send+recv round trip for one
    // request completes atomically w.r.t. any other caller - two interleaved
    // requests on the same socket could otherwise read each other's response.
    api: Mutex<WebApi>,
}

impl FreenetBridge {
    /// Connect to a Freenet node running on this machine, retrying with a
    /// fixed delay for up to `CONNECT_MAX_ATTEMPTS * CONNECT_RETRY_DELAY` (30s)
    /// before giving up - see that constant's doc comment for why this needs
    /// its own retry budget separate from `MAX_ATTEMPTS` above.
    pub async fn connect_local() -> Result<Self> {
        let url = node_ws_url();
        for attempt in 1..=CONNECT_MAX_ATTEMPTS {
            match tokio_tungstenite::connect_async(&url).await {
                Ok((stream, _)) => {
                    if attempt > 1 {
                        tracing::info!(attempt, "connected to Freenet node after retrying");
                    }
                    return Ok(Self {
                        api: Mutex::new(WebApi::start(stream)),
                    });
                }
                Err(e) if attempt < CONNECT_MAX_ATTEMPTS => {
                    tracing::warn!(
                        attempt,
                        error = %e,
                        "Freenet node not reachable yet at {url}, retrying"
                    );
                    tokio::time::sleep(CONNECT_RETRY_DELAY).await;
                }
                Err(e) => {
                    return Err(e).with_context(|| {
                        format!(
                            "connecting to Freenet node at {url} (gave up after {CONNECT_MAX_ATTEMPTS} attempts)"
                        )
                    })
                }
            }
        }
        unreachable!("loop above always returns on its last iteration")
    }

    /// Publishes a brand-new contract instance (code + initial state) and
    /// returns the network-assigned key. Only meaningful the first time a
    /// given (code, params) pair is published - callers are responsible for
    /// remembering the returned key (see `contracts.rs` /
    /// `db::LocalStore::{get,set}_contract_registration`) and using
    /// `update_state` afterwards instead of calling this again.
    pub async fn put_new(
        &self,
        code: Arc<ContractCode<'static>>,
        params: Parameters<'static>,
        initial_state: Vec<u8>,
    ) -> Result<ContractKey> {
        let contract = ContractContainer::Wasm(ContractWasmAPIVersion::V1(WrappedContract::new(
            code, params,
        )));
        let request = ClientRequest::ContractOp(ContractRequest::Put {
            contract,
            state: WrappedState::new(initial_state),
            related_contracts: RelatedContracts::new(),
            subscribe: false,
            blocking_subscribe: false,
        });
        let mut api = self.api.lock().await;

        for attempt in 1..=MAX_ATTEMPTS {
            api.send(request.clone())
                .await
                .map_err(|e| anyhow::anyhow!("sending PUT request: {e}"))?;

            let outcome = loop {
                match api.recv().await {
                    Ok(HostResponse::ContractResponse(ContractResponse::PutResponse { key })) => {
                        break Ok(key)
                    }
                    Ok(other) => {
                        tracing::debug!(?other, "ignoring unrelated host response while awaiting PUT");
                        continue;
                    }
                    Err(e) => break Err(anyhow::anyhow!("PUT failed: {e}")),
                }
            };

            match outcome {
                Ok(key) => return Ok(key),
                Err(e) if attempt < MAX_ATTEMPTS => {
                    tracing::warn!(attempt, error = %e, "PUT failed, retrying");
                    tokio::time::sleep(RETRY_DELAY).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!("loop above always returns on its last iteration")
    }

    /// Overwrites an already-published contract's state.
    ///
    /// Always sends the *full* new state rather than a delta. `ContentIndexContract`'s
    /// `update_state` merges whatever `UpdateData::State` it's given with what
    /// it already has (union by `post_id`, see that contract's `merge`), so a
    /// full-state update is simpler than computing a delta client-side and
    /// idempotent under retries; the tradeoff is re-sending the whole index on
    /// every post instead of just the new entry, acceptable at this milestone's
    /// scale.
    pub async fn update_state(&self, key: ContractKey, new_full_state: Vec<u8>) -> Result<()> {
        let request = ClientRequest::ContractOp(ContractRequest::Update {
            key,
            data: UpdateData::State(State::from(new_full_state)),
        });
        let mut api = self.api.lock().await;

        for attempt in 1..=MAX_ATTEMPTS {
            api.send(request.clone())
                .await
                .map_err(|e| anyhow::anyhow!("sending UPDATE request: {e}"))?;

            let outcome = loop {
                match api.recv().await {
                    Ok(HostResponse::ContractResponse(ContractResponse::UpdateResponse {
                        ..
                    })) => break Ok(()),
                    Ok(other) => {
                        tracing::debug!(
                            ?other,
                            "ignoring unrelated host response while awaiting UPDATE"
                        );
                        continue;
                    }
                    Err(e) => break Err(anyhow::anyhow!("UPDATE failed: {e}")),
                }
            };

            match outcome {
                Ok(()) => return Ok(()),
                Err(e) if attempt < MAX_ATTEMPTS => {
                    tracing::warn!(attempt, error = %e, "UPDATE failed, retrying");
                    tokio::time::sleep(RETRY_DELAY).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!("loop above always returns on its last iteration")
    }

    /// Fetches a contract's current state. `Ok(None)` is the explicit
    /// "contract not found" response, distinct from a network/protocol
    /// error - callers use this to fall back to an empty initial state
    /// rather than treating it as fatal.
    pub async fn get_state(&self, key: ContractInstanceId) -> Result<Option<Vec<u8>>> {
        let request = ClientRequest::ContractOp(ContractRequest::Get {
            key,
            return_contract_code: false,
            subscribe: false,
            blocking_subscribe: false,
        });
        let mut api = self.api.lock().await;

        for attempt in 1..=MAX_ATTEMPTS {
            api.send(request.clone())
                .await
                .map_err(|e| anyhow::anyhow!("sending GET request: {e}"))?;

            let outcome = loop {
                match api.recv().await {
                    Ok(HostResponse::ContractResponse(ContractResponse::GetResponse {
                        state,
                        ..
                    })) => break Ok(Some(state.as_ref().to_vec())),
                    Ok(HostResponse::ContractResponse(ContractResponse::NotFound { .. })) => {
                        break Ok(None)
                    }
                    Ok(other) => {
                        tracing::debug!(
                            ?other,
                            "ignoring unrelated host response while awaiting GET"
                        );
                        continue;
                    }
                    Err(e) => break Err(anyhow::anyhow!("GET failed: {e}")),
                }
            };

            match outcome {
                Ok(value) => return Ok(value),
                Err(e) if attempt < MAX_ATTEMPTS => {
                    tracing::warn!(attempt, error = %e, "GET failed, retrying");
                    tokio::time::sleep(RETRY_DELAY).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!("loop above always returns on its last iteration")
    }

    /// Asks the node to push every future change to `key` back down this
    /// connection as a `ContractResponse::UpdateNotification` (consumed by
    /// `next_update_notification` below; see `watcher.rs` for the only caller
    /// today). Returns the node's own `subscribed` flag rather than swallowing
    /// it - `false` means the node accepted the request but isn't actually
    /// watching that contract, which is a real, reportable outcome and not the
    /// same as an error.
    ///
    /// **Only meaningful on a connection whose `UpdateNotification`s somebody
    /// is actually reading.** Every other method here is a strict
    /// request/response round trip that logs-and-skips anything else arriving
    /// mid-flight ("ignoring unrelated host response"), so a subscription
    /// established on the delegate's *main* bridge would have its pushes
    /// silently discarded by whichever GET/PUT/UPDATE happened to be in
    /// progress. That's why `watcher.rs` opens a second, dedicated
    /// `FreenetBridge` for subscriptions instead of reusing the main one.
    ///
    /// There is deliberately no `unsubscribe` counterpart: `ContractRequest`
    /// (freenet-stdlib 0.8.5) has no such variant - Put/Update/Get/Subscribe
    /// is the whole surface. Dropping the connection is the only way to stop
    /// receiving pushes, which is exactly what `watcher.rs` does when a
    /// publisher is unfollowed.
    pub async fn subscribe(&self, key: ContractInstanceId) -> Result<bool> {
        let request = ClientRequest::ContractOp(ContractRequest::Subscribe {
            key,
            // No summary: this delegate wants whatever the node considers a
            // change, not a delta computed against a state we claim to
            // already hold. `ContentIndexContract::update_state` merges a
            // full state and a delta identically (both decode to a
            // `ContentIndexState`), so the extra bookkeeping a summary would
            // need buys nothing here - see `watcher.rs`'s decoding.
            summary: None,
        });
        let mut api = self.api.lock().await;

        for attempt in 1..=MAX_ATTEMPTS {
            api.send(request.clone())
                .await
                .map_err(|e| anyhow::anyhow!("sending SUBSCRIBE request: {e}"))?;

            let outcome = loop {
                match api.recv().await {
                    Ok(HostResponse::ContractResponse(ContractResponse::SubscribeResponse {
                        subscribed,
                        ..
                    })) => break Ok(subscribed),
                    // A `Subscribe` implicitly GETs the contract if the node
                    // doesn't have it yet (see `ContractRequest::Subscribe`'s
                    // own docs in freenet-stdlib), so a `NotFound` here is a
                    // real answer to this request, not stray traffic: nobody
                    // on the network is holding that contract right now.
                    // Reported as `false` (not subscribed) rather than
                    // retried three more times - a publisher whose index
                    // hasn't propagated yet is an ordinary state on this
                    // network, and the caller re-attempts on its own cadence.
                    Ok(HostResponse::ContractResponse(ContractResponse::NotFound { .. })) => {
                        break Ok(false)
                    }
                    Ok(other) => {
                        tracing::debug!(
                            ?other,
                            "ignoring unrelated host response while awaiting SUBSCRIBE"
                        );
                        continue;
                    }
                    Err(e) => break Err(anyhow::anyhow!("SUBSCRIBE failed: {e}")),
                }
            };

            match outcome {
                Ok(subscribed) => return Ok(subscribed),
                Err(e) if attempt < MAX_ATTEMPTS => {
                    tracing::warn!(attempt, error = %e, "SUBSCRIBE failed, retrying");
                    tokio::time::sleep(RETRY_DELAY).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!("loop above always returns on its last iteration")
    }

    /// Blocks until the node pushes the next `UpdateNotification` for *any*
    /// contract this connection has subscribed to, and returns which contract
    /// it was for plus the update payload.
    ///
    /// Unlike every other method here this is not a request/response pair -
    /// nothing is sent, and it can wait indefinitely - so it holds the
    /// connection's mutex for as long as it's pending. That is only safe on a
    /// bridge dedicated to watching (see `subscribe`'s docs): any other
    /// caller's GET on the same bridge would block until an unrelated
    /// publisher happened to publish something. `watcher.rs` owns such a
    /// bridge exclusively.
    ///
    /// # Cancel safety
    ///
    /// Dropping this future is how `watcher.rs` reacts to a follow/unfollow
    /// without waiting for a notification first, so it needs to be safe in
    /// the ways that matter here, and it mostly is: a notification already
    /// queued inside `WebApi` stays queued (its internal channel receive is
    /// cancel-safe), and the mutex guard is released. The one documented gap
    /// is `WebApi::recv`'s own: a *streamed* response (one large enough for
    /// the node to chunk it, >64 KiB in freenet-stdlib 0.8.5) that is
    /// mid-reassembly when the future is dropped is lost. A `ContentIndexState`
    /// push is far smaller than that in any realistic case, and `watcher.rs`
    /// polls each followed publisher's index on a timer regardless, so a lost
    /// push costs at most one polling interval of latency rather than a
    /// missed post.
    pub async fn next_update_notification(&self) -> Result<(ContractKey, UpdateData<'static>)> {
        let mut api = self.api.lock().await;
        loop {
            match api.recv().await {
                Ok(HostResponse::ContractResponse(ContractResponse::UpdateNotification {
                    key,
                    update,
                })) => return Ok((key, update)),
                Ok(other) => {
                    tracing::debug!(
                        ?other,
                        "ignoring non-notification host response on the subscription connection"
                    );
                    continue;
                }
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "subscription connection failed while awaiting an update notification: {e}"
                    ))
                }
            }
        }
    }
}
