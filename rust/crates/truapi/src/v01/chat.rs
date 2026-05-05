use super::Bytes;

/// Request to create a chat room.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChatRoomRequest {
    /// Unique room identifier.
    pub room_id: String,
    /// Room display name.
    pub name: String,
    /// URL or base64 image.
    pub icon: String,
}

/// Whether the room was newly created or already existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum ChatRoomRegistrationStatus {
    New,
    Exists,
}

/// Result of a room registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ChatRoomRegistrationResult {
    /// `New` or `Exists`.
    pub status: ChatRoomRegistrationStatus,
}

/// Chat room registration error.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum ChatRoomRegistrationError {
    /// Not allowed.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}

/// Request to register a chat bot.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChatBotRequest {
    /// Unique bot identifier.
    pub bot_id: String,
    /// Bot display name.
    pub name: String,
    /// URL or base64 image.
    pub icon: String,
}

/// Whether the bot was newly registered or already existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum ChatBotRegistrationStatus {
    New,
    Exists,
}

/// Result of a bot registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ChatBotRegistrationResult {
    /// `New` or `Exists`.
    pub status: ChatBotRegistrationStatus,
}

/// Chat bot registration error.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum ChatBotRegistrationError {
    /// Not allowed.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}

/// How the product participates in a chat room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum ChatRoomParticipation {
    RoomHost,
    Bot,
}

/// A chat room the product participates in.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChatRoom {
    /// Room identifier.
    pub room_id: String,
    /// `RoomHost` or `Bot`.
    pub participating_as: ChatRoomParticipation,
}

/// A clickable action button in a chat message.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChatAction {
    /// Action identifier.
    pub action_id: String,
    /// Button label.
    pub title: String,
}

/// Layout for action buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum ChatActionLayout {
    Column,
    Grid,
}

/// A set of action buttons with optional text.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChatActions {
    /// Optional message text.
    pub text: Option<String>,
    /// List of action buttons.
    pub actions: Vec<ChatAction>,
    /// `Column` or `Grid` layout.
    pub layout: ChatActionLayout,
}

/// A media attachment.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChatMedia {
    /// Media URL.
    pub url: String,
}

/// Rich text message with optional media.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChatRichText {
    /// Optional text content.
    pub text: Option<String>,
    /// Attached media items.
    pub media: Vec<ChatMedia>,
}

/// A file attachment in a chat message.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChatFile {
    /// File download URL.
    pub url: String,
    /// File name.
    pub file_name: String,
    /// MIME type.
    pub mime_type: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Optional caption text.
    pub text: Option<String>,
}

/// A reaction to a chat message.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChatReaction {
    /// Message being reacted to.
    pub message_id: String,
    /// Emoji reaction.
    pub emoji: String,
}

/// A custom message with application-defined type and binary payload.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChatCustomMessage {
    /// Application-defined type key.
    pub message_type: String,
    /// Binary payload.
    pub payload: Bytes,
}

/// Content of a chat message -- one of several types.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum ChatMessageContent {
    /// Plain text message.
    Text(String),
    /// Rich text with media.
    RichText(ChatRichText),
    /// Action button set.
    Actions(ChatActions),
    /// File attachment.
    File(ChatFile),
    /// Emoji reaction.
    Reaction(ChatReaction),
    /// Reaction removal.
    ReactionRemoved(ChatReaction),
    /// Custom message.
    Custom(ChatCustomMessage),
}

/// Request to post a message to a chat room.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChatPostMessageRequest {
    /// Room to post to.
    pub room_id: String,
    /// Message content.
    pub payload: ChatMessageContent,
}

/// Result of posting a message.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChatPostMessageResult {
    /// Assigned message ID.
    pub message_id: String,
}

/// Chat message posting error.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum ChatMessagePostingError {
    /// Message exceeded size limit.
    MessageTooLarge,
    /// Catch-all.
    Unknown { reason: String },
}

/// Payload when a user clicks an action button.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ActionTrigger {
    /// Message containing the action.
    pub message_id: String,
    /// Which action was triggered.
    pub action_id: String,
    /// Optional additional data.
    pub payload: Option<Bytes>,
}

/// A slash command from a chat user.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChatCommand {
    /// Command name.
    pub command: String,
    /// Command arguments.
    pub payload: String,
}

/// Payload of a received chat action.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum ChatActionPayload {
    /// A peer posted a message.
    MessagePosted(ChatMessageContent),
    /// A user triggered an action button.
    ActionTriggered(ActionTrigger),
    /// A user issued a command.
    Command(ChatCommand),
}

/// A chat action received from the host.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ReceivedChatAction {
    /// Room where the action occurred.
    pub room_id: String,
    /// Peer who initiated the action.
    pub peer: String,
    /// The action payload.
    pub payload: ChatActionPayload,
}
