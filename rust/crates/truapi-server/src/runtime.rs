//! `PlatformRuntimeHost<P>` adapts a [`truapi_platform::Platform`] into the
//! typed `truapi::api::*` host traits the generated dispatcher routes to.
//!
//! Most methods are straight delegations to the platform; the rest carry
//! host-agnostic logic owned by the core (the chainHead-v1 runtime behind
//! the Chain surface, `dotns` URL parsing for `navigate_to`, and the
//! permission cache layer). Methods with no platform backing return
//! `CallError::unavailable()`.

use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use crate::chain_runtime::{
    ChainRuntime, RuntimeChainProvider, RuntimeFailure, RuntimeFailureKind,
};
use crate::host_logic::dotns::{NavigateDecision, parse_navigate};
use crate::host_logic::entropy::derive_product_entropy_from_source;
use crate::host_logic::features::feature_supported;
use crate::host_logic::identity::{
    PeopleIdentity, decode_people_identity, resources_consumers_storage_key,
};
use crate::host_logic::permissions::{Decision, PermissionsService};
use crate::host_logic::product_account::{
    derive_product_public_key, is_product_identifier, normalize_product_identifier,
    product_public_key_to_address,
};
use crate::host_logic::session::{
    SessionInfo, SessionState, SsoSessionInfo, decode_persisted_session, encode_persisted_session,
};
use crate::host_logic::sso_messages::{
    OnExistingAllowancePolicy, RemoteMessage, RemoteMessageData, RemoteMessageV1,
    SsoAllocationOutcome, SsoRemoteResponse, SsoSessionStatement, alias_request_message,
    build_outgoing_request_statement, create_transaction_message, decode_sso_session_statement,
    resource_allocation_message, sign_payload_message, sign_raw_message,
};
use crate::host_logic::sso_pairing::{
    EncryptedHandshakeResponseV2, PairingDeviceIdentity, VersionedHandshakeResponse,
    create_pairing_bootstrap_from_identity, decode_app_handshake_data,
    decrypt_v2_handshake_response, establish_sso_session_info, generate_pairing_device_identity,
};
use crate::host_logic::statement_store::{
    MAX_MATCH_ALL_TOPICS, MAX_MATCH_ANY_TOPICS, TopicFilterKind, decode_signed_statement,
    decode_verified_statement_data, parse_new_statements, parse_submit_ack, parse_subscribe_ack,
    sign_statement_fields, signed_statement_to_scale, statement_fields_from_v01,
    statement_proof_to_v01, submit_statement_request, subscribe_match_all_request,
    subscribe_match_any_request, unsubscribe_request,
};
use crate::subscription::Spawner;

use futures::channel::oneshot;
use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt, pin_mut};
use parity_scale_codec::{Decode, Encode};
use tracing::{debug, info, instrument, warn};
use truapi::api::{
    Account, Chain, Chat, CoinPayment, Entropy, LocalStorage, Notifications, Payment, Permissions,
    Preimage, ResourceAllocation, Signing, StatementStore, System, Theme,
};
use truapi::v01;
use truapi::v01::{
    OperationStartedResult, RemoteChainHeadFollowItem as V01RemoteChainHeadFollowItem,
    StorageQueryType,
};
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
use truapi::versioned::statement_store::{
    RemoteStatementStoreCreateProofAuthorizedError,
    RemoteStatementStoreCreateProofAuthorizedRequest,
    RemoteStatementStoreCreateProofAuthorizedResponse, RemoteStatementStoreCreateProofError,
    RemoteStatementStoreCreateProofRequest, RemoteStatementStoreCreateProofResponse,
    RemoteStatementStoreSubmitError, RemoteStatementStoreSubmitRequest,
    RemoteStatementStoreSubscribeError, RemoteStatementStoreSubscribeItem,
    RemoteStatementStoreSubscribeRequest,
};
use truapi::versioned::system::{
    HostFeatureSupportedError, HostFeatureSupportedRequest, HostFeatureSupportedResponse,
    HostNavigateToError, HostNavigateToRequest, HostNavigateToResponse,
};
use truapi::versioned::theme::HostThemeSubscribeItem;
use truapi::{CallContext, CallError, CancellationToken, Subscription};
use truapi_platform::{
    ChainProvider as PlatformChainProvider, JsonRpcConnection, Navigation as PlatformNavigation,
    Notifications as PlatformNotifications, PairingPresenter as PlatformPairingPresenter, Platform,
    PreimageHost as PlatformPreimageHost, RuntimeConfig, SessionStore as PlatformSessionStore,
    Storage as PlatformStorage, ThemeHost as PlatformThemeHost,
    UserConfirmation as PlatformUserConfirmation,
};

const PAIRING_SUBSCRIBE_REQUEST_ID: &str = "truapi:sso-pairing:1";
const PAIRING_DEVICE_IDENTITY_STORAGE_KEY: &str = "truapi:sso-device-identity:v1";
const DEFAULT_SSO_STATEMENT_EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;
const DEFAULT_SSO_RESPONSE_TIMEOUT: Duration = Duration::from_secs(180);
#[cfg(not(test))]
const PAIRING_QUERY_INTERVAL: Duration = Duration::from_secs(2);
#[cfg(test)]
const PAIRING_QUERY_INTERVAL: Duration = Duration::from_millis(1);
#[cfg(not(test))]
const PAIRING_QUERY_TIMEOUT_TICKS: u8 = 15;
#[cfg(test)]
const PAIRING_QUERY_TIMEOUT_TICKS: u8 = 10;
const SSO_LOCAL_DISCONNECT_REASON: &str = "SSO session disconnected";
const SSO_PEER_DISCONNECT_REASON: &str = "SSO peer disconnected";

#[derive(Default)]
struct SessionDisconnects {
    inner: Mutex<SessionDisconnectsInner>,
}

#[derive(Default)]
struct SessionDisconnectsInner {
    next_id: u64,
    waiters: Vec<(u64, oneshot::Sender<String>)>,
}

impl SessionDisconnects {
    fn subscribe(&self) -> (u64, oneshot::Receiver<String>) {
        let (tx, rx) = oneshot::channel();
        let mut inner = self
            .inner
            .lock()
            .expect("session disconnect mutex poisoned");
        inner.next_id = inner.next_id.wrapping_add(1);
        let id = inner.next_id;
        inner.waiters.push((id, tx));
        (id, rx)
    }

    fn unsubscribe(&self, id: u64) {
        self.inner
            .lock()
            .expect("session disconnect mutex poisoned")
            .waiters
            .retain(|(waiter_id, _)| *waiter_id != id);
    }

    fn notify(&self, reason: &'static str) {
        let waiters = {
            let mut inner = self
                .inner
                .lock()
                .expect("session disconnect mutex poisoned");
            std::mem::take(&mut inner.waiters)
        };
        for (_, waiter) in waiters {
            let _ = waiter.send(reason.to_string());
        }
    }
}

/// Adapter that exposes a [`truapi_platform::Platform`] through the
/// `truapi::api::*` trait set the generated dispatcher routes to.
pub struct PlatformRuntimeHost<P> {
    platform: Arc<P>,
    runtime_config: RuntimeConfig,
    /// chainHead-v1 state machine. The provider adapter forwards
    /// [`PlatformChainProvider::connect`] into the json-rpc layer.
    chain: ChainRuntime,
    /// Currently-paired session, pushed by the host through a side channel.
    /// Account-management subscriptions read from this in lieu of round-tripping
    /// a callback on every connection-status query.
    session_state: Arc<SessionState>,
    session_disconnects: Arc<SessionDisconnects>,
}

impl<P> PlatformRuntimeHost<P> {
    /// Wrap a platform implementation. The runtime takes ownership via `Arc`.
    /// `spawner` is used by the embedded chain runtime to drive json-rpc
    /// response loops and follow-setup futures.
    pub fn new(platform: Arc<P>, runtime_config: RuntimeConfig, spawner: Spawner) -> Self
    where
        P: Platform + 'static,
    {
        let chain_provider = Self::chain_provider(platform.clone());
        Self {
            platform,
            runtime_config,
            chain: ChainRuntime::new(chain_provider, spawner),
            session_state: SessionState::new(),
            session_disconnects: Arc::new(SessionDisconnects::default()),
        }
    }

    /// Compatibility constructor used only by tests that do not exercise
    /// product-scoped behavior.
    #[cfg(test)]
    fn new_compat(platform: Arc<P>, spawner: Spawner) -> Self
    where
        P: Platform + 'static,
    {
        Self::new(
            platform,
            RuntimeConfig {
                product_label: "unknown".to_string(),
                product_id: "unknown.dot".to_string(),
                site_id: "test".to_string(),
                host_name: "Polkadot Web".to_string(),
                host_icon: Some("https://example.invalid/dotli.png".to_string()),
                host_version: None,
                platform_type: None,
                platform_version: None,
                people_chain_genesis_hash: [0; 32],
                pairing_deeplink_scheme: truapi_platform::PairingDeeplinkScheme::PolkadotApp,
            },
            spawner,
        )
    }

    /// Chain provider backing the chainHead-v1 runtime. Without the `smoldot`
    /// feature, chain access routes through the platform's `ChainProvider`.
    #[cfg(not(feature = "smoldot"))]
    fn chain_provider(platform: Arc<P>) -> Arc<dyn RuntimeChainProvider>
    where
        P: Platform + 'static,
    {
        Arc::new(PlatformChainRuntimeProvider { platform })
    }

    /// With the `smoldot` feature, the embedded light client owns chain
    /// access, falling back to the platform's `ChainProvider` only if the
    /// client fails to start.
    #[cfg(feature = "smoldot")]
    fn chain_provider(platform: Arc<P>) -> Arc<dyn RuntimeChainProvider>
    where
        P: Platform + 'static,
    {
        match crate::smoldot_provider::SmoldotChainProvider::with_bundled_specs() {
            Ok(provider) => Arc::new(provider),
            Err(_err) => Arc::new(PlatformChainRuntimeProvider { platform }),
        }
    }

    /// Clone of the shared session-state holder used by core subscriptions
    /// and tests. Real host lifecycle flows through `SessionStore` and
    /// `disconnect`.
    pub fn session_state(&self) -> Arc<SessionState> {
        self.session_state.clone()
    }

    /// Start syncing the in-memory session from the host-global session store.
    /// The store emits coarse ticks; each tick triggers a fresh read so same-
    /// runtime writes and cross-runtime logout/re-pair take the same path.
    #[instrument(skip_all, fields(runtime.method = "session_store.sync"))]
    pub(crate) fn start_session_store_sync(&self, spawner: Spawner)
    where
        P: Platform + 'static,
    {
        let platform = self.platform.clone();
        let chain = self.chain.clone();
        let runtime_config = self.runtime_config.clone();
        let session_state = self.session_state.clone();
        spawner(Box::pin(async move {
            let mut ticks = PlatformSessionStore::subscribe_session_store(platform.as_ref());
            while let Some(tick) = ticks.next().await {
                if tick.is_err() {
                    continue;
                }
                match PlatformSessionStore::read_session(platform.as_ref()).await {
                    Ok(Some(blob)) => match decode_persisted_session(&blob) {
                        Ok(session) => {
                            let resolved = resolve_session_identity_with_chain(
                                &chain,
                                runtime_config.people_chain_genesis_hash,
                                session,
                            )
                            .await;
                            if encode_persisted_session(&resolved) != blob {
                                let _ = PlatformSessionStore::write_session(
                                    platform.as_ref(),
                                    encode_persisted_session(&resolved),
                                )
                                .await;
                            }
                            session_state.set_session(resolved);
                        }
                        Err(_) => {
                            session_state.clear_session();
                            let _ = PlatformSessionStore::clear_session(platform.as_ref()).await;
                        }
                    },
                    Ok(None) => session_state.clear_session(),
                    Err(_) => {
                        session_state.clear_session();
                        let _ = PlatformSessionStore::clear_session(platform.as_ref()).await;
                    }
                }
            }
        }));
    }

    /// Core-owned logout/disconnect path. It best-effort notifies the SSO
    /// peer, then clears in-memory and persisted session state regardless of
    /// any transport failure.
    #[instrument(skip_all, fields(runtime.method = "account.disconnect"))]
    pub(crate) async fn disconnect(&self)
    where
        P: Platform + 'static,
    {
        self.session_disconnects.notify(SSO_LOCAL_DISCONNECT_REASON);
        let session = self.session_state.current();
        if let Some(session) = session.as_ref() {
            let _ = self.submit_sso_disconnected(session).await;
        }
        self.clear_disconnected_session().await;
    }

    /// Static product/host configuration for this runtime instance.
    pub fn runtime_config(&self) -> &RuntimeConfig {
        &self.runtime_config
    }

