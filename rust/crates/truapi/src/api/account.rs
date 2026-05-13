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
#[async_trait::async_trait]
pub trait Account: Send + Sync {
    /// Subscribe to account connection status changes.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type Subscription,
    ///   type HostAccountConnectionStatusSubscribeItem,
    /// } from "@parity/truapi";
    ///
    /// export function watchAccountConnection(truapi: Client): Subscription {
    ///   return truapi.account.connectionStatusSubscribe().subscribe({
    ///     next: (status: HostAccountConnectionStatusSubscribeItem) =>
    ///       console.log(status),
    ///     error: (error: Error) => console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
    /// }
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
    /// ```truapi-client-example
    /// import { type Client, type HostAccountGetResponse } from "@parity/truapi";
    ///
    /// export async function getAccount(
    ///   truapi: Client,
    /// ): Promise<HostAccountGetResponse> {
    ///   const result = await truapi.account.getAccount({
    ///     productAccountId: {
    ///       dotNsIdentifier: "truapi-playground.dot",
    ///       derivationIndex: 0,
    ///     },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
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
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostAccountGetAliasResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function getAccountAlias(
    ///   truapi: Client,
    /// ): Promise<HostAccountGetAliasResponse> {
    ///   const result = await truapi.account.getAccountAlias({
    ///     productAccountId: {
    ///       dotNsIdentifier: "truapi-playground.dot",
    ///       derivationIndex: 0,
    ///     },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
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
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostAccountCreateProofResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function createAccountProof(
    ///   truapi: Client,
    /// ): Promise<HostAccountCreateProofResponse> {
    ///   const result = await truapi.account.createAccountProof({
    ///     productAccountId: {
    ///       dotNsIdentifier: "truapi-playground.dot",
    ///       derivationIndex: 0,
    ///     },
    ///     ringLocation: {
    ///       genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///       ringRootHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///       hints: { palletInstance: 42 },
    ///     },
    ///     context: "0x",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
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
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostGetLegacyAccountsResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function getLegacyAccounts(
    ///   truapi: Client,
    /// ): Promise<HostGetLegacyAccountsResponse> {
    ///   const result = await truapi.account.getLegacyAccounts();
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
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
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostGetUserIdResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function getUserId(
    ///   truapi: Client,
    /// ): Promise<HostGetUserIdResponse> {
    ///   const result = await truapi.account.getUserId();
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
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
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostRequestLoginResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function requestLogin(
    ///   truapi: Client,
    /// ): Promise<HostRequestLoginResponse> {
    ///   const result = await truapi.account.requestLogin({
    ///     reason: "Sign in to vote on Referendum #42",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
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
