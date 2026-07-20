//! Role-neutral runtime services shared by product-facing runtimes.
//!
//! This module owns only infrastructure that is valid for both pairing hosts
//! and signing hosts. Pairing state, signing state, active sessions, and role
//! controls live on the concrete role objects.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::chain_runtime::{ChainRuntime, RuntimeChainProvider, RuntimeFailure};
use crate::runtime::bulletin_rpc::BulletinRpc;
use crate::runtime::statement_store_rpc::StatementStoreRpc;
use crate::subscription::Spawner;
use async_trait::async_trait;
use truapi_platform::{HostInfo, JsonRpcConnection, Platform};

/// Upper bound on the in-core preimage cache. The cache is a bridge until
/// content propagates to the lookup backend, not a store, so it stays small.
const PREIMAGE_CACHE_MAX_BYTES: usize = 16 * 1024 * 1024;

/// Infrastructure shared by all product runtimes created from one host role.
pub(crate) struct RuntimeServices {
    pub(crate) platform: Arc<dyn Platform>,
    /// Host identity reported to products via `System::host_info`.
    pub(crate) host_info: HostInfo,
    pub(crate) chain: ChainRuntime,
    pub(crate) statement_store: StatementStoreRpc,
    /// In-core Bulletin submission over the configured Bulletin chain.
    pub(crate) bulletin: BulletinRpc,
    /// Values from confirmed in-core submissions, served to `lookup_subscribe`
    /// until the host's content backend has them. Byte-bounded, oldest-first.
    preimage_cache: Mutex<PreimageCache>,
    pub(crate) spawner: Spawner,
    next_core_instance: AtomicU64,
}

impl RuntimeServices {
    /// Build role-neutral runtime services from the platform, the host
    /// identity reported to products, the People-chain genesis hash used by
    /// statement-store backed protocols, and the Bulletin-chain genesis hash
    /// used for in-core preimage submission.
    pub(crate) fn new(
        platform: Arc<dyn Platform>,
        host_info: HostInfo,
        people_chain_genesis_hash: [u8; 32],
        bulletin_chain_genesis_hash: [u8; 32],
        spawner: Spawner,
    ) -> Arc<Self> {
        let chain_provider = Arc::new(HostChainProvider {
            platform: platform.clone(),
        });
        let chain = ChainRuntime::new(chain_provider, spawner.clone());
        let statement_store =
            StatementStoreRpc::new(platform.clone(), people_chain_genesis_hash, spawner.clone());
        let bulletin = BulletinRpc::new(chain.clone(), bulletin_chain_genesis_hash);
        Arc::new(Self {
            platform,
            host_info,
            chain,
            statement_store,
            bulletin,
            preimage_cache: Mutex::new(PreimageCache::default()),
            spawner,
            next_core_instance: AtomicU64::new(1),
        })
    }

    pub(crate) fn next_core_instance(&self) -> u64 {
        self.next_core_instance.fetch_add(1, Ordering::Relaxed)
    }

    /// Store a preimage value under its key for later lookup hits.
    pub(crate) fn cache_preimage(&self, key: [u8; 32], value: Vec<u8>) {
        self.preimage_cache
            .lock()
            .expect("preimage cache mutex poisoned")
            .insert(key, value);
    }

    /// Return a cached preimage value for `key`, if present.
    pub(crate) fn cached_preimage(&self, key: &[u8; 32]) -> Option<Vec<u8>> {
        self.preimage_cache
            .lock()
            .expect("preimage cache mutex poisoned")
            .get(key)
    }
}

/// Byte-bounded, insertion-ordered preimage cache.
#[derive(Default)]
struct PreimageCache {
    entries: VecDeque<([u8; 32], Vec<u8>)>,
    total_bytes: usize,
}

impl PreimageCache {
    fn insert(&mut self, key: [u8; 32], value: Vec<u8>) {
        if value.len() > PREIMAGE_CACHE_MAX_BYTES {
            return;
        }
        if let Some(index) = self
            .entries
            .iter()
            .position(|(existing, _)| *existing == key)
        {
            let (_, old) = self.entries.remove(index).expect("index in range");
            self.total_bytes -= old.len();
        }
        self.total_bytes += value.len();
        self.entries.push_back((key, value));
        while self.total_bytes > PREIMAGE_CACHE_MAX_BYTES {
            let Some((_, evicted)) = self.entries.pop_front() else {
                break;
            };
            self.total_bytes -= evicted.len();
        }
    }

    fn get(&self, key: &[u8; 32]) -> Option<Vec<u8>> {
        self.entries
            .iter()
            .find(|(existing, _)| existing == key)
            .map(|(_, value)| value.clone())
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
            .map_err(|err| {
                RuntimeFailure::unavailable_with_reason("remote_chain_connect", format!("{err:?}"))
            })
    }
}
