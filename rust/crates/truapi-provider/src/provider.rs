//! Genesis-hash registry dispatching to the configured backend.

use std::collections::HashMap;

use truapi::latest::GenericError;
use truapi_platform::{ChainProvider, JsonRpcConnection};

use crate::config::ChainSource;

/// Builder collecting genesis-hash to [`ChainSource`] registrations.
#[derive(Debug, Default)]
pub struct NativeChainProviderBuilder {
    chains: HashMap<[u8; 32], ChainSource>,
}

impl NativeChainProviderBuilder {
    /// Create an empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `source` as the backend for the chain identified by
    /// `genesis_hash`. A later registration for the same hash replaces the
    /// earlier one.
    pub fn chain(mut self, genesis_hash: [u8; 32], source: ChainSource) -> Self {
        self.chains.insert(genesis_hash, source);
        self
    }

    /// Build the provider. Light-client resources start lazily on the first
    /// light-client connect.
    pub fn build(self) -> NativeChainProvider {
        NativeChainProvider {
            chains: self.chains,
            #[cfg(feature = "smoldot")]
            light: crate::light::LightState::new(),
        }
    }
}

/// In-process [`ChainProvider`] whose per-chain backend is a remote WebSocket
/// JSON-RPC node (all targets) or an embedded smoldot light client (native
/// targets).
///
/// Construct **one provider per host process** and share it (behind an `Arc`)
/// with every consumer: the provider owns the single light-client instance,
/// so host-internal flows (domain resolution, statement store) and product
/// connections share sync, peers, and warm state, while each connection keeps
/// its own isolated JSON-RPC queue and response stream.
///
/// The `responses()` stream of returned connections is take-once: the first
/// call yields the live stream, later calls yield an ended stream.
pub struct NativeChainProvider {
    chains: HashMap<[u8; 32], ChainSource>,
    #[cfg(feature = "smoldot")]
    light: crate::light::LightState,
}

impl NativeChainProvider {
    /// Start building a provider.
    pub fn builder() -> NativeChainProviderBuilder {
        NativeChainProviderBuilder::new()
    }

    /// Open a connection for `source`, using `chains` to resolve a parachain's
    /// relay entry.
    async fn connect_source(
        &self,
        source: &ChainSource,
        chains: &HashMap<[u8; 32], ChainSource>,
    ) -> Result<Box<dyn JsonRpcConnection>, GenericError> {
        match source {
            #[cfg(feature = "ws")]
            ChainSource::RpcNode { url } => crate::ws::connect(url.clone()).await,
            #[cfg(feature = "smoldot")]
            ChainSource::LightClient { .. } => self.light.connect(chains, source).await,
            #[cfg(not(any(feature = "ws", feature = "smoldot")))]
            _ => match *source {},
        }
    }
}

#[truapi_platform::async_trait]
impl ChainProvider for NativeChainProvider {
    async fn connect(
        &self,
        genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, GenericError> {
        // Explicit registrations win; otherwise the bundled catalog resolves
        // the whole network — relay wiring and statement placement included —
        // from the genesis hash alone.
        if let Some(source) = self.chains.get(&genesis_hash) {
            return self.connect_source(source, &self.chains).await;
        }
        #[cfg(feature = "networks")]
        if let Some(catalog) = crate::networks::catalog_network_chains(genesis_hash) {
            let source = catalog
                .get(&genesis_hash)
                .expect("catalog_network_chains includes the queried genesis");
            return self.connect_source(source, &catalog).await;
        }
        let genesis_hex = hex::encode(genesis_hash);
        Err(GenericError {
            reason: format!("no chain registered for genesis 0x{genesis_hex}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use truapi_platform::ChainProvider;

    use super::NativeChainProvider;
    use crate::config::ChainSource;

    #[test]
    fn unknown_genesis_is_an_error_naming_the_hash() {
        let provider = NativeChainProvider::builder().build();
        let error = futures::executor::block_on(provider.connect([0xab; 32]))
            .err()
            .expect("connect must fail for an unregistered genesis");
        assert!(error.reason.contains(&"ab".repeat(32)));
    }

    #[cfg(feature = "ws")]
    #[test]
    // On wasm32 without the smoldot backend the enum has a single variant,
    // making the let-else irrefutable there.
    #[allow(irrefutable_let_patterns)]
    fn later_registration_wins() {
        let first = url::Url::parse("ws://first.example").expect("static URL parses");
        let second = url::Url::parse("ws://second.example").expect("static URL parses");
        let provider = NativeChainProvider::builder()
            .chain([1; 32], ChainSource::rpc_node(first))
            .chain([1; 32], ChainSource::rpc_node(second.clone()))
            .build();
        let ChainSource::RpcNode { url } = &provider.chains[&[1; 32]] else {
            panic!("expected an RpcNode source");
        };
        assert_eq!(*url, second);
    }
}
