---
title: "Scheduled Push Notifications"
owner: "@johnthecat"
---

# RFC 0012 — Scheduled Push Notifications

## Summary

This RFC extends `host_push_notification` so a product can schedule a notification to fire at a future wall-clock instant, returning a per-product `NotificationId` that the product can later use to cancel the pending delivery via a new `host_push_notification_cancel` method. Scheduled notifications behave like reminders: the host MUST persist them across app and device restarts and fire them through the OS scheduler. The change is targeted at the v0.2 protocol surface as defined in `truapi-spec/src/v02/mod.rs`.

## Motivation

The current `host_push_notification` is fire-and-forget — the product asks the host to display a notification *now*. Many product flows need the opposite shape: schedule a reminder for later, then optionally retract it if the underlying state changes (event cancelled, payment settled, task completed). Without a host-level primitive every product must:

- keep its own background process running to issue a notification at the right time, which is impractical on web/mobile sandboxed runtimes;
- or piggy-back on chain events, which is a poor fit for time-based reminders.

Exposing scheduling at the host level lets the host delegate to the platform-native scheduler (`UNUserNotificationCenter` on iOS, `AlarmManager`/`WorkManager` on Android, service-worker `showNotification` with `timestamp` on web) and gives products a single, portable API.

A symmetric `cancel` is the minimum useful complement: a scheduled reminder that cannot be retracted is worse than no reminder, because it forces products to defensively avoid scheduling.

## Detailed Design

### API changes (v0.2)

#### Modified type

```rust
type NotificationId = u32;

struct PushNotification {
    /// Notification text.
    text: String,
    /// Optional URL to open on tap.
    deeplink: Option<String>,
    /// Optional Unix timestamp in milliseconds (UTC) at which the notification
    /// should fire. `None` fires immediately, preserving prior behaviour.
    scheduled_at: Option<u64>,
}
```

#### Modified method

```rust
fn host_push_notification(
    &self,
    notification: PushNotification,
) -> Result<NotificationId, PushNotificationError>;
```

The return type changes from `Result<(), GenericError>` to `Result<NotificationId, PushNotificationError>`. The id is returned for **every** call — both immediate and scheduled — so that callers do not branch on the presence of `scheduled_at`. For an immediate notification the id has no operational use (the notification has already been delivered to the OS) but is still returned for shape uniformity.

#### New method

```rust
fn host_push_notification_cancel(
    &self,
    identifier: NotificationId,
) -> Result<(), GenericError>;
```

#### New error type

```rust
enum PushNotificationError {
    /// The product has reached the maximum number of pending scheduled
    /// notifications (see "Limits" below).
    ScheduleLimitReached,
    /// Catch-all.
    Unknown { reason: String },
}
```

`PermissionDenied` is **not** modelled as an explicit variant. Notification permission is gated by `DevicePermission::Notifications` and is requested out-of-band; if the user has not granted it, the host SHOULD treat the call as a no-op and return a fresh id, mirroring how immediate notifications behave today on platforms where the user silenced the app.

### Behavioural requirements

1. **Identifier scope.** `NotificationId` is unique **per product**. Two products may both observe `id = 1` referring to unrelated notifications. The host MUST NOT leak ids across products.

2. **Identifier allocation.** Ids are assigned by the host. Products MUST treat the value as opaque. Hosts MAY reuse ids of cancelled or already-fired notifications, but SHOULD avoid reuse within a session to make logs easier to read.

3. **Past timestamps fire immediately.** If `scheduled_at` is `Some(t)` and `t ≤ now`, the host MUST fire the notification immediately. It MUST NOT reject the request.

4. **Persistence.** Scheduled notifications MUST survive host app restart and device reboot. A scheduled notification's "owner" is the product identity, not the in-memory product session. The host registers the notification with the platform-native scheduler at the time `host_push_notification` is called.

