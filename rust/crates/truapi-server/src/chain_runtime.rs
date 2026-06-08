//! ChainHead v1 state machine used by `PlatformRuntimeHost`.
//!
//! [`ChainRuntime`] keeps one [`ChainConnection`] per chain (keyed by genesis
//! hash) on top of the platform-provided [`JsonRpcConnection`]. Each connection
//! owns the per-product `chainHead_v1_follow` subscriptions, the in-flight
//! request map, and the json-rpc response loop. The follow event stream is
//! parsed into v01 [`RemoteChainHeadFollowItem`] values; one-shot calls
//! (header / body / call / storage / spec / broadcast / stop) are submitted as
//! json-rpc requests and the matching response is decoded back into a typed
//! v01 result.
//!
//! The chain-side traits return [`RuntimeFailure`], a local classification
//! that the [`crate::runtime`] layer maps to [`truapi::CallError`] variants
//! (`Unsupported`, `HostFailure`, ...). This avoids leaking json-rpc plumbing
//! into the public API.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::task::{Context, Poll};

use futures::FutureExt;
use futures::channel::{mpsc, oneshot};
use futures::future::{BoxFuture, Shared};
use futures::stream::BoxStream;
use futures::{Stream, StreamExt};
use serde_json::{Map, Value, json};
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

use crate::subscription::Spawner;

const FOLLOW_METHOD: &str = "remote_chain_head_follow";

/// Cap on the number of distinct remote subscription ids buffered in
/// `pending_follow_events`, and on the events held per id, so a misbehaving
/// node (or late events for a retired remote id) cannot grow memory without
/// bound while a follow id is being established.
const MAX_PENDING_FOLLOW_EVENT_IDS: usize = 64;
const MAX_PENDING_FOLLOW_EVENTS_PER_ID: usize = 256;

/// Shared, single-flight `chainHead_v1_follow` setup keyed by local follow id.
/// Concurrent callers for the same id await one in-flight request rather than
/// each opening (and leaking) a separate remote subscription.
type FollowSetup = Shared<BoxFuture<'static, Result<String, RuntimeFailure>>>;

/// Classification of framework-level chain failures separate from JSON-RPC
/// domain errors. Maps cleanly to [`truapi::CallError`] variants at the
/// `PlatformRuntimeHost` boundary.
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
    pub fn method(&self) -> &'static str {
        self.method
    }

    /// Diagnostic reason. Always non-empty for `HostFailure`.
    pub fn reason(&self) -> String {
        match &self.reason {
            Some(reason) => format!("{}: {}", self.method, reason),
            None => self.method.to_string(),
        }
    }
}

/// Provider of `JsonRpcConnection` instances keyed by chain genesis hash.
/// The default [`UnavailableChainProvider`] makes every call fail; real
/// hosts plug in either the platform-side `ChainProvider` or the bundled
/// smoldot provider (feature `smoldot`).
#[async_trait::async_trait]
pub trait RuntimeChainProvider: Send + Sync {
    /// Open or reuse a JSON-RPC connection for the chain identified by
    /// `genesis_hash`.
    async fn connect(
        &self,
        genesis_hash: Vec<u8>,
    ) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure>;
}

/// Default provider: every `connect` call fails with `Unavailable`, so each
/// chain RPC surfaces a typed "unavailable" error to the product.
#[derive(Default)]
pub struct UnavailableChainProvider;

#[async_trait::async_trait]
impl RuntimeChainProvider for UnavailableChainProvider {
    async fn connect(
        &self,
        _genesis_hash: Vec<u8>,
    ) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
        Err(RuntimeFailure::unavailable("remote_chain_connect"))
    }
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
}

