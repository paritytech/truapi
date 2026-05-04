---
title: "Private Chat Host API"
owner: "@replghost"
---

# RFC 0013 — Private Chat Host API

## Summary

This RFC introduces a private wallet-to-wallet chat API for TrUAPI. The API lets a product open a native host chat experience, resolve usernames, send first-contact requests, accept incoming requests, send encrypted messages, and receive chat events without exposing chat private keys or statement-store protocol details to the product.

The API is intended to standardize the private chat host-extension shape being
tested in the dotli/chat.dot proof-of-concept PR. It is not yet a shipped TrUAPI
surface.

## Motivation

TrUAPI currently includes host-owned product chat rooms, simple group chats, bots, and custom chat rendering. Those APIs are useful for product-scoped rooms, but they do not cover private user-to-user messaging.

Private chat needs a different trust boundary:

- The product should not receive P-256 private keys, session secrets, statement signing keys, or decrypted transport internals.
- The host should perform username lookup, identity lookup, encryption, signing, statement-store submission, and statement-store subscriptions.
- Hosted products such as chat.dot should be able to use the same private chat flow as mobile hosts.
- Desktop/web hosts need a safe way to delegate a bounded per-device chat key from the user's main wallet identity so messages display as the user's real identity, not a temporary product/session account.
- Hosts such as dotli should be able to keep core host responsibilities lean by implementing chat as a product SPA, while still retaining host control over keys, permissions, signing, encryption, and transport.

Without a standardized API, each host and SPA must invent its own bridge. That creates compatibility drift across the dotli proof of concept, mobile hosts, and future TrUAPI hosts.

## Detailed Design

### TrUAPI Spec Shape

The normative protocol shape should be added to `truapi-spec/src/v02/mod.rs`
or a future `v03` module as Rust trait methods and Rust data types. The
JavaScript names map mechanically from these trait methods.

```rust
// ─── Private chat types ─────────────────────────────────────────────────────

/// Private chat conversation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivateChatConversationState {
    None,
    RequestSent,
    RequestReceived,
    Active,
}

/// Active peer-facing private chat identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateChatIdentity {
    /// Account ID peers see for this chat identity.
    pub account_id: AccountId,
    /// Resolved username, when available.
    pub username: Option<String>,
    /// 65-byte uncompressed P-256 public key used for chat ECDH.
    pub identifier_key: PublicKey,
}

/// Resolved peer private chat identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateChatPeerResolution {
    pub peer_account_id: AccountId,
    pub identifier_key: Option<PublicKey>,
    pub username: Option<String>,
    pub state: Option<PrivateChatConversationState>,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateChatConversationStateResult {
    pub state: PrivateChatConversationState,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateChatRequestResult {
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateChatAcceptResult {
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateChatMessageResult {
    pub message_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivateChatDelegatedStatementKind {
    ChatRequest,
    ChatAccept,
    ChatMessage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateChatDelegationRequest {
    pub app_id: Option<String>,
    pub expires_in_seconds: Option<u64>,
    pub allowed_statement_kinds: Option<Vec<PrivateChatDelegatedStatementKind>>,
    pub max_messages_per_minute: Option<u32>,
    pub max_open_requests_per_hour: Option<u32>,
    pub peer_scope: Option<Vec<AccountId>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateChatDelegationGrant {
    pub id: String,
    pub account_id: AccountId,
    pub username: Option<String>,
    pub device_public_key: PublicKey,
    pub delegate_account_id: Option<AccountId>,
    pub app_id: String,
    pub session_id: Option<String>,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub allowed_statement_kinds: Vec<PrivateChatDelegatedStatementKind>,
    pub max_messages_per_minute: Option<u32>,
    pub max_open_requests_per_hour: Option<u32>,
    pub peer_scope: Option<Vec<AccountId>>,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateChatDelegationStatus {
    pub supported: bool,
    pub active_grant: Option<PrivateChatDelegationGrant>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivateChatContentType {
    Text,
    ChatAccepted,
    ContactAdded,
    Reacted,
    ReactionRemoved,
    Reply,
    Edited,
    LeftChat,
    RichText,
    Payment,
    Token,
    DataChannelOffer,
    DataChannelAnswer,
    DataChannelIceCandidates,
    DataChannelClosed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateChatMessageEvent {
    pub peer_account_id: AccountId,
    pub message_id: String,
    pub timestamp_ms: u64,
    pub content_type: PrivateChatContentType,
    pub text: Option<String>,
    pub request_id: Option<String>,
    pub referenced_message_id: Option<String>,
    pub emoji: Option<String>,
    pub amount: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivateChatDeliveryStatus {
    Sent,
    Acknowledged,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateChatDeliveryStatusEvent {
    pub message_id: String,
    pub status: PrivateChatDeliveryStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateChatRequestEvent {
    pub peer_account_id: AccountId,
    pub request_id: String,
    pub welcome_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivateChatError {
    PermissionDenied,
    NotConnected,
    Unsupported,
    PeerNotFound,
    InvalidRequest,
    Rejected,
    RateLimited,
    TransportFailed { reason: String },
    Unknown { reason: String },
}
```

