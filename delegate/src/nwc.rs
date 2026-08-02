//! Nostr Wallet Connect (NIP-47) client.
//!
//! Drives the Lightning payment side of Workflow B (design doc §5.2/6.1):
//! requesting an invoice, paying one, and verifying settlement.
//!
//! ## Which crate, and why
//!
//! Uses `nwc` (crate.io, rust-nostr project), pinned to the stable `0.44.0`
//! release - as of 2026-08, the project's `0.45.x` line is alpha-only
//! (`0.45.0-alpha.8`, checked against crates.io's version list), and this is
//! a payment-handling crate, so a stable release was chosen over chasing the
//! newer alpha API (which had already renamed `NostrWalletConnectURI` ->
//! `NostrWalletConnectUri` and restructured construction as
//! `NostrWalletConnect::builder(uri).build()` by that point - a real API
//! churn risk, not just a version bump). The sibling `nostr-sdk` crate was
//! *not* used here: `nwc` already re-exports everything needed (`nostr`'s
//! NIP-47 types) through its `prelude`, and only implements the *client*
//! side of NIP-47 (the party that talks to a wallet service) - which is
//! exactly the role this module needs. `nostr-sdk` is used separately, as a
//! dev-dependency, to build a mock *wallet service* for protocol testing
//! (see `delegate/examples/nwc_protocol_check.rs`) - `nwc` has no
//! server-side API since it isn't meant to implement a wallet.
//!
//! ## Who calls what (worked out from NIP-47, not the design doc's wire
//! sketch - see module docs on that caveat below)
//!
//! The design doc's §6.1 "Example NWC Invoice Request Payload from Reader
//! Delegate" is misleading about direction: NIP-47's `make_invoice` is a
//! request *to a wallet service you already control* asking it to generate
//! an invoice for money you want to *receive* - it is not how a reader asks
//! a publisher for an invoice. Reading the real spec
//! (<https://github.com/nostr-protocol/nips/blob/master/47.md>) and the
//! `nwc` crate's own source (`NWC::make_invoice`/`pay_invoice`/
//! `lookup_invoice`, all plain RPCs against whichever wallet the `NWC`
//! client is constructed with) clarifies the actual roles:
//!
//! - **Publisher** (receiving payment): connects `NwcClient` to *their own*
//!   receiving wallet, calls [`NwcClient::make_invoice`] to mint an invoice,
//!   then [`NwcClient::wait_for_preimage`] (polls `lookup_invoice`) to detect
//!   settlement independently rather than trusting a payer's claim.
//! - **Reader** (paying a subscription): connects a *different* `NwcClient`
//!   instance to *their own* paying wallet, calls [`NwcClient::pay_invoice`]
//!   with the bolt11 string the publisher produced.
//!
//! Both roles use the same `NwcClient` type - which RPC method gets called
//! is what determines the role, not the type. A single Aetheria user with
//! one connected wallet can legitimately play both roles (mint invoices to
//! receive subscription payments to their own publication, and pay other
//! publications' invoices to subscribe to them) - see `ipc.rs`'s `Subscribe`
//! handler, which is presently the *only* caller and (in this milestone's
//! single-identity architecture) exercises both roles against the same
//! connected wallet.
//!
//! `wait_for_preimage` is implemented via polling `lookup_invoice` rather
//! than NIP-47's optional real-time notification extension (kind 23196) -
//! `lookup_invoice` is part of the base spec every wallet service is
//! expected to support, whereas notifications are an add-on some wallets
//! may not implement; polling is simpler and universally compatible, at the
//! cost of a few seconds' extra latency, an acceptable trade for this
//! milestone.

use anyhow::{anyhow, Context, Result};
use nwc::prelude::*;
use std::str::FromStr;
use std::time::Duration;

/// A freshly-minted invoice: the bolt11 string to hand to whoever's paying,
/// plus the payment hash used to poll for settlement afterwards.
#[derive(Debug, Clone)]
pub struct WalletInvoice {
    pub bolt11: String,
    pub payment_hash: String,
}

pub struct NwcClient {
    inner: Option<NWC>,
}

impl NwcClient {
    pub fn disconnected() -> Self {
        Self { inner: None }
    }

