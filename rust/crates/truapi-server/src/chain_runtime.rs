//! ChainHead v1 state machine used by `ProductRuntimeHost`.
//!
//! [`ChainRuntime`] keeps one [`ChainConnection`] per chain (keyed by genesis
//! hash) on top of the platform-provided [`JsonRpcConnection`]. The generic
//! JSON-RPC mechanics are delegated to [`crate::host_rpc_client`], while
//! `subxt-rpcs` owns the raw `chainHead_v1` method shapes and event parsing.
//! This module keeps the TrUAPI-facing local follow ids and maps subxt DTOs to
//! public v01 [`RemoteChainHeadFollowItem`] values.
//!
//! The chain-side traits return [`RuntimeFailure`], a local classification
//! that the [`crate::runtime`] layer maps to [`truapi::CallError`] variants
//! (`Unsupported`, `HostFailure`, ...). This avoids leaking json-rpc plumbing
//! into the public API.

// Temporary for this stack layer: runtime wiring lands in the next child PR.
#![allow(dead_code)]

use core::pin::Pin;
use core::task::{Context, Poll};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use futures::FutureExt;
use futures::channel::mpsc;
use futures::future::{AbortHandle, Abortable};
use futures::future::{BoxFuture, Shared};
use futures::stream::BoxStream;
use futures::{Stream, StreamExt};
use parity_scale_codec::{Decode, Error as ScaleError, Input};
use primitive_types::H256;
use serde::de::{Deserializer, Error as DeError};
use serde_json::Value;
use subxt_rpcs::client::RpcClient;
use subxt_rpcs::methods::chain_head as subxt_chain;
use subxt_rpcs::{ChainHeadRpcMethods, Error as SubxtRpcError, RpcConfig};
use tracing::instrument;
use truapi::v01::{
    OperationStartedResult, RemoteChainHeadBodyRequest, RemoteChainHeadBodyResponse,
    RemoteChainHeadCallRequest, RemoteChainHeadCallResponse, RemoteChainHeadContinueRequest,
    RemoteChainHeadFollowItem, RemoteChainHeadFollowRequest, RemoteChainHeadHeaderRequest,
    RemoteChainHeadHeaderResponse, RemoteChainHeadStopOperationRequest,
    RemoteChainHeadStorageRequest, RemoteChainHeadStorageResponse, RemoteChainHeadUnpinRequest,
    RemoteChainSpecChainNameResponse, RemoteChainSpecGenesisHashResponse,
    RemoteChainSpecPropertiesResponse, RemoteChainTransactionBroadcastRequest,
    RemoteChainTransactionBroadcastResponse, RemoteChainTransactionStopRequest, RuntimeApi,
    RuntimeSpec, RuntimeType, StorageQueryItem, StorageQueryType, StorageResultItem,
};
use truapi_platform::JsonRpcConnection;

use crate::host_rpc_client::HostRpcClient;
use crate::subscription::Spawner;

const FOLLOW_METHOD: &str = "remote_chain_head_follow";

struct TruapiRpcConfig;

impl RpcConfig for TruapiRpcConfig {
    type Header = RawHeader;
    type Hash = H256;
    type AccountId = ();
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawHeader(Vec<u8>);

impl Decode for RawHeader {
    fn decode<I: Input>(input: &mut I) -> Result<Self, ScaleError> {
        let Some(len) = input.remaining_len()? else {
            return Err("raw header input length is unknown".into());
        };
        let mut bytes = vec![0u8; len];
        input.read(&mut bytes)?;
        Ok(Self(bytes))
    }
}

impl<'de> serde::Deserialize<'de> for RawHeader {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = subxt_chain::Bytes::deserialize(deserializer).map_err(D::Error::custom)?;
        Ok(Self(bytes.0))
    }
}

/// Shared, single-flight `chainHead_v1_follow` setup keyed by local follow id.
/// Concurrent callers for the same id await one in-flight request rather than
/// each opening (and leaking) a separate remote subscription.
type FollowSetup = Shared<BoxFuture<'static, Result<String, RuntimeFailure>>>;

/// Shared, single-flight provider connect keyed by genesis hash. Concurrent
/// first connections for the same chain await one in-flight `connect` rather
/// than each opening a connection and orphaning all but the last insert.
type ConnectionSetup = Shared<BoxFuture<'static, Result<Arc<ChainConnection>, RuntimeFailure>>>;

/// Classification of framework-level chain failures separate from JSON-RPC
/// domain errors. Maps cleanly to [`truapi::CallError`] variants at the
/// `ProductRuntimeHost` boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeFailureKind {
    /// Backend is not wired or refused the request for plumbing reasons.
    Unavailable,
    /// Backend responded but the payload was malformed or the call failed.
    HostFailure,
}

/// Framework-level chain failure with a diagnostic reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeFailure {
    kind: RuntimeFailureKind,
    method: &'static str,
    reason: Option<String>,
}

impl RuntimeFailure {
    /// Backend refused the call for unavailability reasons (no provider, the
    /// connection died, etc.).
    pub fn unavailable(method: &'static str) -> Self {
        Self {
            kind: RuntimeFailureKind::Unavailable,
            method,
            reason: None,
        }
    }

    /// Backend produced a structural error (malformed json-rpc, unexpected
    /// shape, ...).
    pub fn host_failure(method: &'static str, reason: impl Into<String>) -> Self {
        Self {
            kind: RuntimeFailureKind::HostFailure,
            method,
            reason: Some(reason.into()),
        }
    }

    /// Failure classification.
    pub fn kind(&self) -> RuntimeFailureKind {
        self.kind
    }

    /// Method tag the failure originated from.
    #[cfg(test)]
    fn method(&self) -> &'static str {
        self.method
    }

    /// Diagnostic reason. Always non-empty for `HostFailure`.
    pub fn reason(&self) -> String {
        match &self.reason {
            Some(reason) => format!("{}: {}", self.method, reason),
            None => self.method.to_string(),
        }
    }

    /// Re-tag this failure under `method`, preserving its kind and reason.
    fn reclassify(&self, method: &'static str) -> RuntimeFailure {
        match self.kind() {
            RuntimeFailureKind::Unavailable => RuntimeFailure::unavailable(method),
            RuntimeFailureKind::HostFailure => RuntimeFailure::host_failure(method, self.reason()),
        }
    }
}

/// Provider of `JsonRpcConnection` instances keyed by chain genesis hash.
/// Hosts plug in the platform-side `ChainProvider`.
#[async_trait::async_trait]
pub trait RuntimeChainProvider: Send + Sync {
    /// Open or reuse a JSON-RPC connection for the chain identified by
    /// `genesis_hash`.
    async fn connect(
        &self,
        genesis_hash: Vec<u8>,
    ) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure>;
}