The `TrUApi` trait addition should follow the existing request-response and
subscription patterns:

```rust
pub trait TrUApi {
    type Subscription;

    // existing methods...

    fn host_private_chat_identity_get(
        &self,
    ) -> Result<PrivateChatIdentity, PrivateChatError>;

    fn host_private_chat_username_resolve(
        &self,
        username: String,
    ) -> Result<Option<AccountId>, PrivateChatError>;

    fn host_private_chat_peer_resolve(
        &self,
        peer_account_id: AccountId,
    ) -> Result<PrivateChatPeerResolution, PrivateChatError>;

    fn host_private_chat_conversation_open(
        &self,
        peer_account_id: AccountId,
    ) -> Result<PrivateChatConversationStateResult, PrivateChatError>;

    fn host_private_chat_request_send(
        &self,
        peer_account_id: AccountId,
        welcome_text: Option<String>,
    ) -> Result<PrivateChatRequestResult, PrivateChatError>;

    fn host_private_chat_accept_send(
        &self,
        peer_account_id: AccountId,
        request_id: String,
        accepted_text: Option<String>,
    ) -> Result<PrivateChatAcceptResult, PrivateChatError>;

    fn host_private_chat_message_send(
        &self,
        peer_account_id: AccountId,
        text: String,
    ) -> Result<PrivateChatMessageResult, PrivateChatError>;

    fn host_private_chat_conversation_state_get(
        &self,
        peer_account_id: AccountId,
    ) -> Result<PrivateChatConversationStateResult, PrivateChatError>;

    fn host_private_chat_delegation_get(
        &self,
    ) -> Result<PrivateChatDelegationStatus, PrivateChatError>;

    fn host_private_chat_delegation_request(
        &self,
        request: Option<PrivateChatDelegationRequest>,
    ) -> Result<PrivateChatDelegationGrant, PrivateChatError>;

    fn host_private_chat_delegation_revoke(
        &self,
        grant_id: Option<String>,
    ) -> Result<(), PrivateChatError>;

    fn host_private_chat_message_subscribe(
        &self,
        callback: Box<dyn FnMut(PrivateChatMessageEvent) + Send>,
    ) -> Self::Subscription;

    fn host_private_chat_delivery_status_subscribe(
        &self,
        callback: Box<dyn FnMut(PrivateChatDeliveryStatusEvent) + Send>,
    ) -> Self::Subscription;

    fn host_private_chat_request_subscribe(
        &self,
        callback: Box<dyn FnMut(PrivateChatRequestEvent) + Send>,
    ) -> Self::Subscription;
}
```

The dotli/chat.dot proof-of-concept JavaScript bridge maps these trait methods onto:

