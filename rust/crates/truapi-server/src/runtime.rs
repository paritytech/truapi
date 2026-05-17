//! `PlatformRuntimeHost<P>` adapts a [`truapi_platform::Platform`] into the
//! typed `truapi::api::*` host traits the generated dispatcher routes to.
//!
//! Most methods are straight delegations to the platform; the rest are
//! either stubbed out (as `CallError::Unsupported` for the Chain surface)
//! or carry host-agnostic logic owned by the core (e.g. `dotns` URL
//! parsing for `navigate_to`, the permission cache layer).

use std::sync::Arc;

use crate::chain_runtime::{
    ChainRuntime, RuntimeChainProvider, RuntimeFailure, RuntimeFailureKind,
};
use crate::host_logic::dotns::{NavigateDecision, parse_navigate};
use crate::host_logic::features::feature_supported;
use crate::host_logic::permissions::{Decision, PermissionsService};
use crate::host_logic::session::SessionState;
use crate::subscription::Spawner;

use futures::StreamExt;
use truapi::api::{
    Account, Chain, Chat, Entropy, JsonRpc, LocalStorage, Payment, Permissions, Preimage,
    ResourceAllocation, Signing, StatementStore, System, Theme,
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
use truapi::versioned::local_storage::{
    HostLocalStorageClearError, HostLocalStorageClearRequest, HostLocalStorageClearResponse,
    HostLocalStorageReadError, HostLocalStorageReadRequest, HostLocalStorageReadResponse,
    HostLocalStorageWriteError, HostLocalStorageWriteRequest, HostLocalStorageWriteResponse,
};
use truapi::versioned::permissions::{
    HostDevicePermissionError, HostDevicePermissionRequest, HostDevicePermissionResponse,
    RemotePermissionError, RemotePermissionRequest, RemotePermissionResponse,
};
use truapi::versioned::preimage::{
    RemotePreimageLookupSubscribeItem, RemotePreimageLookupSubscribeRequest,
};
use truapi::versioned::signing::{
    HostSignPayloadError, HostSignPayloadRequest, HostSignPayloadResponse, HostSignRawError,
    HostSignRawRequest, HostSignRawResponse,
};
use truapi::versioned::statement_store::{
    RemoteStatementStoreCreateProofError, RemoteStatementStoreCreateProofRequest,
    RemoteStatementStoreCreateProofResponse, RemoteStatementStoreSubmitError,
    RemoteStatementStoreSubmitRequest, RemoteStatementStoreSubscribeItem,
    RemoteStatementStoreSubscribeRequest,
};
use truapi::versioned::system::{
    HostFeatureSupportedError, HostFeatureSupportedRequest, HostFeatureSupportedResponse,
    HostNavigateToError, HostNavigateToRequest, HostNavigateToResponse, HostPushNotificationError,
    HostPushNotificationRequest, HostPushNotificationResponse,
};
use truapi::{CallContext, CallError, Subscription};
use truapi_platform::{
    Accounts as PlatformAccounts, ChainProvider as PlatformChainProvider, GenesisHash,
    JsonRpcConnection, Navigation as PlatformNavigation, Notifications as PlatformNotifications,
    Platform, Preimage as PlatformPreimage, Signing as PlatformSigning,
    StatementStore as PlatformStatementStore, Storage as PlatformStorage,
};

/// Adapter that exposes a [`truapi_platform::Platform`] through the
/// `truapi::api::*` trait set the generated dispatcher routes to.
pub struct PlatformRuntimeHost<P> {
    platform: Arc<P>,
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
    pub fn new(platform: Arc<P>, spawner: Spawner) -> Self
    where
        P: Platform + 'static,
    {
        let chain_provider: Arc<dyn RuntimeChainProvider> =
            Arc::new(PlatformChainRuntimeProvider {
                platform: platform.clone(),
            });
        Self {
            platform,
            chain: ChainRuntime::new(chain_provider, spawner),
            session_state: SessionState::new(),
        }
    }

    /// Clone of the shared session-state holder. The platform bridge layer
    /// (`setActiveSession` / `clearActiveSession`) routes through this handle.
    pub fn session_state(&self) -> Arc<SessionState> {
        self.session_state.clone()
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
        genesis_hash: GenesisHash,
    ) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
        PlatformChainProvider::connect(self.platform.as_ref(), genesis_hash)
            .await
            .map(Arc::from)
            .map_err(|_| RuntimeFailure::unavailable("remote_chain_connect"))
    }
}