impl ChainRuntime {
    /// Build a `ChainRuntime` driven by `provider`. Background tasks (response
    /// pumps, follow setup) are spawned on `spawner`.
    pub fn new(provider: Arc<dyn RuntimeChainProvider>, spawner: Spawner) -> Self {
        Self {
            provider,
            spawner,
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start (or attach to an existing) `chainHead_v1_follow` subscription.
    /// Returns a stream of typed follow items that closes when the remote
    /// sends `stop` or the connection drops.
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

        let stream = ManagedSubscription::new(
            rx.boxed(),
            Some(Box::new(move || {
                cleanup_runtime.cleanup_follow(&cleanup_genesis_hash, &cleanup_follow_id);
            })),
        );
        stream
            .filter_map(|signal| async move {
                match signal {
                    FollowSignal::Item(item) => Some(item),
                    FollowSignal::Interrupt => None,
                }
            })
            .boxed()
    }

    /// Fetch a block header.
    pub async fn remote_chain_head_header(
        &self,
        request: RemoteChainHeadHeaderRequest,
    ) -> Result<RemoteChainHeadHeaderResponse, RuntimeFailure> {
        let method = "remote_chain_head_header";
        let connection = self.connection_for(method, &request.genesis_hash).await?;
        let remote_follow_id = self
            .ensure_follow_context(method, &connection, request.follow_subscription_id, false)
            .await?;

        let value = connection
            .request_value(
                method,
                "chainHead_v1_header",
                json!([remote_follow_id, encode_hex(&request.hash)]),
            )
            .await?;
        let header = match value {
            Value::Null => None,
            Value::String(encoded) => Some(
                decode_hex(&encoded)
                    .map_err(|reason| RuntimeFailure::host_failure(method, reason))?,
            ),
            _ => {
                return Err(RuntimeFailure::host_failure(
                    method,
                    "unexpected chainHead_v1_header result",
                ));
            }
        };
        Ok(RemoteChainHeadHeaderResponse { header })
    }

    /// Start a chainHead_v1_body operation.
    pub async fn remote_chain_head_body(
        &self,
        request: RemoteChainHeadBodyRequest,
    ) -> Result<RemoteChainHeadBodyResponse, RuntimeFailure> {
        let method = "remote_chain_head_body";
        let connection = self.connection_for(method, &request.genesis_hash).await?;
        let remote_follow_id = self
            .ensure_follow_context(method, &connection, request.follow_subscription_id, false)
            .await?;

        let value = connection
            .request_value(
                method,
                "chainHead_v1_body",
                json!([remote_follow_id, encode_hex(&request.hash)]),
            )
            .await?;
        let operation = operation_started_result(method, value)?;
        Ok(RemoteChainHeadBodyResponse { operation })
    }

    /// Start a chainHead_v1_storage operation.
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
            .map(encode_storage_query_item)
            .collect::<Vec<_>>();
        let child_trie = request
            .child_trie
            .as_ref()
            .map(|bytes| Value::String(encode_hex(bytes)))
            .unwrap_or(Value::Null);

        let value = connection
            .request_value(
                method,
                "chainHead_v1_storage",
                json!([
                    remote_follow_id,
                    encode_hex(&request.hash),
                    Value::Array(items),
                    child_trie,
                ]),
            )
            .await?;
        let operation = operation_started_result(method, value)?;
        Ok(RemoteChainHeadStorageResponse { operation })
    }

    /// Start a chainHead_v1_call operation.
    pub async fn remote_chain_head_call(
        &self,
        request: RemoteChainHeadCallRequest,
    ) -> Result<RemoteChainHeadCallResponse, RuntimeFailure> {
        let method = "remote_chain_head_call";
        let connection = self.connection_for(method, &request.genesis_hash).await?;
        let remote_follow_id = self
            .ensure_follow_context(method, &connection, request.follow_subscription_id, true)
            .await?;

        let value = connection
            .request_value(
                method,
                "chainHead_v1_call",
                json!([
                    remote_follow_id,
                    encode_hex(&request.hash),
                    request.function,
                    encode_hex(&request.call_parameters),
                ]),
            )
            .await?;
        let operation = operation_started_result(method, value)?;
        Ok(RemoteChainHeadCallResponse { operation })
    }

    /// Release pinned blocks.
    pub async fn remote_chain_head_unpin(
        &self,
        request: RemoteChainHeadUnpinRequest,
    ) -> Result<(), RuntimeFailure> {
        let method = "remote_chain_head_unpin";
        let connection = self.connection_for(method, &request.genesis_hash).await?;
        let remote_follow_id = self
            .ensure_follow_context(method, &connection, request.follow_subscription_id, false)
            .await?;
        let hashes: Vec<Value> = request
            .hashes
            .iter()
            .map(|hash| Value::String(encode_hex(hash)))
            .collect();
        let value = connection
            .request_value(
                method,
                "chainHead_v1_unpin",
                json!([remote_follow_id, Value::Array(hashes)]),
            )
            .await?;
        match value {
            Value::Null => Ok(()),
            _ => Err(RuntimeFailure::host_failure(
                method,
                "unexpected chainHead_v1_unpin result",
            )),
        }
    }

    /// Continue a paused operation.
    pub async fn remote_chain_head_continue(
        &self,
        request: RemoteChainHeadContinueRequest,
    ) -> Result<(), RuntimeFailure> {
        let method = "remote_chain_head_continue";
        let connection = self.connection_for(method, &request.genesis_hash).await?;
        let remote_follow_id = self
            .ensure_follow_context(method, &connection, request.follow_subscription_id, false)
            .await?;
        let value = connection
            .request_value(
                method,
                "chainHead_v1_continue",
                json!([remote_follow_id, request.operation_id]),
            )
            .await?;
        match value {
            Value::Null => Ok(()),
            _ => Err(RuntimeFailure::host_failure(
                method,
                "unexpected chainHead_v1_continue result",
            )),
        }
    }

    /// Stop a chain-head operation.
    pub async fn remote_chain_head_stop_operation(
        &self,
        request: RemoteChainHeadStopOperationRequest,
    ) -> Result<(), RuntimeFailure> {
        let method = "remote_chain_head_stop_operation";
        let connection = self.connection_for(method, &request.genesis_hash).await?;
        let remote_follow_id = self
            .ensure_follow_context(method, &connection, request.follow_subscription_id, false)
            .await?;
        let value = connection
            .request_value(
                method,
                "chainHead_v1_stopOperation",
                json!([remote_follow_id, request.operation_id]),
            )
            .await?;
        match value {
            Value::Null => Ok(()),
            _ => Err(RuntimeFailure::host_failure(
                method,
                "unexpected chainHead_v1_stopOperation result",
            )),
        }
    }

