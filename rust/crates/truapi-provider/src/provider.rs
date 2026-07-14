//! Genesis-hash registry dispatching each connection to its backend.
//!
//! A parachain's relay is provider topology (see `relays`), not part of
//! [`ChainSource`]; the light backend brings the relay up behind the parachain.

use std::collections::HashMap;

use truapi::latest::GenericError;
use truapi_platform::{ChainProvider, JsonRpcConnection};

use crate::config::ChainSource;
use crate::error::ProviderError;

/// Builder collecting genesis-hash to [`ChainSource`] registrations.
#[derive(Debug, Default)]
pub struct EmbeddedChainProviderBuilder {
    chains: HashMap<[u8; 32], ChainSource>,
    /// The relay each parachain syncs through, keyed by parachain genesis hash
    /// (exactly one per parachain).
    #[cfg(feature = "smoldot")]
    relays: HashMap<[u8; 32], [u8; 32]>,
    /// Warm-start database blobs keyed by genesis hash, applied to a
    /// light-client chain at connect time if it has no explicit blob.
    #[cfg(feature = "smoldot")]
    databases: HashMap<[u8; 32], String>,
}

impl EmbeddedChainProviderBuilder {
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

    /// Register `source` as a parachain syncing through the relay registered
    /// under `relay_genesis`. A later registration for `genesis_hash` wins.
    #[cfg(feature = "smoldot")]
    pub(crate) fn parachain(
        mut self,
        genesis_hash: [u8; 32],
        source: ChainSource,
        relay_genesis: [u8; 32],
    ) -> Self {
        self.chains.insert(genesis_hash, source);
        self.relays.insert(genesis_hash, relay_genesis);
        self
    }

    /// Seed a warm-start database blob (previously produced by
    /// [`EmbeddedChainProvider::snapshot`]) for `genesis_hash`, so its light
    /// client resumes from that finalized state instead of syncing from the
    /// chain-spec checkpoint. Applies to catalog-resolved chains too, and is
    /// ignored for a chain that already carries an explicit blob.
    #[cfg(feature = "smoldot")]
    pub fn database(mut self, genesis_hash: [u8; 32], blob: String) -> Self {
        self.databases.insert(genesis_hash, blob);
        self
    }

