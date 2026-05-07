//! Unified [`AccountManagement`] trait.

use crate::versioned::account::{
    HostAccountConnectionStatusSubscribeItem, HostAccountCreateProofError, HostAccountCreateProofRequest,
    HostAccountCreateProofResponse, HostAccountGetAliasError, HostAccountGetAliasRequest,
    HostAccountGetAliasResponse, HostAccountGetError, HostAccountGetRequest,
    HostAccountGetResponse, HostGetNonProductAccountsError, HostGetNonProductAccountsRequest,
    HostGetNonProductAccountsResponse, HostGetUserIdError, HostGetUserIdRequest,
    HostGetUserIdResponse,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Account lookup, aliasing, and proof generation.
///
/// Default methods return [`CallError::HostFailure`] with an `unavailable`
/// reason. Hosts override only the methods they actually support.
#[async_trait::async_trait]
pub trait AccountManagement: Send + Sync {
    /// Subscribe to account connection status changes.
    #[wire(id = 18)]
    async fn host_account_connection_status_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<HostAccountConnectionStatusSubscribeItem> {
        Subscription::empty()
    }

    /// Retrieve a product-scoped account.
    #[wire(id = 22)]
    async fn host_account_get(
        &self,
        _cx: &CallContext,
        _request: HostAccountGetRequest,
    ) -> Result<HostAccountGetResponse, CallError<HostAccountGetError>> {
        Err(CallError::unavailable())
    }

    /// Retrieve a contextual alias for a product account.
    #[wire(id = 24)]
    async fn host_account_get_alias(
        &self,
        _cx: &CallContext,
        _request: HostAccountGetAliasRequest,
    ) -> Result<HostAccountGetAliasResponse, CallError<HostAccountGetAliasError>> {
        Err(CallError::unavailable())
    }

    /// Generate a ring VRF proof for a product account.
    #[wire(id = 26)]
    async fn host_account_create_proof(
        &self,
        _cx: &CallContext,
        _request: HostAccountCreateProofRequest,
    ) -> Result<HostAccountCreateProofResponse, CallError<HostAccountCreateProofError>> {
        Err(CallError::unavailable())
    }

    /// List non-product accounts the user owns.
    #[wire(id = 28)]
    async fn host_get_non_product_accounts(
        &self,
        _cx: &CallContext,
        _request: HostGetNonProductAccountsRequest,
    ) -> Result<HostGetNonProductAccountsResponse, CallError<HostGetNonProductAccountsError>> {
        Err(CallError::unavailable())
    }

    /// Fetch the user's primary identity (V0.2+).
    #[wire(id = 104)]
    async fn host_get_user_id(
        &self,
        _cx: &CallContext,
        _request: HostGetUserIdRequest,
    ) -> Result<HostGetUserIdResponse, CallError<HostGetUserIdError>> {
        Err(CallError::unavailable())
    }
}
