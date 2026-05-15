---
title: "Push Notification Subscriptions"
type: rfc
status: draft
owner: "@pgherveou"
pr:
---

# RFC 0020 — Push Notification Subscriptions

## Summary

Adds two TrUAPI methods, `notifications.push_subscribe` and `notifications.push_unsubscribe`, that let a product declare which Statement Store events the user wants to be woken up for. A subscription is a `(signer, topic)` pair: the host's push backend will deliver a push to the user's device(s) whenever a signed statement matching that pair appears on the store. The product never sees push tokens; the host owns token registration and the integration with the backend described in the push-notifications v2 design.


## Related RFCs

- RFC 0019 — Scheduled Push Notifications (locally scheduled reminders fired by the host's OS scheduler).

This RFC is orthogonal: it covers **remote-event-driven** pushes delivered by the push backend, not locally originated ones.

## Motivation

The current `push_notification` method only handles **locally originated** notifications: the product itself, while running, asks the host to display one. It has no answer for **remote events the user wants to be woken up for when the product is not running** — the canonical reason push notifications exist on mobile platforms.

The push-notifications v2 design assigns this job to a host-side push backend that tails the Statement Store, verifies signatures, and delivers pushes only for `(signer, topic)` pairs the user has whitelisted. To wire products into that model, TrUAPI needs a primitive that lets a product manipulate the user's whitelist.

Use cases this unlocks:

- **Festival announcement subscription.** A conference product publishes festival-wide announcements as signed statements on a well-known topic. When the user taps "notify me about announcements," the product calls `push_subscribe({ signer = festival_signer, topic = announcements_topic })`. The user is now woken up for new announcements even with the product closed.
- **Per-room / per-channel chat notifications.** A chat product subscribes the user to specific session topics so that direct messages from a particular contact wake the device.
- **On-chain event reminders.** A wallet product subscribes the user to a topic onto which an indexer publishes signed statements when relevant chain events fire.
- **Unsubscribe symmetry.** Tapping "stop notifying me" must cleanly retract the whitelist entry without forcing the product to re-derive any device-level state.

Without this primitive, products must either ship their own background process (impractical on web/mobile sandboxes) or expose push tokens to peers (the v1 model the v2 redesign explicitly rejects).

## Context: end-to-end flow

The diagram below shows the publish/subscribe shape this RFC enables. A
**publisher app** signs an update and writes it to the Statement Store; a
**subscriber app** has previously declared interest via the new TrUAPI
methods; the host's push backend tails the store, matches the new statement
against the subscriber's rules, and pushes the same signed statement back to
the subscriber app. The publisher does not know its subscribers in advance,
and there is no pairwise key between them; authenticity comes from the
publisher's signature.

```
Publisher app                                          Subscriber app
(organizer side)                                       (attendee side)
        |                                                       ^   |
        |                                                       |   |
        |                                                       |   |  (1) pushSubscribe({
        |                                              (6) push |   |        signer: pkPublisher,
        |                                               back to |   |        topic:  T_announcements
        |                                                caller |   |      })
        |                                                       |   |
        |                                                       |   v
        |                  +------------------------------------+---+------+
        |                  |  Push backend                                 |
        |                  |  stores rule:                                 |
        |                  |  (pkPublisher, T_announcements)               |
        |                  |    -> this subscriber app                     |
        |                  +-----------------------+-----------------------+
        |                                          ^
        |                                          |  (4) tail / match rule
        |                                          |
        |                  +-----------------------+-----------------------+
        |                  | Statement Store                               |
        |                  +-----------------------+-----------------------+
        |                                          ^
        |   (2) compose statement                  |
        |   statement = {                          |
        |     sender_pk: pkPublisher,              |
        |     topic:     T_announcements,          |
        |     data:      encode(Announcement),     |   <-- PLAINTEXT
        |     sig:       Sr25519_sign(skPublisher, body),
        |   }                                      |
        |                                          |
        |--- (3) statementStore.submit(statement) -+
```

On receipt (step 6) the subscriber app:

```
stmt         = decode(push.data)
require        Sr25519_verify(stmt.sig, body, pkPublisher)
announcement = decode(stmt.data)
display(announcement)
```

The festival-announcement case is the canonical example; the same shape
applies to any one-to-many notification flow (on-chain event indexers,
broadcast channels, system-status alerts). See
[`push-notifications/v2-broadcast-pubsub.md`](https://github.com/paritytech/sdk-team/blob/main/docs/push-notifications/v2-broadcast-pubsub.md)
in the SDK-team docs for the longer write-up of this design.

## Detailed Design

### Trait reorganization

A new trait `Notifications` is added at `rust/crates/truapi/src/api/notifications.rs` and joined into the `TrUApi` super-trait. The existing `push_notification` method moves from `System` onto this trait. Its wire id is preserved (`request_id = 4`), so no wire-protocol break occurs.

```rust
pub trait Notifications: Send + Sync {
    #[wire(request_id = 4)]
    async fn push_notification(
        &self,
        cx: &CallContext,
        request: HostPushNotificationRequest,
    ) -> Result<HostPushNotificationResponse, CallError<HostPushNotificationError>>;

    #[wire(request_id = 134)]
    async fn push_subscribe(
        &self,
        cx: &CallContext,
        request: HostPushSubscribeRequest,
    ) -> Result<HostPushSubscribeResponse, CallError<HostPushSubscribeError>>;

    #[wire(request_id = 136)]
    async fn push_unsubscribe(
        &self,
        cx: &CallContext,
        request: HostPushUnsubscribeRequest,
    ) -> Result<HostPushUnsubscribeResponse, CallError<HostPushUnsubscribeError>>;
}
```

Request ids `134` and `136` are the next free even ids after the existing highest allocation (`132`).

### Type definitions (v0.1 wire types)

New types live in `rust/crates/truapi/src/v01/notifications.rs`, with versioned wrappers in `rust/crates/truapi/src/versioned/notifications.rs`. `Topic` is reused from `v01::statement_store`.

```rust
/// 32-byte statement signer (matches `StatementProof::Sr25519::signer`
/// and `StatementProof::Ed25519::signer`).
pub type StatementSigner = [u8; 32];

/// A single (signer, topic) pair the user wants to be woken up for.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PushSubscriptionRule {
    pub signer: StatementSigner,
    pub topic: Topic,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushSubscribeRequest {
    pub rule: PushSubscriptionRule,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPushSubscribeError {
    PermissionDenied,
    SubscriptionLimitReached,
    BackendUnavailable,
    Unknown { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushUnsubscribeRequest {
    pub rule: PushSubscriptionRule,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPushUnsubscribeError {
    BackendUnavailable,
    Unknown { reason: String },
}
```

Both rule fields are required. There is no wildcard. A product that needs to be pushed for the same signer on three topics issues three `push_subscribe` calls.

`PermissionDenied` is **not** modelled on the unsubscribe path: removing consent never requires consent. `HostPushSubscribeResponse` and `HostPushUnsubscribeResponse` are unit-shaped at the versioned layer (no v0.1 inner type needed).

### Behavioural requirements

1. **Per-product scope.** Each `(product_identity, signer, topic)` triple is independent. Product A subscribing to a rule has no effect on product B's rule set, even for the same `(signer, topic)`. The host MUST NOT leak one product's subscriptions to another, and MUST NOT count one product's pushes against another's quotas.

2. **The product never sees the push token.** The host registers its own device token(s) with the push backend out-of-band (typically on first launch). `push_subscribe` only attaches a rule to those subscriptions; it never returns or accepts tokens.

3. **Fan-out across the user's devices.** If the user is logged in on multiple devices that share this host identity, the rule is registered against each device's subscription. A matching statement wakes every eligible device.

4. **Idempotency.** Registering the same rule twice is a no-op. Unsubscribing an unknown rule is a no-op. Products do not have to track local state to avoid duplicate calls.

5. **Permission gating.** Both methods are gated by `DevicePermission::Notifications`. The host SHOULD request the permission lazily on the first `push_subscribe` call from a product, mirroring the existing `notifications.push_notification` behaviour. `push_unsubscribe` MUST work regardless of permission state.

6. **Survives product not running.** Once a rule is registered, the user is pushed for matching statements until the rule is removed or the product is uninstalled. A push that wakes the device for a product that is not running is the expected outcome, not an error.

7. **Cleanup on product disconnect.** When a product is uninstalled, force-removed, or otherwise reaches end-of-life on the host, the host MUST remove all of that product's subscription rules. Transient disconnects (host restart, network blip) MUST NOT trigger cleanup. Rationale: rules firing for a product the user can no longer reach are user-hostile.

8. **Failure modes.** The host SHOULD treat `BackendUnavailable` as a soft failure — the product retains the option to retry. Local state on the host MAY queue the rule for delivery once the backend is reachable; if it does, the host MUST still return `BackendUnavailable` on the original call so the product is not lied to about the live state.

9. **Limits.** A product MAY register up to **64** active rules. A call that would exceed the cap MUST return `SubscriptionLimitReached`. Unsubscribing a rule frees a slot. The cap is per-product so a noisy product cannot crowd out a quieter one. The protocol does not specify a global cap; hosts MAY impose one as a backend-protection measure.

10. **Rule equality.** Two rules are equal iff both `signer` and `topic` are byte-equal. The host MUST de-duplicate on this equality.

### Permission model

No new permission variant is introduced. `DevicePermission::Notifications` (defined in `v01/permissions.rs`) covers both:

- the existing `notifications.push_notification` (local, host-rendered),
- the new `notifications.push_subscribe` / `unsubscribe` (remote, backend-delivered).

Rationale: from the user's perspective both are "the app may notify me." Splitting the prompt would add friction without a corresponding security gain. From the protocol's perspective the consent target is the user's *attention*, not the specific delivery mechanism.

### Protocol Integration

Per the action-derivation rules in `host-api-protocol.md` §"Interface (ABI)", the two new methods produce four actions:

```
host_push_subscribe_request:        Versioned<HostPushSubscribeRequest>
host_push_subscribe_response:       Versioned<Result<HostPushSubscribeResponse, HostPushSubscribeError>>
host_push_unsubscribe_request:      Versioned<HostPushUnsubscribeRequest>
host_push_unsubscribe_response:     Versioned<Result<HostPushUnsubscribeResponse, HostPushUnsubscribeError>>
```

Request ids `134` and `136` are appended after the existing highest allocation (`132`). The new methods are introduced as `Versioned::V1` payloads. The `push_notification` move preserves its existing `request_id = 4` and its existing `Versioned::V1` payload bytes, so the change is purely additive on the wire.

### Worked example — festival announcements

```
Conference product                   Host (Notifications trait)    Push backend            Statement Store
        |                                       |                         |                       |
   user taps "Notify me about announcements"    |                         |                       |
        |                                       |                         |                       |
        |--- notifications.pushSubscribe({      |                         |                       |
        |     signer: festivalSigner,           |                         |                       |
        |     topic:  announcementsTopic        |                         |                       |
        |   }) ------------------------------>  |                         |                       |
        |                                       |--- POST /v1/subscriptions/rules                 |
        |                                       |    { rule: (signer, topic) } ----->|            |
        |                                       |<-- 200 ----------------------------|            |
        |<-- Ok ---------------------------------|                                                 |
        |                                       |                                                 |
                                            ... later, product is closed ...
                                                                                                  |
                                            festival publishes signed announcement -------------->|
                                                                                                  |
                                                |                         |<-- tail / subscribe --|
                                                |                         |    signed statement   |
                                                |                         |                       |
                                                |                         | verify sig, match     |
                                                |                         | rule, push            |
                                                |<-- APNs/FCM/VoIP -------|                       |
        |<-- user device wakes ------------------|                                                 |
        |    OS shows notification               |                                                 |
        |                                                                                          |
   user taps "Stop notifying me"                                                                   |
        |--- notifications.pushUnsubscribe({    |                         |                       |
        |     signer: festivalSigner,           |                         |                       |
        |     topic:  announcementsTopic        |                         |                       |
        |   }) ------------------------------>  |                         |                       |
        |                                       |--- DELETE /v1/subscriptions/rules               |
        |                                       |    { rule: (signer, topic) } ----->|            |
        |                                       |<-- 200 ----------------------------|            |
        |<-- Ok ---------------------------------|                                                 |
```

The product call site (TypeScript, illustrative):

```ts
import { type Client } from "@parity/truapi";

export async function subscribeToFestivalAnnouncements(
  truapi: Client,
  festivalSigner: Uint8Array,
  announcementsTopic: Uint8Array,
): Promise<void> {
  const result = await truapi.notifications.pushSubscribe({
    rule: { signer: festivalSigner, topic: announcementsTopic },
  });
  if (result.isErr()) throw result.error;
}

export async function unsubscribeFromFestivalAnnouncements(
  truapi: Client,
  festivalSigner: Uint8Array,
  announcementsTopic: Uint8Array,
): Promise<void> {
  const result = await truapi.notifications.pushUnsubscribe({
    rule: { signer: festivalSigner, topic: announcementsTopic },
  });
  if (result.isErr()) throw result.error;
}
```

## Drawbacks

1. **State the host must persist.** Subscription rules belong to the user (not the product session) and must survive product restarts, app updates, and device reboots. Hosts now own a small but real piece of state per product.

2. **Backend coupling.** The host implementation depends on a reachable push backend (an instance of the v2 backend design). Hosts that ship without one must return `BackendUnavailable` for every call, which is a degraded but well-defined state.

3. **Per-product cap of 64 may be too tight for some products.** A chat product whose user has 200 contacts cannot subscribe per-contact. The intended workaround is to subscribe at a coarser topic granularity (e.g., one topic per app-defined channel) or to rely on a single signer covering many topics via wildcards added in a future RFC.

4. **No introspection in v1.** This RFC deliberately omits a `push_list_subscriptions` method (see "Unresolved Questions"). Products that want to reconcile local UI state with server state must currently track it themselves.

## Alternatives

### Keep everything on `System`

Add the two new methods to `System` and leave `push_notification` where it is. **Rejected**: `System` already mixes handshake, feature detection, and navigation. Adding three notification-related methods would make it the catch-all junk drawer. A dedicated `Notifications` trait keeps the surface coherent and gives a natural home for future additions (e.g. a list endpoint, bulk replace). The move is wire-stable because `push_notification` keeps its existing `request_id = 4`.

### Bulk `push_set_subscriptions(rules: Vec<Rule>)` instead of per-rule add/remove

Mirror the backend's `PUT /v1/subscriptions/rules` atomic-replace endpoint at the TrUAPI surface. **Rejected for v1**: products overwhelmingly drive subscriptions from per-rule UI actions (tap toggle → one rule changes). A bulk replace forces every caller to fetch-modify-send, which is the wrong default. A bulk method can be added later without breaking the per-rule API.

### Have the product write its own statement-store subscriptions and forward them to push

Reuse `remote_statement_store_subscribe` (RFC 0008) and have the host detect that a product is interested and forward to push. **Rejected**: conflates two semantically different things (in-product live updates vs. wake-the-device-when-closed) and forces the host to introspect the running product's connection state to decide whether to push. The two have different reliability requirements, payload constraints, and permission stories; they deserve different surfaces.

### Encode the rule as `(signer, channel)` instead of `(signer, topic)`

`Statement.channel` is a single optional 32-byte field, while `Statement.topics` is a vector. Matching on `channel` would be a single equality check. **Rejected**: topic-based rules are what the v2 backend design specifies, and `topics` is the field products already use for fan-out via RFC 0008. Mixing `channel` would split the matching strategy for no benefit.

## Unresolved Questions

- **List endpoint.** Should we add `push_list_subscriptions() -> Vec<PushSubscriptionRule>` to let a product reconcile local UI state with the host's view (e.g., after a logout/login cycle)? Easy to add later. Skipped for v1 to keep the surface minimal.
- **Atomic bulk replace.** Whether and when to add `push_set_subscriptions(Vec<Rule>)` mirroring the backend's `PUT /v1/subscriptions/rules`.
- **Signer schemes beyond Sr25519/Ed25519.** `StatementProof` also has `Ecdsa` (33-byte signer) and `OnChain` (32-byte `who`). The `[u8; 32]` shape covers Sr25519, Ed25519, and `OnChain`. ECDSA-signed statements (33-byte signer) cannot be expressed by the current rule type; if this becomes a real use case the rule should evolve into a tagged enum.
- **Topic-granularity wildcards.** A product that wants "all statements from signer S regardless of topic" must subscribe per-topic. Whether to add `MatchAnyTopic` semantics, and how that interacts with per-product caps, is left for a follow-up.
- **Backend selection.** A host MAY have multiple eligible push backends (e.g., one per network). How a rule binds to a specific backend, and what happens when the user's device set spans backends, is not specified here.
- **Rate-limit visibility.** The v2 backend rate-limits at 30 notifications per 60s per `(sender, receiver)` pair. The TrUAPI surface currently makes that invisible to the product. Whether to expose a "dropped due to rate limit" signal back to the product is open.
