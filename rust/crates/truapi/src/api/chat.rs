//! Unified [`Chat`] trait.

use crate::versioned::chat::{
    HostChatActionSubscribeItem, HostChatCreateRoomError, HostChatCreateRoomRequest,
    HostChatCreateRoomResponse, HostChatCreateSimpleGroupError, HostChatCreateSimpleGroupRequest,
    HostChatCreateSimpleGroupResponse, HostChatListSubscribeItem, HostChatPostMessageError,
    HostChatPostMessageRequest, HostChatPostMessageResponse, HostChatRegisterBotError,
    HostChatRegisterBotRequest, HostChatRegisterBotResponse, ProductChatCustomMessageRenderSubscribeItem,
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
    #[wire(id = 38)]
    async fn host_chat_create_room(
        &self,
        _cx: &CallContext,
        _request: HostChatCreateRoomRequest,
    ) -> Result<HostChatCreateRoomResponse, CallError<HostChatCreateRoomError>> {
        Err(CallError::unavailable())
    }

    /// Register a chat bot.
    #[wire(id = 40)]
    async fn host_chat_register_bot(
        &self,
        _cx: &CallContext,
        _request: HostChatRegisterBotRequest,
    ) -> Result<HostChatRegisterBotResponse, CallError<HostChatRegisterBotError>> {
        Err(CallError::unavailable())
    }

    /// Subscribe to the list of chat rooms.
    #[wire(id = 42)]
    async fn host_chat_list_subscribe(&self, _cx: &CallContext) -> Subscription<HostChatListSubscribeItem> {
        Subscription::empty()
    }

    /// Post a message to a chat room.
    #[wire(id = 46)]
    async fn host_chat_post_message(
        &self,
        _cx: &CallContext,
        _request: HostChatPostMessageRequest,
    ) -> Result<HostChatPostMessageResponse, CallError<HostChatPostMessageError>> {
        Err(CallError::unavailable())
    }

    /// Subscribe to received chat actions.
    #[wire(id = 48)]
    async fn host_chat_action_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<HostChatActionSubscribeItem> {
        Subscription::empty()
    }

    /// Subscribe to custom message render requests from the host.
    #[wire(id = 52)]
    async fn product_chat_custom_message_render_subscribe(
        &self,
        _cx: &CallContext,
    ) -> Subscription<ProductChatCustomMessageRenderSubscribeItem> {
        Subscription::empty()
    }

    /// Create a simple group chat room (V0.2+).
    #[wire(id = 106)]
    async fn host_chat_create_simple_group(
        &self,
        _cx: &CallContext,
        _request: HostChatCreateSimpleGroupRequest,
    ) -> Result<HostChatCreateSimpleGroupResponse, CallError<HostChatCreateSimpleGroupError>> {
        Err(CallError::unavailable())
    }
}
