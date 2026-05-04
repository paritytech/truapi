---
title: "Private Chat Host API"
owner: "@replghost"
---

# RFC 0013 — Private Chat Host API

## Summary

This RFC proposes a minimal host API for private wallet-to-wallet chat. A chat
product such as chat.dot can render the UI and call a small set of host methods,
while the host keeps ownership of permissioning, identity lookup, peer routing,
key material, encryption, signing, statement-store submission, and event
delivery.

This proposal is based on the dotli/chat.dot proof of concept. It is not yet a
shipped TrUAPI surface.

## Motivation

TrUAPI already has product-room chat APIs such as `host_chat_create_room` and
simple group chat. Those APIs are useful for product-owned rooms, but they do
not cover private user-to-user messaging.

Private chat has a different trust boundary. Products should not receive P-256
private keys, shared secrets, statement signing keys, decrypted transport
internals, or raw statement-store protocol details. The host should perform the
sensitive work and expose only the operations a chat UI needs:

1. discover the local chat identity,
2. resolve a username or peer account,
3. send or accept a first-contact request,
4. send a text message,
5. subscribe to chat events.

This lets chat.dot remain a product SPA, keeping hosts such as dotli lean, while
still giving the host full control over user approval and private-chat
capabilities.

## Detailed Design

### Permission

Private chat is a separate host capability and MUST be denied by default.
Products declare the capability in their manifest:

```toml
[permissions]
private_chat = true
```

Hosts MUST NOT expose private-chat methods or events until the user grants this
permission to the requesting product. The grant is product-scoped and separate
from statement-store, raw signing, remote networking, and product-room chat
permissions. If multiple products or iframes are present, chat responses and
events MUST be delivered only to the approved product session.

### API Calls

The normative protocol shape should be added to `truapi-spec/src/v02/mod.rs` or
a future `v03` module as Rust trait methods and Rust data types. The exact Rust
module location is left to the implementing PR.

```rust
enum PrivateChatConversationState {
    None,
    RequestSent,
    RequestReceived,
    Active,
}

struct PrivateChatIdentity {
    /// Account ID peers see for this chat identity.
    account_id: AccountId,
    /// Resolved username, when available.
    username: Option<String>,
    /// Public peer identifier used by the host chat protocol.
    identifier_key: PublicKey,
}

struct PrivateChatPeer {
    account_id: AccountId,
    username: Option<String>,
    identifier_key: Option<PublicKey>,
    state: PrivateChatConversationState,
    request_id: Option<String>,
}

struct PrivateChatRequestReceipt {
    request_id: String,
}

struct PrivateChatMessageReceipt {
    message_id: String,
}

enum PrivateChatContentType {
    Text,
    ChatAccepted,
    Unknown,
}

struct PrivateChatMessageEvent {
    peer_account_id: AccountId,
    message_id: String,
    timestamp_ms: u64,
    content_type: PrivateChatContentType,
    text: Option<String>,
    request_id: Option<String>,
}

enum PrivateChatDeliveryStatus {
    Sent,
    Acknowledged,
    Failed,
}

struct PrivateChatDeliveryStatusEvent {
    peer_account_id: Option<AccountId>,
    message_id: String,
    status: PrivateChatDeliveryStatus,
    reason: Option<String>,
}

struct PrivateChatRequestEvent {
    peer_account_id: AccountId,
    request_id: String,
    welcome_message: Option<String>,
}

enum PrivateChatErr {
    PermissionDenied,
    NotConnected,
    Unsupported,
    PeerNotFound,
    PeerUnavailable,
    InvalidRequest,
    Rejected,
    RateLimited,
    TransportFailed(String),
    Unknown(GenericErr),
}

trait TrUApi {
    type Subscription;

    fn host_private_chat_identity_get(
        &self,
    ) -> Result<PrivateChatIdentity, PrivateChatErr>;

    fn host_private_chat_username_resolve(
        &self,
        username: String,
    ) -> Result<Option<AccountId>, PrivateChatErr>;

    fn host_private_chat_peer_resolve(
        &self,
        peer_account_id: AccountId,
    ) -> Result<PrivateChatPeer, PrivateChatErr>;

    fn host_private_chat_request_send(
        &self,
        peer_account_id: AccountId,
        welcome_text: Option<String>,
    ) -> Result<PrivateChatRequestReceipt, PrivateChatErr>;

    fn host_private_chat_accept_send(
        &self,
        peer_account_id: AccountId,
        request_id: String,
        accepted_text: Option<String>,
    ) -> Result<PrivateChatRequestReceipt, PrivateChatErr>;

    fn host_private_chat_message_send(
        &self,
        peer_account_id: AccountId,
        text: String,
    ) -> Result<PrivateChatMessageReceipt, PrivateChatErr>;

    fn host_private_chat_conversation_state_get(
        &self,
        peer_account_id: AccountId,
    ) -> Result<PrivateChatConversationState, PrivateChatErr>;

    fn host_private_chat_message_subscribe(
        &self,
        callback: fn(PrivateChatMessageEvent),
    ) -> Result<Self::Subscription, PrivateChatErr>;

    fn host_private_chat_delivery_status_subscribe(
        &self,
        callback: fn(PrivateChatDeliveryStatusEvent),
    ) -> Result<Self::Subscription, PrivateChatErr>;

    fn host_private_chat_request_subscribe(
        &self,
        callback: fn(PrivateChatRequestEvent),
    ) -> Result<Self::Subscription, PrivateChatErr>;
}
```

