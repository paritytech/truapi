//! TrUAPI Protocol v0.2 -- trait and type definitions.
//!
//! This module defines the [`TrUApi`] trait containing all TrUAPI v0.2 methods, along with
//! every data type used in their signatures. The three communication patterns are:
//!
//! - **Request-response**: product calls host, host returns a result.
//! - **Subscription**: product subscribes, host pushes values via callback.
//! - **Reverse-subscription**: host initiates, product responds (only used for custom chat
//!   message rendering).

use crate::Subscription;

// ─── Primitive type aliases ──────────────────────────────────────────────────

/// Hex-encoded arbitrary bytes (SCALE length-prefixed on the wire).
pub type Hex = Vec<u8>;

/// Arbitrary binary data (SCALE length-prefixed on the wire).
pub type Bytes = Vec<u8>;

// ─── Common types ────────────────────────────────────────────────────────────

/// Blockchain genesis hash, used to identify a specific chain.
pub type GenesisHash = Hex;

/// Generic error payload carrying a human-readable reason string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericErr {
    pub reason: String,
}

/// Single-variant error enum wrapping [`GenericErr`]. Used by many methods as a
/// catch-all error type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenericError {
    GenericError(GenericErr),
}

// ─── Feature types ───────────────────────────────────────────────────────────

/// Feature to check for host support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Feature {
    /// Is this blockchain supported?
    Chain(GenesisHash),
}

// ─── Navigation types ────────────────────────────────────────────────────────

/// Navigation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigateToError {
    /// Navigation not allowed.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}

// ─── Notification types ──────────────────────────────────────────────────────

/// Push notification payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushNotification {
    /// Notification text.
    pub text: String,
    /// Optional URL to open on tap.
    pub deeplink: Option<String>,
}

// ─── Permission types ────────────────────────────────────────────────────────

/// Device capability to request access to.
///
/// V0.2: extended with `Notifications`, `Nfc`, `Clipboard`, `OpenUrl`, and
/// `Biometrics` per [RFC 0001] (JIT permissions).
///
/// [RFC 0001]: https://github.com/paritytech/triangle-js-sdks/pull/66
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevicePermission {
    /// Push notification delivery permission.
    Notifications,
    Camera,
    Microphone,
    Bluetooth,
    /// Near-field communication access.
    Nfc,
    Location,
    /// System clipboard access.
    Clipboard,
    /// Open a URL in an external browser.
    OpenUrl,
    /// Biometric authentication (fingerprint, face ID).
    Biometrics,
}

/// A single remote-operation permission entry.
///
/// V0.2: replaces `RemotePermissionRequest`. The [`Permissions::remote_permission`] method
/// now accepts a `Vec<RemotePermission>` so products can batch multiple
/// permission requests into a single prompt.
///
/// See [RFC 0001] and [issue #64].
///
/// [RFC 0001]: https://github.com/paritytech/triangle-js-sdks/pull/66
/// [issue #64]: https://github.com/paritytech/triangle-js-sdks/issues/64
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemotePermission {
    /// HTTP/HTTPS/WS/WSS access to specific domains. Each string is a domain
    /// pattern: `"api.example.com"` (exact), `"*.example.com"` (wildcard
    /// subdomain), or `"*"` (all hosts).
    Remote(Vec<String>),
    /// WebRTC access — can expose the user's IP address.
    WebRtc,
    /// Broadcast signed transactions via
    /// [`ChainInteraction::remote_chain_transaction_broadcast`].
    ChainSubmit,
    /// Submit statements via [`StatementStore::remote_statement_store_submit`].
    StatementSubmit,
}

// ─── Storage types ───────────────────────────────────────────────────────────

/// Key name for local storage operations.
pub type StorageKey = String;

/// Binary value stored in local storage.
pub type StorageValue = Vec<u8>;

/// Local storage operation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    /// Storage quota exceeded.
    Full,
    /// Catch-all.
    Unknown { reason: String },
}

// ─── Account types ───────────────────────────────────────────────────────────

/// 32-byte account identifier (typically an SS58 public key).
pub type AccountId = [u8; 32];

/// Variable-length public key.
pub type PublicKey = Vec<u8>;

/// A dotNS domain name identifier (e.g., `"my-product.dot"`).
pub type DotNsIdentifier = String;

/// Key derivation index for generating product-specific accounts.
pub type DerivationIndex = u32;

/// Identifies a product-specific account by combining a dotNS domain name with a
/// derivation index.
pub type ProductAccountId = (DotNsIdentifier, DerivationIndex);

/// An account with its public key and optional display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The account public key (variable-length bytes).
    pub public_key: PublicKey,
    /// Optional human-readable display name.
    pub name: Option<String>,
}

/// A privacy-preserving alias derived via ring VRF, bound to a specific context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextualAlias {
    /// 32-byte context identifier.
    pub context: [u8; 32],
    /// Ring VRF alias (variable length).
    pub alias: Vec<u8>,
}

/// Hints for locating a ring on-chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RingLocationHint {
    /// Optional pallet instance index.
    pub pallet_instance: Option<u32>,
}

/// Locates a specific ring on a specific chain for ring VRF operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RingLocation {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Root hash of the ring.
    pub ring_root_hash: Hex,
    /// Optional location hints.
    pub hints: Option<RingLocationHint>,
}

/// Variable-length ring VRF proof bytes.
pub type RingVrfProof = Vec<u8>;

/// User's authentication state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountConnectionStatus {
    Disconnected,
    Connected,
}

/// The user's primary DotNS account identity.
///
/// V0.2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserIdentity {
    /// The user's primary DotNS identifier.
    pub dot_ns_identifier: DotNsIdentifier,
    /// The user's primary public key.
    pub public_key: PublicKey,
}

/// Error from [`AccountManagement::host_get_user_id`].
///
/// V0.2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserIdentityError {
    /// User denied the identity disclosure request.
    Rejected,
    /// User is not logged in.
    NotConnected,
    /// Catch-all.
    Unknown { reason: String },
}

/// Error returned when credential/account requests fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestCredentialsError {
    /// User is not logged in.
    NotConnected,
    /// User or host rejected the request.
    Rejected,
    /// Domain identifier is invalid.
    DomainNotValid,
    /// Catch-all error with reason.
    Unknown { reason: String },
}

/// Error returned when ring VRF proof creation fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateProofError {
    /// Ring not available at the specified location.
    RingNotFound,
    /// User or host rejected.
    Rejected,
    /// Catch-all.
    Unknown { reason: String },
}

// ─── Signing types ───────────────────────────────────────────────────────────

/// Full Substrate extrinsic signing payload with all fields needed for signature
/// generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningPayload {
    /// Product account that will sign this payload.
    ///
    /// V0.2: replaces the previous `address: String` field per [RFC 0005],
    /// aligning with all other TrUAPI account-related methods.
    ///
    /// [RFC 0005]: https://github.com/paritytech/triangle-js-sdks/pull/82
    pub account: ProductAccountId,
    /// Reference block hash.
    pub block_hash: Hex,
    /// Reference block number.
    pub block_number: Hex,
    /// Mortality era encoding.
    pub era: Hex,
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// SCALE-encoded call data.
    pub method: Hex,
    /// Account nonce.
    pub nonce: Hex,
    /// Runtime spec version.
    pub spec_version: Hex,
    /// Transaction tip.
    pub tip: Hex,
    /// Transaction format version.
    pub transaction_version: Hex,
    /// Extension identifiers.
    pub signed_extensions: Vec<String>,
    /// Extrinsic version.
    pub version: u32,
    /// For multi-asset tips.
    pub asset_id: Option<Hex>,
    /// CheckMetadataHash extension.
    pub metadata_hash: Option<Hex>,
    /// Metadata mode.
    pub mode: Option<u32>,
    /// Request signed transaction back.
    pub with_signed_transaction: Option<bool>,
}

/// Raw data to sign — either binary bytes or a string message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawPayload {
    /// Raw binary data to sign.
    Bytes(Vec<u8>),
    /// String message to sign.
    Payload(String),
}

/// A raw signing request pairing an account with raw data.
///
/// V0.2: `address` replaced with `account: ProductAccountId` per [RFC 0005].
///
/// [RFC 0005]: https://github.com/paritytech/triangle-js-sdks/pull/82
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningRawPayload {
    /// Product account that will sign this data.
    pub account: ProductAccountId,
    /// The data to sign.
    pub data: RawPayload,
}

/// Result of a signing operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningResult {
    /// The cryptographic signature.
    pub signature: Hex,
    /// Full signed transaction, if requested.
    pub signed_transaction: Option<Hex>,
}

/// Signing operation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SigningError {
    /// Payload could not be deserialized.
    FailedToDecode,
    /// User rejected signing.
    Rejected,
    /// Not authenticated.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}

// ─── Transaction creation types ──────────────────────────────────────────────

/// A signed extension for a transaction payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxPayloadExtensionV1 {
    /// Extension name (e.g., `"CheckSpecVersion"`).
    pub id: String,
    /// SCALE-encoded extra data (in extrinsic body).
    pub extra: Hex,
    /// SCALE-encoded implicit data (signed, not in body).
    pub additional_signed: Hex,
}

/// Context information for transaction construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxPayloadContextV1 {
    /// `RuntimeMetadataPrefixed` blob (SCALE).
    pub metadata: Hex,
    /// Native token symbol.
    pub token_symbol: String,
    /// Native token decimals.
    pub token_decimals: u32,
    /// Highest known block number.
    pub best_block_height: u32,
}

/// Version 1 transaction payload with all data needed to construct a signed
/// extrinsic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxPayloadV1 {
    /// Signer hint (address/name), `None` = host picks.
    pub signer: Option<String>,
    /// SCALE-encoded Call data.
    pub call_data: Hex,
    /// Signed extensions.
    pub extensions: Vec<TxPayloadExtensionV1>,
    /// 0 for Extrinsic V4, any for V5.
    pub tx_ext_version: u8,
    /// Transaction context.
    pub context: TxPayloadContextV1,
}

/// Versioned transaction payload envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionedTxPayload {
    /// Version 1 payload.
    V1(TxPayloadV1),
}

/// Transaction creation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateTransactionError {
    /// Payload could not be deserialized.
    FailedToDecode,
    /// User rejected.
    Rejected,
    /// Unsupported payload version or extension.
    NotSupported(String),
    /// Not authenticated.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}

// ─── Chat types ──────────────────────────────────────────────────────────────

/// Request to create a chat room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatRoomRequest {
    /// Unique room identifier.
    pub room_id: String,
    /// Room display name.
    pub name: String,
    /// URL or base64 image.
    pub icon: String,
}

/// Whether the room was newly created or already existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRoomRegistrationStatus {
    New,
    Exists,
}

/// Result of a room registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatRoomRegistrationResult {
    /// `New` or `Exists`.
    pub status: ChatRoomRegistrationStatus,
}

/// Chat room registration error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatRoomRegistrationError {
    /// Not allowed.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}

/// Request to create a simple group chat room.
///
/// V0.2: lightweight group chat that avoids the full Chat Extension v2
/// complexity. Participants join via deep link; the host handles the UI
/// with default rendering (no custom elements).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleGroupChatRequest {
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleGroupChatResult {
    /// Whether the room was newly created or already existed.
    pub status: ChatRoomRegistrationStatus,
    /// Deep link that participants can use to join the room.
    pub join_link: String,
}

