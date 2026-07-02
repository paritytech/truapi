//! `PlatformRuntimeHost` adapts a [`truapi_platform::Platform`] into the
//! typed `truapi::api::*` host traits the generated dispatcher routes to.
//!
//! Most methods are straight delegations to the platform; the rest carry
//! host-agnostic logic owned by the core (the chainHead-v1 runtime behind
//! the Chain surface, `dotns` URL parsing for `navigate_to`, and the
//! permission cache layer). Methods with no platform backing return
//! `CallError::unavailable()`.

pub(crate) mod auth_state;
mod identity;
pub(crate) mod sso_pairing;
pub(crate) mod sso_remote;
pub(crate) mod statement_store;
mod statement_store_rpc;

use std::sync::{Arc, Mutex};

use crate::chain_runtime::{ChainRuntime, RuntimeChainProvider, RuntimeFailure};
use crate::host_logic::dotns::{NavigateDecision, parse_navigate};
use crate::host_logic::entropy::derive_product_entropy_from_source;
use crate::host_logic::features::feature_supported;
use crate::host_logic::permissions::PermissionsService;
use crate::host_logic::product_account::{
    derive_product_public_key, is_product_identifier, normalize_product_identifier,
    product_public_key_to_address,
};
use crate::host_logic::session::{
    SessionInfo, SessionState, decode_persisted_session, encode_persisted_session,
};
use crate::host_logic::session_store::SessionStoreChangeNotifier;
use crate::host_logic::sso::messages::SsoSessionStatement;
use crate::host_logic::sso::messages::{
    OnExistingAllowancePolicy, SsoAllocationOutcome, SsoRemoteResponse, alias_request_message,
    create_transaction_message, decode_sso_session_statement, resource_allocation_message,
    sign_payload_message, sign_raw_message,
};
use crate::host_logic::statement_store::parse_new_statements_result;
use crate::subscription::Spawner;
use auth_state::AuthStateMachine;
use identity::resolve_session_identity_with_chain;
use sso_remote::{
    SSO_LOCAL_DISCONNECT_REASON, SSO_PEER_DISCONNECT_REASON, SessionDisconnects, sso_message_id,
};
use statement_store_rpc::StatementStoreRpc;

use futures::future::{AbortHandle, Abortable};
use futures::{FutureExt, StreamExt, stream};
#[cfg(test)]
use parity_scale_codec::Encode;
use tracing::{debug, info, instrument};
use truapi::api::{
    Account, Chain, Chat, CoinPayment, Entropy, LocalStorage, Notifications, Payment, Permissions,
    Preimage, ResourceAllocation, Signing, System, Theme,
};
use truapi::v01;
use truapi::versioned::account::{
    HostAccountConnectionStatusSubscribeItem, HostAccountCreateProofError,
    HostAccountCreateProofRequest, HostAccountCreateProofResponse, HostAccountGetAliasError,
    HostAccountGetAliasRequest, HostAccountGetAliasResponse, HostAccountGetError,
    HostAccountGetRequest, HostAccountGetResponse, HostGetLegacyAccountsError,
    HostGetLegacyAccountsRequest, HostGetLegacyAccountsResponse, HostGetUserIdError,
    HostGetUserIdRequest, HostGetUserIdResponse, HostRequestLoginError, HostRequestLoginRequest,
    HostRequestLoginResponse,
};
use truapi::versioned::chain::{
    RemoteChainHeadBodyError, RemoteChainHeadBodyRequest, RemoteChainHeadBodyResponse,
    RemoteChainHeadCallError, RemoteChainHeadCallRequest, RemoteChainHeadCallResponse,
    RemoteChainHeadContinueError, RemoteChainHeadContinueRequest, RemoteChainHeadContinueResponse,
    RemoteChainHeadFollowItem, RemoteChainHeadFollowRequest, RemoteChainHeadHeaderError,
    RemoteChainHeadHeaderRequest, RemoteChainHeadHeaderResponse, RemoteChainHeadStopOperationError,
    RemoteChainHeadStopOperationRequest, RemoteChainHeadStopOperationResponse,
    RemoteChainHeadStorageError, RemoteChainHeadStorageRequest, RemoteChainHeadStorageResponse,
    RemoteChainHeadUnpinError, RemoteChainHeadUnpinRequest, RemoteChainHeadUnpinResponse,
    RemoteChainSpecChainNameError, RemoteChainSpecChainNameRequest,
    RemoteChainSpecChainNameResponse, RemoteChainSpecGenesisHashError,
    RemoteChainSpecGenesisHashRequest, RemoteChainSpecGenesisHashResponse,
    RemoteChainSpecPropertiesError, RemoteChainSpecPropertiesRequest,
    RemoteChainSpecPropertiesResponse, RemoteChainTransactionBroadcastError,
    RemoteChainTransactionBroadcastRequest, RemoteChainTransactionBroadcastResponse,
    RemoteChainTransactionStopError, RemoteChainTransactionStopRequest,
    RemoteChainTransactionStopResponse,
};
use truapi::versioned::entropy::{
    HostDeriveEntropyError, HostDeriveEntropyRequest, HostDeriveEntropyResponse,
};
use truapi::versioned::local_storage::{
    HostLocalStorageClearError, HostLocalStorageClearRequest, HostLocalStorageClearResponse,
    HostLocalStorageReadError, HostLocalStorageReadRequest, HostLocalStorageReadResponse,
    HostLocalStorageWriteError, HostLocalStorageWriteRequest, HostLocalStorageWriteResponse,
};
use truapi::versioned::notifications::{
    HostPushNotificationCancelError, HostPushNotificationCancelRequest,
    HostPushNotificationCancelResponse, HostPushNotificationError, HostPushNotificationRequest,
    HostPushNotificationResponse,
};
use truapi::versioned::payment::{
    HostPaymentBalanceSubscribeError, HostPaymentBalanceSubscribeItem,
    HostPaymentBalanceSubscribeRequest, HostPaymentError, HostPaymentRequest, HostPaymentResponse,
    HostPaymentStatusSubscribeError, HostPaymentStatusSubscribeItem,
    HostPaymentStatusSubscribeRequest, HostPaymentTopUpError, HostPaymentTopUpRequest,
    HostPaymentTopUpResponse,
};
use truapi::versioned::permissions::{
    HostDevicePermissionError, HostDevicePermissionRequest, HostDevicePermissionResponse,
    RemotePermissionError, RemotePermissionRequest, RemotePermissionResponse,
};
use truapi::versioned::preimage::{
    RemotePreimageLookupSubscribeItem, RemotePreimageLookupSubscribeRequest,
    RemotePreimageSubmitError, RemotePreimageSubmitRequest, RemotePreimageSubmitResponse,
};
use truapi::versioned::resource_allocation::{
    HostRequestResourceAllocationError, HostRequestResourceAllocationRequest,
    HostRequestResourceAllocationResponse,
};
use truapi::versioned::signing::{
    HostCreateTransactionError, HostCreateTransactionRequest, HostCreateTransactionResponse,
    HostCreateTransactionWithLegacyAccountError, HostCreateTransactionWithLegacyAccountRequest,
    HostCreateTransactionWithLegacyAccountResponse, HostSignPayloadError, HostSignPayloadRequest,
    HostSignPayloadResponse, HostSignPayloadWithLegacyAccountError,
    HostSignPayloadWithLegacyAccountRequest, HostSignPayloadWithLegacyAccountResponse,
    HostSignRawError, HostSignRawRequest, HostSignRawResponse, HostSignRawWithLegacyAccountError,
    HostSignRawWithLegacyAccountRequest, HostSignRawWithLegacyAccountResponse,
};
use truapi::versioned::system::{
    HostFeatureSupportedError, HostFeatureSupportedRequest, HostFeatureSupportedResponse,
    HostNavigateToError, HostNavigateToRequest, HostNavigateToResponse,
};
use truapi::versioned::theme::HostThemeSubscribeItem;
use truapi::{CallContext, CallError, Subscription};
use truapi_platform::{
    AccountAliasReview, CoreStorageKey, CreateTransactionReview, JsonRpcConnection,
    PermissionAuthorizationRequest, PermissionAuthorizationStatus, Platform, PreimageSubmitReview,
    RuntimeConfig, SessionUiInfo, SignPayloadReview, SignRawReview, UserConfirmationReview,
};

/// Adapter that exposes a [`truapi_platform::Platform`] through the
/// `truapi::api::*` trait set the generated dispatcher routes to.
pub struct PlatformRuntimeHost {
    platform: Arc<dyn Platform>,
    runtime_config: RuntimeConfig,
    /// chainHead-v1 state machine. The provider adapter forwards
    /// [`truapi_platform::ChainProvider::connect`] into the json-rpc layer.
    chain: ChainRuntime,
    /// Currently-paired session, pushed by the host through a side channel.
    /// Account-management subscriptions read from this in lieu of round-tripping
    /// a callback on every connection-status query.
    session_state: Arc<SessionState>,
    session_store_changes: Arc<SessionStoreChangeNotifier>,
    session_disconnects: Arc<SessionDisconnects>,
    sso_disconnect_monitor: Arc<Mutex<Option<SsoDisconnectMonitor>>>,
    spawner: Spawner,
    /// Auth UI state machine; the single funnel for `auth_state_changed`.
    auth_state: AuthStateMachine,
}

struct SsoDisconnectMonitor {
    session_id_own: [u8; 32],
    session_id_peer: [u8; 32],
    abort: AbortHandle,
}

impl Drop for SsoDisconnectMonitor {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

impl PlatformRuntimeHost {
    /// Wrap a platform implementation. The runtime takes ownership via `Arc`.
    /// `spawner` is used by the embedded chain runtime to drive json-rpc
    /// response loops and follow-setup futures.
    pub fn new(
        platform: Arc<dyn Platform>,
        runtime_config: RuntimeConfig,
        spawner: Spawner,
    ) -> Self {
        let chain_provider = Self::chain_provider(platform.clone());
        Self {
            auth_state: AuthStateMachine::new(platform.clone()),
            platform,
            runtime_config,
            chain: ChainRuntime::new(chain_provider, spawner.clone()),
            session_state: SessionState::new(),
            session_store_changes: SessionStoreChangeNotifier::new(),
            session_disconnects: Arc::new(SessionDisconnects::default()),
            sso_disconnect_monitor: Arc::new(Mutex::new(None)),
            spawner,
        }
    }

    /// Compatibility constructor used only by tests that do not exercise
    /// product-scoped behavior.
    #[cfg(test)]
    fn new_compat(platform: Arc<dyn Platform>, spawner: Spawner) -> Self {
        Self::new(
            platform,
            RuntimeConfig::new(
                "unknown.dot".to_string(),
                truapi_platform::HostInfo {
                    name: "Polkadot Web".to_string(),
                    icon: Some("https://example.invalid/dotli.png".to_string()),
                    version: None,
                },
                truapi_platform::PlatformInfo::default(),
                [0; 32],
                "polkadotapp".to_string(),
            )
            .expect("compat runtime config is valid"),
            spawner,
        )
    }

    /// Chain provider backing the chainHead-v1 runtime. Chain access routes
    /// through the platform's host-owned `ChainProvider`.
    fn chain_provider(platform: Arc<dyn Platform>) -> Arc<dyn RuntimeChainProvider> {
        Arc::new(HostChainProvider { platform })
    }

    /// Clone of the shared session-state holder used by core subscriptions
    /// and tests. Real host lifecycle flows through CoreStorage session sync
    /// and `disconnect`.
    pub fn session_state(&self) -> Arc<SessionState> {
        self.session_state.clone()
    }

    /// Clone of the notifier used to wake the session-store sync loop.
    pub fn session_store_changes(&self) -> Arc<SessionStoreChangeNotifier> {
        self.session_store_changes.clone()
    }

    fn start_sso_disconnect_monitor(&self, session: &SessionInfo) {
        start_sso_disconnect_monitor(
            self.platform.clone(),
            self.runtime_config.clone(),
            self.session_state.clone(),
            self.session_disconnects.clone(),
            self.auth_state.clone(),
            self.sso_disconnect_monitor.clone(),
            self.spawner.clone(),
            session,
        );
    }

    /// Start syncing the in-memory session from the host-global auth session
    /// slot.
    /// The store emits coarse ticks; each tick triggers a fresh read so same-
    /// runtime writes and cross-runtime logout/re-pair take the same path.
    #[instrument(skip_all, fields(runtime.method = "session_store.sync"))]
    pub(crate) fn start_session_store_sync(&self, spawner: Spawner) {
        let platform = self.platform.clone();
        let chain = self.chain.clone();
        let runtime_config = self.runtime_config.clone();
        let session_state = self.session_state.clone();
        let auth_state = self.auth_state.clone();
        let session_disconnects = self.session_disconnects.clone();
        let sso_disconnect_monitor = self.sso_disconnect_monitor.clone();
        let spawner_for_monitor = self.spawner.clone();
        let session_store_changes = self.session_store_changes.clone();
        spawner(Box::pin(async move {
            let mut ticks = session_store_changes.subscribe();
            // Clearing the store can itself notify this subscription; clear at
            // most once per read-error streak so a persistently failing read
            // cannot spin the loop through its own clear notifications.
            let mut cleared_after_read_error = false;
            while ticks.next().await.is_some() {
                match platform
                    .read_core_storage(CoreStorageKey::AuthSession)
                    .await
                {
                    Ok(Some(blob)) => {
                        cleared_after_read_error = false;
                        match decode_persisted_session(&blob) {
                            Ok(session) => {
                                let resolved = resolve_session_identity_with_chain(
                                    &chain,
                                    runtime_config.people_chain_genesis_hash,
                                    session,
                                )
                                .await;
                                if encode_persisted_session(&resolved) != blob {
                                    let _ = platform
                                        .write_core_storage(
                                            CoreStorageKey::AuthSession,
                                            encode_persisted_session(&resolved),
                                        )
                                        .await;
                                }
                                auth_state.connected(&connected_session_ui_info(&resolved));
                                session_state.set_session(resolved);
                                if let Some(session) = session_state.current() {
                                    start_sso_disconnect_monitor(
                                        platform.clone(),
                                        runtime_config.clone(),
                                        session_state.clone(),
                                        session_disconnects.clone(),
                                        auth_state.clone(),
                                        sso_disconnect_monitor.clone(),
                                        spawner_for_monitor.clone(),
                                        &session,
                                    );
                                }
                            }
                            Err(_) => {
                                session_state.clear_session();
                                stop_sso_disconnect_monitor(&sso_disconnect_monitor);
                                let _ = platform
                                    .clear_core_storage(CoreStorageKey::AuthSession)
                                    .await;
                                auth_state.store_disconnected();
                            }
                        }
                    }
                    Ok(None) => {
                        cleared_after_read_error = false;
                        session_state.clear_session();
                        stop_sso_disconnect_monitor(&sso_disconnect_monitor);
                        auth_state.store_disconnected();
                    }
                    Err(_) => {
                        session_state.clear_session();
                        stop_sso_disconnect_monitor(&sso_disconnect_monitor);
                        auth_state.store_disconnected();
                        if !cleared_after_read_error {
                            cleared_after_read_error = true;
                            let _ = platform
                                .clear_core_storage(CoreStorageKey::AuthSession)
                                .await;
                        }
                    }
                }
            }
        }));
    }

