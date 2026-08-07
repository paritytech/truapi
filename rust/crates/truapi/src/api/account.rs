//! Unified [`Account`] trait.

use crate::versioned::account::{
    HostAccountConnectionStatusSubscribeItem, HostAccountCreateProofError,
    HostAccountCreateProofRequest, HostAccountCreateProofResponse, HostAccountGetAliasError,
    HostAccountGetAliasRequest, HostAccountGetAliasResponse, HostAccountGetError,
    HostAccountGetRequest, HostAccountGetResponse, HostAccountListRingVrfKeysError,
    HostAccountListRingVrfKeysRequest, HostAccountListRingVrfKeysResponse,
    HostAccountRegisterRingVrfKeyError, HostAccountRegisterRingVrfKeyRequest,
    HostAccountRegisterRingVrfKeyResponse, HostAccountRingVrfSignError,
    HostAccountRingVrfSignRequest, HostAccountRingVrfSignResponse, HostAccountSignVrfError,
    HostAccountSignVrfRequest, HostAccountSignVrfResponse, HostGetLegacyAccountsError,
    HostGetLegacyAccountsRequest, HostGetLegacyAccountsResponse, HostGetUserIdError,
    HostGetUserIdRequest, HostGetUserIdResponse, HostRequestLoginError, HostRequestLoginRequest,
    HostRequestLoginResponse,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Account lookup, aliasing, and proof generation.
#[crate::async_trait]
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
    /// ```ts
    /// const result = await truapi.account.getAccount({
    ///   productAccountId: {
    ///     dotNsIdentifier: "truapi-playground.dot",
    ///     derivationIndex: { tag: "Index", value: 0 },
    ///   },
    /// });
    /// assert(result.isOk(), "getAccount failed:", result);
    /// console.log("account retrieved:", result.value);
    ///
    /// const otherProduct = await truapi.account.getAccount({
    ///   productAccountId: {
    ///     dotNsIdentifier: "other-product.dot",
    ///     derivationIndex: { tag: "Index", value: 0 },
    ///   },
    /// });
    /// assert(otherProduct.isOk(), "cross-product getAccount was denied or failed:", otherProduct);
    /// console.log("other product account retrieved after approval:", otherProduct.value);
    /// ```
    #[wire(request_id = 22)]
    async fn get_account(
        &self,
        _cx: &CallContext,
        _request: HostAccountGetRequest,
    ) -> Result<HostAccountGetResponse, CallError<HostAccountGetError>> {
        Err(CallError::unavailable())
    }

    /// Retrieve the contextual alias for a context and ring.
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_INDIVIDUALITY } from "@parity/truapi";
    ///
    /// const PEOPLE_COLLECTION_ID =
    ///   "0x706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652d6c697465" as const;
    /// const keyHandle = {
    ///   dotNsIdentifier: "truapi-playground.dot",
    ///   derivationIndex: { tag: "Index" as const, value: 0 },
    /// };
    /// const ringLocation = {
    ///   chainId: PASEO_NEXT_V2_INDIVIDUALITY.genesis,
    ///   junctions: [
    ///     { tag: "CollectionId" as const, value: PEOPLE_COLLECTION_ID },
    ///   ],
    /// };
    /// const registration = await truapi.account.registerRingVrfKey({
    ///   index: keyHandle.derivationIndex,
    ///   ring: ringLocation,
    /// });
    /// assert(registration.isOk(), "registerRingVrfKey failed:", registration);
    ///
    /// const result = await truapi.account.getAccountAlias({
    ///   keyHandle,
    ///   context: { productId: "truapi-playground.dot", suffix: { tag: "Index", value: 0 } },
    ///   ringLocation,
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

    /// Generate a ring VRF proof with an explicitly registered member key.
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_INDIVIDUALITY } from "@parity/truapi";
    ///
    /// const PEOPLE_COLLECTION_ID =
    ///   "0x706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652d6c697465";
    ///
    /// const result = await truapi.account.createAccountProof({
    ///   keyHandle: {
    ///     dotNsIdentifier: "peopl.dot",
    ///     derivationIndex: { tag: "Index", value: 1 },
    ///   },
    ///   context: { productId: "truapi-playground.dot", suffix: { tag: "Index", value: 0 } },
    ///   ringLocation: {
    ///     chainId: PASEO_NEXT_V2_INDIVIDUALITY.genesis,
    ///     junctions: [
    ///       { tag: "CollectionId", value: PEOPLE_COLLECTION_ID },
    ///     ],
    ///   },
    ///   message: "0x48656c6c6f",
    /// });
    /// assert(result.isErr(), "foreign createAccountProof unexpectedly succeeded:", result);
    /// assert(
    ///   result.error.tag === "Domain" &&
    ///     result.error.value.tag === "V1" &&
    ///     result.error.value.value.tag === "NotAllowlisted",
    ///   "foreign createAccountProof did not return NotAllowlisted:",
    ///   result,
    /// );
    /// console.log("foreign account proof refused without prompting");
    /// ```
    #[wire(request_id = 26)]
    async fn create_account_proof(
        &self,
        _cx: &CallContext,
        _request: HostAccountCreateProofRequest,
    ) -> Result<HostAccountCreateProofResponse, CallError<HostAccountCreateProofError>> {
        Err(CallError::unavailable())
    }

    /// Produce an sr25519 (schnorrkel) VRF signature from a product account.
    ///
    /// The host builds a Merlin transcript from `transcriptLabel` and `items`
    /// and signs it with the account's key, returning the VRF pre-output and
    /// proof. Authorized like signing: local when `AutoSigning` covers the
    /// account, otherwise a per-call user confirmation.
    ///
    /// ```ts
    /// const result = await truapi.account.signVrf({
    ///   account: {
    ///     dotNsIdentifier: "truapi-playground.dot",
    ///     derivationIndex: { tag: "Index", value: 0 },
    ///   },
    ///   transcriptLabel: "0x706f703a61697264726f70",
    ///   items: [
    ///     { label: "0x646f6d61696e", value: "0x706f703a61697264726f70" },
    ///     { label: "0x7369676e6572", value: "0x00" },
    ///   ],
    /// });
    /// assert(result.isOk(), "signVrf failed:", result);
    /// console.log("vrf signature:", result.value);
    /// ```
    #[wire(request_id = 164)]
    async fn sign_vrf(
        &self,
        _cx: &CallContext,
        _request: HostAccountSignVrfRequest,
    ) -> Result<HostAccountSignVrfResponse, CallError<HostAccountSignVrfError>> {
        Err(CallError::unavailable())
    }

    /// Register a ring-VRF key owned by the calling product.
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_INDIVIDUALITY } from "@parity/truapi";
    ///
    /// const PEOPLE_COLLECTION_ID =
    ///   "0x706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652d6c697465";
    ///
    /// const result = await truapi.account.registerRingVrfKey({
    ///   index: { tag: "Index", value: 0 },
    ///   ring: {
    ///     chainId: PASEO_NEXT_V2_INDIVIDUALITY.genesis,
    ///     junctions: [
    ///       { tag: "CollectionId", value: PEOPLE_COLLECTION_ID },
    ///     ],
    ///   },
    /// });
    /// assert(result.isOk(), "registerRingVrfKey failed:", result);
    /// console.log("ring VRF public key:", result.value);
    /// ```
    #[wire(request_id = 166)]
    async fn register_ring_vrf_key(
        &self,
        _cx: &CallContext,
        _request: HostAccountRegisterRingVrfKeyRequest,
    ) -> Result<HostAccountRegisterRingVrfKeyResponse, CallError<HostAccountRegisterRingVrfKeyError>>
    {
        Err(CallError::unavailable())
    }

    /// List registered ring-VRF keys owned by a product.
    ///
    /// ```ts
    /// const result = await truapi.account.listRingVrfKeys({
    ///   owner: "truapi-playground.dot",
    ///   disclosure: "PublicKey",
    /// });
    /// assert(result.isOk(), "listRingVrfKeys failed:", result);
    /// console.log("registered ring VRF keys:", result.value);
    /// ```
    #[wire(request_id = 168)]
    async fn list_ring_vrf_keys(
        &self,
        _cx: &CallContext,
        _request: HostAccountListRingVrfKeysRequest,
    ) -> Result<HostAccountListRingVrfKeysResponse, CallError<HostAccountListRingVrfKeysError>>
    {
        Err(CallError::unavailable())
    }

    /// Sign bytes directly with a registered ring-VRF member key.
    ///
    /// ```ts
    /// const result = await truapi.account.ringVrfSign({
    ///   keyHandle: {
    ///     dotNsIdentifier: "truapi-playground.dot",
    ///     derivationIndex: { tag: "Index", value: 0 },
    ///   },
    ///   message: "0x48656c6c6f",
    /// });
    /// assert(result.isOk(), "ringVrfSign failed:", result);
    /// console.log("ring VRF signature:", result.value);
    /// ```
    #[wire(request_id = 170)]
    async fn ring_vrf_sign(
        &self,
        _cx: &CallContext,
        _request: HostAccountRingVrfSignRequest,
    ) -> Result<HostAccountRingVrfSignResponse, CallError<HostAccountRingVrfSignError>> {
        Err(CallError::unavailable())
    }

    /// List non-product accounts the user owns.
    ///
    /// Current hosts do not expose non-product accounts, so the list is empty.
    ///
    /// ```ts
    /// const result = await truapi.account.getLegacyAccounts();
    /// assert(result.isOk(), "getLegacyAccounts failed:", result);
    /// assert(result.value.accounts.length === 0, "unexpected legacy accounts:", result.value);
    /// console.log("legacy accounts:", result.value.accounts);
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