/// chainHead-v1 state machine on top of a [`RuntimeChainProvider`].
///
/// Each method maps a typed v01 chain request to one or more json-rpc calls,
/// shares one `chainHead_v1_follow` subscription per (genesis_hash, local
/// follow id) pair, and parses follow events back into typed
/// [`RemoteChainHeadFollowItem`] values.
#[derive(Clone)]
pub struct ChainRuntime {
    provider: Arc<dyn RuntimeChainProvider>,
    spawner: Spawner,
    connections: Arc<Mutex<HashMap<String, Arc<ChainConnection>>>>,
    connection_setups: Arc<Mutex<HashMap<String, ConnectionSetup>>>,
}

impl ChainRuntime {
    /// Build a `ChainRuntime` driven by `provider`. Background tasks (response
    /// pumps, follow setup) are spawned on `spawner`.
    pub fn new(provider: Arc<dyn RuntimeChainProvider>, spawner: Spawner) -> Self {
        Self {
            provider,
            spawner,
            connections: Arc::new(Mutex::new(HashMap::new())),
            connection_setups: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start (or attach to an existing) `chainHead_v1_follow` subscription.
    /// Returns a stream of typed follow items that closes when the remote
    /// sends `stop` or the connection drops.
    #[instrument(skip_all, fields(runtime.method = "chain_runtime.follow"))]
    pub fn remote_chain_head_follow(
        &self,
        follow_subscription_id: String,
        request: RemoteChainHeadFollowRequest,
    ) -> BoxStream<'static, RemoteChainHeadFollowItem> {
        let (tx, rx) = mpsc::unbounded();
        let runtime = self.clone();
        let cleanup_runtime = self.clone();
        let cleanup_genesis_hash = request.genesis_hash.clone();
        let cleanup_follow_id = follow_subscription_id.clone();

        let fut = async move {
            if runtime
                .start_follow(follow_subscription_id, request, Some(tx.clone()))
                .await
                .is_err()
            {
                let _ = tx.unbounded_send(FollowSignal::Interrupt);
            }
        };
        (self.spawner)(fut.boxed());

        ManagedSubscription::new(
            rx.boxed(),
            Some(Box::new(move || {
                cleanup_runtime.cleanup_follow(&cleanup_genesis_hash, &cleanup_follow_id);
            })),
        )
        .filter_map(|signal| async move {
            match signal {
                FollowSignal::Item(item) => Some(item),
                FollowSignal::Interrupt => None,
            }
        })
        .boxed()
    }

    /// Fetch a block header.
    #[instrument(skip_all, fields(runtime.method = "chain_runtime.header"))]
    pub async fn remote_chain_head_header(
        &self,
        request: RemoteChainHeadHeaderRequest,
    ) -> Result<RemoteChainHeadHeaderResponse, RuntimeFailure> {
        let method = "remote_chain_head_header";
        let connection = self.connection_for(method, &request.genesis_hash).await?;
        let remote_follow_id = self
            .ensure_follow_context(method, &connection, request.follow_subscription_id, false)
            .await?;

        let hash = hash_from_bytes(method, &request.hash)?;
        let header = connection
            .methods
            .chainhead_v1_header(&remote_follow_id, hash)
            .await
            .map_err(|err| rpc_failure(method, err))?
            .map(|header| header.0);
        Ok(RemoteChainHeadHeaderResponse { header })
    }

    /// Start a chainHead_v1_body operation.
    #[instrument(skip_all, fields(runtime.method = "chain_runtime.body"))]
    pub async fn remote_chain_head_body(
        &self,
        request: RemoteChainHeadBodyRequest,
    ) -> Result<RemoteChainHeadBodyResponse, RuntimeFailure> {
        let method = "remote_chain_head_body";
        let connection = self.connection_for(method, &request.genesis_hash).await?;
        let remote_follow_id = self
            .ensure_follow_context(method, &connection, request.follow_subscription_id, false)
            .await?;

        let operation = connection
            .methods
            .chainhead_v1_body(&remote_follow_id, hash_from_bytes(method, &request.hash)?)
            .await
            .map_err(|err| rpc_failure(method, err))
            .and_then(operation_started_result)?;
        Ok(RemoteChainHeadBodyResponse { operation })
    }

    /// Start a chainHead_v1_storage operation.
    #[instrument(skip_all, fields(runtime.method = "chain_runtime.storage"))]
    pub async fn remote_chain_head_storage(
        &self,
        request: RemoteChainHeadStorageRequest,
    ) -> Result<RemoteChainHeadStorageResponse, RuntimeFailure> {
        let method = "remote_chain_head_storage";
        let connection = self.connection_for(method, &request.genesis_hash).await?;
        let remote_follow_id = self
            .ensure_follow_context(method, &connection, request.follow_subscription_id, false)
            .await?;

        let items = request
            .items
            .iter()
            .map(map_storage_query_item)
            .collect::<Vec<_>>();

        let operation = connection
            .methods
            .chainhead_v1_storage(
                &remote_follow_id,
                hash_from_bytes(method, &request.hash)?,
                items,
                request.child_trie.as_deref(),
            )
            .await
            .map_err(|err| rpc_failure(method, err))
            .and_then(operation_started_result)?;
        Ok(RemoteChainHeadStorageResponse { operation })
    }

    /// Start a chainHead_v1_call operation.
    #[instrument(skip_all, fields(runtime.method = "chain_runtime.call"))]
    pub async fn remote_chain_head_call(
        &self,
        request: RemoteChainHeadCallRequest,
    ) -> Result<RemoteChainHeadCallResponse, RuntimeFailure> {
        let method = "remote_chain_head_call";
        let connection = self.connection_for(method, &request.genesis_hash).await?;
        let remote_follow_id = self
            .ensure_follow_context(method, &connection, request.follow_subscription_id, true)
            .await?;

        let operation = connection
            .methods
            .chainhead_v1_call(
                &remote_follow_id,
                hash_from_bytes(method, &request.hash)?,
                &request.function,
                &request.call_parameters,
            )
            .await
            .map_err(|err| rpc_failure(method, err))
            .and_then(operation_started_result)?;
        Ok(RemoteChainHeadCallResponse { operation })
    }

    /// Release pinned blocks.
    #[instrument(skip_all, fields(runtime.method = "chain_runtime.unpin"))]
    pub async fn remote_chain_head_unpin(
        &self,
        request: RemoteChainHeadUnpinRequest,
    ) -> Result<(), RuntimeFailure> {
        let method = "remote_chain_head_unpin";
        let connection = self.connection_for(method, &request.genesis_hash).await?;
        let remote_follow_id = self
            .ensure_follow_context(method, &connection, request.follow_subscription_id, false)
            .await?;
        for hash in request.hashes {
            connection
                .methods
                .chainhead_v1_unpin(&remote_follow_id, hash_from_bytes(method, &hash)?)
                .await
                .map_err(|err| rpc_failure(method, err))?;
        }
        Ok(())
    }

    /// Continue a paused operation.
    #[instrument(skip_all, fields(runtime.method = "chain_runtime.continue"))]
    pub async fn remote_chain_head_continue(
        &self,
        request: RemoteChainHeadContinueRequest,
    ) -> Result<(), RuntimeFailure> {
        let method = "remote_chain_head_continue";
        let connection = self.connection_for(method, &request.genesis_hash).await?;
        let remote_follow_id = self
            .ensure_follow_context(method, &connection, request.follow_subscription_id, false)
            .await?;
        connection
            .methods
            .chainhead_v1_continue(&remote_follow_id, &request.operation_id)
            .await
            .map_err(|err| rpc_failure(method, err))
    }

    /// Stop a chain-head operation.
    #[instrument(skip_all, fields(runtime.method = "chain_runtime.stop_operation"))]
    pub async fn remote_chain_head_stop_operation(
        &self,
        request: RemoteChainHeadStopOperationRequest,
    ) -> Result<(), RuntimeFailure> {
        let method = "remote_chain_head_stop_operation";
        let connection = self.connection_for(method, &request.genesis_hash).await?;
        let remote_follow_id = self
            .ensure_follow_context(method, &connection, request.follow_subscription_id, false)
            .await?;
        connection
            .methods
            .chainhead_v1_stop_operation(&remote_follow_id, &request.operation_id)
            .await
            .map_err(|err| rpc_failure(method, err))
    }

    /// Echo back the chain genesis hash via chainSpec_v1_genesisHash.
    #[instrument(skip_all, fields(runtime.method = "chain_runtime.spec_genesis_hash"))]
    pub async fn remote_chain_spec_genesis_hash(
        &self,
        genesis_hash: Vec<u8>,
    ) -> Result<RemoteChainSpecGenesisHashResponse, RuntimeFailure> {
        let method = "remote_chain_spec_genesis_hash";
        let connection = self.connection_for(method, &genesis_hash).await?;
        let genesis_hash = connection
            .methods
            .chainspec_v1_genesis_hash()
            .await
            .map_err(|err| rpc_failure(method, err))
            .map(hash_to_bytes)?;
        Ok(RemoteChainSpecGenesisHashResponse { genesis_hash })
    }

    /// Fetch the chain display name via chainSpec_v1_chainName.
    #[instrument(skip_all, fields(runtime.method = "chain_runtime.spec_chain_name"))]
    pub async fn remote_chain_spec_chain_name(
        &self,
        genesis_hash: Vec<u8>,
    ) -> Result<RemoteChainSpecChainNameResponse, RuntimeFailure> {
        let method = "remote_chain_spec_chain_name";
        let connection = self.connection_for(method, &genesis_hash).await?;
        let chain_name = connection
            .methods
            .chainspec_v1_chain_name()
            .await
            .map_err(|err| rpc_failure(method, err))?;
        Ok(RemoteChainSpecChainNameResponse { chain_name })
    }

    /// Fetch the chain JSON properties via chainSpec_v1_properties.
    #[instrument(skip_all, fields(runtime.method = "chain_runtime.spec_properties"))]
    pub async fn remote_chain_spec_properties(
        &self,
        genesis_hash: Vec<u8>,
    ) -> Result<RemoteChainSpecPropertiesResponse, RuntimeFailure> {
        let method = "remote_chain_spec_properties";
        let connection = self.connection_for(method, &genesis_hash).await?;
        let value = connection
            .methods
            .chainspec_v1_properties::<Value>()
            .await
            .map_err(|err| rpc_failure(method, err))?;
        let properties = serde_json::to_string(&value)
            .map_err(|err| RuntimeFailure::host_failure(method, err.to_string()))?;
        Ok(RemoteChainSpecPropertiesResponse { properties })
    }

    /// Broadcast a signed transaction via transaction_v1_broadcast.
    #[instrument(skip_all, fields(runtime.method = "chain_runtime.transaction_broadcast"))]
    pub async fn remote_chain_transaction_broadcast(
        &self,
        request: RemoteChainTransactionBroadcastRequest,
    ) -> Result<RemoteChainTransactionBroadcastResponse, RuntimeFailure> {
        let method = "remote_chain_transaction_broadcast";
        let connection = self.connection_for(method, &request.genesis_hash).await?;
        let operation_id = connection
            .methods
            .transaction_v1_broadcast(&request.transaction)
            .await
            .map_err(|err| rpc_failure(method, err))?;
        Ok(RemoteChainTransactionBroadcastResponse { operation_id })
    }

    /// Stop a transaction broadcast via transaction_v1_stop.
    #[instrument(skip_all, fields(runtime.method = "chain_runtime.transaction_stop"))]
    pub async fn remote_chain_transaction_stop(
        &self,
        request: RemoteChainTransactionStopRequest,
    ) -> Result<(), RuntimeFailure> {
        let method = "remote_chain_transaction_stop";
        let connection = self.connection_for(method, &request.genesis_hash).await?;
        connection
            .methods
            .transaction_v1_stop(&request.operation_id)
            .await
            .map_err(|err| rpc_failure(method, err))
    }

    #[instrument(skip_all, fields(runtime.method = "chain_runtime.connection_for", method = method))]
    async fn connection_for(
        &self,
        method: &'static str,
        genesis_hash: &[u8],
    ) -> Result<Arc<ChainConnection>, RuntimeFailure> {
        let key = encode_hex(genesis_hash);
        let setup = {
            let mut connections = self.connections.lock().unwrap();
            match connections.get(&key) {
                Some(connection) if !connection.is_closed() => return Ok(connection.clone()),
                Some(_) => {
                    connections.remove(&key);
                }
                None => {}
            }
            // Single-flight the provider connect (same shape as
            // `follow_setups`): concurrent first connections for the same
            // chain share one in-flight `connect` instead of racing the
            // insert and orphaning the loser's connection.
            let mut setups = self.connection_setups.lock().unwrap();
            if let Some(existing) = setups.get(&key) {
                existing.clone()
            } else {
                let provider = self.provider.clone();
                let spawner = self.spawner.clone();
                let connections = self.connections.clone();
                let setups_map = self.connection_setups.clone();
                let setup_key = key.clone();
                let genesis_hash = genesis_hash.to_owned();
                let setup: ConnectionSetup = async move {
                    let result = provider.connect(genesis_hash).await.map(|rpc| {
                        let connection = ChainConnection::new(rpc, spawner);
                        connections
                            .lock()
                            .unwrap()
                            .insert(setup_key.clone(), connection.clone());
                        connection
                    });
                    setups_map.lock().unwrap().remove(&setup_key);
                    result
                }
                .boxed()
                .shared();
                setups.insert(key, setup.clone());
                setup
            }
        };

        setup.await.map_err(|failure| failure.reclassify(method))
    }

    #[instrument(skip_all, fields(runtime.method = "chain_runtime.start_follow"))]
    async fn start_follow(
        &self,
        local_follow_id: String,
        request: RemoteChainHeadFollowRequest,
        sender: Option<mpsc::UnboundedSender<FollowSignal>>,
    ) -> Result<(), RuntimeFailure> {
        let connection = self
            .connection_for(FOLLOW_METHOD, &request.genesis_hash)
            .await?;
        // Record this subscriber's sender before kicking off (or joining) the
        // single-flight setup so events route to it regardless of which caller
        // wins the setup.
        connection.register_follow_intent(&local_follow_id, request.with_runtime, sender);
        connection
            .ensure_remote_follow(local_follow_id, request.with_runtime)
            .await?;
        Ok(())
    }

    #[instrument(skip_all, fields(runtime.method = "chain_runtime.ensure_follow_context", method = method))]
    async fn ensure_follow_context(
        &self,
        method: &'static str,
        connection: &Arc<ChainConnection>,
        local_follow_id: String,
        with_runtime: bool,
    ) -> Result<String, RuntimeFailure> {
        let remote_follow_id = connection
            .require_remote_follow(method, local_follow_id.clone())
            .await?;
        if with_runtime && !connection.follow_with_runtime(&local_follow_id) {
            return Err(RuntimeFailure::host_failure(
                method,
                "follow subscription was created without runtime metadata",
            ));
        }
        Ok(remote_follow_id)
    }

    #[instrument(skip_all, fields(runtime.method = "chain_runtime.cleanup_follow"))]
    fn cleanup_follow(&self, genesis_hash: &[u8], local_follow_id: &str) {
        let key = encode_hex(genesis_hash);
        let Some(connection) = self.connections.lock().unwrap().get(&key).cloned() else {
            return;
        };
        connection.unfollow(local_follow_id);
    }
}

/// One delivery on the local follow stream. `Interrupt` signals an
/// abnormal close (connection dropped, follow setup failed); it produces no
/// item but ends the stream.
enum FollowSignal {
    Item(RemoteChainHeadFollowItem),
    Interrupt,
}

struct ChainConnection {
    rpc_client: HostRpcClient,
    methods: ChainHeadRpcMethods<TruapiRpcConfig>,
    spawner: Spawner,
    follows: Mutex<HashMap<String, FollowState>>,
    follow_setups: Mutex<HashMap<String, FollowSetup>>,
}

impl ChainConnection {
    fn new(rpc: Arc<dyn JsonRpcConnection>, spawner: Spawner) -> Arc<Self> {
        let rpc_client = HostRpcClient::new(rpc, spawner.clone());
        let methods = ChainHeadRpcMethods::new(RpcClient::new(rpc_client.clone()));
        Arc::new(Self {
            rpc_client,
            methods,
            spawner,
            follows: Mutex::new(HashMap::new()),
            follow_setups: Mutex::new(HashMap::new()),
        })
    }

