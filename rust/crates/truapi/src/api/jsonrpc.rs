//! Unified [`JsonRpc`] trait.

use crate::versioned::jsonrpc::{
    HostJsonrpcMessageSendError, HostJsonrpcMessageSendRequest, HostJsonrpcMessageSendResponse,
    HostJsonrpcMessageSubscribeItem, HostJsonrpcMessageSubscribeRequest,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// JSON-RPC transport methods.
pub trait JsonRpc: Send + Sync {
    /// Send a JSON-RPC message.
    ///
    /// ```ts
    /// const result = await truapi.jsonRpc.sendMessage({
    ///   genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///   message: '{"jsonrpc":"2.0","id":1,"method":"system_name","params":[]}',
    /// });
    /// result.match(
    ///   () => console.log("ok"),
    ///   (error) => console.error(error),
    /// );
    /// ```
    #[wire(request_id = 70)]
    async fn send_message(
        &self,
        _cx: &CallContext,
        _request: HostJsonrpcMessageSendRequest,
    ) -> Result<HostJsonrpcMessageSendResponse, CallError<HostJsonrpcMessageSendError>> {
        Err(CallError::unavailable())
    }

    /// Subscribe to inbound JSON-RPC messages.
    ///
    /// ```ts
    /// truapi.jsonRpc
    ///   .subscribeMessages({
    ///     request: {
    ///       genesisHash:
    ///         "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     },
    ///   })
    ///   .subscribe({
    ///     next: (item) => console.log(item),
    ///     error: (error) => console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
    /// ```
    #[wire(start_id = 72)]
    async fn subscribe_messages(
        &self,
        _cx: &CallContext,
        _request: HostJsonrpcMessageSubscribeRequest,
    ) -> Subscription<HostJsonrpcMessageSubscribeItem> {
        Subscription::empty()
    }
}