```ts
window.ua.ext.chat.identityGet()
window.ua.ext.chat.usernameResolve(username)
window.ua.ext.chat.peerResolve(peerAccountId)
window.ua.ext.chat.conversationOpen(peerAccountId)
window.ua.ext.chat.requestSend(peerAccountId, welcomeText?)
window.ua.ext.chat.acceptSend(peerAccountId, requestId, acceptedText?)
window.ua.ext.chat.messageSend(peerAccountId, text)
window.ua.ext.chat.conversationStateGet(peerAccountId)
window.ua.ext.chat.delegationGet()
window.ua.ext.chat.delegationRequest(request?)
window.ua.ext.chat.delegationRevoke(grantId?)
window.ua.on("chatMessage", callback)
window.ua.off("chatMessage", callback)
window.ua.on("chatDeliveryStatus", callback)
window.ua.off("chatDeliveryStatus", callback)
window.ua.on("chatRequest", callback)
window.ua.off("chatRequest", callback)
```

The Rust names intentionally use `host_private_chat_*` to avoid ambiguity with
the existing product-room chat APIs such as `host_chat_create_room` and
`host_chat_create_simple_group`.

The proof-of-concept currently uses `requestPrepare`, `acceptPrepare`, and
`messagePrepare`. The standardized API SHOULD use `requestSend`, `acceptSend`,
and `messageSend` because the host owns encryption, signing, and submission.
The product receives only stable identifiers, not a submit-capable statement
payload.

### Namespace

The JavaScript host extension is exposed as:

```ts
window.ua.ext.chat
```

For older hosts, `window.host.ext.chat` may be supported as a compatibility alias, but `window.ua.ext.chat` is canonical.

### Permission

Add a chat permission to the host permission model:

```rust
enum RemotePermission {
    // existing variants...
    ChatPrivate,
}
```

Products declare the capability in their manifest. The exact manifest key is
open for bikeshedding, but the intended shape is:

```toml
[permissions]
private_chat = true
```

The host MUST deny chat access by default. A product can only use this API after user approval. Hosts SHOULD present the requesting product identity and explain that the product can ask the host to send and receive encrypted chat messages on the user's behalf.

Private chat is a scoped host capability. Hosts MUST NOT expose
`host_private_chat_*` methods or private-chat events to a product unless that
specific product has been granted private chat permission.

This permission is independent from statement-store, raw signing, remote
networking, and product-room chat permissions. Granting private chat permission
does not grant direct statement-store submission, arbitrary signing, arbitrary
remote networking, or access to product-owned chat room APIs.

The grant is product-scoped and host-enforced. A user approving private chat for
one product does not approve private chat for other products embedded in the
same host. If multiple products or iframes are present, the host MUST route chat
responses and events only to the approved product frame/session.

If scoped delegation is supported, it is an additional wallet-signed constraint
on top of host permission. The host permission allows the product to request
private chat operations; the delegation grant determines whether the active
device/session may represent the user's main wallet identity, for which peers,
for which statement kinds, and for how long.

### API Methods

#### `host_private_chat_identity_get`

Returns the active peer-facing private chat identity.

```rust
fn host_private_chat_identity_get() -> Result<PrivateChatIdentity, PrivateChatError>
```

`account_id` is the user identity that peers see. Mobile-native hosts commonly use the wallet chat account. Hosted products SHOULD use the main wallet account when scoped delegation is active. `identifier_key` is the active 65-byte uncompressed P-256 public key used for chat ECDH.

#### `host_private_chat_username_resolve`

Resolves a username to an account ID.

```rust
fn host_private_chat_username_resolve(username: String) -> Result<Option<AccountId>, PrivateChatError>
```

#### `host_private_chat_peer_resolve`

Resolves a peer account ID to the peer's current chat identity.

```rust
fn host_private_chat_peer_resolve(peer_account_id: AccountId) -> Result<PrivateChatPeerResolution, PrivateChatError>
```

The host MAY resolve `identifier_key` from People-chain identity records, a signed active device announcement, or another host-supported discovery source. The product does not choose or verify the discovery source.

#### `host_private_chat_conversation_open`

Opens or refreshes host-side session state and subscriptions for a peer.

