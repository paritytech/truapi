//! Per-chain backend configuration.

/// Where a chain's JSON-RPC service comes from.
///
/// Registered per genesis hash on a
/// [`EmbeddedChainProviderBuilder`](crate::EmbeddedChainProviderBuilder).
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
        /// JSON chain specification of the target chain.
        specification: std::borrow::Cow<'static, str>,
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

    /// Start configuring an embedded light-client backend for `specification`;
    /// finish with [`LightClientBuilder::build`]. A `ChainSource` is per-chain
    /// transport config — a parachain's relay is provider topology, kept apart.
    #[cfg(feature = "smoldot")]
    pub fn light_client(
        specification: impl Into<std::borrow::Cow<'static, str>>,
    ) -> LightClientBuilder {
        LightClientBuilder {
            specification: specification.into(),
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
    specification: std::borrow::Cow<'static, str>,
    database_content: Option<String>,
    statement_protocol: bool,
}

#[cfg(feature = "smoldot")]
impl LightClientBuilder {
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
            specification: self.specification,
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
            .database("db")
            .without_statement_protocol()
            .build();
        let ChainSource::LightClient {
            database_content,
            statement_protocol,
            ..
        } = source
        else {
            panic!("expected a LightClient source");
        };
        assert_eq!(database_content.as_deref(), Some("db"));
        assert!(!statement_protocol);
    }
}
