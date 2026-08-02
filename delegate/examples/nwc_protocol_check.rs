//! Standalone protocol check (not part of the shipped delegate binary - runs
//! via `cargo run --example nwc_protocol_check`): exercises the *real*
//! NIP-47 (Nostr Wallet Connect) wire protocol over a real public Nostr
//! relay, with **zero real money and no real wallet account** - see
//! `delegate/src/nwc.rs`'s module docs for why `NwcClient` uses the `nwc`
//! crate (client-side only) and this example uses `nostr-sdk` directly to
//! play the *wallet service* role instead.
//!
//! ## What this actually proves
//!
//! `nwc` (this delegate's real NWC dependency) has no way to fabricate a
//! wallet service to talk to, and there's no free public NIP-47 wallet
//! service to test against without a funded account. So this program *is*
//! the test harness: it spins up a hand-built mock wallet service (real
//! Nostr keys, real relay connection, real NIP-04 encrypted events, real
//! kind 13194/23194/23195 events per spec) as a background task, then drives
//! two real `nwc::NWC` clients - one per role - against it, over
//! `wss://relay.damus.io` (overridable via `AETHERIA_NWC_TEST_RELAY`):
//!
//! 1. A "publisher" `NWC` client calls `make_invoice` - the mock wallet
//!    replies with a fake (unspendable) bolt11-shaped string + payment hash.
//! 2. A "reader" `NWC` client - a *different* NWC connection (different
//!    per-app secret, same wallet pubkey - exactly how one real wallet
//!    serves multiple connected apps) - calls `pay_invoice` with that
//!    invoice string. The mock wallet marks it settled and returns a fake
//!    preimage.
//! 3. The publisher client polls `lookup_invoice` until it sees the same
//!    settlement, exactly as `NwcClient::wait_for_preimage` does in the real
//!    delegate.
//!
//! Every byte of NIP-47 request/response traffic here is real: real Nostr
//! events, signed and NIP-04-encrypted for real, relayed through a real
//! public relay neither side controls. The only thing that's fake is the
//! Lightning backend the mock wallet pretends to have - which is exactly the
//! one piece this task explicitly says not to try to substitute with a
//! funded real account.
//!
//! Run: `cargo run --example nwc_protocol_check`

use nostr_sdk::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex as AsyncMutex;

#[derive(Clone)]
struct PendingInvoice {
    bolt11: String,
    amount_msat: u64,
    settled: bool,
    preimage: Option<String>,
}

type WalletState = Arc<AsyncMutex<HashMap<String, PendingInvoice>>>;

fn relay_url() -> String {
    std::env::var("AETHERIA_NWC_TEST_RELAY").unwrap_or_else(|_| "wss://relay.damus.io".to_string())
}

fn random_hex(bytes: usize) -> String {
    use rand::RngCore;
    let mut buf = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "warn,nwc_protocol_check=info".into()),
        )
        .init();

    let relay = relay_url();
    println!("Using relay: {relay}");

    // --- Mock wallet identity (one real Lightning-wallet-shaped Nostr
    // keypair, standing in for e.g. an Alby account) ---
    let wallet_keys = Keys::generate();
    println!("Mock wallet pubkey: {}", wallet_keys.public_key());

    // Two independent app connections to the *same* wallet - exactly how a
    // real NWC-enabled wallet serves multiple apps. "Publisher" mints
    // invoices to receive; "reader" pays them.
    let app_secret_publisher = Keys::generate().secret_key().clone();
    let app_secret_reader = Keys::generate().secret_key().clone();

    let relay_url_parsed = RelayUrl::parse(&relay)?;
    let uri_publisher = NostrWalletConnectURI::new(
        wallet_keys.public_key(),
        vec![relay_url_parsed.clone()],
        app_secret_publisher,
        None,
    );
    let uri_reader = NostrWalletConnectURI::new(
        wallet_keys.public_key(),
        vec![relay_url_parsed],
        app_secret_reader,
        None,
    );

    let state: WalletState = Arc::new(AsyncMutex::new(HashMap::new()));

    // --- Spawn the mock wallet service ---
    let wallet_task = tokio::spawn(run_mock_wallet(wallet_keys, relay.clone(), state));

    // Give the wallet task time to connect, subscribe, and publish its
    // info event before any client tries to talk to it.
    tokio::time::sleep(Duration::from_secs(3)).await;

    let result = run_protocol_check(uri_publisher, uri_reader).await;

    wallet_task.abort();

    match result {
        Ok(()) => {
            println!(
                "\nPASS: make_invoice / pay_invoice / lookup_invoice round-tripped over the \
                 real Nostr relay ({relay}) via real NIP-47 encrypted events, using two \
                 independent NWC connections to one mock wallet. No real money involved."
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("\nFAIL: {e:#}");
            Err(e)
        }
    }
}

