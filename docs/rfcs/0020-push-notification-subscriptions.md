---
title: "Push Notification Subscriptions"
type: rfc
status: draft
owner: "@pgherveou"
pr:
---

# RFC 0020 — Push Notification Subscriptions

## Summary

Adds four TrUAPI methods — `push_add_rules`, `push_remove_rules`, `push_list_rules`, `push_set_rules` — that mirror the rule-management endpoints of the [v2 push backend spec](https://hackmd.io/@1JCaGppGSUqHtJilikYaKw/r16YTVg5Ze). From the product's point of view a rule is just a `topic`: the product does not specify the signer, the host injects it when forwarding the rule to its push backend. The backend then delivers a push to the user's device(s) whenever a signed statement matching the resulting `(signer, topic)` pair appears on the Statement Store. The product never sees push tokens.

The method names use `add` / `remove` rather than `subscribe` / `unsubscribe` because the `_subscribe` suffix is reserved for streaming TrUAPI methods (e.g. `statementStore.subscribe`).

## References

- Push notifications, original (v1, peer-to-peer): https://hackmd.io/@1JCaGppGSUqHtJilikYaKw/SyPN2yV6lx
- Push notifications backend design (v2, backend-mediated): https://hackmd.io/@1JCaGppGSUqHtJilikYaKw/r16YTVg5Ze

This RFC exposes a TrUAPI-shaped surface over the rule-management API defined in the v2 spec.

## Motivation

The push-notifications v2 design assigns delivery to a host-side push backend that tails the Statement Store, verifies signatures, and delivers pushes only for `(signer, topic)` pairs the user has whitelisted. TrUAPI needs a primitive that lets a product manipulate that whitelist. The product supplies the `topic`; the host fills in the `signer` from the calling product's identity before forwarding to the backend.

### Worked example: festival announcements

A conference product publishes festival-wide announcements as signed statements on a well-known topic, signed with the product's own identity key (`pkProduct`). When the user taps "notify me about announcements," the subscriber app calls `push_add_rules({ rules: [{ topic: announcements_topic }] })`. The host injects `pkProduct` as the signer when relaying to the backend, so from that point on the user is woken up for new announcements even with the product closed:

```
Publisher app                                          Subscriber app
(organizer side)                                       (attendee side)
        |                                                       ^   |
        |                                                       |   |
        |                                                       |   |  (1) pushAddRules({
        |                                              (6) push |   |        rules: [{
        |                                               back to |   |          topic: T_announcements
        |                                                caller |   |        }]
        |                                                       |   |      })
        |                                                       |   |
        |                                                       |   v
        |                  +------------------------------------+---+------+
        |                  |  Host                                         |
        |                  |  injects signer = pkProduct, then forwards    |
        |                  |  to push backend:                             |
        |                  |    rule (pkProduct, T_announcements)          |
        |                  |      -> this subscriber app                   |
        |                  +-----------------------+-----------------------+
        |                                          ^
        |                                          |  (4) tail / match rule
        |                                          |
        |                  +-----------------------+-----------------------+
        |                  | Statement Store                               |
        |                  +-----------------------+-----------------------+
        |                                          ^
        |   (2) compose signed statement           |
        |--- (3) statementStore.submit(statement) -+
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

```rust
#[wire(request_id = 134)]
async fn push_add_rules(
    &self, cx: &CallContext, request: HostPushAddRulesRequest,
) -> Result<HostPushAddRulesResponse, CallError<HostPushAddRulesError>>;

#[wire(request_id = 136)]
async fn push_remove_rules(
    &self, cx: &CallContext, request: HostPushRemoveRulesRequest,
) -> Result<HostPushRemoveRulesResponse, CallError<HostPushRemoveRulesError>>;

#[wire(request_id = 138)]
async fn push_list_rules(
    &self, cx: &CallContext, request: HostPushListRulesRequest,
) -> Result<HostPushListRulesResponse, CallError<HostPushListRulesError>>;

#[wire(request_id = 140)]
async fn push_set_rules(
    &self, cx: &CallContext, request: HostPushSetRulesRequest,
) -> Result<HostPushSetRulesResponse, CallError<HostPushSetRulesError>>;
```

### Types

`Topic` is reused from `v01::statement_store`.

```rust
/// A single topic the user wants to be woken up for.
///
/// At the host level the effective key is (product, topic): rules are scoped
/// per calling product, so two products can register the same topic
/// independently and never see each other's rules. The product does not
/// specify the signer; the host injects it when forwarding the rule to the
/// push backend.
pub struct PushSubscriptionRule {
    pub topic: Topic,
}

pub struct HostPushAddRulesRequest    { pub rules: Vec<PushSubscriptionRule> }
pub struct HostPushRemoveRulesRequest { pub rules: Vec<PushSubscriptionRule> }
pub struct HostPushListRulesRequest;
pub struct HostPushSetRulesRequest    { pub rules: Vec<PushSubscriptionRule> }

pub struct HostPushListRulesResponse {
    pub rules: Vec<PushSubscriptionRule>,
}

pub enum HostPushAddRulesError {
    /// The user has not granted `DevicePermission::Notifications`. The host
    /// SHOULD prompt for the permission lazily on the first such call from
    /// a product; if the user dismisses or declines, this variant is
    /// returned and no rules are stored.
    PermissionDenied,
    /// The host could not reach the push backend; no rules were stored.
    BackendUnavailable,
    /// Catch-all. `reason`
    Unknown { reason: String },
}

pub enum HostPushRemoveRulsError {
    BackendUnavailable,
    Unknown { reason: String },
}

pub enum HostPushListRulesError {
    BackendUnavailable,
    Unknown { reason: String },
}

pub enum HostPushSetRulesError {
    PermissionDenied,
    BackendUnavailable,
    Unknown { reason: String },
}
```
