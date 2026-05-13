---
title: "Chat Extension v2 — Extendable Chat System for Host API"
type: rfc
status: draft
owner: "@valentin-parity"
pr:
---

# RFC 0002 — Chat Extension v2: Extendable Chat System for Host API

|                 |                                                            |
| --------------- | ---------------------------------------------------------- |
| **Start Date**  | 2026-03-12                                                 |
| **Description** | Extendable chat system controlled by Product scripts       |
| **Authors**     | Valentin Sergeev                                                        |

## Summary

This RFC replaces the Chat Extension v1 of the Host API with a new architecture where **Products control the chat experience** while the **Host provides the UI shell**.

In v1, the Host owned the message store and the Product could only post and receive messages. In v2, the Product becomes the **data source** for messages, participants, metadata, and actions. The Host renders the chat UI, handles pagination, and delegates all data operations back to the Product via registered handlers.

The key design principle is: **be generic on the API level, but define reasonable defaults**. If a Product registers no custom handlers, the Host falls back to built-in behavior (two participants, standard message actions, default input). If a Product wants full control, it can override any aspect of the chat experience — message fetching, participant lists, toolbar actions, input UI, and per-message action menus.

This enables Products to build arbitrarily complex chat experiences (group chats, on-chain messaging, custom protocols) while reusing the Host's native chat UI components.

## Motivation

Chat Extension v1 had several limitations:

1. **Host-owned message store**: All messages lived in the Host's database. Products could not bring their own message source (e.g. on-chain messages, P2P protocols, external backends). This made it impossible to build Products with custom messaging protocols.

2. **Fixed chat structure**: v1 assumed a simple room-and-messages model with no customization of actions, participants, or input UI. Products that needed richer interactions (reactions, media, payments, custom actions) had no path forward.

3. **No scalability path**: With the Host storing all messages, there was no way to support large chats (10k+ members) or large numbers of rooms without overwhelming the Host's local storage.

4. **Rigid participant model**: v1 had no concept of participant types. The new Polkadot identity model (Person, LitePerson, Contact) requires a richer participant abstraction.

v2 solves these problems by inverting control: the Product decides how messages are stored, fetched, and acted upon. The Host becomes a rendering engine with sensible defaults.

## Detailed Design

### 1. Initialization Model

Products are JS scripts that the Host executes from scratch on each app start. During script initialization, the Product:

1. Creates chat rooms via `chat_room_create`
2. Registers handlers via `handle_*` functions (message source, actions, participants, etc.)
3. Pushes updates via `on_*` functions (metadata changes, new messages, etc.)

**The Host MUST delay rendering decisions for a chat room until the Product script has finished initialization.** This ensures the Host knows which handlers were registered before it attempts to display any chat UI.

If a `handle_*` function is never called for a room, the Host uses its built-in default behavior for that aspect.

### 2. Room Lifecycle

#### 2.1 Types

```rust
RoomId = String
ProductId = DotNsIdentifier

enum ResourceUri {
  Inline(Base64String),
  Preimage(CID)
}

struct CreateChatRoomRequest {
    // Host computes the actual room ID to guarantee scoping.
    // e.g. blake2b("product" ++ productId ++ room_id_source)
    room_id_source: String,
    // Initial state for the room metadata
    initial_metadata: {
        avatar: ResourceUri?,
        name: String,
        description: String?,
    },
    // Optional. Initial state for the room preview that will define appearance in the chat list. If not provided - default rendering is used
    initial_preview?: {
        order: ChatRoomPreviewOrder,
        timestamp: Timestamp,
        badge: ChatRoomPreviewBadge,
    },
}

struct CreateChatRoomResponse {
    status: ChatRoomRegistrationStatus,
    room_id: RoomId,
}

enum ChatRoomRegistrationStatus {
    New,
    Exists,
}

struct ChatRoom {
    id: RoomId,
}
```

#### 2.2 Methods

```rust
// Create a new chat room. Host deduplicates by room_id_source per product.
// If a room with the same room_id_source already exists, returns Exists status.
fn chat_room_create(
    request: CreateChatRoomRequest
) -> Result<CreateChatRoomResponse, ChatRoomRegistrationErr>;

// Subscribe to the list of rooms created by this Product.
fn chat_room_list_subscribe(
    callback: fn(Vec<ChatRoom>)
) -> Result<Subscriber, GenericErr>;

// Delete a room and all associated data.
fn chat_room_delete(
    room_id: RoomId
) -> Result<(), GenericErr>;
```

#### 2.3 Errors

```rust
enum ChatRoomRegistrationErr {
    PermissionDenied,
    Unknown(GenericErr),
}
```