fn unsupported_with_reason<E>(reason: &str) -> CallError<E> {
    CallError::HostFailure {
        reason: reason.to_string(),
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

    async fn push_notification(
        &self,
        _cx: &CallContext,
        request: HostPushNotificationRequest,
    ) -> Result<HostPushNotificationResponse, CallError<HostPushNotificationError>> {
        let HostPushNotificationRequest::V1(inner) = request;
        PlatformNotifications::push_notification(self.platform.as_ref(), inner)
            .await
            .map(|()| HostPushNotificationResponse::V1)
            .map_err(|err| CallError::Domain(HostPushNotificationError::V1(err)))
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
            decision => decision
                .canonical_url()
                .expect("only Reject yields no canonical URL"),
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

impl<P> Account for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    async fn connection_status_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<HostAccountConnectionStatusSubscribeItem> {
        Subscription::new(self.session_state.subscribe())
    }

    async fn get_account(
        &self,
        _cx: &CallContext,
        request: HostAccountGetRequest,
    ) -> Result<HostAccountGetResponse, CallError<HostAccountGetError>> {
        PlatformAccounts::host_account_get(self.platform.as_ref(), request)
            .await
            .map_err(|err| CallError::Domain(HostAccountGetError::V1(err)))
    }

    async fn get_account_alias(
        &self,
        _cx: &CallContext,
        request: HostAccountGetAliasRequest,
    ) -> Result<HostAccountGetAliasResponse, CallError<HostAccountGetAliasError>> {
        PlatformAccounts::host_account_get_alias(self.platform.as_ref(), request)
            .await
            .map_err(|err| CallError::Domain(HostAccountGetAliasError::V1(err)))
    }

    async fn create_account_proof(
        &self,
        _cx: &CallContext,
        request: HostAccountCreateProofRequest,
    ) -> Result<HostAccountCreateProofResponse, CallError<HostAccountCreateProofError>> {
        PlatformAccounts::host_account_create_proof(self.platform.as_ref(), request)
            .await
            .map_err(|err| CallError::Domain(HostAccountCreateProofError::V1(err)))
    }

    async fn get_legacy_accounts(
        &self,
        _cx: &CallContext,
        request: HostGetLegacyAccountsRequest,
    ) -> Result<HostGetLegacyAccountsResponse, CallError<HostGetLegacyAccountsError>> {
        PlatformAccounts::host_get_legacy_accounts(self.platform.as_ref(), request)
            .await
            .map_err(|err| CallError::Domain(HostGetLegacyAccountsError::V1(err)))
    }

    async fn get_user_id(
        &self,
        _cx: &CallContext,
        request: HostGetUserIdRequest,
    ) -> Result<HostGetUserIdResponse, CallError<HostGetUserIdError>> {
        PlatformAccounts::host_get_user_id(self.platform.as_ref(), request)
            .await
            .map_err(|err| CallError::Domain(HostGetUserIdError::V1(err)))
    }

    async fn request_login(
        &self,
        _cx: &CallContext,
        _request: HostRequestLoginRequest,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        Err(unsupported_with_reason(
            "request_login is not implemented by the platform layer",
        ))
    }
}

// ---------------------------------------------------------------------------
// Signing
// ---------------------------------------------------------------------------

impl<P> Signing for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    async fn sign_payload(
        &self,
        _cx: &CallContext,
        request: HostSignPayloadRequest,
    ) -> Result<HostSignPayloadResponse, CallError<HostSignPayloadError>> {
        PlatformSigning::host_sign_payload(self.platform.as_ref(), request)
            .await
            .map_err(|err| CallError::Domain(HostSignPayloadError::V1(err)))
    }

    async fn sign_raw(
        &self,
        _cx: &CallContext,
        request: HostSignRawRequest,
    ) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
        PlatformSigning::host_sign_raw(self.platform.as_ref(), request)
            .await
            .map_err(|err| CallError::Domain(HostSignRawError::V1(err)))
    }

    // create_transaction, create_transaction_with_legacy_account,
    // sign_payload_with_legacy_account, sign_raw_with_legacy_account fall
    // back to the trait defaults (Err(CallError::unavailable())). The
    // platform surface only covers host_sign_payload / host_sign_raw.
}

// ---------------------------------------------------------------------------
// StatementStore
// ---------------------------------------------------------------------------