    pub fn is_connected(&self) -> bool {
        self.inner.is_some()
    }

    /// Connect using a `nostr+walletconnect://...` URI exported from a
    /// wallet such as Alby, Mutiny, Phoenix, or Umbrel.
    ///
    /// Verifies the connection actually works by calling NIP-47's `get_info`
    /// once - every wallet service is required to support it, so a failure
    /// here is a real "this connection string doesn't work" signal (wrong
    /// secret, unreachable relay, wallet offline), not the kind of transient
    /// hiccup `freenet_bridge.rs` retries - so it's surfaced immediately
    /// rather than stored and discovered later on first real use.
    pub async fn connect(&mut self, uri_str: &str) -> Result<()> {
        let uri = NostrWalletConnectURI::from_str(uri_str.trim())
            .map_err(|e| anyhow!("parsing NWC connection string: {e}"))?;
        let client = NWC::new(uri);
        client
            .get_info()
            .await
            .map_err(|e| anyhow!("{e}"))
            .context(
                "wallet did not answer NIP-47 get_info - check the connection string and that \
                 the wallet/relay are reachable",
            )?;
        self.inner = Some(client);
        Ok(())
    }

    fn client(&self) -> Result<&NWC> {
        self.inner
            .as_ref()
            .ok_or_else(|| anyhow!("no wallet connected - call connect_wallet first"))
    }

    /// Request a Lightning invoice, e.g. for a subscription payment. Mirrors
    /// NIP-47's `make_invoice` - called by whoever wants to *receive*
    /// payment (the publisher), against their own connected wallet.
    pub async fn make_invoice(&self, amount_msat: u64, description: &str) -> Result<WalletInvoice> {
        let client = self.client()?;
        let response = client
            .make_invoice(MakeInvoiceRequest {
                amount: amount_msat,
                description: Some(description.to_string()),
                description_hash: None,
                // 1 hour - generous enough for a human to approve a payment
                // in their wallet UI, short enough not to leave stale
                // pending invoices around indefinitely.
                expiry: Some(3600),
            })
            .await
            .map_err(|e| anyhow!("{e}"))
            .context("NIP-47 make_invoice failed")?;
        let payment_hash = response.payment_hash.clone().ok_or_else(|| {
            anyhow!("wallet's make_invoice response did not include a payment_hash")
        })?;
        Ok(WalletInvoice {
            bolt11: response.invoice,
            payment_hash,
        })
    }

    /// Pay a previously-issued bolt11 invoice. Mirrors NIP-47's
    /// `pay_invoice` - called by whoever is *sending* payment (the reader),
    /// against their own connected wallet. Returns the payment preimage.
    pub async fn pay_invoice(&self, invoice: &str) -> Result<String> {
        let client = self.client()?;
        let response = client
            .pay_invoice(PayInvoiceRequest::new(invoice))
            .await
            .map_err(|e| anyhow!("{e}"))
            .context("NIP-47 pay_invoice failed")?;
        Ok(response.preimage)
    }

    /// Poll NIP-47's `lookup_invoice` until `payment_hash` shows as settled,
    /// returning its preimage - the publisher side's independent
    /// verification that a claimed payment really cleared (design doc §5.2
    /// step 5), rather than trusting whatever `pay_invoice` on the payer's
    /// side reported.
    pub async fn wait_for_preimage(
        &self,
        payment_hash: &str,
        timeout: Duration,
        poll_interval: Duration,
    ) -> Result<String> {
        let client = self.client()?;
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let lookup = client
                .lookup_invoice(LookupInvoiceRequest {
                    payment_hash: Some(payment_hash.to_string()),
                    invoice: None,
                })
                .await
                .map_err(|e| anyhow!("{e}"))
                .context("NIP-47 lookup_invoice failed")?;

            if lookup.state == Some(TransactionState::Settled) {
                return lookup
                    .preimage
                    .ok_or_else(|| anyhow!("invoice settled but wallet returned no preimage"));
            }
            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!(
                    "timed out after {:?} waiting for invoice {payment_hash} to settle",
                    timeout
                );
            }
            tokio::time::sleep(poll_interval).await;
        }
    }
}
