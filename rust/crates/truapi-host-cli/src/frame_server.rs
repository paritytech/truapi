//! Product-frame WebSocket bridge for the pairing host.
//!
//! Each WebSocket connection is one product: inbound binary frames are pushed
//! into a [`ProductRuntime`] and its outgoing frames are written back as
//! binary messages. One binary WS message carries exactly one SCALE
//! `ProtocolMessage`, matching the browser transport's framing.

use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, warn};
use truapi_server::{
    FrameSink, PairingHostRuntime, ProductContext, ProductRuntime, SigningHostRuntime,
};

pub trait ProductRuntimeFactory: Send + Sync + 'static {
    fn product_runtime(&self, product: ProductContext, sink: Arc<dyn FrameSink>) -> ProductRuntime;

    /// Subscribe to a signal that invalidates existing product connections.
    fn connection_reset(&self) -> Option<watch::Receiver<u64>> {
        None
    }
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

/// Signing runtime factory whose active session can be replaced without
/// restarting the frame listener.
pub struct SwitchableSigningRuntime {
    current: RwLock<Arc<SigningHostRuntime>>,
    generation: watch::Sender<u64>,
}

impl SwitchableSigningRuntime {
    pub fn new(runtime: Arc<SigningHostRuntime>) -> Arc<Self> {
        let (generation, _) = watch::channel(0);
        Arc::new(Self {
            current: RwLock::new(runtime),
            generation,
        })
    }

    /// Replace the runtime and disconnect every product using the old one.
    pub fn replace(&self, runtime: Arc<SigningHostRuntime>) {
        *self.current.write().expect("runtime lock poisoned") = runtime;
        self.generation
            .send_modify(|generation| *generation = generation.wrapping_add(1));
    }
}

impl ProductRuntimeFactory for SwitchableSigningRuntime {
    fn product_runtime(&self, product: ProductContext, sink: Arc<dyn FrameSink>) -> ProductRuntime {
        self.current
            .read()
            .expect("runtime lock poisoned")
            .product_runtime(product, sink)
    }

    fn connection_reset(&self) -> Option<watch::Receiver<u64>> {
        Some(self.generation.subscribe())
    }
}

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
) -> Result<()> {
    let bound = listener.local_addr()?;
    debug!(%bound, %product_id, "product frame server listening");
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(err) => {
                warn!(%err, "product frame accept failed");
                continue;
            }
        };
        let runtime = runtime.clone();
        let product_id = product_id.clone();
        tokio::task::spawn_local(async move {
            if let Err(err) = serve_connection(runtime, product_id, stream).await {
                debug!(%peer, %err, "frame connection ended");
            }
        });
    }
}

async fn serve_connection(
    runtime: Arc<dyn ProductRuntimeFactory>,
    product_id: String,
    stream: TcpStream,
) -> Result<()> {
    // Subscribe before resolving the runtime so a concurrent replacement can
    // only cause an extra reconnect, never leave a connection on stale state.
    let mut reset = runtime.connection_reset();
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

    loop {
        let message = tokio::select! {
            _ = connection_reset(&mut reset) => break,
            message = read.next() => message,
        };
        let Some(message) = message else {
            break;
        };
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

async fn connection_reset(reset: &mut Option<watch::Receiver<u64>>) {
    match reset {
        Some(reset) => {
            let _ = reset.changed().await;
        }
        None => std::future::pending().await,
    }
}
