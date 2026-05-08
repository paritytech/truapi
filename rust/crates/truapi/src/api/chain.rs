//! Unified [`ChainInteraction`] trait.

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

/// Chain head and transaction interactions.
///
/// Default methods return [`CallError::HostFailure`] with an `unavailable`
/// reason. Hosts override only the methods they can actually service.
#[async_trait::async_trait]
pub trait ChainInteraction: Send + Sync {
    /// Follow the chain head and receive block events.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type Subscription,
    ///   type RemoteChainHeadFollowItem,
    /// } from "@parity/truapi";
    ///
    /// export function followChainHead(truapi: Client): Subscription {
    ///   return truapi.chainInteraction
    ///     .chainHeadFollow({
    ///       request: { genesisHash: new Uint8Array(), withRuntime: false },
    ///     })
    ///     .subscribe({
    ///       next: (item: RemoteChainHeadFollowItem) => console.log(item),
    ///       error: (error: Error) => console.error(error),
    ///       complete: () => console.log("completed"),
    ///     });
    /// }
    /// ```
    #[wire(start_id = 76)]
    async fn remote_chain_head_follow(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadFollowRequest,
    ) -> Subscription<RemoteChainHeadFollowItem> {
        Subscription::empty()
    }

    /// Fetch a block header.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type RemoteChainHeadHeaderResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function getChainHeadHeader(
    ///   truapi: Client,
    /// ): Promise<RemoteChainHeadHeaderResponse> {
    ///   const result = await truapi.chainInteraction.chainHeadHeader({
    ///     genesisHash: new Uint8Array(),
    ///     followSubscriptionId: "",
    ///     hash: new Uint8Array(),
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 80)]
    async fn remote_chain_head_header(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadHeaderRequest,
    ) -> Result<RemoteChainHeadHeaderResponse, CallError<RemoteChainHeadHeaderError>> {
        Err(CallError::unavailable())
    }

    /// Fetch a block body.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type RemoteChainHeadBodyResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function getChainHeadBody(
    ///   truapi: Client,
    /// ): Promise<RemoteChainHeadBodyResponse> {
    ///   const result = await truapi.chainInteraction.chainHeadBody({
    ///     genesisHash: new Uint8Array(),
    ///     followSubscriptionId: "",
    ///     hash: new Uint8Array(),
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 82)]
    async fn remote_chain_head_body(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadBodyRequest,
    ) -> Result<RemoteChainHeadBodyResponse, CallError<RemoteChainHeadBodyError>> {
        Err(CallError::unavailable())
    }

    /// Query runtime storage at a specific block.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type RemoteChainHeadStorageResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function getChainHeadStorage(
    ///   truapi: Client,
    /// ): Promise<RemoteChainHeadStorageResponse> {
    ///   const result = await truapi.chainInteraction.chainHeadStorage({
    ///     genesisHash: new Uint8Array(),
    ///     followSubscriptionId: "",
    ///     hash: new Uint8Array(),
    ///     items: [
    ///       { key: new Uint8Array(), queryType: { tag: "Value", value: undefined } },
    ///     ],
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 84)]
    async fn remote_chain_head_storage(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadStorageRequest,
    ) -> Result<RemoteChainHeadStorageResponse, CallError<RemoteChainHeadStorageError>> {
        Err(CallError::unavailable())
    }

    /// Invoke a runtime call at a specific block.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type RemoteChainHeadCallResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function callChainHeadRuntime(
    ///   truapi: Client,
    /// ): Promise<RemoteChainHeadCallResponse> {
    ///   const result = await truapi.chainInteraction.chainHeadCall({
    ///     genesisHash: new Uint8Array(),
    ///     followSubscriptionId: "",
    ///     hash: new Uint8Array(),
    ///     function: "Core_version",
    ///     callParameters: new Uint8Array(),
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 86)]
    async fn remote_chain_head_call(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadCallRequest,
    ) -> Result<RemoteChainHeadCallResponse, CallError<RemoteChainHeadCallError>> {
        Err(CallError::unavailable())
    }

    /// Release pinned blocks.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function unpinChainHead(truapi: Client): Promise<void> {
    ///   const result = await truapi.chainInteraction.chainHeadUnpin({
    ///     genesisHash: new Uint8Array(),
    ///     followSubscriptionId: "",
    ///     hashes: [new Uint8Array()],
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 88)]
    async fn remote_chain_head_unpin(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadUnpinRequest,
    ) -> Result<RemoteChainHeadUnpinResponse, CallError<RemoteChainHeadUnpinError>> {
        Err(CallError::unavailable())
    }

    /// Continue a paused chain-head operation.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function continueChainHeadOperation(
    ///   truapi: Client,
    /// ): Promise<void> {
    ///   const result = await truapi.chainInteraction.chainHeadContinue({
    ///     genesisHash: new Uint8Array(),
    ///     followSubscriptionId: "",
    ///     operationId: "op-id",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 90)]
    async fn remote_chain_head_continue(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadContinueRequest,
    ) -> Result<RemoteChainHeadContinueResponse, CallError<RemoteChainHeadContinueError>> {
        Err(CallError::unavailable())
    }

    /// Stop a chain-head operation.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function stopChainHeadOperation(
    ///   truapi: Client,
    /// ): Promise<void> {
    ///   const result = await truapi.chainInteraction.chainHeadStopOperation({
    ///     genesisHash: new Uint8Array(),
    ///     followSubscriptionId: "",
    ///     operationId: "op-id",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 92)]
    async fn remote_chain_head_stop_operation(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadStopOperationRequest,
    ) -> Result<RemoteChainHeadStopOperationResponse, CallError<RemoteChainHeadStopOperationError>>
    {
        Err(CallError::unavailable())
    }

    /// Fetch the canonical genesis hash for a chain.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type RemoteChainSpecGenesisHashResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function getChainGenesisHash(
    ///   truapi: Client,
    /// ): Promise<RemoteChainSpecGenesisHashResponse> {
    ///   const result = await truapi.chainInteraction.chainSpecGenesisHash({
    ///     genesisHash: new Uint8Array(),
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 94)]
    async fn remote_chain_spec_genesis_hash(
        &self,
        _cx: &CallContext,
        _request: RemoteChainSpecGenesisHashRequest,
    ) -> Result<RemoteChainSpecGenesisHashResponse, CallError<RemoteChainSpecGenesisHashError>>
    {
        Err(CallError::unavailable())
    }

    /// Fetch the display name of a chain.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type RemoteChainSpecChainNameResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function getChainName(
    ///   truapi: Client,
    /// ): Promise<RemoteChainSpecChainNameResponse> {
    ///   const result = await truapi.chainInteraction.chainSpecChainName({
    ///     genesisHash: new Uint8Array(),
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 96)]
    async fn remote_chain_spec_chain_name(
        &self,
        _cx: &CallContext,
        _request: RemoteChainSpecChainNameRequest,
    ) -> Result<RemoteChainSpecChainNameResponse, CallError<RemoteChainSpecChainNameError>> {
        Err(CallError::unavailable())
    }

    /// Fetch the JSON-encoded properties of a chain.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type RemoteChainSpecPropertiesResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function getChainProperties(
    ///   truapi: Client,
    /// ): Promise<RemoteChainSpecPropertiesResponse> {
    ///   const result = await truapi.chainInteraction.chainSpecProperties({
    ///     genesisHash: new Uint8Array(),
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 98)]
    async fn remote_chain_spec_properties(
        &self,
        _cx: &CallContext,
        _request: RemoteChainSpecPropertiesRequest,
    ) -> Result<RemoteChainSpecPropertiesResponse, CallError<RemoteChainSpecPropertiesError>> {
        Err(CallError::unavailable())
    }

    /// Broadcast a signed transaction.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type RemoteChainTransactionBroadcastResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function broadcastTransaction(
    ///   truapi: Client,
    /// ): Promise<RemoteChainTransactionBroadcastResponse> {
    ///   const result = await truapi.chainInteraction.chainTransactionBroadcast({
    ///     genesisHash: new Uint8Array(),
    ///     transaction: new Uint8Array(),
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 100)]
    async fn remote_chain_transaction_broadcast(
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
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function stopTransactionBroadcast(
    ///   truapi: Client,
    /// ): Promise<void> {
    ///   const result = await truapi.chainInteraction.chainTransactionStop({
    ///     genesisHash: new Uint8Array(),
    ///     operationId: "op-id",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 102)]
    async fn remote_chain_transaction_stop(
        &self,
        _cx: &CallContext,
        _request: RemoteChainTransactionStopRequest,
    ) -> Result<RemoteChainTransactionStopResponse, CallError<RemoteChainTransactionStopError>>
    {
        Err(CallError::unavailable())
    }
}