### 3. Room Metadata & Preview

Products control how rooms appear in the chat list and inside the chat screen.

#### 3.1 Types

```rust
struct RoomMetadata {
    id: RoomId,
    avatar: ResourceUri?,
    name: String,
    description: String?,
}

enum ChatRoomPreviewOrder {
    Timestamp,
    // Pinned rooms are ordered above non-pinned.
    // Multiple pinned rooms are secondarily ordered by timestamp.
    PinToTop,
}

enum ChatRoomPreviewBadge {
    // Derived from message read status (default) or from
    // the Product's subscribeUnreadCount handler.
    UnreadCount,
    Image(ResourceUri),
}

struct ChatRoomPreviewText {
    preview_text: String,
    // true to show animation, e.g. "User is typing..."
    ongoing: bool,
}

struct ChatRoomPreview {
    room_id: RoomId,
    order: ChatRoomPreviewOrder,
    timestamp: Timestamp,
    badge: ChatRoomPreviewBadge,
}
```

#### 3.2 Methods

```rust
// Notify the Host of updated room metadata.
// Host shows cached value from the last call.
// If never called, Host uses initial_metadata from CreateChatRoomRequest.
fn on_chat_room_metadata_change(new_metadata: RoomMetadata);

// Notify the Host of updated room preview.
// Same caching behavior as metadata.
fn on_chat_room_preview_change(preview: ChatRoomPreview);
```

### 4. Messages

This is the core of v2. The Product can either rely on the Host's built-in message store, or take over as the message data source.

#### 4.1 Types

```rust
MessageId = UUID
ChatParticipantId = String
Timestamp = u64

struct MessageOrigin {
    participant_id: ChatParticipantId,
}

CustomMessageType = Vec<u8>
CustomMessagePayload = Vec<u8>

enum MessageContent {
    // Particular case of RichText, for optimization.
    Text(String),

    // General purpose text message with optional media.
    RichText {
        text: Option<String>,  // Markdown-enabled
        media: Vec<ChatMedia>,
    },

    // preview_text: Human-readable summary for chat list preview
    // (e.g. "Payment received", "Shared a location").
    // If None, Host displays "Sent a message".
    Custom {
        message_type: CustomMessageType,
        payload: CustomMessagePayload,
        preview_text: Option<String>,
    },
}

enum ChatMedia {
    Image(ResourceUri),
    File(ResourceUri),
    Video(ResourceUri)
}

Emoji = String

struct MessageReaction {
    origin: MessageOrigin,
    reaction: Emoji,
}

struct AggregatedReaction {
    count: u32,
    reacted_by_me: bool,
}

enum MessageStatus {
    // Message is visible to sender but not yet ready for delivery.
    // e.g. media is still uploading.
    Preparing,
    ReadyForDelivery,
    Delivered,
    Read,
}

struct Message {
    id: MessageId,
    origin: MessageOrigin,
    room_id: RoomId,
    timestamp: Timestamp,
    content: MessageContent,
    status: MessageStatus,
    reply_to: Option<MessageId>,
    last_edited: Option<Timestamp>,
    reactions: Map<Emoji, AggregatedReaction>,
}
```

#### 4.2 Message Feed Updates

The Product notifies the Host when the message feed changes. This is the **primary method** for pushing message data to the Host.

```rust
enum MessageFeedUpdate {
    NewMessage(Message),
    MessageEdit(MessageId, MessageContent, Timestamp),
    ReactionPlaced(MessageId, MessageReaction),
    ReactionRemoved(MessageId, MessageReaction),
    MessageDeleted(MessageId),
}

// Notify the Host of a message feed update.
//
// Behavior depends on whether handle_chat_messages_source was called:
// - If Product registered a message source: Host updates in-memory screen
//   state only (no persistent writes).
// - If Product did NOT register a message source: Host persists the update
//   to its local message store.
fn on_message_feed_update(
    room_id: RoomId,
    update: MessageFeedUpdate
);
```

#### 4.3 Product as Message Source

When a Product registers as the message source, it takes full control over message fetching and unread tracking. The Host MAY cache pages for performance but MUST NOT attempt to store or request the entire history.

