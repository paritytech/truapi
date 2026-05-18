use parity_scale_codec::{Decode, Encode};

use super::Topic;

/// Notification text and tap target shown by the host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushNotificationRequest {
    /// Notification text.
    pub text: String,
    /// Optional URL to open on tap.
    pub deeplink: Option<String>,
}

/// Request to register one or more topics the user wants to be woken up for.
/// Each topic is added independently; existing rules are not touched.
///
/// At the host level the effective key is `(product, topic)`: rules are
/// scoped per calling product, so two products can register the same topic
/// independently and never see each other's rules. The product does not
/// specify the signer; the host injects it when forwarding the rule to the
/// push backend.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushAddRulesRequest {
    /// Topics to register.
    pub topics: Vec<Topic>,
}

/// Failure modes for [`HostPushAddRulesRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPushAddRulesError {
    /// The user has not granted `DevicePermission::Notifications`.
    PermissionDenied,
    /// The host's push backend is currently unreachable; the rule was not
    /// registered. The product MAY retry later.
    BackendUnavailable,
    /// Catch-all.
    Unknown { reason: String },
}

/// Request to remove one or more previously registered topics.
/// Topics not currently active are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushRemoveRulesRequest {
    /// Topics to remove.
    pub topics: Vec<Topic>,
}

/// Failure modes for [`HostPushRemoveRulesRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPushRemoveRulesError {
    /// The host's push backend is currently unreachable; the rule may still
    /// be active. The product MAY retry later.
    BackendUnavailable,
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
    /// The host's push backend is currently unreachable. The product MAY
    /// retry later.
    BackendUnavailable,
    /// Catch-all.
    Unknown { reason: String },
}

/// Atomic replace of the calling product's full topic set with the supplied
/// vector. After a successful call, the product's active topics are exactly
/// `topics`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushSetRulesRequest {
    /// Topics that should be active for this product after the call.
    pub topics: Vec<Topic>,
}

/// Failure modes for [`HostPushSetRulesRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPushSetRulesError {
    /// The user has not granted `DevicePermission::Notifications`.
    PermissionDenied,
    /// The host's push backend is currently unreachable; no change was
    /// applied. The product MAY retry later.
    BackendUnavailable,
    /// Catch-all.
    Unknown { reason: String },
}
