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
    /// import { from, take } from "rxjs";
    ///
    /// from(truapi.account.connectionStatusSubscribe())
    ///   .pipe(take(3))
    ///   .subscribe({
    ///     next: (status) => console.log(status),
    ///     error: (error) => console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
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
    /// ```ts
    /// const result = await truapi.account.getAccount({
    ///   productAccountId: {
    ///     dotNsIdentifier: "truapi-playground.dot",
    ///     derivationIndex: 0,
    ///   },
    /// });
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
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
    /// ```ts
    /// const result = await truapi.account.getAccountAlias({
    ///   productAccountId: {
    ///     dotNsIdentifier: "truapi-playground.dot",
    ///     derivationIndex: 0,
    ///   },
    /// });
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
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
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
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
    /// ```ts
    /// const result = await truapi.account.getLegacyAccounts();
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
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
    /// ```ts
    /// const result = await truapi.account.getUserId();
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
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
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
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
