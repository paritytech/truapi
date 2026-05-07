use parity_scale_codec::{Decode, Encode};

use crate::v01::ChatRoomRegistrationStatus;

/// Request to create a simple group chat room.
///
/// V0.2: lightweight group chat that avoids the full Chat Extension v2
/// complexity. Participants join via deep link; the host handles the UI
/// with default rendering (no custom elements).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatCreateSimpleGroupRequest {
    /// Unique room identifier source.
    pub room_id: String,
    /// Room display name.
    pub name: String,
    /// URL or base64 image for the room avatar.
    pub icon: String,
}

/// Result of creating a simple group chat room.
///
/// V0.2.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostChatCreateSimpleGroupResponse {
    /// Whether the room was newly created or already existed.
    pub status: ChatRoomRegistrationStatus,
    /// Deep link that participants can use to join the room.
    pub join_link: String,
}
