use parity_scale_codec::{Decode, Encode};

use super::{ProductAccountId, Topic};

/// Opaque identifier for a push notification, unique per product.
pub type NotificationId = u32;

/// Push notification payload.
///
/// When `scheduled_at` is `Some`, the notification is deferred to the given
/// wall-clock instant (Unix milliseconds UTC). `None` fires immediately,
/// preserving prior behaviour. See [RFC 0019].
///
/// [RFC 0019]: https://github.com/paritytech/truapi/blob/main/docs/rfcs/0019-scheduled-notifications.md
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushNotificationRequest {
    /// Notification text.
    pub text: String,
    /// Optional URL to open on tap.
    pub deeplink: Option<String>,
    /// Optional Unix timestamp in milliseconds (UTC) at which the notification
    /// should fire. `None` fires immediately.
    pub scheduled_at: Option<u64>,
}

/// Successful push notification response carrying the assigned id.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushNotificationResponse {
    /// Host-assigned notification identifier.
    pub id: NotificationId,
}

/// Push notification error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPushNotificationError {
    /// The host-wide queue of pending scheduled notifications is full.
    ScheduleLimitReached,
    /// Catch-all.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}

/// Request to cancel a previously scheduled notification.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushNotificationCancelRequest {
    /// The notification identifier returned by [`HostPushNotificationResponse`].
    pub id: NotificationId,
}

/// Request to register one or more topics the user wants to be woken up for.
/// Each topic is added independently; existing rules are not touched.
///
/// When `signer` is `None` the host injects the calling product's own
/// identity as the signer. Set `signer` explicitly to subscribe to
/// statements published by a *different* product (e.g. a conference
/// organizer's announcements).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushAddRulesRequest {
    /// Topics to register.
    pub topics: Vec<Topic>,
    /// Signer whose statements should trigger a push. Defaults to the
    /// calling product's own identity when `None`.
    pub signer: Option<ProductAccountId>,
}

/// Failure modes for [`HostPushAddRulesRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPushAddRulesError {
    /// The user has not granted `DevicePermission::Notifications`.
    PermissionDenied,
    /// The notification system is currently unavailable; the rule was not
    /// registered. The product MAY retry later.
    NotificationSystemUnavailable(String),
    /// Catch-all.
    Unknown { reason: String },
}

/// Request to remove one or more previously registered topics.
/// Topics not currently active are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushRemoveRulesRequest {
    /// Topics to remove.
    pub topics: Vec<Topic>,
    /// Signer scope. When `None`, removes rules for the calling product's
    /// own identity.
    pub signer: Option<ProductAccountId>,
}

/// Failure modes for [`HostPushRemoveRulesRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPushRemoveRulesError {
    /// The notification system is currently unavailable; the rule may still
    /// be active. The product MAY retry later.
    NotificationSystemUnavailable(String),
    /// Catch-all.
    Unknown { reason: String },
}

/// Request to list the calling product's currently registered subscription
/// rules. Has no fields; the host scopes results by the calling product
/// identity.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushListRulesRequest {}

/// Snapshot of the calling product's currently registered topics.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushListRulesResponse {
    /// Currently registered topics for this product, in unspecified order.
    pub topics: Vec<Topic>,
}

/// Failure modes for [`HostPushListRulesRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPushListRulesError {
    /// The notification system is currently unavailable. The product MAY
    /// retry later.
    NotificationSystemUnavailable(String),
    /// Catch-all.
    Unknown { reason: String },
}

/// Atomic replace of the full topic set for the given signer with the
/// supplied vector. After a successful call, the active topics are exactly
/// `topics`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushSetRulesRequest {
    /// Topics that should be active after the call.
    pub topics: Vec<Topic>,
    /// Signer scope. When `None`, replaces rules for the calling product's
    /// own identity.
    pub signer: Option<ProductAccountId>,
}

/// Failure modes for [`HostPushSetRulesRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPushSetRulesError {
    /// The user has not granted `DevicePermission::Notifications`.
    PermissionDenied,
    /// The notification system is currently unavailable; no change was
    /// applied. The product MAY retry later.
    NotificationSystemUnavailable(String),
    /// Catch-all.
    Unknown { reason: String },
}
