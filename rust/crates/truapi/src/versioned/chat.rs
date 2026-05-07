//! Versioned wrappers for [`Chat`](crate::api::Chat) methods.

use crate::{v01, v02};

versioned_type! {
    /// Versioned wrapper for [`v01::HostChatCreateRoomRequest`] and older versions.
    pub enum HostChatCreateRoomRequest { V1 => v01::HostChatCreateRoomRequest }
    /// Versioned wrapper for [`v01::HostChatCreateRoomResponse`] and older versions.
    pub enum HostChatCreateRoomResponse { V1 => v01::HostChatCreateRoomResponse }
    /// Versioned wrapper for [`v01::HostChatCreateRoomError`] and older versions.
    pub enum HostChatCreateRoomError { V1 => v01::HostChatCreateRoomError }
    /// Versioned wrapper for [`v02::HostChatCreateSimpleGroupRequest`] and older versions.
    pub enum HostChatCreateSimpleGroupRequest { V2 => v02::HostChatCreateSimpleGroupRequest }
    /// Versioned wrapper for [`v02::HostChatCreateSimpleGroupResponse`] and older versions.
    pub enum HostChatCreateSimpleGroupResponse { V2 => v02::HostChatCreateSimpleGroupResponse }
    /// Versioned wrapper for [`v02::HostChatCreateSimpleGroupError`] and older versions.
    pub enum HostChatCreateSimpleGroupError { V2 => v02::HostChatCreateSimpleGroupError }
    /// Versioned wrapper for [`v01::HostChatRegisterBotRequest`] and older versions.
    pub enum HostChatRegisterBotRequest { V1 => v01::HostChatRegisterBotRequest }
    /// Versioned wrapper for [`v01::HostChatRegisterBotResponse`] and older versions.
    pub enum HostChatRegisterBotResponse { V1 => v01::HostChatRegisterBotResponse }
    /// Versioned wrapper for [`v01::HostChatRegisterBotError`] and older versions.
    pub enum HostChatRegisterBotError { V1 => v01::HostChatRegisterBotError }
    /// Versioned wrapper for [`v01::HostChatPostMessageRequest`] and older versions.
    pub enum HostChatPostMessageRequest { V1 => v01::HostChatPostMessageRequest }
    /// Versioned wrapper for [`v01::HostChatPostMessageResponse`] and older versions.
    pub enum HostChatPostMessageResponse { V1 => v01::HostChatPostMessageResponse }
    /// Versioned wrapper for [`v01::HostChatPostMessageError`] and older versions.
    pub enum HostChatPostMessageError { V1 => v01::HostChatPostMessageError }
    /// Versioned wrapper for [`v01::HostChatListSubscribeItem`] and older versions.
    pub enum HostChatListSubscribeItem { V1 => v01::HostChatListSubscribeItem }
    /// Versioned wrapper for [`v01::HostChatActionSubscribeItem`] and older versions.
    pub enum HostChatActionSubscribeItem { V1 => v01::HostChatActionSubscribeItem }
    /// Versioned wrapper for [`v01::ProductChatCustomMessageRenderSubscribeItem`] and older versions.
    pub enum ProductChatCustomMessageRenderSubscribeItem { V1 => v01::ProductChatCustomMessageRenderSubscribeItem }
}
