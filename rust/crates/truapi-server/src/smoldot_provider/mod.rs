//! Rust-owned light client backend for [`RuntimeChainProvider`].
//!
//! Wraps a single `smoldot_light::Client` that owns Paseo + Asset Hub Paseo.
//! Each `connect(genesis_hash)` returns a [`JsonRpcConnection`] that forwards
//! requests to the corresponding smoldot chain and streams responses back to
//! the chain runtime.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use futures::SinkExt;
use futures::StreamExt;
use futures::channel::mpsc;
use futures::stream::BoxStream;
use smoldot_light::{
    AddChainConfig, AddChainConfigJsonRpc, AddChainError, ChainId, Client, JsonRpcResponses,
};
use truapi_platform::JsonRpcConnection;

use crate::chain_runtime::{RuntimeChainProvider, RuntimeFailure};

#[cfg(not(target_arch = "wasm32"))]
mod native_platform;
#[cfg(not(target_arch = "wasm32"))]
use native_platform::{PlatformRefAlias, make_platform};

#[cfg(target_arch = "wasm32")]
mod wasm_helpers;
#[cfg(target_arch = "wasm32")]
mod wasm_platform;
#[cfg(target_arch = "wasm32")]
mod wasm_socket;
#[cfg(target_arch = "wasm32")]
use wasm_platform::{PlatformRefAlias, make_platform};

const PASEO_SPEC: &str = include_str!("specs/paseo.json");
const ASSET_HUB_PASEO_SPEC: &str = include_str!("specs/asset-hub-paseo.json");

const PASEO_RELAY_GENESIS: &str =
    "0x77afd6190f1554ad45fd0d31aee62aacc33c6db0ea801129acb813f913e0764f";
const ASSET_HUB_PASEO_GENESIS: &str =
    "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2";

/// Errors returned by [`SmoldotChainProvider::with_bundled_specs`].
#[derive(Debug)]
pub enum SmoldotInitError {
    /// Failed to add the Paseo relay chain to the client.
    AddRelay(AddChainError),
    /// Failed to add the Asset Hub parachain to the client.
    AddParaChain(AddChainError),
}

impl fmt::Display for SmoldotInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AddRelay(err) => write!(f, "failed to add relay chain: {err}"),
            Self::AddParaChain(err) => write!(f, "failed to add parachain: {err}"),
        }
    }
}

impl std::error::Error for SmoldotInitError {}

struct ChainEntry {
    chain_id: ChainId,
    responses: Mutex<Option<JsonRpcResponses<PlatformRefAlias>>>,
}

type ClientRef = Arc<Mutex<Client<PlatformRefAlias>>>;

/// A [`RuntimeChainProvider`] backed by `smoldot_light::Client`.
///
/// Built via [`SmoldotChainProvider::with_bundled_specs`] with Paseo + Asset
/// Hub Paseo pre-loaded.
pub struct SmoldotChainProvider {
    client: ClientRef,
    chains: HashMap<String, Arc<ChainEntry>>,
}

impl SmoldotChainProvider {
    /// Build a provider with Paseo + Asset Hub Paseo specs already added to
    /// the client. Each chain's json-rpc bus is held until the first
    /// `connect` call drains it.
    pub fn with_bundled_specs() -> Result<Self, SmoldotInitError> {
        let platform = make_platform();
        let mut client: Client<PlatformRefAlias> = Client::new(platform);

        let relay = client
            .add_chain(AddChainConfig {
                specification: PASEO_SPEC,
                json_rpc: AddChainConfigJsonRpc::Enabled {
                    max_pending_requests: std::num::NonZeroU32::new(u32::MAX).unwrap(),
                    max_subscriptions: u32::MAX,
                },
                database_content: "",
                potential_relay_chains: std::iter::empty(),
                user_data: (),
                statement_protocol_config: None,
            })
            .map_err(SmoldotInitError::AddRelay)?;

        let para = client
            .add_chain(AddChainConfig {
                specification: ASSET_HUB_PASEO_SPEC,
                json_rpc: AddChainConfigJsonRpc::Enabled {
                    max_pending_requests: std::num::NonZeroU32::new(u32::MAX).unwrap(),
                    max_subscriptions: u32::MAX,
                },
                database_content: "",
                potential_relay_chains: std::iter::once(relay.chain_id),
                user_data: (),
                statement_protocol_config: None,
            })
            .map_err(SmoldotInitError::AddParaChain)?;

        let mut chains = HashMap::new();
        chains.insert(
            PASEO_RELAY_GENESIS.to_string(),
            Arc::new(ChainEntry {
                chain_id: relay.chain_id,
                responses: Mutex::new(relay.json_rpc_responses),
            }),
        );
        chains.insert(
            ASSET_HUB_PASEO_GENESIS.to_string(),
            Arc::new(ChainEntry {
                chain_id: para.chain_id,
                responses: Mutex::new(para.json_rpc_responses),
            }),
        );

        Ok(Self {
            client: Arc::new(Mutex::new(client)),
            chains,
        })
    }
}