5. **Cancellation is idempotent.** `host_push_notification_cancel` MUST return `Ok(())` in all of the following cases:
   - the id refers to a pending scheduled notification owned by the calling product (the notification is removed from the schedule);
   - the id refers to a notification that has already fired;
   - the id was never issued, or belongs to a different product;
   - the id refers to an immediate (non-scheduled) notification that already left the host.

   This avoids forcing every product to maintain its own "is this still pending?" bookkeeping. A product that needs to know whether a cancel actually took effect can track it locally.

6. **Permission gating.** Both `host_push_notification` and `host_push_notification_cancel` are gated by `DevicePermission::Notifications`. No new permission is introduced. Rationale: scheduling is a strict superset of immediate delivery from the user's perspective ("the app may show me notifications"); requiring a second prompt would be friction without a corresponding security gain.

7. **Limits.** A product MAY have up to **64** pending scheduled notifications outstanding at any time. This matches the iOS local-notification quota and is the binding constraint across platforms. Calls that would exceed the limit return `Err(PushNotificationError::ScheduleLimitReached)`. Immediate notifications (with `scheduled_at = None`) do not count against this limit. Cancelling a pending notification frees a slot.

   No maximum schedule horizon is defined. Hosts MAY warn or clamp at the platform level if necessary, but the protocol does not specify a ceiling.

8. **Cleanup on product disconnect.** When a product is uninstalled, force-removed, or otherwise disconnected from the host in a way that ends its installed lifecycle, the host MUST cancel all of that product's pending scheduled notifications. Rationale: a notification firing for a product the user can no longer reach is user-hostile. Transient disconnects (host restart, network blip) MUST NOT trigger cleanup — the persistence requirement (item 4) applies.

9. **Backward compatibility.** Adding `scheduled_at` to `PushNotification` and changing the return type of `host_push_notification` is a wire-format break. See "Protocol Integration" below.

### Protocol Integration

Per the action-derivation rules (`host-api-protocol.md` §"Interface (ABI)"), the changes produce four actions in total:

```
host_push_notification_request:        Versioned<PushNotification>
host_push_notification_response:       Versioned<Result<NotificationId, PushNotificationError>>

host_push_notification_cancel_request:  Versioned<NotificationId>
host_push_notification_cancel_response: Versioned<Result<(), GenericError>>
```

Action ordering follows the order of method declarations in the trait. The two cancel actions are appended immediately after the existing `host_push_notification_*` actions to keep notification-related variants contiguous.

The change to `PushNotification` and the change to the response type are introduced as a new `Versioned::V2` variant of the request and response payloads, leaving `V1` (text-only request, `()` success) intact for any host or product still on the older payload version. Hosts and products that advertise v0.2+ MUST send and accept `V2`.

## Drawbacks

1. **Wire-format break.** Existing v0.2 clients/hosts that already shipped against the current `host_push_notification` signature need to migrate to the new payload. The `Versioned::V2` envelope keeps the break gated, but real-world deployments will need a coordinated rollout.

2. **64-notification cap is platform-driven, not product-driven.** Products that legitimately need more (e.g., dense calendar apps) cannot exceed it. The cap is a hard lower bound across platforms, so this is unavoidable without per-host negotiation, which is out of scope.

## Alternatives

### Separate `host_push_notification_schedule` method

Keep `host_push_notification` unchanged and introduce a sibling `host_push_notification_schedule(text, deeplink, when) -> Result<NotificationId, _>`. **Rejected**: doubles the surface area for what is conceptually one operation with an optional time, and forces products to choose between two near-identical methods at every call site. Folding `scheduled_at` into the existing struct is more honest about the semantics.

### Use a `Duration`-from-now instead of a wall-clock timestamp

Take `scheduled_in_ms: Option<u64>` rather than `scheduled_at`. **Rejected**: relative durations are poorly defined under clock skew between scheduling and firing, and the "reminder at 9am tomorrow" use case is inherently absolute. Wall-clock ms UTC is what the underlying platform schedulers all consume.

## Unresolved Questions

- **Bulk cancel.** A product may want to cancel all of its pending notifications (e.g., on logout). Should `host_push_notification_cancel_all() -> Result<(), GenericError>` be added? Trivial to add later if demand emerges; not in this RFC.