    /// Echo back the chain genesis hash via chainSpec_v1_genesisHash.
    pub async fn remote_chain_spec_genesis_hash(
        &self,
        genesis_hash: Vec<u8>,
    ) -> Result<RemoteChainSpecGenesisHashResponse, RuntimeFailure> {
        let method = "remote_chain_spec_genesis_hash";
        let connection = self.connection_for(method, &genesis_hash).await?;
        let value = connection
            .request_value(method, "chainSpec_v1_genesisHash", json!([]))
            .await?;
        match value {
            Value::String(encoded) => {
                let bytes = decode_hex(&encoded)
                    .map_err(|reason| RuntimeFailure::host_failure(method, reason))?;
                Ok(RemoteChainSpecGenesisHashResponse {
                    genesis_hash: bytes,
                })
            }
            _ => Err(RuntimeFailure::host_failure(
                method,
                "unexpected chainSpec_v1_genesisHash result",
            )),
        }
    }

    /// Fetch the chain display name via chainSpec_v1_chainName.
    pub async fn remote_chain_spec_chain_name(
        &self,
        genesis_hash: Vec<u8>,
    ) -> Result<RemoteChainSpecChainNameResponse, RuntimeFailure> {
        let method = "remote_chain_spec_chain_name";
        let connection = self.connection_for(method, &genesis_hash).await?;
        let value = connection
            .request_value(method, "chainSpec_v1_chainName", json!([]))
            .await?;
        match value {
            Value::String(name) => Ok(RemoteChainSpecChainNameResponse { chain_name: name }),
            _ => Err(RuntimeFailure::host_failure(
                method,
                "unexpected chainSpec_v1_chainName result",
            )),
        }
    }

    /// Fetch the chain JSON properties via chainSpec_v1_properties.
    pub async fn remote_chain_spec_properties(
        &self,
        genesis_hash: Vec<u8>,
    ) -> Result<RemoteChainSpecPropertiesResponse, RuntimeFailure> {
        let method = "remote_chain_spec_properties";
        let connection = self.connection_for(method, &genesis_hash).await?;
        let value = connection
            .request_value(method, "chainSpec_v1_properties", json!([]))
            .await?;
        let properties = serde_json::to_string(&value)
            .map_err(|err| RuntimeFailure::host_failure(method, err.to_string()))?;
        Ok(RemoteChainSpecPropertiesResponse { properties })
    }

    /// Broadcast a signed transaction via transaction_v1_broadcast.
    pub async fn remote_chain_transaction_broadcast(
        &self,
        request: RemoteChainTransactionBroadcastRequest,
    ) -> Result<RemoteChainTransactionBroadcastResponse, RuntimeFailure> {
        let method = "remote_chain_transaction_broadcast";
        let connection = self.connection_for(method, &request.genesis_hash).await?;
        let value = connection
            .request_value(
                method,
                "transaction_v1_broadcast",
                json!([encode_hex(&request.transaction)]),
            )
            .await?;
        match value {
            Value::Null => Ok(RemoteChainTransactionBroadcastResponse { operation_id: None }),
            Value::String(operation_id) => Ok(RemoteChainTransactionBroadcastResponse {
                operation_id: Some(operation_id),
            }),
            _ => Err(RuntimeFailure::host_failure(
                method,
                "unexpected transaction_v1_broadcast result",
            )),
        }
    }

    /// Stop a transaction broadcast via transaction_v1_stop.
    pub async fn remote_chain_transaction_stop(
        &self,
        request: RemoteChainTransactionStopRequest,
    ) -> Result<(), RuntimeFailure> {
        let method = "remote_chain_transaction_stop";
        let connection = self.connection_for(method, &request.genesis_hash).await?;
        let value = connection
            .request_value(method, "transaction_v1_stop", json!([request.operation_id]))
            .await?;
        match value {
            Value::Null => Ok(()),
            _ => Err(RuntimeFailure::host_failure(
                method,
                "unexpected transaction_v1_stop result",
            )),
        }
    }

    async fn connection_for(
        &self,
        method: &'static str,
        genesis_hash: &[u8],
    ) -> Result<Arc<ChainConnection>, RuntimeFailure> {
        let key = encode_hex(genesis_hash);
        if let Some(connection) = self.connections.lock().unwrap().get(&key).cloned() {
            if !connection.is_closed() {
                return Ok(connection);
            }
            self.connections.lock().unwrap().remove(&key);
        }

        let rpc = self
            .provider
            .connect(genesis_hash.to_owned())
            .await
            .map_err(|failure| match failure.kind() {
                RuntimeFailureKind::Unavailable => RuntimeFailure::unavailable(method),
                RuntimeFailureKind::HostFailure => {
                    RuntimeFailure::host_failure(method, failure.reason())
                }
            })?;
        let connection = ChainConnection::new(rpc, self.spawner.clone());
        self.connections
            .lock()
            .unwrap()
            .insert(key, connection.clone());
        Ok(connection)
    }

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

