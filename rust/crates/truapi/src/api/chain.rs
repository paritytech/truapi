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
    /// import {
    ///   type Client,
    ///   type Subscription,
    ///   type RemoteChainHeadFollowItem,
    /// } from "@parity/truapi";
    ///
    /// export function followChainHead(truapi: Client): Subscription {
    ///   return truapi.chain
    ///     .followHeadSubscribe({
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
    /// import {
    ///   type Client,
    ///   type RemoteChainHeadHeaderResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function getChainHeadHeader(
    ///   truapi: Client,
    /// ): Promise<RemoteChainHeadHeaderResponse> {
    ///   const genesisHash =
    ///     "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2";
    ///
    ///   let cleanup = () => {};
    ///   const { subscriptionId, finalizedHash } = await new Promise<{
    ///     subscriptionId: string;
    ///     finalizedHash: `0x${string}`;
    ///   }>((resolve, reject) => {
    ///     const sub = truapi.chain
    ///       .followHeadSubscribe({
    ///         request: { genesisHash, withRuntime: false },
    ///       })
    ///       .subscribe({
    ///         next: (item) => {
    ///           if (item.tag === "Initialized") {
    ///             resolve({
    ///               subscriptionId: sub.subscriptionId,
    ///               finalizedHash: item.value.finalizedBlockHashes[0],
    ///             });
    ///           }
    ///         },
    ///         error: reject,
    ///         complete: () =>
    ///           reject(new Error("follow ended before Initialized")),
    ///       });
    ///     cleanup = () => sub.unsubscribe();
    ///   });
    ///
    ///   try {
    ///     const result = await truapi.chain.getHeadHeader({
    ///       genesisHash,
    ///       followSubscriptionId: subscriptionId,
    ///       hash: finalizedHash,
    ///     });
    ///     if (result.isErr()) throw result.error;
    ///     return result.value;
    ///   } finally {
    ///     cleanup();
    ///   }
    /// }
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
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function getChainHeadBody(
    ///   truapi: Client,
    /// ): Promise<{ hexBlobs: string[] }> {
    ///   const genesisHash =
    ///     "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2";
    ///
    ///   let operationId: string | null = null;
    ///   let cleanup = () => {};
    ///
    ///   const body = await new Promise<{ hexBlobs: string[] }>(
    ///     (resolve, reject) => {
    ///       const sub = truapi.chain
    ///         .followHeadSubscribe({
    ///           request: { genesisHash, withRuntime: false },
    ///         })
    ///         .subscribe({
    ///           next: async (item) => {
    ///             try {
    ///               if (item.tag === "Initialized") {
    ///                 const started = await truapi.chain.getHeadBody({
    ///                   genesisHash,
    ///                   followSubscriptionId: sub.subscriptionId,
    ///                   hash: item.value.finalizedBlockHashes[0],
    ///                 });
    ///                 if (started.isErr()) throw started.error;
    ///                 const op = started.value.operation;
    ///                 if (op.tag !== "Started") {
    ///                   throw new Error("body call rejected: " + op.tag);
    ///                 }
    ///                 operationId = op.value.operationId;
    ///               } else if (
    ///                 operationId &&
    ///                 (item.tag === "OperationBodyDone" ||
    ///                   item.tag === "OperationError" ||
    ///                   item.tag === "OperationInaccessible") &&
    ///                 item.value.operationId === operationId
    ///               ) {
    ///                 if (item.tag === "OperationBodyDone") {
    ///                   resolve({
    ///                     hexBlobs: item.value.value as unknown as string[],
    ///                   });
    ///                 } else if (item.tag === "OperationError") {
    ///                   reject(new Error("operation error: " + item.value.error));
    ///                 } else {
    ///                   reject(new Error("operation inaccessible"));
    ///                 }
    ///               }
    ///             } catch (err) {
    ///               reject(err as Error);
    ///             }
    ///           },
    ///           error: reject,
    ///           complete: () =>
    ///             reject(new Error("follow ended before body result")),
    ///         });
    ///       cleanup = () => sub.unsubscribe();
    ///     },
    ///   );
    ///
    ///   cleanup();
    ///   return body;
    /// }
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
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function getChainHeadStorage(truapi: Client): Promise<{
    ///   items: unknown[];
    /// }> {
    ///   const genesisHash =
    ///     "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2";
    ///   const storageKey = "0x26aa394eea5630e07c48ae0c9558cef7";
    ///
    ///   let operationId: string | null = null;
    ///   let cleanup = () => {};
    ///   const items: unknown[] = [];
    ///
    ///   const done = await new Promise<{ items: unknown[] }>(
    ///     (resolve, reject) => {
    ///       const sub = truapi.chain
    ///         .followHeadSubscribe({
    ///           request: { genesisHash, withRuntime: false },
    ///         })
    ///         .subscribe({
    ///           next: async (item) => {
    ///             try {
    ///               if (item.tag === "Initialized") {
    ///                 const started = await truapi.chain.getHeadStorage({
    ///                   genesisHash,
    ///                   followSubscriptionId: sub.subscriptionId,
    ///                   hash: item.value.finalizedBlockHashes[0],
    ///                   items: [{ key: storageKey, queryType: "Value" }],
    ///                 });
    ///                 if (started.isErr()) throw started.error;
    ///                 const op = started.value.operation;
    ///                 if (op.tag !== "Started") {
    ///                   throw new Error("storage call rejected: " + op.tag);
    ///                 }
    ///                 operationId = op.value.operationId;
    ///               } else if (
    ///                 operationId &&
    ///                 (item.tag === "OperationStorageItems" ||
    ///                   item.tag === "OperationStorageDone" ||
    ///                   item.tag === "OperationWaitingForContinue" ||
    ///                   item.tag === "OperationError" ||
    ///                   item.tag === "OperationInaccessible") &&
    ///                 item.value.operationId === operationId
    ///               ) {
    ///                 if (item.tag === "OperationStorageItems") {
    ///                   items.push(...item.value.items);
    ///                 } else if (item.tag === "OperationWaitingForContinue") {
    ///                   const cont = await truapi.chain.continueHead({
    ///                     genesisHash,
    ///                     followSubscriptionId: sub.subscriptionId,
    ///                     operationId,
    ///                   });
    ///                   if (cont.isErr()) throw cont.error;
    ///                 } else if (item.tag === "OperationStorageDone") {
    ///                   resolve({ items });
    ///                 } else if (item.tag === "OperationError") {
    ///                   reject(new Error("operation error: " + item.value.error));
    ///                 } else {
    ///                   reject(new Error("operation inaccessible"));
    ///                 }
    ///               }
    ///             } catch (err) {
    ///               reject(err as Error);
    ///             }
    ///           },
    ///           error: reject,
    ///           complete: () =>
    ///             reject(new Error("follow ended before storage result")),
    ///         });
    ///       cleanup = () => sub.unsubscribe();
    ///     },
    ///   );
    ///
    ///   cleanup();
    ///   return done;
    /// }
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
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function callChainHeadRuntime(
    ///   truapi: Client,
    /// ): Promise<{ output: string }> {
    ///   const genesisHash =
    ///     "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2";
    ///
    ///   let operationId: string | null = null;
    ///   let cleanup = () => {};
    ///
    ///   const callResult = await new Promise<{ output: string }>(
    ///     (resolve, reject) => {
    ///       const sub = truapi.chain
    ///         .followHeadSubscribe({
    ///           request: { genesisHash, withRuntime: false },
    ///         })
    ///         .subscribe({
    ///           next: async (item) => {
    ///             try {
    ///               if (item.tag === "Initialized") {
    ///                 const started = await truapi.chain.callHead({
    ///                   genesisHash,
    ///                   followSubscriptionId: sub.subscriptionId,
    ///                   hash: item.value.finalizedBlockHashes[0],
    ///                   function: "Core_version",
    ///                   callParameters: "0x",
    ///                 });
    ///                 if (started.isErr()) throw started.error;
    ///                 const op = started.value.operation;
    ///                 if (op.tag !== "Started") {
    ///                   throw new Error("call rejected: " + op.tag);
    ///                 }
    ///                 operationId = op.value.operationId;
    ///               } else if (
    ///                 operationId &&
    ///                 (item.tag === "OperationCallDone" ||
    ///                   item.tag === "OperationError" ||
    ///                   item.tag === "OperationInaccessible") &&
    ///                 item.value.operationId === operationId
    ///               ) {
    ///                 if (item.tag === "OperationCallDone") {
    ///                   resolve({
    ///                     output: item.value.output as unknown as string,
    ///                   });
    ///                 } else if (item.tag === "OperationError") {
    ///                   reject(new Error("operation error: " + item.value.error));
    ///                 } else {
    ///                   reject(new Error("operation inaccessible"));
    ///                 }
    ///               }
    ///             } catch (err) {
    ///               reject(err as Error);
    ///             }
    ///           },
    ///           error: reject,
    ///           complete: () =>
    ///             reject(new Error("follow ended before call result")),
    ///         });
    ///       cleanup = () => sub.unsubscribe();
    ///     },
    ///   );
    ///
    ///   cleanup();
    ///   return callResult;
    /// }
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
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function unpinChainHead(truapi: Client): Promise<void> {
    ///   const genesisHash =
    ///     "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2";
    ///
    ///   let cleanup = () => {};
    ///   const { subscriptionId, finalizedHash } = await new Promise<{
    ///     subscriptionId: string;
    ///     finalizedHash: `0x${string}`;
    ///   }>((resolve, reject) => {
    ///     const sub = truapi.chain
    ///       .followHeadSubscribe({
    ///         request: { genesisHash, withRuntime: false },
    ///       })
    ///       .subscribe({
    ///         next: (item) => {
    ///           if (item.tag === "Initialized") {
    ///             resolve({
    ///               subscriptionId: sub.subscriptionId,
    ///               finalizedHash: item.value.finalizedBlockHashes[0],
    ///             });
    ///           }
    ///         },
    ///         error: reject,
    ///         complete: () =>
    ///           reject(new Error("follow ended before Initialized")),
    ///       });
    ///     cleanup = () => sub.unsubscribe();
    ///   });
    ///
    ///   try {
    ///     const result = await truapi.chain.unpinHead({
    ///       genesisHash,
    ///       followSubscriptionId: subscriptionId,
    ///       hashes: [finalizedHash],
    ///     });
    ///     if (result.isErr()) throw result.error;
    ///   } finally {
    ///     cleanup();
    ///   }
    /// }
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
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function continueChainHeadOperation(
    ///   truapi: Client,
    /// ): Promise<void> {
    ///   const genesisHash =
    ///     "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2";
    ///
    ///   let cleanup = () => {};
    ///   const subscriptionId = await new Promise<string>((resolve, reject) => {
    ///     const sub = truapi.chain
    ///       .followHeadSubscribe({
    ///         request: { genesisHash, withRuntime: false },
    ///       })
    ///       .subscribe({
    ///         next: (item) => {
    ///           if (item.tag === "Initialized") resolve(sub.subscriptionId);
    ///         },
    ///         error: reject,
    ///         complete: () =>
    ///           reject(new Error("follow ended before Initialized")),
    ///       });
    ///     cleanup = () => sub.unsubscribe();
    ///   });
    ///
    ///   try {
    ///     const result = await truapi.chain.continueHead({
    ///       genesisHash,
    ///       followSubscriptionId: subscriptionId,
    ///       operationId: "op-id",
    ///     });
    ///     if (result.isErr()) throw result.error;
    ///   } finally {
    ///     cleanup();
    ///   }
    /// }
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
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function stopChainHeadOperation(
    ///   truapi: Client,
    /// ): Promise<void> {
    ///   const genesisHash =
    ///     "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2";
    ///
    ///   let cleanup = () => {};
    ///   const subscriptionId = await new Promise<string>((resolve, reject) => {
    ///     const sub = truapi.chain
    ///       .followHeadSubscribe({
    ///         request: { genesisHash, withRuntime: false },
    ///       })
    ///       .subscribe({
    ///         next: (item) => {
    ///           if (item.tag === "Initialized") resolve(sub.subscriptionId);
    ///         },
    ///         error: reject,
    ///         complete: () =>
    ///           reject(new Error("follow ended before Initialized")),
    ///       });
    ///     cleanup = () => sub.unsubscribe();
    ///   });
    ///
    ///   try {
    ///     const result = await truapi.chain.stopHeadOperation({
    ///       genesisHash,
    ///       followSubscriptionId: subscriptionId,
    ///       operationId: "op-id",
    ///     });
    ///     if (result.isErr()) throw result.error;
    ///   } finally {
    ///     cleanup();
    ///   }
    /// }
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
    /// import {
    ///   type Client,
    ///   type RemoteChainSpecGenesisHashResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function getChainGenesisHash(
    ///   truapi: Client,
    /// ): Promise<RemoteChainSpecGenesisHashResponse> {
    ///   const result = await truapi.chain.getSpecGenesisHash({
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
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
    /// import {
    ///   type Client,
    ///   type RemoteChainSpecChainNameResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function getChainName(
    ///   truapi: Client,
    /// ): Promise<RemoteChainSpecChainNameResponse> {
    ///   const result = await truapi.chain.getSpecChainName({
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
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
    /// import {
    ///   type Client,
    ///   type RemoteChainSpecPropertiesResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function getChainProperties(
    ///   truapi: Client,
    /// ): Promise<RemoteChainSpecPropertiesResponse> {
    ///   const result = await truapi.chain.getSpecProperties({
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
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
    /// import {
    ///   type Client,
    ///   type RemoteChainTransactionBroadcastResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function broadcastTransaction(
    ///   truapi: Client,
    /// ): Promise<RemoteChainTransactionBroadcastResponse> {
    ///   const result = await truapi.chain.broadcastTransaction({
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     transaction: "0x",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
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
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function stopTransactionBroadcast(
    ///   truapi: Client,
    /// ): Promise<void> {
    ///   const result = await truapi.chain.stopTransaction({
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     operationId: "op-id",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
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
