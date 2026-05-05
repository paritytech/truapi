//! Unified [`Chat`] trait.

use crate::v02::{
    ChatBotRegistrationError, ChatBotRegistrationResult, ChatBotRegistrationStatus,
    ChatMessagePostingError, ChatPostMessageResult, ChatRoomRegistrationError,
    ChatRoomRegistrationResult, ChatRoomRegistrationStatus, SimpleGroupChatResult,
};
use crate::versioned::chat::{
    HostChatActionItem, HostChatCreateRoomRequest, HostChatCreateRoomResponse,
    HostChatCreateSimpleGroupRequest, HostChatCreateSimpleGroupResponse, HostChatListItem,
    HostChatPostMessageRequest, HostChatPostMessageResponse, HostChatRegisterBotRequest,
    HostChatRegisterBotResponse, ProductChatCustomMessageRenderItem,
};
use crate::wire;
use crate::{CallContext, Subscription};

/// Chat room, bot, and message APIs. Unified counterpart of
/// [`crate::v02::Chat`].
///
/// Every method has a default body that flags the call as unavailable through
/// [`CallContext::fail_unavailable`] and returns a placeholder value. Hosts
/// override only the methods they actually support.
#[async_trait::async_trait]
pub trait Chat: Send + Sync {
    /// Create a chat room.
    #[wire(id = 38)]
    async fn host_chat_create_room(
        &self,
        cx: &CallContext,
        _request: HostChatCreateRoomRequest,
    ) -> Result<HostChatCreateRoomResponse, ChatRoomRegistrationError> {
        cx.fail_unavailable();
        Ok(HostChatCreateRoomResponse::V2(ChatRoomRegistrationResult {
            status: ChatRoomRegistrationStatus::New,
        }))
    }

    /// Create a simple group chat room (V0.2+).
    #[wire(id = 106)]
    async fn host_chat_create_simple_group(
        &self,
        cx: &CallContext,
        _request: HostChatCreateSimpleGroupRequest,
    ) -> Result<HostChatCreateSimpleGroupResponse, ChatRoomRegistrationError> {
        cx.fail_unavailable();
        Ok(HostChatCreateSimpleGroupResponse::V2(
            SimpleGroupChatResult {
                status: ChatRoomRegistrationStatus::New,
                join_link: String::new(),
            },
        ))
    }

    /// Register a chat bot.
    #[wire(id = 40)]
    async fn host_chat_register_bot(
        &self,
        cx: &CallContext,
        _request: HostChatRegisterBotRequest,
    ) -> Result<HostChatRegisterBotResponse, ChatBotRegistrationError> {
        cx.fail_unavailable();
        Ok(HostChatRegisterBotResponse::V2(ChatBotRegistrationResult {
            status: ChatBotRegistrationStatus::New,
        }))
    }

    /// Post a message to a chat room.
    #[wire(id = 46)]
    async fn host_chat_post_message(
        &self,
        cx: &CallContext,
        _request: HostChatPostMessageRequest,
    ) -> Result<HostChatPostMessageResponse, ChatMessagePostingError> {
        cx.fail_unavailable();
        Ok(HostChatPostMessageResponse::V2(ChatPostMessageResult {
            message_id: String::new(),
        }))
    }

    /// Subscribe to the list of chat rooms.
    #[wire(id = 42)]
    async fn host_chat_list_subscribe(&self, cx: &CallContext) -> Subscription<HostChatListItem> {
        cx.fail_unavailable();
        Subscription::empty()
    }

    /// Subscribe to received chat actions.
    #[wire(id = 48)]
    async fn host_chat_action_subscribe(
        &self,
        cx: &CallContext,
    ) -> Subscription<HostChatActionItem> {
        cx.fail_unavailable();
        Subscription::empty()
    }

    /// Subscribe to custom message render requests from the host.
    #[wire(id = 52)]
    async fn product_chat_custom_message_render_subscribe(
        &self,
        cx: &CallContext,
    ) -> Subscription<ProductChatCustomMessageRenderItem> {
        cx.fail_unavailable();
        Subscription::empty()
    }
}
