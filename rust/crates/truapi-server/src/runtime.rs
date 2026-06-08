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
use crate::host_logic::session::{SessionInfo, SessionState};
use crate::subscription::Spawner;

use futures::StreamExt;
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
    Notifications as PlatformNotifications, Platform, PreimageHost as PlatformPreimageHost,
    RuntimeConfig, Storage as PlatformStorage, ThemeHost as PlatformThemeHost,
    UserConfirmation as PlatformUserConfirmation,
};

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
        _cx: &CallContext,
        request: HostAccountGetAliasRequest,
    ) -> Result<HostAccountGetAliasResponse, CallError<HostAccountGetAliasError>> {
        let HostAccountGetAliasRequest::V1(v01::HostAccountGetAliasRequest { product_account_id }) =
            request;

        if self.session_state.current().is_none() {
            return Err(CallError::Domain(HostAccountGetAliasError::V1(
                v01::HostAccountGetError::NotConnected,
            )));
        }

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

        Err(CallError::Domain(HostAccountGetAliasError::V1(
            v01::HostAccountGetError::Unknown {
                reason: "SSO account alias not implemented".to_string(),
            },
        )))
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
        Err(CallError::unavailable())
    }
}

impl<P> Signing for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    async fn sign_payload(
        &self,
        _cx: &CallContext,
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
        if self.session_state.current().is_none() {
            return Err(CallError::Domain(HostSignPayloadError::V1(
                v01::HostSignPayloadError::Rejected,
            )));
        }
        let confirmed =
            PlatformUserConfirmation::confirm_sign_payload(self.platform.as_ref(), inner.encode())
                .await
                .map_err(|err| CallError::HostFailure {
                    reason: format!("sign payload confirmation failed: {err:?}"),
                })?;
        if !confirmed {
            return Err(CallError::Domain(HostSignPayloadError::V1(
                v01::HostSignPayloadError::Rejected,
            )));
        }
        Err(CallError::Domain(HostSignPayloadError::V1(
            v01::HostSignPayloadError::Unknown {
                reason: "SSO signing not implemented".to_string(),
            },
        )))
    }

    async fn sign_raw(
        &self,
        _cx: &CallContext,
        request: HostSignRawRequest,
    ) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
        let HostSignRawRequest::V1(inner) = request;
        if !self.is_product_account_valid_for_caller(&inner.account.dot_ns_identifier) {
            return Err(CallError::Domain(HostSignRawError::V1(
                v01::HostSignPayloadError::PermissionDenied,
            )));
        }
        if self.session_state.current().is_none() {
            return Err(CallError::Domain(HostSignRawError::V1(
                v01::HostSignPayloadError::Rejected,
            )));
        }
        let confirmed =
            PlatformUserConfirmation::confirm_sign_raw(self.platform.as_ref(), inner.encode())
                .await
                .map_err(|err| CallError::HostFailure {
                    reason: format!("sign raw confirmation failed: {err:?}"),
                })?;
        if !confirmed {
            return Err(CallError::Domain(HostSignRawError::V1(
                v01::HostSignPayloadError::Rejected,
            )));
        }
        Err(CallError::Domain(HostSignRawError::V1(
            v01::HostSignPayloadError::Unknown {
                reason: "SSO signing not implemented".to_string(),
            },
        )))
    }

    async fn create_transaction(
        &self,
        _cx: &CallContext,
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
        if self.session_state.current().is_none() {
            return Err(CallError::Domain(HostCreateTransactionError::V1(
                v01::HostCreateTransactionError::Rejected,
            )));
        }
        let confirmed = PlatformUserConfirmation::confirm_create_transaction(
            self.platform.as_ref(),
            inner.encode(),
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
        Err(CallError::Domain(HostCreateTransactionError::V1(
            v01::HostCreateTransactionError::Unknown {
                reason: "SSO transaction creation not implemented".to_string(),
            },
        )))
    }

    async fn sign_payload_with_legacy_account(
        &self,
        _cx: &CallContext,
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
        let confirmed =
            PlatformUserConfirmation::confirm_sign_payload(self.platform.as_ref(), inner.encode())
                .await
                .map_err(|err| CallError::HostFailure {
                    reason: format!("sign payload confirmation failed: {err:?}"),
                })?;
        if !confirmed {
            return Err(CallError::Domain(
                HostSignPayloadWithLegacyAccountError::V1(v01::HostSignPayloadError::Rejected),
            ));
        }
        Err(CallError::Domain(
            HostSignPayloadWithLegacyAccountError::V1(v01::HostSignPayloadError::Unknown {
                reason: "SSO signing not implemented".to_string(),
            }),
        ))
    }

    async fn sign_raw_with_legacy_account(
        &self,
        _cx: &CallContext,
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
        let confirmed =
            PlatformUserConfirmation::confirm_sign_raw(self.platform.as_ref(), inner.encode())
                .await
                .map_err(|err| CallError::HostFailure {
                    reason: format!("sign raw confirmation failed: {err:?}"),
                })?;
        if !confirmed {
            return Err(CallError::Domain(HostSignRawWithLegacyAccountError::V1(
                v01::HostSignPayloadError::Rejected,
            )));
        }
        Err(CallError::Domain(HostSignRawWithLegacyAccountError::V1(
            v01::HostSignPayloadError::Unknown {
                reason: "SSO signing not implemented".to_string(),
            },
        )))
    }

    async fn create_transaction_with_legacy_account(
        &self,
        _cx: &CallContext,
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
            inner.encode(),
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
        Err(CallError::Domain(
            HostCreateTransactionWithLegacyAccountError::V1(
                v01::HostCreateTransactionError::Unknown {
                    reason: "SSO transaction creation not implemented".to_string(),
                },
            ),
        ))
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
        _cx: &CallContext,
        request: HostRequestResourceAllocationRequest,
    ) -> Result<HostRequestResourceAllocationResponse, CallError<HostRequestResourceAllocationError>>
    {
        let HostRequestResourceAllocationRequest::V1(inner) = request;
        if self.session_state.current().is_none() {
            return Err(CallError::Domain(HostRequestResourceAllocationError::V1(
                v01::ResourceAllocationError::Unknown {
                    reason: "No active session".to_string(),
                },
            )));
        }

        let confirmed = PlatformUserConfirmation::confirm_resource_allocation(
            self.platform.as_ref(),
            inner.encode(),
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

        Err(CallError::Domain(HostRequestResourceAllocationError::V1(
            v01::ResourceAllocationError::Unknown {
                reason: "SSO resource allocation not implemented".to_string(),
            },
        )))
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
    use parity_scale_codec::Encode;
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
            entropy_secret: Some((0..32).map(|i| i as u8).collect()),
            lite_username: Some("alice".to_string()),
            full_username: Some("Alice Smith".to_string()),
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

    struct DeadConnection;
    impl JsonRpcConnection for DeadConnection {
        fn send(&self, _request: String) {}
        fn responses(&self) -> BoxStream<'static, String> {
            Box::pin(stream::empty())
        }
    }

    impl ChainProvider for StubPlatform {
        async fn connect(
            &self,
            _genesis_hash: Vec<u8>,
        ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
            Ok(Box::new(DeadConnection))
        }
    }

    impl PairingPresenter for StubPlatform {
        async fn present_pairing(&self, _deeplink: String) -> Result<(), v01::GenericError> {
            Err(v01::GenericError {
                reason: "pairing presenter callback not provided by host".to_string(),
            })
        }
    }

    impl SessionStore for StubPlatform {
        async fn read_session(&self) -> Result<Option<Vec<u8>>, v01::GenericError> {
            Ok(None)
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
    fn get_account_alias_same_domain_reaches_sso_boundary() {
        let host =
            PlatformRuntimeHost::new(stub_platform(), runtime_config("myapp.dot"), test_spawner());
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let err = futures::executor::block_on(
            host.get_account_alias(&cx, account_alias_request("myapp.dot")),
        )
        .unwrap_err();
        match err {
            CallError::Domain(HostAccountGetAliasError::V1(
                v01::HostAccountGetError::Unknown { reason },
            )) => assert_eq!(reason, "SSO account alias not implemented"),
            other => panic!("expected SSO alias boundary error, got {other:?}"),
        }
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
    fn get_account_alias_cross_domain_accepts_confirmation_then_reaches_sso_boundary() {
        let host = PlatformRuntimeHost::new(
            Arc::new(StubPlatform {
                account_alias_confirmed: true,
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
        match err {
            CallError::Domain(HostAccountGetAliasError::V1(
                v01::HostAccountGetError::Unknown { reason },
            )) => assert_eq!(reason, "SSO account alias not implemented"),
            other => panic!("expected SSO alias boundary error, got {other:?}"),
        }
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
    fn sign_raw_accepts_confirmation_then_reaches_sso_boundary() {
        let host = PlatformRuntimeHost::new(
            Arc::new(StubPlatform {
                sign_raw_confirmed: true,
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
        match err {
            CallError::Domain(HostSignRawError::V1(v01::HostSignPayloadError::Unknown {
                reason,
            })) => assert_eq!(reason, "SSO signing not implemented"),
            other => panic!("expected SSO boundary error, got {other:?}"),
        }
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
    fn create_transaction_accepts_confirmation_then_reaches_sso_boundary() {
        let host = PlatformRuntimeHost::new(
            Arc::new(StubPlatform {
                create_transaction_confirmed: true,
                ..Default::default()
            }),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let request = HostCreateTransactionRequest::V1(product_tx_payload("myapp.dot"));
        let err = futures::executor::block_on(host.create_transaction(&cx, request)).unwrap_err();
        match err {
            CallError::Domain(HostCreateTransactionError::V1(
                v01::HostCreateTransactionError::Unknown { reason },
            )) => assert_eq!(reason, "SSO transaction creation not implemented"),
            other => panic!("expected SSO transaction boundary error, got {other:?}"),
        }
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
    fn legacy_sign_raw_accepts_derived_ss58_then_reaches_sso_boundary() {
        let host = PlatformRuntimeHost::new(
            Arc::new(StubPlatform {
                sign_raw_confirmed: true,
                ..Default::default()
            }),
            runtime_config("myapp.dot"),
            test_spawner(),
        );
        host.session_state().set_session(session_info());
        let cx = CallContext::new();
        let request =
            HostSignRawWithLegacyAccountRequest::V1(v01::HostSignRawWithLegacyAccountRequest {
                signer: "5CyFsdhwjXy7wWpDEM6isungQ3LfGnu9UXkt7paBQ6DYRxk1".to_string(),
                payload: raw_payload(),
            });
        let err = futures::executor::block_on(host.sign_raw_with_legacy_account(&cx, request))
            .unwrap_err();
        match err {
            CallError::Domain(HostSignRawWithLegacyAccountError::V1(
                v01::HostSignPayloadError::Unknown { reason },
            )) => assert_eq!(reason, "SSO signing not implemented"),
            other => panic!("expected SSO boundary error, got {other:?}"),
        }
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
    fn resource_allocation_accepts_confirmation_then_reaches_sso_boundary() {
        let host = PlatformRuntimeHost::new_compat(
            Arc::new(StubPlatform {
                resource_allocation_confirmed: true,
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
        match err {
            CallError::Domain(HostRequestResourceAllocationError::V1(
                v01::ResourceAllocationError::Unknown { reason },
            )) => assert_eq!(reason, "SSO resource allocation not implemented"),
            other => panic!("expected SSO boundary error, got {other:?}"),
        }
    }

    #[test]
    fn request_login_returns_unavailable() {
        let host = PlatformRuntimeHost::new_compat(stub_platform(), test_spawner());
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let err = futures::executor::block_on(host.request_login(&cx, request)).unwrap_err();
        assert!(matches!(err, CallError::HostFailure { reason } if reason == "unavailable"));
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