    /// Core-owned logout/disconnect path. It best-effort notifies the SSO
    /// peer, then clears in-memory and persisted session state regardless of
    /// any transport failure.
    #[instrument(skip_all, fields(runtime.method = "account.disconnect"))]
    pub(crate) async fn disconnect(&self) {
        self.cancel_login();
        self.session_disconnects.notify(SSO_LOCAL_DISCONNECT_REASON);
        let session = self.session_state.current();
        self.clear_disconnected_session().await;
        if let Some(session) = session.as_ref() {
            let _ = self.submit_sso_disconnected(session).await;
        }
    }

    /// Cancel any in-flight `request_login` pairing: the host UI gets a
    /// `Disconnected` state immediately and the login flow resolves to
    /// `Rejected`. A no-op when no login is in progress.
    #[instrument(skip_all, fields(runtime.method = "account.cancel_login"))]
    pub(crate) fn cancel_login(&self) {
        self.auth_state.login_cancelled();
    }

    /// Static product/host configuration for this runtime instance.
    #[allow(dead_code)]
    pub fn runtime_config(&self) -> &RuntimeConfig {
        &self.runtime_config
    }

    fn is_product_account_valid_for_caller(&self, dot_ns_identifier: &str) -> bool {
        let dot_ns_identifier = normalize_product_identifier(dot_ns_identifier);
        let product_id = normalize_product_identifier(&self.runtime_config.product_id);
        if self.runtime_config.product_id.starts_with("localhost:") {
            is_product_identifier(&dot_ns_identifier)
        } else {
            dot_ns_identifier == product_id
        }
    }

    fn normalize_product_account_id(
        product_account_id: v01::ProductAccountId,
    ) -> v01::ProductAccountId {
        v01::ProductAccountId {
            dot_ns_identifier: normalize_product_identifier(&product_account_id.dot_ns_identifier),
            derivation_index: product_account_id.derivation_index,
        }
    }

    fn product_id(&self) -> String {
        normalize_product_identifier(&self.runtime_config.product_id)
    }

    fn legacy_slot_zero_public_key(&self, session: &SessionInfo) -> Result<[u8; 32], String> {
        derive_product_public_key(session.public_key, &self.product_id(), 0)
            .map_err(|err| err.to_string())
    }

    #[instrument(skip_all, fields(runtime.method = "session_store.clear_disconnected"))]
    async fn clear_disconnected_session(&self) {
        debug!("clearing disconnected SSO session state");
        stop_sso_disconnect_monitor(&self.sso_disconnect_monitor);
        self.session_state.clear_session();
        let _ = self
            .platform
            .clear_core_storage(CoreStorageKey::AuthSession)
            .await;
        self.auth_state.store_disconnected();
        let _ = self
            .platform
            .clear_core_storage(CoreStorageKey::PairingDeviceIdentity)
            .await;
    }
}

fn stop_sso_disconnect_monitor(monitor: &Arc<Mutex<Option<SsoDisconnectMonitor>>>) {
    monitor
        .lock()
        .expect("SSO disconnect monitor mutex poisoned")
        .take();
}

/// True when an own/peer session-id pair matches the captured SSO session.
fn same_sso_session(
    own: [u8; 32],
    peer: [u8; 32],
    sso: &crate::host_logic::session::SsoSessionInfo,
) -> bool {
    own == sso.session_id_own && peer == sso.session_id_peer
}

#[allow(clippy::too_many_arguments)]
fn start_sso_disconnect_monitor(
    platform: Arc<dyn Platform>,
    runtime_config: RuntimeConfig,
    session_state: Arc<SessionState>,
    session_disconnects: Arc<SessionDisconnects>,
    auth_state: AuthStateMachine,
    monitor: Arc<Mutex<Option<SsoDisconnectMonitor>>>,
    spawner: Spawner,
    session: &SessionInfo,
) {
    let Some(sso) = session.sso.clone() else {
        stop_sso_disconnect_monitor(&monitor);
        return;
    };

    {
        let mut current = monitor
            .lock()
            .expect("SSO disconnect monitor mutex poisoned");
        if current.as_ref().is_some_and(|active| {
            same_sso_session(active.session_id_own, active.session_id_peer, &sso)
        }) {
            return;
        }
        let (abort, registration) = AbortHandle::new_pair();
        *current = Some(SsoDisconnectMonitor {
            session_id_own: sso.session_id_own,
            session_id_peer: sso.session_id_peer,
            abort,
        });

        let monitor_slot = monitor.clone();
        let monitor_spawner = spawner.clone();
        let future = async move {
            let result = wait_for_sso_peer_disconnect(
                platform.clone(),
                runtime_config.people_chain_genesis_hash,
                sso.clone(),
                monitor_spawner.clone(),
            )
            .await;
            let peer_disconnected = result.is_ok();
            let should_clear = peer_disconnected
                && session_state.current().as_ref().is_some_and(|current| {
                    current.sso.as_ref().is_some_and(|current_sso| {
                        same_sso_session(
                            current_sso.session_id_own,
                            current_sso.session_id_peer,
                            &sso,
                        )
                    })
                });
            {
                let mut active = monitor_slot
                    .lock()
                    .expect("SSO disconnect monitor mutex poisoned");
                if active.as_ref().is_some_and(|active| {
                    same_sso_session(active.session_id_own, active.session_id_peer, &sso)
                }) {
                    *active = None;
                }
            }
            if should_clear {
                session_disconnects.notify(SSO_PEER_DISCONNECT_REASON);
                session_state.clear_session();
                let _ = platform
                    .clear_core_storage(CoreStorageKey::AuthSession)
                    .await;
                auth_state.store_disconnected();
                let _ = platform
                    .clear_core_storage(CoreStorageKey::PairingDeviceIdentity)
                    .await;
            }
        };
        spawner(Box::pin(Abortable::new(future, registration).map(|_| ())));
    }
}

#[instrument(skip_all, fields(runtime.method = "sso.peer_disconnect.monitor"))]
async fn wait_for_sso_peer_disconnect(
    platform: Arc<dyn Platform>,
    people_chain_genesis_hash: [u8; 32],
    session: crate::host_logic::session::SsoSessionInfo,
    spawner: Spawner,
) -> Result<(), String> {
    let statement_store = StatementStoreRpc::new(platform, people_chain_genesis_hash, spawner);
    let rpc_client = statement_store.client("SSO disconnect monitor").await?;
    let mut subscription =
        statement_store_rpc::subscribe_match_all(&rpc_client, &[session.session_id_peer])
            .await
            .map_err(|err| format!("SSO disconnect monitor subscribe failed: {err}"))?;
    while let Some(item) = subscription.next().await {
        let value = item.map_err(|err| format!("SSO disconnect monitor item failed: {err}"))?;
        let page = parse_new_statements_result("sso-peer-disconnect-monitor".to_string(), &value)
            .map_err(|err| err.to_string())?;
        for statement in page.statements {
            if matches!(
                decode_sso_session_statement(
                    &session,
                    &statement,
                    "truapi:sso-peer-disconnect-monitor",
                    "truapi:sso-peer-disconnect-monitor",
                )?,
                Some(SsoSessionStatement::Disconnected)
            ) {
                return Ok(());
            }
        }
    }
    Err("SSO disconnect monitor response stream ended".to_string())
}

impl PlatformRuntimeHost {
    /// Read a stored permission authorization status without prompting.
    #[instrument(skip_all, fields(runtime.method = "permissions.authorization_status"))]
    pub(crate) async fn permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
    ) -> Result<PermissionAuthorizationStatus, v01::GenericError> {
        let service = PermissionsService::new(
            self.platform.as_ref(),
            self.platform.as_ref(),
            &self.runtime_config.product_id,
        );
        service.authorization_status(&request).await
    }

    /// Read stored permission authorization statuses without prompting.
    #[instrument(skip_all, fields(runtime.method = "permissions.authorization_statuses"))]
    pub(crate) async fn permission_authorization_statuses(
        &self,
        requests: Vec<PermissionAuthorizationRequest>,
    ) -> Result<Vec<PermissionAuthorizationStatus>, v01::GenericError> {
        let service = PermissionsService::new(
            self.platform.as_ref(),
            self.platform.as_ref(),
            &self.runtime_config.product_id,
        );
        service.authorization_statuses(&requests).await
    }

    /// Update a stored permission authorization status. `NotDetermined`
    /// clears the stored value so the next product request prompts again.
    #[instrument(skip_all, fields(runtime.method = "permissions.set_authorization_status"))]
    pub(crate) async fn set_permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus,
    ) -> Result<(), v01::GenericError> {
        let service = PermissionsService::new(
            self.platform.as_ref(),
            self.platform.as_ref(),
            &self.runtime_config.product_id,
        );
        service.set_authorization_status(&request, status).await
    }

    #[instrument(skip_all, fields(runtime.method = "permissions.chain_submit_authorization"))]
    async fn chain_submit_authorization(&self) -> Result<PermissionAuthorizationStatus, String> {
        let service = PermissionsService::new(
            self.platform.as_ref(),
            self.platform.as_ref(),
            &self.runtime_config.product_id,
        );
        service
            .check_or_prompt_remote(v01::RemotePermissionRequest {
                permission: v01::RemotePermission::ChainSubmit,
            })
            .await
            .map_err(|err| format!("permission storage failed: {err:?}"))
    }

    async fn require_chain_submit<E>(&self, denied_error: E) -> Result<(), CallError<E>> {
        match self.chain_submit_authorization().await {
            Ok(PermissionAuthorizationStatus::Authorized) => Ok(()),
            Ok(
                PermissionAuthorizationStatus::Denied
                | PermissionAuthorizationStatus::NotDetermined,
            ) => Err(CallError::Domain(denied_error)),
            Err(reason) => Err(CallError::HostFailure { reason }),
        }
    }

    /// Shared tail of the four `sign_*` methods: submit the already-built SSO
    /// message, require a `Sign` wallet response, and map its payload into the
    /// caller's concrete response variant. `wrap` adapts a sign-payload domain
    /// error into the method's error type.
    async fn submit_sign_request<E, R>(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        wrap: fn(v01::HostSignPayloadError) -> E,
        action: &str,
        message: crate::host_logic::sso::messages::RemoteMessage,
        into_response: impl FnOnce(v01::HostSignPayloadResponse) -> R,
    ) -> Result<R, CallError<E>> {
        let response = self
            .submit_sso_remote_message(cx, session, action, message)
            .await
            .map_err(|reason| signing_unknown_call_error(wrap, reason))?;
        let SsoRemoteResponse::Sign(response) = response else {
            return Err(signing_unknown_call_error(
                wrap,
                UNEXPECTED_SSO_SIGNING_RESPONSE,
            ));
        };
        response
            .payload
            .map(|payload| {
                into_response(v01::HostSignPayloadResponse {
                    signature: payload.signature,
                    signed_transaction: payload.signed_transaction,
                })
            })
            .map_err(|reason| signing_unknown_call_error(wrap, reason))
    }

    fn validate_legacy_address_signer(
        &self,
        session: &SessionInfo,
        signer: &str,
    ) -> Result<[u8; 32], v01::HostSignPayloadError> {
        let public_key = self
            .legacy_slot_zero_public_key(session)
            .map_err(|reason| v01::HostSignPayloadError::Unknown { reason })?;
        let expected = product_public_key_to_address(public_key);
        if expected == signer
            || parse_legacy_signer_hex(signer).is_some_and(|key| key == public_key)
        {
            Ok(public_key)
        } else {
            Err(v01::HostSignPayloadError::Unknown {
                reason: "Account can't be derived from product account id".to_string(),
            })
        }
    }

    fn validate_legacy_public_key_signer(
        &self,
        session: &SessionInfo,
        signer: [u8; 32],
    ) -> Result<(), v01::HostCreateTransactionError> {
        let public_key = self
            .legacy_slot_zero_public_key(session)
            .map_err(|reason| v01::HostCreateTransactionError::Unknown { reason })?;
        if public_key == signer {
            Ok(())
        } else {
            Err(v01::HostCreateTransactionError::Unknown {
                reason: "Account can't be derived from product account id".to_string(),
            })
        }
    }
}

fn parse_legacy_signer_hex(signer: &str) -> Option<[u8; 32]> {
    let raw = signer
        .strip_prefix("0x")
        .or_else(|| signer.strip_prefix("0X"))
        .unwrap_or(signer);
    if raw.len() != 64 {
        return None;
    }
    hex::decode(raw).ok()?.try_into().ok()
}

/// Adapter from `truapi_platform::ChainProvider` into the
/// [`RuntimeChainProvider`] surface the chain runtime expects.
/// Reuses the platform-supplied json-rpc connection and converts the
/// platform `GenericError` into a `RuntimeFailure::Unavailable`.
struct HostChainProvider {
    platform: Arc<dyn Platform>,
}

#[async_trait::async_trait]
impl RuntimeChainProvider for HostChainProvider {
    #[instrument(skip_all, fields(runtime.method = "chain.provider.connect"))]
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

fn runtime_failure_to_call_error<E>(failure: RuntimeFailure) -> CallError<E> {
    CallError::HostFailure {
        reason: failure.reason(),
    }
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

impl System for PlatformRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "system.feature_supported"))]
    async fn feature_supported(
        &self,
        _cx: &CallContext,
        request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, CallError<HostFeatureSupportedError>> {
        let HostFeatureSupportedRequest::V1(inner) = request;
        feature_supported(self.platform.as_ref(), inner)
            .await
            .map(HostFeatureSupportedResponse::V1)
            .map_err(|err| CallError::Domain(HostFeatureSupportedError::V1(err)))
    }

    #[instrument(skip_all, fields(runtime.method = "system.navigate_to"))]
    async fn navigate_to(
        &self,
        _cx: &CallContext,
        request: HostNavigateToRequest,
    ) -> Result<HostNavigateToResponse, CallError<HostNavigateToError>> {
        let HostNavigateToRequest::V1(v01::HostNavigateToRequest { url }) = request;
        let resolved = match parse_navigate(&url) {
            NavigateDecision::Reject { reason } => {
                return Err(CallError::Domain(HostNavigateToError::V1(
                    v01::HostNavigateToError::Unknown { reason },
                )));
            }
            decision => match decision.canonical_url() {
                Some(url) => url,
                None => {
                    return Err(CallError::HostFailure {
                        reason: "navigate decision produced no canonical URL".to_string(),
                    });
                }
            },
        };
        self.platform
            .navigate_to(resolved)
            .await
            .map(|()| HostNavigateToResponse::V1)
            .map_err(|err| CallError::Domain(HostNavigateToError::V1(err)))
    }
}

// ---------------------------------------------------------------------------
// Permissions
// ---------------------------------------------------------------------------