    fn is_closed(&self) -> bool {
        self.rpc_client.is_closed()
    }

    fn follow_with_runtime(&self, local_follow_id: &str) -> bool {
        self.follows
            .lock()
            .unwrap()
            .get(local_follow_id)
            .is_some_and(|follow| follow.with_runtime)
    }

    fn remote_follow_id(&self, local_follow_id: &str) -> Option<String> {
        self.follows
            .lock()
            .unwrap()
            .get(local_follow_id)
            .and_then(|follow| follow.remote_subscription_id.clone())
    }

    /// Record intent to follow `local_follow_id`, attaching `sender` for a
    /// follow subscriber. Idempotent: an existing follow keeps its
    /// `with_runtime` flag and remote id; only the sender is (re)attached.
    fn register_follow_intent(
        &self,
        local_follow_id: &str,
        with_runtime: bool,
        sender: Option<mpsc::UnboundedSender<FollowSignal>>,
    ) {
        let mut follows = self.follows.lock().unwrap();
        match follows.get_mut(local_follow_id) {
            Some(follow) => {
                if sender.is_some() {
                    follow.sender = sender;
                }
            }
            None => {
                follows.insert(
                    local_follow_id.to_string(),
                    FollowState {
                        with_runtime,
                        remote_subscription_id: None,
                        abort: None,
                        sender,
                    },
                );
            }
        }
    }

