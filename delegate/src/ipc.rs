//! Local IPC server: the React/Tauri UI talks to the Delegate over a
//! loopback-only WebSocket so key material and ciphertext never cross a
//! process boundary the UI can inspect directly.
//!
//! Protocol: newline-agnostic JSON request/response pairs correlated by
//! `id`. This is a Phase 2 prototype covering only the publisher's own
//! publish -> feed -> read loop entirely against the local SQLite cache;
//! there is no Freenet broadcast or subscriber decryption yet (see
//! `freenet_bridge.rs` and `nwc.rs`).

use crate::{
    contracts::{self, PublisherIdentity},
    crypto,
    db::LocalStore,
    freenet_bridge::FreenetBridge,
    keys::DelegateKeys,
    nwc::NwcClient,
};
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    ListPosts,
    GetPost {
        post_id: String,
    },
    PublishPost {
        title: String,
        summary: String,
        markdown: String,
        /// "public" or "subscriber".
        access: String,
    },
}

#[derive(Deserialize)]
struct Envelope {
    id: String,
    #[serde(flatten)]
    request: Request,
}

#[derive(Serialize)]
struct Response<'a> {
    id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

struct Delegate {
    db: LocalStore,
    keys: DelegateKeys,
    freenet: FreenetBridge,
    #[allow(dead_code)]
    nwc: NwcClient,
    identity: PublisherIdentity,
}

pub async fn serve(
    port: u16,
    db: LocalStore,
    keys: DelegateKeys,
    freenet: FreenetBridge,
    nwc: NwcClient,
    identity: PublisherIdentity,
) -> Result<()> {
    let delegate = Arc::new(Mutex::new(Delegate {
        db,
        keys,
        freenet,
        nwc,
        identity,
    }));

    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "delegate IPC listening (loopback only)");

    while let Ok((stream, peer)) = listener.accept().await {
        tracing::debug!(%peer, "UI connection accepted");
        let delegate = delegate.clone();
        tokio::spawn(async move {
            let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
                return;
            };
            let (mut write, mut read) = ws.split();
            while let Some(Ok(msg)) = read.next().await {
                let Message::Text(text) = msg else { continue };
                let reply = handle_message(&delegate, &text).await;
                if write.send(Message::Text(reply.into())).await.is_err() {
                    break;
                }
            }
        });
    }

    Ok(())
}

async fn handle_message(delegate: &Arc<Mutex<Delegate>>, text: &str) -> String {
    let envelope: Envelope = match serde_json::from_str(text) {
        Ok(e) => e,
        Err(e) => {
            return serde_json::to_string(&Response {
                id: "unknown",
                result: None,
                error: Some(format!("invalid request: {e}")),
            })
            .unwrap();
        }
    };

    let d = delegate.lock().await;
    let outcome = match envelope.request {
        Request::ListPosts => handle_list_posts(&d),
        Request::GetPost { post_id } => handle_get_post(&d, &post_id),
        Request::PublishPost {
            title,
            summary,
            markdown,
            access,
        } => handle_publish_post(&d, &title, &summary, &markdown, &access).await,
    };

    let response = match outcome {
        Ok(result) => Response {
            id: &envelope.id,
            result: Some(result),
            error: None,
        },
        Err(e) => Response {
            id: &envelope.id,
            result: None,
            error: Some(e.to_string()),
        },
    };
    serde_json::to_string(&response).unwrap()
}

fn handle_list_posts(delegate: &Delegate) -> Result<serde_json::Value> {
    let posts = delegate.db.list_posts()?;
    let json: Vec<_> = posts
        .into_iter()
        .map(|p| {
            serde_json::json!({
                "post_id": hex::encode(p.post_id),
                "title": p.title,
                "summary": p.summary,
                "access_level": p.access_level,
                "epoch_id": p.epoch_id,
                "published_at": p.published_at,
            })
        })
        .collect();
    Ok(serde_json::json!(json))
}

