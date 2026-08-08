//! Unified [`Chat`] trait.

use crate::versioned::chat::{
    HostChatActionSubscribeItem, HostChatCreateRoomError, HostChatCreateRoomRequest,
    HostChatCreateRoomResponse, HostChatListSubscribeItem, HostChatPostMessageError,
    HostChatPostMessageRequest, HostChatPostMessageResponse, HostChatRegisterBotError,
    HostChatRegisterBotRequest, HostChatRegisterBotResponse, ProductChatCustomMessageRenderItem,
    ProductChatCustomMessageRenderRequest,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Chat room, bot, and message APIs.
#[crate::service(required_execution = Chat)]
#[crate::async_trait]
pub trait Chat: Send + Sync {
    /// Create a chat room.
    ///
    /// ```ts
    /// const result = await truapi.chat.createRoom({
    ///   roomId: "test-room",
    ///   name: "Test Room",
    ///   icon: "",
    /// });
    /// assert(result.isOk(), "createRoom failed:", result);
    /// console.log("room created:", result.value);
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
    /// assert(result.isOk(), "registerBot failed:", result);
    /// console.log("bot registered:", result.value);
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
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const item = await firstValueFrom(
    ///   from(truapi.chat.listSubscribe()),
    /// );
    /// console.log("room list received:", item);
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
    /// assert(result.isOk(), "postMessage failed:", result);
    /// console.log("message posted:", result.value);
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
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const item = await firstValueFrom(
    ///   from(truapi.chat.actionSubscribe()),
    /// );
    /// console.log("action received:", item);
    /// ```
    #[wire(start_id = 48)]
    async fn action_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<HostChatActionSubscribeItem> {
        Subscription::empty()
    }

    /// Streams renderer trees for one stored custom message.
    ///
    /// ```ts
    /// import { of } from "rxjs";
    /// truapi.chat.onCustomMessageRender(({ messageType, payload }) => {
    ///   return of({ tag: "String", value: { text: `${messageType}: ${payload}` } });
    /// });
    /// ```
    #[wire(host_initiated, start_id = 52)]
    fn custom_message_render(
        &self,
        _cx: &CallContext,
        _request: ProductChatCustomMessageRenderRequest,
    ) -> Subscription<ProductChatCustomMessageRenderItem> {
        Subscription::empty()
    }
}