    /// Issue `chainHead_v1_follow` exactly once per local follow id and return
    /// the remote subscription id. Concurrent callers for the same id share
    /// one in-flight setup instead of each opening a duplicate remote
    /// subscription that would then leak.
    #[instrument(skip_all, fields(runtime.method = "chain_connection.ensure_remote_follow"))]
    async fn ensure_remote_follow(
        self: &Arc<Self>,
        local_follow_id: String,
        with_runtime: bool,
    ) -> Result<String, RuntimeFailure> {
        if let Some(remote_follow_id) = self.remote_follow_id(&local_follow_id) {
            return Ok(remote_follow_id);
        }

        let setup = {
            let mut setups = self.follow_setups.lock().unwrap();
            if let Some(existing) = setups.get(&local_follow_id) {
                existing.clone()
            } else {
                let connection = self.clone();
                let id = local_follow_id.clone();
                let setup: FollowSetup =
                    async move { connection.run_follow_setup(id, with_runtime).await }
                        .boxed()
                        .shared();
                setups.insert(local_follow_id.clone(), setup.clone());
                setup
            }
        };

        let result = setup.await;
        // On failure, drop the cached setup so a later re-subscribe can retry.
        // On success the established follow short-circuits the fast path above,
        // and `remove_follow` clears the entry at teardown.
        if result.is_err() {
            self.follow_setups.lock().unwrap().remove(&local_follow_id);
        }
        result
    }

    /// Return the remote follow id for an already-created local follow.
    ///
    /// Follow-bound request methods must not create remote follows themselves:
    /// the local follow stream owns cleanup, so only `follow_head_subscribe`
    /// may establish the remote subscription.
    #[instrument(skip_all, fields(runtime.method = "chain_connection.require_remote_follow"))]
    async fn require_remote_follow(
        self: &Arc<Self>,
        method: &'static str,
        local_follow_id: String,
    ) -> Result<String, RuntimeFailure> {
        if let Some(remote_follow_id) = self.remote_follow_id(&local_follow_id) {
            return Ok(remote_follow_id);
        }

        let setup = {
            let follows = self.follows.lock().unwrap();
            if !follows.contains_key(&local_follow_id) {
                return Err(RuntimeFailure::host_failure(
                    method,
                    format!("unknown follow subscription id {local_follow_id:?}"),
                ));
            }
            self.follow_setups
                .lock()
                .unwrap()
                .get(&local_follow_id)
                .cloned()
        };

        match setup {
            Some(setup) => setup.await.map_err(|failure| failure.reclassify(method)),
            None => Err(RuntimeFailure::host_failure(
                method,
                format!("follow subscription {local_follow_id:?} is not established"),
            )),
        }
    }