impl Permissions for PlatformRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "permissions.request_device_permission"))]
    async fn request_device_permission(
        &self,
        _cx: &CallContext,
        request: HostDevicePermissionRequest,
    ) -> Result<HostDevicePermissionResponse, CallError<HostDevicePermissionError>> {
        let HostDevicePermissionRequest::V1(inner) = request;
        let service = PermissionsService::new(
            self.platform.as_ref(),
            self.platform.as_ref(),
            &self.runtime_config.product_id,
        );
        match service.check_or_prompt_device(inner).await {
            Ok(decision) => Ok(HostDevicePermissionResponse::V1(
                v01::HostDevicePermissionResponse {
                    granted: decision == PermissionAuthorizationStatus::Authorized,
                },
            )),
            Err(err) => Err(CallError::HostFailure {
                reason: format!("permission storage failed: {err:?}"),
            }),
        }
    }

    #[instrument(skip_all, fields(runtime.method = "permissions.request_remote_permission"))]
    async fn request_remote_permission(
        &self,
        _cx: &CallContext,
        request: RemotePermissionRequest,
    ) -> Result<RemotePermissionResponse, CallError<RemotePermissionError>> {
        let RemotePermissionRequest::V1(inner) = request;
        let service = PermissionsService::new(
            self.platform.as_ref(),
            self.platform.as_ref(),
            &self.runtime_config.product_id,
        );
        match service.check_or_prompt_remote(inner).await {
            Ok(decision) => Ok(RemotePermissionResponse::V1(
                v01::RemotePermissionResponse {
                    granted: decision == PermissionAuthorizationStatus::Authorized,
                },
            )),
            Err(err) => Err(CallError::HostFailure {
                reason: format!("permission storage failed: {err:?}"),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// LocalStorage
// ---------------------------------------------------------------------------

impl LocalStorage for PlatformRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "local_storage.read"))]
    async fn read(
        &self,
        _cx: &CallContext,
        request: HostLocalStorageReadRequest,
    ) -> Result<HostLocalStorageReadResponse, CallError<HostLocalStorageReadError>> {
        let HostLocalStorageReadRequest::V1(v01::HostLocalStorageReadRequest { key }) = request;
        self.platform
            .read(key)
            .await
            .map(|value| {
                HostLocalStorageReadResponse::V1(v01::HostLocalStorageReadResponse { value })
            })
            .map_err(|err| CallError::Domain(HostLocalStorageReadError::V1(err)))
    }

    #[instrument(skip_all, fields(runtime.method = "local_storage.write"))]
    async fn write(
        &self,
        _cx: &CallContext,
        request: HostLocalStorageWriteRequest,
    ) -> Result<HostLocalStorageWriteResponse, CallError<HostLocalStorageWriteError>> {
        let HostLocalStorageWriteRequest::V1(v01::HostLocalStorageWriteRequest { key, value }) =
            request;
        self.platform
            .write(key, value)
            .await
            .map(|()| HostLocalStorageWriteResponse::V1)
            .map_err(|err| CallError::Domain(HostLocalStorageWriteError::V1(err)))
    }

    #[instrument(skip_all, fields(runtime.method = "local_storage.clear"))]
    async fn clear(
        &self,
        _cx: &CallContext,
        request: HostLocalStorageClearRequest,
    ) -> Result<HostLocalStorageClearResponse, CallError<HostLocalStorageClearError>> {
        let HostLocalStorageClearRequest::V1(v01::HostLocalStorageClearRequest { key }) = request;
        self.platform
            .clear(key)
            .await
            .map(|()| HostLocalStorageClearResponse::V1)
            .map_err(|err| CallError::Domain(HostLocalStorageClearError::V1(err)))
    }
}

// ---------------------------------------------------------------------------
// Account
// ---------------------------------------------------------------------------
//
// Account-management flows live in the Rust core itself, backed by the shared
// session state and, for alias/proof/login success paths, the SSO service.

impl Account for PlatformRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "account.get_account"))]
    async fn get_account(
        &self,
        _cx: &CallContext,
        request: HostAccountGetRequest,
    ) -> Result<HostAccountGetResponse, CallError<HostAccountGetError>> {
        let HostAccountGetRequest::V1(v01::HostAccountGetRequest { product_account_id }) = request;
        let product_account_id = Self::normalize_product_account_id(product_account_id);

        if !is_product_identifier(&product_account_id.dot_ns_identifier) {
            return Err(CallError::Domain(HostAccountGetError::V1(
                v01::HostAccountGetError::DomainNotValid,
            )));
        }

        let Some(session) = self.session_state.current() else {
            return Err(CallError::Domain(HostAccountGetError::V1(
                v01::HostAccountGetError::NotConnected,
            )));
        };

        let public_key = derive_product_public_key(
            session.public_key,
            &product_account_id.dot_ns_identifier,
            product_account_id.derivation_index,
        )
        .map_err(|err| {
            CallError::Domain(HostAccountGetError::V1(v01::HostAccountGetError::Unknown {
                reason: err.to_string(),
            }))
        })?;

        Ok(HostAccountGetResponse::V1(v01::HostAccountGetResponse {
            account: v01::ProductAccount {
                public_key: public_key.to_vec(),
            },
        }))
    }

    #[instrument(skip_all, fields(runtime.method = "account.get_account_alias"))]
    async fn get_account_alias(
        &self,
        cx: &CallContext,
        request: HostAccountGetAliasRequest,
    ) -> Result<HostAccountGetAliasResponse, CallError<HostAccountGetAliasError>> {
        let HostAccountGetAliasRequest::V1(v01::HostAccountGetAliasRequest { product_account_id }) =
            request;
        let product_account_id = Self::normalize_product_account_id(product_account_id);

        let Some(session) = self.session_state.current() else {
            return Err(CallError::Domain(HostAccountGetAliasError::V1(
                v01::HostAccountGetError::NotConnected,
            )));
        };

        if !is_product_identifier(&product_account_id.dot_ns_identifier) {
            return Err(CallError::Domain(HostAccountGetAliasError::V1(
                v01::HostAccountGetError::DomainNotValid,
            )));
        }

        let product_id = self.product_id();
        if product_account_id.dot_ns_identifier != product_id {
            let confirmed = self
                .platform
                .confirm_user_action(UserConfirmationReview::AccountAlias(AccountAliasReview {
                    requesting_product_id: product_id.clone(),
                    target_product_id: product_account_id.dot_ns_identifier.clone(),
                }))
                .await
                .map_err(|err| CallError::HostFailure {
                    reason: format!("account alias confirmation failed: {err:?}"),
                })?;
            if !confirmed {
                return Err(CallError::Domain(HostAccountGetAliasError::V1(
                    v01::HostAccountGetError::Rejected,
                )));
            }
        }

        let message_id = sso_message_id(cx, "account-alias");
        let message = alias_request_message(message_id.clone(), product_account_id, product_id);
        let response = self
            .submit_sso_remote_message_without_timeout(cx, &session, "account-alias", message)
            .await
            .map_err(|reason| {
                CallError::Domain(HostAccountGetAliasError::V1(
                    v01::HostAccountGetError::Unknown { reason },
                ))
            })?;
        let SsoRemoteResponse::RingVrfAlias(response) = response else {
            return Err(CallError::Domain(HostAccountGetAliasError::V1(
                v01::HostAccountGetError::Unknown {
                    reason: "Unexpected SSO response for account alias request".to_string(),
                },
            )));
        };
        response
            .payload
            .map(HostAccountGetAliasResponse::V1)
            .map_err(|reason| {
                CallError::Domain(HostAccountGetAliasError::V1(
                    v01::HostAccountGetError::Unknown { reason },
                ))
            })
    }

    #[instrument(skip_all, fields(runtime.method = "account.create_account_proof"))]
    async fn create_account_proof(
        &self,
        _cx: &CallContext,
        _request: HostAccountCreateProofRequest,
    ) -> Result<HostAccountCreateProofResponse, CallError<HostAccountCreateProofError>> {
        Err(CallError::Unsupported)
    }

    #[instrument(skip_all, fields(runtime.method = "account.get_legacy_accounts"))]
    async fn get_legacy_accounts(
        &self,
        _cx: &CallContext,
        _request: HostGetLegacyAccountsRequest,
    ) -> Result<HostGetLegacyAccountsResponse, CallError<HostGetLegacyAccountsError>> {
        let Some(session) = self.session_state.current() else {
            return Ok(HostGetLegacyAccountsResponse::V1(
                v01::HostGetLegacyAccountsResponse { accounts: vec![] },
            ));
        };

        let product_id = self.product_id();
        if !is_product_identifier(&product_id) {
            return Err(CallError::Domain(HostGetLegacyAccountsError::V1(
                v01::HostAccountGetError::DomainNotValid,
            )));
        }

        let public_key =
            derive_product_public_key(session.public_key, &product_id, 0).map_err(|err| {
                CallError::Domain(HostGetLegacyAccountsError::V1(
                    v01::HostAccountGetError::Unknown {
                        reason: err.to_string(),
                    },
                ))
            })?;

        Ok(HostGetLegacyAccountsResponse::V1(
            v01::HostGetLegacyAccountsResponse {
                accounts: vec![v01::LegacyAccount {
                    public_key: public_key.to_vec(),
                    name: session.lite_username,
                }],
            },
        ))
    }

    #[instrument(skip_all, fields(runtime.method = "account.get_user_id"))]
    async fn get_user_id(
        &self,
        _cx: &CallContext,
        _request: HostGetUserIdRequest,
    ) -> Result<HostGetUserIdResponse, CallError<HostGetUserIdError>> {
        let Some(session) = self.session_state.current() else {
            return Err(CallError::Domain(HostGetUserIdError::V1(
                v01::HostGetUserIdError::NotConnected,
            )));
        };

        let primary_username = session
            .full_username
            .filter(|value| !value.is_empty())
            .or_else(|| session.lite_username.filter(|value| !value.is_empty()))
            .ok_or_else(|| {
                CallError::Domain(HostGetUserIdError::V1(v01::HostGetUserIdError::Unknown {
                    reason: "No primary username for this session".to_string(),
                }))
            })?;

        Ok(HostGetUserIdResponse::V1(v01::HostGetUserIdResponse {
            primary_username,
        }))
    }

    #[instrument(skip_all, fields(runtime.method = "account.connection_status_subscribe"))]
    async fn connection_status_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<HostAccountConnectionStatusSubscribeItem> {
        Subscription::new(self.session_state.subscribe())
    }

    #[instrument(skip_all, fields(runtime.method = "account.request_login", product = %self.runtime_config.product_id))]
    async fn request_login(
        &self,
        _cx: &CallContext,
        _request: HostRequestLoginRequest,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        self.request_login_flow().await
    }
}

/// Host-UI projection of an active session for `AuthState::Connected`.
fn connected_session_ui_info(session: &SessionInfo) -> SessionUiInfo {
    SessionUiInfo {
        public_key: session.public_key,
        identity_account_id: session.identity_account_id,
        lite_username: session.lite_username.clone(),
        full_username: session.full_username.clone(),
    }
}

const UNEXPECTED_SSO_SIGNING_RESPONSE: &str = "Unexpected SSO response for signing request";
const UNEXPECTED_SSO_TRANSACTION_RESPONSE: &str = "Unexpected SSO response for transaction request";

fn signing_unknown_call_error<E>(
    wrap: fn(v01::HostSignPayloadError) -> E,
    reason: impl Into<String>,
) -> CallError<E> {
    CallError::Domain(wrap(v01::HostSignPayloadError::Unknown {
        reason: reason.into(),
    }))
}

fn transaction_unknown_call_error<E>(
    wrap: fn(v01::HostCreateTransactionError) -> E,
    reason: impl Into<String>,
) -> CallError<E> {
    CallError::Domain(wrap(v01::HostCreateTransactionError::Unknown {
        reason: reason.into(),
    }))
}

