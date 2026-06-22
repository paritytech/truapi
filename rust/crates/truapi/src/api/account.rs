//! Unified [`Account`] trait.

use crate::versioned::account::{
    HostAccountConnectionStatusSubscribeItem, HostAccountCreateProofError,
    HostAccountCreateProofRequest, HostAccountCreateProofResponse, HostAccountGetAliasError,
    HostAccountGetAliasRequest, HostAccountGetAliasResponse, HostAccountGetError,
    HostAccountGetRequest, HostAccountGetResponse, HostGetLegacyAccountsError,
    HostGetLegacyAccountsRequest, HostGetLegacyAccountsResponse, HostGetUserIdError,
    HostGetUserIdRequest, HostGetUserIdResponse, HostRequestLoginError, HostRequestLoginRequest,
    HostRequestLoginResponse,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Account lookup, aliasing, and proof generation.
pub trait Account: Send + Sync {
    /// Subscribe to account connection status changes.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const status = await firstValueFrom(
    ///   from(truapi.account.connectionStatusSubscribe()),
    /// );
    /// console.log("connection status:", status);
    /// ```
    #[wire(start_id = 18)]
    async fn connection_status_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<HostAccountConnectionStatusSubscribeItem> {
        Subscription::empty()
    }

    /// Retrieve a product-scoped account.
    ///
    /// # Permissions
    ///
    /// - **auth**: required
    ///
    /// ```ts
    /// const result = await truapi.account.getAccount({
    ///   productAccountId: {
    ///     dotNsIdentifier: "truapi-playground.dot",
    ///     derivationIndex: 0,
    ///   },
    /// });
    /// assert(result.isOk(), "getAccount failed:", result);
    /// console.log("account retrieved:", result.value);
    /// ```
    #[wire(request_id = 22)]
    async fn get_account(
        &self,
        _cx: &CallContext,
        _request: HostAccountGetRequest,
    ) -> Result<HostAccountGetResponse, CallError<HostAccountGetError>> {
        Err(CallError::unavailable())
    }

    /// Retrieve a contextual alias for a product account.
    ///
    /// # Permissions
    ///
    /// - **auth**: required
    ///
    /// ```ts
    /// const result = await truapi.account.getAccountAlias({
    ///   productAccountId: {
    ///     dotNsIdentifier: "truapi-playground.dot",
    ///     derivationIndex: 0,
    ///   },
    /// });
    /// assert(result.isOk(), "getAccountAlias failed:", result);
    /// console.log("account alias:", result.value);
    /// ```
    #[wire(request_id = 24)]
    async fn get_account_alias(
        &self,
        _cx: &CallContext,
        _request: HostAccountGetAliasRequest,
    ) -> Result<HostAccountGetAliasResponse, CallError<HostAccountGetAliasError>> {
        Err(CallError::unavailable())
    }

    /// Generate a ring VRF proof for a product account.
    ///
    /// # Permissions
    ///
    /// - **auth**: required
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// const result = await truapi.account.createAccountProof({
    ///   productAccountId: {
    ///     dotNsIdentifier: "truapi-playground.dot",
    ///     derivationIndex: 0,
    ///   },
    ///   ringLocation: {
    ///     genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    ///     ringRootHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     hints: { palletInstance: 42 },
    ///   },
    ///   context: "0x",
    /// });
    /// assert(result.isOk(), "createAccountProof failed:", result);
    /// console.log("account proof created:", result.value);
    /// ```
    #[wire(request_id = 26)]
    async fn create_account_proof(
        &self,
        _cx: &CallContext,
        _request: HostAccountCreateProofRequest,
    ) -> Result<HostAccountCreateProofResponse, CallError<HostAccountCreateProofError>> {
        Err(CallError::unavailable())
    }

    /// List non-product accounts the user owns.
    ///
    /// # Permissions
    ///
    /// - **auth**: required
    ///
    /// ```ts
    /// const result = await truapi.account.getLegacyAccounts();
    /// assert(result.isOk(), "getLegacyAccounts failed:", result);
    /// console.log("legacy accounts:", result.value);
    /// ```
    #[wire(request_id = 28)]
    async fn get_legacy_accounts(
        &self,
        _cx: &CallContext,
        _request: HostGetLegacyAccountsRequest,
    ) -> Result<HostGetLegacyAccountsResponse, CallError<HostGetLegacyAccountsError>> {
        Err(CallError::unavailable())
    }

    /// Fetch the user's primary identity.
    ///
    /// # Permissions
    ///
    /// - **auth**: required
    /// - **prompt**: identity disclosure confirmation
    /// - **denial_error**: PermissionDenied
    ///
    /// ```ts
    /// const result = await truapi.account.getUserId();
    /// assert(result.isOk(), "getUserId failed:", result);
    /// console.log("user id:", result.value);
    /// ```
    #[wire(request_id = 110)]
    async fn get_user_id(
        &self,
        _cx: &CallContext,
        _request: HostGetUserIdRequest,
    ) -> Result<HostGetUserIdResponse, CallError<HostGetUserIdError>> {
        Err(CallError::unavailable())
    }

    /// Request the host to present the login flow to the user.
    ///
    /// Products should call this in response to a user action (e.g. tapping a
    /// "Sign in" button), not automatically on load.
    ///
    /// ```ts
    /// const result = await truapi.account.requestLogin({
    ///   reason: "Sign in to vote on Referendum #42",
    /// });
    /// assert(result.isOk(), "requestLogin failed:", result);
    /// console.log("login completed:", result.value);
    /// ```
    #[wire(request_id = 112)]
    async fn request_login(
        &self,
        _cx: &CallContext,
        _request: HostRequestLoginRequest,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        Err(CallError::unavailable())
    }
}