    /// Body of the single-flight follow setup: ensure the `FollowState`
    /// exists, issue `chainHead_v1_follow`, and record the remote id.
    #[instrument(skip_all, fields(runtime.method = "chain_connection.run_follow_setup"))]
    async fn run_follow_setup(
        self: Arc<Self>,
        local_follow_id: String,
        with_runtime: bool,
    ) -> Result<String, RuntimeFailure> {
        self.follows
            .lock()
            .unwrap()
            .entry(local_follow_id.clone())
            .or_insert_with(|| FollowState {
                with_runtime,
                remote_subscription_id: None,
                abort: None,
                sender: None,
            });

        let mut follow = self
            .methods
            .chainhead_v1_follow(with_runtime)
            .await
            .map_err(|err| {
                self.remove_follow(&local_follow_id);
                rpc_failure(FOLLOW_METHOD, err)
            })?;
        let remote_follow_id = follow
            .subscription_id()
            .ok_or_else(|| {
                RuntimeFailure::host_failure(FOLLOW_METHOD, "missing follow subscription id")
            })?
            .to_string();

        let (abort, abort_registration) = AbortHandle::new_pair();
        let connection = self.clone();
        let pump_follow_id = local_follow_id.clone();
        let pump = async move {
            while let Some(item) = follow.next().await {
                match item {
                    Ok(event) => match map_follow_event(event) {
                        Ok(item) => {
                            let is_stop = matches!(item, RemoteChainHeadFollowItem::Stop);
                            connection.deliver_follow_event(&pump_follow_id, item, false);
                            if is_stop {
                                break;
                            }
                        }
                        Err(_) => {
                            connection.interrupt_follow(&pump_follow_id, false);
                            break;
                        }
                    },
                    Err(_) => {
                        connection.interrupt_follow(&pump_follow_id, false);
                        break;
                    }
                }
            }
            connection.remove_follow_without_abort(&pump_follow_id);
        };

        if !self.attach_remote_follow(&local_follow_id, remote_follow_id.clone(), abort) {
            return Err(RuntimeFailure::unavailable(FOLLOW_METHOD));
        }

        (self.spawner)(Abortable::new(pump, abort_registration).map(|_| ()).boxed());
        Ok(remote_follow_id)
    }

    fn attach_remote_follow(
        &self,
        local_follow_id: &str,
        remote_follow_id: String,
        abort: AbortHandle,
    ) -> bool {
        let mut follows = self.follows.lock().unwrap();
        let Some(follow) = follows.get_mut(local_follow_id) else {
            return false;
        };
        follow.remote_subscription_id = Some(remote_follow_id);
        follow.abort = Some(abort);
        true
    }

    fn remove_follow(&self, local_follow_id: &str) {
        self.follow_setups.lock().unwrap().remove(local_follow_id);
        if let Some(mut follow) = self.follows.lock().unwrap().remove(local_follow_id)
            && let Some(abort) = follow.abort.take()
        {
            abort.abort();
        }
    }

    fn remove_follow_without_abort(&self, local_follow_id: &str) {
        self.follow_setups.lock().unwrap().remove(local_follow_id);
        self.follows.lock().unwrap().remove(local_follow_id);
    }

    fn unfollow(&self, local_follow_id: &str) {
        self.remove_follow(local_follow_id);
    }

    fn deliver_follow_event(
        &self,
        local_follow_id: &str,
        event: RemoteChainHeadFollowItem,
        abort_on_stop: bool,
    ) {
        let sender = self
            .follows
            .lock()
            .unwrap()
            .get(local_follow_id)
            .and_then(|follow| follow.sender.clone());
        let is_stop = matches!(event, RemoteChainHeadFollowItem::Stop);
        if let Some(sender) = sender {
            let _ = sender.unbounded_send(FollowSignal::Item(event));
        }
        if is_stop {
            if abort_on_stop {
                self.remove_follow(local_follow_id);
            } else {
                self.remove_follow_without_abort(local_follow_id);
            }
        }
    }

    fn interrupt_follow(&self, local_follow_id: &str, abort: bool) {
        let sender = self
            .follows
            .lock()
            .unwrap()
            .get(local_follow_id)
            .and_then(|follow| follow.sender.clone());
        if let Some(sender) = sender {
            let _ = sender.unbounded_send(FollowSignal::Interrupt);
        }
        if abort {
            self.remove_follow(local_follow_id);
        } else {
            self.remove_follow_without_abort(local_follow_id);
        }
    }
}

struct FollowState {
    with_runtime: bool,
    remote_subscription_id: Option<String>,
    abort: Option<AbortHandle>,
    sender: Option<mpsc::UnboundedSender<FollowSignal>>,
}

/// Subscription wrapper that runs an `on_drop` cleanup when the stream is
/// dropped. Used by `remote_chain_head_follow` to send `chainHead_v1_unfollow`
/// when the local follow stream is dropped.
struct ManagedSubscription<T> {
    inner: BoxStream<'static, T>,
    on_drop: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl<T> ManagedSubscription<T> {
    fn new(inner: BoxStream<'static, T>, on_drop: Option<Box<dyn FnOnce() + Send + Sync>>) -> Self {
        Self { inner, on_drop }
    }
}

impl<T> Drop for ManagedSubscription<T> {
    fn drop(&mut self) {
        if let Some(on_drop) = self.on_drop.take() {
            on_drop();
        }
    }
}

impl<T> Stream for ManagedSubscription<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        this.inner.as_mut().poll_next(cx)
    }
}

fn operation_started_result(
    response: subxt_chain::MethodResponse,
) -> Result<OperationStartedResult, RuntimeFailure> {
    match response {
        subxt_chain::MethodResponse::Started(started) => Ok(OperationStartedResult::Started {
            operation_id: started.operation_id,
        }),
        subxt_chain::MethodResponse::LimitReached => Ok(OperationStartedResult::LimitReached),
    }
}

