//! Unified [`Chat`] trait.

use crate::versioned::chat::{
    HostChatActionSubscribeItem, HostChatCreateRoomError, HostChatCreateRoomRequest,
    HostChatCreateRoomResponse, HostChatCreateSimpleGroupError, HostChatCreateSimpleGroupRequest,
    HostChatCreateSimpleGroupResponse, HostChatListSubscribeItem, HostChatPostMessageError,
    HostChatPostMessageRequest, HostChatPostMessageResponse, HostChatRegisterBotError,
    HostChatRegisterBotRequest, HostChatRegisterBotResponse,
    ProductChatCustomMessageRenderSubscribeItem, ProductChatCustomMessageRenderSubscribeRequest,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Chat room, bot, and message APIs.
///
/// Default methods return [`CallError::HostFailure`] with an `unavailable`
/// reason. Hosts override only the methods they actually support.
#[async_trait::async_trait]
pub trait Chat: Send + Sync {
    /// Create a chat room.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostChatCreateRoomResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function createRoom(
    ///   truapi: Client,
    /// ): Promise<HostChatCreateRoomResponse> {
    ///   const result = await truapi.chat.chatCreateRoom({
    ///     roomId: "test-room",
    ///     name: "Test Room",
    ///     icon: "",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 38)]
    async fn host_chat_create_room(
        &self,
        _cx: &CallContext,
        _request: HostChatCreateRoomRequest,
    ) -> Result<HostChatCreateRoomResponse, CallError<HostChatCreateRoomError>> {
        Err(CallError::unavailable())
    }

    /// Register a chat bot.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostChatRegisterBotResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function registerBot(
    ///   truapi: Client,
    /// ): Promise<HostChatRegisterBotResponse> {
    ///   const result = await truapi.chat.chatRegisterBot({
    ///     botId: "test-bot",
    ///     name: "Test Bot",
    ///     icon: "",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 40)]
    async fn host_chat_register_bot(
        &self,
        _cx: &CallContext,
        _request: HostChatRegisterBotRequest,
    ) -> Result<HostChatRegisterBotResponse, CallError<HostChatRegisterBotError>> {
        Err(CallError::unavailable())
    }

    /// Subscribe to the list of chat rooms.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type Subscription,
    ///   type HostChatListSubscribeItem,
    /// } from "@parity/truapi";
    ///
    /// export function watchChatRooms(truapi: Client): Subscription {
    ///   return truapi.chat.chatListSubscribe().subscribe({
    ///     next: (rooms: HostChatListSubscribeItem) => console.log(rooms),
    ///     error: (error: Error) => console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
    /// }
    /// ```
    #[wire(start_id = 42)]
    async fn host_chat_list_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<HostChatListSubscribeItem> {
        Subscription::empty()
    }

    /// Post a message to a chat room.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostChatPostMessageResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function postChatMessage(
    ///   truapi: Client,
    /// ): Promise<HostChatPostMessageResponse> {
    ///   const result = await truapi.chat.chatPostMessage({
    ///     roomId: "test-room",
    ///     payload: { tag: "Text", value: { text: "Hello from playground!" } },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 46)]
    async fn host_chat_post_message(
        &self,
        _cx: &CallContext,
        _request: HostChatPostMessageRequest,
    ) -> Result<HostChatPostMessageResponse, CallError<HostChatPostMessageError>> {
        Err(CallError::unavailable())
    }

    /// Subscribe to received chat actions.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type Subscription,
    ///   type HostChatActionSubscribeItem,
    /// } from "@parity/truapi";
    ///
    /// export function watchChatActions(truapi: Client): Subscription {
    ///   return truapi.chat.chatActionSubscribe().subscribe({
    ///     next: (action: HostChatActionSubscribeItem) =>
    ///       console.log(action),
    ///     error: (error: Error) => console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
    /// }
    /// ```
    #[wire(start_id = 48)]
    async fn host_chat_action_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<HostChatActionSubscribeItem> {
        Subscription::empty()
    }

    /// Subscribe to custom message render requests from the host. Each
    /// emitted item is a [`CustomRendererNode`](crate::v01::CustomRendererNode)
    /// tree describing the rendered UI.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type CustomRendererNode,
    ///   type Subscription,
    /// } from "@parity/truapi";
    ///
    /// export function renderCustomChatMessage(truapi: Client): Subscription {
    ///   return truapi.chat
    ///     .chatCustomMessageRenderSubscribe({
    ///       request: {
    ///         messageId: "msg-1",
    ///         messageType: "custom-render-demo",
    ///         payload: new Uint8Array(),
    ///       },
    ///     })
    ///     .subscribe({
    ///       next: (node: CustomRendererNode) => console.log(node),
    ///       error: (error: Error) => console.error(error),
    ///       complete: () => console.log("completed"),
    ///     });
    /// }
    /// ```
    #[wire(start_id = 52)]
    async fn product_chat_custom_message_render_subscribe(
        &self,
        _cx: &CallContext,
        _request: ProductChatCustomMessageRenderSubscribeRequest,
    ) -> Subscription<ProductChatCustomMessageRenderSubscribeItem> {
        Subscription::empty()
    }

    /// Create a simple group chat room (V0.2+).
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostChatCreateSimpleGroupResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function createSimpleGroup(
    ///   truapi: Client,
    /// ): Promise<HostChatCreateSimpleGroupResponse> {
    ///   const result = await truapi.chat.chatCreateSimpleGroup({
    ///     roomId: "test-simple-group",
    ///     name: "Test Group",
    ///     icon: "",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 130)]
    async fn host_chat_create_simple_group(
        &self,
        _cx: &CallContext,
        _request: HostChatCreateSimpleGroupRequest,
    ) -> Result<HostChatCreateSimpleGroupResponse, CallError<HostChatCreateSimpleGroupError>> {
        Err(CallError::unavailable())
    }
}
