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
use anyhow::{Context, Result};
use base64::Engine;
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
    GetProfile,
    UpdateProfile {
        display_name: String,
        bio: String,
        /// A `data:<mime>;base64,<payload>` URL, or `None`/omitted to leave
        /// the avatar unchanged from whatever's already stored.
        #[serde(default)]
        avatar_data_url: Option<String>,
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
        Request::GetProfile => handle_get_profile(&d),
        Request::UpdateProfile {
            display_name,
            bio,
            avatar_data_url,
        } => handle_update_profile(&d, &display_name, &bio, avatar_data_url.as_deref()).await,
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
            // `post_contract_id` is `None` for a post whose network publish
            // failed (or hasn't been retried yet) - see
            // `handle_publish_post`. That's a legitimate, non-error state:
            // the post is real and locally saved, just not yet distributed
            // to the network, so it still shows up here rather than being
            // hidden or treated as corrupt.
            serde_json::json!({
                "post_id": hex::encode(p.post_id),
                "title": p.title,
                "summary": p.summary,
                "access_level": p.access_level,
                "epoch_id": p.epoch_id,
                "published_at": p.published_at,
                "network_synced": p.post_contract_id.is_some(),
                "post_contract_id": p.post_contract_id,
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
        "network_synced": row.post_contract_id.is_some(),
        "post_contract_id": row.post_contract_id,
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

    // The local SQLite write above already committed - this post is real
    // and already in the local feed (`list_posts`/`get_post` will show it)
    // regardless of what happens next. Freenet is additive on top of that,
    // per the design philosophy documented in CLAUDE.md and at the top of
    // this file, and the real gateway network is known to be flaky enough
    // that even `freenet_bridge.rs`'s client-side retries sometimes come up
    // empty (see CLAUDE.md's "Working end-to-end" section) - that is
    // expected, not a bug to propagate as a hard failure. So: don't let a
    // network-publish error fail the whole IPC response and don't let it
    // invalidate the local write either. Catch it, log it, and report
    // honestly which side succeeded so the UI can show something like
    // "saved locally, not yet synced to the network" instead of a bare
    // failure - while `list_posts`/`get_post` keep working unconditionally
    // for the post either way.
    let (post_contract_id, network_synced, network_error) = match contracts::publish_post_to_network(
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
    .await
    {
        Ok(contract_id) => {
            delegate.db.set_post_contract_id(&post_id, &contract_id)?;
            (Some(contract_id), true, None)
        }
        Err(e) => {
            tracing::warn!(
                post_id = %hex::encode(post_id),
                error = %e,
                "network publish failed after retries; post saved locally only, not yet synced to the network"
            );
            (None, false, Some(e.to_string()))
        }
    };

    Ok(serde_json::json!({
        "post_id": hex::encode(post_id),
        "post_contract_id": post_contract_id,
        "network_synced": network_synced,
        "network_error": network_error,
    }))
}

fn handle_get_profile(delegate: &Delegate) -> Result<serde_json::Value> {
    let avatar_freenet_key = contracts::known_avatar_key(&delegate.db)?;
    match delegate.db.get_profile()? {
        Some(p) => Ok(serde_json::json!({
            "display_name": p.display_name,
            "bio": p.bio,
            "avatar_data_url": match (&p.avatar_bytes, &p.avatar_mime) {
                (Some(bytes), Some(mime)) => Some(encode_data_url(mime, bytes)),
                _ => None,
            },
            "avatar_freenet_key": avatar_freenet_key,
        })),
        // No local row yet (fresh install, Profile tab never saved) - mirror
        // the placeholder `ensure_publisher_identity` publishes on first run
        // rather than showing something inconsistent with the network.
        None => Ok(serde_json::json!({
            "display_name": "Untitled Publication",
            "bio": "",
            "avatar_data_url": null,
            "avatar_freenet_key": avatar_freenet_key,
        })),
    }
}

async fn handle_update_profile(
    delegate: &Delegate,
    display_name: &str,
    bio: &str,
    avatar_data_url: Option<&str>,
) -> Result<serde_json::Value> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    // Resolve this call's avatar bytes/mime/network-key: either a fresh
    // upload (published/updated on the network right away, best-effort), or
    // - if the user didn't touch the avatar this save - whatever's already
    // cached locally and already registered on the network.
    let mut network_error: Option<String> = None;
    let (avatar_bytes, avatar_mime, avatar_freenet_key): (
        Option<Vec<u8>>,
        Option<String>,
        Option<String>,
    ) = if let Some(data_url) = avatar_data_url {
        let (mime, bytes) = decode_data_url(data_url)?;
        let key = match contracts::publish_avatar_to_network(
            &delegate.freenet,
            &delegate.db,
            &delegate.identity,
            delegate.keys.master_signing_verifying_bytes(),
            bytes.clone(),
        )
        .await
        {
            Ok(key) => Some(key),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "avatar network publish failed after retries; avatar saved locally only, not yet synced to the network"
                );
                network_error = Some(e.to_string());
                contracts::known_avatar_key(&delegate.db)?
            }
        };
        (Some(bytes), Some(mime), key)
    } else {
        let existing = delegate.db.get_profile()?;
        let key = contracts::known_avatar_key(&delegate.db)?;
        match existing {
            Some(p) => (p.avatar_bytes, p.avatar_mime, key),
            None => (None, None, key),
        }
    };

    // Local write commits unconditionally - same "network is additive, never
    // blocks the local save" philosophy as `handle_publish_post` above: a
    // hiccup on the flaky real gateway network must not lose the user's edits.
    delegate.db.set_profile(
        display_name,
        bio,
        avatar_bytes.as_deref(),
        avatar_mime.as_deref(),
        now,
    )?;

    let profile_synced = match contracts::publish_profile_to_network(
        &delegate.freenet,
        &delegate.keys,
        &delegate.identity,
        display_name,
        bio,
        avatar_freenet_key.clone(),
    )
    .await
    {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "profile network publish failed after retries; changes saved locally only, not yet synced to the network"
            );
            network_error.get_or_insert(e.to_string());
            false
        }
    };

    Ok(serde_json::json!({
        "display_name": display_name,
        "bio": bio,
        "avatar_data_url": match (&avatar_bytes, &avatar_mime) {
            (Some(bytes), Some(mime)) => Some(encode_data_url(mime, bytes)),
            _ => None,
        },
        "avatar_freenet_key": avatar_freenet_key,
        "network_synced": profile_synced,
        "network_error": network_error,
    }))
}

/// Minimal data-URL codec (`data:<mime>;base64,<payload>`) for the profile
/// avatar image over IPC - the UI reads/writes a `<input type="file">` as a
/// data URL, so this avoids inventing a second wire representation just for
/// this one field.
fn decode_data_url(data_url: &str) -> Result<(String, Vec<u8>)> {
    let rest = data_url
        .strip_prefix("data:")
        .ok_or_else(|| anyhow::anyhow!("avatar_data_url must start with \"data:\""))?;
    let (meta, payload) = rest
        .split_once(',')
        .ok_or_else(|| anyhow::anyhow!("avatar_data_url missing ',' separator"))?;
    let mime = meta
        .strip_suffix(";base64")
        .ok_or_else(|| anyhow::anyhow!("avatar_data_url must be base64-encoded"))?
        .to_string();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .context("decoding avatar image base64")?;
    Ok((mime, bytes))
}

fn encode_data_url(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
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
