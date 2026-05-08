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
    /// ```truapi-playground-request
    /// { "genesisHash": "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2", "withRuntime": false }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, types as T, type Provider } from "@parity/truapi";
    ///
    /// export function followChainHead(
    ///   provider: Provider,
    ///   request: T.V01RemoteChainHeadFollowRequest,
    /// ) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   return truapi.chainInteraction.chainHeadFollow({
    ///     request,
    ///     onData: (item) => console.log(item),
    ///     onError: console.error,
    ///     onInterrupt: () => console.log("interrupted"),
    ///     onClose: console.error,
    ///   });
    /// }
    /// ```
    #[wire(id = 76)]
    async fn remote_chain_head_follow(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadFollowRequest,
    ) -> Subscription<RemoteChainHeadFollowItem> {
        Subscription::empty()
    }

    /// Fetch a block header.
    ///
    /// ```truapi-playground-request
    /// { "genesisHash": "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2", "followSubscriptionId": "", "hash": "0x0000000000000000000000000000000000000000000000000000000000000000" }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, types as T, type Provider } from "@parity/truapi";
    ///
    /// export async function getChainHeadHeader(
    ///   provider: Provider,
    ///   request: T.V01RemoteChainHeadHeaderRequest,
    /// ) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.chainInteraction.chainHeadHeader(request);
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(id = 80)]
    async fn remote_chain_head_header(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadHeaderRequest,
    ) -> Result<RemoteChainHeadHeaderResponse, CallError<RemoteChainHeadHeaderError>> {
        Err(CallError::unavailable())
    }

    /// Fetch a block body.
    ///
    /// ```truapi-playground-request
    /// { "genesisHash": "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2", "followSubscriptionId": "", "hash": "0x0000000000000000000000000000000000000000000000000000000000000000" }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, types as T, type Provider } from "@parity/truapi";
    ///
    /// export async function getChainHeadBody(
    ///   provider: Provider,
    ///   request: T.V01RemoteChainHeadBodyRequest,
    /// ) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.chainInteraction.chainHeadBody(request);
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(id = 82)]
    async fn remote_chain_head_body(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadBodyRequest,
    ) -> Result<RemoteChainHeadBodyResponse, CallError<RemoteChainHeadBodyError>> {
        Err(CallError::unavailable())
    }

    /// Query runtime storage at a specific block.
    ///
    /// ```truapi-playground-request
    /// { "genesisHash": "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2", "followSubscriptionId": "", "hash": "0x0000000000000000000000000000000000000000000000000000000000000000", "items": [{ "key": "0x26aa394eea5630e07c48ae0c9558cef7", "queryType": { "tag": "Value" } }], "childTrie": null }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, types as T, type Provider } from "@parity/truapi";
    ///
    /// export async function getChainHeadStorage(
    ///   provider: Provider,
    ///   request: T.V01RemoteChainHeadStorageRequest,
    /// ) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.chainInteraction.chainHeadStorage(request);
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(id = 84)]
    async fn remote_chain_head_storage(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadStorageRequest,
    ) -> Result<RemoteChainHeadStorageResponse, CallError<RemoteChainHeadStorageError>> {
        Err(CallError::unavailable())
    }

    /// Invoke a runtime call at a specific block.
    ///
    /// ```truapi-playground-request
    /// { "genesisHash": "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2", "followSubscriptionId": "", "hash": "0x0000000000000000000000000000000000000000000000000000000000000000", "function": "Core_version", "callParameters": "0x" }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, types as T, type Provider } from "@parity/truapi";
    ///
    /// export async function callChainHeadRuntime(
    ///   provider: Provider,
    ///   request: T.V01RemoteChainHeadCallRequest,
    /// ) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.chainInteraction.chainHeadCall(request);
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(id = 86)]
    async fn remote_chain_head_call(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadCallRequest,
    ) -> Result<RemoteChainHeadCallResponse, CallError<RemoteChainHeadCallError>> {
        Err(CallError::unavailable())
    }

    /// Release pinned blocks.
    ///
    /// ```truapi-playground-request
    /// { "genesisHash": "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2", "followSubscriptionId": "", "hashes": ["0x0000000000000000000000000000000000000000000000000000000000000000"] }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, types as T, type Provider } from "@parity/truapi";
    ///
    /// export async function unpinChainHead(
    ///   provider: Provider,
    ///   request: T.V01RemoteChainHeadUnpinRequest,
    /// ) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.chainInteraction.chainHeadUnpin(request);
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(id = 88)]
    async fn remote_chain_head_unpin(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadUnpinRequest,
    ) -> Result<RemoteChainHeadUnpinResponse, CallError<RemoteChainHeadUnpinError>> {
        Err(CallError::unavailable())
    }

    /// Continue a paused chain-head operation.
    ///
    /// ```truapi-playground-request
    /// { "genesisHash": "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2", "followSubscriptionId": "", "operationId": "op-id" }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, types as T, type Provider } from "@parity/truapi";
    ///
    /// export async function continueChainHeadOperation(
    ///   provider: Provider,
    ///   request: T.V01RemoteChainHeadContinueRequest,
    /// ) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.chainInteraction.chainHeadContinue(request);
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(id = 90)]
    async fn remote_chain_head_continue(
        &self,
        _cx: &CallContext,
        _request: RemoteChainHeadContinueRequest,
    ) -> Result<RemoteChainHeadContinueResponse, CallError<RemoteChainHeadContinueError>> {
        Err(CallError::unavailable())
    }

    /// Stop a chain-head operation.
    ///
    /// ```truapi-playground-request
    /// { "genesisHash": "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2", "followSubscriptionId": "", "operationId": "op-id" }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, types as T, type Provider } from "@parity/truapi";
    ///
    /// export async function stopChainHeadOperation(
    ///   provider: Provider,
    ///   request: T.V01RemoteChainHeadStopOperationRequest,
    /// ) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.chainInteraction.chainHeadStopOperation(request);
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(id = 92)]
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
    /// ```truapi-playground-request
    /// { "genesisHash": "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2" }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export async function getChainGenesisHash(provider: Provider, genesisHash: Uint8Array) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.chainInteraction.chainSpecGenesisHash({
    ///     genesisHash,
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(id = 94)]
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
    /// ```truapi-playground-request
    /// { "genesisHash": "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2" }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export async function getChainName(provider: Provider, genesisHash: Uint8Array) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.chainInteraction.chainSpecChainName({
    ///     genesisHash,
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(id = 96)]
    async fn remote_chain_spec_chain_name(
        &self,
        _cx: &CallContext,
        _request: RemoteChainSpecChainNameRequest,
    ) -> Result<RemoteChainSpecChainNameResponse, CallError<RemoteChainSpecChainNameError>> {
        Err(CallError::unavailable())
    }

    /// Fetch the JSON-encoded properties of a chain.
    ///
    /// ```truapi-playground-request
    /// { "genesisHash": "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2" }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export async function getChainProperties(provider: Provider, genesisHash: Uint8Array) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.chainInteraction.chainSpecProperties({
    ///     genesisHash,
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(id = 98)]
    async fn remote_chain_spec_properties(
        &self,
        _cx: &CallContext,
        _request: RemoteChainSpecPropertiesRequest,
    ) -> Result<RemoteChainSpecPropertiesResponse, CallError<RemoteChainSpecPropertiesError>> {
        Err(CallError::unavailable())
    }

    /// Broadcast a signed transaction.
    ///
    /// ```truapi-playground-request
    /// { "genesisHash": "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2", "transaction": "0x" }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export async function broadcastTransaction(
    ///   provider: Provider,
    ///   genesisHash: Uint8Array,
    ///   transaction: Uint8Array,
    /// ) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.chainInteraction.chainTransactionBroadcast({
    ///     genesisHash,
    ///     transaction,
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(id = 100)]
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
    /// ```truapi-playground-request
    /// { "genesisHash": "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2", "operationId": "op-id" }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export async function stopTransactionBroadcast(
    ///   provider: Provider,
    ///   genesisHash: Uint8Array,
    ///   operationId: string,
    /// ) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.chainInteraction.chainTransactionStop({
    ///     genesisHash,
    ///     operationId,
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(id = 102)]
    async fn remote_chain_transaction_stop(
        &self,
        _cx: &CallContext,
        _request: RemoteChainTransactionStopRequest,
    ) -> Result<RemoteChainTransactionStopResponse, CallError<RemoteChainTransactionStopError>>
    {
        Err(CallError::unavailable())
    }
}