/// Plays the *wallet service* half of NIP-47 - the half `nwc` (a client-only
/// crate) has no code for. Built directly on `nostr-sdk`'s `Client` because
/// that's the lowest-level piece available; there's no "mock NWC server"
/// crate to reach for.
async fn run_mock_wallet(wallet_keys: Keys, relay: String, state: WalletState) -> anyhow::Result<()> {
    let client = Client::new(wallet_keys.clone());
    client.add_relay(&relay).await?;
    client.connect().await;

    // NIP-47 kind 13194: wallet service info event, replaceable, listing
    // supported methods as a space-separated string.
    let info_builder = EventBuilder::new(Kind::WalletConnectInfo, "pay_invoice make_invoice lookup_invoice get_balance get_info");
    client.send_event_builder(info_builder).await?;
    println!("[wallet] published NIP-47 info event (kind 13194)");

    let filter = Filter::new()
        .pubkey(wallet_keys.public_key())
        .kind(Kind::WalletConnectRequest)
        .since(Timestamp::now());
    client.subscribe(filter, None).await?;
    println!("[wallet] subscribed for incoming NIP-47 requests (kind 23194)");

    let wallet_keys_for_handler = wallet_keys.clone();
    let notif_client = client.clone();
    notif_client
        .handle_notifications(move |notification| {
            let state = state.clone();
            let wallet_keys = wallet_keys_for_handler.clone();
            let client = client.clone();
            async move {
                if let RelayPoolNotification::Event { event, .. } = notification {
                    if event.kind != Kind::WalletConnectRequest {
                        return Ok(false);
                    }
                    if let Err(e) =
                        handle_request(&client, &wallet_keys, &state, &event).await
                    {
                        eprintln!("[wallet] error handling request: {e:#}");
                    }
                }
                Ok(false) // never exit on our own
            }
        })
        .await?;

    Ok(())
}

async fn handle_request(
    client: &Client,
    wallet_keys: &Keys,
    state: &WalletState,
    event: &Event,
) -> anyhow::Result<()> {
    let decrypted = nip04::decrypt(wallet_keys.secret_key(), &event.pubkey, &event.content)
        .map_err(|e| anyhow::anyhow!("nip04 decrypt failed: {e}"))?;
    let value: serde_json::Value = serde_json::from_str(&decrypted)?;
    let request = nip47::Request::from_value(value)
        .map_err(|e| anyhow::anyhow!("parsing NIP-47 request: {e}"))?;

    println!("[wallet] received {:?} from {}", request.method, event.pubkey);

    let response = match request.params {
        nip47::RequestParams::MakeInvoice(p) => {
            let payment_hash = random_hex(32);
            let bolt11 = format!("lnbcmock1{payment_hash}");
            let mut invoices = state.lock().await;
            invoices.insert(
                payment_hash.clone(),
                PendingInvoice {
                    bolt11: bolt11.clone(),
                    amount_msat: p.amount,
                    settled: false,
                    preimage: None,
                },
            );
            nip47::Response {
                result_type: nip47::Method::MakeInvoice,
                error: None,
                result: Some(nip47::ResponseResult::MakeInvoice(nip47::MakeInvoiceResponse {
                    invoice: bolt11,
                    payment_hash: Some(payment_hash),
                    description: p.description,
                    description_hash: None,
                    preimage: None,
                    amount: Some(p.amount),
                    created_at: Some(Timestamp::now()),
                    expires_at: p.expiry.map(|e| Timestamp::now() + e),
                })),
            }
        }
        nip47::RequestParams::PayInvoice(p) => {
            let mut invoices = state.lock().await;
            let entry = invoices.values_mut().find(|inv| inv.bolt11 == p.invoice);
            match entry {
                Some(inv) => {
                    let preimage = random_hex(32);
                    inv.settled = true;
                    inv.preimage = Some(preimage.clone());
                    nip47::Response {
                        result_type: nip47::Method::PayInvoice,
                        error: None,
                        result: Some(nip47::ResponseResult::PayInvoice(nip47::PayInvoiceResponse {
                            preimage,
                            fees_paid: Some(0),
                        })),
                    }
                }
                None => nip47::Response {
                    result_type: nip47::Method::PayInvoice,
                    error: Some(nip47::NIP47Error {
                        code: nip47::ErrorCode::NotFound,
                        message: "unknown invoice".to_string(),
                    }),
                    result: None,
                },
            }
        }
        nip47::RequestParams::LookupInvoice(p) => {
            let invoices = state.lock().await;
            let found: Option<(String, PendingInvoice)> = if let Some(hash) = &p.payment_hash {
                invoices.get(hash).map(|inv| (hash.clone(), inv.clone()))
            } else if let Some(inv_str) = &p.invoice {
                invoices
                    .iter()
                    .find(|(_, inv)| &inv.bolt11 == inv_str)
                    .map(|(hash, inv)| (hash.clone(), inv.clone()))
            } else {
                None
            };
            match found {
                Some((hash, inv)) => nip47::Response {
                    result_type: nip47::Method::LookupInvoice,
                    error: None,
                    result: Some(nip47::ResponseResult::LookupInvoice(nip47::LookupInvoiceResponse {
                        transaction_type: Some(nip47::TransactionType::Incoming),
                        state: Some(if inv.settled {
                            nip47::TransactionState::Settled
                        } else {
                            nip47::TransactionState::Pending
                        }),
                        invoice: Some(inv.bolt11.clone()),
                        description: None,
                        description_hash: None,
                        preimage: inv.preimage.clone(),
                        payment_hash: hash,
                        amount: inv.amount_msat,
                        fees_paid: 0,
                        created_at: Timestamp::now(),
                        expires_at: None,
                        settled_at: if inv.settled { Some(Timestamp::now()) } else { None },
                        metadata: None,
                    })),
                },
                None => nip47::Response {
                    result_type: nip47::Method::LookupInvoice,
                    error: Some(nip47::NIP47Error {
                        code: nip47::ErrorCode::NotFound,
                        message: "unknown invoice".to_string(),
                    }),
                    result: None,
                },
            }
        }
        nip47::RequestParams::GetInfo => nip47::Response {
            result_type: nip47::Method::GetInfo,
            error: None,
            result: Some(nip47::ResponseResult::GetInfo(nip47::GetInfoResponse {
                alias: Some("aetheria-mock-wallet".to_string()),
                color: None,
                pubkey: Some(wallet_keys.public_key().to_hex()),
                network: Some("mock".to_string()),
                block_height: None,
                block_hash: None,
                methods: vec![
                    nip47::Method::PayInvoice,
                    nip47::Method::MakeInvoice,
                    nip47::Method::LookupInvoice,
                    nip47::Method::GetInfo,
                ],
                notifications: Vec::new(),
            })),
        },
        other => {
            eprintln!("[wallet] unhandled method: {other:?}");
            return Ok(());
        }
    };

    let encrypted = nip04::encrypt(wallet_keys.secret_key(), &event.pubkey, response.as_json())
        .map_err(|e| anyhow::anyhow!("nip04 encrypt failed: {e}"))?;
    let response_builder = EventBuilder::new(Kind::WalletConnectResponse, encrypted)
        .tag(Tag::public_key(event.pubkey))
        .tag(Tag::event(event.id));
    client.send_event_builder(response_builder).await?;
    println!("[wallet] sent response for {:?}", request.method);

    Ok(())
}

