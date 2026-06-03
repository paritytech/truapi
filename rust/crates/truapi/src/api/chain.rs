//! Unified [`Chain`] trait.

use crate::versioned::chain::{
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
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Chain interaction methods.
pub trait Chain: Send + Sync {
    /// Follow the chain head and receive block events.
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const item = await firstValueFrom(
    ///   from(
    ///     truapi.chain.followHeadSubscribe({
    ///       request: {
    ///         genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    ///         withRuntime: false,
    ///       },
    ///     }),
    ///   ),
    /// );
    /// console.log(item);
    /// ```
    #[wire(start_id = 76)]
    async fn follow_head_subscribe(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadFollowRequest,
    ) -> Subscription<RemoteChainHeadFollowItem> {
        Subscription::empty()
    }

    /// Fetch a block header.
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// import { firstValueFrom, mergeMap } from "rxjs";
    ///
    /// const result = await firstValueFrom(
    ///   withChainHeadFollow({
    ///     genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    ///   }).pipe(
    ///     mergeMap(({ genesisHash, followSubscriptionId, hash }) =>
    ///       truapi.chain.getHeadHeader({ genesisHash, followSubscriptionId, hash }),
    ///     ),
    ///   ),
    /// );
    /// assert(result.isOk(), "getHeadHeader failed:", result);
    /// console.log(result.value);
    /// ```
    #[wire(request_id = 80)]
    async fn get_head_header(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadHeaderRequest,
    ) -> Result<RemoteChainHeadHeaderResponse, CallError<RemoteChainHeadHeaderError>> {
        Err(CallError::unavailable())
    }

    /// Fetch a block body.
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// import { firstValueFrom, mergeMap } from "rxjs";
    ///
    /// const result = await firstValueFrom(
    ///   withChainHeadFollow({
    ///     genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    ///   }).pipe(
    ///     mergeMap(({ genesisHash, followSubscriptionId, hash }) =>
    ///       truapi.chain.getHeadBody({ genesisHash, followSubscriptionId, hash }),
    ///     ),
    ///   ),
    /// );
    /// assert(result.isOk(), "getHeadBody failed:", result);
    /// console.log(result.value);
    /// ```
    #[wire(request_id = 82)]
    async fn get_head_body(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadBodyRequest,
    ) -> Result<RemoteChainHeadBodyResponse, CallError<RemoteChainHeadBodyError>> {
        Err(CallError::unavailable())
    }

    /// Query runtime storage at a specific block.
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// import { firstValueFrom, mergeMap } from "rxjs";
    ///
    /// const result = await firstValueFrom(
    ///   withChainHeadFollow({
    ///     genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    ///   }).pipe(
    ///     mergeMap(({ genesisHash, followSubscriptionId, hash }) =>
    ///       truapi.chain.getHeadStorage({
    ///         genesisHash,
    ///         followSubscriptionId,
    ///         hash,
    ///         items: [{ key: "0x26aa394eea5630e07c48ae0c9558cef7", queryType: "Value" }],
    ///       }),
    ///     ),
    ///   ),
    /// );
    /// assert(result.isOk(), "getHeadStorage failed:", result);
    /// console.log(result.value);
    /// ```
    #[wire(request_id = 84)]
    async fn get_head_storage(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadStorageRequest,
    ) -> Result<RemoteChainHeadStorageResponse, CallError<RemoteChainHeadStorageError>> {
        Err(CallError::unavailable())
    }

    /// Invoke a runtime call at a specific block.
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// import { firstValueFrom, mergeMap } from "rxjs";
    ///
    /// const result = await firstValueFrom(
    ///   withChainHeadFollow({
    ///     genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    ///     withRuntime: true,
    ///   }).pipe(
    ///     mergeMap(({ genesisHash, followSubscriptionId, hash }) =>
    ///       truapi.chain.callHead({
    ///         genesisHash,
    ///         followSubscriptionId,
    ///         hash,
    ///         function: "Core_version",
    ///         callParameters: "0x",
    ///       }),
    ///     ),
    ///   ),
    /// );
    /// assert(result.isOk(), "callHead failed:", result);
    /// console.log(result.value);
    /// ```
    #[wire(request_id = 86)]
    async fn call_head(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadCallRequest,
    ) -> Result<RemoteChainHeadCallResponse, CallError<RemoteChainHeadCallError>> {
        Err(CallError::unavailable())
    }

    /// Release pinned blocks.
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// import { firstValueFrom, mergeMap } from "rxjs";
    ///
    /// const result = await firstValueFrom(
    ///   withChainHeadFollow({
    ///     genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    ///   }).pipe(
    ///     mergeMap(({ genesisHash, followSubscriptionId, hash }) =>
    ///       truapi.chain.unpinHead({
    ///         genesisHash,
    ///         followSubscriptionId,
    ///         hashes: [hash],
    ///       }),
    ///     ),
    ///   ),
    /// );
    /// assert(result.isOk(), "unpinHead failed:", result);
    /// console.log("ok");
    /// ```
    #[wire(request_id = 88)]
    async fn unpin_head(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadUnpinRequest,
    ) -> Result<RemoteChainHeadUnpinResponse, CallError<RemoteChainHeadUnpinError>> {
        Err(CallError::unavailable())
    }

    /// Continue a paused chain-head operation.
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// import { firstValueFrom, mergeMap } from "rxjs";
    ///
    /// const result = await firstValueFrom(
    ///   withChainHeadFollow({
    ///     genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    ///   }).pipe(
    ///     mergeMap(({ genesisHash, followSubscriptionId }) =>
    ///       truapi.chain.continueHead({
    ///         genesisHash,
    ///         followSubscriptionId,
    ///         operationId: "op-id",
    ///       }),
    ///     ),
    ///   ),
    /// );
    /// assert(result.isOk(), "continueHead failed:", result);
    /// console.log("ok");
    /// ```
    #[wire(request_id = 90)]
    async fn continue_head(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadContinueRequest,
    ) -> Result<RemoteChainHeadContinueResponse, CallError<RemoteChainHeadContinueError>> {
        Err(CallError::unavailable())
    }

    /// Stop a chain-head operation.
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// import { firstValueFrom, mergeMap } from "rxjs";
    ///
    /// const result = await firstValueFrom(
    ///   withChainHeadFollow({
    ///     genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    ///   }).pipe(
    ///     mergeMap(({ genesisHash, followSubscriptionId }) =>
    ///       truapi.chain.stopHeadOperation({
    ///         genesisHash,
    ///         followSubscriptionId,
    ///         operationId: "op-id",
    ///       }),
    ///     ),
    ///   ),
    /// );
    /// assert(result.isOk(), "stopHeadOperation failed:", result);
    /// console.log("ok");
    /// ```
    #[wire(request_id = 92)]
    async fn stop_head_operation(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadStopOperationRequest,
    ) -> Result<RemoteChainHeadStopOperationResponse, CallError<RemoteChainHeadStopOperationError>>
    {
        Err(CallError::unavailable())
    }

    /// Fetch the canonical genesis hash for a chain.
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// const result = await truapi.chain.getSpecGenesisHash({
    ///   genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    /// });
    /// assert(result.isOk(), "getSpecGenesisHash failed:", result);
    /// console.log(result.value);
    /// ```
    #[wire(request_id = 94)]
    async fn get_spec_genesis_hash(
        &self,
        _cx: &CallContext,
        _request: RemoteChainSpecGenesisHashRequest,
    ) -> Result<RemoteChainSpecGenesisHashResponse, CallError<RemoteChainSpecGenesisHashError>>
    {
        Err(CallError::unavailable())
    }

    /// Fetch the display name of a chain.
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// const result = await truapi.chain.getSpecChainName({
    ///   genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    /// });
    /// assert(result.isOk(), "getSpecChainName failed:", result);
    /// console.log(result.value);
    /// ```
    #[wire(request_id = 96)]
    async fn get_spec_chain_name(
        &self,
        _cx: &CallContext,
        _request: RemoteChainSpecChainNameRequest,
    ) -> Result<RemoteChainSpecChainNameResponse, CallError<RemoteChainSpecChainNameError>> {
        Err(CallError::unavailable())
    }

    /// Fetch the JSON-encoded properties of a chain.
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// const result = await truapi.chain.getSpecProperties({
    ///   genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    /// });
    /// assert(result.isOk(), "getSpecProperties failed:", result);
    /// console.log(result.value);
    /// ```
    #[wire(request_id = 98)]
    async fn get_spec_properties(
        &self,
        _cx: &CallContext,
        _request: RemoteChainSpecPropertiesRequest,
    ) -> Result<RemoteChainSpecPropertiesResponse, CallError<RemoteChainSpecPropertiesError>> {
        Err(CallError::unavailable())
    }

    /// Broadcast a signed transaction.
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// const result = await truapi.chain.broadcastTransaction({
    ///   genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    ///   transaction: "0x",
    /// });
    /// assert(result.isOk(), "broadcastTransaction failed:", result);
    /// console.log(result.value);
    /// ```
    #[wire(request_id = 100)]
    async fn broadcast_transaction(
        &self,
        _cx: &CallContext,
        _request: RemoteChainTransactionBroadcastRequest,
    ) -> Result<
        RemoteChainTransactionBroadcastResponse,
        CallError<RemoteChainTransactionBroadcastError>,
    > {
        Err(CallError::unavailable())
    }

    /// Stop a transaction broadcast.
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// const result = await truapi.chain.stopTransaction({
    ///   genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    ///   operationId: "op-id",
    /// });
    /// assert(result.isOk(), "stopTransaction failed:", result);
    /// console.log("ok");
    /// ```
    #[wire(request_id = 102)]
    async fn stop_transaction(
        &self,
        _cx: &CallContext,
        _request: RemoteChainTransactionStopRequest,
    ) -> Result<RemoteChainTransactionStopResponse, CallError<RemoteChainTransactionStopError>>
    {
        Err(CallError::unavailable())
    }
}