impl Signing for PlatformRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "signing.sign_payload"))]
    async fn sign_payload(
        &self,
        cx: &CallContext,
        request: HostSignPayloadRequest,
    ) -> Result<HostSignPayloadResponse, CallError<HostSignPayloadError>> {
        info!("sign_payload: requesting wallet signature");
        let HostSignPayloadRequest::V1(mut inner) = request;
        inner.account = Self::normalize_product_account_id(inner.account);
        if !self.is_product_account_valid_for_caller(&inner.account.dot_ns_identifier) {
            return Err(CallError::Domain(HostSignPayloadError::V1(
                v01::HostSignPayloadError::PermissionDenied,
            )));
        }
        self.require_chain_submit(HostSignPayloadError::V1(
            v01::HostSignPayloadError::PermissionDenied,
        ))
        .await?;
        let Some(session) = self.session_state.current() else {
            return Err(CallError::Domain(HostSignPayloadError::V1(
                v01::HostSignPayloadError::Rejected,
            )));
        };
        let confirmed = self
            .platform
            .confirm_user_action(UserConfirmationReview::SignPayload(
                SignPayloadReview::Product(inner.clone()),
            ))
            .await
            .map_err(|err| CallError::HostFailure {
                reason: format!("sign payload confirmation failed: {err:?}"),
            })?;
        if !confirmed {
            return Err(CallError::Domain(HostSignPayloadError::V1(
                v01::HostSignPayloadError::Rejected,
            )));
        }
        let message_id = sso_message_id(cx, "sign-payload");
        let message = sign_payload_message(message_id, inner);
        self.submit_sign_request(
            cx,
            &session,
            HostSignPayloadError::V1,
            "sign-payload",
            message,
            HostSignPayloadResponse::V1,
        )
        .await
    }

    #[instrument(skip_all, fields(runtime.method = "signing.sign_raw"))]
    async fn sign_raw(
        &self,
        cx: &CallContext,
        request: HostSignRawRequest,
    ) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
        info!("sign_raw: requesting wallet signature");
        let HostSignRawRequest::V1(mut inner) = request;
        inner.account = Self::normalize_product_account_id(inner.account);
        if !self.is_product_account_valid_for_caller(&inner.account.dot_ns_identifier) {
            return Err(CallError::Domain(HostSignRawError::V1(
                v01::HostSignPayloadError::PermissionDenied,
            )));
        }
        self.require_chain_submit(HostSignRawError::V1(
            v01::HostSignPayloadError::PermissionDenied,
        ))
        .await?;
        let Some(session) = self.session_state.current() else {
            return Err(CallError::Domain(HostSignRawError::V1(
                v01::HostSignPayloadError::Rejected,
            )));
        };
        let confirmed = self
            .platform
            .confirm_user_action(UserConfirmationReview::SignRaw(SignRawReview::Product(
                inner.clone(),
            )))
            .await
            .map_err(|err| CallError::HostFailure {
                reason: format!("sign raw confirmation failed: {err:?}"),
            })?;
        if !confirmed {
            return Err(CallError::Domain(HostSignRawError::V1(
                v01::HostSignPayloadError::Rejected,
            )));
        }
        let message_id = sso_message_id(cx, "sign-raw");
        let message = sign_raw_message(message_id, inner);
        self.submit_sign_request(
            cx,
            &session,
            HostSignRawError::V1,
            "sign-raw",
            message,
            HostSignRawResponse::V1,
        )
        .await
    }

    #[instrument(skip_all, fields(runtime.method = "signing.create_transaction"))]
    async fn create_transaction(
        &self,
        cx: &CallContext,
        request: HostCreateTransactionRequest,
    ) -> Result<HostCreateTransactionResponse, CallError<HostCreateTransactionError>> {
        info!("create_transaction: requesting wallet signature");
        let HostCreateTransactionRequest::V1(mut inner) = request;
        inner.signer = Self::normalize_product_account_id(inner.signer);
        if !self.is_product_account_valid_for_caller(&inner.signer.dot_ns_identifier) {
            return Err(CallError::Domain(HostCreateTransactionError::V1(
                v01::HostCreateTransactionError::PermissionDenied,
            )));
        }
        self.require_chain_submit(HostCreateTransactionError::V1(
            v01::HostCreateTransactionError::PermissionDenied,
        ))
        .await?;
        let Some(session) = self.session_state.current() else {
            return Err(CallError::Domain(HostCreateTransactionError::V1(
                v01::HostCreateTransactionError::Rejected,
            )));
        };
        if is_ios_diagnosis_e2e() {
            return Ok(HostCreateTransactionResponse::V1(
                v01::HostCreateTransactionResponse {
                    transaction: vec![0x84, 0x00, 0x00],
                },
            ));
        }
        let confirmed = self
            .platform
            .confirm_user_action(UserConfirmationReview::CreateTransaction(
                CreateTransactionReview::Product(inner.clone()),
            ))
            .await
            .map_err(|err| CallError::HostFailure {
                reason: format!("create transaction confirmation failed: {err:?}"),
            })?;
        if !confirmed {
            return Err(CallError::Domain(HostCreateTransactionError::V1(
                v01::HostCreateTransactionError::Rejected,
            )));
        }
        let message_id = sso_message_id(cx, "create-transaction");
        let message = create_transaction_message(message_id, inner);
        let response = self
            .submit_sso_remote_message(cx, &session, "create-transaction", message)
            .await
            .map_err(|reason| {
                transaction_unknown_call_error(HostCreateTransactionError::V1, reason)
            })?;
        let SsoRemoteResponse::CreateTransaction(response) = response else {
            return Err(transaction_unknown_call_error(
                HostCreateTransactionError::V1,
                UNEXPECTED_SSO_TRANSACTION_RESPONSE,
            ));
        };
        response
            .signed_transaction
            .map(|transaction| {
                HostCreateTransactionResponse::V1(v01::HostCreateTransactionResponse {
                    transaction,
                })
            })
            .map_err(|reason| {
                transaction_unknown_call_error(HostCreateTransactionError::V1, reason)
            })
    }

    #[instrument(skip_all, fields(runtime.method = "signing.sign_payload_with_legacy_account"))]
    async fn sign_payload_with_legacy_account(
        &self,
        cx: &CallContext,
        request: HostSignPayloadWithLegacyAccountRequest,
    ) -> Result<
        HostSignPayloadWithLegacyAccountResponse,
        CallError<HostSignPayloadWithLegacyAccountError>,
    > {
        let HostSignPayloadWithLegacyAccountRequest::V1(inner) = request;
        let Some(session) = self.session_state.current() else {
            return Err(CallError::Domain(
                HostSignPayloadWithLegacyAccountError::V1(v01::HostSignPayloadError::Rejected),
            ));
        };
        self.validate_legacy_address_signer(&session, &inner.signer)
            .map_err(|err| CallError::Domain(HostSignPayloadWithLegacyAccountError::V1(err)))?;
        self.require_chain_submit(HostSignPayloadWithLegacyAccountError::V1(
            v01::HostSignPayloadError::PermissionDenied,
        ))
        .await?;
        let confirmed = self
            .platform
            .confirm_user_action(UserConfirmationReview::SignPayload(
                SignPayloadReview::LegacyAccount(inner.clone()),
            ))
            .await
            .map_err(|err| CallError::HostFailure {
                reason: format!("sign payload confirmation failed: {err:?}"),
            })?;
        if !confirmed {
            return Err(CallError::Domain(
                HostSignPayloadWithLegacyAccountError::V1(v01::HostSignPayloadError::Rejected),
            ));
        }
        let message_id = sso_message_id(cx, "legacy-sign-payload");
        let message = sign_payload_message(
            message_id,
            v01::HostSignPayloadRequest {
                account: v01::ProductAccountId {
                    dot_ns_identifier: self.product_id(),
                    derivation_index: 0,
                },
                payload: inner.payload,
            },
        );
        self.submit_sign_request(
            cx,
            &session,
            HostSignPayloadWithLegacyAccountError::V1,
            "legacy-sign-payload",
            message,
            HostSignPayloadWithLegacyAccountResponse::V1,
        )
        .await
    }

    #[instrument(skip_all, fields(runtime.method = "signing.sign_raw_with_legacy_account"))]
    async fn sign_raw_with_legacy_account(
        &self,
        cx: &CallContext,
        request: HostSignRawWithLegacyAccountRequest,
    ) -> Result<HostSignRawWithLegacyAccountResponse, CallError<HostSignRawWithLegacyAccountError>>
    {
        let HostSignRawWithLegacyAccountRequest::V1(inner) = request;
        let Some(session) = self.session_state.current() else {
            return Err(CallError::Domain(HostSignRawWithLegacyAccountError::V1(
                v01::HostSignPayloadError::Rejected,
            )));
        };
        self.validate_legacy_address_signer(&session, &inner.signer)
            .map_err(|err| CallError::Domain(HostSignRawWithLegacyAccountError::V1(err)))?;
        self.require_chain_submit(HostSignRawWithLegacyAccountError::V1(
            v01::HostSignPayloadError::PermissionDenied,
        ))
        .await?;
        let confirmed = self
            .platform
            .confirm_user_action(UserConfirmationReview::SignRaw(
                SignRawReview::LegacyAccount(inner.clone()),
            ))
            .await
            .map_err(|err| CallError::HostFailure {
                reason: format!("sign raw confirmation failed: {err:?}"),
            })?;
        if !confirmed {
            return Err(CallError::Domain(HostSignRawWithLegacyAccountError::V1(
                v01::HostSignPayloadError::Rejected,
            )));
        }
        let message_id = sso_message_id(cx, "legacy-sign-raw");
        let message = sign_raw_message(
            message_id,
            v01::HostSignRawRequest {
                account: v01::ProductAccountId {
                    dot_ns_identifier: self.product_id(),
                    derivation_index: 0,
                },
                payload: inner.payload,
            },
        );
        self.submit_sign_request(
            cx,
            &session,
            HostSignRawWithLegacyAccountError::V1,
            "legacy-sign-raw",
            message,
            HostSignRawWithLegacyAccountResponse::V1,
        )
        .await
    }

    #[instrument(skip_all, fields(runtime.method = "signing.create_transaction_with_legacy_account"))]
    async fn create_transaction_with_legacy_account(
        &self,
        cx: &CallContext,
        request: HostCreateTransactionWithLegacyAccountRequest,
    ) -> Result<
        HostCreateTransactionWithLegacyAccountResponse,
        CallError<HostCreateTransactionWithLegacyAccountError>,
    > {
        let HostCreateTransactionWithLegacyAccountRequest::V1(inner) = request;
        let Some(session) = self.session_state.current() else {
            return Err(CallError::Domain(
                HostCreateTransactionWithLegacyAccountError::V1(
                    v01::HostCreateTransactionError::Rejected,
                ),
            ));
        };
        self.validate_legacy_public_key_signer(&session, inner.signer)
            .map_err(|err| {
                CallError::Domain(HostCreateTransactionWithLegacyAccountError::V1(err))
            })?;
        self.require_chain_submit(HostCreateTransactionWithLegacyAccountError::V1(
            v01::HostCreateTransactionError::PermissionDenied,
        ))
        .await?;
        let confirmed = self
            .platform
            .confirm_user_action(UserConfirmationReview::CreateTransaction(
                CreateTransactionReview::LegacyAccount(inner.clone()),
            ))
            .await
            .map_err(|err| CallError::HostFailure {
                reason: format!("create transaction confirmation failed: {err:?}"),
            })?;
        if !confirmed {
            return Err(CallError::Domain(
                HostCreateTransactionWithLegacyAccountError::V1(
                    v01::HostCreateTransactionError::Rejected,
                ),
            ));
        }
        let message_id = sso_message_id(cx, "legacy-create-transaction");
        let message = create_transaction_message(
            message_id,
            v01::ProductAccountTxPayload {
                signer: v01::ProductAccountId {
                    dot_ns_identifier: self.product_id(),
                    derivation_index: 0,
                },
                genesis_hash: inner.genesis_hash,
                call_data: inner.call_data,
                extensions: inner.extensions,
                tx_ext_version: inner.tx_ext_version,
            },
        );
        let response = self
            .submit_sso_remote_message(cx, &session, "legacy-create-transaction", message)
            .await
            .map_err(|reason| {
                transaction_unknown_call_error(
                    HostCreateTransactionWithLegacyAccountError::V1,
                    reason,
                )
            })?;
        let SsoRemoteResponse::CreateTransaction(response) = response else {
            return Err(transaction_unknown_call_error(
                HostCreateTransactionWithLegacyAccountError::V1,
                UNEXPECTED_SSO_TRANSACTION_RESPONSE,
            ));
        };
        response
            .signed_transaction
            .map(|transaction| {
                HostCreateTransactionWithLegacyAccountResponse::V1(
                    v01::HostCreateTransactionWithLegacyAccountResponse { transaction },
                )
            })
            .map_err(|reason| {
                transaction_unknown_call_error(
                    HostCreateTransactionWithLegacyAccountError::V1,
                    reason,
                )
            })
    }
}

// ---------------------------------------------------------------------------
// Chain
// ---------------------------------------------------------------------------
//
// The chain surface is backed by `ChainRuntime`, which keeps one
// `chainHead_v1` connection per genesis hash on top of the platform-supplied
// `ChainProvider::connect`. Requests go through `request_value` and parse
// json-rpc responses into typed v01 results; follow notifications are
// translated into `RemoteChainHeadFollowItem` items on the subscription
// stream.

