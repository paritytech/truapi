//! Unified [`JsonRpc`] trait.

use crate::versioned::jsonrpc::{
    HostJsonrpcMessageSendError, HostJsonrpcMessageSendRequest, HostJsonrpcMessageSendResponse,
    HostJsonrpcMessageSubscribeItem, HostJsonrpcMessageSubscribeRequest,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Raw JSON-RPC passthrough to a chain node.
///
/// Default methods return [`CallError::HostFailure`] with an `unavailable`
/// reason. Hosts override only the methods they actually support.
///
#[async_trait::async_trait]
pub trait JsonRpc: Send + Sync {
    /// Send a JSON-RPC message to the chain identified by genesis hash.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function sendJsonRpc(truapi: Client): Promise<void> {
    ///   const result = await truapi.jsonRpc.jsonrpcMessageSend({
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
    ///   return truapi.jsonRpc
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
}
