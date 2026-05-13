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

/// Chat and custom-renderer methods.
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
    ///   const result = await truapi.chat.createRoom({
    ///     roomId: "test-room",
    ///     name: "Test Room",
    ///     icon: "",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 118)]
    async fn create_room(
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
    ///   const result = await truapi.chat.registerBot({
    ///     botId: "test-bot",
    ///     name: "Test Bot",
    ///     icon: "",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 120)]
    async fn register_bot(
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
    ///   return truapi.chat.listSubscribe().subscribe({
    ///     next: (rooms: HostChatListSubscribeItem) => console.log(rooms),
    ///     error: (error: Error) => console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
    /// }
    /// ```
    #[wire(start_id = 122)]
    async fn list_subscribe(&self, _cx: &CallContext) -> Subscription<HostChatListSubscribeItem> {
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
    ///   const result = await truapi.chat.postMessage({
    ///     roomId: "test-room",
    ///     payload: { tag: "Text", value: { text: "Hello from playground!" } },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 126)]
    async fn post_message(
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
    ///   return truapi.chat.actionSubscribe().subscribe({
    ///     next: (action: HostChatActionSubscribeItem) =>
    ///       console.log(action),
    ///     error: (error: Error) => console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
    /// }
    /// ```
    #[wire(start_id = 132)]
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
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type CustomRendererNode,
    ///   type Subscription,
    /// } from "@parity/truapi";
    ///
    /// export function renderCustomChatMessage(truapi: Client): Subscription {
    ///   return truapi.chat
    ///     .customMessageRenderSubscribe({
    ///       request: {
    ///         messageId: "msg-1",
    ///         messageType: "custom-render-demo",
    ///         payload: "0x",
    ///       },
    ///     })
    ///     .subscribe({
    ///       next: (node: CustomRendererNode) => console.log(node),
    ///       error: (error: Error) => console.error(error),
    ///       complete: () => console.log("completed"),
    ///     });
    /// }
    /// ```
    #[wire(start_id = 138)]
    async fn custom_message_render_subscribe(
        &self,
        _cx: &CallContext,
        _request: ProductChatCustomMessageRenderSubscribeRequest,
    ) -> Subscription<ProductChatCustomMessageRenderSubscribeItem> {
        Subscription::empty()
    }
}