impl Chain for PlatformRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "chain.follow_head_subscribe"))]
    async fn follow_head_subscribe(
        &self,
        cx: &CallContext,
        request: RemoteChainHeadFollowRequest,
    ) -> Subscription<RemoteChainHeadFollowItem> {
        let RemoteChainHeadFollowRequest::V1(inner) = request;
        let follow_subscription_id = cx.request_id().to_string();
        let stream = self
            .chain
            .remote_chain_head_follow(follow_subscription_id, inner)
            .map(RemoteChainHeadFollowItem::V1);
        Subscription::new(Box::pin(stream))
    }

    #[instrument(skip_all, fields(runtime.method = "chain.get_head_header"))]
    async fn get_head_header(
        &self,
        _cx: &CallContext,
        request: RemoteChainHeadHeaderRequest,
    ) -> Result<RemoteChainHeadHeaderResponse, CallError<RemoteChainHeadHeaderError>> {
        let RemoteChainHeadHeaderRequest::V1(inner) = request;
        self.chain
            .remote_chain_head_header(inner)
            .await
            .map(RemoteChainHeadHeaderResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.get_head_body"))]
    async fn get_head_body(
        &self,
        _cx: &CallContext,
        request: RemoteChainHeadBodyRequest,
    ) -> Result<RemoteChainHeadBodyResponse, CallError<RemoteChainHeadBodyError>> {
        let RemoteChainHeadBodyRequest::V1(inner) = request;
        self.chain
            .remote_chain_head_body(inner)
            .await
            .map(RemoteChainHeadBodyResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.get_head_storage"))]
    async fn get_head_storage(
        &self,
        _cx: &CallContext,
        request: RemoteChainHeadStorageRequest,
    ) -> Result<RemoteChainHeadStorageResponse, CallError<RemoteChainHeadStorageError>> {
        let RemoteChainHeadStorageRequest::V1(inner) = request;
        self.chain
            .remote_chain_head_storage(inner)
            .await
            .map(RemoteChainHeadStorageResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.call_head"))]
    async fn call_head(
        &self,
        _cx: &CallContext,
        request: RemoteChainHeadCallRequest,
    ) -> Result<RemoteChainHeadCallResponse, CallError<RemoteChainHeadCallError>> {
        let RemoteChainHeadCallRequest::V1(inner) = request;
        self.chain
            .remote_chain_head_call(inner)
            .await
            .map(RemoteChainHeadCallResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.unpin_head"))]
    async fn unpin_head(
        &self,
        _cx: &CallContext,
        request: RemoteChainHeadUnpinRequest,
    ) -> Result<RemoteChainHeadUnpinResponse, CallError<RemoteChainHeadUnpinError>> {
        let RemoteChainHeadUnpinRequest::V1(inner) = request;
        self.chain
            .remote_chain_head_unpin(inner)
            .await
            .map(|()| RemoteChainHeadUnpinResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.continue_head"))]
    async fn continue_head(
        &self,
        _cx: &CallContext,
        request: RemoteChainHeadContinueRequest,
    ) -> Result<RemoteChainHeadContinueResponse, CallError<RemoteChainHeadContinueError>> {
        let RemoteChainHeadContinueRequest::V1(inner) = request;
        self.chain
            .remote_chain_head_continue(inner)
            .await
            .map(|()| RemoteChainHeadContinueResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.stop_head_operation"))]
    async fn stop_head_operation(
        &self,
        _cx: &CallContext,
        request: RemoteChainHeadStopOperationRequest,
    ) -> Result<RemoteChainHeadStopOperationResponse, CallError<RemoteChainHeadStopOperationError>>
    {
        let RemoteChainHeadStopOperationRequest::V1(inner) = request;
        self.chain
            .remote_chain_head_stop_operation(inner)
            .await
            .map(|()| RemoteChainHeadStopOperationResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.get_spec_genesis_hash"))]
    async fn get_spec_genesis_hash(
        &self,
        _cx: &CallContext,
        request: RemoteChainSpecGenesisHashRequest,
    ) -> Result<RemoteChainSpecGenesisHashResponse, CallError<RemoteChainSpecGenesisHashError>>
    {
        let RemoteChainSpecGenesisHashRequest::V1(inner) = request;
        self.chain
            .remote_chain_spec_genesis_hash(inner.genesis_hash)
            .await
            .map(RemoteChainSpecGenesisHashResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.get_spec_chain_name"))]
    async fn get_spec_chain_name(
        &self,
        _cx: &CallContext,
        request: RemoteChainSpecChainNameRequest,
    ) -> Result<RemoteChainSpecChainNameResponse, CallError<RemoteChainSpecChainNameError>> {
        let RemoteChainSpecChainNameRequest::V1(inner) = request;
        self.chain
            .remote_chain_spec_chain_name(inner.genesis_hash)
            .await
            .map(RemoteChainSpecChainNameResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.get_spec_properties"))]
    async fn get_spec_properties(
        &self,
        _cx: &CallContext,
        request: RemoteChainSpecPropertiesRequest,
    ) -> Result<RemoteChainSpecPropertiesResponse, CallError<RemoteChainSpecPropertiesError>> {
        let RemoteChainSpecPropertiesRequest::V1(inner) = request;
        self.chain
            .remote_chain_spec_properties(inner.genesis_hash)
            .await
            .map(RemoteChainSpecPropertiesResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.broadcast_transaction"))]
    async fn broadcast_transaction(
        &self,
        _cx: &CallContext,
        request: RemoteChainTransactionBroadcastRequest,
    ) -> Result<
        RemoteChainTransactionBroadcastResponse,
        CallError<RemoteChainTransactionBroadcastError>,
    > {
        let RemoteChainTransactionBroadcastRequest::V1(inner) = request;
        self.chain
            .remote_chain_transaction_broadcast(inner)
            .await
            .map(RemoteChainTransactionBroadcastResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }

    #[instrument(skip_all, fields(runtime.method = "chain.stop_transaction"))]
    async fn stop_transaction(
        &self,
        _cx: &CallContext,
        request: RemoteChainTransactionStopRequest,
    ) -> Result<RemoteChainTransactionStopResponse, CallError<RemoteChainTransactionStopError>>
    {
        let RemoteChainTransactionStopRequest::V1(inner) = request;
        self.chain
            .remote_chain_transaction_stop(inner)
            .await
            .map(|()| RemoteChainTransactionStopResponse::V1)
            .map_err(runtime_failure_to_call_error)
    }
}

// ---------------------------------------------------------------------------
// Deferred product surfaces.
//
// Payment and full account proof are explicitly out of current dotli parity,
// but products should still observe dotli's typed "not implemented" errors
// rather than a generic transport failure.
// Chat and CoinPayment remain outside this milestone and keep their generated
// trait defaults until another host/product needs real implementations.

const PAYMENTS_NOT_IMPLEMENTED: &str = "Payments are not supported in dot.li";

fn is_ios_diagnosis_e2e() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var("TRUAPI_IOS_E2E_AUTORUN_DIAGNOSIS")
            .ok()
            .as_deref()
            == Some("1")
    }
    #[cfg(target_arch = "wasm32")]
    {
        false
    }
}

fn single_item_subscription<T>(item: T) -> Subscription<T>
where
    T: Send + 'static,
{
    Subscription::new(Box::pin(
        stream::once(async move { item }).chain(stream::pending()),
    ))
}

impl Chat for PlatformRuntimeHost {}
impl CoinPayment for PlatformRuntimeHost {}
impl Payment for PlatformRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "payment.balance_subscribe"))]
    async fn balance_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPaymentBalanceSubscribeRequest,
    ) -> Result<
        Subscription<HostPaymentBalanceSubscribeItem>,
        CallError<HostPaymentBalanceSubscribeError>,
    > {
        if is_ios_diagnosis_e2e() {
            return Ok(single_item_subscription(
                HostPaymentBalanceSubscribeItem::V1(v01::HostPaymentBalanceSubscribeItem {
                    available: 1_000_000,
                }),
            ));
        }

        Err(CallError::Domain(HostPaymentBalanceSubscribeError::V1(
            v01::HostPaymentBalanceSubscribeError::PermissionDenied,
        )))
    }

    #[instrument(skip_all, fields(runtime.method = "payment.request"))]
    async fn request(
        &self,
        _cx: &CallContext,
        _request: HostPaymentRequest,
    ) -> Result<HostPaymentResponse, CallError<HostPaymentError>> {
        if is_ios_diagnosis_e2e() {
            return Ok(HostPaymentResponse::V1(v01::HostPaymentResponse {
                id: "e2e-payment".to_string(),
            }));
        }

        Err(CallError::Domain(HostPaymentError::V1(
            v01::HostPaymentError::Unknown {
                reason: PAYMENTS_NOT_IMPLEMENTED.to_string(),
            },
        )))
    }

    #[instrument(skip_all, fields(runtime.method = "payment.status_subscribe"))]
    async fn status_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPaymentStatusSubscribeRequest,
    ) -> Result<
        Subscription<HostPaymentStatusSubscribeItem>,
        CallError<HostPaymentStatusSubscribeError>,
    > {
        if is_ios_diagnosis_e2e() {
            return Ok(single_item_subscription(
                HostPaymentStatusSubscribeItem::V1(v01::HostPaymentStatusSubscribeItem::Completed),
            ));
        }

        Err(CallError::Domain(HostPaymentStatusSubscribeError::V1(
            v01::HostPaymentStatusSubscribeError::Unknown {
                reason: PAYMENTS_NOT_IMPLEMENTED.to_string(),
            },
        )))
    }

    #[instrument(skip_all, fields(runtime.method = "payment.top_up"))]
    async fn top_up(
        &self,
        _cx: &CallContext,
        _request: HostPaymentTopUpRequest,
    ) -> Result<HostPaymentTopUpResponse, CallError<HostPaymentTopUpError>> {
        if is_ios_diagnosis_e2e() {
            return Ok(HostPaymentTopUpResponse::V1);
        }

        Err(CallError::Domain(HostPaymentTopUpError::V1(
            v01::HostPaymentTopUpError::Unknown {
                reason: PAYMENTS_NOT_IMPLEMENTED.to_string(),
            },
        )))
    }
}

impl ResourceAllocation for PlatformRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "resource_allocation.request"))]
    async fn request(
        &self,
        cx: &CallContext,
        request: HostRequestResourceAllocationRequest,
    ) -> Result<HostRequestResourceAllocationResponse, CallError<HostRequestResourceAllocationError>>
    {
        let HostRequestResourceAllocationRequest::V1(inner) = request;
        let Some(session) = self.session_state.current() else {
            return Err(CallError::Domain(HostRequestResourceAllocationError::V1(
                v01::ResourceAllocationError::Unknown {
                    reason: "No active session".to_string(),
                },
            )));
        };

        let confirmed = self
            .platform
            .confirm_user_action(UserConfirmationReview::ResourceAllocation(inner.clone()))
            .await
            .map_err(|err| CallError::HostFailure {
                reason: format!("resource allocation confirmation failed: {err:?}"),
            })?;
        if !confirmed {
            return Err(CallError::Domain(HostRequestResourceAllocationError::V1(
                v01::ResourceAllocationError::Unknown {
                    reason: "User rejected resource allocation".to_string(),
                },
            )));
        }
        let message_id = sso_message_id(cx, "resource-allocation");
        let message = resource_allocation_message(
            message_id,
            self.product_id(),
            inner.resources,
            OnExistingAllowancePolicy::Increase,
        );
        let response = self
            .submit_sso_remote_message(cx, &session, "resource-allocation", message)
            .await
            .map_err(|reason| {
                CallError::Domain(HostRequestResourceAllocationError::V1(
                    v01::ResourceAllocationError::Unknown { reason },
                ))
            })?;
        let SsoRemoteResponse::ResourceAllocation(response) = response else {
            return Err(CallError::Domain(HostRequestResourceAllocationError::V1(
                v01::ResourceAllocationError::Unknown {
                    reason: "Unexpected SSO response for resource allocation request".to_string(),
                },
            )));
        };
        response
            .payload
            .map(|outcomes| {
                HostRequestResourceAllocationResponse::V1(
                    v01::HostRequestResourceAllocationResponse {
                        outcomes: outcomes
                            .into_iter()
                            .map(resource_allocation_outcome)
                            .collect(),
                    },
                )
            })
            .map_err(|reason| {
                CallError::Domain(HostRequestResourceAllocationError::V1(
                    v01::ResourceAllocationError::Unknown { reason },
                ))
            })
    }
}

fn resource_allocation_outcome(outcome: SsoAllocationOutcome) -> v01::AllocationOutcome {
    match outcome {
        SsoAllocationOutcome::Allocated(_) => v01::AllocationOutcome::Allocated,
        SsoAllocationOutcome::Rejected => v01::AllocationOutcome::Rejected,
        SsoAllocationOutcome::NotAvailable => v01::AllocationOutcome::NotAvailable,
    }
}
// ---------------------------------------------------------------------------
// Entropy
// ---------------------------------------------------------------------------

impl Entropy for PlatformRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "entropy.derive"))]
    async fn derive(
        &self,
        _cx: &CallContext,
        request: HostDeriveEntropyRequest,
    ) -> Result<HostDeriveEntropyResponse, CallError<HostDeriveEntropyError>> {
        let HostDeriveEntropyRequest::V1(v01::HostDeriveEntropyRequest { context }) = request;
        let Some(session) = self.session_state.current() else {
            return Err(CallError::Domain(HostDeriveEntropyError::V1(
                v01::HostDeriveEntropyError::Unknown {
                    reason: "Not connected".to_string(),
                },
            )));
        };
        let Some(root_entropy_source) = session.root_entropy_source else {
            return Err(CallError::Domain(HostDeriveEntropyError::V1(
                v01::HostDeriveEntropyError::Unknown {
                    reason: "Session secret missing".to_string(),
                },
            )));
        };

        let entropy =
            derive_product_entropy_from_source(&root_entropy_source, &self.product_id(), &context)
                .map_err(|err| {
                    CallError::Domain(HostDeriveEntropyError::V1(
                        v01::HostDeriveEntropyError::Unknown {
                            reason: err.to_string(),
                        },
                    ))
                })?;

        Ok(HostDeriveEntropyResponse::V1(
            v01::HostDeriveEntropyResponse { entropy },
        ))
    }
}

// ---------------------------------------------------------------------------
// Preimage
// ---------------------------------------------------------------------------

impl Preimage for PlatformRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "preimage.lookup_subscribe"))]
    async fn lookup_subscribe(
        &self,
        _cx: &CallContext,
        request: RemotePreimageLookupSubscribeRequest,
    ) -> Subscription<RemotePreimageLookupSubscribeItem> {
        let RemotePreimageLookupSubscribeRequest::V1(v01::RemotePreimageLookupSubscribeRequest {
            key,
        }) = request;
        let stream = self
            .platform
            .lookup_preimage(key)
            .filter_map(|item| async move {
                item.ok().map(|value| {
                    RemotePreimageLookupSubscribeItem::V1(v01::RemotePreimageLookupSubscribeItem {
                        value,
                    })
                })
            });
        Subscription::new(Box::pin(stream))
    }

    #[instrument(skip_all, fields(runtime.method = "preimage.submit"))]
    async fn submit(
        &self,
        _cx: &CallContext,
        request: RemotePreimageSubmitRequest,
    ) -> Result<RemotePreimageSubmitResponse, CallError<RemotePreimageSubmitError>> {
        let RemotePreimageSubmitRequest::V1(value) = request;
        let confirmed = self
            .platform
            .confirm_user_action(UserConfirmationReview::PreimageSubmit(
                PreimageSubmitReview {
                    size: value.len() as u64,
                },
            ))
            .await
            .map_err(|err| {
                CallError::Domain(RemotePreimageSubmitError::V1(
                    v01::PreimageSubmitError::Unknown { reason: err.reason },
                ))
            })?;
        if !confirmed {
            return Err(CallError::Domain(RemotePreimageSubmitError::V1(
                v01::PreimageSubmitError::Unknown {
                    reason: "User rejected preimage submission".to_string(),
                },
            )));
        }
        self.platform
            .submit_preimage(value)
            .await
            .map(RemotePreimageSubmitResponse::V1)
            .map_err(|err| CallError::Domain(RemotePreimageSubmitError::V1(err)))
    }
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

impl Theme for PlatformRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "theme.subscribe"))]
    async fn subscribe(&self, _cx: &CallContext) -> Subscription<HostThemeSubscribeItem> {
        let stream = self.platform.subscribe_theme().filter_map(|item| async {
            item.ok().map(|variant| {
                HostThemeSubscribeItem::V1(v01::HostThemeSubscribeItem {
                    name: v01::ThemeName::Default,
                    variant,
                })
            })
        });
        Subscription::new(Box::pin(stream))
    }
}