impl<P> StatementStore for PlatformRuntimeHost<P>
where
    P: Platform + 'static,
{
    async fn subscribe(
        &self,
        _cx: &CallContext,
        request: RemoteStatementStoreSubscribeRequest,
    ) -> Subscription<RemoteStatementStoreSubscribeItem> {
        let stream = PlatformStatementStore::remote_statement_store_subscribe(
            self.platform.as_ref(),
            request,
        )
        .await;
        Subscription::new(stream)
    }

    async fn submit(
        &self,
        _cx: &CallContext,
        request: RemoteStatementStoreSubmitRequest,
    ) -> Result<(), CallError<RemoteStatementStoreSubmitError>> {
        PlatformStatementStore::remote_statement_store_submit(self.platform.as_ref(), request)
            .await
            .map_err(|err| CallError::Domain(RemoteStatementStoreSubmitError::V1(err)))
    }

    async fn create_proof(
        &self,
        _cx: &CallContext,
        request: RemoteStatementStoreCreateProofRequest,
    ) -> Result<
        RemoteStatementStoreCreateProofResponse,
        CallError<RemoteStatementStoreCreateProofError>,
    > {
        PlatformStatementStore::remote_statement_store_create_proof(self.platform.as_ref(), request)
            .await
            .map_err(|err| CallError::Domain(RemoteStatementStoreCreateProofError::V1(err)))
    }

    // create_proof_authorized falls back to the default. The platform
    // surface does not expose pre-allocated allowance signing.
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
        let stream =
            PlatformPreimage::remote_preimage_lookup_subscribe(self.platform.as_ref(), request)
                .await;
        Subscription::new(stream)
    }

    // submit falls back to the default. The platform surface does not
    // include preimage submission.
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
// These API surfaces (Chat, JsonRpc, Payment, ResourceAllocation, Entropy,
// Theme) are not part of the v0.1 platform contract, so we leave every
// method at its default `Err(CallError::unavailable())` body and supply
// empty trait impls here. Adding a method later only requires implementing
// the relevant `truapi_platform::*` extension trait.