    async fn ensure_follow_context(
        &self,
        method: &'static str,
        connection: &Arc<ChainConnection>,
        local_follow_id: String,
        with_runtime: bool,
    ) -> Result<String, RuntimeFailure> {
        let remote_follow_id = connection
            .ensure_remote_follow(local_follow_id.clone(), with_runtime)
            .await?;
        if with_runtime && !connection.follow_with_runtime(&local_follow_id) {
            return Err(RuntimeFailure::host_failure(
                method,
                "follow subscription was created without runtime metadata",
            ));
        }
        Ok(remote_follow_id)
    }

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
    rpc: Arc<dyn JsonRpcConnection>,
    request_ids: AtomicU64,
    closed: AtomicBool,
    follows: Mutex<HashMap<String, FollowState>>,
    follows_by_remote: Mutex<HashMap<String, String>>,
    pending_follow_events: Mutex<HashMap<String, Vec<RemoteChainHeadFollowItem>>>,
    follow_setups: Mutex<HashMap<String, FollowSetup>>,
    requests: Mutex<HashMap<String, PendingRequest>>,
}

impl ChainConnection {
    fn new(rpc: Arc<dyn JsonRpcConnection>, spawner: Spawner) -> Arc<Self> {
        let connection = Arc::new(Self {
            rpc,
            request_ids: AtomicU64::new(1),
            closed: AtomicBool::new(false),
            follows: Mutex::new(HashMap::new()),
            follows_by_remote: Mutex::new(HashMap::new()),
            pending_follow_events: Mutex::new(HashMap::new()),
            follow_setups: Mutex::new(HashMap::new()),
            requests: Mutex::new(HashMap::new()),
        });
        connection.clone().spawn_response_loop(spawner);
        connection
    }

    fn spawn_response_loop(self: Arc<Self>, spawner: Spawner) {
        let rpc = self.rpc.clone();
        let fut = async move {
            let mut responses = rpc.responses();
            while let Some(response) = responses.next().await {
                if let Err(failure) = self.handle_response(&response) {
                    self.close_with_failure(failure);
                    return;
                }
            }
            self.close_with_failure(RuntimeFailure::unavailable(FOLLOW_METHOD));
        };
        (spawner)(fut.boxed());
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    async fn request_value(
        &self,
        method: &'static str,
        rpc_method: &'static str,
        params: Value,
    ) -> Result<Value, RuntimeFailure> {
        let request_id = format!(
            "truapi:{}",
            self.request_ids.fetch_add(1, Ordering::Relaxed)
        );
        let (tx, rx) = oneshot::channel();
        {
            // Check `closed` and insert under the same lock `close_with_failure`
            // takes, so the connection cannot drain the request map between the
            // check and the insert and leave this request to hang forever.
            let mut requests = self.requests.lock().unwrap();
            if self.is_closed() {
                return Err(RuntimeFailure::unavailable(method));
            }
            requests.insert(request_id.clone(), PendingRequest { method, tx });
        }
        self.rpc.send(
            json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "method": rpc_method,
                "params": params,
            })
            .to_string(),
        );

