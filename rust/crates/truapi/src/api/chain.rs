//! Unified [`ChainInteraction`] trait.

use crate::versioned::chain::{
    HostCreateTransactionError, HostCreateTransactionRequest, HostCreateTransactionResponse,
    HostCreateTransactionWithLegacyAccountError, HostCreateTransactionWithLegacyAccountRequest,
    HostCreateTransactionWithLegacyAccountResponse, HostJsonrpcMessageSendError,
    HostJsonrpcMessageSendRequest, HostJsonrpcMessageSendResponse,
    HostJsonrpcMessageSubscribeItem, HostJsonrpcMessageSubscribeRequest, HostSignPayloadError,
    HostSignPayloadRequest, HostSignPayloadResponse, HostSignPayloadWithLegacyAccountError,
    HostSignPayloadWithLegacyAccountRequest, HostSignPayloadWithLegacyAccountResponse,
    HostSignRawError, HostSignRawRequest, HostSignRawResponse,
    HostSignRawWithLegacyAccountError, HostSignRawWithLegacyAccountRequest,
    HostSignRawWithLegacyAccountResponse,
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

/// Chain interaction, signing, and transaction construction.
///
/// Default methods return [`CallError::HostFailure`] with an `unavailable`
/// reason. Hosts override only the methods they can actually service.
#[async_trait::async_trait]
pub trait ChainInteraction: Send + Sync {
    /// Construct a signed extrinsic for a product account.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostCreateTransactionResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function createTransaction(
    ///   truapi: Client,
    /// ): Promise<HostCreateTransactionResponse> {
    ///   const result = await truapi.chainInteraction.createTransaction({
    ///     productAccountId: {
    ///       dotNsIdentifier: "truapi-playground.dot",
    ///       derivationIndex: 0,
    ///     },
    ///     payload: {
    ///       tag: "V1",
    ///       value: {
    ///         callData: "0x0000",
    ///         extensions: [],
    ///         txExtVersion: 0,
    ///         context: {
    ///           metadata: "0x",
    ///           tokenSymbol: "DOT",
    ///           tokenDecimals: 10,
    ///           bestBlockHeight: 0,
    ///         },
    ///       },
    ///     },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 30)]
    async fn host_create_transaction(
        &self,
        _cx: &CallContext,
        _request: HostCreateTransactionRequest,
    ) -> Result<HostCreateTransactionResponse, CallError<HostCreateTransactionError>> {
        Err(CallError::unavailable())
    }

    /// Construct a signed extrinsic for a non-product account.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostCreateTransactionWithLegacyAccountResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function createTransactionWithLegacyAccount(
    ///   truapi: Client,
    /// ): Promise<HostCreateTransactionWithLegacyAccountResponse> {
    ///   const result = await truapi.chainInteraction.createTransactionWithLegacyAccount({
    ///     payload: {
    ///       tag: "V1",
    ///       value: {
    ///         callData: "0x0000",
    ///         extensions: [],
    ///         txExtVersion: 0,
    ///         context: {
    ///           metadata: "0x",
    ///           tokenSymbol: "DOT",
    ///           tokenDecimals: 10,
    ///           bestBlockHeight: 0,
    ///         },
    ///       },
    ///     },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 32)]
    async fn host_create_transaction_with_legacy_account(
        &self,
        _cx: &CallContext,
        _request: HostCreateTransactionWithLegacyAccountRequest,
    ) -> Result<
        HostCreateTransactionWithLegacyAccountResponse,
        CallError<HostCreateTransactionWithLegacyAccountError>,
    > {
        Err(CallError::unavailable())
    }

    /// Sign raw bytes with a non-product (legacy) account.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostSignPayloadResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function signRawWithLegacyAccount(
    ///   truapi: Client,
    /// ): Promise<HostSignPayloadResponse> {
    ///   const result = await truapi.chainInteraction.signRawWithLegacyAccount({
    ///     signer: "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    ///     payload: {
    ///       tag: "Bytes",
    ///       value: { bytes: "0x48656c6c6f" },
    ///     },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 34)]
    async fn host_sign_raw_with_legacy_account(
        &self,
        _cx: &CallContext,
        _request: HostSignRawWithLegacyAccountRequest,
    ) -> Result<HostSignRawWithLegacyAccountResponse, CallError<HostSignRawWithLegacyAccountError>>
    {
        Err(CallError::unavailable())
    }

    /// Sign a Substrate extrinsic payload with a non-product (legacy) account.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostSignPayloadResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function signPayloadWithLegacyAccount(
    ///   truapi: Client,
    /// ): Promise<HostSignPayloadResponse> {
    ///   const result = await truapi.chainInteraction.signPayloadWithLegacyAccount({
    ///     signer: "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    ///     payload: {
    ///       account: { dotNsIdentifier: "truapi-playground.dot", derivationIndex: 0 },
    ///       blockHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///       blockNumber: "0x00000000",
    ///       era: "0x00",
    ///       genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///       method: "0x0000",
    ///       nonce: "0x00000000",
    ///       signedExtensions: [],
    ///       specVersion: "0x00000000",
    ///       tip: "0x00000000000000000000000000000000",
    ///       transactionVersion: "0x00000000",
    ///       version: 4,
    ///     },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 36)]
    async fn host_sign_payload_with_legacy_account(
        &self,
        _cx: &CallContext,
        _request: HostSignPayloadWithLegacyAccountRequest,
    ) -> Result<
        HostSignPayloadWithLegacyAccountResponse,
        CallError<HostSignPayloadWithLegacyAccountError>,
    > {
        Err(CallError::unavailable())
    }

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
    ///     .chainHeadFollowSubscribe({
    ///       request: {
    ///         genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///         withRuntime: false,
    ///       },
    ///     })
    ///     .subscribe({
    ///       next: (item: RemoteChainHeadFollowItem) => console.log(item),
    ///       error: (error: Error) => console.error(error),
    ///       complete: () => console.log("completed"),
    ///     });
    /// }
    /// ```
    #[wire(start_id = 76)]
    async fn remote_chain_head_follow_subscribe(
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
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     followSubscriptionId: "",
    ///     hash: "0x0000000000000000000000000000000000000000000000000000000000000000",
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
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     followSubscriptionId: "",
    ///     hash: "0x0000000000000000000000000000000000000000000000000000000000000000",
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
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     followSubscriptionId: "",
    ///     hash: "0x0000000000000000000000000000000000000000000000000000000000000000",
    ///     items: [
    ///       {
    ///         key: "0x26aa394eea5630e07c48ae0c9558cef7",
    ///         queryType: "Value",
    ///       },
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
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     followSubscriptionId: "",
    ///     hash: "0x0000000000000000000000000000000000000000000000000000000000000000",
    ///     function: "Core_version",
    ///     callParameters: "0x",
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
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     followSubscriptionId: "",
    ///     hashes: [
    ///       "0x0000000000000000000000000000000000000000000000000000000000000000",
    ///     ],
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
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
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
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
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
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
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
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
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
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
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
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     transaction: "0x",
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
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
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

    /// Send a JSON-RPC message to the chain identified by genesis hash.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function sendJsonRpc(truapi: Client): Promise<void> {
    ///   const result = await truapi.chainInteraction.jsonrpcMessageSend({
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     message: "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"system_name\",\"params\":[]}",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 70)]
    async fn host_jsonrpc_message_send(
        &self,
        _cx: &CallContext,
        _request: HostJsonrpcMessageSendRequest,
    ) -> Result<HostJsonrpcMessageSendResponse, CallError<HostJsonrpcMessageSendError>> {
        Err(CallError::unavailable())
    }

    /// Subscribe to inbound JSON-RPC messages for a chain.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type Subscription,
    ///   type HostJsonrpcMessageSubscribeItem,
    /// } from "@parity/truapi";
    ///
    /// export function subscribeJsonRpc(truapi: Client): Subscription {
    ///   return truapi.chainInteraction
    ///     .jsonrpcMessageSubscribe({
    ///       request: {
    ///         genesisHash:
    ///           "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///       },
    ///     })
    ///     .subscribe({
    ///       next: (item: HostJsonrpcMessageSubscribeItem) =>
    ///         console.log(item),
    ///       error: (error: Error) => console.error(error),
    ///       complete: () => console.log("completed"),
    ///     });
    /// }
    /// ```
    #[wire(start_id = 72)]
    async fn host_jsonrpc_message_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostJsonrpcMessageSubscribeRequest,
    ) -> Subscription<HostJsonrpcMessageSubscribeItem> {
        Subscription::empty()
    }

    /// Sign raw bytes or a message.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostSignPayloadResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function signRawBytes(
    ///   truapi: Client,
    /// ): Promise<HostSignPayloadResponse> {
    ///   const result = await truapi.chainInteraction.signRaw({
    ///     account: { dotNsIdentifier: "truapi-playground.dot", derivationIndex: 0 },
    ///     payload: {
    ///       tag: "Bytes",
    ///       value: {
    ///         bytes: "0x48656c6c6f2c20776f726c6421",
    ///       },
    ///     },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 114)]
    async fn host_sign_raw(
        &self,
        _cx: &CallContext,
        _request: HostSignRawRequest,
    ) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
        Err(CallError::unavailable())
    }

    /// Sign a Substrate extrinsic payload.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostSignPayloadResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function signPayload(
    ///   truapi: Client,
    /// ): Promise<HostSignPayloadResponse> {
    ///   const result = await truapi.chainInteraction.signPayload({
    ///     account: { dotNsIdentifier: "truapi-playground.dot", derivationIndex: 0 },
    ///     blockHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     blockNumber: "0x00000000",
    ///     era: "0x00",
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     method: "0x00003448656c6c6f2c20776f726c6421",
    ///     nonce: "0x00000000",
    ///     signedExtensions: [],
    ///     specVersion: "0x00000000",
    ///     tip: "0x00000000000000000000000000000000",
    ///     transactionVersion: "0x00000000",
    ///     version: 4,
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 116)]
    async fn host_sign_payload(
        &self,
        _cx: &CallContext,
        _request: HostSignPayloadRequest,
    ) -> Result<HostSignPayloadResponse, CallError<HostSignPayloadError>> {
        Err(CallError::unavailable())
    }
}
