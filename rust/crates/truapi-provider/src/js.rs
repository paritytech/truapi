//! JavaScript-facing API for browser hosts (`js` feature, wasm32 only).
//!
//! Exposes the provider to JS without a Rust consumer: build a provider from
//! chain registrations, connect per genesis hash, and drive the raw JSON-RPC
//! string pipe. `nextResponse()` is pull-based, mirroring the smoldot npm
//! package's `nextJsonRpcResponse` so existing host code maps 1:1.
//!
//! ```js
//! const builder = new ChainProviderBuilder();
//! builder.addRpcChain("0x77af…", "wss://node.example");
//! const provider = builder.build();
//! const connection = await provider.connect("0x77af…");
//! connection.send('{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_genesisHash","params":[]}');
//! const response = await connection.nextResponse(); // undefined once closed
//! connection.close();
//! ```
//!
//! Construct one provider per page/worker: connections share the provider's
//! resources, matching the one-provider-per-host-process contract.

use std::sync::Arc;

use futures::lock::Mutex;
use futures::stream::{BoxStream, StreamExt};
use truapi_platform::{ChainProvider as _, JsonRpcConnection};
use wasm_bindgen::prelude::*;

use crate::config::ChainSource;
use crate::provider::{EmbeddedChainProvider, EmbeddedChainProviderBuilder};

/// Collects genesis-hash to chain-source registrations from JS.
#[wasm_bindgen]
pub struct ChainProviderBuilder {
    inner: Option<EmbeddedChainProviderBuilder>,
}

#[wasm_bindgen]
impl ChainProviderBuilder {
    /// Create an empty builder.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        ChainProviderBuilder {
            inner: Some(EmbeddedChainProviderBuilder::new()),
        }
    }

    /// Register a remote JSON-RPC node for the chain identified by the
    /// `0x`-prefixed genesis hash. A later registration for the same hash
    /// replaces the earlier one.
    #[wasm_bindgen(js_name = addRpcChain)]
    pub fn add_rpc_chain(&mut self, genesis_hash: &str, url: &str) -> Result<(), JsError> {
        let genesis = parse_genesis(genesis_hash)?;
        let url =
            url::Url::parse(url).map_err(|err| JsError::new(&format!("invalid URL: {err}")))?;
        let builder = self
            .inner
            .take()
            .ok_or_else(|| JsError::new("builder was already consumed by build()"))?;
        self.inner = Some(builder.chain(genesis, ChainSource::rpc_node(url)));
        Ok(())
    }

    /// Register an embedded light-client chain identified by the
    /// `0x`-prefixed genesis hash. Use this for a relay or standalone chain;
    /// parachains are served through the bundled catalog (`addNetwork`), which
    /// supplies their relay wiring, since a parachain's relay is never a
    /// caller-supplied option. The statement-store networking protocol is
    /// enabled unless `statement_protocol` is `false`.
    #[cfg(feature = "smoldot")]
    #[wasm_bindgen(js_name = addLightChain)]
    pub fn add_light_chain(
        &mut self,
        genesis_hash: &str,
        specification: String,
        statement_protocol: Option<bool>,
    ) -> Result<(), JsError> {
        let genesis = parse_genesis(genesis_hash)?;
        let mut source = ChainSource::light_client(specification);
        if statement_protocol == Some(false) {
            source = source.without_statement_protocol();
        }
        let builder = self
            .inner
            .take()
            .ok_or_else(|| JsError::new("builder was already consumed by build()"))?;
        self.inner = Some(builder.chain(genesis, source.build()));
        Ok(())
    }

    /// Seed a warm-start database blob (from
    /// [`snapshot`](ChainProviderHandle::snapshot)) for the `0x`-prefixed
    /// genesis hash, so its light client resumes from that finalized state
    /// instead of re-syncing from the checkpoint.
    #[cfg(feature = "smoldot")]
    #[wasm_bindgen(js_name = setDatabase)]
    pub fn set_database(&mut self, genesis_hash: &str, blob: String) -> Result<(), JsError> {
        let genesis = parse_genesis(genesis_hash)?;
        let builder = self
            .inner
            .take()
            .ok_or_else(|| JsError::new("builder was already consumed by build()"))?;
        self.inner = Some(builder.database(genesis, blob));
        Ok(())
    }

    /// Register every chain of the bundled network `name` (relay plus system
    /// parachains, with relay wiring and statement-store placement supplied by
    /// the catalog). Returns the network's genesis hashes.
    #[cfg(feature = "networks")]
    #[wasm_bindgen(js_name = addNetwork)]
    pub fn add_network(&mut self, name: &str) -> Result<NetworkChains, JsError> {
        let builder = self
            .inner
            .take()
            .ok_or_else(|| JsError::new("builder was already consumed by build()"))?;
        let (builder, chains) = builder
            .add_network(name)
            .map_err(|err| JsError::new(&err.reason))?;
        self.inner = Some(builder);
        Ok(NetworkChains {
            relay: hex0x(&chains.relay),
            asset_hub: hex0x(&chains.asset_hub),
            bulletin: hex0x(&chains.bulletin),
            people: hex0x(&chains.people),
        })
    }

    /// Build the provider, consuming the builder.
    pub fn build(&mut self) -> Result<ChainProviderHandle, JsError> {
        let builder = self
            .inner
            .take()
            .ok_or_else(|| JsError::new("builder was already consumed by build()"))?;
        Ok(ChainProviderHandle {
            inner: Arc::new(builder.build()),
        })
    }
}

