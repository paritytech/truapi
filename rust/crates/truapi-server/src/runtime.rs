//! `PlatformRuntimeHost<P>` adapts a [`truapi_platform::Platform`] into the
//! typed `truapi::api::*` host traits the generated dispatcher routes to.
//!
//! Most methods are straight delegations to the platform; the rest carry
//! host-agnostic logic owned by the core (the chainHead-v1 runtime behind
//! the Chain surface, `dotns` URL parsing for `navigate_to`, and the
//! permission cache layer). Methods with no platform backing return
//! `CallError::unavailable()`.

use std::sync::Arc;

use crate::chain_runtime::{
    ChainRuntime, RuntimeChainProvider, RuntimeFailure, RuntimeFailureKind,
};
use crate::host_logic::dotns::{NavigateDecision, parse_navigate};
use crate::host_logic::entropy::derive_product_entropy;
use crate::host_logic::features::feature_supported;
use crate::host_logic::permissions::{Decision, PermissionsService};
use crate::host_logic::product_account::{
    derive_product_public_key, is_product_identifier, product_public_key_to_address,
};
use crate::host_logic::session::{
    SessionInfo, SessionState, SsoSessionInfo, decode_persisted_session, encode_persisted_session,
};
use crate::host_logic::sso_messages::{
    OnExistingAllowancePolicy, RemoteMessage, SsoAllocationOutcome, SsoRemoteResponse,
    SsoSessionStatement, alias_request_message, build_outgoing_request_statement,
    create_transaction_message, decode_sso_session_statement, resource_allocation_message,
    sign_payload_message, sign_raw_message,
};
use crate::host_logic::sso_pairing::{
    AppHandshakeData, create_pairing_bootstrap, decode_app_handshake_data,
    decrypt_handshake_answer, establish_sso_session_info,
};
use crate::host_logic::statement_store::{
    decode_statement_data, parse_new_statements, parse_subscribe_ack, submit_statement_request,
    subscribe_match_all_request,
};
use crate::subscription::Spawner;

use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt, pin_mut};
use parity_scale_codec::Encode;
use truapi::api::{
    Account, Chain, Chat, CoinPayment, Entropy, LocalStorage, Notifications, Payment, Permissions,
    Preimage, ResourceAllocation, Signing, StatementStore, System, Theme,
};
use truapi::v01;
use truapi::versioned::account::{
    HostAccountConnectionStatusSubscribeItem, HostAccountGetAliasError, HostAccountGetAliasRequest,
    HostAccountGetAliasResponse, HostAccountGetError, HostAccountGetRequest,
    HostAccountGetResponse, HostGetLegacyAccountsError, HostGetLegacyAccountsRequest,
    HostGetLegacyAccountsResponse, HostGetUserIdError, HostGetUserIdRequest, HostGetUserIdResponse,
    HostRequestLoginError, HostRequestLoginRequest, HostRequestLoginResponse,
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
    ChainProvider as PlatformChainProvider, JsonRpcConnection, Navigation as PlatformNavigation,
    Notifications as PlatformNotifications, PairingPresenter as PlatformPairingPresenter, Platform,
    PreimageHost as PlatformPreimageHost, RuntimeConfig, SessionStore as PlatformSessionStore,
    Storage as PlatformStorage, ThemeHost as PlatformThemeHost,
    UserConfirmation as PlatformUserConfirmation,
};

const PAIRING_SUBSCRIBE_REQUEST_ID: &str = "truapi:sso-pairing:1";
const DEFAULT_SSO_STATEMENT_EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;

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
        }
    }

    /// Compatibility constructor used by existing tests and bridge surfaces
    /// until they pass real product runtime config.
    pub fn new_compat(platform: Arc<P>, spawner: Spawner) -> Self
    where
        P: Platform + 'static,
    {
        Self::new(platform, RuntimeConfig::compatibility_default(), spawner)
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

    /// Clone of the shared session-state holder. The platform bridge layer
    /// (`setActiveSession` / `clearActiveSession`) routes through this handle.
    pub fn session_state(&self) -> Arc<SessionState> {
        self.session_state.clone()
    }

    /// Static product/host configuration for this runtime instance.
    pub fn runtime_config(&self) -> &RuntimeConfig {
        &self.runtime_config
    }

    fn is_product_account_valid_for_caller(&self, dot_ns_identifier: &str) -> bool {
        if self.runtime_config.product_label.starts_with("localhost:") {
            is_product_identifier(dot_ns_identifier)
        } else {
            dot_ns_identifier == self.runtime_config.product_id
        }
    }

    fn legacy_slot_zero_public_key(&self, session: &SessionInfo) -> Result<[u8; 32], String> {
        derive_product_public_key(session.public_key, &self.runtime_config.product_id, 0)
            .map_err(|err| err.to_string())
    }
}