async fn run_protocol_check(
    uri_publisher: NostrWalletConnectURI,
    uri_reader: NostrWalletConnectURI,
) -> anyhow::Result<()> {
    let nwc_publisher = nwc::NWC::new(uri_publisher);
    let nwc_reader = nwc::NWC::new(uri_reader);

    println!("\n[publisher] get_info ...");
    let info = nwc_publisher.get_info().await?;
    println!("[publisher] get_info -> alias={:?} methods={:?}", info.alias, info.methods);

    println!("\n[publisher] make_invoice(21000 msat) ...");
    let invoice = nwc_publisher
        .make_invoice(nwc::prelude::MakeInvoiceRequest {
            amount: 21_000,
            description: Some("Aetheria NWC protocol check (no real value)".to_string()),
            description_hash: None,
            expiry: Some(3600),
        })
        .await?;
    println!(
        "[publisher] make_invoice -> invoice={} payment_hash={:?}",
        invoice.invoice, invoice.payment_hash
    );
    let payment_hash = invoice
        .payment_hash
        .clone()
        .ok_or_else(|| anyhow::anyhow!("mock wallet did not return a payment_hash"))?;

    println!("\n[reader] pay_invoice(...) ...");
    let pay_response = nwc_reader
        .pay_invoice(nwc::prelude::PayInvoiceRequest::new(&invoice.invoice))
        .await?;
    println!("[reader] pay_invoice -> preimage={}", pay_response.preimage);

    println!("\n[publisher] polling lookup_invoice until settled ...");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let looked_up = loop {
        let lookup = nwc_publisher
            .lookup_invoice(nwc::prelude::LookupInvoiceRequest {
                payment_hash: Some(payment_hash.clone()),
                invoice: None,
            })
            .await?;
        if lookup.state == Some(nwc::prelude::TransactionState::Settled) {
            break lookup;
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for lookup_invoice to report settled");
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    println!(
        "[publisher] lookup_invoice -> state={:?} preimage={:?}",
        looked_up.state, looked_up.preimage
    );

    anyhow::ensure!(
        looked_up.preimage.as_deref() == Some(pay_response.preimage.as_str()),
        "preimage mismatch: pay_invoice returned {:?}, lookup_invoice returned {:?}",
        pay_response.preimage,
        looked_up.preimage
    );

    Ok(())
}
