//! Swift/UniFFI bindings (the `uniffi` feature, native targets).
//!
//! Exposes the embedded smoldot [`ChainProvider`](truapi_platform::ChainProvider)
//! to Swift (and other UniFFI targets): build a provider, connect to a chain by
//! genesis hash, and drive the raw JSON-RPC string pipe. Chain specs, relay
//! topology, and statement-store placement come from the bundled network
//! catalog, so the only argument a host supplies is the genesis hash.
//!
//! Inbound responses are delivered to a foreign [`ChainMessageListener`]; a
//! background thread pumps the response stream and invokes it until the
//! connection closes.

use std::sync::Arc;

use futures::executor::block_on;
use futures::stream::StreamExt;
use truapi_platform::{ChainProvider as _, JsonRpcConnection};

use crate::EmbeddedChainProvider;

/// Errors surfaced to the foreign caller.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ProviderError {
    /// The chain could not be connected (unknown genesis, transport failure).
    #[error("{reason}")]
    Connect {
        /// Human-readable failure reason.
        reason: String,
    },
    /// The genesis hash was not exactly 32 bytes.
    #[error("genesis hash must be 32 bytes")]
    BadGenesis,
}

/// Sink for a connection's inbound JSON-RPC responses and notifications,
/// implemented on the foreign (Swift) side.
#[uniffi::export(with_foreign)]
pub trait ChainMessageListener: Send + Sync {
    /// Called for each JSON-RPC response or notification string.
    fn on_message(&self, message: String);
    /// Called once the connection's response stream ends.
    fn on_closed(&self);
}

/// Embedded-smoldot chain provider. Construct one per process and share it;
/// every connection runs on the single embedded light client.
#[derive(uniffi::Object)]
pub struct ChainProvider {
    inner: EmbeddedChainProvider,
}

#[uniffi::export]
impl ChainProvider {
    /// Create a provider backed by the bundled network catalog.
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: EmbeddedChainProvider::builder().build(),
        })
    }

    /// Open a connection to the chain identified by `genesis_hash` (32 bytes).
    /// The network is resolved from the catalog; responses are delivered to
    /// `listener` until the connection closes.
    pub fn connect(
        &self,
        genesis_hash: Vec<u8>,
        listener: Arc<dyn ChainMessageListener>,
    ) -> Result<Arc<ChainConnection>, ProviderError> {
        let genesis: [u8; 32] = genesis_hash
            .try_into()
            .map_err(|_| ProviderError::BadGenesis)?;
        let connection =
            block_on(self.inner.connect(genesis)).map_err(|error| ProviderError::Connect {
                reason: error.reason,
            })?;
        let connection: Arc<dyn JsonRpcConnection> = Arc::from(connection);

        let mut responses = connection.responses();
        std::thread::spawn(move || {
            block_on(async move {
                while let Some(message) = responses.next().await {
                    listener.on_message(message);
                }
                listener.on_closed();
            });
        });

        Ok(Arc::new(ChainConnection { inner: connection }))
    }
}

/// A live JSON-RPC connection: a raw string pipe to one chain.
#[derive(uniffi::Object)]
pub struct ChainConnection {
    inner: Arc<dyn JsonRpcConnection>,
}

#[uniffi::export]
impl ChainConnection {
    /// Queue a JSON-RPC request string.
    pub fn send(&self, request: String) {
        self.inner.send(request);
    }

    /// Close the connection; the listener's `on_closed` fires once the stream
    /// ends.
    pub fn close(&self) {
        self.inner.close();
    }
}
