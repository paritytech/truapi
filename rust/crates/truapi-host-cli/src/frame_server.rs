//! Product-frame WebSocket bridge for the pairing host.
//!
//! Each WebSocket connection is one product: inbound binary frames are pushed
//! into a [`ProductRuntime`] and its outgoing frames are written back as
//! binary messages. One binary WS message carries exactly one SCALE
//! `ProtocolMessage`, matching the browser transport's framing.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info};
use truapi_server::{
    FrameSink, PairingHostRuntime, ProductContext, ProductRuntime, SigningHostRuntime,
};

pub trait ProductRuntimeFactory: Send + Sync + 'static {
    fn product_runtime(&self, product: ProductContext, sink: Arc<dyn FrameSink>) -> ProductRuntime;
}

impl ProductRuntimeFactory for PairingHostRuntime {
    fn product_runtime(&self, product: ProductContext, sink: Arc<dyn FrameSink>) -> ProductRuntime {
        PairingHostRuntime::product_runtime(self, product, sink)
    }
}

impl ProductRuntimeFactory for SigningHostRuntime {
    fn product_runtime(&self, product: ProductContext, sink: Arc<dyn FrameSink>) -> ProductRuntime {
        SigningHostRuntime::product_runtime(self, product, sink)
    }
}

use crate::metrics::{Category, MetricsRecorder, Outcome};

/// Frame sink that writes each outgoing protocol frame as one binary message.
struct WsFrameSink {
    outbound: mpsc::UnboundedSender<Message>,
}

impl FrameSink for WsFrameSink {
    fn emit_frame(&self, frame: Vec<u8>) {
        let _ = self.outbound.send(Message::Binary(frame));
    }
}

/// Bind the product-frame listener on `addr`.
pub async fn bind(addr: SocketAddr) -> Result<TcpListener> {
    TcpListener::bind(addr)
        .await
        .with_context(|| format!("frame server failed to bind {addr}"))
}

/// Accept product-frame connections on `listener` for `product_id` until
/// cancelled.
///
/// The product dispatch future is `!Send` (matching the single-threaded wasm
/// runtime), so connections are driven with `spawn_local`; callers must run
/// this inside a `tokio::task::LocalSet`. The runtime's own subscription work
/// is `Send` and still runs on the multi-thread pool via the tokio spawner.
pub async fn accept_loop(
    runtime: Arc<dyn ProductRuntimeFactory>,
    product_id: String,
    listener: TcpListener,
    metrics: Arc<MetricsRecorder>,
) -> Result<()> {
    let bound = listener.local_addr()?;
    info!(%bound, %product_id, "product frame server listening");
    loop {
        let (stream, peer) = listener.accept().await?;
        let runtime = runtime.clone();
        let product_id = product_id.clone();
        let metrics = metrics.clone();
        tokio::task::spawn_local(async move {
            if let Err(err) = serve_connection(runtime, product_id, stream, metrics).await {
                debug!(%peer, %err, "frame connection ended");
            }
        });
    }
}

async fn serve_connection(
    runtime: Arc<dyn ProductRuntimeFactory>,
    product_id: String,
    stream: TcpStream,
    metrics: Arc<MetricsRecorder>,
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
                let started = Instant::now();
                let result = product_runtime.receive_frame(bytes.to_vec()).await;
                record_frame(&metrics, started, &result);
                if let Err(err) = result {
                    debug!(%err, "product runtime rejected frame");
                }
            }
            Ok(Message::Text(text)) => {
                let started = Instant::now();
                let result = product_runtime.receive_frame(text.as_bytes().to_vec()).await;
                record_frame(&metrics, started, &result);
                if let Err(err) = result {
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

/// Record one product-frame operation: latency from receive to dispatch
/// completion (`receive_frame` awaits the handler), and its outcome. The wire
/// id is not decoded yet, so the category is the coarse `Frame`; decoding it
/// into per-method categories is the next step.
fn record_frame<E: std::fmt::Display>(
    metrics: &MetricsRecorder,
    started: Instant,
    result: &Result<(), E>,
) {
    let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
    let (outcome, error_class) = match result {
        Ok(()) => (Outcome::Success, None),
        Err(err) => (Outcome::Error, Some(err.to_string())),
    };
    metrics.record(Category::Frame, "product_frame", latency_ms, outcome, error_class);
}
