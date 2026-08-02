//! Persistent mock NIP-47 wallet service (companion to
//! `nwc_protocol_check.rs`, which spins up an equivalent wallet plus its own
//! test clients in one process and exits). This variant stays running and
//! prints a `nostr+walletconnect://` connection string, so a real running
//! `aetheria-delegate` process can `connect_wallet` to it over the IPC
//! protocol and exercise the production `NwcClient`/`handle_subscribe` code
//! path end-to-end - not just the standalone protocol check.
//!
//! Same "zero real money" scope as `nwc_protocol_check.rs`: this is a fake
//! Lightning backend behind a real NIP-47/Nostr wire protocol.
//!
//! Run: `cargo run --example mock_nwc_wallet`, then paste the printed URI
//! into the delegate's `connect_wallet` IPC call.

use nostr_sdk::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
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
    std::env::var("AETHERIA_NWC_TEST_RELAY").unwrap_or_else(|_| "wss://nos.lol".to_string())
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
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "warn".into()))
        .init();

    let relay = relay_url();
    let wallet_keys = Keys::generate();
    let app_secret = Keys::generate().secret_key().clone();
    let relay_url_parsed = RelayUrl::parse(&relay)?;
    let uri = NostrWalletConnectURI::new(wallet_keys.public_key(), vec![relay_url_parsed], app_secret, None);

    println!("Relay: {relay}");
    println!("Mock wallet pubkey: {}", wallet_keys.public_key());
    println!("\nConnection string (paste into connect_wallet):\n{uri}\n");

    let state: WalletState = Arc::new(AsyncMutex::new(HashMap::new()));

    let client = Client::new(wallet_keys.clone());
    client.add_relay(&relay).await?;
    client.connect().await;

    let info_builder = EventBuilder::new(
        Kind::WalletConnectInfo,
        "pay_invoice make_invoice lookup_invoice get_balance get_info",
    );
    client.send_event_builder(info_builder).await?;
    println!("[wallet] published NIP-47 info event (kind 13194)");

    let filter = Filter::new()
        .pubkey(wallet_keys.public_key())
        .kind(Kind::WalletConnectRequest)
        .since(Timestamp::now());
    client.subscribe(filter, None).await?;
    println!("[wallet] subscribed for incoming NIP-47 requests (kind 23194) - waiting...\n");

    let notif_client = client.clone();
    notif_client
        .handle_notifications(move |notification| {
            let state = state.clone();
            let wallet_keys = wallet_keys.clone();
            let client = client.clone();
            async move {
                if let RelayPoolNotification::Event { event, .. } = notification {
                    if event.kind == Kind::WalletConnectRequest {
                        if let Err(e) = handle_request(&client, &wallet_keys, &state, &event).await {
                            eprintln!("[wallet] error handling request: {e:#}");
                        }
                    }
                }
                Ok(false)
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
    let request =
        nip47::Request::from_value(value).map_err(|e| anyhow::anyhow!("parsing NIP-47 request: {e}"))?;

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
    println!("[wallet] sent response for {:?}\n", request.method);

    Ok(())
}