```rust
PaginationCursor = String

struct Page<T> {
    next: Option<PaginationCursor>,
    items: T,
}

type MessagesPage = Page<Vec<Message>>

enum PaginationAnchor {
    // Fetch the next page. If cursor is None, fetches from the beginning.
    NextPage(Option<PaginationCursor>),
    // Fetch the previous page. If cursor is None, fetches from the end.
    PreviousPage(Option<PaginationCursor>),
}

type UpdateUnreadCountFn = fn(u32);

// User-initiated action in the chat.
enum UserChatAction {
    NewMessage {
        content: MessageContent,
        reply_to: Option<MessageId>,
    },
    EditMessage {
        message_id: MessageId,
        new_content: MessageContent,
    },
    ReactionPlaced {
        message_id: MessageId,
        reaction: MessageReaction,
    },
    ReactionRemoved {
        message_id: MessageId,
        reaction: MessageReaction,
    },
    MessageDeleted {
        message_id: MessageId
    }
}

// Return type varies by action:
// - NewMessage → the finalized Message (with ID, timestamp, status assigned by Product)
// - EditMessage, ReactionPlaced, ReactionRemoved → void
enum UserChatActionResult {
    MessageCreated(Message),
    Acknowledged,
}

// Register the Product as the message data source for a specific room.
// Per-room: must be called separately for each room the Product wants to control.
fn handle_chat_messages_source(
    room_id: RoomId,

    // Product provides a function to fetch a page of messages.
    fetch_message_page: fn(
        anchor: Option<PaginationAnchor>,
        page_size: u32
    ) -> MessagesPage,

    // Product takes over unread count tracking.
    subscribe_unread_count: fn(
        update_count: UpdateUnreadCountFn
    ),

    // Host calls this when the user performs a chat action (send, edit, react).
    on_user_action: fn(UserChatAction) -> UserChatActionResult,

    // Host calls this when the user has seen a message.
    // Product can update its unread counter accordingly.
    on_user_seen_message: fn(Message),
);
```

#### 4.4 Reaction Details

When a user taps on a reaction to see who reacted, the Host requests the full list from the Product. This is **not paginated** — for large chats where tracking individual reactors is impractical, the Product returns `NotAvailable`.

```rust
enum MessageReactionsErr {
    // The Product cannot provide individual reaction details
    // (e.g. chat is too large).
    NotAvailable,
    Unknown(GenericErr),
}

// Register a handler to load individual reactions for a message.
// If not registered, the Host does not offer a "see who reacted" UI.
fn handle_message_reactions_detail(
    room_id: RoomId,
    get_reactions: fn(MessageId) -> Result<Vec<MessageReaction>, MessageReactionsErr>,
);
```

#### 4.5 Chat Placeholder

Shown when a chat room has no messages yet.

```rust
struct ChatPlaceholder {
    text: String,
}

fn handle_chat_placeholder(
    get_placeholder: fn() -> ChatPlaceholder
);
```

### 5. Message Actions

Products can customize the actions available on each message (long-press menu). By default, the Host provides built-in actions (copy, reply, edit own messages, react).

#### 5.1 Types

```rust
Deeplink = String

enum MessageActions {
    // Product takes full control. Only product-defined actions are shown.
    Custom(Vec<ProductMessageAction>),

    // Host shows built-in actions plus additional product-defined actions
    // in a separate section.
    ExtendBuiltIn(Vec<ProductMessageAction>),
}

enum BuiltInMessageAction {
    Copy(String),
    Edit(EditableMessageContent),
    Reply,
    // If allowed_emoji_set is None, all emoji are allowed.
    Reaction(Option<Vec<Emoji>>),
}

enum EditableMessageContent {
    Text(String),
    RichText { text: Option<String>, media: Vec<ChatMedia> },
}

struct ProductMessageAction {
    label: String,
    handler: ProductMessageActionHandler,
}

enum ProductMessageActionHandler {
    Deeplink(Deeplink),
    Callback(fn(Message)),
}
```

#### 5.2 Methods

```rust
// Optional. Register a handler that determines available actions per message.
// If not called, Host uses default built-in actions.
fn handle_message_actions(
    get_message_actions: fn(Message) -> MessageActions
);
```

### 6. Toolbar Actions

Products can add action buttons to the top-right area of the chat screen.

```rust
ToolbarActionId = String

struct ToolbarAction {
    id: ToolbarActionId,
    icon: ResourceUri,
    // Shown as label if Host needs to collapse actions into a dropdown.
    label: String,
    // Called when the user taps this action.
    on_triggered: fn(),
}

// Register a handler that provides toolbar actions for this room.
fn handle_toolbar_actions(
    get_toolbar_actions: fn() -> Vec<ToolbarAction>
);
```

### 7. Chat Footer / Input

Products can customize the chat input area at the bottom of the screen.

#### 7.1 Types