    fn is_product_account_valid_for_caller(&self, dot_ns_identifier: &str) -> bool {
        let dot_ns_identifier = normalize_product_identifier(dot_ns_identifier);
        let product_id = normalize_product_identifier(&self.runtime_config.product_id);
        if self.runtime_config.product_label.starts_with("localhost:") {
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
    async fn clear_disconnected_session(&self)
    where
        P: Platform + 'static,
    {
        debug!("clearing disconnected SSO session state");
        self.session_state.clear_session();
        let _ = PlatformSessionStore::clear_session(self.platform.as_ref()).await;
        let _ = PlatformStorage::clear(
            self.platform.as_ref(),
            PAIRING_DEVICE_IDENTITY_STORAGE_KEY.to_string(),
        )
        .await;
    }
}

impl<P> PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    #[instrument(skip_all, fields(runtime.method = "permissions.chain_submit_decision"))]
    async fn chain_submit_decision(&self) -> Result<Decision, String> {
        let service = PermissionsService::new(self.platform.as_ref(), self.platform.as_ref());
        service
            .check_or_prompt_remote(v01::RemotePermissionRequest {
                permission: v01::RemotePermission::ChainSubmit,
            })
            .await
            .map_err(|err| format!("permission storage failed: {err:?}"))
    }

    #[instrument(skip_all, fields(runtime.method = "session.identity.resolve"))]
    async fn resolve_session_identity(&self, session: SessionInfo) -> SessionInfo {
        resolve_session_identity_with_chain(
            &self.chain,
            self.runtime_config.people_chain_genesis_hash,
            session,
        )
        .await
    }

    #[instrument(skip_all, fields(runtime.method = "sso.disconnect.submit"))]
    async fn submit_sso_disconnected(&self, session: &SessionInfo) -> Result<(), String> {
        let sso = session
            .sso
            .as_ref()
            .ok_or_else(|| "No SSO session state".to_string())?;
        let message_id = "truapi:sso:disconnect".to_string();
        let message = RemoteMessage {
            message_id: message_id.clone(),
            data: RemoteMessageData::V1(RemoteMessageV1::Disconnected),
        };
        let statement = build_outgoing_request_statement(
            sso,
            message_id.clone(),
            vec![message],
            fresh_statement_expiry(),
        )?;
        let connection = PlatformChainProvider::connect(
            self.platform.as_ref(),
            self.runtime_config.people_chain_genesis_hash.to_vec(),
        )
        .await
        .map_err(|err| format!("SSO statement-store connect failed: {err:?}"))?;
        connection.send(submit_statement_request(
            &format!("truapi:sso-submit:{message_id}"),
            &statement,
        ));
        Ok(())
    }

    #[instrument(skip_all, fields(runtime.method = "sso.remote_message.submit", action = action))]
    async fn submit_sso_remote_message(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        action: &str,
        message: RemoteMessage,
    ) -> Result<SsoRemoteResponse, String> {
        self.submit_sso_remote_message_with_timeout(
            cx,
            session,
            action,
            message,
            Some(DEFAULT_SSO_RESPONSE_TIMEOUT),
        )
        .await
    }

    #[instrument(skip_all, fields(runtime.method = "sso.remote_message.submit_without_timeout", action = action))]
    async fn submit_sso_remote_message_without_timeout(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        action: &str,
        message: RemoteMessage,
    ) -> Result<SsoRemoteResponse, String> {
        self.submit_sso_remote_message_with_timeout(cx, session, action, message, None)
            .await
    }

    #[instrument(skip_all, fields(runtime.method = "sso.remote_message.submit_with_timeout", action = action))]
    async fn submit_sso_remote_message_with_timeout(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        action: &str,
        message: RemoteMessage,
        timeout: Option<Duration>,
    ) -> Result<SsoRemoteResponse, String> {
        let sso = session
            .sso
            .as_ref()
            .ok_or_else(|| "No SSO session state".to_string())?;
        let message_id = sso_message_id(cx, action);
        let statement = build_outgoing_request_statement(
            sso,
            message_id.clone(),
            vec![message],
            fresh_statement_expiry(),
        )?;
        let connection = PlatformChainProvider::connect(
            self.platform.as_ref(),
            self.runtime_config.people_chain_genesis_hash.to_vec(),
        )
        .await
        .map_err(|err| format!("SSO statement-store connect failed: {err:?}"))?;
        let own_subscription_request_id = format!("truapi:sso-sub-own:{message_id}");
        let peer_subscription_request_id = format!("truapi:sso-sub-peer:{message_id}");
        let submit_request_id = format!("truapi:sso-submit:{message_id}");
        connection.send(subscribe_match_all_request(
            &own_subscription_request_id,
            &[sso.session_id_own],
        ));
        connection.send(subscribe_match_all_request(
            &peer_subscription_request_id,
            &[sso.session_id_peer],
        ));
        connection.send(submit_statement_request(&submit_request_id, &statement));
        debug!(action, %message_id, "submitted SSO remote message, awaiting response");
        let responses = connection.responses();
        let subscription_guard = SsoRemoteSubscriptionGuard::new(
            connection,
            own_subscription_request_id.clone(),
            peer_subscription_request_id.clone(),
        );
        let (disconnect_waiter_id, disconnect) = self.session_disconnects.subscribe();
        let result = wait_for_sso_remote_response(
            responses,
            SsoRemoteResponseWait {
                session: sso,
                own_subscription_request_id: &own_subscription_request_id,
                peer_subscription_request_id: &peer_subscription_request_id,
                submit_request_id: &submit_request_id,
                statement_request_id: &message_id,
                remote_message_id: &message_id,
                timeout,
                disconnect: Some(disconnect),
                own_remote_subscription_id: subscription_guard.own_remote_subscription_id(),
                peer_remote_subscription_id: subscription_guard.peer_remote_subscription_id(),
            },
        )
        .await;
        self.session_disconnects.unsubscribe(disconnect_waiter_id);
        match &result {
            Ok(_) => debug!(action, %message_id, "SSO remote response received"),
            Err(reason) => warn!(action, %message_id, %reason, "SSO remote message failed"),
        }
        if matches!(&result, Err(reason) if reason == SSO_PEER_DISCONNECT_REASON) {
            self.session_disconnects.notify(SSO_PEER_DISCONNECT_REASON);
            self.clear_disconnected_session().await;
        }
        result
    }

    fn validate_legacy_address_signer(
        &self,
        session: &SessionInfo,
        signer: &str,
    ) -> Result<(), v01::HostSignPayloadError> {
        let public_key = self
            .legacy_slot_zero_public_key(session)
            .map_err(|reason| v01::HostSignPayloadError::Unknown { reason })?;
        let expected = product_public_key_to_address(public_key);
        if expected == signer {
            Ok(())
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

struct SsoRemoteResponseWait<'a> {
    session: &'a SsoSessionInfo,
    own_subscription_request_id: &'a str,
    peer_subscription_request_id: &'a str,
    submit_request_id: &'a str,
    statement_request_id: &'a str,
    remote_message_id: &'a str,
    timeout: Option<Duration>,
    disconnect: Option<oneshot::Receiver<String>>,
    own_remote_subscription_id: SharedRemoteSubscriptionId,
    peer_remote_subscription_id: SharedRemoteSubscriptionId,
}

struct SsoRemoteResponseTarget<'a> {
    session: &'a SsoSessionInfo,
    own_subscription_request_id: &'a str,
    peer_subscription_request_id: &'a str,
    submit_request_id: &'a str,
    statement_request_id: &'a str,
    remote_message_id: &'a str,
    own_remote_subscription_slot: SharedRemoteSubscriptionId,
    peer_remote_subscription_slot: SharedRemoteSubscriptionId,
}

type SharedRemoteSubscriptionId = Arc<Mutex<Option<String>>>;

struct SsoRemoteSubscriptionGuard {
    connection: Box<dyn JsonRpcConnection>,
    own_unsubscribe_request_id: String,
    peer_unsubscribe_request_id: String,
    own_remote_subscription_id: SharedRemoteSubscriptionId,
    peer_remote_subscription_id: SharedRemoteSubscriptionId,
}

struct PairingSubscriptionGuard {
    connection: Box<dyn JsonRpcConnection>,
    unsubscribe_request_id: String,
    remote_subscription_id: SharedRemoteSubscriptionId,
}

impl PairingSubscriptionGuard {
    fn new(connection: Box<dyn JsonRpcConnection>) -> Self {
        Self {
            connection,
            unsubscribe_request_id: format!("{PAIRING_SUBSCRIBE_REQUEST_ID}:unsubscribe"),
            remote_subscription_id: Arc::new(Mutex::new(None)),
        }
    }

    fn remote_subscription_id(&self) -> SharedRemoteSubscriptionId {
        self.remote_subscription_id.clone()
    }
}

impl Drop for PairingSubscriptionGuard {
    fn drop(&mut self) {
        if let Some(remote_subscription_id) = self
            .remote_subscription_id
            .lock()
            .expect("pairing subscription id mutex poisoned")
            .as_ref()
        {
            self.connection.send(unsubscribe_request(
                &self.unsubscribe_request_id,
                remote_subscription_id,
            ));
        }
    }
}

impl SsoRemoteSubscriptionGuard {
    fn new(
        connection: Box<dyn JsonRpcConnection>,
        own_subscription_request_id: String,
        peer_subscription_request_id: String,
    ) -> Self {
        Self {
            connection,
            own_unsubscribe_request_id: format!("{own_subscription_request_id}:unsubscribe"),
            peer_unsubscribe_request_id: format!("{peer_subscription_request_id}:unsubscribe"),
            own_remote_subscription_id: Arc::new(Mutex::new(None)),
            peer_remote_subscription_id: Arc::new(Mutex::new(None)),
        }
    }

    fn own_remote_subscription_id(&self) -> SharedRemoteSubscriptionId {
        self.own_remote_subscription_id.clone()
    }

    fn peer_remote_subscription_id(&self) -> SharedRemoteSubscriptionId {
        self.peer_remote_subscription_id.clone()
    }
}

impl Drop for SsoRemoteSubscriptionGuard {
    fn drop(&mut self) {
        if let Some(remote_subscription_id) = self
            .own_remote_subscription_id
            .lock()
            .expect("SSO own subscription id mutex poisoned")
            .as_ref()
        {
            self.connection.send(unsubscribe_request(
                &self.own_unsubscribe_request_id,
                remote_subscription_id,
            ));
        }
        if let Some(remote_subscription_id) = self
            .peer_remote_subscription_id
            .lock()
            .expect("SSO peer subscription id mutex poisoned")
            .as_ref()
        {
            self.connection.send(unsubscribe_request(
                &self.peer_unsubscribe_request_id,
                remote_subscription_id,
            ));
        }
    }
}

#[instrument(skip_all, fields(runtime.method = "sso.remote_response.wait"))]
async fn wait_for_sso_remote_response(
    responses: BoxStream<'static, String>,
    wait: SsoRemoteResponseWait<'_>,
) -> Result<SsoRemoteResponse, String> {
    let timeout_reason = wait.timeout.map(|timeout| {
        format!(
            "SSO response timed out after {} for {}",
            format_timeout_duration(timeout),
            wait.remote_message_id
        )
    });
    let response = wait_for_sso_remote_response_inner(
        responses,
        SsoRemoteResponseTarget {
            session: wait.session,
            own_subscription_request_id: wait.own_subscription_request_id,
            peer_subscription_request_id: wait.peer_subscription_request_id,
            submit_request_id: wait.submit_request_id,
            statement_request_id: wait.statement_request_id,
            remote_message_id: wait.remote_message_id,
            own_remote_subscription_slot: wait.own_remote_subscription_id.clone(),
            peer_remote_subscription_slot: wait.peer_remote_subscription_id.clone(),
        },
    )
    .fuse();
    let timeout = async move {
        match (wait.timeout, timeout_reason) {
            (Some(timeout), Some(reason)) => {
                futures_timer::Delay::new(timeout).await;
                reason
            }
            _ => futures::future::pending::<String>().await,
        }
    }
    .fuse();
    let disconnect = async move {
        match wait.disconnect {
            Some(rx) => rx
                .await
                .unwrap_or_else(|_| SSO_LOCAL_DISCONNECT_REASON.to_string()),
            None => futures::future::pending::<String>().await,
        }
    }
    .fuse();
    pin_mut!(response, timeout, disconnect);
    futures::select! {
        result = response => result,
        reason = timeout => Err(reason),
        reason = disconnect => Err(reason),
    }
}

#[instrument(skip_all, fields(runtime.method = "sso.remote_response.wait_inner"))]
async fn wait_for_sso_remote_response_inner(
    mut responses: BoxStream<'static, String>,
    target: SsoRemoteResponseTarget<'_>,
) -> Result<SsoRemoteResponse, String> {
    let mut own_remote_subscription_id: Option<String> = None;
    let mut peer_remote_subscription_id: Option<String> = None;
    let mut request_accepted = false;
    let mut pending_remote_response = None;

    while let Some(frame) = responses.next().await {
        if own_remote_subscription_id.is_none()
            && let Some(id) = parse_subscribe_ack(&frame, target.own_subscription_request_id)
                .map_err(|err| err.to_string())?
        {
            *target
                .own_remote_subscription_slot
                .lock()
                .expect("SSO own subscription id mutex poisoned") = Some(id.clone());
            own_remote_subscription_id = Some(id);
            continue;
        }
        if peer_remote_subscription_id.is_none()
            && let Some(id) = parse_subscribe_ack(&frame, target.peer_subscription_request_id)
                .map_err(|err| err.to_string())?
        {
            *target
                .peer_remote_subscription_slot
                .lock()
                .expect("SSO peer subscription id mutex poisoned") = Some(id.clone());
            peer_remote_subscription_id = Some(id);
            continue;
        }

        let submit_ack = parse_submit_ack(&frame, target.submit_request_id)
            .map_err(|err| format!("SSO statement submit failed: {err}"))?;
        if submit_ack.is_some() {
            continue;
        }

        let Some(page) = parse_new_statements(&frame).map_err(|err| err.to_string())? else {
            continue;
        };
        if !subscription_id_matches(
            &page.remote_subscription_id,
            own_remote_subscription_id.as_deref(),
            peer_remote_subscription_id.as_deref(),
        ) {
            continue;
        }

        for statement in page.statements {
            match decode_sso_session_statement(
                target.session,
                &statement,
                target.statement_request_id,
                target.remote_message_id,
            )? {
                Some(SsoSessionStatement::RequestAccepted) => {
                    request_accepted = true;
                    if let Some(response) = pending_remote_response.take() {
                        return Ok(response);
                    }
                }
                Some(SsoSessionStatement::RemoteResponse(response)) => {
                    if request_accepted {
                        return Ok(response);
                    }
                    pending_remote_response = Some(response);
                }
                Some(SsoSessionStatement::Disconnected) => {
                    return Err(SSO_PEER_DISCONNECT_REASON.to_string());
                }
                None => {}
            }
        }
    }

    Err(format!(
        "SSO response stream ended before response for {}",
        target.remote_message_id
    ))
}

fn format_timeout_duration(duration: Duration) -> String {
    if duration.subsec_millis() == 0 {
        format!("{}s", duration.as_secs())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

fn subscription_id_matches(
    remote_subscription_id: &str,
    own_remote_subscription_id: Option<&str>,
    peer_remote_subscription_id: Option<&str>,
) -> bool {
    own_remote_subscription_id == Some(remote_subscription_id)
        || peer_remote_subscription_id == Some(remote_subscription_id)
}

fn sso_message_id(cx: &CallContext, action: &str) -> String {
    if cx.request_id().is_empty() {
        format!("truapi:sso:{action}")
    } else {
        cx.request_id().to_string()
    }
}

fn fresh_statement_expiry() -> u64 {
    let timestamp = current_unix_secs().saturating_add(DEFAULT_SSO_STATEMENT_EXPIRY_SECS);
    timestamp << 32
}

#[cfg(not(target_arch = "wasm32"))]
fn current_unix_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(target_arch = "wasm32")]
fn current_unix_secs() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}

/// Adapter from `truapi_platform::ChainProvider` into the
/// [`RuntimeChainProvider`] surface the chain runtime expects.
/// Reuses the platform-supplied json-rpc connection and converts the
/// platform `GenericError` into a `RuntimeFailure::Unavailable`.
struct PlatformChainRuntimeProvider<P> {
    platform: Arc<P>,
}

#[async_trait::async_trait]
impl<P> RuntimeChainProvider for PlatformChainRuntimeProvider<P>
where
    P: Platform + 'static,
{
    #[instrument(skip_all, fields(runtime.method = "chain.provider.connect"))]
    async fn connect(
        &self,
        genesis_hash: Vec<u8>,
    ) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
        PlatformChainProvider::connect(self.platform.as_ref(), genesis_hash)
            .await
            .map(Arc::from)
            .map_err(|_| RuntimeFailure::unavailable("remote_chain_connect"))
    }
}

fn runtime_failure_to_call_error<E>(failure: RuntimeFailure) -> CallError<E> {
    match failure.kind() {
        RuntimeFailureKind::Unavailable => CallError::HostFailure {
            reason: failure.reason(),
        },
        RuntimeFailureKind::HostFailure => CallError::HostFailure {
            reason: failure.reason(),
        },
    }
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

impl<P> System for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    #[instrument(skip_all, fields(runtime.method = "system.feature_supported"))]
    async fn feature_supported(
        &self,
        _cx: &CallContext,
        request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, CallError<HostFeatureSupportedError>> {
        feature_supported(self.platform.as_ref(), request)
            .await
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
        PlatformNavigation::navigate_to(self.platform.as_ref(), resolved)
            .await
            .map(|()| HostNavigateToResponse::V1)
            .map_err(|err| CallError::Domain(HostNavigateToError::V1(err)))
    }
}

// ---------------------------------------------------------------------------
// Permissions
// ---------------------------------------------------------------------------

impl<P> Permissions for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    #[instrument(skip_all, fields(runtime.method = "permissions.request_device_permission"))]
    async fn request_device_permission(
        &self,
        _cx: &CallContext,
        request: HostDevicePermissionRequest,
    ) -> Result<HostDevicePermissionResponse, CallError<HostDevicePermissionError>> {
        let HostDevicePermissionRequest::V1(inner) = request;
        let service = PermissionsService::new(self.platform.as_ref(), self.platform.as_ref());
        match service.check_or_prompt_device(inner).await {
            Ok(decision) => Ok(HostDevicePermissionResponse::V1(
                v01::HostDevicePermissionResponse {
                    granted: decision == Decision::Granted,
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
        let service = PermissionsService::new(self.platform.as_ref(), self.platform.as_ref());
        match service.check_or_prompt_remote(inner).await {
            Ok(decision) => Ok(RemotePermissionResponse::V1(
                v01::RemotePermissionResponse {
                    granted: decision == Decision::Granted,
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

impl<P> LocalStorage for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    #[instrument(skip_all, fields(runtime.method = "local_storage.read"))]
    async fn read(
        &self,
        _cx: &CallContext,
        request: HostLocalStorageReadRequest,
    ) -> Result<HostLocalStorageReadResponse, CallError<HostLocalStorageReadError>> {
        let HostLocalStorageReadRequest::V1(v01::HostLocalStorageReadRequest { key }) = request;
        PlatformStorage::read(self.platform.as_ref(), key)
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
        PlatformStorage::write(self.platform.as_ref(), key, value)
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
        PlatformStorage::clear(self.platform.as_ref(), key)
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

impl<P> Account for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
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
            let confirmed = PlatformUserConfirmation::confirm_account_alias(
                self.platform.as_ref(),
                (
                    product_id.clone(),
                    product_account_id.dot_ns_identifier.clone(),
                )
                    .encode(),
            )
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

        let service = PermissionsService::new(self.platform.as_ref(), self.platform.as_ref());
        let permission = v01::RemotePermissionRequest {
            permission: v01::RemotePermission::UserId,
        };
        match service.check_or_prompt_remote(permission).await {
            Ok(Decision::Granted) => Ok(HostGetUserIdResponse::V1(v01::HostGetUserIdResponse {
                primary_username,
            })),
            Ok(Decision::Denied) => Err(CallError::Domain(HostGetUserIdError::V1(
                v01::HostGetUserIdError::PermissionDenied,
            ))),
            Err(err) => Err(CallError::HostFailure {
                reason: format!("permission storage failed: {err:?}"),
            }),
        }
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
        cx: &CallContext,
        _request: HostRequestLoginRequest,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        if cx.cancel().is_cancelled() {
            return Err(request_login_cancelled());
        }
        if self.session_state.current().is_some() {
            debug!("request_login: already connected, returning early");
            return Ok(HostRequestLoginResponse::V1(
                v01::HostRequestLoginResponse::AlreadyConnected,
            ));
        }

        let pairing_identity = load_or_create_pairing_device_identity(self.platform.as_ref())
            .await
            .map_err(|reason| {
                CallError::Domain(HostRequestLoginError::V1(
                    v01::HostRequestLoginError::Unknown { reason },
                ))
            })?;
        let bootstrap =
            create_pairing_bootstrap_from_identity(&self.runtime_config, pairing_identity)
                .map_err(|err| {
                    CallError::Domain(HostRequestLoginError::V1(
                        v01::HostRequestLoginError::Unknown {
                            reason: err.to_string(),
                        },
                    ))
                })?;
        let presenter = PlatformPairingPresenter::present_pairing(
            self.platform.as_ref(),
            bootstrap.deeplink.clone(),
        )
        .fuse();
        let statement_store_connect = PlatformChainProvider::connect(
            self.platform.as_ref(),
            self.runtime_config.people_chain_genesis_hash.to_vec(),
        )
        .fuse();
        let cancellation = wait_for_call_cancelled(cx.cancel().clone()).fuse();
        pin_mut!(presenter, statement_store_connect, cancellation);

        let statement_store = futures::select! {
            reason = cancellation => return Err(request_login_cancelled_with_reason(reason)),
            presenter_result = presenter => {
                presenter_result.map_err(|err| CallError::HostFailure {
                    reason: format!("pairing presentation failed: {err:?}"),
                })?;
                return Ok(HostRequestLoginResponse::V1(
                    v01::HostRequestLoginResponse::Rejected,
                ));
            }
            connect_result = statement_store_connect => connect_result.map_err(|err| CallError::HostFailure {
                reason: format!("pairing statement-store connect failed: {err:?}"),
            })?,
        };
        info!("presenting pairing QR, waiting for wallet handshake");
        statement_store.send(subscribe_match_all_request(
            PAIRING_SUBSCRIBE_REQUEST_ID,
            &[bootstrap.topic],
        ));
        debug!("subscribed to pairing topic, polling statement store");
        let responses = statement_store.responses();
        let subscription_guard = PairingSubscriptionGuard::new(statement_store);
        let pairing_response = wait_for_v2_pairing_success(
            subscription_guard.connection.as_ref(),
            responses,
            subscription_guard.remote_subscription_id(),
            bootstrap.topic,
            bootstrap.encryption_secret_key,
        )
        .fuse();
        pin_mut!(pairing_response);

        futures::select! {
            reason = cancellation => Err(request_login_cancelled_with_reason(reason)),
            presenter_result = presenter => {
                presenter_result.map_err(|err| CallError::HostFailure {
                    reason: format!("pairing presentation failed: {err:?}"),
                })?;
                info!("pairing presentation closed before handshake, login rejected");
                Ok(HostRequestLoginResponse::V1(
                    v01::HostRequestLoginResponse::Rejected,
                ))
            }
            response_result = pairing_response => {
                let response = response_result.map_err(|reason| CallError::HostFailure {
                    reason,
                })?;
                let sso = establish_sso_session_info(
                    &bootstrap,
                    response.peer_statement_account_id,
                    response.success.sso_enc_pub_key,
                )
                    .map_err(|reason| CallError::HostFailure { reason })?;
                let session = SessionInfo {
                    public_key: response.success.root_account_id,
                    sso: Some(sso),
                    root_entropy_source: Some(response.success.root_entropy_source),
                    identity_account_id: Some(response.success.identity_account_id),
                    lite_username: None,
                    full_username: None,
                };
                let session = self.resolve_session_identity(session).await;
                PlatformSessionStore::write_session(
                    self.platform.as_ref(),
                    encode_persisted_session(&session),
                )
                .await
                .map_err(|err| CallError::HostFailure {
                    reason: format!("session persist failed: {err:?}"),
                })?;
                self.session_state.set_session(session);
                info!("login succeeded, SSO session established");
                Ok(HostRequestLoginResponse::V1(
                    v01::HostRequestLoginResponse::Success,
                ))
            }
        }
    }
}

static IDENTITY_LOOKUP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[instrument(skip_all, fields(runtime.method = "session.identity.resolve_with_chain"))]
async fn resolve_session_identity_with_chain(
    chain: &ChainRuntime,
    people_chain_genesis_hash: [u8; 32],
    mut session: SessionInfo,
) -> SessionInfo {
    if session_has_username(&session) || people_chain_genesis_hash == [0; 32] {
        return session;
    }

    let preferred_account = session.identity_account_id.unwrap_or(session.public_key);
    match lookup_people_identity(chain, people_chain_genesis_hash, preferred_account).await {
        Ok(Some(identity)) => {
            apply_people_identity(&mut session, identity);
            return session;
        }
        Ok(None) => {}
        Err(reason) => warn!(
            account = %hex::encode(preferred_account),
            %reason,
            "People-chain identity lookup failed"
        ),
    }

    if preferred_account != session.public_key {
        match lookup_people_identity(chain, people_chain_genesis_hash, session.public_key).await {
            Ok(Some(identity)) => apply_people_identity(&mut session, identity),
            Ok(None) => {}
            Err(reason) => warn!(
                account = %hex::encode(session.public_key),
                %reason,
                "People-chain root identity lookup failed"
            ),
        }
    }

    session
}

fn session_has_username(session: &SessionInfo) -> bool {
    session
        .full_username
        .as_ref()
        .is_some_and(|value| !value.is_empty())
        || session
            .lite_username
            .as_ref()
            .is_some_and(|value| !value.is_empty())
}

fn apply_people_identity(session: &mut SessionInfo, identity: PeopleIdentity) {
    if identity
        .full_username
        .as_ref()
        .is_some_and(|value| !value.is_empty())
    {
        session.full_username = identity.full_username;
    }
    if identity
        .lite_username
        .as_ref()
        .is_some_and(|value| !value.is_empty())
    {
        session.lite_username = identity.lite_username;
    }
}

#[instrument(skip_all, fields(runtime.method = "session.identity.lookup"))]
async fn lookup_people_identity(
    chain: &ChainRuntime,
    people_chain_genesis_hash: [u8; 32],
    account_id: [u8; 32],
) -> Result<Option<PeopleIdentity>, String> {
    let genesis_hash = people_chain_genesis_hash.to_vec();
    let key = resources_consumers_storage_key(&account_id);
    let follow_id = format!(
        "truapi:identity:{}:{}",
        IDENTITY_LOOKUP_COUNTER.fetch_add(1, Ordering::Relaxed),
        hex::encode(account_id),
    );
    let mut follow = chain.remote_chain_head_follow(
        follow_id.clone(),
        v01::RemoteChainHeadFollowRequest {
            genesis_hash: genesis_hash.clone(),
            with_runtime: false,
        },
    );

    let hash = wait_for_identity_follow_hash(&mut follow).await?;
    let response = chain
        .remote_chain_head_storage(v01::RemoteChainHeadStorageRequest {
            genesis_hash,
            follow_subscription_id: follow_id,
            hash,
            items: vec![v01::StorageQueryItem {
                key: key.clone(),
                query_type: StorageQueryType::Value,
            }],
            child_trie: None,
        })
        .await
        .map_err(|failure| failure.reason())?;

    let operation_id = match response.operation {
        OperationStartedResult::Started { operation_id } => operation_id,
        OperationStartedResult::LimitReached => {
            return Err("People-chain storage lookup limit reached".to_string());
        }
    };
    let Some(value) = wait_for_identity_storage_value(&mut follow, &operation_id, &key).await?
    else {
        return Ok(None);
    };
    decode_people_identity(&value).map(Some)
}

#[instrument(skip_all, fields(runtime.method = "session.identity.wait_follow_hash"))]
async fn wait_for_identity_follow_hash(
    follow: &mut BoxStream<'static, V01RemoteChainHeadFollowItem>,
) -> Result<Vec<u8>, String> {
    let timeout = futures_timer::Delay::new(Duration::from_secs(10)).fuse();
    pin_mut!(timeout);
    loop {
        let next = follow.next().fuse();
        pin_mut!(next);
        futures::select! {
            item = next => match item {
                Some(V01RemoteChainHeadFollowItem::Initialized { finalized_block_hashes, .. }) => {
                    if let Some(hash) = finalized_block_hashes.last() {
                        return Ok(hash.clone());
                    }
                }
                Some(V01RemoteChainHeadFollowItem::BestBlockChanged { best_block_hash }) => {
                    return Ok(best_block_hash);
                }
                Some(V01RemoteChainHeadFollowItem::Stop) | None => {
                    return Err("People-chain follow stopped before initialization".to_string());
                }
                _ => {}
            },
            () = timeout => return Err("People-chain follow initialization timed out".to_string()),
        }
    }
}

#[instrument(skip_all, fields(runtime.method = "session.identity.wait_storage_value"))]
async fn wait_for_identity_storage_value(
    follow: &mut BoxStream<'static, V01RemoteChainHeadFollowItem>,
    operation_id: &str,
    key: &[u8],
) -> Result<Option<Vec<u8>>, String> {
    let timeout = futures_timer::Delay::new(Duration::from_secs(10)).fuse();
    pin_mut!(timeout);
    let mut value = None;
    loop {
        let next = follow.next().fuse();
        pin_mut!(next);
        futures::select! {
            item = next => match item {
                Some(V01RemoteChainHeadFollowItem::OperationStorageItems { operation_id: item_operation_id, items })
                    if item_operation_id == operation_id =>
                {
                    for item in items {
                        if item.key == key {
                            value = item.value;
                        }
                    }
                }
                Some(V01RemoteChainHeadFollowItem::OperationStorageDone { operation_id: item_operation_id })
                    if item_operation_id == operation_id =>
                {
                    return Ok(value);
                }
                Some(V01RemoteChainHeadFollowItem::OperationInaccessible { operation_id: item_operation_id })
                    if item_operation_id == operation_id =>
                {
                    return Ok(None);
                }
                Some(V01RemoteChainHeadFollowItem::OperationError { operation_id: item_operation_id, error })
                    if item_operation_id == operation_id =>
                {
                    return Err(error);
                }
                Some(V01RemoteChainHeadFollowItem::Stop) | None => {
                    return Err("People-chain follow stopped during storage lookup".to_string());
                }
                _ => {}
            },
            () = timeout => return Err("People-chain storage lookup timed out".to_string()),
        }
    }
}

const CALL_CANCELLATION_POLL: Duration = Duration::from_millis(10);
const CALL_CANCELLED_REASON: &str = "request cancelled";

#[instrument(skip_all, fields(runtime.method = "sso.pairing_device.load_or_create"))]
async fn load_or_create_pairing_device_identity(
    storage: &(impl PlatformStorage + ?Sized),
) -> Result<PairingDeviceIdentity, String> {
    if let Some(raw) =
        PlatformStorage::read(storage, PAIRING_DEVICE_IDENTITY_STORAGE_KEY.to_string())
            .await
            .map_err(|err| format!("pairing device identity read failed: {err:?}"))?
    {
        match PairingDeviceIdentity::decode(&mut raw.as_slice()) {
            Ok(identity) => return Ok(identity),
            Err(err) => {
                warn!("stored pairing device identity is invalid, regenerating: {err}");
                let _ = PlatformStorage::clear(
                    storage,
                    PAIRING_DEVICE_IDENTITY_STORAGE_KEY.to_string(),
                )
                .await;
            }
        }
    }

    let identity = generate_pairing_device_identity()
        .map_err(|err| format!("pairing identity failed: {err}"))?;
    PlatformStorage::write(
        storage,
        PAIRING_DEVICE_IDENTITY_STORAGE_KEY.to_string(),
        identity.encode(),
    )
    .await
    .map_err(|err| format!("pairing device identity write failed: {err:?}"))?;
    Ok(identity)
}

#[instrument(skip_all, fields(runtime.method = "call.wait_cancelled"))]
async fn wait_for_call_cancelled(cancel: CancellationToken) -> String {
    while !cancel.is_cancelled() {
        futures_timer::Delay::new(CALL_CANCELLATION_POLL).await;
    }
    CALL_CANCELLED_REASON.to_string()
}

fn request_login_cancelled() -> CallError<HostRequestLoginError> {
    request_login_cancelled_with_reason(CALL_CANCELLED_REASON.to_string())
}

fn request_login_cancelled_with_reason(reason: String) -> CallError<HostRequestLoginError> {
    CallError::Domain(HostRequestLoginError::V1(
        v01::HostRequestLoginError::Unknown { reason },
    ))
}

struct PairingSuccess {
    peer_statement_account_id: [u8; 32],
    success: crate::host_logic::sso_pairing::HandshakeSuccessV2,
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.wait_success"))]
async fn wait_for_v2_pairing_success(
    connection: &dyn JsonRpcConnection,
    mut responses: BoxStream<'static, String>,
    remote_subscription_slot: SharedRemoteSubscriptionId,
    topic: [u8; 32],
    core_encryption_secret_key: [u8; 32],
) -> Result<PairingSuccess, String> {
    let mut remote_subscription_id = None;
    let mut pending_query_request_id = None;
    let mut pending_query_remote_id = None;
    let mut pending_query_elapsed_ticks = 0u8;

    #[cfg(target_arch = "wasm32")]
    loop {
        let Some(frame) = responses.next().await else {
            return Err("pairing statement-store response stream ended".to_string());
        };
        if let Some(success) = handle_v2_pairing_frame(
            connection,
            &frame,
            &mut remote_subscription_id,
            &remote_subscription_slot,
            &mut pending_query_request_id,
            &mut pending_query_remote_id,
            &mut pending_query_elapsed_ticks,
            core_encryption_secret_key,
        )? {
            return Ok(success);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut query_counter = 0usize;
        let poll = futures_timer::Delay::new(PAIRING_QUERY_INTERVAL).fuse();
        pin_mut!(poll);
        loop {
            futures::select! {
                frame = responses.next().fuse() => {
                    let Some(frame) = frame else {
                        return Err("pairing statement-store response stream ended".to_string());
                    };
                    if let Some(success) = handle_v2_pairing_frame(
                        connection,
                        &frame,
                        &mut remote_subscription_id,
                        &remote_subscription_slot,
                        &mut pending_query_request_id,
                        &mut pending_query_remote_id,
                        &mut pending_query_elapsed_ticks,
                        core_encryption_secret_key,
                    )? {
                        return Ok(success);
                    }
                }
                _ = poll => {
                    if pending_query_request_id.is_some() {
                        pending_query_elapsed_ticks = pending_query_elapsed_ticks.saturating_add(1);
                    }
                    if pending_query_request_id.is_some()
                        && pending_query_elapsed_ticks >= PAIRING_QUERY_TIMEOUT_TICKS
                    {
                        if let Some(remote_id) = pending_query_remote_id.as_deref() {
                            connection.send(unsubscribe_request(
                                &format!(
                                    "{}:timeout-unsubscribe",
                                    pending_query_request_id
                                        .as_deref()
                                        .unwrap_or(PAIRING_SUBSCRIBE_REQUEST_ID)
                                ),
                                remote_id,
                            ));
                        }
                        pending_query_request_id = None;
                        pending_query_remote_id = None;
                        pending_query_elapsed_ticks = 0;
                    }
                    if pending_query_request_id.is_none() {
                        query_counter += 1;
                        let query_request_id =
                            format!("{PAIRING_SUBSCRIBE_REQUEST_ID}:query:{query_counter}");
                        connection.send(subscribe_match_all_request(&query_request_id, &[topic]));
                        pending_query_elapsed_ticks = 0;
                        pending_query_request_id = Some(query_request_id);
                    }
                    poll.set(futures_timer::Delay::new(PAIRING_QUERY_INTERVAL).fuse());
                }
            }
        }
    }
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.handle_frame"))]
fn handle_v2_pairing_frame(
    connection: &dyn JsonRpcConnection,
    frame: &str,
    remote_subscription_id: &mut Option<String>,
    remote_subscription_slot: &SharedRemoteSubscriptionId,
    pending_query_request_id: &mut Option<String>,
    pending_query_remote_id: &mut Option<String>,
    pending_query_elapsed_ticks: &mut u8,
    core_encryption_secret_key: [u8; 32],
) -> Result<Option<PairingSuccess>, String> {
    if remote_subscription_id.is_none()
        && let Some(id) = parse_subscribe_ack(frame, PAIRING_SUBSCRIBE_REQUEST_ID)
            .map_err(|err| err.to_string())?
    {
        *remote_subscription_slot
            .lock()
            .expect("pairing subscription id mutex poisoned") = Some(id.clone());
        *remote_subscription_id = Some(id);
        return Ok(None);
    }
    if let Some((query_request_id, id)) =
        parse_pairing_query_subscribe_ack(frame, pending_query_request_id.as_deref())?
    {
        *pending_query_request_id = Some(query_request_id);
        *pending_query_remote_id = Some(id);
        *pending_query_elapsed_ticks = 0;
        return Ok(None);
    }

    let Some(page) = parse_new_statements(frame).map_err(|err| err.to_string())? else {
        return Ok(None);
    };
    let is_live_subscription =
        Some(page.remote_subscription_id.as_str()) == remote_subscription_id.as_deref();
    let is_query_subscription =
        Some(page.remote_subscription_id.as_str()) == pending_query_remote_id.as_deref();
    if !is_live_subscription && !is_query_subscription {
        return Ok(None);
    }

    if is_query_subscription && page.remaining.unwrap_or(0) == 0 {
        connection.send(unsubscribe_request(
            &format!(
                "{}:unsubscribe",
                pending_query_request_id
                    .as_deref()
                    .unwrap_or(PAIRING_SUBSCRIBE_REQUEST_ID)
            ),
            &page.remote_subscription_id,
        ));
        *pending_query_request_id = None;
        *pending_query_remote_id = None;
        *pending_query_elapsed_ticks = 0;
    }
    for statement in page.statements {
        if let Some(success) = decode_v2_pairing_statement(&statement, core_encryption_secret_key)?
        {
            return Ok(Some(success));
        }
    }

    Ok(None)
}

fn parse_pairing_query_subscribe_ack(
    frame: &str,
    pending_query_request_id: Option<&str>,
) -> Result<Option<(String, String)>, String> {
    if let Some(query_request_id) = pending_query_request_id
        && let Some(id) =
            parse_subscribe_ack(frame, query_request_id).map_err(|err| err.to_string())?
    {
        return Ok(Some((query_request_id.to_string(), id)));
    }

    let value: serde_json::Value = serde_json::from_str(frame).map_err(|err| err.to_string())?;
    let Some(request_id) = value.get("id").and_then(serde_json::Value::as_str) else {
        return Ok(None);
    };
    if !request_id.starts_with(&format!("{PAIRING_SUBSCRIBE_REQUEST_ID}:query:")) {
        return Ok(None);
    }
    if let Some(error) = value.get("error") {
        return Err(error
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("statement-store query subscribe failed")
            .to_string());
    }
    let Some(remote_id) = value.get("result").and_then(serde_json::Value::as_str) else {
        return Err("missing query subscribe result".to_string());
    };
    Ok(Some((request_id.to_string(), remote_id.to_string())))
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.decode_statement"))]
fn decode_v2_pairing_statement(
    statement: &[u8],
    core_encryption_secret_key: [u8; 32],
) -> Result<Option<PairingSuccess>, String> {
    let verified =
        decode_verified_statement_data(statement, None).map_err(|err| err.to_string())?;
    let handshake = decode_app_handshake_data(&verified.data)?;
    let VersionedHandshakeResponse::V2 {
        encrypted_message,
        public_key,
    } = handshake
    else {
        return Err("pairing response is not SSO V2".to_string());
    };
    match decrypt_v2_handshake_response(core_encryption_secret_key, public_key, &encrypted_message)?
    {
        EncryptedHandshakeResponseV2::Pending(_) => Ok(None),
        EncryptedHandshakeResponseV2::Failed(reason) => Err(reason),
        EncryptedHandshakeResponseV2::Success(success) => Ok(Some(PairingSuccess {
            peer_statement_account_id: verified.signer,
            success,
        })),
    }
}

impl<P> Signing for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
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
        match self.chain_submit_decision().await {
            Ok(Decision::Granted) => {}
            Ok(Decision::Denied) => {
                return Err(CallError::Domain(HostSignPayloadError::V1(
                    v01::HostSignPayloadError::PermissionDenied,
                )));
            }
            Err(reason) => return Err(CallError::HostFailure { reason }),
        }
        let Some(session) = self.session_state.current() else {
            return Err(CallError::Domain(HostSignPayloadError::V1(
                v01::HostSignPayloadError::Rejected,
            )));
        };
        let confirmed = PlatformUserConfirmation::confirm_sign_payload(
            self.platform.as_ref(),
            inner.clone().encode(),
        )
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
        let response = self
            .submit_sso_remote_message(cx, &session, "sign-payload", message)
            .await
            .map_err(|reason| {
                CallError::Domain(HostSignPayloadError::V1(
                    v01::HostSignPayloadError::Unknown { reason },
                ))
            })?;
        let SsoRemoteResponse::Sign(response) = response else {
            return Err(CallError::Domain(HostSignPayloadError::V1(
                v01::HostSignPayloadError::Unknown {
                    reason: "Unexpected SSO response for signing request".to_string(),
                },
            )));
        };
        response
            .payload
            .map(|payload| {
                HostSignPayloadResponse::V1(v01::HostSignPayloadResponse {
                    signature: payload.signature,
                    signed_transaction: payload.signed_transaction,
                })
            })
            .map_err(|reason| {
                CallError::Domain(HostSignPayloadError::V1(
                    v01::HostSignPayloadError::Unknown { reason },
                ))
            })
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
        match self.chain_submit_decision().await {
            Ok(Decision::Granted) => {}
            Ok(Decision::Denied) => {
                return Err(CallError::Domain(HostSignRawError::V1(
                    v01::HostSignPayloadError::PermissionDenied,
                )));
            }
            Err(reason) => return Err(CallError::HostFailure { reason }),
        }
        let Some(session) = self.session_state.current() else {
            return Err(CallError::Domain(HostSignRawError::V1(
                v01::HostSignPayloadError::Rejected,
            )));
        };
        let confirmed = PlatformUserConfirmation::confirm_sign_raw(
            self.platform.as_ref(),
            inner.clone().encode(),
        )
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
        let response = self
            .submit_sso_remote_message(cx, &session, "sign-raw", message)
            .await
            .map_err(|reason| {
                CallError::Domain(HostSignRawError::V1(v01::HostSignPayloadError::Unknown {
                    reason,
                }))
            })?;
        let SsoRemoteResponse::Sign(response) = response else {
            return Err(CallError::Domain(HostSignRawError::V1(
                v01::HostSignPayloadError::Unknown {
                    reason: "Unexpected SSO response for signing request".to_string(),
                },
            )));
        };
        response
            .payload
            .map(|payload| {
                HostSignRawResponse::V1(v01::HostSignPayloadResponse {
                    signature: payload.signature,
                    signed_transaction: payload.signed_transaction,
                })
            })
            .map_err(|reason| {
                CallError::Domain(HostSignRawError::V1(v01::HostSignPayloadError::Unknown {
                    reason,
                }))
            })
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
        match self.chain_submit_decision().await {
            Ok(Decision::Granted) => {}
            Ok(Decision::Denied) => {
                return Err(CallError::Domain(HostCreateTransactionError::V1(
                    v01::HostCreateTransactionError::PermissionDenied,
                )));
            }
            Err(reason) => return Err(CallError::HostFailure { reason }),
        }
        let Some(session) = self.session_state.current() else {
            return Err(CallError::Domain(HostCreateTransactionError::V1(
                v01::HostCreateTransactionError::Rejected,
            )));
        };
        let confirmed = PlatformUserConfirmation::confirm_create_transaction(
            self.platform.as_ref(),
            inner.clone().encode(),
        )
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
                CallError::Domain(HostCreateTransactionError::V1(
                    v01::HostCreateTransactionError::Unknown { reason },
                ))
            })?;
        let SsoRemoteResponse::CreateTransaction(response) = response else {
            return Err(CallError::Domain(HostCreateTransactionError::V1(
                v01::HostCreateTransactionError::Unknown {
                    reason: "Unexpected SSO response for transaction request".to_string(),
                },
            )));
        };
        response
            .signed_transaction
            .map(|transaction| {
                HostCreateTransactionResponse::V1(v01::HostCreateTransactionResponse {
                    transaction,
                })
            })
            .map_err(|reason| {
                CallError::Domain(HostCreateTransactionError::V1(
                    v01::HostCreateTransactionError::Unknown { reason },
                ))
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
        match self.chain_submit_decision().await {
            Ok(Decision::Granted) => {}
            Ok(Decision::Denied) => {
                return Err(CallError::Domain(
                    HostSignPayloadWithLegacyAccountError::V1(
                        v01::HostSignPayloadError::PermissionDenied,
                    ),
                ));
            }
            Err(reason) => return Err(CallError::HostFailure { reason }),
        }
        let confirmed = PlatformUserConfirmation::confirm_sign_payload(
            self.platform.as_ref(),
            inner.clone().encode(),
        )
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
        let response = self
            .submit_sso_remote_message(cx, &session, "legacy-sign-payload", message)
            .await
            .map_err(|reason| {
                CallError::Domain(HostSignPayloadWithLegacyAccountError::V1(
                    v01::HostSignPayloadError::Unknown { reason },
                ))
            })?;
        let SsoRemoteResponse::Sign(response) = response else {
            return Err(CallError::Domain(
                HostSignPayloadWithLegacyAccountError::V1(v01::HostSignPayloadError::Unknown {
                    reason: "Unexpected SSO response for signing request".to_string(),
                }),
            ));
        };
        response
            .payload
            .map(|payload| {
                HostSignPayloadWithLegacyAccountResponse::V1(v01::HostSignPayloadResponse {
                    signature: payload.signature,
                    signed_transaction: payload.signed_transaction,
                })
            })
            .map_err(|reason| {
                CallError::Domain(HostSignPayloadWithLegacyAccountError::V1(
                    v01::HostSignPayloadError::Unknown { reason },
                ))
            })
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
        match self.chain_submit_decision().await {
            Ok(Decision::Granted) => {}
            Ok(Decision::Denied) => {
                return Err(CallError::Domain(HostSignRawWithLegacyAccountError::V1(
                    v01::HostSignPayloadError::PermissionDenied,
                )));
            }
            Err(reason) => return Err(CallError::HostFailure { reason }),
        }
        let confirmed = PlatformUserConfirmation::confirm_sign_raw(
            self.platform.as_ref(),
            inner.clone().encode(),
        )
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
        let response = self
            .submit_sso_remote_message(cx, &session, "legacy-sign-raw", message)
            .await
            .map_err(|reason| {
                CallError::Domain(HostSignRawWithLegacyAccountError::V1(
                    v01::HostSignPayloadError::Unknown { reason },
                ))
            })?;
        let SsoRemoteResponse::Sign(response) = response else {
            return Err(CallError::Domain(HostSignRawWithLegacyAccountError::V1(
                v01::HostSignPayloadError::Unknown {
                    reason: "Unexpected SSO response for signing request".to_string(),
                },
            )));
        };
        response
            .payload
            .map(|payload| {
                HostSignRawWithLegacyAccountResponse::V1(v01::HostSignPayloadResponse {
                    signature: payload.signature,
                    signed_transaction: payload.signed_transaction,
                })
            })
            .map_err(|reason| {
                CallError::Domain(HostSignRawWithLegacyAccountError::V1(
                    v01::HostSignPayloadError::Unknown { reason },
                ))
            })
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
        match self.chain_submit_decision().await {
            Ok(Decision::Granted) => {}
            Ok(Decision::Denied) => {
                return Err(CallError::Domain(
                    HostCreateTransactionWithLegacyAccountError::V1(
                        v01::HostCreateTransactionError::PermissionDenied,
                    ),
                ));
            }
            Err(reason) => return Err(CallError::HostFailure { reason }),
        }
        let confirmed = PlatformUserConfirmation::confirm_create_transaction(
            self.platform.as_ref(),
            inner.clone().encode(),
        )
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
                CallError::Domain(HostCreateTransactionWithLegacyAccountError::V1(
                    v01::HostCreateTransactionError::Unknown { reason },
                ))
            })?;
        let SsoRemoteResponse::CreateTransaction(response) = response else {
            return Err(CallError::Domain(
                HostCreateTransactionWithLegacyAccountError::V1(
                    v01::HostCreateTransactionError::Unknown {
                        reason: "Unexpected SSO response for transaction request".to_string(),
                    },
                ),
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
                CallError::Domain(HostCreateTransactionWithLegacyAccountError::V1(
                    v01::HostCreateTransactionError::Unknown { reason },
                ))
            })
    }
}

impl<P> StatementStore for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    #[instrument(skip_all, fields(runtime.method = "statement_store.subscribe"))]
    async fn subscribe(
        &self,
        cx: &CallContext,
        request: RemoteStatementStoreSubscribeRequest,
    ) -> Result<
        Subscription<RemoteStatementStoreSubscribeItem>,
        CallError<RemoteStatementStoreSubscribeError>,
    > {
        let (kind, topics) = match statement_store_topic_filter(request) {
            Ok(value) => value,
            Err(reason) => {
                return Err(CallError::Domain(RemoteStatementStoreSubscribeError::V1(
                    v01::GenericError { reason },
                )));
            }
        };
        let request_id = if cx.request_id().is_empty() {
            "truapi:ss-subscribe".to_string()
        } else {
            cx.request_id().to_string()
        };
        let connection = match PlatformChainProvider::connect(
            self.platform.as_ref(),
            self.runtime_config.people_chain_genesis_hash.to_vec(),
        )
        .await
        {
            Ok(connection) => connection,
            Err(err) => {
                return Err(CallError::Domain(RemoteStatementStoreSubscribeError::V1(
                    v01::GenericError {
                        reason: format!("statement-store connect failed: {err:?}"),
                    },
                )));
            }
        };
        connection.send(match kind {
            TopicFilterKind::MatchAll => subscribe_match_all_request(&request_id, &topics),
            TopicFilterKind::MatchAny => subscribe_match_any_request(&request_id, &topics),
        });
        let responses = connection.responses();
        let stream = statement_store_subscription_stream(connection, responses, request_id);
        Ok(Subscription::new(Box::pin(stream)))
    }

    #[instrument(skip_all, fields(runtime.method = "statement_store.create_proof"))]
    async fn create_proof(
        &self,
        _cx: &CallContext,
        request: RemoteStatementStoreCreateProofRequest,
    ) -> Result<
        RemoteStatementStoreCreateProofResponse,
        CallError<RemoteStatementStoreCreateProofError>,
    > {
        let RemoteStatementStoreCreateProofRequest::V1(mut inner) = request;
        inner.product_account_id = Self::normalize_product_account_id(inner.product_account_id);
        if !self.is_product_account_valid_for_caller(&inner.product_account_id.dot_ns_identifier) {
            return Err(CallError::Domain(RemoteStatementStoreCreateProofError::V1(
                v01::RemoteStatementStoreCreateProofError::UnknownAccount,
            )));
        }
        let proof = self
            .create_statement_proof(inner.statement)
            .map_err(statement_proof_error)?;
        Ok(RemoteStatementStoreCreateProofResponse::V1(
            v01::RemoteStatementStoreCreateProofResponse { proof },
        ))
    }

    #[instrument(skip_all, fields(runtime.method = "statement_store.create_proof_authorized"))]
    async fn create_proof_authorized(
        &self,
        _cx: &CallContext,
        request: RemoteStatementStoreCreateProofAuthorizedRequest,
    ) -> Result<
        RemoteStatementStoreCreateProofAuthorizedResponse,
        CallError<RemoteStatementStoreCreateProofAuthorizedError>,
    > {
        let RemoteStatementStoreCreateProofAuthorizedRequest::V1(statement) = request;
        let proof = self
            .create_statement_proof(statement)
            .map_err(statement_proof_authorized_error)?;
        Ok(RemoteStatementStoreCreateProofAuthorizedResponse::V1(
            v01::RemoteStatementStoreCreateProofResponse { proof },
        ))
    }

    #[instrument(skip_all, fields(runtime.method = "statement_store.submit"))]
    async fn submit(
        &self,
        cx: &CallContext,
        request: RemoteStatementStoreSubmitRequest,
    ) -> Result<(), CallError<RemoteStatementStoreSubmitError>> {
        let RemoteStatementStoreSubmitRequest::V1(statement) = request;
        let statement = signed_statement_to_scale(statement).map_err(|reason| {
            CallError::Domain(RemoteStatementStoreSubmitError::V1(v01::GenericError {
                reason,
            }))
        })?;
        let request_id = if cx.request_id().is_empty() {
            "truapi:ss-submit".to_string()
        } else {
            cx.request_id().to_string()
        };
        let connection = PlatformChainProvider::connect(
            self.platform.as_ref(),
            self.runtime_config.people_chain_genesis_hash.to_vec(),
        )
        .await
        .map_err(|err| {
            CallError::Domain(RemoteStatementStoreSubmitError::V1(v01::GenericError {
                reason: format!("statement-store connect failed: {err:?}"),
            }))
        })?;
        connection.send(submit_statement_request(&request_id, &statement));
        wait_for_statement_submit_ack(connection.responses(), &request_id)
            .await
            .map_err(|reason| {
                CallError::Domain(RemoteStatementStoreSubmitError::V1(v01::GenericError {
                    reason,
                }))
            })
    }
}

fn statement_store_topic_filter(
    request: RemoteStatementStoreSubscribeRequest,
) -> Result<(TopicFilterKind, Vec<[u8; 32]>), String> {
    match request {
        RemoteStatementStoreSubscribeRequest::V1(
            v01::RemoteStatementStoreSubscribeRequest::MatchAll(topics),
        ) => {
            if topics.len() > MAX_MATCH_ALL_TOPICS {
                return Err(format!(
                    "MatchAll has {} topics, maximum is {}",
                    topics.len(),
                    MAX_MATCH_ALL_TOPICS
                ));
            }
            Ok((TopicFilterKind::MatchAll, topics))
        }
        RemoteStatementStoreSubscribeRequest::V1(
            v01::RemoteStatementStoreSubscribeRequest::MatchAny(topics),
        ) => {
            if topics.len() > MAX_MATCH_ANY_TOPICS {
                return Err(format!(
                    "MatchAny has {} topics, maximum is {}",
                    topics.len(),
                    MAX_MATCH_ANY_TOPICS
                ));
            }
            Ok((TopicFilterKind::MatchAny, topics))
        }
    }
}

#[instrument(skip_all, fields(runtime.method = "statement_store.wait_submit_ack"))]
async fn wait_for_statement_submit_ack(
    mut responses: BoxStream<'static, String>,
    request_id: &str,
) -> Result<(), String> {
    while let Some(frame) = responses.next().await {
        if parse_submit_ack(&frame, request_id)
            .map_err(|err| err.to_string())?
            .is_some()
        {
            return Ok(());
        }
    }
    Err("statement-store submit response stream ended".to_string())
}

fn statement_store_subscription_stream(
    connection: Box<dyn JsonRpcConnection>,
    responses: BoxStream<'static, String>,
    request_id: String,
) -> impl futures::Stream<Item = RemoteStatementStoreSubscribeItem> + Send {
    StatementStoreSubscriptionStream {
        connection,
        responses,
        request_id,
        remote_subscription_id: None,
        is_complete: false,
    }
}

struct StatementStoreSubscriptionStream {
    connection: Box<dyn JsonRpcConnection>,
    responses: BoxStream<'static, String>,
    request_id: String,
    remote_subscription_id: Option<String>,
    is_complete: bool,
}

impl Unpin for StatementStoreSubscriptionStream {}

impl futures::Stream for StatementStoreSubscriptionStream {
    type Item = RemoteStatementStoreSubscribeItem;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let state = self.get_mut();
        loop {
            let frame = match state.responses.as_mut().poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(frame)) => frame,
                Poll::Ready(None) => return Poll::Ready(None),
            };

            if state.remote_subscription_id.is_none() {
                match parse_subscribe_ack(&frame, &state.request_id) {
                    Ok(Some(id)) => {
                        state.remote_subscription_id = Some(id);
                        continue;
                    }
                    Ok(None) => {}
                    Err(_) => return Poll::Ready(None),
                }
            }

            let page = match parse_new_statements(&frame) {
                Ok(Some(page)) => page,
                Ok(None) => continue,
                Err(_) => return Poll::Ready(None),
            };
            if state
                .remote_subscription_id
                .as_ref()
                .is_some_and(|id| id != &page.remote_subscription_id)
            {
                continue;
            }

            let was_complete = state.is_complete;
            let is_complete = was_complete || page.remaining == Some(0);
            state.is_complete = is_complete;
            let statements = page
                .statements
                .into_iter()
                .filter_map(|statement| decode_signed_statement(&statement).ok())
                .collect::<Vec<_>>();
            if statements.is_empty() {
                if is_complete && !was_complete {
                    return Poll::Ready(Some(RemoteStatementStoreSubscribeItem::V1(
                        v01::RemoteStatementStoreSubscribeItem {
                            statements,
                            is_complete,
                        },
                    )));
                }
                continue;
            }

            return Poll::Ready(Some(RemoteStatementStoreSubscribeItem::V1(
                v01::RemoteStatementStoreSubscribeItem {
                    statements,
                    is_complete,
                },
            )));
        }
    }
}

impl Drop for StatementStoreSubscriptionStream {
    fn drop(&mut self) {
        if let Some(remote_subscription_id) = self.remote_subscription_id.as_ref() {
            self.connection.send(unsubscribe_request(
                &format!("{}:unsubscribe", self.request_id),
                remote_subscription_id,
            ));
        }
    }
}

impl<P> PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    fn create_statement_proof(
        &self,
        statement: v01::Statement,
    ) -> Result<v01::StatementProof, StatementProofFailure> {
        let session = self
            .session_state
            .current()
            .ok_or(StatementProofFailure::NoSession)?;
        let sso = session
            .sso
            .as_ref()
            .ok_or(StatementProofFailure::NoSession)?;
        let fields = statement_fields_from_v01(statement)
            .map_err(StatementProofFailure::InvalidStatement)?;
        let signed = sign_statement_fields(sso.ss_secret, sso.ss_public_key, fields)
            .map_err(StatementProofFailure::UnableToSign)?;
        signed
            .into_iter()
            .find_map(|field| match field {
                crate::host_logic::statement_store::StatementField::Proof(proof) => {
                    Some(statement_proof_to_v01(proof))
                }
                _ => None,
            })
            .ok_or_else(|| StatementProofFailure::UnableToSign("missing proof".to_string()))
    }
}

enum StatementProofFailure {
    NoSession,
    InvalidStatement(String),
    UnableToSign(String),
}

fn statement_proof_error(
    failure: StatementProofFailure,
) -> CallError<RemoteStatementStoreCreateProofError> {
    match failure {
        StatementProofFailure::NoSession => {
            CallError::Domain(RemoteStatementStoreCreateProofError::V1(
                v01::RemoteStatementStoreCreateProofError::UnableToSign,
            ))
        }
        StatementProofFailure::UnableToSign(_reason) => {
            CallError::Domain(RemoteStatementStoreCreateProofError::V1(
                v01::RemoteStatementStoreCreateProofError::UnableToSign,
            ))
        }
        StatementProofFailure::InvalidStatement(reason) => {
            CallError::Domain(RemoteStatementStoreCreateProofError::V1(
                v01::RemoteStatementStoreCreateProofError::Unknown { reason },
            ))
        }
    }
}

fn statement_proof_authorized_error(
    failure: StatementProofFailure,
) -> CallError<RemoteStatementStoreCreateProofAuthorizedError> {
    match failure {
        StatementProofFailure::NoSession => {
            CallError::Domain(RemoteStatementStoreCreateProofAuthorizedError::V1(
                v01::RemoteStatementStoreCreateProofError::UnableToSign,
            ))
        }
        StatementProofFailure::UnableToSign(_reason) => {
            CallError::Domain(RemoteStatementStoreCreateProofAuthorizedError::V1(
                v01::RemoteStatementStoreCreateProofError::UnableToSign,
            ))
        }
        StatementProofFailure::InvalidStatement(reason) => {
            CallError::Domain(RemoteStatementStoreCreateProofAuthorizedError::V1(
                v01::RemoteStatementStoreCreateProofError::Unknown { reason },
            ))
        }
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

impl<P> Chain for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
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

impl<P> Chat for PlatformRuntimeHost<P> where P: Platform + 'static {}
impl<P> CoinPayment for PlatformRuntimeHost<P> where P: Platform + 'static {}
impl<P> Payment for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    #[instrument(skip_all, fields(runtime.method = "payment.balance_subscribe"))]
    async fn balance_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPaymentBalanceSubscribeRequest,
    ) -> Result<
        Subscription<HostPaymentBalanceSubscribeItem>,
        CallError<HostPaymentBalanceSubscribeError>,
    > {
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
        Err(CallError::Domain(HostPaymentTopUpError::V1(
            v01::HostPaymentTopUpError::Unknown {
                reason: PAYMENTS_NOT_IMPLEMENTED.to_string(),
            },
        )))
    }
}

impl<P> ResourceAllocation for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
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

        let confirmed = PlatformUserConfirmation::confirm_resource_allocation(
            self.platform.as_ref(),
            inner.clone().encode(),
        )
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

impl<P> Entropy for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
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

impl<P> Preimage for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    #[instrument(skip_all, fields(runtime.method = "preimage.lookup_subscribe"))]
    async fn lookup_subscribe(
        &self,
        _cx: &CallContext,
        request: RemotePreimageLookupSubscribeRequest,
    ) -> Subscription<RemotePreimageLookupSubscribeItem> {
        let RemotePreimageLookupSubscribeRequest::V1(v01::RemotePreimageLookupSubscribeRequest {
            key,
        }) = request;
        let stream = PlatformPreimageHost::lookup_preimage(self.platform.as_ref(), key).filter_map(
            |item| async move {
                item.ok().map(|value| {
                    RemotePreimageLookupSubscribeItem::V1(v01::RemotePreimageLookupSubscribeItem {
                        value,
                    })
                })
            },
        );
        Subscription::new(Box::pin(stream))
    }

    #[instrument(skip_all, fields(runtime.method = "preimage.submit"))]
    async fn submit(
        &self,
        _cx: &CallContext,
        request: RemotePreimageSubmitRequest,
    ) -> Result<RemotePreimageSubmitResponse, CallError<RemotePreimageSubmitError>> {
        let RemotePreimageSubmitRequest::V1(value) = request;
        PlatformPreimageHost::confirm_preimage_submit(self.platform.as_ref(), value.len() as u64)
            .await
            .map_err(|err| CallError::Domain(RemotePreimageSubmitError::V1(err)))?;
        PlatformPreimageHost::submit_preimage(self.platform.as_ref(), value)
            .await
            .map(RemotePreimageSubmitResponse::V1)
            .map_err(|err| CallError::Domain(RemotePreimageSubmitError::V1(err)))
    }
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

impl<P> Theme for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    #[instrument(skip_all, fields(runtime.method = "theme.subscribe"))]
    async fn subscribe(&self, _cx: &CallContext) -> Subscription<HostThemeSubscribeItem> {
        let stream =
            PlatformThemeHost::subscribe_theme(self.platform.as_ref()).filter_map(|item| async {
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
impl<P> Notifications for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    #[instrument(skip_all, fields(runtime.method = "notifications.send_push_notification"))]
    async fn send_push_notification(
        &self,
        _cx: &CallContext,
        request: HostPushNotificationRequest,
    ) -> Result<HostPushNotificationResponse, CallError<HostPushNotificationError>> {
        let HostPushNotificationRequest::V1(inner) = request;
        PlatformNotifications::push_notification(self.platform.as_ref(), inner)
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
        PlatformNotifications::cancel_notification(self.platform.as_ref(), id)
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
    #[cfg(not(target_arch = "wasm32"))]
    use crate::chain_runtime::thread_per_task_spawner;
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};
    use futures::stream::{self, BoxStream};
    use hkdf::Hkdf;
    use p256::PublicKey as P256PublicKey;
    use p256::SecretKey as P256SecretKey;
    use p256::ecdh::diffie_hellman;
    use p256::elliptic_curve::sec1::ToEncodedPoint;
    use parity_scale_codec::{Decode, Encode};
    use schnorrkel::{ExpansionMode, MiniSecretKey};
    use sha2::Sha256;
    use std::sync::Mutex;
    use truapi::v01;
    use truapi_platform::{
        ChainProvider, Features as PlatformFeatures, JsonRpcConnection,
        Navigation as PlatformNavigation, Notifications as PlatformNotifications, PairingPresenter,
        Permissions as PlatformPermissions, PreimageHost, SessionStore, Storage as PlatformStorage,
        ThemeHost, UserConfirmation,
    };

    fn test_spawner() -> Spawner {
        #[cfg(not(target_arch = "wasm32"))]
        {
            thread_per_task_spawner()
        }
        #[cfg(target_arch = "wasm32")]
        {
            immediate_spawner()
        }
    }

    fn immediate_spawner() -> Spawner {
        Arc::new(futures::executor::block_on)
    }

    fn remote_subscription_slot() -> SharedRemoteSubscriptionId {
        Arc::new(Mutex::new(None))
    }

    /// Minimal Platform impl that only answers `feature_supported`. Every
    /// other callback returns a unit value or empty stream, so the runtime
    /// can exercise its delegation paths without pulling in a real backend.
    struct StubPlatform {
        remote_permission_granted: bool,
        account_alias_confirmed: bool,
        account_alias_error: Option<&'static str>,
        sign_payload_confirmed: bool,
        sign_payload_error: Option<&'static str>,
        sign_raw_confirmed: bool,
        sign_raw_error: Option<&'static str>,
        create_transaction_confirmed: bool,
        create_transaction_error: Option<&'static str>,
        resource_allocation_confirmed: bool,
        resource_allocation_error: Option<&'static str>,
        session_blob: Option<Vec<u8>>,
        session_error: Option<&'static str>,
        session_clears: Arc<Mutex<usize>>,
        session_writes: Arc<Mutex<Vec<Vec<u8>>>>,
        pairing_error: Option<&'static str>,
        pairing_pending: bool,
        presented_pairings: Arc<Mutex<Vec<String>>>,
        pairing_success_response: bool,
        notification_id: v01::NotificationId,
        pushed_notifications: Arc<Mutex<Vec<v01::HostPushNotificationRequest>>>,
        cancelled_notifications: Arc<Mutex<Vec<v01::NotificationId>>>,
        sent_rpc: Arc<Mutex<Vec<String>>>,
        rpc_responses: Vec<String>,
        chain_connect_error: Option<&'static str>,
        local_storage: Arc<Mutex<std::collections::HashMap<String, Vec<u8>>>>,
    }

    impl Default for StubPlatform {
        fn default() -> Self {
            Self {
                remote_permission_granted: true,
                account_alias_confirmed: false,
                account_alias_error: None,
                sign_payload_confirmed: false,
                sign_payload_error: None,
                sign_raw_confirmed: false,
                sign_raw_error: None,
                create_transaction_confirmed: false,
                create_transaction_error: None,
                resource_allocation_confirmed: false,
                resource_allocation_error: None,
                session_blob: None,
                session_error: None,
                session_clears: Arc::new(Mutex::new(0)),
                session_writes: Arc::new(Mutex::new(Vec::new())),
                pairing_error: None,
                pairing_pending: false,
                presented_pairings: Arc::new(Mutex::new(Vec::new())),
                pairing_success_response: false,
                notification_id: 0,
                pushed_notifications: Arc::new(Mutex::new(Vec::new())),
                cancelled_notifications: Arc::new(Mutex::new(Vec::new())),
                sent_rpc: Arc::new(Mutex::new(Vec::new())),
                rpc_responses: Vec::new(),
                chain_connect_error: None,
                local_storage: Arc::new(Mutex::new(std::collections::HashMap::new())),
            }
        }
    }

    fn stub_platform() -> Arc<StubPlatform> {
        Arc::new(StubPlatform::default())
    }

    fn runtime_config(product_id: &str) -> RuntimeConfig {
        RuntimeConfig {
            product_label: product_id.trim_end_matches(".dot").to_string(),
            product_id: product_id.to_string(),
            site_id: "test".to_string(),
            host_name: "Polkadot Web".to_string(),
            host_icon: Some("https://example.invalid/dotli.png".to_string()),
            host_version: None,
            platform_type: None,
            platform_version: None,
            people_chain_genesis_hash: [0; 32],
            pairing_deeplink_scheme: truapi_platform::PairingDeeplinkScheme::PolkadotApp,
        }
    }

    fn session_info() -> crate::host_logic::session::SessionInfo {
        crate::host_logic::session::SessionInfo {
            public_key: [
                0x80, 0x05, 0x28, 0xc9, 0x55, 0x87, 0x3e, 0x4c, 0x78, 0xb7, 0xdf, 0x24, 0xf7, 0x1d,
                0xb8, 0xf5, 0x81, 0xaa, 0x99, 0xe3, 0x49, 0x3b, 0xf4, 0x96, 0xed, 0xf1, 0x51, 0xab,
                0xc1, 0xd7, 0x20, 0x23,
            ],
            sso: None,
            root_entropy_source: Some([
                0x15, 0xcb, 0x94, 0x34, 0x84, 0x0b, 0x56, 0xbe, 0x1f, 0xdd, 0x91, 0xc4, 0x6a, 0x13,
                0xf5, 0x20, 0xf4, 0x91, 0x61, 0x2e, 0xa5, 0xd6, 0x06, 0x92, 0x0d, 0x91, 0x38, 0xe8,
                0xbd, 0xd6, 0x3c, 0xb0,
            ]),
            identity_account_id: Some([
                0x80, 0x05, 0x28, 0xc9, 0x55, 0x87, 0x3e, 0x4c, 0x78, 0xb7, 0xdf, 0x24, 0xf7, 0x1d,
                0xb8, 0xf5, 0x81, 0xaa, 0x99, 0xe3, 0x49, 0x3b, 0xf4, 0x96, 0xed, 0xf1, 0x51, 0xab,
                0xc1, 0xd7, 0x20, 0x23,
            ]),
            lite_username: Some("alice".to_string()),
            full_username: Some("Alice Smith".to_string()),
        }
    }

    fn sso_session_info() -> crate::host_logic::session::SessionInfo {
        let mut session = session_info();
        let mini_secret = MiniSecretKey::from_bytes(&[7; 32]).unwrap();
        let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
        let (_, peer_public_key) = peer_statement_keypair();
        let core_secret = P256SecretKey::from_slice(&[1; 32]).unwrap();
        let peer_secret = P256SecretKey::from_slice(&[2; 32]).unwrap();
        session.sso = Some(crate::host_logic::session::SsoSessionInfo {
            ss_secret: keypair.secret.to_bytes(),
            ss_public_key: keypair.public.to_bytes(),
            enc_secret: core_secret.to_bytes().into(),
            peer_enc_pubkey: peer_secret
                .public_key()
                .to_encoded_point(false)
                .as_bytes()
                .try_into()
                .unwrap(),
            identity_account_id: peer_public_key,
            session_id_own: [4; 32],
            session_id_peer: [5; 32],
            request_channel: [6; 32],
            response_channel: [7; 32],
            peer_request_channel: [8; 32],
        });
        session.root_entropy_source = Some(keypair.secret.to_bytes()[..32].try_into().unwrap());
        session
    }

    fn peer_statement_keypair() -> ([u8; 64], [u8; 32]) {
        let mini_secret = MiniSecretKey::from_bytes(&[9; 32]).unwrap();
        let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
        (keypair.secret.to_bytes(), keypair.public.to_bytes())
    }

    fn signed_test_statement(data: Vec<u8>) -> Vec<u8> {
        let (secret, public) = peer_statement_keypair();
        crate::host_logic::statement_store::sign_statement_fields(
            secret,
            public,
            vec![crate::host_logic::statement_store::StatementField::Data(
                data,
            )],
        )
        .unwrap()
        .encode()
    }

    fn submitted_remote_message(
        platform: &Arc<StubPlatform>,
        session: &crate::host_logic::session::SessionInfo,
    ) -> crate::host_logic::sso_messages::RemoteMessage {
        let sent = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
        let submit = sent
            .iter()
            .rev()
            .find(|request| request.contains("\"statement_submit\""))
            .expect("statement_submit request should be sent");
        let value: serde_json::Value = serde_json::from_str(submit).unwrap();
        let statement_hex = value["params"][0].as_str().unwrap();
        let statement =
            hex::decode(statement_hex.strip_prefix("0x").unwrap_or(statement_hex)).unwrap();
        let encrypted = crate::host_logic::statement_store::decode_statement_data(&statement)
            .expect("statement data should decode");
        let data = crate::host_logic::sso_pairing::decrypt_session_statement_data(
            session.sso.as_ref().unwrap(),
            &encrypted,
        )
        .expect("statement data should decrypt");
        let crate::host_logic::sso_pairing::SsoStatementData::Request { data, .. } = data else {
            panic!("expected request statement data");
        };
        crate::host_logic::sso_messages::RemoteMessage::decode(&mut data[0].as_slice())
            .expect("remote message should decode")
    }

    fn sso_success_responses(
        session: &crate::host_logic::session::SessionInfo,
        message_id: &str,
        response: crate::host_logic::sso_messages::RemoteMessage,
    ) -> Vec<String> {
        let own_subscription_id = format!("own-sub-{message_id}");
        let peer_subscription_id = format!("peer-sub-{message_id}");
        vec![
            subscribe_ack_frame(
                &format!("truapi:sso-sub-own:{message_id}"),
                &own_subscription_id,
            ),
            subscribe_ack_frame(
                &format!("truapi:sso-sub-peer:{message_id}"),
                &peer_subscription_id,
            ),
            new_statements_frame(
                &own_subscription_id,
                vec![sso_statement(
                    session,
                    crate::host_logic::sso_pairing::SsoStatementData::Response {
                        request_id: message_id.to_string(),
                        response_code: 0,
                    },
                    1,
                )],
            ),
            new_statements_frame(
                &peer_subscription_id,
                vec![sso_statement(
                    session,
                    crate::host_logic::sso_pairing::SsoStatementData::Request {
                        request_id: format!("wallet-response-{message_id}"),
                        data: vec![response.encode()],
                    },
                    2,
                )],
            ),
        ]
    }

    fn sso_peer_disconnect_responses(
        session: &crate::host_logic::session::SessionInfo,
        message_id: &str,
    ) -> Vec<String> {
        let own_subscription_id = format!("own-sub-{message_id}");
        let peer_subscription_id = format!("peer-sub-{message_id}");
        vec![
            subscribe_ack_frame(
                &format!("truapi:sso-sub-own:{message_id}"),
                &own_subscription_id,
            ),
            subscribe_ack_frame(
                &format!("truapi:sso-sub-peer:{message_id}"),
                &peer_subscription_id,
            ),
            new_statements_frame(
                &own_subscription_id,
                vec![sso_statement(
                    session,
                    crate::host_logic::sso_pairing::SsoStatementData::Response {
                        request_id: message_id.to_string(),
                        response_code: 0,
                    },
                    1,
                )],
            ),
            new_statements_frame(
                &peer_subscription_id,
                vec![sso_statement(
                    session,
                    crate::host_logic::sso_pairing::SsoStatementData::Request {
                        request_id: format!("wallet-disconnect-{message_id}"),
                        data: vec![
                            crate::host_logic::sso_messages::RemoteMessage {
                                message_id: format!("wallet-disconnect-{message_id}"),
                                data: crate::host_logic::sso_messages::RemoteMessageData::V1(
                                    crate::host_logic::sso_messages::RemoteMessageV1::Disconnected,
                                ),
                            }
                            .encode(),
                        ],
                    },
                    2,
                )],
            ),
        ]
    }

    fn subscribe_ack_frame(request_id: &str, subscription_id: &str) -> String {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": subscription_id,
        })
        .to_string()
    }

    fn new_statements_frame(subscription_id: &str, statements: Vec<Vec<u8>>) -> String {
        let statements = statements
            .into_iter()
            .map(|statement| format!("0x{}", hex::encode(statement)))
            .collect::<Vec<_>>();
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "statement_subscribeStatement",
            "params": {
                "subscription": subscription_id,
                "result": {
                    "event": "newStatements",
                    "data": {
                        "statements": statements,
                        "remaining": 0,
                    },
                },
            },
        })
        .to_string()
    }

    fn sso_statement(
        session: &crate::host_logic::session::SessionInfo,
        data: crate::host_logic::sso_pairing::SsoStatementData,
        nonce_seed: u8,
    ) -> Vec<u8> {
        let mut nonce = [0; crate::host_logic::sso_pairing::AES_GCM_NONCE_LEN];
        nonce[0] = nonce_seed;
        let encrypted = crate::host_logic::sso_pairing::encrypt_session_statement_data_with_nonce(
            session.sso.as_ref().unwrap(),
            &data,
            nonce,
        )
        .unwrap();
        signed_test_statement(encrypted)
    }

    fn core_encryption_public_key_from_deeplink(deeplink: &str) -> [u8; 65] {
        pairing_device_from_deeplink(deeplink).1
    }

    fn pairing_device_from_deeplink(deeplink: &str) -> ([u8; 32], [u8; 65]) {
        let encoded = deeplink
            .split("handshake=")
            .nth(1)
            .expect("pairing deeplink should include handshake");
        let handshake = hex::decode(encoded).expect("handshake should be hex");
        let decoded = crate::host_logic::sso_pairing::VersionedHandshakeProposal::decode(
            &mut handshake.as_slice(),
        )
        .expect("handshake should decode");
        let crate::host_logic::sso_pairing::VersionedHandshakeProposal::V2(proposal) = decoded
        else {
            panic!("handshake should be V2");
        };
        (
            proposal.device.statement_account_id,
            proposal.device.encryption_public_key,
        )
    }

    fn wallet_handshake_statement(deeplink: &str) -> Vec<u8> {
        let core_public_key =
            P256PublicKey::from_sec1_bytes(&core_encryption_public_key_from_deeplink(deeplink))
                .expect("core encryption public key should decode");
        let wallet_ephemeral_secret = P256SecretKey::from_slice(&[3; 32]).unwrap();
        let wallet_ephemeral_public = wallet_ephemeral_secret.public_key().to_encoded_point(false);
        let mut wallet_ephemeral_public_bytes = [0u8; 65];
        wallet_ephemeral_public_bytes.copy_from_slice(wallet_ephemeral_public.as_bytes());
        let wallet_persistent_public: [u8; 65] = P256SecretKey::from_slice(&[2; 32])
            .unwrap()
            .public_key()
            .to_encoded_point(false)
            .as_bytes()
            .try_into()
            .unwrap();
        let answer = crate::host_logic::sso_pairing::EncryptedHandshakeResponseV2::Success(
            crate::host_logic::sso_pairing::HandshakeSuccessV2 {
                identity_account_id: peer_statement_keypair().1,
                root_account_id: session_info().public_key,
                identity_chat_private_key: [0x77; 32],
                sso_enc_pub_key: wallet_persistent_public,
                device_enc_pub_key: wallet_persistent_public,
                root_entropy_source: [0x66; 32],
            },
        );
        let shared_secret = diffie_hellman(
            wallet_ephemeral_secret.to_nonzero_scalar(),
            core_public_key.as_affine(),
        );
        let hkdf = Hkdf::<Sha256>::new(None, shared_secret.raw_secret_bytes());
        let mut aes_key = [0u8; 32];
        hkdf.expand(&[], &mut aes_key).unwrap();
        let nonce = [0x44; crate::host_logic::sso_pairing::AES_GCM_NONCE_LEN];
        let cipher = Aes256Gcm::new_from_slice(&aes_key).unwrap();
        let mut encrypted_message = nonce.to_vec();
        encrypted_message.extend(
            cipher
                .encrypt(Nonce::from_slice(&nonce), answer.encode().as_slice())
                .unwrap(),
        );
        let handshake = crate::host_logic::sso_pairing::VersionedHandshakeResponse::V2 {
            encrypted_message,
            public_key: wallet_ephemeral_public_bytes,
        };

        signed_test_statement(handshake.encode())
    }

    fn sign_response_message(
        message_id: &str,
        signature: Vec<u8>,
        signed_transaction: Option<Vec<u8>>,
    ) -> crate::host_logic::sso_messages::RemoteMessage {
        crate::host_logic::sso_messages::RemoteMessage {
            message_id: format!("wallet-{message_id}"),
            data: crate::host_logic::sso_messages::RemoteMessageData::V1(
                crate::host_logic::sso_messages::RemoteMessageV1::SignResponse(
                    crate::host_logic::sso_messages::SigningResponse {
                        responding_to: message_id.to_string(),
                        payload: Ok(
                            crate::host_logic::sso_messages::SigningPayloadResponseData {
                                signature,
                                signed_transaction,
                            },
                        ),
                    },
                ),
            ),
        }
    }

    fn account_id(identifier: &str, derivation_index: u32) -> v01::ProductAccountId {
        v01::ProductAccountId {
            dot_ns_identifier: identifier.to_string(),
            derivation_index,
        }
    }

    fn account_alias_request(identifier: &str) -> HostAccountGetAliasRequest {
        HostAccountGetAliasRequest::V1(v01::HostAccountGetAliasRequest {
            product_account_id: account_id(identifier, 0),
        })
    }

    fn raw_payload() -> v01::RawPayload {
        v01::RawPayload::Bytes {
            bytes: b"hello".to_vec(),
        }
    }

    fn sign_payload_data() -> v01::HostSignPayloadData {
        v01::HostSignPayloadData {
            block_hash: vec![0; 32],
            block_number: vec![0; 4],
            era: vec![0],
            genesis_hash: vec![1; 32],
            method: vec![0],
            nonce: vec![0],
            spec_version: vec![0],
            tip: vec![0],
            transaction_version: vec![0],
            signed_extensions: vec![],
            version: 4,
            asset_id: None,
            metadata_hash: None,
            mode: None,
            with_signed_transaction: None,
        }
    }

    fn product_tx_payload(identifier: &str) -> v01::ProductAccountTxPayload {
        v01::ProductAccountTxPayload {
            signer: account_id(identifier, 0),
            genesis_hash: [1; 32],
            call_data: vec![0],
            extensions: vec![],
            tx_ext_version: 0,
        }
    }

    fn resource_allocation_request() -> HostRequestResourceAllocationRequest {
        HostRequestResourceAllocationRequest::V1(v01::HostRequestResourceAllocationRequest {
            resources: vec![
                v01::AllocatableResource::StatementStoreAllowance,
                v01::AllocatableResource::AutoSigning,
            ],
        })
    }

    fn statement() -> v01::Statement {
        v01::Statement {
            proof: None,
            decryption_key: None,
            expiry: Some(99),
            channel: Some([1; 32]),
            topics: vec![[2; 32], [3; 32]],
            data: Some(vec![4, 5, 6]),
        }
    }

    fn signed_statement(topic: [u8; 32]) -> v01::SignedStatement {
        v01::SignedStatement {
            proof: v01::StatementProof::Sr25519 {
                signature: [9; 64],
                signer: [8; 32],
            },
            decryption_key: None,
            expiry: Some(99),
            channel: Some([1; 32]),
            topics: vec![topic],
            data: Some(vec![4, 5, 6]),
        }
    }

    impl PlatformStorage for StubPlatform {
        async fn read(
            &self,
            key: String,
        ) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
            Ok(self
                .local_storage
                .lock()
                .expect("local storage mutex poisoned")
                .get(&key)
                .cloned())
        }
        async fn write(
            &self,
            key: String,
            value: Vec<u8>,
        ) -> Result<(), v01::HostLocalStorageReadError> {
            self.local_storage
                .lock()
                .expect("local storage mutex poisoned")
                .insert(key, value);
            Ok(())
        }
        async fn clear(&self, key: String) -> Result<(), v01::HostLocalStorageReadError> {
            self.local_storage
                .lock()
                .expect("local storage mutex poisoned")
                .remove(&key);
            Ok(())
        }
    }

    impl PlatformNavigation for StubPlatform {
        async fn navigate_to(&self, _url: String) -> Result<(), v01::HostNavigateToError> {
            Ok(())
        }
    }

    impl PlatformNotifications for StubPlatform {
        async fn push_notification(
            &self,
            notification: v01::HostPushNotificationRequest,
        ) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
            self.pushed_notifications
                .lock()
                .expect("notification list mutex poisoned")
                .push(notification);
            Ok(v01::HostPushNotificationResponse {
                id: self.notification_id,
            })
        }

        async fn cancel_notification(&self, id: u32) -> Result<(), v01::GenericError> {
            self.cancelled_notifications
                .lock()
                .expect("notification cancellation list mutex poisoned")
                .push(id);
            Ok(())
        }
    }

    impl PlatformPermissions for StubPlatform {
        async fn device_permission(
            &self,
            _request: v01::HostDevicePermissionRequest,
        ) -> Result<v01::HostDevicePermissionResponse, v01::GenericError> {
            Ok(v01::HostDevicePermissionResponse { granted: true })
        }

        async fn remote_permission(
            &self,
            _request: v01::RemotePermissionRequest,
        ) -> Result<v01::RemotePermissionResponse, v01::GenericError> {
            Ok(v01::RemotePermissionResponse {
                granted: self.remote_permission_granted,
            })
        }
    }

    impl PlatformFeatures for StubPlatform {
        async fn feature_supported(
            &self,
            request: HostFeatureSupportedRequest,
        ) -> Result<HostFeatureSupportedResponse, v01::GenericError> {
            let HostFeatureSupportedRequest::V1(_) = request;
            Ok(HostFeatureSupportedResponse::V1(
                v01::HostFeatureSupportedResponse { supported: true },
            ))
        }
    }

    struct RecordingConnection {
        sent: Arc<Mutex<Vec<String>>>,
        responses: Vec<String>,
        presented_pairings: Arc<Mutex<Vec<String>>>,
        pairing_success_response: bool,
    }

    impl JsonRpcConnection for RecordingConnection {
        fn send(&self, request: String) {
            self.sent
                .lock()
                .expect("rpc list mutex poisoned")
                .push(request);
        }
        fn responses(&self) -> BoxStream<'static, String> {
            if self.pairing_success_response {
                let presented_pairings = self.presented_pairings.clone();
                return Box::pin(stream::unfold(0, move |state| {
                    let presented_pairings = presented_pairings.clone();
                    async move {
                        match state {
                            0 => Some((
                                subscribe_ack_frame(PAIRING_SUBSCRIBE_REQUEST_ID, "pairing-sub"),
                                1,
                            )),
                            1 => {
                                for _ in 0..100 {
                                    let deeplink = presented_pairings
                                        .lock()
                                        .expect("pairing list mutex poisoned")
                                        .first()
                                        .cloned();
                                    if let Some(deeplink) = deeplink {
                                        return Some((
                                            new_statements_frame(
                                                "pairing-sub",
                                                vec![wallet_handshake_statement(&deeplink)],
                                            ),
                                            2,
                                        ));
                                    }
                                    futures_timer::Delay::new(Duration::from_millis(1)).await;
                                }
                                panic!("pairing deeplink was not presented");
                            }
                            _ => None,
                        }
                    }
                }));
            }
            if self.responses.is_empty() {
                Box::pin(futures::stream::pending())
            } else {
                Box::pin(stream::iter(self.responses.clone()))
            }
        }
    }

    impl ChainProvider for StubPlatform {
        async fn connect(
            &self,
            _genesis_hash: Vec<u8>,
        ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
            if let Some(reason) = self.chain_connect_error {
                return Err(v01::GenericError {
                    reason: reason.to_string(),
                });
            }
            Ok(Box::new(RecordingConnection {
                sent: self.sent_rpc.clone(),
                responses: self.rpc_responses.clone(),
                presented_pairings: self.presented_pairings.clone(),
                pairing_success_response: self.pairing_success_response,
            }))
        }
    }

    impl PairingPresenter for StubPlatform {
        async fn present_pairing(&self, deeplink: String) -> Result<(), v01::GenericError> {
            self.presented_pairings
                .lock()
                .expect("pairing list mutex poisoned")
                .push(deeplink);
            if self.pairing_pending {
                futures::future::pending::<()>().await;
            }
            if let Some(reason) = self.pairing_error {
                Err(v01::GenericError {
                    reason: reason.to_string(),
                })
            } else {
                Ok(())
            }
        }
    }

    impl SessionStore for StubPlatform {
        async fn read_session(&self) -> Result<Option<Vec<u8>>, v01::GenericError> {
            if let Some(reason) = self.session_error {
                Err(v01::GenericError {
                    reason: reason.to_string(),
                })
            } else {
                Ok(self.session_blob.clone())
            }
        }
        async fn write_session(&self, value: Vec<u8>) -> Result<(), v01::GenericError> {
            self.session_writes
                .lock()
                .expect("session write list mutex poisoned")
                .push(value);
            Ok(())
        }
        async fn clear_session(&self) -> Result<(), v01::GenericError> {
            *self
                .session_clears
                .lock()
                .expect("session clear counter mutex poisoned") += 1;
            Ok(())
        }
        fn subscribe_session_store(&self) -> BoxStream<'static, Result<(), v01::GenericError>> {
            Box::pin(stream::once(async { Ok(()) }))
        }
    }