        match rx.await {
            Ok(result) => result,
            Err(_) => Err(RuntimeFailure::unavailable(method)),
        }
    }

    fn follow_with_runtime(&self, local_follow_id: &str) -> bool {
        self.follows
            .lock()
            .unwrap()
            .get(local_follow_id)
            .map(|follow| follow.with_runtime)
            .unwrap_or(false)
    }

    fn remote_follow_id(&self, local_follow_id: &str) -> Option<String> {
        self.follows
            .lock()
            .unwrap()
            .get(local_follow_id)
            .and_then(|follow| follow.remote_follow_id.clone())
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
                        remote_follow_id: None,
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

    /// Body of the single-flight follow setup: ensure the `FollowState`
    /// exists, issue `chainHead_v1_follow`, and record the remote id.
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
                remote_follow_id: None,
                sender: None,
            });

        let remote_follow_id = match self
            .request_value(FOLLOW_METHOD, "chainHead_v1_follow", json!([with_runtime]))
            .await
        {
            Ok(Value::String(value)) => value,
            Ok(_) => {
                self.remove_follow(&local_follow_id);
                return Err(RuntimeFailure::host_failure(
                    FOLLOW_METHOD,
                    "unexpected chainHead_v1_follow result",
                ));
            }
            Err(failure) => {
                self.remove_follow(&local_follow_id);
                return Err(failure);
            }
        };

        self.set_remote_follow_id(&local_follow_id, remote_follow_id.clone());
        Ok(remote_follow_id)
    }

    fn set_remote_follow_id(&self, local_follow_id: &str, remote_follow_id: String) {
        let attached = {
            let mut follows = self.follows.lock().unwrap();
            if let Some(follow) = follows.get_mut(local_follow_id) {
                follow.remote_follow_id = Some(remote_follow_id.clone());
                self.follows_by_remote
                    .lock()
                    .unwrap()
                    .insert(remote_follow_id.clone(), local_follow_id.to_string());
                true
            } else {
                false
            }
        };
        if !attached {
            // The local follow was torn down (stream dropped, connection
            // closed) while `chainHead_v1_follow` was in flight. Release the
            // now-orphaned remote subscription rather than leaking it on the
            // node, and drop any events buffered for it.
            self.send_unfollow(&remote_follow_id);
            self.pending_follow_events
                .lock()
                .unwrap()
                .remove(&remote_follow_id);
            return;
        }
        let buffered = self
            .pending_follow_events
            .lock()
            .unwrap()
            .remove(&remote_follow_id)
            .unwrap_or_default();
        for event in buffered {
            self.deliver_follow_event(local_follow_id, event);
        }
    }

    fn remove_follow(&self, local_follow_id: &str) {
        self.follow_setups.lock().unwrap().remove(local_follow_id);
        if let Some(follow) = self.follows.lock().unwrap().remove(local_follow_id)
            && let Some(remote_follow_id) = follow.remote_follow_id
        {
            self.follows_by_remote
                .lock()
                .unwrap()
                .remove(&remote_follow_id);
        }
    }

    fn unfollow(&self, local_follow_id: &str) {
        let remote_follow_id = self.remote_follow_id(local_follow_id);
        self.remove_follow(local_follow_id);
        let Some(remote_follow_id) = remote_follow_id else {
            return;
        };
        self.send_unfollow(&remote_follow_id);
    }

    /// Send a `chainHead_v1_unfollow` for `remote_follow_id`. Best-effort: a
    /// closed connection simply drops the request.
    fn send_unfollow(&self, remote_follow_id: &str) {
        self.rpc.send(
            json!({
                "jsonrpc": "2.0",
                "id": format!("truapi:{}", self.request_ids.fetch_add(1, Ordering::Relaxed)),
                "method": "chainHead_v1_unfollow",
                "params": [remote_follow_id],
            })
            .to_string(),
        );
    }

    fn handle_response(&self, response: &str) -> Result<(), RuntimeFailure> {
        let value: Value = serde_json::from_str(response).map_err(|error| {
            RuntimeFailure::host_failure(FOLLOW_METHOD, format!("invalid json-rpc frame: {error}"))
        })?;

        if value.get("method") == Some(&Value::String("chainHead_v1_followEvent".to_string())) {
            return self.handle_follow_notification(&value);
        }

        let Some(request_id) = value.get("id").and_then(json_id) else {
            return Ok(());
        };
        let Some(pending) = self.requests.lock().unwrap().remove(&request_id) else {
            return Ok(());
        };

        if let Some(result) = value.get("result") {
            let _ = pending.tx.send(Ok(result.clone()));
            return Ok(());
        }

        if let Some(error) = value.get("error") {
            let reason = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("json-rpc error")
                .to_string();
            let _ = pending
                .tx
                .send(Err(RuntimeFailure::host_failure(pending.method, reason)));
            return Ok(());
        }

        let _ = pending.tx.send(Err(RuntimeFailure::host_failure(
            pending.method,
            "json-rpc response missing result and error",
        )));
        Ok(())
    }

    fn handle_follow_notification(&self, value: &Value) -> Result<(), RuntimeFailure> {
        let params = value
            .get("params")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                RuntimeFailure::host_failure(
                    FOLLOW_METHOD,
                    "missing chainHead_v1_followEvent params",
                )
            })?;
        let remote_follow_id = params
            .get("subscription")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                RuntimeFailure::host_failure(
                    FOLLOW_METHOD,
                    "missing chainHead_v1_followEvent subscription",
                )
            })?;
        let event = parse_follow_event(params.get("result").ok_or_else(|| {
            RuntimeFailure::host_failure(FOLLOW_METHOD, "missing chainHead_v1_followEvent result")
        })?)?;
        let local_follow_id = self
            .follows_by_remote
            .lock()
            .unwrap()
            .get(remote_follow_id)
            .cloned();
        match local_follow_id {
            Some(local_follow_id) => self.deliver_follow_event(&local_follow_id, event),
            None => {
                let mut pending = self.pending_follow_events.lock().unwrap();
                // Bound the buffer in both dimensions so a misbehaving node, or
                // late events for a retired remote id, cannot grow memory
                // without limit while a follow id is being established.
                let known = pending.contains_key(remote_follow_id);
                if !known && pending.len() >= MAX_PENDING_FOLLOW_EVENT_IDS {
                    return Ok(());
                }
                let buffer = pending.entry(remote_follow_id.to_string()).or_default();
                if buffer.len() >= MAX_PENDING_FOLLOW_EVENTS_PER_ID {
                    return Ok(());
                }
                buffer.push(event);
            }
        }
        Ok(())
    }

    fn deliver_follow_event(&self, local_follow_id: &str, event: RemoteChainHeadFollowItem) {
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
            self.remove_follow(local_follow_id);
        }
    }

    fn close_with_failure(&self, failure: RuntimeFailure) {
        // Flip `closed` while holding the requests lock so it is mutually
        // exclusive with `request_value`'s check-and-insert: a request is
        // either seen here and failed, or sees `closed` and bails, but never
        // slips through to hang.
        let requests = {
            let mut requests = self.requests.lock().unwrap();
            self.closed.store(true, Ordering::Relaxed);
            std::mem::take(&mut *requests)
        };
        for (_, request) in requests {
            let mapped = match failure.kind() {
                RuntimeFailureKind::Unavailable => RuntimeFailure::unavailable(request.method),
                RuntimeFailureKind::HostFailure => {
                    RuntimeFailure::host_failure(request.method, failure.reason())
                }
            };
            let _ = request.tx.send(Err(mapped));
        }

        let follows = std::mem::take(&mut *self.follows.lock().unwrap());
        self.follows_by_remote.lock().unwrap().clear();
        self.pending_follow_events.lock().unwrap().clear();
        self.follow_setups.lock().unwrap().clear();
        for (_, follow) in follows {
            if let Some(sender) = follow.sender {
                let _ = sender.unbounded_send(FollowSignal::Interrupt);
            }
        }
    }
}