fn map_follow_event(
    event: subxt_chain::FollowEvent<H256>,
) -> Result<RemoteChainHeadFollowItem, RuntimeFailure> {
    match event {
        subxt_chain::FollowEvent::Initialized(event) => {
            Ok(RemoteChainHeadFollowItem::Initialized {
                finalized_block_hashes: event
                    .finalized_block_hashes
                    .into_iter()
                    .map(hash_to_bytes)
                    .collect(),
                finalized_block_runtime: event
                    .finalized_block_runtime
                    .map(map_runtime_event)
                    .transpose()?,
            })
        }
        subxt_chain::FollowEvent::NewBlock(event) => Ok(RemoteChainHeadFollowItem::NewBlock {
            block_hash: hash_to_bytes(event.block_hash),
            parent_block_hash: hash_to_bytes(event.parent_block_hash),
            new_runtime: event.new_runtime.map(map_runtime_event).transpose()?,
        }),
        subxt_chain::FollowEvent::BestBlockChanged(event) => {
            Ok(RemoteChainHeadFollowItem::BestBlockChanged {
                best_block_hash: hash_to_bytes(event.best_block_hash),
            })
        }
        subxt_chain::FollowEvent::Finalized(event) => Ok(RemoteChainHeadFollowItem::Finalized {
            finalized_block_hashes: event
                .finalized_block_hashes
                .into_iter()
                .map(hash_to_bytes)
                .collect(),
            pruned_block_hashes: event
                .pruned_block_hashes
                .into_iter()
                .map(hash_to_bytes)
                .collect(),
        }),
        subxt_chain::FollowEvent::OperationBodyDone(event) => {
            Ok(RemoteChainHeadFollowItem::OperationBodyDone {
                operation_id: event.operation_id,
                value: event.value.into_iter().map(|bytes| bytes.0).collect(),
            })
        }
        subxt_chain::FollowEvent::OperationCallDone(event) => {
            Ok(RemoteChainHeadFollowItem::OperationCallDone {
                operation_id: event.operation_id,
                output: event.output.0,
            })
        }
        subxt_chain::FollowEvent::OperationStorageItems(event) => {
            Ok(RemoteChainHeadFollowItem::OperationStorageItems {
                operation_id: event.operation_id,
                items: event
                    .items
                    .into_iter()
                    .map(map_storage_result)
                    .collect::<Result<Vec<_>, _>>()?,
            })
        }
        subxt_chain::FollowEvent::OperationStorageDone(event) => {
            Ok(RemoteChainHeadFollowItem::OperationStorageDone {
                operation_id: event.operation_id,
            })
        }
        subxt_chain::FollowEvent::OperationWaitingForContinue(event) => {
            Ok(RemoteChainHeadFollowItem::OperationWaitingForContinue {
                operation_id: event.operation_id,
            })
        }
        subxt_chain::FollowEvent::OperationInaccessible(event) => {
            Ok(RemoteChainHeadFollowItem::OperationInaccessible {
                operation_id: event.operation_id,
            })
        }
        subxt_chain::FollowEvent::OperationError(event) => {
            Ok(RemoteChainHeadFollowItem::OperationError {
                operation_id: event.operation_id,
                error: event.error,
            })
        }
        subxt_chain::FollowEvent::Stop => Ok(RemoteChainHeadFollowItem::Stop),
    }
}

fn map_runtime_event(event: subxt_chain::RuntimeEvent) -> Result<RuntimeType, RuntimeFailure> {
    match event {
        subxt_chain::RuntimeEvent::Valid(event) => {
            let mut apis = event
                .spec
                .apis
                .into_iter()
                .map(|(name, version)| RuntimeApi { name, version })
                .collect::<Vec<_>>();
            apis.sort_by(|left, right| left.name.cmp(&right.name));
            Ok(RuntimeType::Valid(RuntimeSpec {
                spec_name: event.spec.spec_name,
                impl_name: event.spec.impl_name,
                spec_version: event.spec.spec_version,
                impl_version: event.spec.impl_version,
                transaction_version: Some(event.spec.transaction_version),
                apis,
            }))
        }
        subxt_chain::RuntimeEvent::Invalid(event) => {
            Ok(RuntimeType::Invalid { error: event.error })
        }
    }
}

fn map_storage_query_item(item: &StorageQueryItem) -> subxt_chain::StorageQuery<&[u8]> {
    subxt_chain::StorageQuery {
        key: item.key.as_slice(),
        query_type: match item.query_type {
            StorageQueryType::Value => subxt_chain::StorageQueryType::Value,
            StorageQueryType::Hash => subxt_chain::StorageQueryType::Hash,
            StorageQueryType::ClosestDescendantMerkleValue => {
                subxt_chain::StorageQueryType::ClosestDescendantMerkleValue
            }
            StorageQueryType::DescendantsValues => subxt_chain::StorageQueryType::DescendantsValues,
            StorageQueryType::DescendantsHashes => subxt_chain::StorageQueryType::DescendantsHashes,
        },
    }
}

fn map_storage_result(
    item: subxt_chain::StorageResult,
) -> Result<StorageResultItem, RuntimeFailure> {
    let mut result = StorageResultItem {
        key: item.key.0,
        value: None,
        hash: None,
        closest_descendant_merkle_value: None,
    };
    match item.result {
        subxt_chain::StorageResultType::Value(value) => result.value = Some(value.0),
        subxt_chain::StorageResultType::Hash(hash) => result.hash = Some(hash.0),
        subxt_chain::StorageResultType::ClosestDescendantMerkleValue(value) => {
            result.closest_descendant_merkle_value = Some(value.0);
        }
    }
    Ok(result)
}

fn hash_from_bytes(method: &'static str, bytes: &[u8]) -> Result<H256, RuntimeFailure> {
    if bytes.len() != 32 {
        return Err(RuntimeFailure::host_failure(
            method,
            format!("expected 32-byte hash, got {}", bytes.len()),
        ));
    }
    Ok(H256::from_slice(bytes))
}

fn hash_to_bytes(hash: H256) -> Vec<u8> {
    hash.as_bytes().to_vec()
}

fn rpc_failure(method: &'static str, error: SubxtRpcError) -> RuntimeFailure {
    match error {
        SubxtRpcError::Client(_) | SubxtRpcError::DisconnectedWillReconnect(_) => {
            RuntimeFailure::unavailable(method)
        }
        error => RuntimeFailure::host_failure(method, error.to_string()),
    }
}

/// Encode a byte slice as a `0x`-prefixed lowercase hex string.
pub(crate) fn encode_hex(value: &[u8]) -> String {
    format!("0x{}", hex::encode(value))
}