impl<P> PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    async fn chain_submit_decision(&self) -> Result<Decision, String> {
        let service = PermissionsService::new(self.platform.as_ref(), self.platform.as_ref());
        service
            .check_or_prompt_remote(v01::RemotePermissionRequest {
                permission: v01::RemotePermission::ChainSubmit,
            })
            .await
            .map_err(|err| format!("permission storage failed: {err:?}"))
    }

    async fn submit_sso_remote_message(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        action: &str,
        message: RemoteMessage,
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
        connection.send(subscribe_match_all_request(
            &own_subscription_request_id,
            &[sso.session_id_own],
        ));
        connection.send(subscribe_match_all_request(
            &peer_subscription_request_id,
            &[sso.session_id_peer],
        ));
        connection.send(submit_statement_request(
            &format!("truapi:sso-submit:{message_id}"),
            &statement,
        ));
        wait_for_sso_remote_response(
            connection.responses(),
            sso,
            &own_subscription_request_id,
            &peer_subscription_request_id,
            &message_id,
            &message_id,
        )
        .await
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

async fn wait_for_sso_remote_response(
    mut responses: BoxStream<'static, String>,
    session: &SsoSessionInfo,
    own_subscription_request_id: &str,
    peer_subscription_request_id: &str,
    statement_request_id: &str,
    remote_message_id: &str,
) -> Result<SsoRemoteResponse, String> {
    let mut own_remote_subscription_id = None;
    let mut peer_remote_subscription_id = None;
    let mut request_accepted = false;
    let mut pending_remote_response = None;

    while let Some(frame) = responses.next().await {
        if own_remote_subscription_id.is_none() {
            if let Some(id) = parse_subscribe_ack(&frame, own_subscription_request_id)
                .map_err(|err| err.to_string())?
            {
                own_remote_subscription_id = Some(id);
                continue;
            }
        }
        if peer_remote_subscription_id.is_none() {
            if let Some(id) = parse_subscribe_ack(&frame, peer_subscription_request_id)
                .map_err(|err| err.to_string())?
            {
                peer_remote_subscription_id = Some(id);
                continue;
            }
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
                session,
                &statement,
                statement_request_id,
                remote_message_id,
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
                None => {}
            }
        }
    }

    Err(format!(
        "SSO response stream ended before response for {remote_message_id}"
    ))
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
    async fn feature_supported(
        &self,
        _cx: &CallContext,
        request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, CallError<HostFeatureSupportedError>> {
        feature_supported(self.platform.as_ref(), request)
            .await
            .map_err(|err| CallError::Domain(HostFeatureSupportedError::V1(err)))
    }

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
    async fn get_account(
        &self,
        _cx: &CallContext,
        request: HostAccountGetRequest,
    ) -> Result<HostAccountGetResponse, CallError<HostAccountGetError>> {
        let HostAccountGetRequest::V1(v01::HostAccountGetRequest { product_account_id }) = request;

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

    async fn get_account_alias(
        &self,
        cx: &CallContext,
        request: HostAccountGetAliasRequest,
    ) -> Result<HostAccountGetAliasResponse, CallError<HostAccountGetAliasError>> {
        let HostAccountGetAliasRequest::V1(v01::HostAccountGetAliasRequest { product_account_id }) =
            request;

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

        if product_account_id.dot_ns_identifier != self.runtime_config.product_id {
            let confirmed = PlatformUserConfirmation::confirm_account_alias(
                self.platform.as_ref(),
                (
                    self.runtime_config.product_id.clone(),
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
        let message = alias_request_message(
            message_id.clone(),
            product_account_id,
            self.runtime_config.product_id.clone(),
        );
        let response = self
            .submit_sso_remote_message(cx, &session, "account-alias", message)
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

        if !is_product_identifier(&self.runtime_config.product_id) {
            return Err(CallError::Domain(HostGetLegacyAccountsError::V1(
                v01::HostAccountGetError::DomainNotValid,
            )));
        }

        let public_key =
            derive_product_public_key(session.public_key, &self.runtime_config.product_id, 0)
                .map_err(|err| {
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

    async fn connection_status_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<HostAccountConnectionStatusSubscribeItem> {
        Subscription::new(self.session_state.subscribe())
    }

    async fn request_login(
        &self,
        _cx: &CallContext,
        _request: HostRequestLoginRequest,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        if self.session_state.current().is_some() {
            return Ok(HostRequestLoginResponse::V1(
                v01::HostRequestLoginResponse::AlreadyConnected,
            ));
        }
        match PlatformSessionStore::read_session(self.platform.as_ref()).await {
            Ok(Some(blob)) => {
                let session = decode_persisted_session(&blob).map_err(|reason| {
                    CallError::Domain(HostRequestLoginError::V1(
                        v01::HostRequestLoginError::Unknown { reason },
                    ))
                })?;
                self.session_state.set_session(session);
                return Ok(HostRequestLoginResponse::V1(
                    v01::HostRequestLoginResponse::Success,
                ));
            }
            Ok(None) => {}
            Err(err) => {
                return Err(CallError::Domain(HostRequestLoginError::V1(
                    v01::HostRequestLoginError::Unknown {
                        reason: format!("session restore failed: {err:?}"),
                    },
                )));
            }
        }

        let bootstrap = create_pairing_bootstrap(&self.runtime_config).map_err(|err| {
            CallError::Domain(HostRequestLoginError::V1(
                v01::HostRequestLoginError::Unknown {
                    reason: err.to_string(),
                },
            ))
        })?;
        let statement_store = PlatformChainProvider::connect(
            self.platform.as_ref(),
            self.runtime_config.people_chain_genesis_hash.to_vec(),
        )
        .await
        .map_err(|err| CallError::HostFailure {
            reason: format!("pairing statement-store connect failed: {err:?}"),
        })?;
        statement_store.send(subscribe_match_all_request(
            PAIRING_SUBSCRIBE_REQUEST_ID,
            &[bootstrap.topic],
        ));
        let core_encryption_secret_key = bootstrap.encryption_secret_key;
        let presenter = PlatformPairingPresenter::present_pairing(
            self.platform.as_ref(),
            bootstrap.deeplink.clone(),
        )
        .fuse();
        let pairing_statement = wait_for_pairing_statement(statement_store.responses()).fuse();
        pin_mut!(presenter, pairing_statement);

        futures::select! {
            presenter_result = presenter => {
                presenter_result.map_err(|err| CallError::HostFailure {
                    reason: format!("pairing presentation failed: {err:?}"),
                })?;
                Ok(HostRequestLoginResponse::V1(
                    v01::HostRequestLoginResponse::Rejected,
                ))
            }
            statement_result = pairing_statement => {
                let statement = statement_result.map_err(|reason| CallError::HostFailure {
                    reason,
                })?;
                let payload = decode_statement_data(&statement).map_err(|err| CallError::HostFailure {
                    reason: err.to_string(),
                })?;
                let handshake = decode_app_handshake_data(&payload).map_err(|reason| CallError::HostFailure {
                    reason,
                })?;
                let AppHandshakeData::V1 {
                    encrypted_message,
                    public_key,
                } = handshake;
                let answer = decrypt_handshake_answer(
                    core_encryption_secret_key,
                    public_key,
                    &encrypted_message,
                )
                .map_err(|reason| CallError::HostFailure { reason })?;
                let sso = establish_sso_session_info(&bootstrap, &answer)
                    .map_err(|reason| CallError::HostFailure { reason })?;
                let session = SessionInfo {
                    public_key: answer.root_user_account_id,
                    sso: Some(sso),
                    entropy_secret: Some(bootstrap.statement_store_secret.to_vec()),
                    lite_username: None,
                    full_username: None,
                };
                PlatformSessionStore::write_session(
                    self.platform.as_ref(),
                    encode_persisted_session(&session),
                )
                .await
                .map_err(|err| CallError::HostFailure {
                    reason: format!("session persist failed: {err:?}"),
                })?;
                self.session_state.set_session(session);
                Ok(HostRequestLoginResponse::V1(
                    v01::HostRequestLoginResponse::Success,
                ))
            }
        }
    }
}

async fn wait_for_pairing_statement(
    mut responses: BoxStream<'static, String>,
) -> Result<Vec<u8>, String> {
    let mut remote_subscription_id = None;
    while let Some(frame) = responses.next().await {
        if remote_subscription_id.is_none() {
            if let Some(id) = parse_subscribe_ack(&frame, PAIRING_SUBSCRIBE_REQUEST_ID)
                .map_err(|err| err.to_string())?
            {
                remote_subscription_id = Some(id);
                continue;
            }
        }

        let Some(page) = parse_new_statements(&frame).map_err(|err| err.to_string())? else {
            continue;
        };
        if remote_subscription_id
            .as_ref()
            .is_some_and(|expected| expected != &page.remote_subscription_id)
        {
            continue;
        }
        if let Some(statement) = page.statements.into_iter().next() {
            return Ok(statement);
        }
    }

    Err("pairing statement-store response stream ended".to_string())
}

impl<P> Signing for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    async fn sign_payload(
        &self,
        cx: &CallContext,
        request: HostSignPayloadRequest,
    ) -> Result<HostSignPayloadResponse, CallError<HostSignPayloadError>> {
        let HostSignPayloadRequest::V1(inner) = request;
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

    async fn sign_raw(
        &self,
        cx: &CallContext,
        request: HostSignRawRequest,
    ) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
        let HostSignRawRequest::V1(inner) = request;
        if !self.is_product_account_valid_for_caller(&inner.account.dot_ns_identifier) {
            return Err(CallError::Domain(HostSignRawError::V1(
                v01::HostSignPayloadError::PermissionDenied,
            )));
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

    async fn create_transaction(
        &self,
        cx: &CallContext,
        request: HostCreateTransactionRequest,
    ) -> Result<HostCreateTransactionResponse, CallError<HostCreateTransactionError>> {
        let HostCreateTransactionRequest::V1(inner) = request;
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
                    dot_ns_identifier: self.runtime_config.product_id.clone(),
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
                    dot_ns_identifier: self.runtime_config.product_id.clone(),
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
                    dot_ns_identifier: self.runtime_config.product_id.clone(),
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

impl<P> StatementStore for PlatformRuntimeHost<P> where P: Platform + 'static {}

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
// Traits that defer entirely to default "unavailable" trait bodies.
//
// These API surfaces (Chat, CoinPayment, Payment)
// are not part of the v0.1 platform contract, so we leave every method at its
// default `Err(CallError::unavailable())` body and supply empty trait impls
// here. Adding a method later only requires implementing the relevant
// `truapi_platform::*` extension trait.

impl<P> Chat for PlatformRuntimeHost<P> where P: Platform + 'static {}
impl<P> CoinPayment for PlatformRuntimeHost<P> where P: Platform + 'static {}
impl<P> Payment for PlatformRuntimeHost<P> where P: Platform + 'static {}

impl<P> ResourceAllocation for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
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
            self.runtime_config.product_id.clone(),
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
        let Some(entropy_secret) = session.entropy_secret else {
            return Err(CallError::Domain(HostDeriveEntropyError::V1(
                v01::HostDeriveEntropyError::Unknown {
                    reason: "Session secret missing".to_string(),
                },
            )));
        };

        let entropy =
            derive_product_entropy(&entropy_secret, &self.runtime_config.product_id, &context)
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
    async fn subscribe(&self, _cx: &CallContext) -> Subscription<HostThemeSubscribeItem> {
        let stream =
            PlatformThemeHost::subscribe_theme(self.platform.as_ref()).filter_map(|item| async {
                item.ok()
                    .map(|theme| HostThemeSubscribeItem::V1(v01::HostThemeSubscribeItem { theme }))
            });
        Subscription::new(Box::pin(stream))
    }
}

// `Notifications` methods are required (no default bodies), so the
// unavailable stubs are spelled out. The v0.1 platform contract does not
// model host-assigned notification ids, cancellation, or scheduling.
impl<P> Notifications for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
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
    use crate::chain_runtime::thread_per_task_spawner;
    use futures::stream::{self, BoxStream};
    use p256::SecretKey as P256SecretKey;
    use p256::elliptic_curve::sec1::ToEncodedPoint;
    use parity_scale_codec::{Decode, Encode};
    use schnorrkel::{ExpansionMode, MiniSecretKey};
    use std::sync::Mutex;
    use truapi::v01;
    use truapi_platform::{
        ChainProvider, Features as PlatformFeatures, JsonRpcConnection,
        Navigation as PlatformNavigation, Notifications as PlatformNotifications, PairingPresenter,
        Permissions as PlatformPermissions, PreimageHost, SessionStore, Storage as PlatformStorage,
        ThemeHost, UserConfirmation,
    };

    fn test_spawner() -> Spawner {
        thread_per_task_spawner()
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
        pairing_error: Option<&'static str>,
        pairing_pending: bool,
        presented_pairings: Arc<Mutex<Vec<String>>>,
        sent_rpc: Arc<Mutex<Vec<String>>>,
        rpc_responses: Vec<String>,
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
                pairing_error: None,
                pairing_pending: false,
                presented_pairings: Arc::new(Mutex::new(Vec::new())),
                sent_rpc: Arc::new(Mutex::new(Vec::new())),
                rpc_responses: Vec::new(),
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
            host_metadata_url: "https://example.invalid/metadata.json".to_string(),
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
            entropy_secret: Some((0..32).map(|i| i as u8).collect()),
            lite_username: Some("alice".to_string()),
            full_username: Some("Alice Smith".to_string()),
        }
    }

    fn sso_session_info() -> crate::host_logic::session::SessionInfo {
        let mut session = session_info();
        let mini_secret = MiniSecretKey::from_bytes(&[7; 32]).unwrap();
        let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
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
            identity_account_id: [3; 32],
            session_id_own: [4; 32],
            session_id_peer: [5; 32],
            request_channel: [6; 32],
            response_channel: [7; 32],
            peer_request_channel: [8; 32],
        });
        session.entropy_secret = Some(keypair.secret.to_bytes().to_vec());
        session
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
        vec![crate::host_logic::statement_store::StatementField::Data(
            encrypted,
        )]
        .encode()
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

    impl PlatformStorage for StubPlatform {
        async fn read(
            &self,
            _key: String,
        ) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
            Ok(None)
        }
        async fn write(
            &self,
            _key: String,
            _value: Vec<u8>,
        ) -> Result<(), v01::HostLocalStorageReadError> {
            Ok(())
        }
        async fn clear(&self, _key: String) -> Result<(), v01::HostLocalStorageReadError> {
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
            _notification: v01::HostPushNotificationRequest,
        ) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
            Ok(v01::HostPushNotificationResponse { id: 0 })
        }

        async fn cancel_notification(&self, _id: u32) -> Result<(), v01::GenericError> {
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
    }

    impl JsonRpcConnection for RecordingConnection {
        fn send(&self, request: String) {
            self.sent
                .lock()
                .expect("rpc list mutex poisoned")
                .push(request);
        }
        fn responses(&self) -> BoxStream<'static, String> {
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
            Ok(Box::new(RecordingConnection {
                sent: self.sent_rpc.clone(),
                responses: self.rpc_responses.clone(),
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
        async fn write_session(&self, _value: Vec<u8>) -> Result<(), v01::GenericError> {
            Ok(())
        }
        async fn clear_session(&self) -> Result<(), v01::GenericError> {
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
        fn subscribe_theme(&self) -> BoxStream<'static, Result<v01::Theme, v01::GenericError>> {
            Box::pin(stream::once(async { Ok(v01::Theme::Dark) }))
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
        assert!(matches!(
            message.data,
            crate::host_logic::sso_messages::RemoteMessageData::V1(
                crate::host_logic::sso_messages::RemoteMessageV1::RingVrfAliasRequest(_)
            )
        ));
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
        session.entropy_secret = None;
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
                theme: v01::Theme::Dark
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
            message.data,
            crate::host_logic::sso_messages::RemoteMessageData::V1(
                crate::host_logic::sso_messages::RemoteMessageV1::SignRequest(
                    crate::host_logic::sso_messages::SigningRequest::Raw(_)
                )
            )
        ));
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
            message.data,
            crate::host_logic::sso_messages::RemoteMessageData::V1(
                crate::host_logic::sso_messages::RemoteMessageV1::SignRequest(
                    crate::host_logic::sso_messages::SigningRequest::Payload(_)
                )
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
            message.data,
            crate::host_logic::sso_messages::RemoteMessageData::V1(
                crate::host_logic::sso_messages::RemoteMessageV1::SignRequest(
                    crate::host_logic::sso_messages::SigningRequest::Raw(_)
                )
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
        assert_eq!(sent_rpc.len(), 1);
        let request: serde_json::Value = serde_json::from_str(&sent_rpc[0]).unwrap();
        assert_eq!(request["method"], "statement_subscribeStatement");
        assert_eq!(
            request["params"][0]["matchAll"][0].as_str().unwrap().len(),
            66
        );
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
    fn request_login_waits_for_pairing_statement() {
        let wallet_ephemeral_secret = p256::SecretKey::from_slice(&[2; 32]).unwrap();
        let wallet_ephemeral_public = wallet_ephemeral_secret.public_key().to_encoded_point(false);
        let mut wallet_ephemeral_public_bytes = [0u8; 65];
        wallet_ephemeral_public_bytes.copy_from_slice(wallet_ephemeral_public.as_bytes());
        let handshake = crate::host_logic::sso_pairing::AppHandshakeData::V1 {
            encrypted_message: vec![0xde, 0xad],
            public_key: wallet_ephemeral_public_bytes,
        };
        let statement = vec![crate::host_logic::statement_store::StatementField::Data(
            handshake.encode(),
        )]
        .encode();
        let notification = format!(
            r#"{{"jsonrpc":"2.0","method":"statement_subscribeStatement","params":{{"subscription":"remote-sub","result":{{"event":"newStatements","data":{{"statements":["0x{}"],"remaining":0}}}}}}}}"#,
            hex::encode(statement)
        );
        let host = PlatformRuntimeHost::new_compat(
            Arc::new(StubPlatform {
                pairing_pending: true,
                rpc_responses: vec![
                    r#"{"jsonrpc":"2.0","id":"truapi:sso-pairing:1","result":"remote-sub"}"#
                        .to_string(),
                    notification,
                ],
                ..Default::default()
            }),
            test_spawner(),
        );
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let err = futures::executor::block_on(host.request_login(&cx, request)).unwrap_err();

        match err {
            CallError::HostFailure { reason } => {
                assert_eq!(reason, "encrypted SSO handshake answer is too short");
            }
            other => panic!("expected handshake decrypt failure, got {other:?}"),
        }
    }

    #[test]
    fn request_login_restores_persisted_session() {
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
            HostRequestLoginResponse::V1(v01::HostRequestLoginResponse::Success)
        );
        assert_eq!(host.session_state().current(), Some(stored));
    }

    #[test]
    fn request_login_rejects_corrupt_persisted_session() {
        let host = PlatformRuntimeHost::new_compat(
            Arc::new(StubPlatform {
                session_blob: Some(vec![0xff]),
                ..Default::default()
            }),
            test_spawner(),
        );
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let err = futures::executor::block_on(host.request_login(&cx, request)).unwrap_err();

        match err {
            CallError::Domain(HostRequestLoginError::V1(v01::HostRequestLoginError::Unknown {
                reason,
            })) => assert!(reason.contains("invalid session blob")),
            other => panic!("expected corrupt session login error, got {other:?}"),
        }
        assert!(host.session_state().current().is_none());
    }

    #[test]
    fn request_login_maps_session_store_failure() {
        let host = PlatformRuntimeHost::new_compat(
            Arc::new(StubPlatform {
                session_error: Some("storage failed"),
                ..Default::default()
            }),
            test_spawner(),
        );
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let err = futures::executor::block_on(host.request_login(&cx, request)).unwrap_err();

        match err {
            CallError::Domain(HostRequestLoginError::V1(v01::HostRequestLoginError::Unknown {
                reason,
            })) => assert!(reason.contains("storage failed")),
            other => panic!("expected session-store login error, got {other:?}"),
        }
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