struct PendingRequest {
    method: &'static str,
    tx: oneshot::Sender<Result<Value, RuntimeFailure>>,
}

struct FollowState {
    with_runtime: bool,
    remote_follow_id: Option<String>,
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

fn parse_follow_event(value: &Value) -> Result<RemoteChainHeadFollowItem, RuntimeFailure> {
    let event = value
        .get("event")
        .and_then(Value::as_str)
        .ok_or_else(|| RuntimeFailure::host_failure(FOLLOW_METHOD, "missing event name"))?;
    match event {
        "initialized" => Ok(RemoteChainHeadFollowItem::Initialized {
            finalized_block_hashes: decode_hex_vec_field(value, "finalizedBlockHashes")?,
            finalized_block_runtime: value
                .get("finalizedBlockRuntime")
                .map(parse_runtime_type)
                .transpose()
                .map_err(|reason| RuntimeFailure::host_failure(FOLLOW_METHOD, reason))?,
        }),
        "newBlock" => Ok(RemoteChainHeadFollowItem::NewBlock {
            block_hash: decode_hex_field(value, "blockHash")?,
            parent_block_hash: decode_hex_field(value, "parentBlockHash")?,
            new_runtime: value
                .get("newRuntime")
                .map(parse_runtime_type)
                .transpose()
                .map_err(|reason| RuntimeFailure::host_failure(FOLLOW_METHOD, reason))?,
        }),
        "bestBlockChanged" => Ok(RemoteChainHeadFollowItem::BestBlockChanged {
            best_block_hash: decode_hex_field(value, "bestBlockHash")?,
        }),
        "finalized" => Ok(RemoteChainHeadFollowItem::Finalized {
            finalized_block_hashes: decode_hex_vec_field(value, "finalizedBlockHashes")?,
            pruned_block_hashes: decode_hex_vec_field(value, "prunedBlockHashes")?,
        }),
        "operationBodyDone" => Ok(RemoteChainHeadFollowItem::OperationBodyDone {
            operation_id: string_field(value, "operationId")?,
            value: decode_hex_vec_field(value, "value")?,
        }),
        "operationCallDone" => Ok(RemoteChainHeadFollowItem::OperationCallDone {
            operation_id: string_field(value, "operationId")?,
            output: decode_hex_field(value, "output")?,
        }),
        "operationStorageItems" => Ok(RemoteChainHeadFollowItem::OperationStorageItems {
            operation_id: string_field(value, "operationId")?,
            items: parse_storage_result_items(value)?,
        }),
        "operationStorageDone" => Ok(RemoteChainHeadFollowItem::OperationStorageDone {
            operation_id: string_field(value, "operationId")?,
        }),
        "operationWaitingForContinue" => {
            Ok(RemoteChainHeadFollowItem::OperationWaitingForContinue {
                operation_id: string_field(value, "operationId")?,
            })
        }
        "operationInaccessible" => Ok(RemoteChainHeadFollowItem::OperationInaccessible {
            operation_id: string_field(value, "operationId")?,
        }),
        "operationError" => Ok(RemoteChainHeadFollowItem::OperationError {
            operation_id: string_field(value, "operationId")?,
            error: string_field(value, "error")?,
        }),
        "stop" => Ok(RemoteChainHeadFollowItem::Stop),
        other => Err(RuntimeFailure::host_failure(
            FOLLOW_METHOD,
            format!("unsupported follow event {other}"),
        )),
    }
}

fn parse_storage_result_items(value: &Value) -> Result<Vec<StorageResultItem>, RuntimeFailure> {
    let Some(items) = value.get("items").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    items
        .iter()
        .map(|item| -> Result<StorageResultItem, RuntimeFailure> {
            let key = decode_hex_field(item, "key")?;
            let value = optional_hex_field(item, "value")?;
            let hash = optional_hex_field(item, "hash")?;
            let closest_descendant_merkle_value =
                optional_hex_field(item, "closestDescendantMerkleValue")?;
            Ok(StorageResultItem {
                key,
                value,
                hash,
                closest_descendant_merkle_value,
            })
        })
        .collect()
}

fn operation_started_result(
    method: &'static str,
    value: Value,
) -> Result<OperationStartedResult, RuntimeFailure> {
    let result = value
        .get("result")
        .and_then(Value::as_str)
        .ok_or_else(|| RuntimeFailure::host_failure(method, "missing operation result kind"))?;
    match result {
        "started" => Ok(OperationStartedResult::Started {
            operation_id: value
                .get("operationId")
                .and_then(Value::as_str)
                .ok_or_else(|| RuntimeFailure::host_failure(method, "missing operation id"))?
                .to_string(),
        }),
        "limitReached" => Ok(OperationStartedResult::LimitReached),
        other => Err(RuntimeFailure::host_failure(
            method,
            format!("unexpected operation result {other}"),
        )),
    }
}

fn parse_runtime_type(value: &Value) -> Result<RuntimeType, String> {
    let Some(kind) = value.get("type").and_then(Value::as_str) else {
        return Ok(RuntimeType::Invalid {
            error: "missing runtime type".to_string(),
        });
    };
    match kind {
        "valid" => {
            let spec = value
                .get("spec")
                .and_then(Value::as_object)
                .ok_or_else(|| "missing valid runtime spec".to_string())?;
            let apis = spec
                .get("apis")
                .and_then(Value::as_object)
                .map(|apis| {
                    let mut entries = apis
                        .iter()
                        .filter_map(|(name, version)| {
                            version.as_u64().map(|version| RuntimeApi {
                                name: name.clone(),
                                version: version as u32,
                            })
                        })
                        .collect::<Vec<_>>();
                    entries.sort_by(|left, right| left.name.cmp(&right.name));
                    entries
                })
                .unwrap_or_default();
            Ok(RuntimeType::Valid(RuntimeSpec {
                spec_name: string_object_field(spec, "specName")?,
                impl_name: string_object_field(spec, "implName")?,
                spec_version: u32_object_field(spec, "specVersion")?,
                impl_version: u32_object_field(spec, "implVersion")?,
                transaction_version: spec
                    .get("transactionVersion")
                    .and_then(Value::as_u64)
                    .map(|value| value as u32),
                apis,
            }))
        }
        "invalid" => Ok(RuntimeType::Invalid {
            error: value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("invalid runtime")
                .to_string(),
        }),
        other => Err(format!("unsupported runtime type {other}")),
    }
}

fn encode_storage_query_item(item: &StorageQueryItem) -> Value {
    let query_type = match item.query_type {
        StorageQueryType::Value => "value",
        StorageQueryType::Hash => "hash",
        StorageQueryType::ClosestDescendantMerkleValue => "closestDescendantMerkleValue",
        StorageQueryType::DescendantsValues => "descendantsValues",
        StorageQueryType::DescendantsHashes => "descendantsHashes",
    };
    let mut map = Map::new();
    map.insert("key".to_string(), Value::String(encode_hex(&item.key)));
    map.insert("type".to_string(), Value::String(query_type.to_string()));
    Value::Object(map)
}

fn json_id(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn decode_hex_field(value: &Value, field: &str) -> Result<Vec<u8>, RuntimeFailure> {
    let string = string_field(value, field)?;
    decode_hex(&string).map_err(|reason| RuntimeFailure::host_failure(FOLLOW_METHOD, reason))
}

fn optional_hex_field(value: &Value, field: &str) -> Result<Option<Vec<u8>>, RuntimeFailure> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => decode_hex(text)
            .map(Some)
            .map_err(|reason| RuntimeFailure::host_failure(FOLLOW_METHOD, reason)),
        Some(_) => Err(RuntimeFailure::host_failure(
            FOLLOW_METHOD,
            format!("invalid {field}"),
        )),
    }
}

