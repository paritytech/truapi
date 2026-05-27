---
title: "Push Notification Subscriptions"
type: rfc
status: draft
owner: ["@pgherveou", "@sbalaguer"]
pr:
---

# RFC 0020 — Push Notification Subscriptions

## Summary

Adds four TrUAPI methods — `push_add_rules`, `push_remove_rules`, `push_list_rules`, `push_set_rules` — that mirror the rule-management endpoints of the [v2 push backend spec](https://hackmd.io/@1JCaGppGSUqHtJilikYaKw/r16YTVg5Ze). A rule is a `(signer, topic)` pair the product specifies in full: `signer` (mandatory) is the publisher whose statements should wake the user. The backend then delivers a push to the user's device(s) whenever a signed statement matching that `(signer, topic)` pair appears on the Statement Store. The product never sees push tokens.

The method names use `add` / `remove` rather than `subscribe` / `unsubscribe` because the `_subscribe` suffix is reserved for streaming TrUAPI methods (e.g. `statementStore.subscribe`).

An **interim transport**, `push_broadcast`, distributes announcements **without using the Statement Store as the distribution layer**. The host submits the announcement to the push backend, **setting the publisher `signer` itself** (the product cannot override it), and the backend fans out using the same `(signer, topic)` rule matching. It is marked **(interim)** in the API and Types sections below.

## References

- Push notifications, original (v1, peer-to-peer): https://hackmd.io/@1JCaGppGSUqHtJilikYaKw/SyPN2yV6lx
- Push notifications backend design (v2, backend-mediated): https://hackmd.io/@1JCaGppGSUqHtJilikYaKw/r16YTVg5Ze

This RFC exposes a TrUAPI-shaped surface over the rule-management API defined in the v2 spec.

## Motivation

The push-notifications v2 design assigns delivery to a host-side notification system that tails the Statement Store, verifies signatures, and delivers pushes only for `(signer, topic)` pairs the user has whitelisted. TrUAPI needs a primitive that lets a product manipulate that whitelist. `signer` is **mandatory** on every rule: the product always names the publisher it wants.

### Worked example: festival announcements

A conference product publishes festival-wide announcements signed by the organizer. An attendee's app subscribes by calling `push_add_rules({ topics: [announcements_topic], signer: organizer_id })`, passing the organizer product's `ProductAccountId` explicitly. The organizer publishes with `push_broadcast` — the host sets the `signer` to the organizer and submits the announcement to the backend. From that point on the attendee is woken for new announcements even with the app closed:

```
Publisher app                                          Subscriber app
(organizer side)                                       (attendee side)
        |                                                       ^   |
        |                                                       |   |
        |                                              (5) push |   |  (1) push_add_rules({ topics: [T], signer: organizer_id })
        |                                               back to |   |
        |                                                caller |   |
        |                                                       |   v
        |                  +------------------------------------+---+------+
        |                  |  Host + push backend                          |
        |                  |  stores rule (organizer_id, T)                |
        |                  |  (4) match (organizer_id, T)                  |
        |                  |       -> deliver to this subscriber           |
        |                  +-----------------------+-----------------------+
        |                                          ^
        |   (2) push_broadcast({ topics: [T],      |  (3) host sets signer
        |       content })                         |      and submits to
        |------------------------------------------+      the backend
```

## Detailed Design

### API

Each TrUAPI method mirrors one backend endpoint:

| TrUAPI method       | Backend endpoint                 | Purpose                          |
| ------------------- | -------------------------------- | -------------------------------- |
| `push_add_rules`    | `POST   /v1/subscriptions/rules` | add one or more rules            |
| `push_remove_rules` | `DELETE /v1/subscriptions/rules` | remove one or more rules         |
| `push_list_rules`   | `GET    /v1/subscriptions`       | snapshot of currently active set |
| `push_set_rules`    | `PUT    /v1/subscriptions/rules` | atomic replace of the full set   |
| `push_broadcast`    | direct submit _(interim)_        | publish a signed announcement    |

```rust
#[wire(request_id = 164)]
async fn push_add_rules(
    &self, cx: &CallContext, request: HostPushAddRulesRequest,
) -> Result<HostPushAddRulesResponse, CallError<HostPushAddRulesError>>;

#[wire(request_id = 166)]
async fn push_remove_rules(
    &self, cx: &CallContext, request: HostPushRemoveRulesRequest,
) -> Result<HostPushRemoveRulesResponse, CallError<HostPushRemoveRulesError>>;

#[wire(request_id = 168)]
async fn push_list_rules(
    &self, cx: &CallContext, request: HostPushListRulesRequest,
) -> Result<HostPushListRulesResponse, CallError<HostPushListRulesError>>;

#[wire(request_id = 170)]
async fn push_set_rules(
    &self, cx: &CallContext, request: HostPushSetRulesRequest,
) -> Result<HostPushSetRulesResponse, CallError<HostPushSetRulesError>>;
```

#### Interim: direct broadcast

`push_broadcast` distributes an announcement **without using the Statement Store as the distribution layer**. The product sends only `{ topics, content }`. The host **sets the `signer` itself** — to the calling product's channel identity, host-set so the product cannot override or spoof it — and submits the announcement to the backend. The backend matches `(signer, topic)` against subscriber rules; matching, rate-limiting, dedup, and dispatch are unchanged — only the distribution layer differs. The product never sets `signer`, which is why it is absent from the request.

```rust
#[wire(request_id = 172)]
async fn push_broadcast(
    &self, cx: &CallContext, request: HostPushBroadcastRequest,
) -> Result<HostPushBroadcastResponse, CallError<HostPushBroadcastError>>;
```

### Types

`Topic` is reused from `v01::statement_store`.

A rule is a `(signer, topic)` pair. `signer` is **mandatory**: the subscriber always names the publisher.

```rust
pub struct HostPushAddRulesRequest    { pub topics: Vec<Topic>, pub signer: ProductAccountId }
pub struct HostPushRemoveRulesRequest { pub topics: Vec<Topic>, pub signer: ProductAccountId }
pub struct HostPushListRulesRequest;
pub struct HostPushSetRulesRequest    { pub topics: Vec<Topic>, pub signer: ProductAccountId }

pub struct HostPushListRulesResponse {
    pub topics: Vec<Topic>,
}

pub enum HostPushAddRulesError {
    /// The user has not granted `DevicePermission::Notifications`. The host
    /// SHOULD prompt for the permission lazily on the first such call from
    /// a product; if the user dismisses or declines, this variant is
    /// returned and no rules are stored.
    PermissionDenied,
    /// The notification system is currently unavailable; no rules were stored.
    NotificationSystemUnavailable(String),
    /// Catch-all. `reason`
    Unknown { reason: String },
}

pub enum HostPushRemoveRulsError {
    NotificationSystemUnavailable(String),
    Unknown { reason: String },
}

pub enum HostPushListRulesError {
    NotificationSystemUnavailable(String),
    Unknown { reason: String },
}

pub enum HostPushSetRulesError {
    PermissionDenied,
    NotificationSystemUnavailable(String),
    Unknown { reason: String },
}
```

#### Interim: direct broadcast

The broadcast is **not** a Statement Store statement: it is a plain `{ topics, content }` the host submits with a host-set `signer`, so there is no `channel`, topic slots, or `expiry`. A later version can move distribution to the Statement Store without changing subscriber rules.

```rust
pub struct PushBroadcastContent {
    pub title: String,
    pub body: String,
    pub deeplink: Option<String>,   // route/URL to open on tap
}

pub struct HostPushBroadcastRequest {
    pub topics: Vec<Topic>,         // matched against subscriber rules (signer = caller)
    pub content: PushBroadcastContent,
}

pub struct HostPushBroadcastResponse {
    pub message_hash: [u8; 32],     // Blake2b-256 of the broadcast (dedup / audit)
}

pub enum HostPushBroadcastError {
    NotificationSystemUnavailable(String),
    Unknown { reason: String },
}
```
