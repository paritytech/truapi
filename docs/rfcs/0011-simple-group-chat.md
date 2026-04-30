---
title: "Simple Group Chat"
owner: "@filvecchiato"
---

# RFC 0011 — Simple Group Chat

## Summary

This RFC introduces `host_chat_create_simple_group`, a lightweight method for products to create group chat rooms where the host owns the full UI and message rendering. Unlike product-controlled rooms (`host_chat_create_room`), simple groups require no custom rendering, bot registration, or action handling — the host provides a standard group chat experience out of the box.

## Motivation

The current chat API is designed around product-controlled rooms: the product registers a room, posts messages, handles actions, and optionally provides custom renderers. This model is powerful but heavyweight for the common case where a product simply wants users to talk to each other in a shared context.

Examples:

- **Event chat** — a ticketing product creates a group for attendees of a specific event.
- **DAO discussion** — a governance product creates a group for token holders participating in a proposal.
- **Game lobby** — a gaming product creates a chat for players in a match.

In all these cases the product does not need to control the message flow or render custom UI. It only needs to create a room, give it a name, and hand users a way to join. Today this requires the full room extension flow (`host_chat_create_room` + `host_chat_post_message` + action subscriptions), which is unnecessary overhead.

A dedicated simple group method lets products spin up standard group chats with minimal integration effort, while the host retains full control over the chat experience (rendering, moderation, notifications).

## Detailed Design

### API

```rust
fn host_chat_create_simple_group(
    request: SimpleGroupChatRequest
) -> Result<SimpleGroupChatResult, ChatRoomRegistrationErr>
```

#### Request

```rust
struct SimpleGroupChatRequest {
    /// Stable, product-scoped identifier for this group. Must be unique per product.
    /// Re-creating with the same group_id returns the existing room (idempotent).
    group_id: str,
    /// Human-readable group name displayed in the contact list.
    name: str,
}
```

#### Response

```rust
struct SimpleGroupChatResult {
    /// Current registration status.
    status: ChatRoomRegistrationStatus,
    /// A shareable link that other users can use to join the group.
    join_link: str
}
```

`ChatRoomRegistrationStatus` (`New` | `Exists`) and `ChatRoomRegistrationErr` (`PermissionDenied` | `Unknown`) are reused from the existing room registration API.

### Behavioral Requirements

1. **Idempotency** — if the product calls `host_chat_create_simple_group` with a `group_id` that already exists for that product, the host returns `Exists` status and the same `join_link`. The host MUST NOT create a duplicate room.

2. **Host-owned UI** — the host renders the group using its standard chat UI. The product cannot post messages, subscribe to actions, or provide custom renderers for simple groups. The group behaves like a native host chat room from the user's perspective.

3. **Join link** — the host generates a `join_link` that the product can share (e.g. via deep link, QR code, or in-app navigation). The join mechanism and link format are host-defined. The link should remain stable for the lifetime of the group.

4. **Visibility** — simple groups appear in `host_chat_list_subscribe` with `participating_as: RoomHost`, so the product can track which groups it has created.

5. **Lifecycle** — the group persists until explicitly removed by the host or the user. The product does not have a delete API in this version — group cleanup is deferred to the full Chat Extension v2.

### Protocol Integration

The method introduces one new request/response pair in the protocol. It reuses existing error and status types from the chat group.

```
host_chat_create_simple_group
  request:  SimpleGroupChatRequest  (Struct: group_id, name, icon)
  response: Result<SimpleGroupChatResult, ChatRoomRegistrationErr>
```

### Relationship to Existing Chat API

| Capability | `host_chat_create_room` | `host_chat_create_simple_group` |
|---|---|---|
| Product posts messages | Yes | No |
| Custom message rendering | Yes | No |
| Action subscriptions | Yes | No |
| Bot integration | Yes | No |
| Join link for participants | No | Yes |
| Host-native UI | Product-controlled | Host-controlled |

Simple groups are intentionally limited. Products that need richer control should use the existing room extension API or wait for Chat Extension v2.

## Drawbacks

1. **No product-side messaging** — products cannot post system messages, announcements, or bot responses into simple groups. If a product later needs this capability, it must migrate to a full room.

2. **No delete API** — products cannot programmatically close or archive groups. This is acceptable for v0.2 but should be addressed in Chat Extension v2.

3. **Join link dependency** — the product relies on the host to generate and manage join links. If the host's link format changes, existing shared links may break.

## Alternatives

### Reuse `host_chat_create_room` with a flag

Instead of a new method, add an `is_simple: bool` flag to `ChatRoomRequest`. Rejected because it muddies the semantics of the existing method — a "simple" room would silently ignore `host_chat_post_message` calls, which is confusing. A separate method makes the capability boundary explicit.

### Product-generated invite codes

Instead of the host providing a `join_link`, the product could generate its own invite codes and resolve them via a callback. Rejected because it adds unnecessary round-trips and couples the join flow to the product's availability. Host-managed links are simpler and work even when the product is not loaded.

## Unresolved Questions

- **Group size limits** — should the host enforce a maximum number of participants? If so, should it be configurable per group or a global host policy?
- **Group metadata updates** — should the product be able to update the group name or icon after creation? Currently not supported.
- **Group deletion** — deferred to Chat Extension v2. The mechanism (product-initiated vs. host-only) needs design.
- **Participant list** — should the product be able to query who has joined? This has privacy implications and is deferred.