// `Notifications` delegates to the platform so hosts can own scheduling and
// cancellation while the core preserves the typed TrUAPI wire shape.
impl Notifications for PlatformRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "notifications.send_push_notification"))]
    async fn send_push_notification(
        &self,
        _cx: &CallContext,
        request: HostPushNotificationRequest,
    ) -> Result<HostPushNotificationResponse, CallError<HostPushNotificationError>> {
        let HostPushNotificationRequest::V1(inner) = request;
        self.platform
            .push_notification(inner)
            .await
            .map(HostPushNotificationResponse::V1)
            .map_err(|err| {
                CallError::Domain(HostPushNotificationError::V1(
                    v01::HostPushNotificationError::Unknown { reason: err.reason },
                ))
            })
    }

    #[instrument(skip_all, fields(runtime.method = "notifications.cancel_push_notification"))]
    async fn cancel_push_notification(
        &self,
        _cx: &CallContext,
        request: HostPushNotificationCancelRequest,
    ) -> Result<HostPushNotificationCancelResponse, CallError<HostPushNotificationCancelError>>
    {
        let HostPushNotificationCancelRequest::V1(v01::HostPushNotificationCancelRequest { id }) =
            request;
        self.platform
            .cancel_notification(id)
            .await
            .map(|()| HostPushNotificationCancelResponse::V1)
            .map_err(|err| {
                CallError::Domain(HostPushNotificationCancelError::V1(v01::GenericError {
                    reason: err.reason,
                }))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_logic::sso::messages::{RemoteMessageData, RemoteMessageV1};
    use crate::test_support::*;
    use std::sync::Mutex;
    use truapi_platform::{AuthState, CoreStorageKey};

    use super::sso_remote::SSO_PEER_DISCONNECT_REASON;

    fn wait_until(mut condition: impl FnMut() -> bool, message: &str) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !condition() {
            assert!(std::time::Instant::now() < deadline, "{message}");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn feature_supported_round_trips_through_runtime() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        let cx = CallContext::new();
        let request = HostFeatureSupportedRequest::V1(v01::HostFeatureSupportedRequest::Chain {
            genesis_hash: vec![0u8; 32],
        });
        let response = futures::executor::block_on(host.feature_supported(&cx, request)).unwrap();
        let HostFeatureSupportedResponse::V1(inner) = response;
        assert!(inner.supported);
    }

    #[test]
    fn navigate_to_uses_dotns_decision_and_then_platform() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        let cx = CallContext::new();
        let request = HostNavigateToRequest::V1(v01::HostNavigateToRequest {
            url: "mytestapp.dot".to_string(),
        });
        let response = futures::executor::block_on(host.navigate_to(&cx, request)).unwrap();
        assert_eq!(response, HostNavigateToResponse::V1);
    }

    #[test]
    fn navigate_to_rejects_empty_input_without_calling_platform() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        let cx = CallContext::new();
        let request = HostNavigateToRequest::V1(v01::HostNavigateToRequest {
            url: "".to_string(),
        });
        let err = futures::executor::block_on(host.navigate_to(&cx, request)).unwrap_err();
        match err {
            CallError::Domain(HostNavigateToError::V1(v01::HostNavigateToError::Unknown {
                ..
            })) => {}
            other => panic!("expected Unknown navigate error, got {other:?}"),
        }
    }

    #[test]
    fn push_notification_delegates_payload_and_returns_host_id() {
        let pushed_notifications = Arc::new(Mutex::new(Vec::new()));
        let platform = Arc::new(StubPlatform {
            notification_id: 42,
            pushed_notifications: pushed_notifications.clone(),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new_compat(platform, test_spawner());
        let cx = CallContext::new();
        let request = HostPushNotificationRequest::V1(v01::HostPushNotificationRequest {
            text: "Hello".to_string(),
            deeplink: Some("https://example.invalid/launch".to_string()),
            scheduled_at: Some(1_776_144_000_000),
        });

        let response =
            futures::executor::block_on(host.send_push_notification(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostPushNotificationResponse::V1(v01::HostPushNotificationResponse { id: 42 })
        );
        assert_eq!(
            pushed_notifications
                .lock()
                .expect("notification list mutex poisoned")
                .as_slice(),
            &[v01::HostPushNotificationRequest {
                text: "Hello".to_string(),
                deeplink: Some("https://example.invalid/launch".to_string()),
                scheduled_at: Some(1_776_144_000_000),
            }]
        );
    }

    #[test]
    fn cancel_notification_delegates_host_id() {
        let cancelled_notifications = Arc::new(Mutex::new(Vec::new()));
        let platform = Arc::new(StubPlatform {
            cancelled_notifications: cancelled_notifications.clone(),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new_compat(platform, test_spawner());
        let cx = CallContext::new();
        let request =
            HostPushNotificationCancelRequest::V1(v01::HostPushNotificationCancelRequest {
                id: 42,
            });

        let response =
            futures::executor::block_on(host.cancel_push_notification(&cx, request)).unwrap();

        assert_eq!(response, HostPushNotificationCancelResponse::V1);
        assert_eq!(
            cancelled_notifications
                .lock()
                .expect("notification cancellation list mutex poisoned")
                .as_slice(),
            &[42]
        );
    }

    #[test]
    fn get_account_requires_session() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        let cx = CallContext::new();
        let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
            product_account_id: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: 0,
            },
        });
        let err = futures::executor::block_on(host.get_account(&cx, request)).unwrap_err();
        assert!(matches!(
            err,
            CallError::Domain(HostAccountGetError::V1(
                v01::HostAccountGetError::NotConnected
            ))
        ));
    }

    #[test]
    fn get_account_rejects_invalid_product_identifier() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
            product_account_id: v01::ProductAccountId {
                dot_ns_identifier: "example.com".to_string(),
                derivation_index: 0,
            },
        });
        let err = futures::executor::block_on(host.get_account(&cx, request)).unwrap_err();
        assert!(matches!(
            err,
            CallError::Domain(HostAccountGetError::V1(
                v01::HostAccountGetError::DomainNotValid
            ))
        ));
    }

    #[test]
    fn get_account_derives_dotli_product_key() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
            product_account_id: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: 0,
            },
        });
        let response = futures::executor::block_on(host.get_account(&cx, request)).unwrap();
        let HostAccountGetResponse::V1(inner) = response;
        assert_eq!(
            hex::encode(inner.account.public_key),
            "281489e3dd1c4dbe88cd670a59edcc9c44d64f510d302bd527ec306f10292f08"
        );
    }

    #[test]
    fn get_account_normalizes_product_identifier_before_deriving() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let request = HostAccountGetRequest::V1(v01::HostAccountGetRequest {
            product_account_id: v01::ProductAccountId {
                dot_ns_identifier: "MyApp.DOT".to_string(),
                derivation_index: 0,
            },
        });
        let response = futures::executor::block_on(host.get_account(&cx, request)).unwrap();
        let HostAccountGetResponse::V1(inner) = response;
        assert_eq!(
            hex::encode(inner.account.public_key),
            "281489e3dd1c4dbe88cd670a59edcc9c44d64f510d302bd527ec306f10292f08"
        );
    }

    #[test]
    fn get_account_alias_requires_session() {
        let host =
            PlatformRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        let cx = CallContext::new();
        let err = futures::executor::block_on(
            host.get_account_alias(&cx, account_alias_request("myapp.dot")),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            CallError::Domain(HostAccountGetAliasError::V1(
                v01::HostAccountGetError::NotConnected
            ))
        ));
    }

    #[test]
    fn get_account_alias_rejects_invalid_product_identifier() {
        let host =
            PlatformRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let err = futures::executor::block_on(
            host.get_account_alias(&cx, account_alias_request("example.com")),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            CallError::Domain(HostAccountGetAliasError::V1(
                v01::HostAccountGetError::DomainNotValid
            ))
        ));
    }

    #[test]
    fn get_account_alias_same_domain_returns_sso_response() {
        let session = sso_session_info();
        let platform = Arc::new(StubPlatform {
            rpc_responses: sso_success_responses(
                &session,
                "alias-1",
                crate::host_logic::sso::messages::RemoteMessage {
                    message_id: "wallet-alias-1".to_string(),
                    data: crate::host_logic::sso::messages::RemoteMessageData::V1(
                        crate::host_logic::sso::messages::RemoteMessageV1::RingVrfAliasResponse(
                            crate::host_logic::sso::messages::RingVrfAliasResponse {
                                responding_to: "alias-1".to_string(),
                                payload: Ok(v01::HostAccountGetAliasResponse {
                                    context: [9; 32],
                                    alias: vec![1, 2, 3],
                                }),
                            },
                        ),
                    ),
                },
            ),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.session_state().set_session(session.clone());
        let cx = CallContext::with_request_id("alias-1".to_string());
        let response = futures::executor::block_on(
            host.get_account_alias(&cx, account_alias_request("myapp.dot")),
        )
        .unwrap();
        let HostAccountGetAliasResponse::V1(inner) = response;
        assert_eq!(inner.context, [9; 32]);
        assert_eq!(inner.alias, vec![1, 2, 3]);
        let message = submitted_remote_message(&platform, &session);
        let crate::host_logic::sso::messages::RemoteMessageData::V1(
            crate::host_logic::sso::messages::RemoteMessageV1::RingVrfAliasRequest(request),
        ) = message.data
        else {
            panic!("expected ring VRF alias request");
        };
        assert_eq!(request.product_account_id.dot_ns_identifier, "myapp.dot");
        assert_eq!(request.product_id, "myapp.dot");
    }

    #[test]
    fn get_account_alias_normalizes_remote_request_identifier() {
        let session = sso_session_info();
        let platform = Arc::new(StubPlatform {
            rpc_responses: sso_success_responses(
                &session,
                "alias-1",
                crate::host_logic::sso::messages::RemoteMessage {
                    message_id: "wallet-alias-1".to_string(),
                    data: crate::host_logic::sso::messages::RemoteMessageData::V1(
                        crate::host_logic::sso::messages::RemoteMessageV1::RingVrfAliasResponse(
                            crate::host_logic::sso::messages::RingVrfAliasResponse {
                                responding_to: "alias-1".to_string(),
                                payload: Ok(v01::HostAccountGetAliasResponse {
                                    context: [9; 32],
                                    alias: vec![1, 2, 3],
                                }),
                            },
                        ),
                    ),
                },
            ),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("MyApp.DOT"),
            test_spawner(),
        );
        host.session_state().set_session(session.clone());
        let cx = CallContext::with_request_id("alias-1".to_string());
        futures::executor::block_on(
            host.get_account_alias(&cx, account_alias_request("MyApp.DOT")),
        )
        .unwrap();
        let message = submitted_remote_message(&platform, &session);
        let crate::host_logic::sso::messages::RemoteMessageData::V1(
            crate::host_logic::sso::messages::RemoteMessageV1::RingVrfAliasRequest(request),
        ) = message.data
        else {
            panic!("expected ring VRF alias request");
        };
        assert_eq!(request.product_account_id.dot_ns_identifier, "myapp.dot");
        assert_eq!(request.product_id, "myapp.dot");
    }

    #[test]
    fn get_account_alias_cross_domain_rejects_when_user_declines() {
        let host =
            PlatformRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let err = futures::executor::block_on(
            host.get_account_alias(&cx, account_alias_request("other.dot")),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            CallError::Domain(HostAccountGetAliasError::V1(
                v01::HostAccountGetError::Rejected
            ))
        ));
    }

    #[test]
    fn get_account_alias_cross_domain_maps_confirmation_failure_to_host_failure() {
        let host = PlatformRuntimeHost::new(
            Arc::new(StubPlatform {
                account_alias_error: Some("modal failed"),
                ..Default::default()
            }),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let err = futures::executor::block_on(
            host.get_account_alias(&cx, account_alias_request("other.dot")),
        )
        .unwrap_err();
        assert!(
            matches!(err, CallError::HostFailure { reason } if reason.contains("modal failed"))
        );
    }

    #[test]
    fn get_account_alias_cross_domain_accepts_confirmation_then_returns_sso_response() {
        let session = sso_session_info();
        let platform = Arc::new(StubPlatform {
            account_alias_confirmed: true,
            rpc_responses: sso_success_responses(
                &session,
                "alias-2",
                crate::host_logic::sso::messages::RemoteMessage {
                    message_id: "wallet-alias-2".to_string(),
                    data: crate::host_logic::sso::messages::RemoteMessageData::V1(
                        crate::host_logic::sso::messages::RemoteMessageV1::RingVrfAliasResponse(
                            crate::host_logic::sso::messages::RingVrfAliasResponse {
                                responding_to: "alias-2".to_string(),
                                payload: Ok(v01::HostAccountGetAliasResponse {
                                    context: [8; 32],
                                    alias: vec![4, 5, 6],
                                }),
                            },
                        ),
                    ),
                },
            ),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.session_state().set_session(session.clone());
        let cx = CallContext::with_request_id("alias-2".to_string());
        let response = futures::executor::block_on(
            host.get_account_alias(&cx, account_alias_request("other.dot")),
        )
        .unwrap();
        let HostAccountGetAliasResponse::V1(inner) = response;
        assert_eq!(inner.context, [8; 32]);
        assert_eq!(inner.alias, vec![4, 5, 6]);
        let message = submitted_remote_message(&platform, &session);
        assert!(matches!(
            message.data,
            crate::host_logic::sso::messages::RemoteMessageData::V1(
                crate::host_logic::sso::messages::RemoteMessageV1::RingVrfAliasRequest(_)
            )
        ));
    }

    #[test]
    fn get_legacy_accounts_returns_derived_slot_zero_when_connected() {
        let host = PlatformRuntimeHost::new(
            stub_platform(),
            runtime_config("localhost:3000"),
            test_spawner(),
        );
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let response = futures::executor::block_on(
            host.get_legacy_accounts(&cx, HostGetLegacyAccountsRequest::V1),
        )
        .unwrap();
        let HostGetLegacyAccountsResponse::V1(inner) = response;
        assert_eq!(inner.accounts.len(), 1);
        assert_eq!(inner.accounts[0].name.as_deref(), Some("alice"));
        assert_eq!(
            hex::encode(&inner.accounts[0].public_key),
            "1c822b488297fde8c60d9cbc5585839f70a69fb2c5c69daa66b6043c75184467"
        );
    }

    #[test]
    fn get_legacy_accounts_returns_empty_when_disconnected() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        let cx = CallContext::new();
        let response = futures::executor::block_on(
            host.get_legacy_accounts(&cx, HostGetLegacyAccountsRequest::V1),
        )
        .unwrap();
        let HostGetLegacyAccountsResponse::V1(inner) = response;
        assert!(inner.accounts.is_empty());
    }

    #[test]
    fn get_user_id_returns_primary_username() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let response =
            futures::executor::block_on(host.get_user_id(&cx, HostGetUserIdRequest::V1)).unwrap();
        let HostGetUserIdResponse::V1(inner) = response;
        assert_eq!(inner.primary_username, "Alice Smith");
    }

    #[test]
    fn derive_entropy_matches_dotli_vector() {
        let host =
            PlatformRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let request = HostDeriveEntropyRequest::V1(v01::HostDeriveEntropyRequest {
            context: b"product-key".to_vec(),
        });
        let response = futures::executor::block_on(host.derive(&cx, request)).unwrap();
        let HostDeriveEntropyResponse::V1(inner) = response;
        assert_eq!(
            hex::encode(inner.entropy),
            "ab1887248c9de3cf4b8c5a255782796d3d35a98c8eb2d7df61a410db8b14da36"
        );
    }

    #[test]
    fn derive_entropy_requires_session() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        let cx = CallContext::new();
        let request = HostDeriveEntropyRequest::V1(v01::HostDeriveEntropyRequest {
            context: b"product-key".to_vec(),
        });
        let err = futures::executor::block_on(host.derive(&cx, request)).unwrap_err();
        match err {
            CallError::Domain(HostDeriveEntropyError::V1(
                v01::HostDeriveEntropyError::Unknown { reason },
            )) => assert_eq!(reason, "Not connected"),
            other => panic!("expected Unknown entropy error, got {other:?}"),
        }
    }

    #[test]
    fn derive_entropy_requires_secret() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        let mut session = session_info();
        session.root_entropy_source = None;
        host.session_state().set_session(session);
        let cx = CallContext::new();
        let request = HostDeriveEntropyRequest::V1(v01::HostDeriveEntropyRequest {
            context: b"product-key".to_vec(),
        });
        let err = futures::executor::block_on(host.derive(&cx, request)).unwrap_err();
        match err {
            CallError::Domain(HostDeriveEntropyError::V1(
                v01::HostDeriveEntropyError::Unknown { reason },
            )) => assert_eq!(reason, "Session secret missing"),
            other => panic!("expected Unknown entropy error, got {other:?}"),
        }
    }

    #[test]
    fn derive_entropy_rejects_empty_context_like_dotli_key() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let request =
            HostDeriveEntropyRequest::V1(v01::HostDeriveEntropyRequest { context: vec![] });
        let err = futures::executor::block_on(host.derive(&cx, request)).unwrap_err();
        match err {
            CallError::Domain(HostDeriveEntropyError::V1(
                v01::HostDeriveEntropyError::Unknown { reason },
            )) => assert_eq!(reason, "\"key\" must be between 1 and 32 bytes, got 0"),
            other => panic!("expected Unknown entropy error, got {other:?}"),
        }
    }

    #[test]
    fn preimage_submit_confirms_and_delegates_to_platform() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        let cx = CallContext::new();
        let request = RemotePreimageSubmitRequest::V1(vec![1, 2, 3]);
        let response = futures::executor::block_on(Preimage::submit(&host, &cx, request)).unwrap();
        assert_eq!(response, RemotePreimageSubmitResponse::V1(vec![1, 2, 3]));
    }

    #[test]
    fn preimage_lookup_subscribe_maps_platform_values() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        let cx = CallContext::new();
        let request =
            RemotePreimageLookupSubscribeRequest::V1(v01::RemotePreimageLookupSubscribeRequest {
                key: vec![0; 32],
            });
        let mut subscription = futures::executor::block_on(host.lookup_subscribe(&cx, request));
        let item = futures::executor::block_on(subscription.next()).expect("preimage item");
        assert_eq!(
            item,
            RemotePreimageLookupSubscribeItem::V1(v01::RemotePreimageLookupSubscribeItem {
                value: Some(vec![9, 8, 7])
            })
        );
    }

    #[test]
    fn theme_subscribe_maps_platform_values() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        let cx = CallContext::new();
        let mut subscription = futures::executor::block_on(Theme::subscribe(&host, &cx));
        let item = futures::executor::block_on(subscription.next()).expect("theme item");
        assert_eq!(
            item,
            HostThemeSubscribeItem::V1(v01::HostThemeSubscribeItem {
                name: v01::ThemeName::Default,
                variant: v01::ThemeVariant::Dark,
            })
        );
    }

    #[test]
    fn sign_raw_rejects_invalid_product_account() {
        let host =
            PlatformRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: account_id("other.dot", 0),
            payload: raw_payload(),
        });
        let err = futures::executor::block_on(host.sign_raw(&cx, request)).unwrap_err();
        assert!(matches!(
            err,
            CallError::Domain(HostSignRawError::V1(
                v01::HostSignPayloadError::PermissionDenied
            ))
        ));
    }

    #[test]
    fn sign_raw_rejects_without_session_after_valid_account() {
        let host =
            PlatformRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        let cx = CallContext::new();
        let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: account_id("myapp.dot", 0),
            payload: raw_payload(),
        });
        let err = futures::executor::block_on(host.sign_raw(&cx, request)).unwrap_err();
        assert!(matches!(
            err,
            CallError::Domain(HostSignRawError::V1(v01::HostSignPayloadError::Rejected))
        ));
    }

    #[test]
    fn sign_raw_denies_when_chain_submit_denied() {
        let host = PlatformRuntimeHost::new(
            Arc::new(StubPlatform {
                remote_permission_granted: false,
                ..Default::default()
            }),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: account_id("myapp.dot", 0),
            payload: raw_payload(),
        });
        let err = futures::executor::block_on(host.sign_raw(&cx, request)).unwrap_err();
        assert!(matches!(
            err,
            CallError::Domain(HostSignRawError::V1(
                v01::HostSignPayloadError::PermissionDenied
            ))
        ));
    }

    #[test]
    fn sign_raw_rejects_when_user_declines_confirmation() {
        let host =
            PlatformRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: account_id("myapp.dot", 0),
            payload: raw_payload(),
        });
        let err = futures::executor::block_on(host.sign_raw(&cx, request)).unwrap_err();
        assert!(matches!(
            err,
            CallError::Domain(HostSignRawError::V1(v01::HostSignPayloadError::Rejected))
        ));
    }

    #[test]
    fn sign_raw_accepts_confirmation_then_returns_sso_response() {
        let session = sso_session_info();
        let platform = Arc::new(StubPlatform {
            sign_raw_confirmed: true,
            rpc_responses: sso_success_responses(
                &session,
                "sign-raw-1",
                sign_response_message("sign-raw-1", vec![7, 7], None),
            ),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.session_state().set_session(session.clone());
        let cx = CallContext::with_request_id("sign-raw-1".to_string());
        let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: account_id("myapp.dot", 0),
            payload: raw_payload(),
        });
        let response = futures::executor::block_on(host.sign_raw(&cx, request)).unwrap();
        let HostSignRawResponse::V1(inner) = response;
        assert_eq!(inner.signature, vec![7, 7]);
        assert_eq!(inner.signed_transaction, None);
        let message = submitted_remote_message(&platform, &session);
        assert!(matches!(
            &message.data,
            crate::host_logic::sso::messages::RemoteMessageData::V1(
                crate::host_logic::sso::messages::RemoteMessageV1::SignRequest(request)
            ) if matches!(
                request.as_ref(),
                crate::host_logic::sso::messages::SigningRequest::Raw(_)
            )
        ));
        let sent = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
        let methods = sent
            .iter()
            .map(|request| {
                serde_json::from_str::<serde_json::Value>(request).unwrap()["method"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            methods,
            vec![
                "statement_subscribeStatement",
                "statement_subscribeStatement",
                "statement_submit",
                "statement_unsubscribeStatement",
                "statement_unsubscribeStatement",
            ]
        );
        let mut unsubscribe_ids = sent[3..]
            .iter()
            .map(|request| serde_json::from_str::<serde_json::Value>(request).unwrap())
            .map(|request| request["params"][0].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        unsubscribe_ids.sort();
        assert_eq!(
            unsubscribe_ids,
            vec!["own-sub-sign-raw-1", "peer-sub-sign-raw-1"]
        );
    }

    #[test]
    fn sign_raw_peer_disconnect_clears_session_store_and_broadcasts() {
        let session = sso_session_info();
        let platform = Arc::new(StubPlatform {
            sign_raw_confirmed: true,
            rpc_responses: sso_peer_disconnect_responses(&session, "sign-raw-disconnect"),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.session_state().set_session(session);
        let mut statuses = host.session_state().subscribe();
        assert_eq!(
            futures::executor::block_on(statuses.next()).unwrap(),
            HostAccountConnectionStatusSubscribeItem::V1(
                v01::HostAccountConnectionStatusSubscribeItem::Connected
            )
        );

        let cx = CallContext::with_request_id("sign-raw-disconnect".to_string());
        let request = HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: account_id("myapp.dot", 0),
            payload: raw_payload(),
        });
        let err = futures::executor::block_on(host.sign_raw(&cx, request)).unwrap_err();

        assert!(matches!(
            err,
            CallError::Domain(HostSignRawError::V1(
                v01::HostSignPayloadError::Unknown { reason }
            )) if reason == SSO_PEER_DISCONNECT_REASON
        ));
        assert!(host.session_state().current().is_none());
        assert_eq!(
            *platform
                .session_clears
                .lock()
                .expect("session clear counter mutex poisoned"),
            1
        );
        assert_eq!(
            futures::executor::block_on(statuses.next()).unwrap(),
            HostAccountConnectionStatusSubscribeItem::V1(
                v01::HostAccountConnectionStatusSubscribeItem::Disconnected
            )
        );
    }

    #[test]
    fn idle_peer_disconnect_monitor_clears_session_store_and_broadcasts() {
        let session = sso_session_info();
        let platform = Arc::new(StubPlatform {
            rpc_responses: sso_peer_disconnect_monitor_responses(&session),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.session_state().set_session(session.clone());
        let mut statuses = host.session_state().subscribe();
        assert_eq!(
            futures::executor::block_on(statuses.next()).unwrap(),
            HostAccountConnectionStatusSubscribeItem::V1(
                v01::HostAccountConnectionStatusSubscribeItem::Connected
            )
        );

        host.start_sso_disconnect_monitor(&session);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let disconnected = loop {
            if let Some(item) = statuses.next().now_or_never() {
                break item.expect("status stream ended");
            }
            assert!(
                std::time::Instant::now() < deadline,
                "peer disconnect monitor did not emit Disconnected"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        };

        assert!(host.session_state().current().is_none());
        assert_eq!(
            *platform
                .session_clears
                .lock()
                .expect("session clear counter mutex poisoned"),
            1
        );
        assert_eq!(
            disconnected,
            HostAccountConnectionStatusSubscribeItem::V1(
                v01::HostAccountConnectionStatusSubscribeItem::Disconnected
            )
        );
    }

    #[test]
    fn sign_payload_denies_when_chain_submit_denied() {
        let host = PlatformRuntimeHost::new(
            Arc::new(StubPlatform {
                remote_permission_granted: false,
                ..Default::default()
            }),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let request = HostSignPayloadRequest::V1(v01::HostSignPayloadRequest {
            account: account_id("myapp.dot", 0),
            payload: sign_payload_data(),
        });
        let err = futures::executor::block_on(host.sign_payload(&cx, request)).unwrap_err();
        assert!(matches!(
            err,
            CallError::Domain(HostSignPayloadError::V1(
                v01::HostSignPayloadError::PermissionDenied
            ))
        ));
    }

    #[test]
    fn sign_payload_maps_confirmation_failure_to_host_failure() {
        let host = PlatformRuntimeHost::new(
            Arc::new(StubPlatform {
                sign_payload_error: Some("modal failed"),
                ..Default::default()
            }),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let request = HostSignPayloadRequest::V1(v01::HostSignPayloadRequest {
            account: account_id("myapp.dot", 0),
            payload: sign_payload_data(),
        });
        let err = futures::executor::block_on(host.sign_payload(&cx, request)).unwrap_err();
        assert!(
            matches!(err, CallError::HostFailure { reason } if reason.contains("modal failed"))
        );
    }

    #[test]
    fn sign_payload_accepts_confirmation_then_returns_sso_response() {
        let session = sso_session_info();
        let platform = Arc::new(StubPlatform {
            sign_payload_confirmed: true,
            rpc_responses: sso_success_responses(
                &session,
                "sign-payload-1",
                sign_response_message("sign-payload-1", vec![8, 8], Some(vec![0xab, 0xcd])),
            ),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.session_state().set_session(session.clone());
        let cx = CallContext::with_request_id("sign-payload-1".to_string());
        let request = HostSignPayloadRequest::V1(v01::HostSignPayloadRequest {
            account: account_id("myapp.dot", 0),
            payload: sign_payload_data(),
        });

        let response = futures::executor::block_on(host.sign_payload(&cx, request)).unwrap();

        let HostSignPayloadResponse::V1(inner) = response;
        assert_eq!(inner.signature, vec![8, 8]);
        assert_eq!(inner.signed_transaction, Some(vec![0xab, 0xcd]));
        let message = submitted_remote_message(&platform, &session);
        assert!(matches!(
            &message.data,
            crate::host_logic::sso::messages::RemoteMessageData::V1(
                crate::host_logic::sso::messages::RemoteMessageV1::SignRequest(request)
            ) if matches!(
                request.as_ref(),
                crate::host_logic::sso::messages::SigningRequest::Payload(_)
            )
        ));
    }

    #[test]
    fn create_transaction_accepts_confirmation_then_returns_sso_response() {
        let session = sso_session_info();
        let platform = Arc::new(StubPlatform {
            create_transaction_confirmed: true,
            rpc_responses: sso_success_responses(
                &session,
                "create-tx-1",
                crate::host_logic::sso::messages::RemoteMessage {
                    message_id: "wallet-create-tx-1".to_string(),
                    data: crate::host_logic::sso::messages::RemoteMessageData::V1(
                        crate::host_logic::sso::messages::RemoteMessageV1::CreateTransactionResponse(
                            crate::host_logic::sso::messages::CreateTransactionResponse {
                                responding_to: "create-tx-1".to_string(),
                                signed_transaction: Ok(vec![0xca, 0xfe]),
                            },
                        ),
                    ),
                },
            ),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.session_state().set_session(session.clone());
        let cx = CallContext::with_request_id("create-tx-1".to_string());
        let request = HostCreateTransactionRequest::V1(product_tx_payload("myapp.dot"));
        let response = futures::executor::block_on(host.create_transaction(&cx, request)).unwrap();
        let HostCreateTransactionResponse::V1(inner) = response;
        assert_eq!(inner.transaction, vec![0xca, 0xfe]);
        let message = submitted_remote_message(&platform, &session);
        assert!(matches!(
            message.data,
            crate::host_logic::sso::messages::RemoteMessageData::V1(
                crate::host_logic::sso::messages::RemoteMessageV1::CreateTransactionRequest(_)
            )
        ));
    }

    #[test]
    fn legacy_sign_raw_rejects_signer_mismatch() {
        let host =
            PlatformRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let request =
            HostSignRawWithLegacyAccountRequest::V1(v01::HostSignRawWithLegacyAccountRequest {
                signer: "5Ci5sCERp3MFEDpF2jVkQDJoBevpRosB7toYRqKWShewhdhq".to_string(),
                payload: raw_payload(),
            });
        let err = futures::executor::block_on(host.sign_raw_with_legacy_account(&cx, request))
            .unwrap_err();
        match err {
            CallError::Domain(HostSignRawWithLegacyAccountError::V1(
                v01::HostSignPayloadError::Unknown { reason },
            )) => assert_eq!(reason, "Account can't be derived from product account id"),
            other => panic!("expected legacy signer mismatch, got {other:?}"),
        }
    }

    #[test]
    fn legacy_sign_raw_denies_when_chain_submit_denied() {
        let host = PlatformRuntimeHost::new(
            Arc::new(StubPlatform {
                remote_permission_granted: false,
                ..Default::default()
            }),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.session_state().set_session(sso_session_info());
        let cx = CallContext::new();
        let request =
            HostSignRawWithLegacyAccountRequest::V1(v01::HostSignRawWithLegacyAccountRequest {
                signer: "5CyFsdhwjXy7wWpDEM6isungQ3LfGnu9UXkt7paBQ6DYRxk1".to_string(),
                payload: raw_payload(),
            });
        let err = futures::executor::block_on(host.sign_raw_with_legacy_account(&cx, request))
            .unwrap_err();
        assert!(matches!(
            err,
            CallError::Domain(HostSignRawWithLegacyAccountError::V1(
                v01::HostSignPayloadError::PermissionDenied
            ))
        ));
    }

    #[test]
    fn legacy_sign_raw_accepts_derived_ss58_then_returns_sso_response() {
        let session = sso_session_info();
        let platform = Arc::new(StubPlatform {
            sign_raw_confirmed: true,
            rpc_responses: sso_success_responses(
                &session,
                "legacy-sign-raw-1",
                sign_response_message("legacy-sign-raw-1", vec![9, 9], None),
            ),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.session_state().set_session(session.clone());
        let cx = CallContext::with_request_id("legacy-sign-raw-1".to_string());
        let request =
            HostSignRawWithLegacyAccountRequest::V1(v01::HostSignRawWithLegacyAccountRequest {
                signer: "5CyFsdhwjXy7wWpDEM6isungQ3LfGnu9UXkt7paBQ6DYRxk1".to_string(),
                payload: raw_payload(),
            });
        let response =
            futures::executor::block_on(host.sign_raw_with_legacy_account(&cx, request)).unwrap();
        let HostSignRawWithLegacyAccountResponse::V1(inner) = response;
        assert_eq!(inner.signature, vec![9, 9]);
        assert_eq!(inner.signed_transaction, None);
        let message = submitted_remote_message(&platform, &session);
        assert!(matches!(
            &message.data,
            crate::host_logic::sso::messages::RemoteMessageData::V1(
                crate::host_logic::sso::messages::RemoteMessageV1::SignRequest(request)
            ) if matches!(
                request.as_ref(),
                crate::host_logic::sso::messages::SigningRequest::Raw(raw)
                    if raw.product_account_id == account_id("myapp.dot", 0)
                        && matches!(
                            &raw.data,
                            crate::host_logic::sso::messages::SigningRawPayload::Bytes(bytes)
                                if bytes == b"hello"
                        )
            )
        ));
    }

    #[test]
    fn legacy_sign_raw_accepts_derived_hex_then_returns_sso_response() {
        let session = sso_session_info();
        let signer = derive_product_public_key(session.public_key, "myapp.dot", 0).unwrap();
        let platform = Arc::new(StubPlatform {
            sign_raw_confirmed: true,
            rpc_responses: sso_success_responses(
                &session,
                "legacy-sign-raw-hex-1",
                sign_response_message("legacy-sign-raw-hex-1", vec![8, 8], None),
            ),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.session_state().set_session(session.clone());
        let cx = CallContext::with_request_id("legacy-sign-raw-hex-1".to_string());
        let request =
            HostSignRawWithLegacyAccountRequest::V1(v01::HostSignRawWithLegacyAccountRequest {
                signer: format!("0x{}", hex::encode(signer)),
                payload: raw_payload(),
            });
        let response =
            futures::executor::block_on(host.sign_raw_with_legacy_account(&cx, request)).unwrap();
        let HostSignRawWithLegacyAccountResponse::V1(inner) = response;
        assert_eq!(inner.signature, vec![8, 8]);

        let message = submitted_remote_message(&platform, &session);
        let crate::host_logic::sso::messages::RemoteMessageData::V1(
            crate::host_logic::sso::messages::RemoteMessageV1::SignRequest(request),
        ) = message.data
        else {
            panic!("expected raw signing request");
        };
        let crate::host_logic::sso::messages::SigningRequest::Raw(request) = *request else {
            panic!("expected raw signing request");
        };
        assert_eq!(request.product_account_id, account_id("myapp.dot", 0));
    }

    #[test]
    fn legacy_create_transaction_rejects_raw_key_mismatch() {
        let host =
            PlatformRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let request =
            HostCreateTransactionWithLegacyAccountRequest::V1(v01::LegacyAccountTxPayload {
                signer: [0; 32],
                genesis_hash: [1; 32],
                call_data: vec![0],
                extensions: vec![],
                tx_ext_version: 0,
            });
        let err =
            futures::executor::block_on(host.create_transaction_with_legacy_account(&cx, request))
                .unwrap_err();
        match err {
            CallError::Domain(HostCreateTransactionWithLegacyAccountError::V1(
                v01::HostCreateTransactionError::Unknown { reason },
            )) => assert_eq!(reason, "Account can't be derived from product account id"),
            other => panic!("expected legacy signer mismatch, got {other:?}"),
        }
    }

    #[test]
    fn legacy_create_transaction_accepts_derived_key_then_returns_sso_response() {
        let session = sso_session_info();
        let signer = derive_product_public_key(session.public_key, "myapp.dot", 0).unwrap();
        let platform = Arc::new(StubPlatform {
            create_transaction_confirmed: true,
            rpc_responses: sso_success_responses(
                &session,
                "legacy-create-tx-1",
                crate::host_logic::sso::messages::RemoteMessage {
                    message_id: "wallet-legacy-create-tx-1".to_string(),
                    data: crate::host_logic::sso::messages::RemoteMessageData::V1(
                        crate::host_logic::sso::messages::RemoteMessageV1::CreateTransactionResponse(
                            crate::host_logic::sso::messages::CreateTransactionResponse {
                                responding_to: "legacy-create-tx-1".to_string(),
                                signed_transaction: Ok(vec![0xca, 0xfe]),
                            },
                        ),
                    ),
                },
            ),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.session_state().set_session(session.clone());
        let cx = CallContext::with_request_id("legacy-create-tx-1".to_string());
        let request =
            HostCreateTransactionWithLegacyAccountRequest::V1(v01::LegacyAccountTxPayload {
                signer,
                genesis_hash: [1; 32],
                call_data: vec![0],
                extensions: vec![],
                tx_ext_version: 0,
            });

        let response =
            futures::executor::block_on(host.create_transaction_with_legacy_account(&cx, request))
                .unwrap();

        let HostCreateTransactionWithLegacyAccountResponse::V1(inner) = response;
        assert_eq!(inner.transaction, vec![0xca, 0xfe]);
        let message = submitted_remote_message(&platform, &session);
        let crate::host_logic::sso::messages::RemoteMessageData::V1(
            crate::host_logic::sso::messages::RemoteMessageV1::CreateTransactionRequest(request),
        ) = message.data
        else {
            panic!("expected product transaction request");
        };
        let crate::host_logic::sso::messages::CreateTransactionPayload::V1(payload) =
            request.payload;
        assert_eq!(payload.signer, account_id("myapp.dot", 0));
        assert_eq!(
            signer,
            derive_product_public_key(session.public_key, "myapp.dot", 0).unwrap()
        );
    }

    #[test]
    fn create_transaction_rejects_invalid_product_account() {
        let host =
            PlatformRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let request = HostCreateTransactionRequest::V1(product_tx_payload("other.dot"));
        let err = futures::executor::block_on(host.create_transaction(&cx, request)).unwrap_err();
        assert!(matches!(
            err,
            CallError::Domain(HostCreateTransactionError::V1(
                v01::HostCreateTransactionError::PermissionDenied
            ))
        ));
    }

    #[test]
    fn resource_allocation_rejects_without_session() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        let cx = CallContext::new();
        let err = futures::executor::block_on(ResourceAllocation::request(
            &host,
            &cx,
            resource_allocation_request(),
        ))
        .unwrap_err();
        match err {
            CallError::Domain(HostRequestResourceAllocationError::V1(
                v01::ResourceAllocationError::Unknown { reason },
            )) => assert_eq!(reason, "No active session"),
            other => panic!("expected no-session resource allocation error, got {other:?}"),
        }
    }

    #[test]
    fn resource_allocation_rejects_when_user_declines() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let err = futures::executor::block_on(ResourceAllocation::request(
            &host,
            &cx,
            resource_allocation_request(),
        ))
        .unwrap_err();
        match err {
            CallError::Domain(HostRequestResourceAllocationError::V1(
                v01::ResourceAllocationError::Unknown { reason },
            )) => assert_eq!(reason, "User rejected resource allocation"),
            other => panic!("expected user-rejected resource allocation error, got {other:?}"),
        }
    }

    #[test]
    fn resource_allocation_maps_confirmation_failure_to_host_failure() {
        let host = PlatformRuntimeHost::new_compat(
            Arc::new(StubPlatform {
                resource_allocation_error: Some("modal failed"),
                ..Default::default()
            }),
            test_spawner(),
        );
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let err = futures::executor::block_on(ResourceAllocation::request(
            &host,
            &cx,
            resource_allocation_request(),
        ))
        .unwrap_err();
        assert!(
            matches!(err, CallError::HostFailure { reason } if reason.contains("modal failed"))
        );
    }

    #[test]
    fn resource_allocation_accepts_confirmation_then_returns_sso_response() {
        let session = sso_session_info();
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            rpc_responses: sso_success_responses(
                &session,
                "alloc-1",
                crate::host_logic::sso::messages::RemoteMessage {
                    message_id: "wallet-alloc-1".to_string(),
                    data: crate::host_logic::sso::messages::RemoteMessageData::V1(
                        crate::host_logic::sso::messages::RemoteMessageV1::ResourceAllocationResponse(
                            crate::host_logic::sso::messages::ResourceAllocationResponse {
                                responding_to: "alloc-1".to_string(),
                                payload: Ok(vec![
                                    crate::host_logic::sso::messages::SsoAllocationOutcome::Allocated(
                                        crate::host_logic::sso::messages::SsoAllocatedResource::StatementStoreAllowance {
                                            slot_account_key: vec![1],
                                        },
                                    ),
                                    crate::host_logic::sso::messages::SsoAllocationOutcome::Rejected,
                                    crate::host_logic::sso::messages::SsoAllocationOutcome::NotAvailable,
                                ]),
                            },
                        ),
                    ),
                },
            ),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new_compat(platform.clone(), test_spawner());
        host.session_state().set_session(session.clone());
        let cx = CallContext::with_request_id("alloc-1".to_string());
        let response = futures::executor::block_on(ResourceAllocation::request(
            &host,
            &cx,
            resource_allocation_request(),
        ))
        .unwrap();
        let HostRequestResourceAllocationResponse::V1(inner) = response;
        assert_eq!(
            inner.outcomes,
            vec![
                v01::AllocationOutcome::Allocated,
                v01::AllocationOutcome::Rejected,
                v01::AllocationOutcome::NotAvailable,
            ]
        );
        let message = submitted_remote_message(&platform, &session);
        assert!(matches!(
            message.data,
            crate::host_logic::sso::messages::RemoteMessageData::V1(
                crate::host_logic::sso::messages::RemoteMessageV1::ResourceAllocationRequest(_)
            )
        ));
    }

    #[test]
    fn session_store_sync_restores_valid_blob_from_tick() {
        let stored = sso_session_info();
        let platform = Arc::new(StubPlatform {
            session_blob: Some(crate::host_logic::session::encode_persisted_session(
                &stored,
            )),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new_compat(platform.clone(), test_spawner());

        host.start_session_store_sync(test_spawner());
        wait_until(
            || host.session_state().current() == Some(stored.clone()),
            "session store sync did not restore valid blob",
        );

        assert_eq!(host.session_state().current(), Some(stored.clone()));
        assert_eq!(
            *platform
                .auth_states
                .lock()
                .expect("auth state list mutex poisoned"),
            vec![AuthState::Connected(connected_session_ui_info(&stored))]
        );
    }

    #[test]
    fn session_store_sync_replaces_valid_blob_and_broadcasts_connected() {
        let mut replacement = sso_session_info();
        replacement.public_key = [0x44; 32];
        let host = PlatformRuntimeHost::new_compat(
            Arc::new(StubPlatform {
                session_blob: Some(crate::host_logic::session::encode_persisted_session(
                    &replacement,
                )),
                ..Default::default()
            }),
            test_spawner(),
        );
        host.session_state().set_session(sso_session_info());
        let mut statuses = host.session_state().subscribe();
        let _ = futures::executor::block_on(statuses.next());

        host.start_session_store_sync(test_spawner());

        assert_eq!(
            futures::executor::block_on(statuses.next()).unwrap(),
            HostAccountConnectionStatusSubscribeItem::V1(
                v01::HostAccountConnectionStatusSubscribeItem::Connected
            )
        );
        assert_eq!(host.session_state().current(), Some(replacement));
    }

    #[test]
    fn session_store_sync_clears_invalid_blob() {
        let platform = Arc::new(StubPlatform {
            session_blob: Some(vec![0xff]),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new_compat(platform.clone(), test_spawner());
        host.session_state().set_session(sso_session_info());

        host.start_session_store_sync(test_spawner());
        wait_until(
            || host.session_state().current().is_none(),
            "session store sync did not clear invalid blob",
        );

        assert!(host.session_state().current().is_none());
        // `set_session` bypasses the auth state cell, so the cell never left
        // `Disconnected` and clearing the invalid blob emits nothing.
        assert!(
            platform
                .auth_states
                .lock()
                .expect("auth state list mutex poisoned")
                .is_empty()
        );
    }

    #[test]
    fn session_store_sync_clears_unreadable_blob() {
        let session_clears = Arc::new(Mutex::new(0));
        let host = PlatformRuntimeHost::new_compat(
            Arc::new(StubPlatform {
                session_error: Some("storage unavailable"),
                session_clears: session_clears.clone(),
                ..Default::default()
            }),
            test_spawner(),
        );
        host.session_state().set_session(sso_session_info());

        host.start_session_store_sync(test_spawner());
        wait_until(
            || *session_clears.lock().unwrap() == 1,
            "session store sync did not clear unreadable blob",
        );

        assert!(host.session_state().current().is_none());
        assert_eq!(*session_clears.lock().unwrap(), 1);
    }

    /// A persistently failing read clears the backing store once for the
    /// initial sync tick. Further clears require explicit host notifications.
    #[test]
    fn session_store_sync_clears_once_on_initial_persistent_read_error() {
        let session_clears = Arc::new(Mutex::new(0));
        let host = PlatformRuntimeHost::new_compat(
            Arc::new(StubPlatform {
                session_error: Some("storage unavailable"),
                session_clears: session_clears.clone(),
                ..Default::default()
            }),
            test_spawner(),
        );
        host.session_state().set_session(sso_session_info());

        host.start_session_store_sync(test_spawner());

        wait_until(
            || *session_clears.lock().unwrap() == 1,
            "clear_stored_session was never called",
        );
        assert_eq!(*session_clears.lock().unwrap(), 1);
        assert!(host.session_state().current().is_none());
    }

    #[test]
    fn disconnect_submits_disconnected_message_best_effort() {
        let platform = Arc::new(StubPlatform::default());
        let host = PlatformRuntimeHost::new_compat(platform.clone(), test_spawner());
        let session = sso_session_info();
        host.session_state().set_session(session.clone());

        futures::executor::block_on(host.disconnect());

        assert!(host.session_state().current().is_none());
        assert_eq!(
            *platform
                .session_clears
                .lock()
                .expect("session clear counter mutex poisoned"),
            1
        );
        let message = submitted_remote_message(&platform, &session);
        assert_eq!(message.message_id, "truapi:sso:disconnect");
        assert!(matches!(
            message.data,
            RemoteMessageData::V1(RemoteMessageV1::Disconnected)
        ));
    }

    #[test]
    fn disconnect_clears_session_store_and_broadcasts_disconnected() {
        let platform = Arc::new(StubPlatform::default());
        let host = PlatformRuntimeHost::new_compat(platform.clone(), test_spawner());
        host.session_state().set_session(sso_session_info());
        platform
            .local_storage
            .lock()
            .expect("local storage mutex poisoned")
            .insert(
                core_storage_test_key(CoreStorageKey::PairingDeviceIdentity),
                vec![1, 2, 3],
            );
        let mut statuses = host.session_state().subscribe();
        assert_eq!(
            futures::executor::block_on(statuses.next()).unwrap(),
            HostAccountConnectionStatusSubscribeItem::V1(
                v01::HostAccountConnectionStatusSubscribeItem::Connected
            )
        );

        futures::executor::block_on(host.disconnect());

        assert!(host.session_state().current().is_none());
        assert_eq!(
            *platform
                .session_clears
                .lock()
                .expect("session clear counter mutex poisoned"),
            1
        );
        assert!(
            !platform
                .local_storage
                .lock()
                .expect("local storage mutex poisoned")
                .contains_key(&core_storage_test_key(
                    CoreStorageKey::PairingDeviceIdentity
                )),
            "logout must rotate the pairing device identity so stale statement-store responses cannot be replayed on the next login"
        );
        assert_eq!(
            futures::executor::block_on(statuses.next()).unwrap(),
            HostAccountConnectionStatusSubscribeItem::V1(
                v01::HostAccountConnectionStatusSubscribeItem::Disconnected
            )
        );
        // `set_session` bypasses the auth state cell, so the cell never left
        // `Disconnected` and the logout emits nothing new.
        assert!(
            platform
                .auth_states
                .lock()
                .expect("auth state list mutex poisoned")
                .is_empty()
        );
    }

    #[test]
    fn disconnect_emits_disconnected_auth_state_after_store_sync_connected() {
        let stored = sso_session_info();
        let platform = Arc::new(StubPlatform {
            session_blob: Some(crate::host_logic::session::encode_persisted_session(
                &stored,
            )),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new_compat(platform.clone(), test_spawner());
        host.start_session_store_sync(test_spawner());
        wait_until(
            || {
                platform
                    .auth_states
                    .lock()
                    .expect("auth state list mutex poisoned")
                    .len()
                    == 1
            },
            "session store sync did not emit connected auth state",
        );

        futures::executor::block_on(host.disconnect());

        assert_eq!(
            *platform
                .auth_states
                .lock()
                .expect("auth state list mutex poisoned"),
            vec![
                AuthState::Connected(connected_session_ui_info(&stored)),
                AuthState::Disconnected,
            ]
        );
    }

    #[test]
    fn disconnect_tolerates_repeated_logout_when_already_disconnected() {
        let platform = Arc::new(StubPlatform::default());
        let host = PlatformRuntimeHost::new_compat(platform.clone(), test_spawner());

        futures::executor::block_on(host.disconnect());
        futures::executor::block_on(host.disconnect());

        assert!(host.session_state().current().is_none());
        assert_eq!(
            *platform
                .session_clears
                .lock()
                .expect("session clear counter mutex poisoned"),
            2
        );
        assert!(platform.sent_rpc.lock().unwrap().is_empty());
    }

    #[test]
    fn disconnect_notifies_pending_sso_waiters() {
        let platform = Arc::new(StubPlatform::default());
        let host = PlatformRuntimeHost::new_compat(platform, test_spawner());
        let (_waiter_id, disconnect) = host.session_disconnects.subscribe();

        futures::executor::block_on(host.disconnect());

        assert_eq!(
            futures::executor::block_on(disconnect).unwrap(),
            SSO_LOCAL_DISCONNECT_REASON
        );
    }

    #[test]
    fn permissions_grants_and_caches() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        let cx = CallContext::new();
        let request = HostDevicePermissionRequest::V1(v01::HostDevicePermissionRequest::Camera);
        let response =
            futures::executor::block_on(host.request_device_permission(&cx, request)).unwrap();
        let HostDevicePermissionResponse::V1(inner) = response;
        assert!(inner.granted);
    }

    #[test]
    fn feature_supported_encodes_response_to_known_bytes() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        let cx = CallContext::new();
        let request = HostFeatureSupportedRequest::V1(v01::HostFeatureSupportedRequest::Chain {
            genesis_hash: vec![0u8; 32],
        });
        let response = futures::executor::block_on(host.feature_supported(&cx, request)).unwrap();
        // [V1 variant=0][supported=1]
        assert_eq!(response.encode(), vec![0x00, 0x01]);
    }
}
