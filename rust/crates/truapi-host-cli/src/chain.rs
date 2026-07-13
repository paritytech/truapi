//! Native WebSocket `ChainProvider` / `JsonRpcConnection`.
//!
//! The headless hosts reach the real People-chain statement store over
//! WebSocket JSON-RPC (the same node an iOS/web client uses). Every `connect`
//! opens a fresh socket; the runtime's `HostRpcClient` sits on top and speaks
//! statement-store RPC.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::BroadcastStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, warn};
use truapi::latest as api;
use truapi_platform::{ChainProvider, JsonRpcConnection};

use crate::network::ChainEndpoint;

/// Broadcast backlog for inbound JSON-RPC frames per connection.
const INBOUND_CHANNEL_CAPACITY: usize = 1024;

/// Chain provider that maps a requested genesis hash to a WebSocket endpoint.
///
/// The all-zero genesis (the headless SSO sentinel) and any unmapped genesis
/// fall back to the People-chain statement store; the Asset Hub and Bulletin
/// genesis hashes route to their own nodes (opt-in) for live playground
/// examples.
pub struct WsChainProvider {
    fallback_url: String,
    by_genesis: HashMap<[u8; 32], String>,
}

impl WsChainProvider {
    pub fn new(fallback_url: impl Into<String>, live_chain_endpoints: &[ChainEndpoint]) -> Self {
        // The fallback is the People-chain statement store, which serves the
        // SSO/identity path directly. Asset Hub routing (for the `Chain/*`
        // examples) is opt-in; when off, those genesis requests fall back to the
        // People node, which does not serve Asset Hub chainHead, so they fail
        // cleanly without disturbing the SSO/signer path.
        let by_genesis = if std::env::var("E2E_LIVE_CHAIN").as_deref() == Ok("1") {
            live_chain_endpoints
                .iter()
                .map(|endpoint| (endpoint.genesis, endpoint.ws.to_string()))
                .collect()
        } else {
            HashMap::new()
        };
        Self {
            fallback_url: fallback_url.into(),
            by_genesis,
        }
    }

    fn url_for(&self, genesis_hash: &[u8; 32]) -> &str {
        self.by_genesis
            .get(genesis_hash)
            .map(String::as_str)
            .unwrap_or(&self.fallback_url)
    }
}

#[async_trait]
impl ChainProvider for WsChainProvider {
    async fn connect(
        &self,
        genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, api::GenericError> {
        let url = self.url_for(&genesis_hash);
        debug!(genesis = %hex::encode(genesis_hash), %url, "chain connect");
        let connection = WsJsonRpcConnection::connect(url)
            .await
            .map_err(|reason| api::GenericError { reason })?;
        Ok(Box::new(connection))
    }
}

/// One WebSocket JSON-RPC connection: outbound requests are queued to a writer
/// task, inbound frames are broadcast to every `responses()` stream.
pub struct WsJsonRpcConnection {
    outbound: mpsc::UnboundedSender<Message>,
    inbound: broadcast::Sender<String>,
    closed: Arc<AtomicBool>,
}

impl WsJsonRpcConnection {
    async fn connect(url: &str) -> Result<Self, String> {
        let (stream, _response) = connect_async(url)
            .await
            .map_err(|err| format!("statement-store websocket connect failed: {err}"))?;
        let (mut write, mut read) = stream.split();
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<Message>();
        let (inbound_tx, _inbound_rx) = broadcast::channel(INBOUND_CHANNEL_CAPACITY);
        let closed = Arc::new(AtomicBool::new(false));

        tokio::spawn(async move {
            while let Some(message) = outbound_rx.recv().await {
                if write.send(message).await.is_err() {
                    break;
                }
            }
            let _ = write.close().await;
        });

        let reader_inbound = inbound_tx.clone();
        let reader_closed = closed.clone();
        tokio::spawn(async move {
            while let Some(message) = read.next().await {
                match message {
                    Ok(Message::Text(text)) => {
                        let _ = reader_inbound.send(text.to_string());
                    }
                    Ok(Message::Binary(bytes)) => {
                        if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                            let _ = reader_inbound.send(text);
                        }
                    }
                    Ok(Message::Close(_)) | Err(_) => break,
                    Ok(_) => {}
                }
            }
            reader_closed.store(true, Ordering::Release);
        });

        Ok(Self {
            outbound: outbound_tx,
            inbound: inbound_tx,
            closed,
        })
    }
}

impl JsonRpcConnection for WsJsonRpcConnection {
    fn send(&self, request: String) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        let _ = self.outbound.send(Message::Text(request));
    }

    fn responses(&self) -> BoxStream<'static, String> {
        BroadcastStream::new(self.inbound.subscribe())
            .filter_map(|item| async move {
                match item {
                    Ok(response) => Some(response),
                    Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(
                        dropped,
                    )) => {
                        warn!(dropped, "chain response subscriber lagged");
                        None
                    }
                }
            })
            .boxed()
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
}
