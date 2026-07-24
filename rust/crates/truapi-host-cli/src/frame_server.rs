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

/// Process-local product selection shared by the command loop and frame server.
pub struct ProductSelection {
    current: watch::Sender<ProductContext>,
}

impl ProductSelection {
    /// Validate and normalize the initial product id.
    pub fn new(product_id: String) -> Result<Arc<Self>> {
        let product = ProductContext::new(product_id)
            .map_err(|error| anyhow::anyhow!("invalid product id: {error}"))?;
        let (current, _) = watch::channel(product);
        Ok(Arc::new(Self { current }))
    }

    /// Return the normalized current product id.
    pub fn current(&self) -> String {
        self.current.borrow().product_id.clone()
    }

    /// Select a validated product, returning whether the selection changed.
    pub fn select(&self, product_id: String) -> Result<bool> {
        let product = ProductContext::new(product_id)
            .map_err(|error| anyhow::anyhow!("invalid product id: {error}"))?;
        Ok(self.current.send_if_modified(|current| {
            if current == &product {
                false
            } else {
                *current = product;
                true
            }
        }))
    }

    fn subscribe(&self) -> watch::Receiver<ProductContext> {
        self.current.subscribe()
    }
}

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
/// Each connection is driven independently on the Tokio worker pool. The
/// shared dispatcher contract requires `Send` futures, while the WASM adapter
/// may still poll those futures on its single-threaded local executor.
pub async fn accept_loop(
    runtime: Arc<dyn ProductRuntimeFactory>,
    product: Arc<ProductSelection>,
    listener: TcpListener,
) -> Result<()> {
    let bound = listener.local_addr()?;
    let product_id = product.current();
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
        let product = product.clone();
        tokio::spawn(async move {
            if let Err(err) = serve_connection(runtime, product, stream).await {
                debug!(%peer, %err, "frame connection ended");
            }
        });
    }
}

async fn serve_connection(
    runtime: Arc<dyn ProductRuntimeFactory>,
    selected_product: Arc<ProductSelection>,
    stream: TcpStream,
) -> Result<()> {
    // Subscribe before resolving the runtime so a concurrent replacement can
    // only cause an extra reconnect, never leave a connection on stale state.
    let mut reset = runtime.connection_reset();
    let mut product_updates = selected_product.subscribe();
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

    let product = product_updates.borrow().clone();
    let sink = Arc::new(WsFrameSink {
        outbound: outbound_tx.clone(),
    });
    let product_runtime = runtime.product_runtime(product, sink);

    loop {
        let message = tokio::select! {
            _ = connection_reset(&mut reset) => break,
            _ = product_updates.changed() => break,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_selection_validates_and_normalizes_ids() -> Result<()> {
        let product = ProductSelection::new(" Dotli.DOT ".to_string())?;

        assert_eq!(product.current(), "dotli.dot");
        assert!(product.select("localhost:3000".to_string())?);
        assert_eq!(product.current(), "localhost:3000");
        assert!(!product.select("LOCALHOST:3000".to_string())?);
        assert!(product.select("example.com".to_string()).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn changing_product_notifies_connections() -> Result<()> {
        let product = ProductSelection::new("first.dot".to_string())?;
        let mut connection = product.subscribe();

        assert!(product.select("second.dot".to_string())?);
        connection.changed().await?;
        assert_eq!(connection.borrow().product_id, "second.dot");
        assert!(!product.select("SECOND.DOT".to_string())?);
        assert!(!connection.has_changed()?);
        Ok(())
    }
}