/// Request to register a chat bot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatBotRequest {
    /// Unique bot identifier.
    pub bot_id: String,
    /// Bot display name.
    pub name: String,
    /// URL or base64 image.
    pub icon: String,
}

/// Whether the bot was newly registered or already existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatBotRegistrationStatus {
    New,
    Exists,
}

/// Result of a bot registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatBotRegistrationResult {
    /// `New` or `Exists`.
    pub status: ChatBotRegistrationStatus,
}

/// Chat bot registration error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatBotRegistrationError {
    /// Not allowed.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}

/// How the product participates in a chat room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRoomParticipation {
    RoomHost,
    Bot,
}

/// A chat room the product participates in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatRoom {
    /// Room identifier.
    pub room_id: String,
    /// `RoomHost` or `Bot`.
    pub participating_as: ChatRoomParticipation,
}

/// A clickable action button in a chat message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatAction {
    /// Action identifier.
    pub action_id: String,
    /// Button label.
    pub title: String,
}

/// Layout for action buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatActionLayout {
    Column,
    Grid,
}

/// A set of action buttons with optional text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatActions {
    /// Optional message text.
    pub text: Option<String>,
    /// List of action buttons.
    pub actions: Vec<ChatAction>,
    /// `Column` or `Grid` layout.
    pub layout: ChatActionLayout,
}

/// A media attachment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMedia {
    /// Media URL.
    pub url: String,
}

/// Rich text message with optional media.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatRichText {
    /// Optional text content.
    pub text: Option<String>,
    /// Attached media items.
    pub media: Vec<ChatMedia>,
}

/// A file attachment in a chat message.
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatReaction {
    /// Message being reacted to.
    pub message_id: String,
    /// Emoji reaction.
    pub emoji: String,
}

/// A custom message with application-defined type and binary payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatCustomMessage {
    /// Application-defined type key.
    pub message_type: String,
    /// Binary payload.
    pub payload: Bytes,
}

/// Content of a chat message — one of several types.
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPostMessageRequest {
    /// Room to post to.
    pub room_id: String,
    /// Message content.
    pub payload: ChatMessageContent,
}

/// Result of posting a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPostMessageResult {
    /// Assigned message ID.
    pub message_id: String,
}

/// Chat message posting error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatMessagePostingError {
    /// Message exceeded size limit.
    MessageTooLarge,
    /// Catch-all.
    Unknown { reason: String },
}

/// Payload when a user clicks an action button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionTrigger {
    /// Message containing the action.
    pub message_id: String,
    /// Which action was triggered.
    pub action_id: String,
    /// Optional additional data.
    pub payload: Option<Bytes>,
}

/// A slash command from a chat user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatCommand {
    /// Command name.
    pub command: String,
    /// Command arguments.
    pub payload: String,
}

/// Payload of a received chat action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatActionPayload {
    /// A peer posted a message.
    MessagePosted(ChatMessageContent),
    /// A user triggered an action button.
    ActionTriggered(ActionTrigger),
    /// A user issued a command.
    Command(ChatCommand),
}

/// A chat action received from the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedChatAction {
    /// Room where the action occurred.
    pub room_id: String,
    /// Peer who initiated the action.
    pub peer: String,
    /// The action payload.
    pub payload: ChatActionPayload,
}

// ─── Custom renderer types ───────────────────────────────────────────────────

/// Variable-length unsigned integer used for dimensions (SCALE compact-encoded
/// on the wire).
pub type Size = u64;

/// CSS-like dimensions: (top, end, bottom, start).
/// Bottom defaults to top, start defaults to end when `None`.
pub type Dimensions = (Size, Size, Option<Size>, Option<Size>);

/// Text typography presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypographyStyle {
    TitleXL,
    Headline,
    BodyM,
    BodyS,
    Caption,
}

/// Button style variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Text,
}

/// Semantic color tokens for theming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorToken {
    TextPrimary,
    TextSecondary,
    TextTertiary,
    BackgroundPrimary,
    BackgroundSecondary,
    BackgroundTertiary,
    Success,
    Error,
    Warning,
}

/// 2D content alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentAlignment {
    TopStart,
    TopCenter,
    TopEnd,
    CenterStart,
    Center,
    CenterEnd,
    BottomStart,
    BottomCenter,
    BottomEnd,
}

/// Horizontal alignment options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizontalAlignment {
    Start,
    Center,
    End,
}

/// Vertical alignment options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalAlignment {
    Top,
    Center,
    Bottom,
}

/// Layout arrangement (like CSS flexbox `justify-content`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrangement {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Shape for borders and backgrounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Border radius value.
    Rounded(Size),
    /// Circular shape.
    Circle,
}

/// Border styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BorderStyle {
    /// Border width.
    pub width: Size,
    /// Border color.
    pub color: ColorToken,
    /// Border shape.
    pub shape: Option<Shape>,
}

/// Background styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Background {
    /// Background color.
    pub color: ColorToken,
    /// Background shape.
    pub shape: Option<Shape>,
}

/// Layout and styling modifiers applied to custom renderer components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    /// Outer spacing.
    Margin(Dimensions),
    /// Inner spacing.
    Padding(Dimensions),
    /// Background fill.
    Background(Background),
    /// Border style.
    Border(BorderStyle),
    /// Fixed height.
    Height(Size),
    /// Fixed width.
    Width(Size),
    /// Minimum width.
    MinWidth(Size),
    /// Minimum height.
    MinHeight(Size),
    /// Fill available width.
    FillWidth(bool),
    /// Fill available height.
    FillHeight(bool),
}

/// Properties for a [`CustomRendererNode::Box`] container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxProps {
    /// Content alignment within the box.
    pub content_alignment: Option<ContentAlignment>,
}

/// Properties for a [`CustomRendererNode::Column`] layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnProps {
    /// Horizontal alignment of children.
    pub horizontal_alignment: Option<HorizontalAlignment>,
    /// Vertical arrangement of children.
    pub vertical_arrangement: Option<Arrangement>,
}

/// Properties for a [`CustomRendererNode::Row`] layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowProps {
    /// Vertical alignment of children.
    pub vertical_alignment: Option<VerticalAlignment>,
    /// Horizontal arrangement of children.
    pub horizontal_arrangement: Option<Arrangement>,
}

/// Properties for a [`CustomRendererNode::Text`] display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextProps {
    /// Typography preset.
    pub style: Option<TypographyStyle>,
    /// Text color.
    pub color: Option<ColorToken>,
}

/// Properties for a [`CustomRendererNode::Button`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonProps {
    /// Button label text.
    pub text: String,
    /// Button style variant.
    pub variant: Option<ButtonVariant>,
    /// Action identifier triggered on click.
    pub click_action: String,
}

/// Properties for a [`CustomRendererNode::TextField`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFieldProps {
    /// Placeholder text.
    pub placeholder: Option<String>,
    /// Initial value.
    pub initial_value: Option<String>,
    /// Action identifier triggered on submit.
    pub submit_action: String,
}

/// A component in the custom renderer UI tree, combining modifiers, typed props,
/// and recursive children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component<P> {
    /// Layout and styling modifiers.
    pub modifiers: Vec<Modifier>,
    /// Component-specific properties.
    pub props: P,
    /// Child nodes.
    pub children: Vec<CustomRendererNode>,
}

/// A node in the custom renderer UI tree. Can be nested recursively via the
/// `children` field of each [`Component`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomRendererNode {
    /// Empty node.
    Nil,
    /// Raw text string.
    String(String),
    /// Generic container.
    Box(Component<BoxProps>),
    /// Vertical layout.
    Column(Component<ColumnProps>),
    /// Horizontal layout.
    Row(Component<RowProps>),
    /// Flexible space.
    Spacer(Component<()>),
    /// Text display.
    Text(Component<TextProps>),
    /// Interactive button.
    Button(Component<ButtonProps>),
    /// Text input.
    TextField(Component<TextFieldProps>),
}

/// Request from the host asking the product to render a custom chat message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomMessageRenderRequest {
    /// Message identifier.
    pub message_id: String,
    /// Application-defined message type.
    pub message_type: String,
    /// Binary payload.
    pub payload: Bytes,
}

// ─── Statement store types ───────────────────────────────────────────────────

/// 32-byte topic identifier.
pub type Topic = [u8; 32];

/// 32-byte channel identifier.
pub type Channel = [u8; 32];

/// 32-byte decryption key.
pub type DecryptionKey = [u8; 32];

/// Cryptographic proof for a statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatementProof {
    /// Sr25519 signature proof.
    Sr25519 {
        signature: [u8; 64],
        signer: [u8; 32],
    },
    /// Ed25519 signature proof.
    Ed25519 {
        signature: [u8; 64],
        signer: [u8; 32],
    },
    /// ECDSA signature proof.
    Ecdsa {
        signature: [u8; 65],
        signer: [u8; 33],
    },
    /// On-chain event proof.
    OnChain {
        who: [u8; 32],
        block_hash: [u8; 32],
        event: u64,
    },
}

/// A statement with optional proof and metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// Optional cryptographic proof.
    pub proof: Option<StatementProof>,
    /// Optional decryption key.
    pub decryption_key: Option<DecryptionKey>,
    /// Optional Unix timestamp expiry.
    pub expiry: Option<u64>,
    /// Optional channel.
    pub channel: Option<Channel>,
    /// Topic tags.
    pub topics: Vec<Topic>,
    /// Optional data payload.
    pub data: Option<Bytes>,
}

/// A statement with a required (not optional) proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedStatement {
    /// Required cryptographic proof.
    pub proof: StatementProof,
    /// Optional decryption key.
    pub decryption_key: Option<DecryptionKey>,
    /// Optional Unix timestamp expiry.
    pub expiry: Option<u64>,
    /// Optional channel.
    pub channel: Option<Channel>,
    /// Topic tags.
    pub topics: Vec<Topic>,
    /// Optional data payload.
    pub data: Option<Bytes>,
}

/// Statement proof creation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatementProofError {
    /// Signing operation failed.
    UnableToSign,
    /// Account not recognized.
    UnknownAccount,
    /// Catch-all.
    Unknown { reason: String },
}

// ─── Preimage types ──────────────────────────────────────────────────────────

/// Hash of the preimage.
pub type PreimageKey = Hex;

/// The preimage data.
pub type PreimageValue = Vec<u8>;

// ─── Chain interaction types ─────────────────────────────────────────────────

/// Block hash identifier.
pub type BlockHash = Hex;

/// Operation identifier for async chain operations.
pub type OperationId = String;

/// A runtime API identified by name and version.
pub type RuntimeApi = (String, u32);

/// Runtime specification metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSpec {
    /// Specification name.
    pub spec_name: String,
    /// Implementation name.
    pub impl_name: String,
    /// Spec version number.
    pub spec_version: u32,
    /// Implementation version.
    pub impl_version: u32,
    /// Transaction format version.
    pub transaction_version: Option<u32>,
    /// Supported runtime APIs.
    pub apis: Vec<RuntimeApi>,
}

/// Runtime validity check result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeType {
    /// Valid runtime with spec.
    Valid(RuntimeSpec),
    /// Invalid runtime with error.
    Invalid { error: String },
}

