---
title: "Push Notification Subscriptions"
type: rfc
status: draft
owner: ["@pgherveou", "@sbalaguer"]
date: 2026-05-27
pr:
---

# RFC: Push Notification Subscriptions

## Summary

Adds four TrUAPI methods — `push_add_rules`, `push_remove_rules`, `push_list_rules`, `push_set_rules` — that expose the rule-management endpoints of the [v2 push backend spec](https://hackmd.io/@1JCaGppGSUqHtJilikYaKw/r16YTVg5Ze) to products. A rule is a `(signer, topic)` pair the product specifies in full: `signer` is the publisher whose signed statements should wake the user. The backend delivers a push to the user's device(s) whenever a signed statement matching a whitelisted `(signer, topic)` pair appears on the Statement Store. The product never sees push tokens; tokens live in the backend subscription keyed to the authenticated device.

A fifth method, `push_broadcast`, is an **interim transport** that distributes an announcement without using the Statement Store as the distribution layer. The host submits the announcement to the push backend and **sets the publisher `signer` itself** to the calling product's identity (the product cannot override it), and the backend fans out using the same `(signer, topic)` rule matching. It is marked **(interim)** throughout.

## Motivation

The push-notifications v2 design assigns delivery to a host-side notification system that tails the Statement Store, verifies signatures, and delivers pushes only for `(signer, topic)` pairs the user has whitelisted. TrUAPI needs a primitive that lets a product manipulate that whitelist on the user's own device. `signer` is mandatory on every rule: the product always names the publisher it wants to hear from.

### Worked example: festival announcements

A conference product publishes festival-wide announcements signed by the organizer:

- The attendee's app subscribes by calling `push_add_rules` with a rule naming the organizer's `AccountId`.
- The organizer publishes with `push_broadcast`; the host sets `signer` to the organizer's identity and submits the announcement to the backend.
- The backend matches `(organizer, topic)` against the attendee's rule and delivers a push.

From that point the attendee is woken for new announcements even with the app closed:

```
Publisher app                                          Subscriber app
(organizer side)                                       (attendee side)
        |                                                       ^   |
        |                                                       |   |
        |                                              (5) push |   |  (1) push_add_rules({ rules: [{ signer: organizer_id, topics: [T] }] })
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

## Stakeholders

- **Subscriber products** that want their users woken by publisher activity (event apps, channels) without running their own background process.
- **Publisher products** that announce to their audience; with `push_broadcast` they publish under a host-attested identity they cannot forge.
- **Host implementers**, who own the push token, the user's `Notifications` permission grant, and the binding of `signer` on broadcast.
- **Push backend operators**, who run the Statement Store tailer, rule store, and dispatch described in the v2 spec.

The design follows the v2 backend spec ([backend-mediated](https://hackmd.io/@1JCaGppGSUqHtJilikYaKw/r16YTVg5Ze)), which itself supersedes the original peer-to-peer v1 design ([v1](https://hackmd.io/@1JCaGppGSUqHtJilikYaKw/SyPN2yV6lx)). This RFC exposes a TrUAPI-shaped surface over that backend's rule-management API.

## Explanation

### Rule model

A rule is a `(signer, topic)` pair. `signer` is mandatory: the subscriber always names the publisher. Rules are grouped per signer on the wire as `PushRule { signer, topics }`, which is equivalent to the flat `(signer, topic)` tuple set the backend stores. `Topic` is reused from `v01::statement_store`.

All rule operations are scoped to the **calling user's own subscription**: a product manages only the rules on the device it is running on, and cannot read or mutate another user's rules.

### API

Each TrUAPI method maps to one backend endpoint:

| TrUAPI method       | Backend endpoint                 | Purpose                                       |
| ------------------- | -------------------------------- | --------------------------------------------- |
| `push_add_rules`    | `POST   /v1/subscriptions/rules` | additively whitelist rules                    |
| `push_remove_rules` | `DELETE /v1/subscriptions/rules` | remove specific rules                         |
| `push_list_rules`   | `GET    /v1/subscriptions`       | snapshot of the currently active rule set     |
| `push_set_rules`    | `PUT    /v1/subscriptions/rules` | atomic replace of the full multi-signer set   |
| `push_broadcast`    | direct submit _(interim)_        | publish a signed announcement                 |

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

### Semantics

- **`push_add_rules`** additively whitelists the rules in the request. Adding a rule that is already present is a no-op for that rule. The call is **idempotent**: the post-state is the set union of the prior rules and the requested rules, regardless of how many were already present.
- **`push_remove_rules`** removes the named rules. Removing a rule that is not present is a no-op for that rule. The call is **idempotent**: the post-state is the prior set minus the requested rules.
- **`push_set_rules`** atomically replaces the **entire** rule set for the subscription with exactly the rules in the request, across all signers. Rules not present in the request are deleted; this is the only operation that affects rules for signers the caller did not name.
- **`push_list_rules`** returns the full active rule set as `Vec<PushRule>`, including the `signer` of each rule. It is read-only and reflects the subscription's current state after any prior add/remove/set.

Within a single subscription the same `(signer, topic)` pair is never duplicated, so the rule set behaves as a set rather than a multiset.

### Permission gating

`push_add_rules` and `push_set_rules` are gated by `DevicePermission::Notifications`: they create the capacity for the user to receive pushes, which requires consent. The host SHOULD prompt for the permission lazily on the first such call; if the user dismisses or declines, the call returns `PermissionDenied` and no rules are stored.

`push_remove_rules` and `push_list_rules` carry **no** `PermissionDenied` variant. Removing rules only de-escalates (it can never cause new notifications), and listing returns only the user's own rules to the user's own product; neither expands what the product can do without consent.

### Types

```rust
/// One or more topics the subscriber wants to hear about from a single publisher.
pub struct PushRule {
    /// The publisher whose signed statements should wake the user.
    pub signer: AccountId,
    /// Topics to match for this publisher.
    pub topics: Vec<Topic>,
}

pub struct HostPushAddRulesRequest    { pub rules: Vec<PushRule> }
pub struct HostPushRemoveRulesRequest { pub rules: Vec<PushRule> }
pub struct HostPushListRulesRequest;
pub struct HostPushSetRulesRequest    { pub rules: Vec<PushRule> }

pub struct HostPushAddRulesResponse;
pub struct HostPushRemoveRulesResponse;
pub struct HostPushSetRulesResponse;

pub struct HostPushListRulesResponse {
    /// The full active rule set for the calling subscription.
    pub rules: Vec<PushRule>,
}

pub enum HostPushAddRulesError {
    /// The user has not granted `DevicePermission::Notifications`. The host
    /// SHOULD prompt for the permission lazily on the first such call from a
    /// product; if the user dismisses or declines, this variant is returned
    /// and no rules are stored.
    PermissionDenied,
    /// The notification system is currently unavailable; no rules were stored.
    NotificationSystemUnavailable(String),
    /// Catch-all.
    Unknown { reason: String },
}

pub enum HostPushRemoveRulesError {
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

### Interim: direct broadcast

`push_broadcast` distributes an announcement without using the Statement Store as the distribution layer. The product sends only `{ topics, content }`. The host **sets the `signer` itself** to the calling product's identity, host-set so the product cannot override or spoof it, and submits the announcement to the backend. The backend matches `(signer, topic)` against subscriber rules; matching, rate-limiting, dedup, and dispatch are the same as for Statement-Store-sourced announcements. Only the distribution layer differs. The product never sets `signer`, which is why it is absent from the request.

The broadcast is not a Statement Store statement: it is a plain `{ topics, content }` the host submits with a host-set `signer`, so there is no `channel`, topic slots, or `expiry`. The backend enforces its own per-publisher rate limits and notification payload size caps as defined in the v2 backend spec.

```rust
#[wire(request_id = 172)]
async fn push_broadcast(
    &self, cx: &CallContext, request: HostPushBroadcastRequest,
) -> Result<HostPushBroadcastResponse, CallError<HostPushBroadcastError>>;

pub struct PushBroadcastContent {
    pub title: String,
    pub body: String,
    /// Route or URL to open on tap.
    pub deeplink: Option<String>,
}

pub struct HostPushBroadcastRequest {
    /// Matched against subscriber rules; `signer` is set by the host to the caller.
    pub topics: Vec<Topic>,
    pub content: PushBroadcastContent,
}

pub struct HostPushBroadcastResponse {
    /// Blake2b-256 of the broadcast, for dedup and audit.
    pub message_hash: [u8; 32],
}

pub enum HostPushBroadcastError {
    NotificationSystemUnavailable(String),
    Unknown { reason: String },
}
```

## Drawbacks

- **Two delivery paths during the interim.** `push_broadcast` and Statement-Store-sourced announcements coexist, so the backend matches the same `(signer, topic)` rules against two sources until distribution is unified. This is transitional complexity that the Future Directions section retires.
- **No per-product rule quota is specified here.** A product can add an unbounded number of rules to the user's subscription, subject only to whatever the backend imposes. Quota policy is left to the backend.
- **`push_set_rules` is a blunt instrument.** Because it replaces the whole multi-signer set, a product that holds a stale snapshot can clobber rules added by another product on the same subscription. Products that only mean to adjust their own publisher should prefer add/remove.

## Testing, Security, and Privacy

- **Testing.** Each method has a wire round-trip equality test (the repo's wire-equality and wire-table-loop smoke tests cover request/response shapes). Idempotency is verified by asserting that repeated `push_add_rules`/`push_remove_rules` calls converge to the same `push_list_rules` snapshot, and that `push_set_rules` yields exactly the posted set. The `PermissionDenied` path is exercised for add/set.
- **Push tokens are never exposed.** The token lives in the backend subscription keyed to the authenticated device; TrUAPI returns only rules. A product cannot read or derive the token.
- **Rule operations are scoped to the calling user's own subscription.** A product cannot read or mutate rules on another user's device. Add/remove/set/list all act on the subscription of the device the product runs on.
- **`signer` on broadcast is host-attested.** In `push_broadcast` the host sets `signer` to the calling product's identity; a product cannot broadcast under another publisher's identity.
- **Subscribe-vs-publish asymmetry.** A product may subscribe the user to **any** `signer` (the festival example subscribes the attendee to the organizer), but may only **broadcast as itself**. Naming a publisher in a rule grants no ability to publish as that publisher.

## Performance, Ergonomics, and Compatibility

### Performance

Rule management is low-frequency control-plane traffic (subscribe/unsubscribe), not on any hot path. Delivery cost is borne by the backend tailer and dispatch, unchanged by this RFC. `push_broadcast` adds a direct submit path but reuses the existing matching and rate-limiting machinery.

### Ergonomics

The `PushRule { signer, topics }` shape groups topics per publisher, so a product subscribing to several topics from one signer sends one entry rather than N flat tuples. Idempotent add/remove let products converge state without read-modify-write races; `push_set_rules` is available when a product genuinely owns the whole set.

### Compatibility

These are new methods at fresh wire ids (164–172); no existing method changes, so there is no wire break for current clients. Hosts that do not implement the push backend return `NotificationSystemUnavailable`.

## Prior Art and References

- Push notifications, original (v1, peer-to-peer): https://hackmd.io/@1JCaGppGSUqHtJilikYaKw/SyPN2yV6lx
- Push notifications backend design (v2, backend-mediated): https://hackmd.io/@1JCaGppGSUqHtJilikYaKw/r16YTVg5Ze
- RFC 0019 — Scheduled Push Notifications (`0019-scheduled-notifications.md`): host-mediated, OS-scheduler-backed local notifications, complementary to the backend-mediated delivery here.
- RFC 0008 — Statement Store: the `Topic` type and the statement model that the non-interim delivery path tails.

## Unresolved Questions

- **Broadcast authorization.** Should the host gate which products may call `push_broadcast`, or is host-attested `signer` sufficient on its own? An unbounded broadcast right lets any product wake its subscribers at the backend's rate-limit ceiling.
- **Rule quota.** Should TrUAPI surface a per-subscription rule cap (and a corresponding error) rather than deferring entirely to the backend?
- **List pagination.** `push_list_rules` returns the whole set in one response. A subscription with many rules may warrant pagination; left out until a concrete need appears.

## Future Directions and Related Material

The non-interim path moves announcement **distribution** onto the Statement Store: a publisher writes a signed statement, the backend tailer matches it against the same `(signer, topic)` rules, and delivery proceeds identically. At that point `push_broadcast` is retired as a distribution mechanism while subscriber rules and the four rule-management methods stay unchanged. The interim `push_broadcast` exists only so products can announce before the Statement-Store distribution path is live; designing rules around `(signer, topic)` from the start is what makes the eventual switch transparent to subscribers.
