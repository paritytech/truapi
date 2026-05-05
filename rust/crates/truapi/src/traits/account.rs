//! Unified [`AccountManagement`] trait.

use crate::v02::{
    Account, ContextualAlias, CreateProofError, RequestCredentialsError, UserIdentity,
    UserIdentityError,
};
use crate::versioned::account::{
    HostAccountConnectionStatusItem, HostAccountCreateProofRequest, HostAccountCreateProofResponse,
    HostAccountGetAliasRequest, HostAccountGetAliasResponse, HostAccountGetRequest,
    HostAccountGetResponse, HostGetNonProductAccountsRequest, HostGetNonProductAccountsResponse,
    HostGetUserIdRequest, HostGetUserIdResponse,
};
use crate::wire;
use crate::{CallContext, Subscription};

/// Account lookup, aliasing, and proof generation. Unified counterpart of
/// [`crate::v02::AccountManagement`].
///
/// Every method has a default body that flags the call as unavailable through
/// [`CallContext::fail_unavailable`] and returns a placeholder value. Hosts
/// override only the methods they actually support; unimplemented methods
/// surface as Interrupt frames at the wire level.
#[async_trait::async_trait]
pub trait AccountManagement: Send + Sync {
    /// Retrieve a product-scoped account.
    #[wire(id = 22)]
    async fn host_account_get(
        &self,
        cx: &CallContext,
        _request: HostAccountGetRequest,
    ) -> Result<HostAccountGetResponse, RequestCredentialsError> {
        cx.fail_unavailable();
        Ok(HostAccountGetResponse::V2(Account {
            public_key: Vec::new(),
            name: None,
        }))
    }

    /// Retrieve a contextual alias for a product account.
    #[wire(id = 24)]
    async fn host_account_get_alias(
        &self,
        cx: &CallContext,
        _request: HostAccountGetAliasRequest,
    ) -> Result<HostAccountGetAliasResponse, RequestCredentialsError> {
        cx.fail_unavailable();
        Ok(HostAccountGetAliasResponse::V2(ContextualAlias {
            context: [0u8; 32],
            alias: Vec::new(),
        }))
    }

    /// Generate a ring VRF proof for a product account.
    #[wire(id = 26)]
    async fn host_account_create_proof(
        &self,
        cx: &CallContext,
        _request: HostAccountCreateProofRequest,
    ) -> Result<HostAccountCreateProofResponse, CreateProofError> {
        cx.fail_unavailable();
        Ok(HostAccountCreateProofResponse::V2(Vec::new()))
    }

    /// List non-product accounts the user owns.
    #[wire(id = 28)]
    async fn host_get_non_product_accounts(
        &self,
        cx: &CallContext,
        _request: HostGetNonProductAccountsRequest,
    ) -> Result<HostGetNonProductAccountsResponse, RequestCredentialsError> {
        cx.fail_unavailable();
        Ok(HostGetNonProductAccountsResponse::V2(Vec::new()))
    }

    /// Subscribe to account connection status changes.
    #[wire(id = 18)]
    async fn host_account_connection_status_subscribe(
        &self,
        cx: &CallContext,
    ) -> Subscription<HostAccountConnectionStatusItem> {
        cx.fail_unavailable();
        Subscription::empty()
    }

    /// Fetch the user's primary identity (V0.2+).
    #[wire(id = 104)]
    async fn host_get_user_id(
        &self,
        cx: &CallContext,
        _request: HostGetUserIdRequest,
    ) -> Result<HostGetUserIdResponse, UserIdentityError> {
        cx.fail_unavailable();
        Ok(HostGetUserIdResponse::V2(UserIdentity {
            dot_ns_identifier: String::new(),
            public_key: Vec::new(),
        }))
    }
}
