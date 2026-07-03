//! Role-neutral runtime services shared by product-facing runtimes.
//!
//! This module owns only infrastructure that is valid for both pairing hosts
//! and signing hosts. Pairing state, signing state, active sessions, and role
//! controls live on the concrete role objects.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::chain_runtime::{ChainRuntime, RuntimeChainProvider, RuntimeFailure};
use crate::runtime::statement_store_rpc::StatementStoreRpc;
use crate::subscription::Spawner;
use async_trait::async_trait;
use truapi_platform::{JsonRpcConnection, Platform};

/// Infrastructure shared by all product runtimes created from one host role.
pub(crate) struct RuntimeServices {
    pub(crate) platform: Arc<dyn Platform>,
    pub(crate) chain: ChainRuntime,
    pub(crate) statement_store: StatementStoreRpc,
    pub(crate) spawner: Spawner,
    next_core_instance: AtomicU64,
}

impl RuntimeServices {
    /// Build role-neutral runtime services from the platform and People-chain
    /// genesis hash used by statement-store backed protocols.
    pub(crate) fn new(
        platform: Arc<dyn Platform>,
        people_chain_genesis_hash: [u8; 32],
        spawner: Spawner,
    ) -> Arc<Self> {
        let chain_provider = Arc::new(HostChainProvider {
            platform: platform.clone(),
        });
        let chain = ChainRuntime::new(chain_provider, spawner.clone());
        let statement_store =
            StatementStoreRpc::new(platform.clone(), people_chain_genesis_hash, spawner.clone());
        Arc::new(Self {
            platform,
            chain,
            statement_store,
            spawner,
            next_core_instance: AtomicU64::new(1),
        })
    }

    pub(crate) fn next_core_instance(&self) -> u64 {
        self.next_core_instance.fetch_add(1, Ordering::Relaxed)
    }
}

/// Adapter from `truapi_platform::ChainProvider` into the
/// [`RuntimeChainProvider`] surface the chain runtime expects.
struct HostChainProvider {
    platform: Arc<dyn Platform>,
}

#[async_trait]
impl RuntimeChainProvider for HostChainProvider {
    async fn connect(
        &self,
        genesis_hash: Vec<u8>,
    ) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
        let genesis_hash: [u8; 32] = genesis_hash.try_into().map_err(|genesis_hash: Vec<u8>| {
            RuntimeFailure::host_failure(
                "remote_chain_connect",
                format!("genesis_hash must be 32 bytes, got {}", genesis_hash.len()),
            )
        })?;
        self.platform
            .connect(genesis_hash)
            .await
            .map(Arc::from)
            .map_err(|_| RuntimeFailure::unavailable("remote_chain_connect"))
    }
}
