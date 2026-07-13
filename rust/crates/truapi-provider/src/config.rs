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

    /// Embedded light-client backend for the given chain spec.
    ///
    /// The statement-store networking protocol is enabled by default; opt out
    /// with [`ChainSource::without_statement_protocol`].
    #[cfg(feature = "smoldot")]
    pub fn light_client(chain_spec: impl Into<std::sync::Arc<str>>) -> Self {
        ChainSource::LightClient {
            chain_spec: chain_spec.into(),
            relay: None,
            database_content: None,
            statement_protocol: true,
        }
    }

    /// Declare the chain a parachain of the given relay-chain genesis hash.
    ///
    /// # Panics
    ///
    /// Panics when called on a non-[`ChainSource::LightClient`] source.
    #[cfg(feature = "smoldot")]
    pub fn with_relay(mut self, relay_genesis: [u8; 32]) -> Self {
        match &mut self {
            ChainSource::LightClient { relay, .. } => *relay = Some(relay_genesis),
            #[allow(unreachable_patterns)]
            _ => panic!("with_relay is only valid on ChainSource::LightClient"),
        }
        self
    }

    /// Attach a warm-start database blob.
    ///
    /// # Panics
    ///
    /// Panics when called on a non-[`ChainSource::LightClient`] source.
    #[cfg(feature = "smoldot")]
    pub fn with_database(mut self, database: String) -> Self {
        match &mut self {
            ChainSource::LightClient {
                database_content, ..
            } => *database_content = Some(database),
            #[allow(unreachable_patterns)]
            _ => panic!("with_database is only valid on ChainSource::LightClient"),
        }
        self
    }

    /// Disable the statement-store networking protocol for this chain.
    ///
    /// # Panics
    ///
    /// Panics when called on a non-[`ChainSource::LightClient`] source.
    #[cfg(feature = "smoldot")]
    pub fn without_statement_protocol(mut self) -> Self {
        match &mut self {
            ChainSource::LightClient {
                statement_protocol, ..
            } => *statement_protocol = false,
            #[allow(unreachable_patterns)]
            _ => panic!("without_statement_protocol is only valid on ChainSource::LightClient"),
        }
        self
    }
}

#[cfg(all(test, feature = "smoldot"))]
mod tests {
    use super::ChainSource;

    #[test]
    fn light_client_enables_statement_protocol_by_default() {
        let ChainSource::LightClient {
            statement_protocol, ..
        } = ChainSource::light_client("{}")
        else {
            panic!("expected a LightClient source");
        };
        assert!(statement_protocol);
    }

    #[test]
    fn chainers_set_fields() {
        let source = ChainSource::light_client("{}")
            .with_relay([7; 32])
            .with_database("db".to_owned())
            .without_statement_protocol();
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
