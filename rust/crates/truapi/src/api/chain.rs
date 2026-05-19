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
    /// import { from, take } from "rxjs";
    ///
    /// from(
    ///   truapi.chain.followHeadSubscribe({
    ///     request: {
    ///       genesisHash:
    ///         "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///       withRuntime: false,
    ///     },
    ///   }),
    /// )
    ///   .pipe(take(3))
    ///   .subscribe({
    ///     next: (item) => console.log(item),
    ///     error: (error) => console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
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
    /// import { Observable, filter, mergeMap, take } from "rxjs";
    ///
    /// followHead({
    ///   genesisHash:
    ///     "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    /// })
    ///   .pipe(
    ///     filter(({ item }) => item.tag === "Initialized"),
    ///     take(1),
    ///     mergeMap(({ item, followSubscriptionId, genesisHash }) =>
    ///       truapi.chain.getHeadHeader({
    ///         genesisHash,
    ///         followSubscriptionId,
    ///         hash: item.value.finalizedBlockHashes[0],
    ///       }),
    ///     ),
    ///   )
    ///   .subscribe((result) =>
    ///     result.match(
    ///       (value) => console.log(value),
    ///       (error) => console.error(error),
    ///     ),
    ///   );
    ///
    /// // #region helpers
    /// function followHead({ genesisHash }: { genesisHash: `0x${string}` }) {
    ///   return new Observable<{
    ///     item: any;
    ///     followSubscriptionId: string;
    ///     genesisHash: `0x${string}`;
    ///   }>((observer) => {
    ///     const sub = truapi.chain
    ///       .followHeadSubscribe({ request: { genesisHash, withRuntime: false } })
    ///       .subscribe({
    ///         next: (item) =>
    ///           observer.next({
    ///             item,
    ///             followSubscriptionId: sub.subscriptionId,
    ///             genesisHash,
    ///           }),
    ///         error: (err) => observer.error(err),
    ///         complete: () => observer.complete(),
    ///       });
    ///     return () => sub.unsubscribe();
    ///   });
    /// }
    /// // #endregion
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
    /// const genesisHash =
    ///   "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2";
    /// const body = await withChainOperation(
    ///   genesisHash,
    ///   ({ subscriptionId, hash }) =>
    ///     truapi.chain.getHeadBody({
    ///       genesisHash,
    ///       followSubscriptionId: subscriptionId,
    ///       hash,
    ///     }),
    ///   (item) => {
    ///     if (item.tag === "OperationBodyDone") return { done: item.value.value };
    ///   },
    /// );
    /// console.log(body);
    ///
    /// // #region helpers
    /// async function withChainOperation(
    ///   genesisHash: `0x${string}`,
    ///   start: (ctx: { subscriptionId: string; hash: `0x${string}` }) => PromiseLike<any>,
    ///   onResult: (item: any, ctx: { sub: any; operationId: string }) => any,
    /// ): Promise<any> {
    ///   return await new Promise<any>((resolve, reject) => {
    ///     let operationId: string | null = null;
    ///     const sub = truapi.chain
    ///       .followHeadSubscribe({ request: { genesisHash, withRuntime: false } })
    ///       .subscribe({
    ///         next: async (item) => {
    ///           try {
    ///             if (item.tag === "Initialized") {
    ///               const started = await start({
    ///                 subscriptionId: sub.subscriptionId,
    ///                 hash: item.value.finalizedBlockHashes[0],
    ///               });
    ///               if (started.isErr()) { reject(started.error); return; }
    ///               const op = started.value.operation;
    ///               if (op.tag !== "Started") { reject(new Error("rejected: " + op.tag)); return; }
    ///               operationId = op.value.operationId;
    ///               return;
    ///             }
    ///             const value = item.value as any;
    ///             if (!operationId || value?.operationId !== operationId) return;
    ///             if (item.tag === "OperationError") {
    ///               reject(new Error("operation error: " + value.error));
    ///               return;
    ///             }
    ///             if (item.tag === "OperationInaccessible") {
    ///               reject(new Error("operation inaccessible"));
    ///               return;
    ///             }
    ///             const out = await onResult(item, { sub, operationId });
    ///             if (out && "done" in out) resolve(out.done);
    ///           } catch (err) {
    ///             reject(err);
    ///           }
    ///         },
    ///         error: reject,
    ///         complete: () => reject(new Error("follow ended before result")),
    ///       });
    ///   });
    /// }
    /// // #endregion
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
    /// const genesisHash =
    ///   "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2";
    /// const storageKey = "0x26aa394eea5630e07c48ae0c9558cef7";
    /// const items: unknown[] = [];
    /// await withChainOperation(
    ///   genesisHash,
    ///   ({ subscriptionId, hash }) =>
    ///     truapi.chain.getHeadStorage({
    ///       genesisHash,
    ///       followSubscriptionId: subscriptionId,
    ///       hash,
    ///       items: [{ key: storageKey, queryType: "Value" }],
    ///     }),
    ///   async (item, { sub, operationId }) => {
    ///     if (item.tag === "OperationStorageItems") items.push(...item.value.items);
    ///     else if (item.tag === "OperationWaitingForContinue") {
    ///       const cont = await truapi.chain.continueHead({
    ///         genesisHash,
    ///         followSubscriptionId: sub.subscriptionId,
    ///         operationId,
    ///       });
    ///       cont.match(
    ///         () => {},
    ///         (error) => console.error(error),
    ///       );
    ///     } else if (item.tag === "OperationStorageDone") {
    ///       return { done: items };
    ///     }
    ///   },
    /// );
    /// console.log(items);
    ///
    /// // #region helpers
    /// async function withChainOperation(
    ///   genesisHash: `0x${string}`,
    ///   start: (ctx: { subscriptionId: string; hash: `0x${string}` }) => PromiseLike<any>,
    ///   onResult: (item: any, ctx: { sub: any; operationId: string }) => any,
    /// ): Promise<any> {
    ///   return await new Promise<any>((resolve, reject) => {
    ///     let operationId: string | null = null;
    ///     const sub = truapi.chain
    ///       .followHeadSubscribe({ request: { genesisHash, withRuntime: false } })
    ///       .subscribe({
    ///         next: async (item) => {
    ///           try {
    ///             if (item.tag === "Initialized") {
    ///               const started = await start({
    ///                 subscriptionId: sub.subscriptionId,
    ///                 hash: item.value.finalizedBlockHashes[0],
    ///               });
    ///               if (started.isErr()) { reject(started.error); return; }
    ///               const op = started.value.operation;
    ///               if (op.tag !== "Started") { reject(new Error("rejected: " + op.tag)); return; }
    ///               operationId = op.value.operationId;
    ///               return;
    ///             }
    ///             const value = item.value as any;
    ///             if (!operationId || value?.operationId !== operationId) return;
    ///             if (item.tag === "OperationError") {
    ///               reject(new Error("operation error: " + value.error));
    ///               return;
    ///             }
    ///             if (item.tag === "OperationInaccessible") {
    ///               reject(new Error("operation inaccessible"));
    ///               return;
    ///             }
    ///             const out = await onResult(item, { sub, operationId });
    ///             if (out && "done" in out) resolve(out.done);
    ///           } catch (err) {
    ///             reject(err);
    ///           }
    ///         },
    ///         error: reject,
    ///         complete: () => reject(new Error("follow ended before result")),
    ///       });
    ///   });
    /// }
    /// // #endregion
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
    /// const genesisHash =
    ///   "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2";
    /// const output = await withChainOperation(
    ///   genesisHash,
    ///   ({ subscriptionId, hash }) =>
    ///     truapi.chain.callHead({
    ///       genesisHash,
    ///       followSubscriptionId: subscriptionId,
    ///       hash,
    ///       function: "Core_version",
    ///       callParameters: "0x",
    ///     }),
    ///   (item) => {
    ///     if (item.tag === "OperationCallDone") return { done: item.value.output };
    ///   },
    /// );
    /// console.log(output);
    ///
    /// // #region helpers
    /// async function withChainOperation(
    ///   genesisHash: `0x${string}`,
    ///   start: (ctx: { subscriptionId: string; hash: `0x${string}` }) => PromiseLike<any>,
    ///   onResult: (item: any, ctx: { sub: any; operationId: string }) => any,
    /// ): Promise<any> {
    ///   return await new Promise<any>((resolve, reject) => {
    ///     let operationId: string | null = null;
    ///     const sub = truapi.chain
    ///       .followHeadSubscribe({ request: { genesisHash, withRuntime: false } })
    ///       .subscribe({
    ///         next: async (item) => {
    ///           try {
    ///             if (item.tag === "Initialized") {
    ///               const started = await start({
    ///                 subscriptionId: sub.subscriptionId,
    ///                 hash: item.value.finalizedBlockHashes[0],
    ///               });
    ///               if (started.isErr()) { reject(started.error); return; }
    ///               const op = started.value.operation;
    ///               if (op.tag !== "Started") { reject(new Error("rejected: " + op.tag)); return; }
    ///               operationId = op.value.operationId;
    ///               return;
    ///             }
    ///             const value = item.value as any;
    ///             if (!operationId || value?.operationId !== operationId) return;
    ///             if (item.tag === "OperationError") {
    ///               reject(new Error("operation error: " + value.error));
    ///               return;
    ///             }
    ///             if (item.tag === "OperationInaccessible") {
    ///               reject(new Error("operation inaccessible"));
    ///               return;
    ///             }
    ///             const out = await onResult(item, { sub, operationId });
    ///             if (out && "done" in out) resolve(out.done);
    ///           } catch (err) {
    ///             reject(err);
    ///           }
    ///         },
    ///         error: reject,
    ///         complete: () => reject(new Error("follow ended before result")),
    ///       });
    ///   });
    /// }
    /// // #endregion
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
    /// import { Observable, filter, mergeMap, take } from "rxjs";
    ///
    /// followHead({
    ///   genesisHash:
    ///     "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    /// })
    ///   .pipe(
    ///     filter(({ item }) => item.tag === "Initialized"),
    ///     take(1),
    ///     mergeMap(({ item, followSubscriptionId, genesisHash }) =>
    ///       truapi.chain.unpinHead({
    ///         genesisHash,
    ///         followSubscriptionId,
    ///         hashes: [item.value.finalizedBlockHashes[0]],
    ///       }),
    ///     ),
    ///   )
    ///   .subscribe((result) =>
    ///     result.match(
    ///       () => console.log("ok"),
    ///       (error) => console.error(error),
    ///     ),
    ///   );
    ///
    /// // #region helpers
    /// function followHead({ genesisHash }: { genesisHash: `0x${string}` }) {
    ///   return new Observable<{
    ///     item: any;
    ///     followSubscriptionId: string;
    ///     genesisHash: `0x${string}`;
    ///   }>((observer) => {
    ///     const sub = truapi.chain
    ///       .followHeadSubscribe({ request: { genesisHash, withRuntime: false } })
    ///       .subscribe({
    ///         next: (item) =>
    ///           observer.next({
    ///             item,
    ///             followSubscriptionId: sub.subscriptionId,
    ///             genesisHash,
    ///           }),
    ///         error: (err) => observer.error(err),
    ///         complete: () => observer.complete(),
    ///       });
    ///     return () => sub.unsubscribe();
    ///   });
    /// }
    /// // #endregion
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
    /// import { Observable, filter, mergeMap, take } from "rxjs";
    ///
    /// followHead({
    ///   genesisHash:
    ///     "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    /// })
    ///   .pipe(
    ///     filter(({ item }) => item.tag === "Initialized"),
    ///     take(1),
    ///     mergeMap(({ followSubscriptionId, genesisHash }) =>
    ///       truapi.chain.continueHead({
    ///         genesisHash,
    ///         followSubscriptionId,
    ///         operationId: "op-id",
    ///       }),
    ///     ),
    ///   )
    ///   .subscribe((result) =>
    ///     result.match(
    ///       () => console.log("ok"),
    ///       (error) => console.error(error),
    ///     ),
    ///   );
    ///
    /// // #region helpers
    /// function followHead({ genesisHash }: { genesisHash: `0x${string}` }) {
    ///   return new Observable<{
    ///     item: any;
    ///     followSubscriptionId: string;
    ///     genesisHash: `0x${string}`;
    ///   }>((observer) => {
    ///     const sub = truapi.chain
    ///       .followHeadSubscribe({ request: { genesisHash, withRuntime: false } })
    ///       .subscribe({
    ///         next: (item) =>
    ///           observer.next({
    ///             item,
    ///             followSubscriptionId: sub.subscriptionId,
    ///             genesisHash,
    ///           }),
    ///         error: (err) => observer.error(err),
    ///         complete: () => observer.complete(),
    ///       });
    ///     return () => sub.unsubscribe();
    ///   });
    /// }
    /// // #endregion
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
    /// import { Observable, filter, mergeMap, take } from "rxjs";
    ///
    /// followHead({
    ///   genesisHash:
    ///     "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    /// })
    ///   .pipe(
    ///     filter(({ item }) => item.tag === "Initialized"),
    ///     take(1),
    ///     mergeMap(({ followSubscriptionId, genesisHash }) =>
    ///       truapi.chain.stopHeadOperation({
    ///         genesisHash,
    ///         followSubscriptionId,
    ///         operationId: "op-id",
    ///       }),
    ///     ),
    ///   )
    ///   .subscribe((result) =>
    ///     result.match(
    ///       () => console.log("ok"),
    ///       (error) => console.error(error),
    ///     ),
    ///   );
    ///
    /// // #region helpers
    /// function followHead({ genesisHash }: { genesisHash: `0x${string}` }) {
    ///   return new Observable<{
    ///     item: any;
    ///     followSubscriptionId: string;
    ///     genesisHash: `0x${string}`;
    ///   }>((observer) => {
    ///     const sub = truapi.chain
    ///       .followHeadSubscribe({ request: { genesisHash, withRuntime: false } })
    ///       .subscribe({
    ///         next: (item) =>
    ///           observer.next({
    ///             item,
    ///             followSubscriptionId: sub.subscriptionId,
    ///             genesisHash,
    ///           }),
    ///         error: (err) => observer.error(err),
    ///         complete: () => observer.complete(),
    ///       });
    ///     return () => sub.unsubscribe();
    ///   });
    /// }
    /// // #endregion
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
    /// const result = await truapi.chain.getSpecGenesisHash({
    ///   genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    /// });
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
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
    /// const result = await truapi.chain.getSpecChainName({
    ///   genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    /// });
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
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
    /// const result = await truapi.chain.getSpecProperties({
    ///   genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    /// });
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
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
    /// const result = await truapi.chain.broadcastTransaction({
    ///   genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///   transaction: "0x",
    /// });
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
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
    /// const result = await truapi.chain.stopTransaction({
    ///   genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///   operationId: "op-id",
    /// });
    /// result.match(
    ///   () => console.log("ok"),
    ///   (error) => console.error(error),
    /// );
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