impl Default for ChainProviderBuilder {
    /// Same as [`ChainProviderBuilder::new`]: an empty builder.
    fn default() -> Self {
        Self::new()
    }
}

/// A built provider; hand out one per page/worker.
#[wasm_bindgen]
pub struct ChainProviderHandle {
    inner: Arc<EmbeddedChainProvider>,
}

#[wasm_bindgen]
impl ChainProviderHandle {
    /// Open a connection to the chain identified by the `0x`-prefixed genesis
    /// hash. Rejects when the chain is not registered or the transport fails.
    pub async fn connect(&self, genesis_hash: &str) -> Result<Connection, JsError> {
        let genesis = parse_genesis(genesis_hash)?;
        let connection = self
            .inner
            .connect(genesis)
            .await
            .map_err(|err| JsError::new(&err.reason))?;
        let responses = connection.responses();
        Ok(Connection {
            inner: Arc::from(connection),
            responses: Arc::new(Mutex::new(responses)),
        })
    }

    /// Produce a warm-start database blob for the `0x`-prefixed genesis hash.
    /// Persist it and feed it back via
    /// [`setDatabase`](ChainProviderBuilder::set_database) on a later run.
    #[cfg(feature = "smoldot")]
    pub async fn snapshot(&self, genesis_hash: &str) -> Result<String, JsError> {
        let genesis = parse_genesis(genesis_hash)?;
        self.inner
            .snapshot(genesis)
            .await
            .map_err(|err| JsError::new(&err.reason))
    }
}

/// A live JSON-RPC connection: a raw string pipe.
#[wasm_bindgen]
pub struct Connection {
    inner: Arc<dyn JsonRpcConnection>,
    responses: Arc<Mutex<BoxStream<'static, String>>>,
}

#[wasm_bindgen]
impl Connection {
    /// Queue a JSON-RPC request string.
    pub fn send(&self, request: String) {
        self.inner.send(request);
    }

    /// Resolve with the next JSON-RPC response or notification, or
    /// `undefined` once the connection is closed or dead.
    #[wasm_bindgen(js_name = nextResponse)]
    pub async fn next_response(&self) -> Option<String> {
        let responses = Arc::clone(&self.responses);
        let mut responses = responses.lock().await;
        responses.next().await
    }

    /// Close the connection; pending `nextResponse()` calls resolve to
    /// `undefined`.
    pub fn close(&self) {
        self.inner.close();
    }
}

/// The genesis hashes of a network registered via
/// [`ChainProviderBuilder::add_network`], as `0x`-prefixed hex strings.
#[cfg(feature = "networks")]
#[wasm_bindgen]
pub struct NetworkChains {
    relay: String,
    asset_hub: String,
    bulletin: String,
    people: String,
}

#[cfg(feature = "networks")]
#[wasm_bindgen]
impl NetworkChains {
    /// Relay-chain genesis hash.
    #[wasm_bindgen(getter)]
    pub fn relay(&self) -> String {
        self.relay.clone()
    }

    /// Asset Hub genesis hash.
    #[wasm_bindgen(getter, js_name = assetHub)]
    pub fn asset_hub(&self) -> String {
        self.asset_hub.clone()
    }

    /// Bulletin-chain genesis hash.
    #[wasm_bindgen(getter)]
    pub fn bulletin(&self) -> String {
        self.bulletin.clone()
    }

    /// People-chain genesis hash.
    #[wasm_bindgen(getter)]
    pub fn people(&self) -> String {
        self.people.clone()
    }
}

#[cfg(feature = "networks")]
fn hex0x(bytes: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn parse_genesis(hex_str: &str) -> Result<[u8; 32], JsError> {
    hex::decode(hex_str.trim_start_matches("0x"))
        .map_err(|err| JsError::new(&format!("invalid genesis hash hex: {err}")))?
        .try_into()
        .map_err(|_| JsError::new("genesis hashes are 32 bytes"))
}
