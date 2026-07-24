//! Native WebSocket `ChainProvider` / `JsonRpcConnection`.
//!
//! The headless hosts reach the real People-chain statement store over
//! WebSocket JSON-RPC (the same node an iOS/web client uses). Every `connect`
//! opens a fresh socket; the runtime's `HostRpcClient` sits on top and speaks
//! statement-store RPC.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
/// fall back to the People-chain statement store. Host-required routes such as
/// Bulletin are always enabled; optional product Chain routes remain opt-in.
pub struct WsChainProvider {
    fallback_url: String,
    by_genesis: HashMap<[u8; 32], String>,
}

impl WsChainProvider {
    pub fn new(fallback_url: impl Into<String>, live_chain_endpoints: &[ChainEndpoint]) -> Self {
        let live_chain_routing = std::env::var("E2E_LIVE_CHAIN").as_deref() == Ok("1");
        Self::with_live_chain_routing(fallback_url, live_chain_endpoints, live_chain_routing)
    }

    fn with_live_chain_routing(
        fallback_url: impl Into<String>,
        live_chain_endpoints: &[ChainEndpoint],
        live_chain_routing: bool,
    ) -> Self {
        // People remains the fallback for the SSO sentinel. Bulletin is a
        // required host dependency for preimage submission and must never be
        // gated by the product-facing Chain/* test switch.
        let by_genesis = live_chain_endpoints
            .iter()
            .filter(|endpoint| endpoint.required_for_host || live_chain_routing)
            .map(|endpoint| (endpoint.genesis, endpoint.ws.to_string()))
            .collect();
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
    /// Receiver created before the reader task starts. The first response
    /// stream takes it so an immediate RPC response cannot race subscription
    /// setup and disappear while the broadcast channel has no receivers.
    initial_inbound: Mutex<Option<broadcast::Receiver<String>>>,
    closed: Arc<AtomicBool>,
}

impl WsJsonRpcConnection {
    async fn connect(url: &str) -> Result<Self, String> {
        let (stream, _response) = connect_async(url)
            .await
            .map_err(|err| format!("statement-store websocket connect failed: {err}"))?;
        let (mut write, mut read) = stream.split();
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<Message>();
        let (inbound_tx, initial_inbound) = broadcast::channel(INBOUND_CHANNEL_CAPACITY);
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
            initial_inbound: Mutex::new(Some(initial_inbound)),
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
        let receiver = self
            .initial_inbound
            .lock()
            .expect("initial chain response receiver mutex poisoned")
            .take()
            .unwrap_or_else(|| self.inbound.subscribe());
        BroadcastStream::new(receiver)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::Network;

    #[test]
    fn first_response_stream_receives_frames_buffered_during_setup() {
        let (outbound, _outbound_rx) = mpsc::unbounded_channel();
        let (inbound, initial_inbound) = broadcast::channel(INBOUND_CHANNEL_CAPACITY);
        let connection = WsJsonRpcConnection {
            outbound,
            inbound: inbound.clone(),
            initial_inbound: Mutex::new(Some(initial_inbound)),
            closed: Arc::new(AtomicBool::new(false)),
        };

        inbound
            .send(r#"{"jsonrpc":"2.0","id":1,"result":"ready"}"#.to_string())
            .expect("initial receiver keeps the frame buffered");

        let mut responses = connection.responses();
        let frame = futures::executor::block_on(responses.next()).expect("buffered response");
        assert_eq!(frame, r#"{"jsonrpc":"2.0","id":1,"result":"ready"}"#);
    }

    #[test]
    fn required_bulletin_route_is_enabled_without_optional_live_chains() {
        let network = Network::PaseoNextV2.config();
        let provider = WsChainProvider::with_live_chain_routing(
            network.people_ws,
            network.live_chain_endpoints,
            false,
        );

        assert_eq!(
            provider.url_for(&network.bulletin_genesis),
            network.bulletin_ws
        );
        assert_eq!(provider.url_for(&network.people_genesis), network.people_ws);
        assert_eq!(
            provider.url_for(&network.live_chain_endpoints[0].genesis),
            network.people_ws
        );
    }

    #[test]
    fn optional_live_chain_routes_are_enabled_by_the_test_switch() {
        let network = Network::PaseoNextV2.config();
        let provider = WsChainProvider::with_live_chain_routing(
            network.people_ws,
            network.live_chain_endpoints,
            true,
        );

        assert_eq!(
            provider.url_for(&network.live_chain_endpoints[0].genesis),
            network.live_chain_endpoints[0].ws
        );
    }
}
