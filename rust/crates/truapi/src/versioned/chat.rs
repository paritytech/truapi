//! Versioned wrappers for [`Chat`](crate::api::Chat) methods.

use crate::{v01, v02};

versioned_type! {
    /// Request wrapper for `host_chat_create_room`.
    pub enum HostChatCreateRoomRequest { V1 => v01::ChatRoomRequest }
    /// Response wrapper for `host_chat_create_room`.
    pub enum HostChatCreateRoomResponse { V1 => v01::ChatRoomRegistrationResult }
    /// Error wrapper for `host_chat_create_room`.
    pub enum HostChatCreateRoomError { V1 => v01::ChatRoomRegistrationError }
    /// Request wrapper for `host_chat_create_simple_group` (V0.2+ only -- no V0.1 counterpart).
    pub enum HostChatCreateSimpleGroupRequest { V2 => v02::SimpleGroupChatRequest }
    /// Response wrapper for `host_chat_create_simple_group` (V0.2+ only).
    pub enum HostChatCreateSimpleGroupResponse { V2 => v02::SimpleGroupChatResult }
    /// Error wrapper for `host_chat_create_simple_group` (V0.2+ only).
    pub enum HostChatCreateSimpleGroupError { V2 => v01::ChatRoomRegistrationError }
    /// Request wrapper for `host_chat_register_bot`.
    pub enum HostChatRegisterBotRequest { V1 => v01::ChatBotRequest }
    /// Response wrapper for `host_chat_register_bot`.
    pub enum HostChatRegisterBotResponse { V1 => v01::ChatBotRegistrationResult }
    /// Error wrapper for `host_chat_register_bot`.
    pub enum HostChatRegisterBotError { V1 => v01::ChatBotRegistrationError }
    /// Request wrapper for `host_chat_post_message`.
    pub enum HostChatPostMessageRequest { V1 => v01::ChatPostMessageRequest }
    /// Response wrapper for `host_chat_post_message`.
    pub enum HostChatPostMessageResponse { V1 => v01::ChatPostMessageResult }
    /// Error wrapper for `host_chat_post_message`.
    pub enum HostChatPostMessageError { V1 => v01::ChatMessagePostingError }
    /// Subscription item wrapper for `host_chat_list_subscribe`.
    pub enum HostChatListSubscribeItem { V1 => Vec<v01::ChatRoom> }
    /// Subscription item wrapper for `host_chat_action_subscribe`.
    pub enum HostChatActionSubscribeItem { V1 => v01::ReceivedChatAction }
    /// Subscription item wrapper for `product_chat_custom_message_render_subscribe`.
    pub enum ProductChatCustomMessageRenderSubscribeItem { V1 => v01::CustomMessageRenderRequest }
}