The JavaScript extension should map this to `window.ua.ext.chat`:

```ts
window.ua.ext.chat.identityGet()
window.ua.ext.chat.usernameResolve(username)
window.ua.ext.chat.peerResolve(peerAccountId)
window.ua.ext.chat.requestSend(peerAccountId, welcomeText?)
window.ua.ext.chat.acceptSend(peerAccountId, requestId, acceptedText?)
window.ua.ext.chat.messageSend(peerAccountId, text)
window.ua.ext.chat.conversationStateGet(peerAccountId)

window.ua.on("chatMessage", callback)
window.ua.on("chatDeliveryStatus", callback)
window.ua.on("chatRequest", callback)
```

### Behavioral Requirements

1. `host_private_chat_identity_get` returns the local peer-facing chat identity.
   Hosted products SHOULD use the user's main wallet identity when the host can
   safely represent it.

2. `host_private_chat_username_resolve` supports username search without making
   the product own identity indexing.

3. `host_private_chat_peer_resolve` validates that the host can route to the
   peer. If the host cannot find a routable peer identity, it returns
   `PeerUnavailable` or `PeerNotFound`.

4. `host_private_chat_request_send`, `host_private_chat_accept_send`, and
   `host_private_chat_message_send` create, encrypt, sign, and submit the
   relevant chat statement. Products receive only stable IDs, never keys or
   submit-capable statement payloads.

5. Subscriptions are live event streams for the approved product session. Hosts
   MAY replay recent events, but replay semantics are out of scope for this RFC.

6. Hosts SHOULD update conversation state before emitting events so
   `host_private_chat_conversation_state_get` reflects pushed events.

### Multi-Device Routing

The minimal product API does not need multi-device routing details. chat.dot
only needs to ask the host to resolve and message a peer; it does not need to
know whether the host routed to a phone, desktop session, delegated browser
session, or future fanout mechanism.

For this RFC, peer routing is a host responsibility behind
`host_private_chat_peer_resolve` and the send methods. The host MAY use
People-chain identity records, signed device announcements, local contacts, or
future routing metadata internally. The exact multi-device discovery and fanout
protocol is deferred.

### Encoding

The JavaScript bridge SHOULD use stable string encodings: `AccountId` and public
keys as lowercase hex strings, timestamps as milliseconds since Unix epoch, and
user-visible text as UTF-8.

## Drawbacks

1. This adds a second chat surface alongside product-room chat. Documentation
   must clearly distinguish private user-to-user chat from product-owned rooms.

2. Hosts still need to implement non-trivial identity lookup, encryption,
   signing, statement-store transport, and subscriptions.

3. Deferring multi-device routing details means hosts can align on the product
   API before the full routing protocol is standardized.

## Alternatives

### Use statement-store primitives directly

Products could call lower-level statement-store APIs directly. Rejected because
it exposes too much protocol surface to products and risks key or crypto
duplication.

### Use raw signing for every message

The SPA could ask the wallet to sign every chat request or message. Rejected
because it creates poor UX, requires the paired wallet to be online for every
message, and does not match mobile-native local chat behavior.

### Reuse product-room chat APIs

Private chat could be modeled as product-created rooms. Rejected because
peer-to-peer wallet chat has different identity, discovery, encryption, and
permission boundaries.

## Unresolved Questions

- Should the permission key be `private_chat`, `chat_private`, or another name?
- Should this land in TrUAPI v0.2 as an additive capability or wait for v0.3?
- Should username lookup be required for all hosts, or optional for hosts that
  only support raw account IDs?
- What scoped delegation and multi-device routing protocol should hosts use
  internally once there are multiple active devices for the same wallet?
