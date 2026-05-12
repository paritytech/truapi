//! Scheduled push notification types (RFC 0019).

use parity_scale_codec::{Decode, Encode};

/// Opaque, host-allocated identifier for a notification. Scope is per-product:
/// two products may observe the same numeric id referring to unrelated
/// notifications. Products MUST treat the value as opaque.
pub type NotificationId = u32;

/// Push notification payload. Extends the v0.1 shape with an optional future
/// fire instant. If `scheduled_at` is `None` the notification fires
/// immediately, preserving v0.1 behaviour but returning a `NotificationId`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushNotificationRequest {
    /// Notification text.
    pub text: String,
    /// Optional URL to open on tap.
    pub deeplink: Option<String>,
    /// Optional Unix timestamp in milliseconds (UTC) at which the
    /// notification should fire. `None` fires immediately.
    pub scheduled_at: Option<u64>,
}

/// Successful response. The id is returned for **every** call — both immediate
/// and scheduled — so callers don't branch on the presence of `scheduled_at`.
/// For an immediate notification the id has no operational use (the host has
/// already delivered the notification to the OS) but is still returned for
/// shape uniformity.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushNotificationResponse {
    /// Per-product identifier the product can later pass to
    /// [`HostPushNotificationCancelRequest`] to retract the pending delivery.
    pub id: NotificationId,
}

/// Domain error variants for [`super::super::api::TrUApiCalls::host_push_notification`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPushNotificationError {
    /// The product has reached the maximum number of pending scheduled
    /// notifications. The cap is host-wide across all installed products;
    /// see RFC 0019 §"Limits".
    ScheduleLimitReached,
    /// Catch-all.
    Unknown {
        /// Human-readable diagnostic.
        reason: String,
    },
}

/// Request to retract a pending scheduled notification.
///
/// Cancellation is idempotent: the host MUST return `Ok(())` whether the id
/// refers to a pending scheduled notification, an already-fired one, an
/// unknown id, or an id owned by a different product. RFC 0019 §5.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushNotificationCancelRequest {
    /// Per-product notification id returned by an earlier
    /// [`HostPushNotificationResponse`].
    pub id: NotificationId,
}