/// Type of storage query to perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageQueryType {
    Value,
    Hash,
    ClosestDescendantMerkleValue,
    DescendantsValues,
    DescendantsHashes,
}

/// A single storage query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageQueryItem {
    /// Storage key to query.
    pub key: Hex,
    /// What to return.
    pub query_type: StorageQueryType,
}

/// Result of a storage query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageResultItem {
    /// The queried key.
    pub key: Hex,
    /// Value, if requested.
    pub value: Option<Hex>,
    /// Hash, if requested.
    pub hash: Option<Hex>,
    /// Merkle value, if requested.
    pub closest_descendant_merkle_value: Option<Hex>,
}

/// Result of starting a chain operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationStartedResult {
    /// Operation started successfully.
    Started {
        /// The assigned operation identifier.
        operation_id: OperationId,
    },
    /// Too many concurrent operations.
    LimitReached,
}

/// Events received when following the chain head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainHeadEvent {
    /// Initial state with finalized blocks.
    Initialized {
        finalized_block_hashes: Vec<BlockHash>,
        finalized_block_runtime: Option<RuntimeType>,
    },
    /// A new block was produced.
    NewBlock {
        block_hash: BlockHash,
        parent_block_hash: BlockHash,
        new_runtime: Option<RuntimeType>,
    },
    /// Best block changed.
    BestBlockChanged { best_block_hash: BlockHash },
    /// Blocks were finalized.
    Finalized {
        finalized_block_hashes: Vec<BlockHash>,
        pruned_block_hashes: Vec<BlockHash>,
    },
    /// Body fetch completed.
    OperationBodyDone {
        operation_id: OperationId,
        value: Vec<Hex>,
    },
    /// Runtime call completed.
    OperationCallDone {
        operation_id: OperationId,
        output: Hex,
    },
    /// Storage results batch.
    OperationStorageItems {
        operation_id: OperationId,
        items: Vec<StorageResultItem>,
    },
    /// Storage query completed.
    OperationStorageDone { operation_id: OperationId },
    /// Operation paused, needs [`ChainInteraction::remote_chain_head_continue`].
    OperationWaitingForContinue { operation_id: OperationId },
    /// Block became inaccessible.
    OperationInaccessible { operation_id: OperationId },
    /// Operation failed.
    OperationError {
        operation_id: OperationId,
        error: String,
    },
    /// Subscription terminated by server.
    Stop,
}

/// Parameters for [`ChainInteraction::remote_chain_head_follow`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainHeadFollowRequest {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Whether to include runtime information in events.
    pub with_runtime: bool,
}

/// Parameters for chain head methods that operate within a follow subscription
/// on a specific block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainHeadBlockRequest {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hash.
    pub hash: BlockHash,
}

/// Parameters for [`ChainInteraction::remote_chain_head_storage`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainHeadStorageRequest {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hash.
    pub hash: BlockHash,
    /// Storage items to query.
    pub items: Vec<StorageQueryItem>,
    /// Optional child trie.
    pub child_trie: Option<Hex>,
}

/// Parameters for [`ChainInteraction::remote_chain_head_call`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainHeadCallRequest {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hash.
    pub hash: BlockHash,
    /// Runtime API function name.
    pub function: String,
    /// SCALE-encoded call parameters.
    pub call_parameters: Hex,
}

/// Parameters for [`ChainInteraction::remote_chain_head_unpin`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainHeadUnpinRequest {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Block hashes to unpin.
    pub hashes: Vec<BlockHash>,
}

/// Parameters for chain head operations that reference a specific operation within
/// a follow subscription.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainHeadOperationRequest {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Follow subscription identifier.
    pub follow_subscription_id: String,
    /// Operation identifier.
    pub operation_id: OperationId,
}

/// Parameters for [`ChainInteraction::remote_chain_transaction_broadcast`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainTransactionBroadcastRequest {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Signed transaction bytes.
    pub transaction: Hex,
}

/// Parameters for [`ChainInteraction::remote_chain_transaction_stop`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainTransactionStopRequest {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Operation identifier of the broadcast to stop.
    pub operation_id: OperationId,
}

// ─── SSS API types (v0.2) ────────────────────────────────────────────────────

/// Filter for statement subscriptions, allowing richer topic matching than plain
/// topic vectors. Each position in the filter can be `Some(topic)` to require an
/// exact match or `None` to act as a wildcard.
///
/// Mirrors the `TopicFilter` type from `polkadot-sdk` statement store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicFilter {
    /// Positional topic matchers. `None` entries act as wildcards.
    pub topics: Vec<Option<Topic>>,
}

// ─── Payment types (v0.2 — Coinage API, RFC 0006) ───────────────────────────
//
// See [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94).

/// Balance amount for payment operations. Interpreted according to the host's
/// single fixed payment asset (e.g. pUSD).
pub type Balance = u128;

/// Unique payment identifier, scoped to the product that created it.
pub type PaymentId = String;

/// Ed25519 private key bytes (32 bytes).
pub type Ed25519PrivateKey = [u8; 32];

/// Current payment balance state pushed to subscribers.
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentBalance {
    /// Balance that can be spent right now.
    pub available: Balance,
    /// Balance the user possesses but cannot spend yet (e.g. in recycling
    /// stage).
    pub pending: Balance,
}

/// Source for a payment top-up operation.
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentTopUpSource {
    /// Fund from one of the calling product's scoped accounts.
    ProductAccount(DerivationIndex),
    /// Fund from a one-time account represented by its private key. This is a
    /// standard account holding public funds — not a coin key.
    PrivateKey(Ed25519PrivateKey),
}

/// Receipt returned after a successful payment request.
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentReceipt {
    /// The assigned payment identifier.
    pub id: PaymentId,
}

/// Payment lifecycle status pushed to subscribers.
///
/// Once a terminal state (`Completed` or `Failed`) is reached, the host
/// delivers it and may close the subscription.
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentStatus {
    /// Payment is being processed.
    Processing,
    /// Payment has been settled successfully.
    Completed,
    /// Payment has failed.
    Failed(String),
}

/// Error from [`Payment::host_payment_balance_subscribe`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentBalanceError {
    /// User denied the balance disclosure request.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}

/// Error from [`Payment::host_payment_top_up`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentTopUpError {
    /// The source account does not hold sufficient funds.
    InsufficientFunds,
    /// The source account was not found or is invalid.
    InvalidSource,
    /// Catch-all.
    Unknown { reason: String },
}

/// Error from [`Payment::host_payment_request`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentRequestError {
    /// User denied the payment request.
    Denied,
    /// User's available balance is not sufficient for the requested amount.
    InsufficientBalance,
    /// Catch-all.
    Unknown { reason: String },
}

/// Error from [`Payment::host_payment_status_subscribe`].
///
/// See [RFC 0006].
///
/// [RFC 0006]: https://github.com/paritytech/triangle-js-sdks/pull/94
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentStatusError {
    /// Payment ID was not found or does not belong to the current product.
    PaymentNotFound,
    /// Catch-all.
    Unknown { reason: String },
}

// ─── Entropy types (v0.2, RFC 0007) ──────────────────────────────────────────
//
// See [RFC 0007](https://github.com/paritytech/triangle-js-sdks/pull/95).

/// 32 bytes of deterministic entropy derived from the user's root BIP-39
/// entropy via a three-layer BLAKE2b-256 keyed hashing scheme. The same
/// root account + product + key always yields the same output on any
/// conforming host.
///
/// See [RFC 0007].
///
/// [RFC 0007]: https://github.com/paritytech/triangle-js-sdks/pull/95
pub type Entropy = [u8; 32];

/// Error from [`EntropyDerivation::host_derive_entropy`].
///
/// Under normal operation the function always succeeds; `Unknown` indicates an
/// unrecoverable internal host error.
///
/// See [RFC 0007].
///
/// [RFC 0007]: https://github.com/paritytech/triangle-js-sdks/pull/95
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeriveEntropyError {
    /// An unexpected error occurred in the host.
    Unknown,
}

// ─── TrUAPI traits ─────────────────────────────────────────────────────────

