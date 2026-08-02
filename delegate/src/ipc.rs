//! Local IPC server: the React/Tauri UI talks to the Delegate over a
//! loopback-only WebSocket so key material and ciphertext never cross a
//! process boundary the UI can inspect directly.

use crate::{db::LocalStore, freenet_bridge::FreenetBridge, keys::DelegateKeys, nwc::NwcClient};
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

pub async fn serve(
    port: u16,
    _db: LocalStore,
    _keys: DelegateKeys,
    _freenet: FreenetBridge,
    _nwc: NwcClient,
) -> Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "delegate IPC listening (loopback only)");

    while let Ok((stream, peer)) = listener.accept().await {
        tracing::debug!(%peer, "UI connection accepted");
        tokio::spawn(async move {
            let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
                return;
            };
            let (mut write, mut read) = ws.split();
            while let Some(Ok(msg)) = read.next().await {
                if let Message::Text(text) = msg {
                    // TODO: route to real handlers (publish post, list
                    // subscribers, request invoice, etc.) once the UI
                    // protocol is defined.
                    let echo = format!("{{\"error\":\"not_implemented\",\"received\":{text:?}}}");
                    if write.send(Message::Text(echo)).await.is_err() {
                        break;
                    }
                }
            }
        });
    }

    Ok(())
}
