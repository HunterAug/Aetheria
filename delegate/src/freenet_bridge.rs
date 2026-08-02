//! Bridge to a local Freenet node over its native host protocol (a
//! WebSocket API exposed by `freenet-core` at a local port).
//!
//! Responsible for PUT/GET/subscribe against the four Aetheria contracts
//! (`PublisherProfileContract`, `ContentIndexContract`, `PostDataContract`,
//! `SubscriberRegistryContract`). Confirmed against a real running local
//! node (2026-08-02): the WebSocket API lives at `ws://127.0.0.1:7509/`
//! (root path, not `/contract/command` as originally guessed), default port
//! 7509. Manually verified end-to-end with the `fdev` CLI: built
//! `post-data-contract` with `fdev build`, published a real
//! `EncryptedPostPayload` state with `fdev publish ... contract --state`,
//! and read it back with `fdev execute get` - byte-for-byte round trip.
//!
//! Still `todo!()` here: this struct doesn't yet talk to the node itself.
//! The real client library is `freenet_stdlib::client_api::WebApi` (connects
//! over the same websocket, `send(ClientRequest)` / `recv() -> HostResult`)
//! - swap these stubs for that once the delegate needs to do this
//! programmatically instead of shelling out to `fdev`.

use anyhow::Result;

pub struct FreenetBridge {
    #[allow(dead_code)]
    node_ws_url: String,
}

impl FreenetBridge {
    /// Connect to a Freenet node running on this machine.
    pub async fn connect_local() -> Result<Self> {
        // TODO(Phase 3): replace with `freenet_stdlib::client_api::WebApi`
        // once the delegate actually needs to PUT/GET contract state itself.
        Ok(Self {
            node_ws_url: "ws://127.0.0.1:7509/".to_string(),
        })
    }

    // Not called yet - the current milestone only exercises the publisher's
    // own publish/feed/read loop against the local SQLite cache. These get
    // wired in once a local Freenet node is actually running to develop
    // against (see module docs).
    #[allow(dead_code)]
    pub async fn get_state(&self, contract_id: &str) -> Result<Vec<u8>> {
        let _ = contract_id;
        todo!("Freenet GET not yet implemented")
    }

    #[allow(dead_code)]
    pub async fn put_state(&self, contract_id: &str, state: &[u8]) -> Result<()> {
        let _ = (contract_id, state);
        todo!("Freenet PUT not yet implemented")
    }

    #[allow(dead_code)]
    pub async fn subscribe(&self, contract_id: &str) -> Result<()> {
        let _ = contract_id;
        todo!("Freenet SUBSCRIBE not yet implemented")
    }
}