fn encode_genesis_hash(hash: &[u8]) -> String {
    let mut out = String::with_capacity(2 + hash.len() * 2);
    out.push_str("0x");
    for byte in hash {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[async_trait::async_trait]
impl RuntimeChainProvider for SmoldotChainProvider {
    async fn connect(
        &self,
        genesis_hash: Vec<u8>,
    ) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
        let key = encode_genesis_hash(&genesis_hash);
        let entry = self
            .chains
            .get(&key)
            .cloned()
            .ok_or_else(|| RuntimeFailure::unavailable("remote_chain_connect"))?;

        let responses = entry
            .responses
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| RuntimeFailure::unavailable("remote_chain_connect"))?;

        Ok(Arc::new(SmoldotJsonRpcConnection::new(
            self.client.clone(),
            entry.chain_id,
            responses,
        )))
    }
}

struct SmoldotJsonRpcConnection {
    client: ClientRef,
    chain_id: ChainId,
    responses: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
}

impl SmoldotJsonRpcConnection {
    fn new(
        client: ClientRef,
        chain_id: ChainId,
        mut responses: JsonRpcResponses<PlatformRefAlias>,
    ) -> Self {
        let (mut tx, rx) = mpsc::unbounded::<String>();
        spawn_pump(async move {
            while let Some(response) = responses.next().await {
                if tx.send(response).await.is_err() {
                    break;
                }
            }
        });
        Self {
            client,
            chain_id,
            responses: Mutex::new(Some(rx)),
        }
    }
}

impl JsonRpcConnection for SmoldotJsonRpcConnection {
    fn send(&self, request: String) {
        let mut client = self.client.lock().unwrap();
        if let Err(err) = client.json_rpc_request(request, self.chain_id) {
            log_send_error(err);
        }
    }

    fn responses(&self) -> BoxStream<'static, String> {
        let rx = self
            .responses
            .lock()
            .unwrap()
            .take()
            .expect("SmoldotJsonRpcConnection::responses() called twice");
        rx.boxed()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_pump<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    std::thread::spawn(move || {
        futures::executor::block_on(future);
    });
}

#[cfg(target_arch = "wasm32")]
fn spawn_pump<F>(future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}

#[cfg(not(target_arch = "wasm32"))]
fn log_send_error(err: smoldot_light::HandleRpcError) {
    eprintln!("smoldot json_rpc_request failed: {err}");
}

#[cfg(target_arch = "wasm32")]
fn log_send_error(err: smoldot_light::HandleRpcError) {
    web_sys::console::error_1(&format!("smoldot json_rpc_request failed: {err}").into());
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn paseo_genesis_bytes() -> Vec<u8> {
        let clean = PASEO_RELAY_GENESIS.trim_start_matches("0x");
        let mut out = Vec::with_capacity(32);
        for pair in clean.as_bytes().chunks(2) {
            out.push(u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap());
        }
        out
    }

    /// Smoke test: building the provider with bundled specs must register
    /// the two known chains. Verifies the smoldot module compiles and the
    /// client accepts the bundled chainspecs without doing any network I/O.
    #[test]
    fn smoldot_module_compiles_and_starts_idle_runtime() {
        let provider = SmoldotChainProvider::with_bundled_specs().expect("init");
        assert!(provider.chains.contains_key(PASEO_RELAY_GENESIS));
        assert!(provider.chains.contains_key(ASSET_HUB_PASEO_GENESIS));
        drop(provider);
    }

    #[test]
    fn connect_unknown_genesis_is_unavailable() {
        let provider = SmoldotChainProvider::with_bundled_specs().expect("init");
        let result = futures::executor::block_on(provider.connect(vec![0u8; 32]));
        assert!(result.is_err());
    }

    #[test]
    #[ignore = "hits the live Paseo relay network; run manually with --ignored"]
    fn streams_initialized_event_for_paseo() {
        let provider = SmoldotChainProvider::with_bundled_specs().expect("init");
        let connection =
            futures::executor::block_on(provider.connect(paseo_genesis_bytes())).expect("connect");
        connection.send(
            r#"{"jsonrpc":"2.0","id":"1","method":"chainHead_v1_follow","params":[true]}"#
                .to_string(),
        );

        let mut stream = connection.responses();
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut initialized_seen = false;
        while Instant::now() < deadline {
            let Some(frame) = futures::executor::block_on(stream.next()) else {
                break;
            };
            if frame.contains("\"event\":\"initialized\"") {
                initialized_seen = true;
                break;
            }
        }
        assert!(initialized_seen, "did not observe initialized event");
    }
}
