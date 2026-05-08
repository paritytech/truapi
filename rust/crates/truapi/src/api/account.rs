//! Unified [`AccountManagement`] trait.

use crate::versioned::account::{
    HostAccountConnectionStatusSubscribeItem, HostAccountCreateProofError,
    HostAccountCreateProofRequest, HostAccountCreateProofResponse, HostAccountGetAliasError,
    HostAccountGetAliasRequest, HostAccountGetAliasResponse, HostAccountGetError,
    HostAccountGetRequest, HostAccountGetResponse, HostGetLegacyAccountsError,
    HostGetLegacyAccountsRequest, HostGetLegacyAccountsResponse, HostGetUserIdError,
    HostGetUserIdRequest, HostGetUserIdResponse,
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
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type Subscription,
    ///   type HostAccountConnectionStatusSubscribeItem,
    /// } from "@parity/truapi";
    ///
    /// export function watchAccountConnection(truapi: Client): Subscription {
    ///   return truapi.accountManagement.accountConnectionStatusSubscribe().subscribe({
    ///     next: (status: HostAccountConnectionStatusSubscribeItem) =>
    ///       console.log(status),
    ///     error: (error: Error) => console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
    /// }
    /// ```
    #[wire(start_id = 18)]
    async fn host_account_connection_status_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<HostAccountConnectionStatusSubscribeItem> {
        Subscription::empty()
    }

    /// Retrieve a product-scoped account.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostAccountGetResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function getAccount(
    ///   truapi: Client,
    /// ): Promise<HostAccountGetResponse> {
    ///   const result = await truapi.accountManagement.accountGet({
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
    async fn host_account_get(
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
    ///   const result = await truapi.accountManagement.accountGetAlias({
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
    async fn host_account_get_alias(
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
    ///   const result = await truapi.accountManagement.accountCreateProof({
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
    async fn host_account_create_proof(
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
    ///   const result = await truapi.accountManagement.getLegacyAccounts();
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 28)]
    async fn host_get_legacy_accounts(
        &self,
        _cx: &CallContext,
        _request: HostGetLegacyAccountsRequest,
    ) -> Result<HostGetLegacyAccountsResponse, CallError<HostGetLegacyAccountsError>> {
        Err(CallError::unavailable())
    }

    /// Fetch the user's primary identity (V0.2+).
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
    ///   const result = await truapi.accountManagement.getUserId();
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 110)]
    async fn host_get_user_id(
        &self,
        _cx: &CallContext,
        _request: HostGetUserIdRequest,
    ) -> Result<HostGetUserIdResponse, CallError<HostGetUserIdError>> {
        Err(CallError::unavailable())
    }
}