```rust
type AccountId = [u8; 32];

enum ChatFooter {
    // Standard message input with optional payment and attachment buttons.
    DefaultChatMessageInput(DefaultChatMessageInputConfig),
    // No input area.
    None,
    // Custom rendering using the existing SerializedCustomChatMessage system.
    Custom(fn(SerializedCustomChatMessage)),
}

struct DefaultChatMessageInputConfig {
    payment: ChatInputPayment,
    attachments: ChatInputAttachments,
}

enum ChatInputPayment {
    Disabled,
    Enabled {
        // The account that receives the payment.
        destination: AccountId,
    },
}

struct ChatInputAttachments {
    photo: bool,
    video: bool,
}
```

#### 7.2 Methods

```rust
// Register a custom chat footer configuration.
// If not called, Host uses DefaultChatMessageInput with payment disabled
// and default attachment settings.
fn handle_chat_footer(footer: ChatFooter);
```

### 8. Participants

Products can customize the participant list. The default is two participants: the user and the Product.

#### 8.1 Types

```rust
PersonId = u32

enum ChatParticipant {
    // Verified person with a numeric ID from the People Registry.
    // Has access to Ring VRF proofs.
    Person(PersonId),

    // Registered app user identified by AccountId.
    // Not yet verified through the People Registry.
    LitePerson(AccountId),

    // Entry in the user's address book. Host infers
    // appearance (nickname, avatar) from the contact book.
    Contact(AccountId),

    // Another Product acting as a participant.
    Product(ProductId),

    // Arbitrary participant with explicit appearance.
    Custom {
        nickname: String,
        avatar: ResourceUri,
    },
}
```

#### 8.2 Methods

```rust
// Optional. Register handlers for participant data.
// Default: two participants — the user and the Product.
fn handle_chat_participants(
    get_participants_count: fn() -> u32,
    get_participants_page: fn(
        anchor: Option<PaginationAnchor>,
        page_size: u32
    ) -> Page<Vec<ChatParticipant>>,
    get_participants_by_ids: fn(
        ids: Vec<ChatParticipantId>
    ) -> Vec<ChatParticipant>,
);
```

### 9. Participant Actions

Products can customize actions available when tapping on a participant.

```rust
enum ParticipantActions {
    // Product takes full control.
    Custom(Vec<ProductParticipantAction>),
    // Host shows built-in actions plus product-defined actions.
    ExtendBuiltIn(Vec<ProductParticipantAction>),
}

enum BuiltInParticipantAction {
    OpenContact(AccountId),
    OpenProductInfo(ProductId),
}

struct ProductParticipantAction {
    label: String,
    handler: ProductParticipantActionHandler,
}

enum ProductParticipantActionHandler {
    Deeplink(Deeplink),
    Callback(fn(ChatParticipant)),
}

// Optional. Register a handler for participant actions.
// Default: Host shows built-in actions based on participant type.
fn handle_participant_actions(
    get_participant_actions: fn(ChatParticipant) -> ParticipantActions
);
```

### 10. Default Behaviors Summary

When a Product does **not** register a handler, the Host applies these defaults:

| Aspect | Default Behavior |
|---|---|
| Room preview | Ordered by timestamp, unread count badge, last message as preview text. Custom messages use `preview_text` if provided, otherwise "Sent a message" |
| Message source | Host stores and fetches messages from its local DB |
| Unread count | Derived from read status vs total message count |
| Message actions | Copy, Reply, Edit (own messages only), Reaction (all emoji) |
| Toolbar actions | None |
| Chat footer | Default input, payment disabled, default attachments |
| Participants | Two participants: user and Product |
| Participant actions | Based on participant type (open contact, open product info) |
| Reaction details | Not available — no "see who reacted" UI |
| Placeholder | None |

### 11. Transport & Serialization

This RFC follows the same transport and serialization rules as the Host API protocol:

- Messages are serialized using JAM codec.
- Request/response and subscription patterns follow the rules defined in the Host API design document (Section: Transport).
- All `handle_*` functions follow the subscription pattern: `_start`, `_stop`, `_interrupt`, `_receive`.
- All `on_*` functions follow the request pattern: `_request`, `_response`.
- `chat_room_create`, `chat_room_delete` follow the request pattern.
- `chat_room_list_subscribe` follows the subscription pattern.

### Requirements

#### Functional Requirements