impl<P> Chat for PlatformRuntimeHost<P> where P: Platform + 'static {}
impl<P> JsonRpc for PlatformRuntimeHost<P> where P: Platform + 'static {}
impl<P> Payment for PlatformRuntimeHost<P> where P: Platform + 'static {}
impl<P> ResourceAllocation for PlatformRuntimeHost<P> where P: Platform + 'static {}
impl<P> Entropy for PlatformRuntimeHost<P> where P: Platform + 'static {}
impl<P> Theme for PlatformRuntimeHost<P> where P: Platform + 'static {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain_runtime::thread_per_task_spawner;
    use async_trait::async_trait;
    use futures::stream::{self, BoxStream};
    use parity_scale_codec::Encode;
    use truapi::v01;
    use truapi_platform::{
        Accounts as PlatformAccounts, ChainProvider, Features as PlatformFeatures, GenesisHash,
        JsonRpcConnection, Navigation as PlatformNavigation,
        Notifications as PlatformNotifications, Permissions as PlatformPermissions,
        Preimage as PlatformPreimage, Signing as PlatformSigning,
        StatementStore as PlatformStatementStore, Storage as PlatformStorage,
    };

    fn test_spawner() -> Spawner {
        thread_per_task_spawner()
    }

    /// Minimal Platform impl that only answers `feature_supported`. Every
    /// other callback returns a unit value or empty stream, so the runtime
    /// can exercise its delegation paths without pulling in a real backend.
    struct StubPlatform;

    #[async_trait]
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

    #[async_trait]
    impl PlatformNavigation for StubPlatform {
        async fn navigate_to(&self, _url: String) -> Result<(), v01::HostNavigateToError> {
            Ok(())
        }
    }

    #[async_trait]
    impl PlatformNotifications for StubPlatform {
        async fn push_notification(
            &self,
            _notification: v01::HostPushNotificationRequest,
        ) -> Result<(), v01::GenericError> {
            Ok(())
        }
    }

    #[async_trait]
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
            Ok(v01::RemotePermissionResponse { granted: true })
        }
    }

    #[async_trait]
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

    #[async_trait]
    impl ChainProvider for StubPlatform {
        async fn connect(
            &self,
            _genesis_hash: GenesisHash,
        ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
            Ok(Box::new(DeadConnection))
        }
    }

    #[async_trait]
    impl PlatformAccounts for StubPlatform {
        async fn host_account_get(
            &self,
            _request: truapi::versioned::account::HostAccountGetRequest,
        ) -> Result<truapi::versioned::account::HostAccountGetResponse, v01::HostAccountGetError>
        {
            Err(v01::HostAccountGetError::NotConnected)
        }
        async fn host_account_get_alias(
            &self,
            _request: truapi::versioned::account::HostAccountGetAliasRequest,
        ) -> Result<truapi::versioned::account::HostAccountGetAliasResponse, v01::HostAccountGetError>
        {
            Err(v01::HostAccountGetError::NotConnected)
        }
        async fn host_account_create_proof(
            &self,
            _request: truapi::versioned::account::HostAccountCreateProofRequest,
        ) -> Result<
            truapi::versioned::account::HostAccountCreateProofResponse,
            v01::HostAccountCreateProofError,
        > {
            Err(v01::HostAccountCreateProofError::RingNotFound)
        }
        async fn host_get_legacy_accounts(
            &self,
            _request: truapi::versioned::account::HostGetLegacyAccountsRequest,
        ) -> Result<
            truapi::versioned::account::HostGetLegacyAccountsResponse,
            v01::HostAccountGetError,
        > {
            Ok(
                truapi::versioned::account::HostGetLegacyAccountsResponse::V1(
                    v01::HostGetLegacyAccountsResponse { accounts: vec![] },
                ),
            )
        }
        async fn host_account_connection_status_subscribe(
            &self,
        ) -> BoxStream<'static, HostAccountConnectionStatusSubscribeItem> {
            Box::pin(stream::empty())
        }
        async fn host_get_user_id(
            &self,
            _request: truapi::versioned::account::HostGetUserIdRequest,
        ) -> Result<truapi::versioned::account::HostGetUserIdResponse, v01::HostGetUserIdError>
        {
            Err(v01::HostGetUserIdError::NotConnected)
        }
    }

    #[async_trait]
    impl PlatformSigning for StubPlatform {
        async fn host_sign_payload(
            &self,
            _request: HostSignPayloadRequest,
        ) -> Result<HostSignPayloadResponse, v01::HostSignPayloadError> {
            Err(v01::HostSignPayloadError::Rejected)
        }
        async fn host_sign_raw(
            &self,
            _request: HostSignRawRequest,
        ) -> Result<HostSignRawResponse, v01::HostSignPayloadError> {
            Err(v01::HostSignPayloadError::Rejected)
        }
    }

    #[async_trait]
    impl PlatformStatementStore for StubPlatform {
        async fn remote_statement_store_subscribe(
            &self,
            _request: RemoteStatementStoreSubscribeRequest,
        ) -> BoxStream<'static, RemoteStatementStoreSubscribeItem> {
            Box::pin(stream::empty())
        }
        async fn remote_statement_store_submit(
            &self,
            _request: RemoteStatementStoreSubmitRequest,
        ) -> Result<(), v01::GenericError> {
            Ok(())
        }
        async fn remote_statement_store_create_proof(
            &self,
            _request: RemoteStatementStoreCreateProofRequest,
        ) -> Result<
            RemoteStatementStoreCreateProofResponse,
            v01::RemoteStatementStoreCreateProofError,
        > {
            Err(v01::RemoteStatementStoreCreateProofError::UnableToSign)
        }
    }

    #[async_trait]
    impl PlatformPreimage for StubPlatform {
        async fn remote_preimage_lookup_subscribe(
            &self,
            _request: RemotePreimageLookupSubscribeRequest,
        ) -> BoxStream<'static, RemotePreimageLookupSubscribeItem> {
            Box::pin(stream::empty())
        }
    }

    #[test]
    fn feature_supported_round_trips_through_runtime() {
        let host = PlatformRuntimeHost::new(Arc::new(StubPlatform), test_spawner());
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
        let host = PlatformRuntimeHost::new(Arc::new(StubPlatform), test_spawner());
        let cx = CallContext::new();
        let request = HostNavigateToRequest::V1(v01::HostNavigateToRequest {
            url: "mytestapp.dot".to_string(),
        });
        let response = futures::executor::block_on(host.navigate_to(&cx, request)).unwrap();
        assert_eq!(response, HostNavigateToResponse::V1);
    }

    #[test]
    fn navigate_to_rejects_empty_input_without_calling_platform() {
        let host = PlatformRuntimeHost::new(Arc::new(StubPlatform), test_spawner());
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
    fn request_login_returns_unsupported() {
        let host = PlatformRuntimeHost::new(Arc::new(StubPlatform), test_spawner());
        let cx = CallContext::new();
        let request = HostRequestLoginRequest::V1(v01::HostRequestLoginRequest { reason: None });
        let err = futures::executor::block_on(host.request_login(&cx, request)).unwrap_err();
        assert!(matches!(err, CallError::HostFailure { .. }));
    }

    #[test]
    fn permissions_grants_and_caches() {
        let host = PlatformRuntimeHost::new(Arc::new(StubPlatform), test_spawner());
        let cx = CallContext::new();
        let request = HostDevicePermissionRequest::V1(v01::HostDevicePermissionRequest::Camera);
        let response =
            futures::executor::block_on(host.request_device_permission(&cx, request)).unwrap();
        let HostDevicePermissionResponse::V1(inner) = response;
        assert!(inner.granted);
    }

    #[test]
    fn feature_supported_encodes_response_to_known_bytes() {
        let host = PlatformRuntimeHost::new(Arc::new(StubPlatform), test_spawner());
        let cx = CallContext::new();
        let request = HostFeatureSupportedRequest::V1(v01::HostFeatureSupportedRequest::Chain {
            genesis_hash: vec![0u8; 32],
        });
        let response = futures::executor::block_on(host.feature_supported(&cx, request)).unwrap();
        // [V1 variant=0][supported=1]
        assert_eq!(response.encode(), vec![0x00, 0x01]);
    }
}