    /// Build the provider. Light-client resources start lazily on the first
    /// light-client connect.
    pub fn build(self) -> EmbeddedChainProvider {
        EmbeddedChainProvider {
            chains: self.chains,
            #[cfg(feature = "smoldot")]
            relays: self.relays,
            #[cfg(feature = "smoldot")]
            databases: self.databases,
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
pub struct EmbeddedChainProvider {
    chains: HashMap<[u8; 32], ChainSource>,
    /// The relay each explicitly-registered parachain syncs through; catalog
    /// parachains carry theirs in the catalog entry.
    #[cfg(feature = "smoldot")]
    relays: HashMap<[u8; 32], [u8; 32]>,
    #[cfg(feature = "smoldot")]
    databases: HashMap<[u8; 32], String>,
    #[cfg(feature = "smoldot")]
    light: crate::light::LightState,
}

impl EmbeddedChainProvider {
    /// Start building a provider.
    pub fn builder() -> EmbeddedChainProviderBuilder {
        EmbeddedChainProviderBuilder::new()
    }

    /// Open a connection for `source`; for a parachain, `relay`/`chains` give
    /// the light backend the relay to sync it through.
    #[cfg_attr(not(feature = "smoldot"), allow(unused_variables))]
    async fn connect_source(
        &self,
        source: &ChainSource,
        chains: &HashMap<[u8; 32], ChainSource>,
        relay: Option<[u8; 32]>,
    ) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
        match source {
            #[cfg(feature = "ws")]
            ChainSource::RpcNode { url } => crate::ws::connect(url.clone()).await,
            #[cfg(feature = "smoldot")]
            ChainSource::LightClient { .. } => self.light.connect(chains, source, relay).await,
        }
    }

    /// Apply a seeded warm-start database blob to `source` if one exists for
    /// `genesis_hash` and the source is a light client with no explicit blob.
    #[cfg(feature = "smoldot")]
    fn with_seeded_database(&self, genesis_hash: [u8; 32], mut source: ChainSource) -> ChainSource {
        if let Some(blob) = self.databases.get(&genesis_hash)
            && let ChainSource::LightClient {
                database_content, ..
            } = &mut source
            && database_content.is_none()
        {
            *database_content = Some(blob.clone());
        }
        source
    }

    #[cfg(not(feature = "smoldot"))]
    fn with_seeded_database(&self, _genesis_hash: [u8; 32], source: ChainSource) -> ChainSource {
        source
    }
}

/// Max size for a [`snapshot`](EmbeddedChainProvider::snapshot) database blob.
#[cfg(feature = "smoldot")]
const SNAPSHOT_MAX_BYTES: usize = 8_000_000;

#[cfg(feature = "smoldot")]
impl EmbeddedChainProvider {
    /// Produce a warm-start database blob for `genesis_hash` by asking the
    /// embedded light client for its finalized-database snapshot.
    ///
    /// Persist the returned string and feed it back on a later run via
    /// [`EmbeddedChainProviderBuilder::database`] so the chain resumes from
    /// finalized state instead of re-syncing from the checkpoint. Meaningful
    /// only for light-client chains; a remote-node chain never answers and the
    /// call resolves once that connection ends.
    pub async fn snapshot(&self, genesis_hash: [u8; 32]) -> Result<String, GenericError> {
        use futures::stream::StreamExt;

        let connection = self.connect(genesis_hash).await?;
        let mut responses = connection.responses();
        let id = "truapi-provider:finalizedDatabase";
        connection.send(format!(
            concat!(
                r#"{{"jsonrpc":"2.0","id":"{}","#,
                r#""method":"chainHead_unstable_finalizedDatabase","params":[{}]}}"#
            ),
            id, SNAPSHOT_MAX_BYTES,
        ));
        while let Some(frame) = responses.next().await {
            if let Some(result) = crate::error::result_string_for_id(&frame, id) {
                connection.close();
                return Ok(result);
            }
        }
        connection.close();
        Err(ProviderError::Transport {
            reason: "connection ended before the finalized-database snapshot".to_owned(),
        }
        .into())
    }
}

#[cfg(all(test, feature = "smoldot"))]
impl EmbeddedChainProvider {
    /// Number of implicit relay chains the shared light client currently holds.
    pub(crate) fn relay_count(&self) -> usize {
        self.light.relay_count()
    }
}

#[truapi_platform::async_trait]
impl ChainProvider for EmbeddedChainProvider {
    #[tracing::instrument(skip_all, fields(genesis = %hex::encode(genesis_hash)))]
    async fn connect(
        &self,
        genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, GenericError> {
        // Explicit registrations win; otherwise the catalog resolves the whole
        // network from the genesis hash alone.
        if let Some(source) = self.chains.get(&genesis_hash) {
            let source = self.with_seeded_database(genesis_hash, source.clone());
            #[cfg(feature = "smoldot")]
            let relay = self.relays.get(&genesis_hash).copied();
            #[cfg(not(feature = "smoldot"))]
            let relay = None;
            return Ok(self.connect_source(&source, &self.chains, relay).await?);
        }
        #[cfg(feature = "networks")]
        if let Some((catalog, relay)) = crate::networks::catalog_network_chains(genesis_hash) {
            let source = catalog
                .get(&genesis_hash)
                .expect("catalog_network_chains includes the queried genesis")
                .clone();
            let source = self.with_seeded_database(genesis_hash, source);
            return Ok(self.connect_source(&source, &catalog, relay).await?);
        }
        Err(ProviderError::UnknownGenesis {
            genesis: genesis_hash,
        }
        .into())
    }
}

#[cfg(test)]
mod tests {
    use truapi_platform::ChainProvider;

    use super::EmbeddedChainProvider;
    use crate::config::ChainSource;

    #[test]
    fn unknown_genesis_is_an_error_naming_the_hash() {
        let provider = EmbeddedChainProvider::builder().build();
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
        let provider = EmbeddedChainProvider::builder()
            .chain([1; 32], ChainSource::rpc_node(first))
            .chain([1; 32], ChainSource::rpc_node(second.clone()))
            .build();
        let ChainSource::RpcNode { url } = &provider.chains[&[1; 32]] else {
            panic!("expected an RpcNode source");
        };
        assert_eq!(*url, second);
    }
}
