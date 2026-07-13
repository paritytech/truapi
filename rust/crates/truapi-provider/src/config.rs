//! Per-chain backend configuration.

/// Where a chain's JSON-RPC service comes from.
///
/// Registered per genesis hash on a
/// [`NativeChainProviderBuilder`](crate::NativeChainProviderBuilder).
#[derive(Debug, Clone)]
pub enum ChainSource {
    /// Remote JSON-RPC node reached over `ws://` or `wss://`.
    #[cfg(feature = "ws")]
    #[non_exhaustive]
    RpcNode {
        /// WebSocket endpoint of the node.
        url: url::Url,
    },

    /// Embedded smoldot light client.
    #[cfg(feature = "smoldot")]
    #[non_exhaustive]
    LightClient {
        /// Chain-spec JSON of the target chain.
        chain_spec: std::sync::Arc<str>,
        /// Genesis hash of the relay chain when the target is a parachain.
        ///
        /// Must name another [`ChainSource::LightClient`] entry registered on
        /// the same provider.
        relay: Option<[u8; 32]>,
        /// Warm-start database blob previously returned by the
        /// `chainHead_unstable_finalizedDatabase` JSON-RPC function. Invalid
        /// blobs are silently ignored by smoldot.
        database_content: Option<String>,
        /// Whether the statement-store networking protocol is enabled.
        statement_protocol: bool,
    },
}

impl ChainSource {
    /// Remote JSON-RPC node backend.
    #[cfg(feature = "ws")]
    pub fn rpc_node(url: url::Url) -> Self {
        ChainSource::RpcNode { url }
    }

    /// Start configuring an embedded light-client backend for `chain_spec`.
    ///
    /// Returns a [`LightClientBuilder`]; set relay/database/statement options
    /// on it and finish with [`LightClientBuilder::build`]. The light-client
    /// options live on the builder rather than as setters on [`ChainSource`]
    /// so they cannot be called on a non-light source.
    #[cfg(feature = "smoldot")]
    pub fn light_client(chain_spec: impl Into<std::sync::Arc<str>>) -> LightClientBuilder {
        LightClientBuilder {
            chain_spec: chain_spec.into(),
            relay: None,
            database_content: None,
            statement_protocol: true,
        }
    }
}

/// Builder for a [`ChainSource::LightClient`].
///
/// The statement-store networking protocol is enabled by default; opt out with
/// [`without_statement_protocol`](Self::without_statement_protocol).
#[cfg(feature = "smoldot")]
#[derive(Debug, Clone)]
pub struct LightClientBuilder {
    chain_spec: std::sync::Arc<str>,
    relay: Option<[u8; 32]>,
    database_content: Option<String>,
    statement_protocol: bool,
}

#[cfg(feature = "smoldot")]
impl LightClientBuilder {
    /// Declare the chain a parachain of the relay identified by `relay_genesis`
    /// (which must name another light-client chain registered on the same
    /// provider).
    pub fn relay(mut self, relay_genesis: [u8; 32]) -> Self {
        self.relay = Some(relay_genesis);
        self
    }

    /// Attach a warm-start database blob previously returned by the
    /// `chainHead_unstable_finalizedDatabase` JSON-RPC function. Invalid blobs
    /// are silently ignored by smoldot.
    pub fn database(mut self, database: impl Into<String>) -> Self {
        self.database_content = Some(database.into());
        self
    }

    /// Disable the statement-store networking protocol for this chain.
    pub fn without_statement_protocol(mut self) -> Self {
        self.statement_protocol = false;
        self
    }

    /// Finish, producing a [`ChainSource::LightClient`].
    pub fn build(self) -> ChainSource {
        ChainSource::LightClient {
            chain_spec: self.chain_spec,
            relay: self.relay,
            database_content: self.database_content,
            statement_protocol: self.statement_protocol,
        }
    }
}

#[cfg(feature = "smoldot")]
impl From<LightClientBuilder> for ChainSource {
    fn from(builder: LightClientBuilder) -> Self {
        builder.build()
    }
}

#[cfg(all(test, feature = "smoldot"))]
mod tests {
    use super::ChainSource;

    #[test]
    fn light_client_enables_statement_protocol_by_default() {
        let ChainSource::LightClient {
            statement_protocol, ..
        } = ChainSource::light_client("{}").build()
        else {
            panic!("expected a LightClient source");
        };
        assert!(statement_protocol);
    }

    #[test]
    fn builder_sets_fields() {
        let source = ChainSource::light_client("{}")
            .relay([7; 32])
            .database("db")
            .without_statement_protocol()
            .build();
        let ChainSource::LightClient {
            relay,
            database_content,
            statement_protocol,
            ..
        } = source
        else {
            panic!("expected a LightClient source");
        };
        assert_eq!(relay, Some([7; 32]));
        assert_eq!(database_content.as_deref(), Some("db"));
        assert!(!statement_protocol);
    }
}
