//! Versioned wrappers for [`Chat`](super::super::v02::Chat) methods.

use parity_scale_codec::{Decode, Encode};

use super::Versioned;
use crate::v02::{
    ChatBotRegistrationResult, ChatBotRequest, ChatPostMessageRequest, ChatPostMessageResult,
    ChatRoom, ChatRoomRegistrationResult, ChatRoomRequest, CustomMessageRenderRequest,
    ReceivedChatAction, SimpleGroupChatRequest, SimpleGroupChatResult,
};

/// Request wrapper for `host_chat_create_room`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostChatCreateRoomRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ChatRoomRequest),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ChatRoomRequest),
}

impl Versioned for HostChatCreateRoomRequest {
    type Inner = ChatRoomRequest;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Response wrapper for `host_chat_create_room`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostChatCreateRoomResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ChatRoomRegistrationResult),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ChatRoomRegistrationResult),
}

impl Versioned for HostChatCreateRoomResponse {
    type Inner = ChatRoomRegistrationResult;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Request wrapper for `host_chat_create_simple_group` (V0.2+).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostChatCreateSimpleGroupRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(SimpleGroupChatRequest),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(SimpleGroupChatRequest),
}

impl Versioned for HostChatCreateSimpleGroupRequest {
    type Inner = SimpleGroupChatRequest;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Response wrapper for `host_chat_create_simple_group` (V0.2+).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostChatCreateSimpleGroupResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(SimpleGroupChatResult),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(SimpleGroupChatResult),
}

impl Versioned for HostChatCreateSimpleGroupResponse {
    type Inner = SimpleGroupChatResult;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Request wrapper for `host_chat_register_bot`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostChatRegisterBotRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ChatBotRequest),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ChatBotRequest),
}

impl Versioned for HostChatRegisterBotRequest {
    type Inner = ChatBotRequest;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Response wrapper for `host_chat_register_bot`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostChatRegisterBotResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ChatBotRegistrationResult),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ChatBotRegistrationResult),
}

impl Versioned for HostChatRegisterBotResponse {
    type Inner = ChatBotRegistrationResult;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Request wrapper for `host_chat_post_message`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostChatPostMessageRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ChatPostMessageRequest),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ChatPostMessageRequest),
}

impl Versioned for HostChatPostMessageRequest {
    type Inner = ChatPostMessageRequest;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Response wrapper for `host_chat_post_message`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostChatPostMessageResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ChatPostMessageResult),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ChatPostMessageResult),
}

impl Versioned for HostChatPostMessageResponse {
    type Inner = ChatPostMessageResult;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Subscription item wrapper for `host_chat_list_subscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostChatListItem {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(Vec<ChatRoom>),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(Vec<ChatRoom>),
}

impl Versioned for HostChatListItem {
    type Inner = Vec<ChatRoom>;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Subscription item wrapper for `host_chat_action_subscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostChatActionItem {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ReceivedChatAction),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ReceivedChatAction),
}

impl Versioned for HostChatActionItem {
    type Inner = ReceivedChatAction;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Subscription item wrapper for `product_chat_custom_message_render_subscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum ProductChatCustomMessageRenderItem {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(CustomMessageRenderRequest),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(CustomMessageRenderRequest),
}

impl Versioned for ProductChatCustomMessageRenderItem {
    type Inner = CustomMessageRenderRequest;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}