fn decode_hex_vec_field(value: &Value, field: &str) -> Result<Vec<Vec<u8>>, RuntimeFailure> {
    let Some(values) = value.get(field).and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| {
                    RuntimeFailure::host_failure(FOLLOW_METHOD, format!("invalid {field}"))
                })
                .and_then(|value| {
                    decode_hex(value)
                        .map_err(|reason| RuntimeFailure::host_failure(FOLLOW_METHOD, reason))
                })
        })
        .collect()
}

fn string_field(value: &Value, field: &str) -> Result<String, RuntimeFailure> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| RuntimeFailure::host_failure(FOLLOW_METHOD, format!("missing {field}")))
}

fn string_object_field(value: &Map<String, Value>, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing {field}"))
}

fn u32_object_field(value: &Map<String, Value>, field: &str) -> Result<u32, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .map(|value| value as u32)
        .ok_or_else(|| format!("missing {field}"))
}

/// Encode a byte slice as a `0x`-prefixed lowercase hex string.
pub(crate) fn encode_hex(value: &[u8]) -> String {
    let mut out = String::from("0x");
    for byte in value {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if !value.len().is_multiple_of(2) {
        return Err("invalid hex length".to_string());
    }

    let mut out = Vec::with_capacity(value.len() / 2);
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let chunk = std::str::from_utf8(&bytes[index..index + 2]).map_err(|_| "invalid hex")?;
        let byte = u8::from_str_radix(chunk, 16).map_err(|_| "invalid hex")?;
        out.push(byte);
        index += 2;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Spawner adapter for the chain runtime.
// ---------------------------------------------------------------------------
//
// `ChainRuntime` uses a small wrapper future type so callers can construct
// the runtime with a generic `Spawner` without exposing the `BoxFuture`
// requirement in the public signature.

/// Convenience: build a [`Spawner`] that runs each spawned future on a fresh
/// OS thread driven by [`futures::executor::block_on`]. Useful for tests and
/// embedders that have not yet wired a real runtime. Not available on wasm32
/// since the platform has no threads.
#[cfg(not(target_arch = "wasm32"))]
pub fn thread_per_task_spawner() -> Spawner {
    Arc::new(|fut: futures::future::BoxFuture<'static, ()>| {
        std::thread::spawn(move || futures::executor::block_on(fut));
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::SinkExt;
    use futures::channel::mpsc as fut_mpsc;
    use futures::stream::BoxStream;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn spawner_for_tests() -> Spawner {
        #[cfg(not(target_arch = "wasm32"))]
        {
            thread_per_task_spawner()
        }
        #[cfg(target_arch = "wasm32")]
        {
            Arc::new(futures::executor::block_on)
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

    /// Connection backed by an mpsc channel: tests push frames into `tx`
    /// to simulate asynchronous notifications. Used for follow-event tests.
    struct ChannelConnection {
        rx: Mutex<Option<fut_mpsc::UnboundedReceiver<String>>>,
        sent: Arc<Mutex<Vec<String>>>,
    }

    impl JsonRpcConnection for ChannelConnection {
        fn send(&self, request: String) {
            self.sent.lock().unwrap().push(request);
        }
        fn responses(&self) -> BoxStream<'static, String> {
            let rx = self
                .rx
                .lock()
                .unwrap()
                .take()
                .expect("responses taken twice");
            rx.boxed()
        }
    }

    struct ChannelProvider {
        sender: Arc<Mutex<Option<fut_mpsc::UnboundedSender<String>>>>,
        receiver: Arc<Mutex<Option<fut_mpsc::UnboundedReceiver<String>>>>,
        sent: Arc<Mutex<Vec<String>>>,
    }

    impl ChannelProvider {
        fn new() -> Self {
            let (tx, rx) = fut_mpsc::unbounded();
            Self {
                sender: Arc::new(Mutex::new(Some(tx))),
                receiver: Arc::new(Mutex::new(Some(rx))),
                sent: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn take_sender(&self) -> fut_mpsc::UnboundedSender<String> {
            self.sender
                .lock()
                .unwrap()
                .take()
                .expect("sender taken twice")
        }
    }

    #[async_trait]
    impl RuntimeChainProvider for ChannelProvider {
        async fn connect(
            &self,
            _genesis_hash: Vec<u8>,
        ) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
            let rx = self.receiver.lock().unwrap().take();
            Ok(Arc::new(ChannelConnection {
                rx: Mutex::new(rx),
                sent: self.sent.clone(),
            }))
        }
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

    #[test]
    fn header_request_routes_through_provider() {
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
        let provider = Arc::new(ChannelProvider::new());
        let tx = provider.take_sender();
        let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());

        // The follow setup needs to wait for the rpc response, so we splice
        // it in before starting the subscription.
        let mut tx_owned = tx.clone();
        futures::executor::block_on(async move {
            tx_owned
                .send(r#"{"jsonrpc":"2.0","id":"truapi:1","result":"REMOTE-FOLLOW"}"#.to_string())
                .await
                .unwrap();
        });

        let mut stream = runtime.remote_chain_head_follow(
            "local-follow".to_string(),
            RemoteChainHeadFollowRequest {
                genesis_hash: vec![0u8; 32],
                with_runtime: false,
            },
        );

        // Now push a follow event keyed by remote subscription id.
        let mut tx_owned = tx.clone();
        futures::executor::block_on(async move {
            tx_owned.send(
                r#"{"jsonrpc":"2.0","method":"chainHead_v1_followEvent","params":{"subscription":"REMOTE-FOLLOW","result":{"event":"initialized","finalizedBlockHashes":["0xaabbccdd"]}}}"#
                    .to_string(),
            ).await.unwrap();
            tx_owned.send(
                r#"{"jsonrpc":"2.0","method":"chainHead_v1_followEvent","params":{"subscription":"REMOTE-FOLLOW","result":{"event":"stop"}}}"#
                    .to_string(),
            ).await.unwrap();
        });

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
                assert_eq!(finalized_block_hashes, &vec![vec![0xaa, 0xbb, 0xcc, 0xdd]]);
                assert!(finalized_block_runtime.is_none());
            }
            other => panic!("expected Initialized, got {other:?}"),
        }
        assert!(matches!(items[1], RemoteChainHeadFollowItem::Stop));
    }

    #[cfg_attr(target_arch = "wasm32", ignore)]
    #[test]
    fn drop_follow_stream_sends_unfollow() {
        let provider = Arc::new(ChannelProvider::new());
        let tx = provider.take_sender();
        let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());
        let sent = provider.sent.clone();

        // Pre-load the follow setup response.
        let mut tx_owned = tx.clone();
        futures::executor::block_on(async move {
            tx_owned
                .send(r#"{"jsonrpc":"2.0","id":"truapi:1","result":"REMOTE-FOLLOW"}"#.to_string())
                .await
                .unwrap();
        });

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
        let value: Value = serde_json::from_str(
            r#"{"type":"valid","spec":{"specName":"polkadot","implName":"parity-polkadot","specVersion":1000,"implVersion":1,"transactionVersion":24,"apis":{"0xbeef":2,"0xbabe":4}}}"#,
        )
        .unwrap();
        let runtime_type = parse_runtime_type(&value).unwrap();
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
