//! Unified [`Chat`] trait.

use crate::versioned::chat::{
    HostChatActionSubscribeItem, HostChatCreateRoomError, HostChatCreateRoomRequest,
    HostChatCreateRoomResponse, HostChatListSubscribeItem, HostChatPostMessageError,
    HostChatPostMessageRequest, HostChatPostMessageResponse, HostChatRegisterBotError,
    HostChatRegisterBotRequest, HostChatRegisterBotResponse,
    ProductChatCustomMessageRenderSubscribeItem, ProductChatCustomMessageRenderSubscribeRequest,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Chat room, bot, and message APIs.
pub trait Chat: Send + Sync {
    /// Create a chat room.
    ///
    /// ```ts
    /// const result = await truapi.chat.createRoom({
    ///   roomId: "test-room",
    ///   name: "Test Room",
    ///   icon: "",
    /// });
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
    /// ```
    #[wire(request_id = 38)]
    async fn create_room(
        &self,
        _cx: &CallContext,
        _request: HostChatCreateRoomRequest,
    ) -> Result<HostChatCreateRoomResponse, CallError<HostChatCreateRoomError>> {
        Err(CallError::unavailable())
    }

    /// Register a chat bot.
    ///
    /// ```ts
    /// const result = await truapi.chat.registerBot({
    ///   botId: "test-bot",
    ///   name: "Test Bot",
    ///   icon: "",
    /// });
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
    /// ```
    #[wire(request_id = 40)]
    async fn register_bot(
        &self,
        _cx: &CallContext,
        _request: HostChatRegisterBotRequest,
    ) -> Result<HostChatRegisterBotResponse, CallError<HostChatRegisterBotError>> {
        Err(CallError::unavailable())
    }

    /// Subscribe to the list of chat rooms.
    ///
    /// ```ts
    /// truapi.chat.listSubscribe().subscribe({
    ///   next: (rooms) => console.log(rooms),
    ///   error: (error) => console.error(error),
    ///   complete: () => console.log("completed"),
    /// });
    /// ```
    #[wire(start_id = 42)]
    async fn list_subscribe(&self, _cx: &CallContext) -> Subscription<HostChatListSubscribeItem> {
        Subscription::empty()
    }

    /// Post a message to a chat room.
    ///
    /// ```ts
    /// const result = await truapi.chat.postMessage({
    ///   roomId: "test-room",
    ///   payload: { tag: "Text", value: { text: "Hello from playground!" } },
    /// });
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
    /// ```
    #[wire(request_id = 46)]
    async fn post_message(
        &self,
        _cx: &CallContext,
        _request: HostChatPostMessageRequest,
    ) -> Result<HostChatPostMessageResponse, CallError<HostChatPostMessageError>> {
        Err(CallError::unavailable())
    }

    /// Subscribe to received chat actions.
    ///
    /// ```ts
    /// truapi.chat.actionSubscribe().subscribe({
    ///   next: (action) => console.log(action),
    ///   error: (error) => console.error(error),
    ///   complete: () => console.log("completed"),
    /// });
    /// ```
    #[wire(start_id = 48)]
    async fn action_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<HostChatActionSubscribeItem> {
        Subscription::empty()
    }

    /// Subscribe to custom message render requests from the host. Each
    /// emitted item is a [`CustomRendererNode`](crate::v01::CustomRendererNode)
    /// tree describing the rendered UI.
    ///
    /// ```ts
    /// truapi.chat
    ///   .customMessageRenderSubscribe({
    ///     request: {
    ///       messageId: "msg-1",
    ///       messageType: "custom-render-demo",
    ///       payload: "0x",
    ///     },
    ///   })
    ///   .subscribe({
    ///     next: (node) => console.log(node),
    ///     error: (error) => console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
    /// ```
    #[wire(start_id = 52)]
    async fn custom_message_render_subscribe(
        &self,
        _cx: &CallContext,
        _request: ProductChatCustomMessageRenderSubscribeRequest,
    ) -> Subscription<ProductChatCustomMessageRenderSubscribeItem> {
        Subscription::empty()
    }
}
