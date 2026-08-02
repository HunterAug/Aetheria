//! Nostr Wallet Connect (NIP-47) client stub.
//!
//! Drives the Lightning payment side of Workflow B: requesting an invoice
//! as a reader, or listening for a payment preimage as a publisher. Real
//! implementation needs a Nostr relay connection (NIP-04/44 encrypted DMs)
//! plus a `nostr` client crate — left unimplemented pending Phase 3.
//!
//! See design doc sections 5.2 and 6.1.

use anyhow::Result;

pub struct NwcClient {
    connection_uri: Option<String>,
}

impl NwcClient {
    pub fn disconnected() -> Self {
        Self {
            connection_uri: None,
        }
    }

    /// Connect using a `nostr+walletconnect://...` URI exported from a
    /// wallet such as Alby, Mutiny, or Phoenix.
    pub async fn connect(&mut self, uri: &str) -> Result<()> {
        self.connection_uri = Some(uri.to_string());
        // TODO(Phase 3): open relay connection, verify wallet pubkey.
        todo!("NWC relay connection not yet implemented")
    }

    /// Request a Lightning invoice for a subscription payment.
    ///
    /// Mirrors the `make_invoice` NIP-47 method shown in design doc 6.1.
    pub async fn make_invoice(&self, amount_msat: u64, description: &str) -> Result<String> {
        let _ = (amount_msat, description);
        todo!("NIP-47 make_invoice RPC not yet implemented")
    }

    /// Poll/subscribe for the preimage confirming a previously requested
    /// invoice was paid.
    pub async fn wait_for_preimage(&self, invoice: &str) -> Result<String> {
        let _ = invoice;
        todo!("NIP-47 payment notification listener not yet implemented")
    }
}