    impl UserConfirmation for StubPlatform {
        async fn confirm_sign_payload(&self, _review: Vec<u8>) -> Result<bool, v01::GenericError> {
            if let Some(reason) = self.sign_payload_error {
                Err(v01::GenericError {
                    reason: reason.to_string(),
                })
            } else {
                Ok(self.sign_payload_confirmed)
            }
        }
        async fn confirm_sign_raw(&self, _review: Vec<u8>) -> Result<bool, v01::GenericError> {
            if let Some(reason) = self.sign_raw_error {
                Err(v01::GenericError {
                    reason: reason.to_string(),
                })
            } else {
                Ok(self.sign_raw_confirmed)
            }
        }
        async fn confirm_create_transaction(
            &self,
            _review: Vec<u8>,
        ) -> Result<bool, v01::GenericError> {
            if let Some(reason) = self.create_transaction_error {
                Err(v01::GenericError {
                    reason: reason.to_string(),
                })
            } else {
                Ok(self.create_transaction_confirmed)
            }
        }
        async fn confirm_account_alias(&self, _review: Vec<u8>) -> Result<bool, v01::GenericError> {
            if let Some(reason) = self.account_alias_error {
                Err(v01::GenericError {
                    reason: reason.to_string(),
                })
            } else {
                Ok(self.account_alias_confirmed)
            }
        }
        async fn confirm_resource_allocation(
            &self,
            _review: Vec<u8>,
        ) -> Result<bool, v01::GenericError> {
            if let Some(reason) = self.resource_allocation_error {
                Err(v01::GenericError {
                    reason: reason.to_string(),
                })
            } else {
                Ok(self.resource_allocation_confirmed)
            }
        }
    }