```rust
fn host_private_chat_conversation_open(peer_account_id: AccountId) -> Result<PrivateChatConversationStateResult, PrivateChatError>
```

The host MUST ensure that future `PrivateChatMessageEvent`, `PrivateChatRequestEvent`, and `PrivateChatDeliveryStatusEvent` updates for this conversation can be delivered to the approved product session while the subscription is active. Hosts MAY also perform this setup from `peer_resolve`, `request_send`, `accept_send`, or `message_send`, but `conversation_open` is the explicit lifecycle hook.

#### `host_private_chat_request_send`

Creates, encrypts, signs, and submits a first-contact chat request.

```rust
fn host_private_chat_request_send(
    peer_account_id: AccountId,
    welcome_text: Option<String>,
) -> Result<PrivateChatRequestResult, PrivateChatError>
```

The product receives a `request_id`. It does not receive a submit-capable statement payload.

#### `host_private_chat_accept_send`

Creates, encrypts, signs, and submits an acceptance for an incoming chat request.

```rust
fn host_private_chat_accept_send(
    peer_account_id: AccountId,
    request_id: String,
    accepted_text: Option<String>,
) -> Result<PrivateChatAcceptResult, PrivateChatError>
```

#### `host_private_chat_message_send`

Creates, encrypts, signs, and submits a text chat message.

```rust
fn host_private_chat_message_send(
    peer_account_id: AccountId,
    text: String,
) -> Result<PrivateChatMessageResult, PrivateChatError>
```

The product receives a `message_id`. It does not receive plaintext transport internals or a submit-capable statement payload.

#### `host_private_chat_conversation_state_get`

Returns the host's current conversation state for a peer.

```rust
fn host_private_chat_conversation_state_get(
    peer_account_id: AccountId,
) -> Result<PrivateChatConversationStateResult, PrivateChatError>
```

### Optional Scoped Delegation Methods

Hosted products often cannot keep the user's main wallet key online. To avoid showing peers a temporary product/session identity, hosts MAY support scoped delegation.

```rust
fn host_private_chat_delegation_get() -> Result<PrivateChatDelegationStatus, PrivateChatError>

fn host_private_chat_delegation_request(
    request: Option<PrivateChatDelegationRequest>,
) -> Result<PrivateChatDelegationGrant, PrivateChatError>

fn host_private_chat_delegation_revoke(
    grant_id: Option<String>,
) -> Result<(), PrivateChatError>
```

The host MUST ensure grants are bounded by expiry and usage limits. The host SHOULD revoke or invalidate an active grant when the user signs out, switches wallet identity, revokes product permission, or locks/removes the session secrets required for chat.

### Events

The host emits events through the standard event/subscription mechanism. JavaScript hosts expose:

```ts
window.ua.on("chatMessage", callback)
window.ua.off("chatMessage", callback)
window.ua.on("chatDeliveryStatus", callback)
window.ua.off("chatDeliveryStatus", callback)
window.ua.on("chatRequest", callback)
window.ua.off("chatRequest", callback)
```

```rust
struct PrivateChatMessageEvent {
    peer_account_id: AccountId,
    message_id: String,
    timestamp_ms: u64,
    content_type: PrivateChatContentType,
    text: Option<String>,
    request_id: Option<String>,
    referenced_message_id: Option<String>,
    emoji: Option<String>,
    amount: Option<String>,
}

enum PrivateChatContentType {
    Text,
    ChatAccepted,
    ContactAdded,
    Reacted,
    ReactionRemoved,
    Reply,
    Edited,
    LeftChat,
    RichText,
    Payment,
    Token,
    DataChannelOffer,
    DataChannelAnswer,
    DataChannelIceCandidates,
    DataChannelClosed,
    Unknown,
}

struct PrivateChatDeliveryStatusEvent {
    peer_account_id: Option<AccountId>,
    message_id: String,
    status: PrivateChatDeliveryStatus,
    reason: Option<String>,
}

enum PrivateChatDeliveryStatus {
    Sent,
    Acknowledged,
    Failed,
}

struct PrivateChatRequestEvent {
    peer_account_id: AccountId,
    request_id: String,
    welcome_message: Option<String>,
}
```

