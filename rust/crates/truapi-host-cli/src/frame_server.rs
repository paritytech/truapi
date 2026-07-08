//! Product-frame WebSocket bridge for the pairing host.
//!
//! Each WebSocket connection is one product: inbound binary frames are pushed
//! into a [`ProductRuntime`] and its outgoing frames are written back as
//! binary messages. One binary WS message carries exactly one SCALE
//! `ProtocolMessage`, matching the browser transport's framing.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
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

use crate::metrics::{FrameClass, MetricsRecorder, Outcome, classify_frame, response_outcome};

/// True per-request outcomes, decoded from response frames as they are emitted
/// and consumed by the request loop, keyed by `request_id`.
type OutcomeMap = Arc<Mutex<HashMap<String, Outcome>>>;

/// Frame sink that writes each outgoing protocol frame as one binary message,
/// and records the true outcome of response frames for the metrics layer.
struct WsFrameSink {
    outbound: mpsc::UnboundedSender<Message>,
    outcomes: OutcomeMap,
}

impl FrameSink for WsFrameSink {
    fn emit_frame(&self, frame: Vec<u8>) {
        if let Some((request_id, outcome)) = response_outcome(&frame)
            && let Ok(mut map) = self.outcomes.lock()
        {
            map.insert(request_id, outcome);
        }
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
    let outcomes: OutcomeMap = Arc::new(Mutex::new(HashMap::new()));
    let sink = Arc::new(WsFrameSink {
        outbound: outbound_tx.clone(),
        outcomes: outcomes.clone(),
    });
    let product_runtime = runtime.product_runtime(product, sink);

    while let Some(message) = read.next().await {
        match message {
            Ok(Message::Binary(bytes)) => {
                let class = classify_frame(&bytes);
                let started = Instant::now();
                let result = product_runtime.receive_frame(bytes.to_vec()).await;
                record_frame(&metrics, &class, started, &result, &outcomes);
                if let Err(err) = result {
                    debug!(%err, "product runtime rejected frame");
                }
            }
            Ok(Message::Text(text)) => {
                let bytes = text.as_bytes().to_vec();
                let class = classify_frame(&bytes);
                let started = Instant::now();
                let result = product_runtime.receive_frame(bytes).await;
                record_frame(&metrics, &class, started, &result, &outcomes);
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

/// Error class recorded when dispatch succeeded but the response frame carried
/// a domain error. This is an emitted-schema value consumed by downstream ingest.
const RESPONSE_ERROR_CLASS: &str = "response_error";

/// Record one product-frame op: its `(category, op)`, receive-to-dispatch
/// latency, and true outcome. A clean dispatch (`Ok`) still counts as an
/// error when the captured response frame carried a domain error; a dispatch
/// with no captured response counts as success.
fn record_frame<E: std::fmt::Display>(
    metrics: &MetricsRecorder,
    class: &FrameClass,
    started: Instant,
    result: &Result<(), E>,
    outcomes: &OutcomeMap,
) {
    let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
    let (outcome, error_class) = match result {
        Err(err) => (Outcome::Error, Some(err.to_string())),
        Ok(()) => match outcomes.lock().ok().and_then(|mut m| m.remove(&class.request_id)) {
            Some(Outcome::Error) => (Outcome::Error, Some(RESPONSE_ERROR_CLASS.to_string())),
            Some(other) => (other, None),
            None => (Outcome::Success, None),
        },
    };
    metrics.record(class.category, &class.op, latency_ms, outcome, error_class);
}