fn handle_get_post(delegate: &Delegate, post_id_hex: &str) -> Result<serde_json::Value> {
    let post_id: [u8; 16] = hex::decode_array(post_id_hex)?;
    let row = delegate
        .db
        .get_post(&post_id)?
        .ok_or_else(|| anyhow::anyhow!("post not found"))?;

    let markdown = match row.access_level.as_str() {
        "public" => row
            .markdown_plain
            .ok_or_else(|| anyhow::anyhow!("public post missing plaintext"))?,
        "subscriber" => {
            let key = delegate
                .db
                .get_epoch_key(row.epoch_id)?
                .ok_or_else(|| anyhow::anyhow!("epoch key not available locally"))?;
            let cipher_text = row
                .cipher_text
                .ok_or_else(|| anyhow::anyhow!("subscriber post missing ciphertext"))?;
            let nonce: [u8; 12] = row
                .nonce
                .ok_or_else(|| anyhow::anyhow!("subscriber post missing nonce"))?
                .try_into()
                .map_err(|_| anyhow::anyhow!("corrupt nonce length"))?;
            let bytes = crypto::decrypt_payload(&key, &nonce, &cipher_text)?;
            String::from_utf8(bytes)?
        }
        other => anyhow::bail!("unknown access_level {other}"),
    };

    Ok(serde_json::json!({
        "post_id": post_id_hex,
        "title": row.title,
        "markdown": markdown,
    }))
}

async fn handle_publish_post(
    delegate: &Delegate,
    title: &str,
    summary: &str,
    markdown: &str,
    access: &str,
) -> Result<serde_json::Value> {
    anyhow::ensure!(
        access == "public" || access == "subscriber",
        "access must be \"public\" or \"subscriber\""
    );

    let mut post_id = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut post_id);
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let epoch_id = current_epoch_id(now);

    // Local SQLite write stays exactly as it was before Freenet was wired
    // up - it's the fast local cache `list_posts`/`get_post` read from, and
    // must keep working even though publishing now also reaches the
    // network. `access_tier`/`network_cipher_text`/`network_nonce` are the
    // extra pieces the network side needs alongside what's already stored.
    let (access_tier, network_cipher_text, network_nonce) = if access == "public" {
        delegate.db.insert_post(
            &post_id,
            title,
            summary,
            access,
            epoch_id,
            now,
            Some(markdown),
            None,
            None,
        )?;
        // Matches PostDataContract's own convention for public posts: plain
        // bytes in `cipher_text`, all-zero nonce (see its module docs).
        (aetheria_types::AccessTier::Public, markdown.as_bytes().to_vec(), [0u8; 12])
    } else {
        let key = delegate
            .db
            .get_or_create_epoch_key(epoch_id, crypto::generate_epoch_key, now)?;
        let encrypted = crypto::encrypt_payload(&key, markdown.as_bytes())?;
        delegate.db.insert_post(
            &post_id,
            title,
            summary,
            access,
            epoch_id,
            now,
            None,
            Some(&encrypted.cipher_text),
            Some(&encrypted.nonce),
        )?;
        // TODO(Phase 3): real tier selection once the UI exposes more than
        // one subscription tier (design doc §3.1) - defaults to tier 0.
        (
            aetheria_types::AccessTier::SubscriberOnly { required_tier_id: 0 },
            encrypted.cipher_text,
            encrypted.nonce,
        )
    };

    let post_contract_id = contracts::publish_post_to_network(
        &delegate.freenet,
        &delegate.keys,
        &delegate.identity,
        post_id,
        title,
        summary,
        access_tier,
        epoch_id,
        now,
        network_cipher_text,
        network_nonce,
    )
    .await?;

    Ok(serde_json::json!({
        "post_id": hex::encode(post_id),
        "post_contract_id": post_contract_id,
    }))
}

/// Bucket the current time into a coarse ~30-day "billing epoch".
///
/// TODO(Phase 3): replace with a real calendar-month epoch once the
/// subscription renewal scheduler (design doc §6.2) is implemented.
fn current_epoch_id(now_unix_secs: u64) -> u32 {
    const THIRTY_DAYS_SECS: u64 = 30 * 24 * 60 * 60;
    (now_unix_secs / THIRTY_DAYS_SECS) as u32
}

/// Minimal hex helpers so the delegate doesn't need a full `hex` crate
/// dependency for this small amount of encode/decode.
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn decode_array<const N: usize>(s: &str) -> anyhow::Result<[u8; N]> {
        anyhow::ensure!(s.len() == N * 2, "expected {} hex chars, got {}", N * 2, s.len());
        let mut out = [0u8; N];
        for i in 0..N {
            out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)?;
        }
        Ok(out)
    }
}
