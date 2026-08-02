//! Bridge to a local Freenet node over its native host protocol (a
//! WebSocket JSON/binary API exposed by `freenet-core` at a local port).
//!
//! Responsible for PUT/GET/subscribe against the four Aetheria contracts
//! (`PublisherProfileContract`, `ContentIndexContract`, `PostDataContract`,
//! `SubscriberRegistryContract`). Left unimplemented pending a running local
//! Freenet node to develop against — confirm the exact host protocol port
//! and message framing against `freenet-stdlib`'s client API once the Rust
//! toolchain and `freenet` crate are installed locally.

use anyhow::Result;

pub struct FreenetBridge {
    #[allow(dead_code)]
    node_ws_url: String,
}

impl FreenetBridge {
    /// Connect to a Freenet node running on this machine.
    pub async fn connect_local() -> Result<Self> {
        // TODO(Phase 1/2): replace with `freenet_stdlib::client_api` once
        // the local node's websocket address is confirmed.
        Ok(Self {
            node_ws_url: "ws://127.0.0.1:50509/contract/command".to_string(),
        })
    }

    pub async fn get_state(&self, contract_id: &str) -> Result<Vec<u8>> {
        let _ = contract_id;
        todo!("Freenet GET not yet implemented")
    }

    pub async fn put_state(&self, contract_id: &str, state: &[u8]) -> Result<()> {
        let _ = (contract_id, state);
        todo!("Freenet PUT not yet implemented")
    }

    pub async fn subscribe(&self, contract_id: &str) -> Result<()> {
        let _ = contract_id;
        todo!("Freenet SUBSCRIBE not yet implemented")
    }
}