    impl ThemeHost for StubPlatform {
        fn subscribe_theme(
            &self,
        ) -> BoxStream<'static, Result<v01::ThemeVariant, v01::GenericError>> {
            Box::pin(stream::once(async { Ok(v01::ThemeVariant::Dark) }))
        }
    }

    impl PreimageHost for StubPlatform {
        async fn confirm_preimage_submit(
            &self,
            _size: u64,
        ) -> Result<(), v01::PreimageSubmitError> {
            Ok(())
        }
        async fn submit_preimage(
            &self,
            value: Vec<u8>,
        ) -> Result<Vec<u8>, v01::PreimageSubmitError> {
            Ok(value)
        }
        fn lookup_preimage(
            &self,
            _key: Vec<u8>,
        ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
            Box::pin(stream::once(async { Ok(Some(vec![9, 8, 7])) }))
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
                crate::host_logic::sso_messages::RemoteMessage {
                    message_id: "wallet-alias-1".to_string(),
                    data: crate::host_logic::sso_messages::RemoteMessageData::V1(
                        crate::host_logic::sso_messages::RemoteMessageV1::RingVrfAliasResponse(
                            crate::host_logic::sso_messages::RingVrfAliasResponse {
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
        let crate::host_logic::sso_messages::RemoteMessageData::V1(
            crate::host_logic::sso_messages::RemoteMessageV1::RingVrfAliasRequest(request),
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
                crate::host_logic::sso_messages::RemoteMessage {
                    message_id: "wallet-alias-1".to_string(),
                    data: crate::host_logic::sso_messages::RemoteMessageData::V1(
                        crate::host_logic::sso_messages::RemoteMessageV1::RingVrfAliasResponse(
                            crate::host_logic::sso_messages::RingVrfAliasResponse {
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
        let crate::host_logic::sso_messages::RemoteMessageData::V1(
            crate::host_logic::sso_messages::RemoteMessageV1::RingVrfAliasRequest(request),
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
                crate::host_logic::sso_messages::RemoteMessage {
                    message_id: "wallet-alias-2".to_string(),
                    data: crate::host_logic::sso_messages::RemoteMessageData::V1(
                        crate::host_logic::sso_messages::RemoteMessageV1::RingVrfAliasResponse(
                            crate::host_logic::sso_messages::RingVrfAliasResponse {
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
            crate::host_logic::sso_messages::RemoteMessageData::V1(
                crate::host_logic::sso_messages::RemoteMessageV1::RingVrfAliasRequest(_)
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
    fn get_user_id_returns_primary_username_after_permission() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let response =
            futures::executor::block_on(host.get_user_id(&cx, HostGetUserIdRequest::V1)).unwrap();
        let HostGetUserIdResponse::V1(inner) = response;
        assert_eq!(inner.primary_username, "Alice Smith");
    }

    #[test]
    fn get_user_id_denies_when_permission_denied() {
        let host = PlatformRuntimeHost::new_compat(
            Arc::new(StubPlatform {
                remote_permission_granted: false,
                ..Default::default()
            }),
            test_spawner(),
        );
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let err = futures::executor::block_on(host.get_user_id(&cx, HostGetUserIdRequest::V1))
            .unwrap_err();
        assert!(matches!(
            err,
            CallError::Domain(HostGetUserIdError::V1(
                v01::HostGetUserIdError::PermissionDenied
            ))
        ));
    }

    #[test]
    fn statement_store_create_proof_signs_with_session_key() {
        let host =
            PlatformRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        let session = sso_session_info();
        let expected_signer = session.sso.as_ref().unwrap().ss_public_key;
        host.session_state().set_session(session);
        let cx = CallContext::new();
        let request = RemoteStatementStoreCreateProofRequest::V1(
            v01::RemoteStatementStoreCreateProofRequest {
                product_account_id: account_id("myapp.dot", 0),
                statement: statement(),
            },
        );

        let response =
            futures::executor::block_on(StatementStore::create_proof(&host, &cx, request)).unwrap();

        let RemoteStatementStoreCreateProofResponse::V1(inner) = response;
        let v01::StatementProof::Sr25519 { signer, signature } = inner.proof else {
            panic!("expected sr25519 statement proof");
        };
        assert_eq!(signer, expected_signer);
        assert_ne!(signature, [0; 64]);
    }

    #[test]
    fn statement_store_create_proof_rejects_wrong_product_account() {
        let host =
            PlatformRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        host.session_state().set_session(sso_session_info());
        let cx = CallContext::new();
        let request = RemoteStatementStoreCreateProofRequest::V1(
            v01::RemoteStatementStoreCreateProofRequest {
                product_account_id: account_id("other.dot", 0),
                statement: statement(),
            },
        );

        let err = futures::executor::block_on(StatementStore::create_proof(&host, &cx, request))
            .unwrap_err();

        assert!(matches!(
            err,
            CallError::Domain(RemoteStatementStoreCreateProofError::V1(
                v01::RemoteStatementStoreCreateProofError::UnknownAccount
            ))
        ));
    }

    #[test]
    fn statement_store_create_proof_authorized_signs_with_session_key() {
        let host =
            PlatformRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        let session = sso_session_info();
        let expected_signer = session.sso.as_ref().unwrap().ss_public_key;
        host.session_state().set_session(session);
        let cx = CallContext::new();
        let request = RemoteStatementStoreCreateProofAuthorizedRequest::V1(statement());

        let response = futures::executor::block_on(StatementStore::create_proof_authorized(
            &host, &cx, request,
        ))
        .unwrap();

        let RemoteStatementStoreCreateProofAuthorizedResponse::V1(inner) = response;
        let v01::StatementProof::Sr25519 { signer, .. } = inner.proof else {
            panic!("expected sr25519 statement proof");
        };
        assert_eq!(signer, expected_signer);
    }

    #[test]
    fn statement_store_submit_posts_signed_statement_and_waits_for_ack() {
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![r#"{"jsonrpc":"2.0","id":"submit-1","result":"0xok"}"#.to_string()],
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::with_request_id("submit-1".to_string());
        let request = RemoteStatementStoreSubmitRequest::V1(signed_statement([7; 32]));

        futures::executor::block_on(StatementStore::submit(&host, &cx, request)).unwrap();

        let sent = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
        assert_eq!(sent.len(), 1);
        let request: serde_json::Value = serde_json::from_str(&sent[0]).unwrap();
        assert_eq!(request["method"], "statement_submit");
        let statement_hex = request["params"][0].as_str().unwrap();
        let statement =
            hex::decode(statement_hex.strip_prefix("0x").unwrap_or(statement_hex)).unwrap();
        assert_eq!(
            crate::host_logic::statement_store::decode_signed_statement(&statement).unwrap(),
            signed_statement([7; 32])
        );
    }

    #[test]
    fn statement_store_subscribe_maps_signed_pages() {
        let signed = crate::host_logic::statement_store::signed_statement_to_scale(
            signed_statement([7; 32]),
        )
        .unwrap();
        let unsigned = vec![crate::host_logic::statement_store::StatementField::Data(
            vec![1],
        )]
        .encode();
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![
                subscribe_ack_frame("sub-1", "remote-sub"),
                new_statements_frame("remote-sub", vec![unsigned, signed]),
            ],
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::with_request_id("sub-1".to_string());
        let mut subscription = futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                v01::RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7; 32]]),
            ),
        ))
        .unwrap();

        let item = futures::executor::block_on(subscription.next()).expect("statement page");

        let RemoteStatementStoreSubscribeItem::V1(inner) = item;
        assert!(inner.is_complete);
        assert_eq!(inner.statements, vec![signed_statement([7; 32])]);
        let sent = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
        let request: serde_json::Value = serde_json::from_str(&sent[0]).unwrap();
        assert_eq!(request["method"], "statement_subscribeStatement");
        assert_eq!(
            request["params"][0]["matchAny"][0],
            "0x0707070707070707070707070707070707070707070707070707070707070707"
        );
    }

    #[test]
    fn statement_store_subscribe_unsubscribes_remote_subscription_on_drop() {
        let signed = crate::host_logic::statement_store::signed_statement_to_scale(
            signed_statement([7; 32]),
        )
        .unwrap();
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![
                subscribe_ack_frame("sub-drop", "remote-sub-drop"),
                new_statements_frame("remote-sub-drop", vec![signed]),
            ],
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::with_request_id("sub-drop".to_string());
        let mut subscription = futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                v01::RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7; 32]]),
            ),
        ))
        .unwrap();

        let _ = futures::executor::block_on(subscription.next()).expect("statement page");
        drop(subscription);

        let sent = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
        assert_eq!(sent.len(), 2);
        let unsubscribe: serde_json::Value = serde_json::from_str(&sent[1]).unwrap();
        assert_eq!(unsubscribe["method"], "statement_unsubscribeStatement");
        assert_eq!(unsubscribe["params"][0], "remote-sub-drop");
    }

    #[test]
    fn statement_store_subscribe_emits_empty_completion_page_after_filtering() {
        let unsigned = vec![crate::host_logic::statement_store::StatementField::Data(
            vec![1],
        )]
        .encode();
        let platform = Arc::new(StubPlatform {
            rpc_responses: vec![
                subscribe_ack_frame("sub-empty-complete", "remote-sub-empty"),
                new_statements_frame("remote-sub-empty", vec![unsigned]),
            ],
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(platform, runtime_config("myapp.dot"), test_spawner());
        let cx = CallContext::with_request_id("sub-empty-complete".to_string());
        let mut subscription = futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                v01::RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7; 32]]),
            ),
        ))
        .unwrap();

        let item = futures::executor::block_on(subscription.next()).expect("completion page");

        let RemoteStatementStoreSubscribeItem::V1(inner) = item;
        assert!(inner.is_complete);
        assert!(inner.statements.is_empty());
    }

    #[test]
    fn statement_store_subscribe_rejects_topic_limit_violations() {
        let platform = stub_platform();
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::with_request_id("sub-too-many".to_string());
        let topics = vec![[7; 32]; MAX_MATCH_ANY_TOPICS + 1];

        let err = match futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                v01::RemoteStatementStoreSubscribeRequest::MatchAny(topics),
            ),
        )) {
            Ok(_) => panic!("topic limit violation should fail subscription start"),
            Err(err) => err,
        };

        let CallError::Domain(RemoteStatementStoreSubscribeError::V1(reason)) = err else {
            panic!("expected statement-store subscribe domain error");
        };
        assert_eq!(
            reason.reason,
            format!(
                "MatchAny has {} topics, maximum is {}",
                MAX_MATCH_ANY_TOPICS + 1,
                MAX_MATCH_ANY_TOPICS
            )
        );
        assert!(platform.sent_rpc.lock().unwrap().is_empty());
    }

    #[test]
    fn statement_store_subscribe_reports_chain_connect_failure() {
        let platform = Arc::new(StubPlatform {
            chain_connect_error: Some("chain unavailable"),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let cx = CallContext::with_request_id("sub-connect-fail".to_string());

        let err = match futures::executor::block_on(StatementStore::subscribe(
            &host,
            &cx,
            RemoteStatementStoreSubscribeRequest::V1(
                v01::RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7; 32]]),
            ),
        )) {
            Ok(_) => panic!("chain connect failure should fail subscription start"),
            Err(err) => err,
        };

        let CallError::Domain(RemoteStatementStoreSubscribeError::V1(reason)) = err else {
            panic!("expected statement-store subscribe domain error");
        };
        assert!(
            reason
                .reason
                .contains("statement-store connect failed: GenericError"),
            "unexpected reason: {}",
            reason.reason
        );
        assert!(
            reason.reason.contains("chain unavailable"),
            "unexpected reason: {}",
            reason.reason
        );
        assert!(platform.sent_rpc.lock().unwrap().is_empty());
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
            crate::host_logic::sso_messages::RemoteMessageData::V1(
                crate::host_logic::sso_messages::RemoteMessageV1::SignRequest(request)
            ) if matches!(
                request.as_ref(),
                crate::host_logic::sso_messages::SigningRequest::Raw(_)
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
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&sent[3]).unwrap()["params"][0],
            "own-sub-sign-raw-1"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&sent[4]).unwrap()["params"][0],
            "peer-sub-sign-raw-1"
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
            crate::host_logic::sso_messages::RemoteMessageData::V1(
                crate::host_logic::sso_messages::RemoteMessageV1::SignRequest(request)
            ) if matches!(
                request.as_ref(),
                crate::host_logic::sso_messages::SigningRequest::Payload(_)
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
                crate::host_logic::sso_messages::RemoteMessage {
                    message_id: "wallet-create-tx-1".to_string(),
                    data: crate::host_logic::sso_messages::RemoteMessageData::V1(
                        crate::host_logic::sso_messages::RemoteMessageV1::CreateTransactionResponse(
                            crate::host_logic::sso_messages::CreateTransactionResponse {
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
            crate::host_logic::sso_messages::RemoteMessageData::V1(
                crate::host_logic::sso_messages::RemoteMessageV1::CreateTransactionRequest(_)
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
            crate::host_logic::sso_messages::RemoteMessageData::V1(
                crate::host_logic::sso_messages::RemoteMessageV1::SignRequest(request)
            ) if matches!(
                request.as_ref(),
                crate::host_logic::sso_messages::SigningRequest::Raw(_)
            )
        ));
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
                crate::host_logic::sso_messages::RemoteMessage {
                    message_id: "wallet-alloc-1".to_string(),
                    data: crate::host_logic::sso_messages::RemoteMessageData::V1(
                        crate::host_logic::sso_messages::RemoteMessageV1::ResourceAllocationResponse(
                            crate::host_logic::sso_messages::ResourceAllocationResponse {
                                responding_to: "alloc-1".to_string(),
                                payload: Ok(vec![
                                    crate::host_logic::sso_messages::SsoAllocationOutcome::Allocated(
                                        crate::host_logic::sso_messages::SsoAllocatedResource::StatementStoreAllowance {
                                            slot_account_key: vec![1],
                                        },
                                    ),
                                    crate::host_logic::sso_messages::SsoAllocationOutcome::Rejected,
                                    crate::host_logic::sso_messages::SsoAllocationOutcome::NotAvailable,
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
            crate::host_logic::sso_messages::RemoteMessageData::V1(
                crate::host_logic::sso_messages::RemoteMessageV1::ResourceAllocationRequest(_)
            )
        ));
    }

    #[test]
    fn request_login_presents_pairing_and_rejects_when_dismissed() {
        let platform = stub_platform();
        let host = PlatformRuntimeHost::new_compat(platform.clone(), test_spawner());
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
        let presented = platform
            .presented_pairings
            .lock()
            .expect("pairing list mutex poisoned");
        assert_eq!(presented.len(), 1);
        assert!(presented[0].starts_with("polkadotapp://pair?handshake="));

        let sent_rpc = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
        if let Some(sent) = sent_rpc.first() {
            let request: serde_json::Value = serde_json::from_str(sent).unwrap();
            assert_eq!(request["method"], "statement_subscribeStatement");
            assert_eq!(
                request["params"][0]["matchAll"][0].as_str().unwrap().len(),
                66
            );
        }
    }

    #[test]
    fn request_login_maps_pairing_presenter_failure() {
        let host = PlatformRuntimeHost::new_compat(
            Arc::new(StubPlatform {
                pairing_error: Some("present failed"),
                ..Default::default()
            }),
            test_spawner(),
        );
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let err = futures::executor::block_on(host.request_login(&cx, request)).unwrap_err();

        match err {
            CallError::HostFailure { reason } => {
                assert!(reason.contains("present failed"));
            }
            other => panic!("expected presenter host failure, got {other:?}"),
        }
    }

    #[test]
    fn request_login_reuses_persisted_pairing_device_identity() {
        let platform = stub_platform();
        let host = PlatformRuntimeHost::new_compat(platform.clone(), test_spawner());
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });

        let first = futures::executor::block_on(host.request_login(&cx, request.clone())).unwrap();
        let second = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            first,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
        assert_eq!(
            second,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
        let presented = platform
            .presented_pairings
            .lock()
            .expect("pairing list mutex poisoned");
        assert_eq!(presented.len(), 2);
        assert_eq!(
            pairing_device_from_deeplink(&presented[0]),
            pairing_device_from_deeplink(&presented[1])
        );
        assert!(
            platform
                .local_storage
                .lock()
                .expect("local storage mutex poisoned")
                .contains_key(PAIRING_DEVICE_IDENTITY_STORAGE_KEY)
        );
    }

    #[test]
    fn request_login_observes_call_cancellation() {
        let platform = Arc::new(StubPlatform {
            pairing_pending: true,
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new_compat(platform.clone(), test_spawner());
        let cancel = CancellationToken::new();
        let cancel_from_thread = cancel.clone();
        let cx = CallContext::with_parts("login-cancel".to_string(), cancel);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(25));
            cancel_from_thread.cancel();
        });
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });

        let err = futures::executor::block_on(host.request_login(&cx, request)).unwrap_err();

        match err {
            CallError::Domain(HostRequestLoginError::V1(v01::HostRequestLoginError::Unknown {
                reason,
            })) => assert_eq!(reason, CALL_CANCELLED_REASON),
            other => panic!("expected cancellation domain error, got {other:?}"),
        }
        let sent_rpc = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
        assert!(
            sent_rpc
                .iter()
                .any(|request| request.contains(PAIRING_SUBSCRIBE_REQUEST_ID)),
            "pairing subscription should be started before cancellation"
        );
        let presented = platform
            .presented_pairings
            .lock()
            .expect("pairing list mutex poisoned");
        assert_eq!(presented.len(), 1);
    }

    #[test]
    fn request_login_waits_for_pairing_statement() {
        let wallet_ephemeral_secret = p256::SecretKey::from_slice(&[2; 32]).unwrap();
        let wallet_ephemeral_public = wallet_ephemeral_secret.public_key().to_encoded_point(false);
        let mut wallet_ephemeral_public_bytes = [0u8; 65];
        wallet_ephemeral_public_bytes.copy_from_slice(wallet_ephemeral_public.as_bytes());
        let handshake = crate::host_logic::sso_pairing::VersionedHandshakeResponse::V2 {
            encrypted_message: vec![0xde, 0xad],
            public_key: wallet_ephemeral_public_bytes,
        };
        let statement = signed_test_statement(handshake.encode());
        let notification = format!(
            r#"{{"jsonrpc":"2.0","method":"statement_subscribeStatement","params":{{"subscription":"remote-sub","result":{{"event":"newStatements","data":{{"statements":["0x{}"],"remaining":0}}}}}}}}"#,
            hex::encode(statement)
        );
        let platform = Arc::new(StubPlatform {
            pairing_pending: true,
            rpc_responses: vec![
                r#"{"jsonrpc":"2.0","id":"truapi:sso-pairing:1","result":"remote-sub"}"#
                    .to_string(),
                notification,
            ],
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new_compat(platform.clone(), test_spawner());
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let err = futures::executor::block_on(host.request_login(&cx, request)).unwrap_err();

        match err {
            CallError::HostFailure { reason } => {
                assert_eq!(reason, "encrypted SSO handshake answer is too short");
            }
            other => panic!("expected handshake decrypt failure, got {other:?}"),
        }
        let sent_rpc = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
        let methods = sent_rpc
            .iter()
            .map(|request| serde_json::from_str::<serde_json::Value>(request).unwrap())
            .map(|request| request["method"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            methods.first().map(String::as_str),
            Some("statement_subscribeStatement")
        );
        assert!(
            methods
                .iter()
                .any(|method| method == "statement_unsubscribeStatement"),
            "pairing subscription should be cleaned up"
        );
        let unsubscribe: serde_json::Value = serde_json::from_str(&sent_rpc[1]).unwrap();
        assert_eq!(unsubscribe["params"][0], "remote-sub");
    }

    #[test]
    fn request_login_accepts_valid_pairing_statement_and_persists_session() {
        let session_writes = Arc::new(Mutex::new(Vec::new()));
        let platform = Arc::new(StubPlatform {
            pairing_pending: true,
            pairing_success_response: true,
            session_writes: session_writes.clone(),
            ..Default::default()
        });
        let host = PlatformRuntimeHost::new(
            platform.clone(),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        let mut statuses = host.session_state().subscribe();
        assert_eq!(
            futures::executor::block_on(statuses.next()).unwrap(),
            HostAccountConnectionStatusSubscribeItem::V1(
                v01::HostAccountConnectionStatusSubscribeItem::Disconnected
            )
        );

        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Success)
        );
        assert_eq!(
            futures::executor::block_on(statuses.next()).unwrap(),
            HostAccountConnectionStatusSubscribeItem::V1(
                v01::HostAccountConnectionStatusSubscribeItem::Connected
            )
        );

        let session = host
            .session_state()
            .current()
            .expect("paired session should be active");
        assert_eq!(session.public_key, session_info().public_key);
        assert_eq!(session.root_entropy_source, Some([0x66; 32]));
        assert_eq!(
            session.sso.as_ref().unwrap().identity_account_id,
            peer_statement_keypair().1
        );

        let writes = session_writes
            .lock()
            .expect("session write list mutex poisoned");
        assert_eq!(writes.len(), 1);
        assert_eq!(
            crate::host_logic::session::decode_persisted_session(&writes[0]).unwrap(),
            session
        );

        let methods = platform
            .sent_rpc
            .lock()
            .expect("rpc list mutex poisoned")
            .iter()
            .map(|request| serde_json::from_str::<serde_json::Value>(request).unwrap())
            .map(|request| request["method"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            methods.first().map(String::as_str),
            Some("statement_subscribeStatement")
        );
        assert!(
            methods
                .iter()
                .any(|method| method == "statement_unsubscribeStatement"),
            "pairing subscription should be cleaned up"
        );
    }

    #[test]
    fn request_login_does_not_restore_persisted_session_before_pairing() {
        let stored = session_info();
        let host = PlatformRuntimeHost::new_compat(
            Arc::new(StubPlatform {
                session_blob: Some(crate::host_logic::session::encode_persisted_session(
                    &stored,
                )),
                ..Default::default()
            }),
            test_spawner(),
        );
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
        assert!(host.session_state().current().is_none());
    }

    #[test]
    fn request_login_ignores_corrupt_persisted_session_before_pairing() {
        let session_clears = Arc::new(Mutex::new(0));
        let host = PlatformRuntimeHost::new_compat(
            Arc::new(StubPlatform {
                session_blob: Some(vec![0xff]),
                session_clears: session_clears.clone(),
                ..Default::default()
            }),
            test_spawner(),
        );
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
        assert!(host.session_state().current().is_none());
        assert_eq!(*session_clears.lock().unwrap(), 0);
    }

    #[test]
    fn session_store_sync_restores_valid_blob_from_tick() {
        let stored = sso_session_info();
        let host = PlatformRuntimeHost::new_compat(
            Arc::new(StubPlatform {
                session_blob: Some(crate::host_logic::session::encode_persisted_session(
                    &stored,
                )),
                ..Default::default()
            }),
            test_spawner(),
        );

        host.start_session_store_sync(immediate_spawner());

        assert_eq!(host.session_state().current(), Some(stored));
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

        host.start_session_store_sync(immediate_spawner());

        assert_eq!(host.session_state().current(), Some(replacement));
        assert_eq!(
            futures::executor::block_on(statuses.next()).unwrap(),
            HostAccountConnectionStatusSubscribeItem::V1(
                v01::HostAccountConnectionStatusSubscribeItem::Connected
            )
        );
    }

    #[test]
    fn session_store_sync_clears_invalid_blob() {
        let host = PlatformRuntimeHost::new_compat(
            Arc::new(StubPlatform {
                session_blob: Some(vec![0xff]),
                ..Default::default()
            }),
            test_spawner(),
        );
        host.session_state().set_session(sso_session_info());

        host.start_session_store_sync(immediate_spawner());

        assert!(host.session_state().current().is_none());
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

        host.start_session_store_sync(immediate_spawner());

        assert!(host.session_state().current().is_none());
        assert_eq!(*session_clears.lock().unwrap(), 1);
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
                PAIRING_DEVICE_IDENTITY_STORAGE_KEY.to_string(),
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
                .contains_key(PAIRING_DEVICE_IDENTITY_STORAGE_KEY),
            "logout must rotate the pairing device identity so stale statement-store responses cannot be replayed on the next login"
        );
        assert_eq!(
            futures::executor::block_on(statuses.next()).unwrap(),
            HostAccountConnectionStatusSubscribeItem::V1(
                v01::HostAccountConnectionStatusSubscribeItem::Disconnected
            )
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
    fn sso_remote_response_waiter_times_out() {
        let session = sso_session_info();
        let err = futures::executor::block_on(wait_for_sso_remote_response(
            stream::pending().boxed(),
            SsoRemoteResponseWait {
                session: session.sso.as_ref().unwrap(),
                own_subscription_request_id: "own-sub",
                peer_subscription_request_id: "peer-sub",
                submit_request_id: "submit",
                statement_request_id: "request-1",
                remote_message_id: "request-1",
                timeout: Some(Duration::from_millis(1)),
                disconnect: None,
                own_remote_subscription_id: remote_subscription_slot(),
                peer_remote_subscription_id: remote_subscription_slot(),
            },
        ))
        .unwrap_err();

        assert_eq!(err, "SSO response timed out after 1ms for request-1");
    }

    #[test]
    fn sso_remote_response_waiter_reports_submit_rejections() {
        let session = sso_session_info();
        let err = futures::executor::block_on(wait_for_sso_remote_response(
            stream::iter(vec![
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": "submit",
                    "error": {
                        "code": -32000,
                        "message": "no allowance"
                    },
                })
                .to_string(),
            ])
            .boxed(),
            SsoRemoteResponseWait {
                session: session.sso.as_ref().unwrap(),
                own_subscription_request_id: "own-sub",
                peer_subscription_request_id: "peer-sub",
                submit_request_id: "submit",
                statement_request_id: "request-1",
                remote_message_id: "request-1",
                timeout: Some(Duration::from_secs(60)),
                disconnect: None,
                own_remote_subscription_id: remote_subscription_slot(),
                peer_remote_subscription_id: remote_subscription_slot(),
            },
        ))
        .unwrap_err();

        assert_eq!(
            err,
            "SSO statement submit failed: malformed statement-store frame: no allowance"
        );
    }

    #[test]
    fn sso_remote_response_waiter_stops_on_local_disconnect_signal() {
        let session = sso_session_info();
        let (tx, rx) = oneshot::channel();
        tx.send(SSO_LOCAL_DISCONNECT_REASON.to_string()).unwrap();
        let err = futures::executor::block_on(wait_for_sso_remote_response(
            stream::pending().boxed(),
            SsoRemoteResponseWait {
                session: session.sso.as_ref().unwrap(),
                own_subscription_request_id: "own-sub",
                peer_subscription_request_id: "peer-sub",
                submit_request_id: "submit",
                statement_request_id: "request-1",
                remote_message_id: "request-1",
                timeout: Some(Duration::from_secs(60)),
                disconnect: Some(rx),
                own_remote_subscription_id: remote_subscription_slot(),
                peer_remote_subscription_id: remote_subscription_slot(),
            },
        ))
        .unwrap_err();

        assert_eq!(err, SSO_LOCAL_DISCONNECT_REASON);
    }

    #[test]
    fn sso_remote_response_waiter_without_timeout_stops_on_local_disconnect_signal() {
        let session = sso_session_info();
        let (tx, rx) = oneshot::channel();
        tx.send(SSO_LOCAL_DISCONNECT_REASON.to_string()).unwrap();
        let err = futures::executor::block_on(wait_for_sso_remote_response(
            stream::pending().boxed(),
            SsoRemoteResponseWait {
                session: session.sso.as_ref().unwrap(),
                own_subscription_request_id: "own-sub",
                peer_subscription_request_id: "peer-sub",
                submit_request_id: "submit",
                statement_request_id: "request-1",
                remote_message_id: "request-1",
                timeout: None,
                disconnect: Some(rx),
                own_remote_subscription_id: remote_subscription_slot(),
                peer_remote_subscription_id: remote_subscription_slot(),
            },
        ))
        .unwrap_err();

        assert_eq!(err, SSO_LOCAL_DISCONNECT_REASON);
    }

    #[test]
    fn request_login_ignores_session_store_failure_before_pairing() {
        let host = PlatformRuntimeHost::new_compat(
            Arc::new(StubPlatform {
                session_error: Some("storage failed"),
                ..Default::default()
            }),
            test_spawner(),
        );
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();

        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Rejected)
        );
        assert!(host.session_state().current().is_none());
    }

    #[test]
    fn request_login_returns_already_connected_when_session_exists() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let response = futures::executor::block_on(host.request_login(&cx, request)).unwrap();
        assert_eq!(
            response,
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::AlreadyConnected)
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
