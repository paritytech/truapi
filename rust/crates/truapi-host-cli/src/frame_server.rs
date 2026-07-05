//! Product-frame WebSocket bridge for the pairing host.
//!
//! Each WebSocket connection is one product: inbound binary frames are pushed
//! into a [`ProductRuntime`] and its outgoing frames are written back as
//! binary messages. One binary WS message carries exactly one SCALE
//! `ProtocolMessage`, matching the browser transport's framing.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info};
use truapi_server::{FrameSink, PairingHostRuntime, ProductContext};

/// Frame sink that writes each outgoing protocol frame as one binary message.
struct WsFrameSink {
    outbound: mpsc::UnboundedSender<Message>,
}

impl FrameSink for WsFrameSink {
    fn emit_frame(&self, frame: Vec<u8>) {
        let _ = self.outbound.send(Message::Binary(frame));
    }
}

/// Serve product frames for `product_id` on `addr` until the process exits.
///
/// The product dispatch future is `!Send` (matching the single-threaded wasm
/// runtime), so connections are driven with `spawn_local` under a `LocalSet`.
/// The runtime's own subscription work is `Send` and still runs on the
/// multi-thread pool via the tokio spawner.
pub async fn serve(
    runtime: Arc<PairingHostRuntime>,
    product_id: String,
    addr: SocketAddr,
) -> Result<()> {
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("frame server failed to bind {addr}"))?;
    let bound = listener.local_addr()?;
    info!(%bound, %product_id, "product frame server listening");
    println!("FRAMES_LISTENING ws://{bound}");

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            loop {
                let (stream, peer) = listener.accept().await?;
                let runtime = runtime.clone();
                let product_id = product_id.clone();
                tokio::task::spawn_local(async move {
                    if let Err(err) = serve_connection(runtime, product_id, stream).await {
                        debug!(%peer, %err, "frame connection ended");
                    }
                });
            }
        })
        .await
}

async fn serve_connection(
    runtime: Arc<PairingHostRuntime>,
    product_id: String,
    stream: TcpStream,
) -> Result<()> {
    let ws = accept_async(stream).await?;
    let (mut write, mut read) = ws.split();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<Message>();

    let writer = tokio::spawn(async move {
        while let Some(message) = outbound_rx.recv().await {
            if write.send(message).await.is_err() {
                break;
            }
        }
    });

    let product = ProductContext::new(product_id)
        .map_err(|err| anyhow::anyhow!("invalid product id: {err}"))?;
    let sink = Arc::new(WsFrameSink {
        outbound: outbound_tx.clone(),
    });
    let product_runtime = runtime.product_runtime(product, sink);

    while let Some(message) = read.next().await {
        match message {
            Ok(Message::Binary(bytes)) => {
                if let Err(err) = product_runtime.receive_frame(bytes.to_vec()).await {
                    debug!(%err, "product runtime rejected frame");
                }
            }
            Ok(Message::Text(text)) => {
                if let Err(err) = product_runtime
                    .receive_frame(text.as_bytes().to_vec())
                    .await
                {
                    debug!(%err, "product runtime rejected text frame");
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => {}
        }
    }

    product_runtime.dispose();
    drop(outbound_tx);
    let _ = writer.await;
    Ok(())
}
