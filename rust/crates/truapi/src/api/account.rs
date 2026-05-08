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
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export function watchAccountConnection(provider: Provider) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   return truapi.accountManagement.accountConnectionStatusSubscribe({
    ///     onData: (status) => console.log(status),
    ///     onError: console.error,
    ///     onInterrupt: () => console.log("interrupted"),
    ///     onClose: console.error,
    ///   });
    /// }
    /// ```
    #[wire(id = 18)]
    async fn host_account_connection_status_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<HostAccountConnectionStatusSubscribeItem> {
        Subscription::empty()
    }

    /// Retrieve a product-scoped account.
    ///
    /// ```truapi-playground-request
    /// { "productAccountId": { "dotNsIdentifier": "truapi-playground.dot", "derivationIndex": 0 } }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export async function getAccount(provider: Provider) {
    ///   const truapi = createClient(createTransport(provider));
    ///
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
    #[wire(id = 22)]
    async fn host_account_get(
        &self,
        _cx: &CallContext,
        _request: HostAccountGetRequest,
    ) -> Result<HostAccountGetResponse, CallError<HostAccountGetError>> {
        Err(CallError::unavailable())
    }

    /// Retrieve a contextual alias for a product account.
    ///
    /// ```truapi-playground-request
    /// { "productAccountId": { "dotNsIdentifier": "truapi-playground.dot", "derivationIndex": 0 } }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export async function getAccountAlias(provider: Provider) {
    ///   const truapi = createClient(createTransport(provider));
    ///
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
    #[wire(id = 24)]
    async fn host_account_get_alias(
        &self,
        _cx: &CallContext,
        _request: HostAccountGetAliasRequest,
    ) -> Result<HostAccountGetAliasResponse, CallError<HostAccountGetAliasError>> {
        Err(CallError::unavailable())
    }

    /// Generate a ring VRF proof for a product account.
    ///
    /// ```truapi-playground-request
    /// { "productAccountId": { "dotNsIdentifier": "truapi-playground.dot", "derivationIndex": 0 }, "ringLocation": { "genesisHash": "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2", "ringRootHash": "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2", "hints": { "palletInstance": 42 } }, "context": "0x" }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export async function createAccountProof(
    ///   provider: Provider,
    ///   genesisHash: Uint8Array,
    ///   ringRootHash: Uint8Array,
    /// ) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.accountManagement.accountCreateProof({
    ///     productAccountId: {
    ///       dotNsIdentifier: "truapi-playground.dot",
    ///       derivationIndex: 0,
    ///     },
    ///     ringLocation: {
    ///       genesisHash,
    ///       ringRootHash,
    ///       hints: { palletInstance: 42 },
    ///     },
    ///     context: new Uint8Array(),
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(id = 26)]
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
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export async function getLegacyAccounts(provider: Provider) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.accountManagement.getLegacyAccounts();
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(id = 28)]
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
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export async function getUserId(provider: Provider) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.accountManagement.getUserId();
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(id = 110)]
    async fn host_get_user_id(
        &self,
        _cx: &CallContext,
        _request: HostGetUserIdRequest,
    ) -> Result<HostGetUserIdResponse, CallError<HostGetUserIdError>> {
        Err(CallError::unavailable())
    }
}
