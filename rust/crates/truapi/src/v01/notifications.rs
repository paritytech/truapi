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

/// 32-byte statement signer key.
///
/// Matches the `signer` field of [`StatementProof::Sr25519`] and
/// [`StatementProof::Ed25519`].
///
/// [`StatementProof::Sr25519`]: super::StatementProof::Sr25519
/// [`StatementProof::Ed25519`]: super::StatementProof::Ed25519
pub type StatementSigner = [u8; 32];

/// A single `(signer, topic)` pair the user wants to be woken up for.
///
/// The host's push backend delivers a push to the user's device(s) whenever
/// a signed statement appears on the Statement Store whose signer equals
/// `signer` and whose `topics` list contains `topic`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PushSubscriptionRule {
    /// Signer the matching statement must be signed by.
    pub signer: StatementSigner,
    /// Topic the matching statement must carry in its `topics` list.
    pub topic: Topic,
}

/// Request to register a single subscription rule with the host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushSubscribeRequest {
    /// Rule to register.
    pub rule: PushSubscriptionRule,
}

/// Failure modes for [`HostPushSubscribeRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPushSubscribeError {
    /// The user has not granted `DevicePermission::Notifications`.
    PermissionDenied,
    /// The product has reached the maximum number of active rules.
    SubscriptionLimitReached,
    /// The host's push backend is currently unreachable; the rule was not
    /// registered. The product MAY retry later.
    BackendUnavailable,
    /// Catch-all.
    Unknown { reason: String },
}

/// Request to remove a previously registered subscription rule.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushUnsubscribeRequest {
    /// Rule to remove.
    pub rule: PushSubscriptionRule,
}

/// Failure modes for [`HostPushUnsubscribeRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPushUnsubscribeError {
    /// The host's push backend is currently unreachable; the rule may still
    /// be active. The product MAY retry later.
    BackendUnavailable,
    /// Catch-all.
    Unknown { reason: String },
}