Events are live subscription events unless a host explicitly documents replay
semantics. Hosts MAY also update local conversation state before emitting events
so that `host_private_chat_conversation_state_get` reflects the pushed event.

### JavaScript Encoding

The JavaScript bridge SHOULD use stable string encodings:

- `AccountId` and public keys are lowercase hex strings, preferably `0x` prefixed.
- Timestamps are milliseconds since Unix epoch.
- Opaque diagnostic bytes, if any are ever exposed, are base64 strings.
- User-visible text is UTF-8.

### Security Requirements

- Products MUST NOT receive private chat keys, session secrets, statement signing keys, shared secrets, or raw decrypted transport internals.
- Products MUST NOT submit unsigned or forged chat statements directly through this API.
- Hosts MUST enforce user approval before exposing chat methods or events.
- Hosts MUST scope chat access to the requesting product.
- Hosts SHOULD rate-limit outbound chat requests and messages.
- Hosts SHOULD avoid logging peer IDs, topics, channels, ciphertext, plaintext message bodies, or grant payloads unless an explicit local debug mode is enabled.
- Delegation grants MUST be signed by the wallet identity they represent.
- Delegated device announcements MUST be verifiable by peers before routing first-contact messages to the delegated device key.

### Relationship to Existing APIs

This RFC is separate from `host_chat_create_room`, `host_chat_create_simple_group`, `host_chat_register_bot`, and `host_chat_post_message`.

Those APIs are product-room APIs. This RFC is a private user-to-user messaging API where the host owns cryptography, transport, and identity resolution.

This API can be implemented using lower-level TrUAPI statement-store methods internally, but products SHOULD NOT need direct statement-store access to use private chat.

One intended implementation model is a chat product SPA hosted by dotli or
another TrUAPI host. In that model, the host does not need to embed the full
chat application UI in its core shell. The host provides only the narrow private
chat capability boundary: permissioning, identity, encryption, signing,
statement-store transport, and event delivery. The product owns chat UI and
local product state.

## Drawbacks

- The API adds a second chat surface alongside product-room chat, which can be confusing unless naming and documentation clearly distinguish "private chat" from "product chat rooms".
- Hosts must implement non-trivial crypto, statement-store subscription, identity lookup, and device-delegation behavior.
- Scoped delegation introduces lifecycle complexity: expiry, revocation, sign-out, wallet switching, and multi-device state must be handled carefully.
- The `prepare` names are historical. They may imply that products submit the returned statement, while the current host behavior is prepare-and-submit.

## Alternatives

### Use statement-store primitives directly

Products could use `remote_statement_store_create_proof`, `remote_statement_store_submit`, and `remote_statement_store_subscribe` directly. Rejected because it exposes too much chat protocol surface to products and risks key/crypto duplication.

### Use raw signing for every message

The SPA could call raw signing for each chat request or message. Rejected because it creates poor UX, requires the paired wallet to be online for every message, and does not match mobile-native local chat behavior.

### Reuse product-room chat APIs

Private chat could be modeled as product-created rooms. Rejected because peer-to-peer wallet chat has different identity, discovery, encryption, and permission boundaries.

### Keep this as a dotli-only extension

Rejected because the same private chat product should work across dotli, mobile hosts, desktop hosts, and future TrUAPI-compatible hosts.

## Unresolved Questions

- Should method names keep the experimental `prepare` suffix, or should the standardized TrUAPI names use `send`/`accept` to reflect host submission?
- Should scoped delegation be part of v0.3 immediately, or documented as an optional capability first?
- What exact permission enum name should be used: `ChatPrivate`, `Chat`, or a more granular permission set?
- Should username claiming be included here, or remain outside private chat because username registration is an identity/auth flow?
- What is the canonical signed payload for device announcements, and should it be specified in this RFC or in a lower-level chat protocol RFC?
- How should multi-device delivery be represented once a username can route to multiple active device keys?