/// General-purpose TrUAPI methods for feature detection, navigation, and notifications.
pub trait TrUApiCalls {
    /// Queries whether the host supports a specific feature. Currently only the
    /// `Chain` variant exists, carrying a genesis hash to check whether a
    /// specific blockchain is available.
    ///
    /// # Product Function
    ///
    /// `truApi.featureSupported(feature)`
    ///
    /// # Host Handler
    ///
    /// `container.handleFeatureSupported(handler)`
    ///
    /// # Request Description
    ///
    /// Feature enum — Chain(GenesisHash)
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Check if Polkadot is supported
    /// const polkadotGenesis = "0x91b171bb158e2d3848fa23a9f1c25182fb8e20313b2c1eb49219da7a70ce90c3";
    /// const result = await truApi.featureSupported({
    ///   Chain: polkadotGenesis
    /// });
    /// 
    /// if (result.isOk) {
    ///   console.log("Polkadot supported:", result.value);
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleFeatureSupported((feature, { ok, err }) => {
    ///   if (feature.tag === "Chain") {
    ///     const supported = supportedChains.has(feature.value);
    ///     return ok(supported);
    ///   }
    ///   return ok(false);
    /// });
    /// ```
    fn host_feature_supported(&self, feature: Feature) -> Result<bool, GenericError>;

    /// Requests the host to open a URL, typically in a new browser tab.
    ///
    /// # Product Function
    ///
    /// `truApi.navigateTo(url)`
    ///
    /// # Host Handler
    ///
    /// `container.handleNavigateTo(handler)`
    ///
    /// # Request Description
    ///
    /// The URL to open
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Open an external link
    /// const result = await truApi.navigateTo(
    ///   "https://polkadot.network"
    /// );
    /// 
    /// if (result.isErr) {
    ///   console.error("Navigation failed:", result.error);
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleNavigateTo((url, { ok, err }) => {
    ///   try {
    ///     window.open(url, "_blank");
    ///     return ok(undefined);
    ///   } catch (e) {
    ///     return err({ PermissionDenied: undefined });
    ///   }
    /// });
    /// ```
    fn host_navigate_to(&self, url: String) -> Result<(), NavigateToError>;

    /// Sends a push notification to the user via the host.
    ///
    /// # Product Function
    ///
    /// `truApi.pushNotification(notification)`
    ///
    /// # Host Handler
    ///
    /// `container.handlePushNotification(handler)`
    ///
    /// # Request Description
    ///
    /// See PushNotification type for fields
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Send a notification with a deeplink
    /// const result = await truApi.pushNotification({
    ///   text: "Your transaction was confirmed!",
    ///   deeplink: "myapp://tx/0xabc123"
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handlePushNotification((notification, { ok, err }) => {
    ///   showSystemNotification(notification.text, {
    ///     onclick: () => {
    ///       if (notification.deeplink) {
    ///         navigate(notification.deeplink);
    ///       }
    ///     }
    ///   });
    ///   return ok(undefined);
    /// });
    /// ```
    fn host_push_notification(&self, notification: PushNotification) -> Result<(), GenericError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Device and remote permission requests for camera, microphone, HTTP, and transaction access.
pub trait Permissions {
    /// Requests access to a device capability.
    ///
    /// V0.2: extended set of capabilities per
    /// [RFC 0001](https://github.com/paritytech/triangle-js-sdks/pull/66).
    ///
    /// # Product Function
    ///
    /// `truApi.devicePermission(permission)`
    ///
    /// # Host Handler
    ///
    /// `container.handleDevicePermission(handler)`
    ///
    /// # Request Description
    ///
    /// DevicePermission enum: Notifications | Camera | Microphone | Bluetooth | Nfc | Location | Clipboard | OpenUrl | Biometrics
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Request camera access
    /// const granted = await truApi.devicePermission("Camera");
    /// 
    /// if (granted.isOk && granted.value) {
    ///   startCamera();
    /// }
    /// 
    /// // Request push notification permission
    /// const notifGranted = await truApi.devicePermission("Notifications");
    /// 
    /// // Request biometric authentication
    /// const bioGranted = await truApi.devicePermission("Biometrics");
    /// if (bioGranted.isOk && bioGranted.value) {
    ///   enableBiometricLogin();
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleDevicePermission((permission, { ok, err }) => {
    ///   // Show permission dialog to user
    ///   // permission is one of: "Notifications", "Camera", "Microphone",
    ///   //   "Bluetooth", "Nfc", "Location", "Clipboard", "OpenUrl", "Biometrics"
    ///   const granted = await showPermissionDialog(permission);
    ///   return ok(granted);
    /// });
    /// ```
    fn host_device_permission(&self, permission: DevicePermission) -> Result<bool, GenericError>;

    /// Requests permission for one or more remote operations. Batching multiple
    /// entries into a single call lets the host present a single prompt.
    ///
    /// Returns `true` if **all** requested permissions were granted, `false` if
    /// the user denied at least one. Products that need per-entry feedback
    /// should issue individual calls.
    ///
    /// V0.2: accepts `Vec<RemotePermission>` per
    /// [RFC 0001](https://github.com/paritytech/triangle-js-sdks/pull/66).
    ///
    /// # Product Function
    ///
    /// `truApi.permission(requests)`
    ///
    /// # Host Handler
    ///
    /// `container.handlePermission(handler)`
    ///
    /// # Request Description
    ///
    /// Array of RemotePermission entries: Remote(Vector(str)) | WebRtc | ChainSubmit | StatementSubmit
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Request multiple permissions in a single prompt
    /// const allowed = await truApi.permission([
    ///   { Remote: ["api.coingecko.com", "*.example.com"] },
    ///   { ChainSubmit: undefined },
    ///   { StatementSubmit: undefined },
    /// ]);
    /// 
    /// if (allowed.isOk && allowed.value) {
    ///   // All permissions granted, proceed
    ///   const price = await fetch("https://api.coingecko.com/...");
    /// }
    /// 
    /// // Request WebRTC permission
    /// const webrtcAllowed = await truApi.permission([
    ///   { WebRtc: undefined },
    /// ]);
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handlePermission((permissions, { ok, err }) => {
    ///   // permissions is an array of RemotePermission entries
    ///   // Present a single prompt to the user for all requested permissions
    ///   for (const perm of permissions) {
    ///     if (perm.tag === "Remote") {
    ///       if (!checkDomainAllowlist(perm.value)) {
    ///         return ok(false);
    ///       }
    ///     }
    ///     if (perm.tag === "ChainSubmit") {
    ///       if (!userHasApprovedTxSubmission) {
    ///         return ok(false);
    ///       }
    ///     }
    ///   }
    ///   return ok(true);
    /// });
    /// ```
    fn remote_permission(&self, permissions: Vec<RemotePermission>) -> Result<bool, GenericError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Scoped key-value storage. The host namespaces keys so different products cannot read each other's data.
pub trait LocalStorage {
    /// Reads a value from the scoped key-value store.
    ///
    /// # Product Function
    ///
    /// `truApi.localStorageRead(key)`
    ///
    /// # Host Handler
    ///
    /// `container.handleLocalStorageRead(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Read a stored preference
    /// const result = await truApi.localStorageRead("user-theme");
    /// 
    /// if (result.isOk && result.value !== null) {
    ///   const theme = new TextDecoder().decode(result.value);
    ///   applyTheme(theme);
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleLocalStorageRead((key, { ok, err }) => {
    ///   const namespacedKey = `${productId}:${key}`;
    ///   const value = localStorage.getItem(namespacedKey);
    ///   return ok(value ? new TextEncoder().encode(value) : null);
    /// });
    /// ```
    fn host_local_storage_read(
        &self,
        key: StorageKey,
    ) -> Result<Option<StorageValue>, StorageError>;

    /// Writes a value to the scoped key-value store.
    ///
    /// # Product Function
    ///
    /// `truApi.localStorageWrite([key, value])`
    ///
    /// # Host Handler
    ///
    /// `container.handleLocalStorageWrite(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Store a user preference
    /// const theme = new TextEncoder().encode("dark");
    /// const result = await truApi.localStorageWrite([
    ///   "user-theme",
    ///   theme
    /// ]);
    /// 
    /// if (result.isErr) {
    ///   console.error("Storage write failed:", result.error);
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleLocalStorageWrite(([key, value], { ok, err }) => {
    ///   const namespacedKey = `${productId}:${key}`;
    ///   try {
    ///     localStorage.setItem(namespacedKey, new TextDecoder().decode(value));
    ///     return ok(undefined);
    ///   } catch (e) {
    ///     return err({ Full: undefined });
    ///   }
    /// });
    /// ```
    fn host_local_storage_write(
        &self,
        key: StorageKey,
        value: StorageValue,
    ) -> Result<(), StorageError>;

    /// Clears a value from the scoped key-value store.
    ///
    /// # Product Function
    ///
    /// `truApi.localStorageClear(key)`
    ///
    /// # Host Handler
    ///
    /// `container.handleLocalStorageClear(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Clear stored data
    /// const result = await truApi.localStorageClear("user-theme");
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleLocalStorageClear((key, { ok, err }) => {
    ///   const namespacedKey = `${productId}:${key}`;
    ///   localStorage.removeItem(namespacedKey);
    ///   return ok(undefined);
    /// });
    /// ```
    fn host_local_storage_clear(&self, key: StorageKey) -> Result<(), StorageError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Product-specific account derivation, alias retrieval, ring VRF proofs, connection status, and user identity.
pub trait AccountManagement {
    /// Retrieves a product-specific derived account. The product provides a
    /// product identifier and derivation index; the host derives a unique public
    /// key for that combination.
    ///
    /// # Product Function
    ///
    /// `truApi.accountGet(productAccountId)`
    ///
    /// # Host Handler
    ///
    /// `container.handleAccountGet(handler)`
    ///
    /// # Request Description
    ///
    /// ProductAccountId is a Tuple(DotNsIdentifier, DerivationIndex)
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Get the product account for "my-product" with index 0
    /// const result = await truApi.accountGet([
    ///   "my-product.dot",  // DotNS identifier
    ///   0               // derivation index
    /// ]);
    /// 
    /// if (result.isOk) {
    ///   const { publicKey, name } = result.value;
    ///   console.log("Account:", name ?? "unnamed");
    ///   console.log("Key:", toHex(publicKey));
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleAccountGet(([dotNsId, derivationIndex], { ok, err }) => {
    ///   if (!currentUser) {
    ///     return err({ NotConnected: undefined });
    ///   }
    ///   const account = deriveProductAccount(
    ///     currentUser, dotNsId, derivationIndex
    ///   );
    ///   return ok({
    ///     publicKey: account.publicKey,
    ///     name: account.displayName ?? null,
    ///   });
    /// });
    /// ```
    fn host_account_get(
        &self,
        product_account_id: ProductAccountId,
    ) -> Result<Account, RequestCredentialsError>;

    /// Retrieves a contextual alias (ring VRF based) for a product account.
    ///
    /// # Product Function
    ///
    /// `truApi.accountGetAlias(productAccountId)`
    ///
    /// # Host Handler
    ///
    /// `container.handleAccountGetAlias(handler)`
    ///
    /// # Request Description
    ///
    /// ProductAccountId is a Tuple(DotNsIdentifier, DerivationIndex)
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Get a contextual alias for privacy-preserving identity
    /// const result = await truApi.accountGetAlias([
    ///   "my-product.dot",
    ///   0
    /// ]);
    /// 
    /// if (result.isOk) {
    ///   const { context, alias } = result.value;
    ///   // Use alias for anonymous interactions
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleAccountGetAlias(([dotNsId, derivationIndex], { ok, err }) => {
    ///   if (!currentUser) {
    ///     return err({ NotConnected: undefined });
    ///   }
    ///   const alias = computeContextualAlias(
    ///     currentUser, dotNsId, derivationIndex
    ///   );
    ///   return ok(alias);
    /// });
    /// ```
    fn host_account_get_alias(
        &self,
        product_account_id: ProductAccountId,
    ) -> Result<ContextualAlias, RequestCredentialsError>;

    /// Creates a ring VRF proof for a product account against a specific ring.
    ///
    /// # Product Function
    ///
    /// `truApi.accountCreateProof(params)`
    ///
    /// # Host Handler
    ///
    /// `container.handleAccountCreateProof(handler)`
    ///
    /// # Request Description
    ///
    /// ProductAccountId, RingLocation, and context bytes
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Create a ring VRF proof
    /// const result = await truApi.accountCreateProof([
    ///   ["my-product.dot", 0],          // ProductAccountId
    ///   {                              // RingLocation
    ///     genesisHash: polkadotGenesis,
    ///     ringRootHash: "0xabcdef...",
    ///     hints: { palletInstance: 42 },
    ///   },
    ///   contextBytes                   // Bytes - context data
    /// ]);
    /// 
    /// if (result.isOk) {
    ///   const proof = result.value; // RingVrfProof
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleAccountCreateProof(
    ///   ([productAccountId, ringLocation, context], { ok, err }) => {
    ///     const proof = ringVrf.createProof(
    ///       productAccountId, ringLocation, context
    ///     );
    ///     if (!proof) {
    ///       return err({ RingNotFound: undefined });
    ///     }
    ///     return ok(proof);
    ///   }
    /// );
    /// ```
    fn host_account_create_proof(
        &self,
        product_account_id: ProductAccountId,
        ring_location: RingLocation,
        context: Bytes,
    ) -> Result<RingVrfProof, CreateProofError>;

    /// Retrieves the user's non-product accounts (e.g., their main wallet
    /// account, not derived per-product).
    ///
    /// # Product Function
    ///
    /// `truApi.getNonProductAccounts()`
    ///
    /// # Host Handler
    ///
    /// `container.handleGetNonProductAccounts(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Get the user's wallet accounts
    /// const result = await truApi.getNonProductAccounts();
    /// 
    /// if (result.isOk) {
    ///   for (const account of result.value) {
    ///     console.log(account.name, toHex(account.publicKey));
    ///   }
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleGetNonProductAccounts((_, { ok, err }) => {
    ///   if (!currentUser) {
    ///     return err({ NotConnected: undefined });
    ///   }
    ///   return ok(currentUser.walletAccounts.map(a => ({
    ///     publicKey: a.publicKey,
    ///     name: a.displayName ?? null,
    ///   })));
    /// });
    /// ```
    fn host_get_non_product_accounts(&self) -> Result<Vec<Account>, RequestCredentialsError>;

    /// Subscribes to changes in the user's authentication state. The host pushes
    /// `Connected` or `Disconnected` whenever the auth state changes.
    ///
    /// # Product Function
    ///
    /// `truApi.accountConnectionStatusSubscribe(void, callback)`
    ///
    /// # Host Handler
    ///
    /// `container.handleAccountConnectionStatusSubscribe(handler)`
    ///
    /// # Response Description
    ///
    /// Status enum: "disconnected" | "connected"
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Watch for authentication changes
    /// const sub = truApi.accountConnectionStatusSubscribe(
    ///   undefined,
    ///   (status) => {
    ///     if (status === "connected") {
    ///       showWalletUI();
    ///     } else {
    ///       showConnectButton();
    ///     }
    ///   }
    /// );
    /// 
    /// // Later: clean up
    /// sub.unsubscribe();
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleAccountConnectionStatusSubscribe(
    ///   (params, send, interrupt) => {
    ///     // Send initial status
    ///     send(currentUser ? "connected" : "disconnected");
    /// 
    ///     // Watch for changes
    ///     const unsub = authStore.onChange((user) => {
    ///       send(user ? "connected" : "disconnected");
    ///     });
    /// 
    ///     return () => unsub(); // cleanup
    ///   }
    /// );
    /// ```
    fn host_account_connection_status_subscribe(&self) -> Subscription<AccountConnectionStatus>;

    /// Returns the user's primary DotNS account identifier. Requires JIT
    /// user approval on first call.
    ///
    /// V0.2.
    ///
    /// # Product Function
    ///
    /// `truApi.getUserId()`
    ///
    /// # Host Handler
    ///
    /// `container.handleGetUserId(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Get the user's primary identity
    /// const result = await truApi.getUserId();
    /// 
    /// if (result.isOk) {
    ///   const { dotNsIdentifier, publicKey } = result.value;
    ///   console.log("User:", dotNsIdentifier);
    ///   console.log("Public key:", toHex(publicKey));
    /// } else if (result.error.tag === "Rejected") {
    ///   console.log("User declined identity disclosure");
    /// } else if (result.error.tag === "NotConnected") {
    ///   console.log("User is not logged in");
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleGetUserId((_, { ok, err }) => {
    ///   if (!currentUser) {
    ///     return err({ NotConnected: undefined });
    ///   }
    ///   // Prompt user for permission to disclose identity
    ///   const approved = await showIdentityDisclosureDialog();
    ///   if (!approved) {
    ///     return err({ Rejected: undefined });
    ///   }
    ///   return ok({
    ///     dotNsIdentifier: currentUser.dotNsId,
    ///     publicKey: currentUser.primaryPublicKey,
    ///   });
    /// });
    /// ```
    fn host_get_user_id(&self) -> Result<UserIdentity, UserIdentityError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Transaction payload signing, raw message signing, and full transaction creation.
pub trait Signing {
    /// Requests the host to sign a Substrate transaction payload. The host
    /// typically shows a confirmation modal to the user.
    ///
    /// # Product Function
    ///
    /// `truApi.signPayload(payload)`
    ///
    /// # Host Handler
    ///
    /// `container.handleSignPayload(handler)`
    ///
    /// # Request Description
    ///
    /// See SigningPayload type for all fields
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Sign a Substrate extrinsic payload
    /// const result = await truApi.signPayload({
    ///   account: ["my-product.dot", 0],  // ProductAccountId
    ///   blockHash: "0xabc...",
    ///   blockNumber: "0x01",
    ///   era: "0x6502",
    ///   genesisHash: polkadotGenesis,
    ///   method: "0x0500...",    // encoded call data
    ///   nonce: "0x00",
    ///   specVersion: "0x01",
    ///   tip: "0x00",
    ///   transactionVersion: "0x01",
    ///   signedExtensions: ["CheckSpecVersion", "CheckTxVersion"],
    ///   version: 4,
    ///   withSignedTransaction: true,
    /// });
    /// 
    /// if (result.isOk) {
    ///   const { signature, signedTransaction } = result.value;
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleSignPayload((payload, { ok, err }) => {
    ///   // payload.account is a ProductAccountId tuple
    ///   const [dotNsId, derivationIndex] = payload.account;
    ///   const userApproved = await showSigningDialog(payload);
    ///   if (!userApproved) {
    ///     return err({ Rejected: undefined });
    ///   }
    ///   const key = deriveProductKey(currentUser, dotNsId, derivationIndex);
    ///   const signature = await key.sign(payload);
    ///   return ok({
    ///     signature,
    ///     signedTransaction: null,
    ///   });
    /// });
    /// ```
    fn host_sign_payload(&self, payload: SigningPayload) -> Result<SigningResult, SigningError>;

    /// Requests the host to sign a raw message (not a transaction).
    ///
    /// # Product Function
    ///
    /// `truApi.signRaw(payload)`
    ///
    /// # Host Handler
    ///
    /// `container.handleSignRaw(handler)`
    ///
    /// # Request Description
    ///
    /// See SigningRawPayload type for fields
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Sign a raw message
    /// const result = await truApi.signRaw({
    ///   account: ["my-product.dot", 0],  // ProductAccountId
    ///   data: { Payload: "Please sign this message to verify ownership" }
    /// });
    /// 
    /// // Or sign raw bytes
    /// const result2 = await truApi.signRaw({
    ///   account: ["my-product.dot", 0],
    ///   data: { Bytes: new Uint8Array([1, 2, 3]) }
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleSignRaw((payload, { ok, err }) => {
    ///   const [dotNsId, derivationIndex] = payload.account;
    ///   const userApproved = await showRawSigningDialog(payload);
    ///   if (!userApproved) {
    ///     return err({ Rejected: undefined });
    ///   }
    ///   const key = deriveProductKey(currentUser, dotNsId, derivationIndex);
    ///   const signature = await key.signRaw(payload.data);
    ///   return ok({ signature, signedTransaction: null });
    /// });
    /// ```
    fn host_sign_raw(&self, payload: SigningRawPayload) -> Result<SigningResult, SigningError>;

    /// Requests the host to create and sign a full transaction from a structured
    /// payload, using a product-derived account.
    ///
    /// # Product Function
    ///
    /// `truApi.createTransaction(params)`
    ///
    /// # Host Handler
    ///
    /// `container.handleCreateTransaction(handler)`
    ///
    /// # Request Description
    ///
    /// ProductAccountId and a VersionedTxPayload
    ///
    /// # Response Description
    ///
    /// The signed transaction bytes
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Create a signed transaction using product account
    /// const result = await truApi.createTransaction([
    ///   ["my-product.dot", 0],  // ProductAccountId
    ///   {
    ///     v1: {
    ///       signer: null,        // host picks the signer
    ///       callData: "0x0500...", // SCALE-encoded Call
    ///       extensions: [
    ///         { id: "CheckSpecVersion", extra: "0x", additionalSigned: "0x01000000" },
    ///       ],
    ///       txExtVersion: 0,
    ///       context: {
    ///         metadata: "0x...",
    ///         tokenSymbol: "DOT",
    ///         tokenDecimals: 10,
    ///         bestBlockHeight: 12345678,
    ///       },
    ///     }
    ///   }
    /// ]);
    /// 
    /// if (result.isOk) {
    ///   // Submit the signed transaction
    ///   const signedTx = result.value;
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleCreateTransaction(
    ///   ([productAccountId, versionedPayload], { ok, err }) => {
    ///     if (versionedPayload.tag !== "v1") {
    ///       return err({ NotSupported: "Only v1 supported" });
    ///     }
    ///     const tx = buildAndSignTransaction(
    ///       productAccountId, versionedPayload.value
    ///     );
    ///     return ok(tx);
    ///   }
    /// );
    /// ```
    fn host_create_transaction(
        &self,
        product_account_id: ProductAccountId,
        payload: VersionedTxPayload,
    ) -> Result<Bytes, CreateTransactionError>;

    /// Same as [`host_create_transaction`](Signing::host_create_transaction) but uses the
    /// user's main account instead of a product-derived account.
    ///
    /// # Product Function
    ///
    /// `truApi.createTransactionWithNonProductAccount(payload)`
    ///
    /// # Host Handler
    ///
    /// `container.handleCreateTransactionWithNonProductAccount(handler)`
    ///
    /// # Request Description
    ///
    /// Same VersionedTxPayload structure, without ProductAccountId
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Create transaction with user's main wallet account
    /// const result = await truApi.createTransactionWithNonProductAccount({
    ///   v1: {
    ///     signer: "5GrwvaEF5...",
    ///     callData: "0x0500...",
    ///     extensions: [],
    ///     txExtVersion: 0,
    ///     context: {
    ///       metadata: "0x...",
    ///       tokenSymbol: "DOT",
    ///       tokenDecimals: 10,
    ///       bestBlockHeight: 12345678,
    ///     },
    ///   }
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleCreateTransactionWithNonProductAccount(
    ///   (versionedPayload, { ok, err }) => {
    ///     const tx = buildAndSignTransaction(
    ///       currentUser.mainAccount, versionedPayload.value
    ///     );
    ///     return ok(tx);
    ///   }
    /// );
    /// ```
    fn host_create_transaction_with_non_product_account(
        &self,
        payload: VersionedTxPayload,
    ) -> Result<Bytes, CreateTransactionError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Chat room management, bot registration, message posting, simple group chats, and custom message rendering.
pub trait Chat {
    /// Registers a chat room with the host.
    ///
    /// # Product Function
    ///
    /// `truApi.chatCreateRoom(params)`
    ///
    /// # Host Handler
    ///
    /// `container.handleChatCreateRoom(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Create a chat room
    /// const result = await truApi.chatCreateRoom({
    ///   roomId: "general-chat",
    ///   name: "General Discussion",
    ///   icon: "https://example.com/chat-icon.png"
    /// });
    /// 
    /// if (result.isOk) {
    ///   console.log("Room status:", result.value.status);
    ///   // "New" or "Exists"
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleChatCreateRoom((request, { ok, err }) => {
    ///   const existing = chatRooms.get(request.roomId);
    ///   if (existing) {
    ///     return ok({ status: "Exists" });
    ///   }
    ///   chatRooms.set(request.roomId, {
    ///     name: request.name,
    ///     icon: request.icon,
    ///   });
    ///   return ok({ status: "New" });
    /// });
    /// ```
    fn host_chat_create_room(
        &self,
        request: ChatRoomRequest,
    ) -> Result<ChatRoomRegistrationResult, ChatRoomRegistrationError>;

    /// Creates a simple group chat room. Participants join via the returned
    /// deep link. The host handles the group chat UI with default rendering
    /// (no custom elements).
    ///
    /// V0.2: lightweight alternative to the full Chat Extension v2 (deferred
    /// to v0.3).
    ///
    /// # Product Function
    ///
    /// `truApi.chatCreateSimpleGroup(params)`
    ///
    /// # Host Handler
    ///
    /// `container.handleChatCreateSimpleGroup(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Create a simple group chat
    /// const result = await truApi.chatCreateSimpleGroup({
    ///   roomId: "team-alpha",
    ///   name: "Team Alpha Chat",
    ///   icon: "https://example.com/team-icon.png"
    /// });
    /// 
    /// if (result.isOk) {
    ///   const { status, joinLink } = result.value;
    ///   console.log("Room status:", status); // "New" or "Exists"
    ///   console.log("Share this link:", joinLink);
    ///   // Send joinLink to participants so they can join
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleChatCreateSimpleGroup((request, { ok, err }) => {
    ///   const existing = simpleGroupRooms.get(request.roomId);
    ///   if (existing) {
    ///     return ok({
    ///       status: "Exists",
    ///       joinLink: existing.joinLink,
    ///     });
    ///   }
    ///   const joinLink = generateDeepLink(request.roomId);
    ///   simpleGroupRooms.set(request.roomId, {
    ///     name: request.name,
    ///     icon: request.icon,
    ///     joinLink,
    ///   });
    ///   return ok({ status: "New", joinLink });
    /// });
    /// ```
    fn host_chat_create_simple_group(
        &self,
        request: SimpleGroupChatRequest,
    ) -> Result<SimpleGroupChatResult, ChatRoomRegistrationError>;

    /// Registers a bot identity for chat.
    ///
    /// # Product Function
    ///
    /// `truApi.chatRegisterBot(params)`
    ///
    /// # Host Handler
    ///
    /// `container.handleChatBotRegistration(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Register a bot for automated messages
    /// const result = await truApi.chatRegisterBot({
    ///   botId: "price-bot",
    ///   name: "Price Alert Bot",
    ///   icon: "https://example.com/bot-icon.png"
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleChatBotRegistration((request, { ok, err }) => {
    ///   const existing = chatBots.get(request.botId);
    ///   if (existing) {
    ///     return ok({ status: "Exists" });
    ///   }
    ///   chatBots.set(request.botId, {
    ///     name: request.name,
    ///     icon: request.icon,
    ///   });
    ///   return ok({ status: "New" });
    /// });
    /// ```
    fn host_chat_register_bot(
        &self,
        request: ChatBotRequest,
    ) -> Result<ChatBotRegistrationResult, ChatBotRegistrationError>;

    /// Posts a message to a chat room. Supports text, rich text, actions, files,
    /// reactions, and custom messages.
    ///
    /// # Product Function
    ///
    /// `truApi.chatPostMessage(params)`
    ///
    /// # Host Handler
    ///
    /// `container.handleChatPostMessage(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Post a text message
    /// const result = await truApi.chatPostMessage({
    ///   roomId: "general-chat",
    ///   payload: { Text: "Hello everyone!" }
    /// });
    /// 
    /// // Post an action menu
    /// const result2 = await truApi.chatPostMessage({
    ///   roomId: "general-chat",
    ///   payload: {
    ///     Actions: {
    ///       text: "Choose an option:",
    ///       actions: [
    ///         { actionId: "vote-yes", title: "Vote Yes" },
    ///         { actionId: "vote-no", title: "Vote No" },
    ///       ],
    ///       layout: "Grid"
    ///     }
    ///   }
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleChatPostMessage(({ roomId, payload }, { ok, err }) => {
    ///   const messageId = generateId();
    ///   chatRooms.get(roomId)?.messages.push({
    ///     id: messageId,
    ///     content: payload,
    ///     timestamp: Date.now(),
    ///   });
    ///   return ok({ messageId });
    /// });
    /// ```
    fn host_chat_post_message(
        &self,
        request: ChatPostMessageRequest,
    ) -> Result<ChatPostMessageResult, ChatMessagePostingError>;

    /// Subscribes to the list of chat rooms the product participates in. The
    /// host pushes the full room list whenever it changes.
    ///
    /// # Product Function
    ///
    /// `truApi.chatListSubscribe(void, callback)`
    ///
    /// # Host Handler
    ///
    /// `container.handleChatListSubscribe(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Watch the room list
    /// const sub = truApi.chatListSubscribe(
    ///   undefined,
    ///   (rooms) => {
    ///     console.log("Current rooms:", rooms);
    ///     rooms.forEach(room => {
    ///       console.log(`  ${room.roomId} as ${room.participatingAs}`);
    ///     });
    ///   }
    /// );
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleChatListSubscribe((_, send, interrupt) => {
    ///   // Send initial room list
    ///   send(getRoomsForProduct(productId));
    /// 
    ///   const unsub = roomStore.onChange(() => {
    ///     send(getRoomsForProduct(productId));
    ///   });
    /// 
    ///   return () => unsub();
    /// });
    /// ```
    fn host_chat_list_subscribe(&self) -> Subscription<Vec<ChatRoom>>;

    /// Subscribes to chat actions (messages posted by peers, button clicks,
    /// commands).
    ///
    /// # Product Function
    ///
    /// `truApi.chatActionSubscribe(void, callback)`
    ///
    /// # Host Handler
    ///
    /// `container.handleChatActionSubscribe(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Listen for chat events
    /// const sub = truApi.chatActionSubscribe(
    ///   undefined,
    ///   (action) => {
    ///     const { roomId, peer, payload } = action;
    /// 
    ///     if (payload.tag === "MessagePosted") {
    ///       handleNewMessage(roomId, peer, payload.value);
    ///     } else if (payload.tag === "ActionTriggered") {
    ///       handleAction(payload.value.actionId);
    ///     } else if (payload.tag === "Command") {
    ///       handleCommand(payload.value.command, payload.value.payload);
    ///     }
    ///   }
    /// );
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleChatActionSubscribe((_, send, interrupt) => {
    ///   const unsub = chatEvents.on("action", (action) => {
    ///     send(action);
    ///   });
    ///   return () => unsub();
    /// });
    /// ```
    fn host_chat_action_subscribe(&self) -> Subscription<ReceivedChatAction>;

    /// Registers a renderer for custom chat messages (reverse-subscription).
    ///
    /// This is the only method where roles are reversed: the host initiates by
    /// sending a [`CustomMessageRenderRequest`], and the product responds with a
    /// [`CustomRendererNode`] tree via the returned callback.
    ///
    /// # Pattern
    /// reverse-subscription
    ///
    /// # Product Function
    ///
    /// `createProductChatManager().onCustomMessageRenderingRequest(renderer)`
    ///
    /// # Host Handler
    ///
    /// `container.renderChatCustomMessage(msg, callback)`
    ///
    /// # Request Description
    ///
    /// Host sends message details for product to render
    ///
    /// # Response Description
    ///
    /// Recursive UI tree: Box, Column, Row, Text, Button, TextField, Spacer, Nil
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Register a custom message renderer
    /// const chatManager = createProductChatManager();
    /// 
    /// chatManager.onCustomMessageRenderingRequest(
    ///   ({ messageId, messageType, payload }, render, subscribeActions) => {
    ///     // Render a custom UI
    ///     render({
    ///       Column: {
    ///         modifiers: [{ padding: [8, 12, 8, 12] }],
    ///         props: { horizontalAlignment: "start" },
    ///         children: [
    ///           { Text: {
    ///             modifiers: [],
    ///             props: { style: "headline", color: "textPrimary" },
    ///             children: [{ String: "Custom Poll" }]
    ///           }},
    ///           { Button: {
    ///             modifiers: [],
    ///             props: {
    ///               text: "Vote",
    ///               variant: "primary",
    ///               clickAction: "vote-action"
    ///             },
    ///             children: []
    ///           }}
    ///         ]
    ///       }
    ///     });
    /// 
    ///     // Listen for interactions
    ///     subscribeActions((action) => {
    ///       if (action.actionId === "vote-action") {
    ///         handleVote(messageId);
    ///       }
    ///     });
    ///   }
    /// );
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Host triggers rendering of a custom message
    /// const unsub = container.renderChatCustomMessage(
    ///   {
    ///     messageId: "msg-123",
    ///     messageType: "poll",
    ///     payload: encodedPollData,
    ///   },
    ///   (renderedNode) => {
    ///     // Display the rendered CustomRendererNode tree
    ///     updateChatUI(renderedNode);
    ///   }
    /// );
    /// ```
    ///
    /// # Notes
    ///
    /// This is the only method where roles are reversed. The host initiates and the product responds with rendered UI.
    fn product_chat_custom_message_render_subscribe(
        &self,
        renderer: Box<dyn Fn(CustomMessageRenderRequest) -> CustomRendererNode + Send>,
    ) -> Subscription<()>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Subscribe to, create proofs for, and submit cryptographic statements.
pub trait StatementStore {
    /// Subscribes to statements matching a [`TopicFilter`]. The host pushes
    /// matching signed statements whenever the set changes.
    ///
    /// V0.2: replaces the v0.1 topic-vector signature with a richer
    /// [`TopicFilter`] that supports wildcard positions.
    ///
    /// # Product Function
    ///
    /// `truApi.statementStoreSubscribe(filter, callback)`
    ///
    /// # Host Handler
    ///
    /// `container.handleStatementStoreSubscribe(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Subscribe to statements with a topic filter
    /// const topic = new Uint8Array(32);
    /// topic.set([1, 2, 3]); // topic identifier
    /// 
    /// // Use null entries as wildcards
    /// const sub = truApi.statementStoreSubscribe(
    ///   { topics: [topic, null] },  // match first topic exactly, any second
    ///   (statements) => {
    ///     for (const stmt of statements) {
    ///       console.log("Statement from:", stmt.proof);
    ///       if (stmt.data) {
    ///         processStatement(stmt.data);
    ///       }
    ///     }
    ///   }
    /// );
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleStatementStoreSubscribe((filter, send, interrupt) => {
    ///   // filter.topics is an array where null = wildcard
    ///   send(statementStore.queryByFilter(filter));
    /// 
    ///   const unsub = statementStore.onChange(filter, (statements) => {
    ///     send(statements);
    ///   });
    /// 
    ///   return () => unsub();
    /// });
    /// ```
    fn remote_statement_store_subscribe(
        &self,
        filter: TopicFilter,
    ) -> Subscription<Vec<SignedStatement>>;

    /// Creates a cryptographic proof (signature) for a statement using a product
    /// account's key.
    ///
    /// # Product Function
    ///
    /// `truApi.statementStoreCreateProof(params)`
    ///
    /// # Host Handler
    ///
    /// `container.handleStatementStoreCreateProof(handler)`
    ///
    /// # Request Description
    ///
    /// ProductAccountId and a Statement to sign
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Create a proof for a statement
    /// const result = await truApi.statementStoreCreateProof([
    ///   ["my-product.dot", 0],  // ProductAccountId
    ///   {
    ///     proof: null,
    ///     decryptionKey: null,
    ///     expiry: BigInt(Date.now() + 86400000), // 24 hours
    ///     channel: null,
    ///     topics: [topicHash],
    ///     data: new TextEncoder().encode("my statement"),
    ///   }
    /// ]);
    /// 
    /// if (result.isOk) {
    ///   const proof = result.value; // StatementProof
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleStatementStoreCreateProof(
    ///   ([productAccountId, statement], { ok, err }) => {
    ///     const key = getProductKey(productAccountId);
    ///     if (!key) {
    ///       return err({ UnknownAccount: undefined });
    ///     }
    ///     const proof = key.sign(encodeStatement(statement));
    ///     return ok({ Sr25519: { signature: proof, signer: key.publicKey } });
    ///   }
    /// );
    /// ```
    fn remote_statement_store_create_proof(
        &self,
        product_account_id: ProductAccountId,
        statement: Statement,
    ) -> Result<StatementProof, StatementProofError>;

    /// Submits a pre-encoded statement to the statement store and returns
    /// the statement hash on success.
    ///
    /// V0.2: replaces the v0.1 signature that accepted a [`SignedStatement`]
    /// struct with raw SCALE-encoded bytes.
    ///
    /// # Product Function
    ///
    /// `truApi.statementStoreSubmit(encoded)`
    ///
    /// # Host Handler
    ///
    /// `container.handleStatementStoreSubmit(handler)`
    ///
    /// # Request Description
    ///
    /// Raw SCALE-encoded statement bytes
    ///
    /// # Response Description
    ///
    /// Statement hash on success
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Submit a pre-encoded statement (raw SCALE bytes)
    /// const encodedStatement = encodeStatementToScale(signedStatement);
    /// const result = await truApi.statementStoreSubmit(encodedStatement);
    /// 
    /// if (result.isOk) {
    ///   console.log("Statement hash:", result.value);
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleStatementStoreSubmit((encoded, { ok, err }) => {
    ///   const decoded = decodeStatement(encoded);
    ///   if (!decoded || !verifyProof(decoded)) {
    ///     return err({ GenericError: { reason: "Invalid statement encoding" } });
    ///   }
    ///   const hash = blake2Hash(encoded);
    ///   statementStore.insert(hash, decoded);
    ///   return ok(hash);
    /// });
    /// ```
    fn remote_statement_store_submit(&self, encoded: Bytes) -> Result<String, GenericError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Lookup preimages by hash.
pub trait Preimage {
    /// Subscribes to a preimage by its hash key. The host pushes the value when
    /// it becomes available.
    ///
    /// # Product Function
    ///
    /// `truApi.preimageLookupSubscribe(key, callback)`
    ///
    /// # Host Handler
    ///
    /// `container.handlePreimageLookupSubscribe(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Subscribe to a preimage
    /// const sub = truApi.preimageLookupSubscribe(
    ///   "0xabcdef1234...",  // hash of the preimage
    ///   (value) => {
    ///     if (value !== null) {
    ///       console.log("Preimage found:", value);
    ///     }
    ///   }
    /// );
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handlePreimageLookupSubscribe((key, send, interrupt) => {
    ///   const existing = preimageStore.get(key);
    ///   send(existing ?? null);
    /// 
    ///   const unsub = preimageStore.onAvailable(key, (value) => {
    ///     send(value);
    ///   });
    /// 
    ///   return () => unsub();
    /// });
    /// ```
    fn remote_preimage_lookup_subscribe(
        &self,
        key: PreimageKey,
    ) -> Subscription<Option<PreimageValue>>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Substrate blockchain RPC access implementing the chainHead v1 JSON-RPC spec over binary protocol.
pub trait ChainInteraction {
    /// Follows the chain head, receiving events about new blocks, finalization,
    /// and operation results. Implements the `chainHead_v1_follow` JSON-RPC
    /// method.
    ///
    /// # Product Function
    ///
    /// `truApi.chainHeadFollow(params, callback)`
    ///
    /// # Host Handler
    ///
    /// `container.handleChainConnection(factory)`
    ///
    /// # Response Description
    ///
    /// Enum with 12 variants: Initialized, NewBlock, BestBlockChanged, Finalized, OperationBodyDone, OperationCallDone, OperationStorageItems, OperationStorageDone, OperationWaitingForContinue, OperationInaccessible, OperationError, Stop
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Follow chain head events (low-level)
    /// const sub = truApi.chainHeadFollow(
    ///   { genesisHash: polkadotGenesis, withRuntime: true },
    ///   (event) => {
    ///     switch (event.tag) {
    ///       case "Initialized":
    ///         console.log("Finalized:", event.value.finalizedBlockHashes);
    ///         break;
    ///       case "NewBlock":
    ///         console.log("New block:", event.value.blockHash);
    ///         break;
    ///       case "BestBlockChanged":
    ///         console.log("Best:", event.value.bestBlockHash);
    ///         break;
    ///       case "Finalized":
    ///         console.log("Finalized:", event.value.finalizedBlockHashes);
    ///         break;
    ///     }
    ///   }
    /// );
    /// 
    /// // Typically used via higher-level abstraction:
    /// // const provider = createPapiProvider(polkadotGenesis);
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Host registers a JSON-RPC provider factory
    /// container.handleChainConnection((genesisHash) => {
    ///   // Return a JsonRpcProvider for the requested chain
    ///   const chain = chains.get(genesisHash);
    ///   if (!chain) return null;
    /// 
    ///   return chain.jsonRpcProvider;
    ///   // The chainConnectionManager handles all chain_head_*
    ///   // methods internally via this provider
    /// });
    /// ```
    ///
    /// # Notes
    ///
    /// On the Product Side, typically used via createPapiProvider(genesisHash) from @novasamatech/product-sdk. On the host side, handled via container.handleChainConnection(factory) which manages all chain methods internally.
    fn remote_chain_head_follow(
        &self,
        request: ChainHeadFollowRequest,
    ) -> Subscription<ChainHeadEvent>;

    /// Retrieves a block header by hash within a follow subscription.
    /// Returns the SCALE-encoded block header, or `None`.
    ///
    /// # Product Function
    ///
    /// `truApi.chainHeadHeader(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Response Description
    ///
    /// SCALE-encoded block header, or null
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// const result = await truApi.chainHeadHeader({
    ///   genesisHash: polkadotGenesis,
    ///   followSubscriptionId: subId,
    ///   hash: blockHash,
    /// });
    /// 
    /// if (result.isOk && result.value) {
    ///   const headerBytes = result.value;
    ///   const header = decodeHeader(headerBytes);
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainHead_v1_header JSON-RPC call
    /// ```
    fn remote_chain_head_header(
        &self,
        request: ChainHeadBlockRequest,
    ) -> Result<Option<Hex>, GenericError>;

    /// Retrieves a block body. Returns an operation ID; results arrive as
    /// [`ChainHeadEvent::OperationBodyDone`] events on the follow subscription.
    ///
    /// # Product Function
    ///
    /// `truApi.chainHeadBody(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Response Description
    ///
    /// Started { operationId: OperationId } or LimitReached
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// const result = await truApi.chainHeadBody({
    ///   genesisHash: polkadotGenesis,
    ///   followSubscriptionId: subId,
    ///   hash: blockHash,
    /// });
    /// 
    /// if (result.isOk && result.value.tag === "Started") {
    ///   const opId = result.value.value.operationId;
    ///   // Wait for OperationBodyDone event on follow subscription
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainHead_v1_body JSON-RPC call
    /// ```
    fn remote_chain_head_body(
        &self,
        request: ChainHeadBlockRequest,
    ) -> Result<OperationStartedResult, GenericError>;

    /// Queries chain storage. Returns an operation ID; results arrive as
    /// [`ChainHeadEvent::OperationStorageItems`] /
    /// [`ChainHeadEvent::OperationStorageDone`] events.
    ///
    /// # Product Function
    ///
    /// `truApi.chainHeadStorage(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// const result = await truApi.chainHeadStorage({
    ///   genesisHash: polkadotGenesis,
    ///   followSubscriptionId: subId,
    ///   hash: blockHash,
    ///   items: [
    ///     { key: "0x26aa394eea5630e07c48ae0c9558cef7", type: "Value" }
    ///   ],
    ///   childTrie: null,
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainHead_v1_storage JSON-RPC call
    /// ```
    fn remote_chain_head_storage(
        &self,
        request: ChainHeadStorageRequest,
    ) -> Result<OperationStartedResult, GenericError>;

    /// Executes a runtime API call. Returns an operation ID; result arrives as
    /// [`ChainHeadEvent::OperationCallDone`] event.
    ///
    /// # Product Function
    ///
    /// `truApi.chainHeadCall(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// const result = await truApi.chainHeadCall({
    ///   genesisHash: polkadotGenesis,
    ///   followSubscriptionId: subId,
    ///   hash: blockHash,
    ///   function: "Metadata_metadata",
    ///   callParameters: "0x",
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainHead_v1_call JSON-RPC call
    /// ```
    fn remote_chain_head_call(
        &self,
        request: ChainHeadCallRequest,
    ) -> Result<OperationStartedResult, GenericError>;

    /// Unpins block hashes, allowing the node to discard them.
    ///
    /// # Product Function
    ///
    /// `truApi.chainHeadUnpin(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// await truApi.chainHeadUnpin({
    ///   genesisHash: polkadotGenesis,
    ///   followSubscriptionId: subId,
    ///   hashes: [oldBlockHash1, oldBlockHash2],
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainHead_v1_unpin JSON-RPC call
    /// ```
    fn remote_chain_head_unpin(&self, request: ChainHeadUnpinRequest) -> Result<(), GenericError>;

    /// Continues a paused operation (when
    /// [`ChainHeadEvent::OperationWaitingForContinue`] is received).
    ///
    /// # Product Function
    ///
    /// `truApi.chainHeadContinue(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // When OperationWaitingForContinue is received:
    /// await truApi.chainHeadContinue({
    ///   genesisHash: polkadotGenesis,
    ///   followSubscriptionId: subId,
    ///   operationId: opId,
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainHead_v1_continue JSON-RPC call
    /// ```
    fn remote_chain_head_continue(
        &self,
        request: ChainHeadOperationRequest,
    ) -> Result<(), GenericError>;

    /// Stops an in-progress operation.
    ///
    /// # Product Function
    ///
    /// `truApi.chainHeadStopOperation(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// await truApi.chainHeadStopOperation({
    ///   genesisHash: polkadotGenesis,
    ///   followSubscriptionId: subId,
    ///   operationId: opId,
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainHead_v1_stopOperation JSON-RPC call
    /// ```
    fn remote_chain_head_stop_operation(
        &self,
        request: ChainHeadOperationRequest,
    ) -> Result<(), GenericError>;

    /// Gets the genesis hash for a chain.
    ///
    /// # Product Function
    ///
    /// `truApi.chainSpecGenesisHash(genesisHash)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// const result = await truApi.chainSpecGenesisHash(
    ///   polkadotGenesis
    /// );
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainSpec_v1_genesisHash JSON-RPC call
    /// ```
    fn remote_chain_spec_genesis_hash(
        &self,
        genesis_hash: GenesisHash,
    ) -> Result<Hex, GenericError>;

    /// Gets the chain name.
    ///
    /// # Product Function
    ///
    /// `truApi.chainSpecChainName(genesisHash)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// const result = await truApi.chainSpecChainName(
    ///   polkadotGenesis
    /// );
    /// if (result.isOk) {
    ///   console.log("Chain:", result.value); // "Polkadot"
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainSpec_v1_chainName JSON-RPC call
    /// ```
    fn remote_chain_spec_chain_name(
        &self,
        genesis_hash: GenesisHash,
    ) -> Result<String, GenericError>;

    /// Gets the chain properties as a JSON-encoded string.
    ///
    /// # Product Function
    ///
    /// `truApi.chainSpecProperties(genesisHash)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Response Description
    ///
    /// JSON-encoded chain properties
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// const result = await truApi.chainSpecProperties(
    ///   polkadotGenesis
    /// );
    /// if (result.isOk) {
    ///   const props = JSON.parse(result.value);
    ///   console.log("Token:", props.tokenSymbol); // "DOT"
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to chainSpec_v1_properties JSON-RPC call
    /// ```
    fn remote_chain_spec_properties(
        &self,
        genesis_hash: GenesisHash,
    ) -> Result<String, GenericError>;

    /// Broadcasts a signed transaction to the network.
    /// Returns an operation ID if accepted, `None` if rejected.
    ///
    /// # Product Function
    ///
    /// `truApi.chainTransactionBroadcast(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Response Description
    ///
    /// Operation ID if accepted, null if rejected
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// const result = await truApi.chainTransactionBroadcast({
    ///   genesisHash: polkadotGenesis,
    ///   transaction: signedTxHex,
    /// });
    /// 
    /// if (result.isOk && result.value) {
    ///   console.log("Broadcasting, op:", result.value);
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to transaction_v1_broadcast JSON-RPC call
    /// ```
    fn remote_chain_transaction_broadcast(
        &self,
        request: ChainTransactionBroadcastRequest,
    ) -> Result<Option<String>, GenericError>;

    /// Stops broadcasting a transaction.
    ///
    /// # Product Function
    ///
    /// `truApi.chainTransactionStop(params)`
    ///
    /// # Host Handler
    ///
    /// `Managed by chainConnectionManager`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// await truApi.chainTransactionStop({
    ///   genesisHash: polkadotGenesis,
    ///   operationId: broadcastOpId,
    /// });
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// // Handled automatically by chainConnectionManager
    /// // translates to transaction_v1_stop JSON-RPC call
    /// ```
    fn remote_chain_transaction_stop(
        &self,
        request: ChainTransactionStopRequest,
    ) -> Result<(), GenericError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Payment operations using the Coinage API (RFC 0006).
pub trait Payment {
    /// Subscribes to the user's payment balance. The host prompts the user
    /// for permission to disclose their balance on the first call.
    ///
    /// See [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94).
    ///
    /// # Product Function
    ///
    /// `truApi.paymentBalanceSubscribe(void, callback)`
    ///
    /// # Host Handler
    ///
    /// `container.handlePaymentBalanceSubscribe(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Subscribe to the user's payment balance
    /// const sub = truApi.paymentBalanceSubscribe(
    ///   undefined,
    ///   (balance) => {
    ///     console.log("Available:", balance.available);
    ///     console.log("Pending:", balance.pending);
    ///     updateBalanceUI(balance);
    ///   }
    /// );
    /// 
    /// // Later: clean up
    /// sub.unsubscribe();
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handlePaymentBalanceSubscribe((_, send, interrupt) => {
    ///   // Prompt user for permission to disclose balance
    ///   const allowed = await requestBalancePermission();
    ///   if (!allowed) {
    ///     throw new PaymentBalanceError("PermissionDenied");
    ///   }
    /// 
    ///   // Send initial balance
    ///   send(getUserBalance());
    /// 
    ///   const unsub = balanceStore.onChange((balance) => {
    ///     send(balance);
    ///   });
    /// 
    ///   return () => unsub();
    /// });
    /// ```
    fn host_payment_balance_subscribe(
        &self,
    ) -> Result<Subscription<PaymentBalance>, PaymentBalanceError>;

    /// Tops up the user's payment balance from a product-controlled funding
    /// source. This operation is always in the user's favour and does not
    /// require user consent.
    ///
    /// See [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94).
    ///
    /// # Product Function
    ///
    /// `truApi.paymentTopUp(amount, source)`
    ///
    /// # Host Handler
    ///
    /// `container.handlePaymentTopUp(handler)`
    ///
    /// # Request Description
    ///
    /// Balance amount and a PaymentTopUpSource (ProductAccount or PrivateKey)
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Top up from a product account
    /// const result = await truApi.paymentTopUp(
    ///   1000000n,  // amount in smallest unit
    ///   { ProductAccount: 0 }  // derivation index
    /// );
    /// 
    /// if (result.isErr) {
    ///   if (result.error.tag === "InsufficientFunds") {
    ///     console.error("Source account has insufficient funds");
    ///   }
    /// }
    /// 
    /// // Top up from a one-time private key
    /// const result2 = await truApi.paymentTopUp(
    ///   5000000n,
    ///   { PrivateKey: privateKeyBytes }  // 32-byte Ed25519 private key
    /// );
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handlePaymentTopUp(([amount, source], { ok, err }) => {
    ///   if (source.tag === "ProductAccount") {
    ///     const account = getProductAccount(source.value);
    ///     if (account.balance < amount) {
    ///       return err({ InsufficientFunds: undefined });
    ///     }
    ///     transferToUserBalance(account, amount);
    ///   } else if (source.tag === "PrivateKey") {
    ///     const account = accountFromPrivateKey(source.value);
    ///     if (!account) {
    ///       return err({ InvalidSource: undefined });
    ///     }
    ///     transferToUserBalance(account, amount);
    ///   }
    ///   return ok(undefined);
    /// });
    /// ```
    fn host_payment_top_up(
        &self,
        amount: Balance,
        source: PaymentTopUpSource,
    ) -> Result<(), PaymentTopUpError>;

    /// Requests a payment from the user's available balance to `destination`.
    /// The host prompts the user to authorize. Returns a [`PaymentReceipt`]
    /// whose [`PaymentId`] can be tracked via
    /// [`host_payment_status_subscribe`](Payment::host_payment_status_subscribe).
    ///
    /// A successful response means the user authorized the payment and the
    /// host accepted it for processing, **not** that it has settled.
    ///
    /// See [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94).
    ///
    /// # Product Function
    ///
    /// `truApi.paymentRequest(amount, destination)`
    ///
    /// # Host Handler
    ///
    /// `container.handlePaymentRequest(handler)`
    ///
    /// # Request Description
    ///
    /// Balance amount and destination AccountId (32 bytes)
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Request payment from the user
    /// const result = await truApi.paymentRequest(
    ///   500000n,  // amount
    ///   destinationAccountId  // 32-byte AccountId
    /// );
    /// 
    /// if (result.isOk) {
    ///   const { id } = result.value;
    ///   console.log("Payment accepted, tracking ID:", id);
    ///   // Track the payment status
    ///   trackPayment(id);
    /// } else if (result.error.tag === "Denied") {
    ///   console.log("User denied the payment");
    /// } else if (result.error.tag === "InsufficientBalance") {
    ///   console.log("User does not have enough balance");
    /// }
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handlePaymentRequest(([amount, destination], { ok, err }) => {
    ///   if (getUserBalance().available < amount) {
    ///     return err({ InsufficientBalance: undefined });
    ///   }
    /// 
    ///   const approved = await showPaymentDialog(amount, destination);
    ///   if (!approved) {
    ///     return err({ Denied: undefined });
    ///   }
    /// 
    ///   const paymentId = initiatePayment(amount, destination);
    ///   return ok({ id: paymentId });
    /// });
    /// ```
    fn host_payment_request(
        &self,
        amount: Balance,
        destination: AccountId,
    ) -> Result<PaymentReceipt, PaymentRequestError>;

    /// Subscribes to status updates for a previously requested payment.
    /// Emits status changes until a terminal state (`Completed` or `Failed`).
    ///
    /// See [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94).
    ///
    /// # Product Function
    ///
    /// `truApi.paymentStatusSubscribe(paymentId, callback)`
    ///
    /// # Host Handler
    ///
    /// `container.handlePaymentStatusSubscribe(handler)`
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Track a payment's lifecycle
    /// const sub = truApi.paymentStatusSubscribe(
    ///   paymentId,
    ///   (status) => {
    ///     switch (status.tag ?? status) {
    ///       case "Processing":
    ///         showSpinner("Payment processing...");
    ///         break;
    ///       case "Completed":
    ///         showSuccess("Payment completed!");
    ///         sub.unsubscribe();
    ///         break;
    ///       case "Failed":
    ///         showError("Payment failed:", status.value);
    ///         sub.unsubscribe();
    ///         break;
    ///     }
    ///   }
    /// );
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handlePaymentStatusSubscribe((paymentId, send, interrupt) => {
    ///   const payment = paymentStore.get(paymentId);
    ///   if (!payment) {
    ///     throw new PaymentStatusError("PaymentNotFound");
    ///   }
    /// 
    ///   // Send current status
    ///   send(payment.status);
    /// 
    ///   const unsub = paymentStore.onStatusChange(paymentId, (status) => {
    ///     send(status);
    ///   });
    /// 
    ///   return () => unsub();
    /// });
    /// ```
    fn host_payment_status_subscribe(
        &self,
        payment_id: PaymentId,
    ) -> Result<Subscription<PaymentStatus>, PaymentStatusError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// Deterministic entropy derivation (RFC 0007).
pub trait EntropyDerivation {
    /// Derives 32 bytes of deterministic entropy scoped to the calling product
    /// and the provided `key`. Uses a three-layer BLAKE2b-256 keyed hashing
    /// scheme over the user's root BIP-39 entropy.
    ///
    /// `key` is an arbitrary value (up to 32 bytes) chosen by the caller; the
    /// host does not assign any semantic meaning to it.
    ///
    /// The same root account + product + key always yields the same
    /// [`Entropy`] on every conforming host.
    ///
    /// See [RFC 0007](https://github.com/paritytech/triangle-js-sdks/pull/95).
    ///
    /// # Product Function
    ///
    /// `truApi.deriveEntropy(key)`
    ///
    /// # Host Handler
    ///
    /// `container.handleDeriveEntropy(handler)`
    ///
    /// # Request Description
    ///
    /// Arbitrary key bytes (up to 32 bytes) chosen by the caller
    ///
    /// # Product Example
    ///
    /// ```typescript
    /// // Derive deterministic entropy for a specific purpose
    /// const key = new TextEncoder().encode("my-secret-seed");
    /// const result = await truApi.deriveEntropy(key);
    /// 
    /// if (result.isOk) {
    ///   const entropy = result.value; // 32 bytes, deterministic
    ///   // Use entropy to seed a PRNG, derive keys, etc.
    ///   const derivedKey = await crypto.subtle.importKey(
    ///     "raw", entropy, "HKDF", false, ["deriveBits"]
    ///   );
    /// }
    /// 
    /// // Same key always produces the same entropy for the same user+product
    /// const result2 = await truApi.deriveEntropy(key);
    /// // result2.value === result.value (byte-for-byte identical)
    /// ```
    ///
    /// # Host Example
    ///
    /// ```typescript
    /// container.handleDeriveEntropy((key, { ok, err }) => {
    ///   // Three-layer BLAKE2b-256 keyed hashing:
    ///   // 1. Layer 1: BLAKE2b(rootEntropy, "truapi-entropy-v1")
    ///   // 2. Layer 2: BLAKE2b(layer1, productDotNsId)
    ///   // 3. Layer 3: BLAKE2b(layer2, key)
    ///   const entropy = deriveThreeLayerEntropy(
    ///     currentUser.rootEntropy,
    ///     currentProduct.dotNsId,
    ///     key
    ///   );
    ///   return ok(entropy);
    /// });
    /// ```
    fn host_derive_entropy(&self, key: Vec<u8>) -> Result<Entropy, DeriveEntropyError>;
}

// ────────────────────────────────────────────────────────────────────────────

/// The combined TrUAPI v0.2 interface.
pub trait TrUApi:
    TrUApiCalls
    + Permissions
    + LocalStorage
    + AccountManagement
    + Signing
    + Chat
    + StatementStore
    + Preimage
    + ChainInteraction
    + Payment
    + EntropyDerivation
{
}