1. **Room management**: Create, delete, and list chat rooms. Each room is scoped to a Product.
2. **Room metadata**: Name, avatar, description — updatable at any time by the Product.
3. **Room preview**: Configurable appearance in the chat list — ordering (timestamp, pin-to-top), badge (unread count or custom image), preview text with typing indicator support.
4. **Text messaging**: Plain text and rich text (Markdown) messages.
5. **Media attachments**: Image, file, and video attachments within messages.
6. **Custom message types**: Arbitrary binary message types for Product-specific content.
7. **Message editing**: Edit message content after sending.
8. **Message deletion**: Delete messages.
9. **Reply threads**: Reply to a specific message.
10. **Emoji reactions**: Place and remove emoji reactions on messages. Aggregated counts displayed inline; detailed reaction lists available on demand.
11. **Unread counter**: Track and display unread message count per room — either Host-derived or Product-controlled.
12. **Message status**: Track message lifecycle (preparing, delivered, read).
13. **Typing indicator**: Preview text with `ongoing` flag for "User is typing..." style animations.
14. **Participants**: List, paginate, and resolve participants. Support multiple identity types (Person, LitePerson, Contact, Product, Custom).
15. **Per-message actions**: Configurable long-press menu — built-in actions (copy, reply, edit, react) extensible with Product-defined actions.
16. **Toolbar actions**: Product-defined action buttons in the chat screen header.
17. **Customizable input**: Configurable chat footer — default input with optional payment/attachment buttons, no input, or fully custom.
18. **Participant actions**: Configurable actions when tapping on a participant.
19. **Chat placeholder**: Custom empty-state content for rooms with no messages.

#### Non-Functional Requirements

1. **Scalability**: Must support large chats with 10,000+ members without degrading performance.
2. **Pagination**: Messages and participants are paginated — the Host never attempts to load or store entire histories.
3. **Cacheability**: Host MAY cache pages of messages for performance but MUST NOT treat the cache as authoritative.
4. **Responsiveness**: Incremental feed updates (`on_message_feed_update`) allow the Host to update the UI without re-fetching entire pages.
5. **Graceful defaults**: A Product with zero handler registrations gets a fully functional basic chat out of the box.

#### Technical Constraints

1. **Serialization**: All types are serialized using JAM codec.
2. **Transport patterns**: All `handle_*` functions follow the subscription pattern (`_start`, `_stop`, `_interrupt`, `_receive`). All `on_*` functions and `chat_room_create`/`chat_room_delete` follow the request pattern (`_request`, `_response`).
3. **Product script lifecycle**: Products are JS scripts executed from scratch on each app start. The Host MUST delay rendering until initialization completes.
4. **Room ID scoping**: Room IDs are computed by the Host via `blake2b("product" ++ productId ++ room_id_source)` to guarantee cross-Product uniqueness.
5. **Resource URIs**: Binary resources are referenced via inline Base64 or Preimage CID — no direct URL references.

## Drawbacks

1. **Increased complexity for Product developers**: Products that want custom chat behavior must implement multiple handlers. The defaults mitigate this for simple use cases, but the API surface is large.

2. **Host caching ambiguity**: When the Product is the message source, the Host MAY cache but has no strict contract for cache invalidation. This could lead to stale data in edge cases.

3. **Initialization ordering**: The requirement to delay rendering until script initialization completes adds latency to the first chat screen render.

## Alternatives

- **Chat Extension v1** (Host API v0.4–v0.5): Product could create rooms and post messages, but the Host owned the message store. Bot registration was supported but is deferred to a future RFC.
- **Telegram Bot API**: Similar pattern where bots define commands, inline keyboards, and callback handlers. This RFC's action system (built-in + product actions) is analogous.
- **Matrix protocol**: Federated messaging with extensible event types. The `MessageContent` enum with `Custom` variant follows a similar extensibility model.

## Unresolved Questions

1. **Full message editing**: Should it be possible to edit all fields of a message (origin, timestamp), or only content? Current design only allows content edits.

2. **Bot extension**: v1 defined bot registration (`host_chat_register_bot`). The bot concept needs to be revisited in the context of v2's handler model. Deferred to a future RFC.

## Related Discussions

1. **Payment messages**: The payment button in chat input is in scope, but the full payment message type and rendering is a separate RFC. The `MessageContent` enum may need a `Payment` variant in the future.

2. **Custom message rendering**: `Custom(CustomMessageType, CustomMessagePayload)` messages use the existing `SerializedCustomChatMessage` rendering system. The details of that system are outside the scope of this RFC.

## References

- [Host API Design Document v0.5](https://docs.google.com/document/d/1AxKjF15y7gmdl-a6twc5wd8R5xcxKxMO8Ahp2l20v0g/edit?usp=sharing)
- [Triangle JS SDKs](https://github.com/Polkadot-Community-Foundation/triangle-js-sdks/tree/main/packages/host-api)
- [Chat Extension v1 Issue #41](https://github.com/paritytech/triangle-js-sdks/issues/41)