#[cfg(test)]
fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    hex::decode(value.strip_prefix("0x").unwrap_or(value)).map_err(|_| "invalid hex".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::channel::mpsc as fut_mpsc;
    use futures::stream::BoxStream;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn spawner_for_tests() -> Spawner {
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::subscription::thread_per_subscription_spawner()
        }
        #[cfg(target_arch = "wasm32")]
        {
            Arc::new(futures::executor::block_on)
        }
    }

    #[derive(Default)]
    struct UnavailableChainProvider;

    #[async_trait]
    impl RuntimeChainProvider for UnavailableChainProvider {
        async fn connect(
            &self,
            _genesis_hash: Vec<u8>,
        ) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
            Err(RuntimeFailure::unavailable("remote_chain_connect"))
        }
    }

    /// Provider that echoes a canned response for every request it sees,
    /// driven by a `respond` closure. The closure receives each json-rpc
    /// request string and returns the response string the test wants the
    /// server to deliver. Keeps the response loop synchronized with the
    /// request stream so there is no race between `send` and the response
    /// loop draining frames before pending requests have registered.
    type Responder = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

    struct ScriptedProvider {
        respond: Responder,
        sent: Arc<Mutex<Vec<String>>>,
        sender: Arc<Mutex<Option<fut_mpsc::UnboundedSender<String>>>>,
        receiver: Arc<Mutex<Option<fut_mpsc::UnboundedReceiver<String>>>>,
        connect_calls: Arc<AtomicUsize>,
    }

    impl ScriptedProvider {
        fn new<F>(respond: F) -> Self
        where
            F: Fn(&str) -> Option<String> + Send + Sync + 'static,
        {
            let (tx, rx) = fut_mpsc::unbounded();
            Self {
                respond: Arc::new(respond),
                sent: Arc::new(Mutex::new(Vec::new())),
                sender: Arc::new(Mutex::new(Some(tx))),
                receiver: Arc::new(Mutex::new(Some(rx))),
                connect_calls: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    struct ScriptedConnection {
        respond: Responder,
        sent: Arc<Mutex<Vec<String>>>,
        sender: Arc<Mutex<Option<fut_mpsc::UnboundedSender<String>>>>,
        receiver: Mutex<Option<fut_mpsc::UnboundedReceiver<String>>>,
    }

    impl JsonRpcConnection for ScriptedConnection {
        fn send(&self, request: String) {
            self.sent.lock().unwrap().push(request.clone());
            if let Some(response) = (self.respond)(&request)
                && let Some(sender) = self.sender.lock().unwrap().as_ref()
            {
                let _ = sender.unbounded_send(response);
            }
        }
        fn responses(&self) -> BoxStream<'static, String> {
            let rx = self
                .receiver
                .lock()
                .unwrap()
                .take()
                .expect("ScriptedConnection::responses called twice");
            rx.boxed()
        }

        fn close(&self) {
            self.sender.lock().unwrap().take();
        }
    }

    #[async_trait]
    impl RuntimeChainProvider for ScriptedProvider {
        async fn connect(
            &self,
            _genesis_hash: Vec<u8>,
        ) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
            self.connect_calls.fetch_add(1, Ordering::SeqCst);
            let receiver = self.receiver.lock().unwrap().take();
            Ok(Arc::new(ScriptedConnection {
                respond: self.respond.clone(),
                sent: self.sent.clone(),
                sender: self.sender.clone(),
                receiver: Mutex::new(receiver),
            }))
        }
    }

    /// Clone of the scripted notification sender, used by tests to push
    /// asynchronous frames (e.g. follow events) into the response stream.
    fn notification_sender(provider: &ScriptedProvider) -> fut_mpsc::UnboundedSender<String> {
        provider
            .sender
            .lock()
            .unwrap()
            .as_ref()
            .expect("notification sender available")
            .clone()
    }

    #[test]
    fn unavailable_provider_surfaces_failure() {
        let provider = Arc::new(UnavailableChainProvider);
        let result = futures::executor::block_on(provider.connect(vec![0u8; 32]));
        let err = match result {
            Ok(_) => panic!("expected failure"),
            Err(err) => err,
        };
        assert_eq!(err.kind(), RuntimeFailureKind::Unavailable);
        assert_eq!(err.method(), "remote_chain_connect");
    }

    /// Find the json-rpc request id of the just-sent frame so the scripted
    /// responder can mirror it back to the dispatcher.
    fn extract_id(request: &str) -> Option<String> {
        let value: Value = serde_json::from_str(request).ok()?;
        value.get("id")?.as_str().map(ToString::to_string)
    }

    fn wait_for_sent(
        provider: &ScriptedProvider,
        predicate: impl Fn(&[String]) -> bool,
    ) -> Vec<String> {
        for _ in 0..500 {
            let sent = provider.sent.lock().unwrap().clone();
            if predicate(&sent) {
                return sent;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        provider.sent.lock().unwrap().clone()
    }

    #[test]
    fn header_request_reuses_existing_follow() {
        let provider = Arc::new(ScriptedProvider::new(|request| {
            let id = extract_id(request).unwrap();
            if request.contains("chainHead_v1_follow") {
                Some(format!(
                    r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW"}}"#
                ))
            } else if request.contains("chainHead_v1_header") {
                Some(format!(
                    r#"{{"jsonrpc":"2.0","id":"{id}","result":"0xdeadbeef"}}"#
                ))
            } else {
                None
            }
        }));
        let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());
        let _follow_stream = runtime.remote_chain_head_follow(
            "local-follow".to_string(),
            RemoteChainHeadFollowRequest {
                genesis_hash: vec![0u8; 32],
                with_runtime: false,
            },
        );
        let sent = wait_for_sent(&provider, |sent| {
            sent.iter()
                .any(|request| request.contains("chainHead_v1_follow"))
        });
        assert!(
            sent.iter()
                .any(|request| request.contains("chainHead_v1_follow")),
            "follow setup did not start; sent: {sent:?}",
        );

        let response = futures::executor::block_on(runtime.remote_chain_head_header(
            RemoteChainHeadHeaderRequest {
                genesis_hash: vec![0u8; 32],
                follow_subscription_id: "local-follow".to_string(),
                hash: vec![1u8; 32],
            },
        ))
        .expect("ok response");
        assert_eq!(response.header, Some(vec![0xde, 0xad, 0xbe, 0xef]));
        assert_eq!(provider.connect_calls.load(Ordering::SeqCst), 1);
        let sent = provider.sent.lock().unwrap().clone();
        assert_eq!(sent.len(), 2);
        assert!(sent[0].contains("chainHead_v1_follow"));
        assert!(sent[1].contains("chainHead_v1_header"));
    }

    #[test]
    fn header_request_rejects_unknown_follow_id_without_opening_follow() {
        let provider = Arc::new(ScriptedProvider::new(|request| {
            let id = extract_id(request).unwrap();
            if request.contains("chainHead_v1_follow") {
                Some(format!(
                    r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW"}}"#
                ))
            } else if request.contains("chainHead_v1_header") {
                Some(format!(
                    r#"{{"jsonrpc":"2.0","id":"{id}","result":"0xdeadbeef"}}"#
                ))
            } else {
                None
            }
        }));
        let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());

        let err = futures::executor::block_on(runtime.remote_chain_head_header(
            RemoteChainHeadHeaderRequest {
                genesis_hash: vec![0u8; 32],
                follow_subscription_id: "missing-follow".to_string(),
                hash: vec![1u8; 32],
            },
        ))
        .expect_err("unknown follow id should fail");

        assert_eq!(err.kind(), RuntimeFailureKind::HostFailure);
        assert!(
            err.reason().contains("unknown follow subscription id"),
            "unexpected error: {}",
            err.reason(),
        );
        assert!(provider.sent.lock().unwrap().is_empty());
    }

    /// Two concurrent calls for the same chain must share one provider
    /// `connect` instead of racing the first connection and orphaning the
    /// loser.
    #[test]
    fn concurrent_connection_for_shares_one_connect() {
        struct SlowConnectProvider {
            inner: ScriptedProvider,
        }

        #[async_trait]
        impl RuntimeChainProvider for SlowConnectProvider {
            async fn connect(
                &self,
                genesis_hash: Vec<u8>,
            ) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
                futures_timer::Delay::new(std::time::Duration::from_millis(50)).await;
                self.inner.connect(genesis_hash).await
            }
        }

        let provider = Arc::new(SlowConnectProvider {
            inner: ScriptedProvider::new(|request| {
                let id = extract_id(request).unwrap();
                if request.contains("chainSpec_v1_chainName") {
                    Some(format!(
                        r#"{{"jsonrpc":"2.0","id":"{id}","result":"Polkadot"}}"#
                    ))
                } else {
                    None
                }
            }),
        });
        let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());

        let (first, second) = futures::executor::block_on(futures::future::join(
            runtime.remote_chain_spec_chain_name(vec![0u8; 32]),
            runtime.remote_chain_spec_chain_name(vec![0u8; 32]),
        ));

        assert_eq!(first.unwrap().chain_name, "Polkadot");
        assert_eq!(second.unwrap().chain_name, "Polkadot");
        assert_eq!(provider.inner.connect_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn unknown_genesis_chain_spec_propagates_failure() {
        let provider = Arc::new(UnavailableChainProvider);
        let runtime = ChainRuntime::new(provider, spawner_for_tests());
        let err = match futures::executor::block_on(
            runtime.remote_chain_spec_chain_name(vec![0u8; 32]),
        ) {
            Ok(_) => panic!("expected failure"),
            Err(err) => err,
        };
        assert_eq!(err.kind(), RuntimeFailureKind::Unavailable);
        assert_eq!(err.method(), "remote_chain_spec_chain_name");
    }

    #[test]
    fn json_rpc_error_becomes_host_failure() {
        let provider = Arc::new(ScriptedProvider::new(|request| {
            let id = extract_id(request).unwrap();
            Some(format!(
                r#"{{"jsonrpc":"2.0","id":"{id}","error":{{"code":-32601,"message":"method not found"}}}}"#
            ))
        }));
        let runtime = ChainRuntime::new(provider, spawner_for_tests());
        let err = match futures::executor::block_on(
            runtime.remote_chain_spec_chain_name(vec![0u8; 32]),
        ) {
            Ok(_) => panic!("expected failure"),
            Err(err) => err,
        };
        assert_eq!(err.kind(), RuntimeFailureKind::HostFailure);
        assert!(
            err.reason().contains("method not found"),
            "unexpected reason: {}",
            err.reason()
        );
    }

    #[test]
    fn follow_event_initialized_translates_to_v01_item() {
        // Answer `chainHead_v1_follow` through the synchronized responder so
        // the ack cannot reach the response loop before the pending request
        // is registered.
        let provider = Arc::new(ScriptedProvider::new(|request| {
            let id = extract_id(request).unwrap();
            if request.contains("chainHead_v1_follow") {
                Some(format!(
                    r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW"}}"#
                ))
            } else {
                None
            }
        }));
        let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());

        let mut stream = runtime.remote_chain_head_follow(
            "local-follow".to_string(),
            RemoteChainHeadFollowRequest {
                genesis_hash: vec![0u8; 32],
                with_runtime: false,
            },
        );

        // Push follow events keyed by remote subscription id. Events that
        // land before the follow ack are buffered by remote id and replayed
        // once the follow is established.
        let tx = notification_sender(&provider);
        tx.unbounded_send(
            r#"{"jsonrpc":"2.0","method":"chainHead_v1_followEvent","params":{"subscription":"REMOTE-FOLLOW","result":{"event":"initialized","finalizedBlockHashes":["0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]}}}"#
                .to_string(),
        ).unwrap();
        tx.unbounded_send(
            r#"{"jsonrpc":"2.0","method":"chainHead_v1_followEvent","params":{"subscription":"REMOTE-FOLLOW","result":{"event":"stop"}}}"#
                .to_string(),
        ).unwrap();

        let items: Vec<_> = futures::executor::block_on(async {
            let mut out = Vec::new();
            while let Some(item) = stream.next().await {
                let is_stop = matches!(item, RemoteChainHeadFollowItem::Stop);
                out.push(item);
                if is_stop {
                    break;
                }
            }
            out
        });

        match &items[0] {
            RemoteChainHeadFollowItem::Initialized {
                finalized_block_hashes,
                finalized_block_runtime,
            } => {
                assert_eq!(finalized_block_hashes, &vec![vec![0xaa; 32]]);
                assert!(finalized_block_runtime.is_none());
            }
            other => panic!("expected Initialized, got {other:?}"),
        }
        assert!(matches!(items[1], RemoteChainHeadFollowItem::Stop));
    }

    #[cfg_attr(target_arch = "wasm32", ignore)]
    #[test]
    fn drop_follow_stream_sends_unfollow() {
        let provider = Arc::new(ScriptedProvider::new(|request| {
            let id = extract_id(request).unwrap();
            if request.contains("chainHead_v1_follow") {
                Some(format!(
                    r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW"}}"#
                ))
            } else {
                None
            }
        }));
        let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());
        let sent = provider.sent.clone();

        let stream = runtime.remote_chain_head_follow(
            "local-follow".to_string(),
            RemoteChainHeadFollowRequest {
                genesis_hash: vec![0u8; 32],
                with_runtime: false,
            },
        );

        // Wait until the follow setup roundtrips and lands in `sent`.
        // Generous timeout so the test stays robust under loaded CI runners
        // where the spawner can be slow to schedule the request task.
        for _ in 0..500 {
            if !sent.lock().unwrap().is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        drop(stream);

        // Wait for the cleanup task to run and emit the unfollow request.
        for _ in 0..500 {
            if sent.lock().unwrap().len() >= 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let messages = sent.lock().unwrap().clone();
        assert!(
            messages.iter().any(|m| m.contains("chainHead_v1_unfollow")),
            "unfollow not sent; messages: {messages:?}",
        );
    }

    #[test]
    fn encode_hex_round_trip() {
        let bytes = vec![0x00u8, 0x12, 0xab, 0xff];
        let s = encode_hex(&bytes);
        assert_eq!(s, "0x0012abff");
        assert_eq!(decode_hex(&s).unwrap(), bytes);
    }

    #[test]
    fn parse_runtime_type_valid_sorts_apis() {
        let runtime_type = map_runtime_event(subxt_chain::RuntimeEvent::Valid(
            subxt_chain::RuntimeVersionEvent {
                spec: subxt_chain::RuntimeSpec {
                    spec_name: "polkadot".to_string(),
                    impl_name: "parity-polkadot".to_string(),
                    spec_version: 1000,
                    impl_version: 1,
                    transaction_version: 24,
                    apis: HashMap::from([("0xbeef".to_string(), 2), ("0xbabe".to_string(), 4)]),
                },
            },
        ))
        .unwrap();
        match runtime_type {
            RuntimeType::Valid(spec) => {
                assert_eq!(spec.apis.len(), 2);
                assert_eq!(spec.apis[0].name, "0xbabe");
                assert_eq!(spec.apis[1].name, "0xbeef");
                assert_eq!(spec.transaction_version, Some(24));
            }
            other => panic!("expected Valid, got {other:?}"),
        }
    }
}
