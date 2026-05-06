//! Unified [`Chat`] trait.

use crate::v01::{
    ChatBotRegistrationResult, ChatBotRegistrationStatus, ChatPostMessageResult,
    ChatRoomRegistrationResult, ChatRoomRegistrationStatus,
};
use crate::v02::SimpleGroupChatResult;
use crate::versioned::chat::{
    HostChatActionItem, HostChatCreateRoomError, HostChatCreateRoomRequest,
    HostChatCreateRoomResponse, HostChatCreateSimpleGroupError, HostChatCreateSimpleGroupRequest,
    HostChatCreateSimpleGroupResponse, HostChatListItem, HostChatPostMessageError,
    HostChatPostMessageRequest, HostChatPostMessageResponse, HostChatRegisterBotError,
    HostChatRegisterBotRequest, HostChatRegisterBotResponse, ProductChatCustomMessageRenderItem,
};
use crate::wire;
use crate::{CallContext, Subscription};

/// Chat room, bot, and message APIs.
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
    ) -> Result<HostChatCreateRoomResponse, HostChatCreateRoomError> {
        cx.fail_unavailable();
        Ok(HostChatCreateRoomResponse::V1(ChatRoomRegistrationResult {
            status: ChatRoomRegistrationStatus::New,
        }))
    }

    /// Create a simple group chat room (V0.2+).
    #[wire(id = 106)]
    async fn host_chat_create_simple_group(
        &self,
        cx: &CallContext,
        _request: HostChatCreateSimpleGroupRequest,
    ) -> Result<HostChatCreateSimpleGroupResponse, HostChatCreateSimpleGroupError> {
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
    ) -> Result<HostChatRegisterBotResponse, HostChatRegisterBotError> {
        cx.fail_unavailable();
        Ok(HostChatRegisterBotResponse::V1(ChatBotRegistrationResult {
            status: ChatBotRegistrationStatus::New,
        }))
    }

    /// Post a message to a chat room.
    #[wire(id = 46)]
    async fn host_chat_post_message(
        &self,
        cx: &CallContext,
        _request: HostChatPostMessageRequest,
    ) -> Result<HostChatPostMessageResponse, HostChatPostMessageError> {
        cx.fail_unavailable();
        Ok(HostChatPostMessageResponse::V1(ChatPostMessageResult {
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
