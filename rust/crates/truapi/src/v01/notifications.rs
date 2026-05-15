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

/// A single `(signer, topic)` rule the user wants to be woken up for.
///
/// The host's push backend delivers a push to the user's device(s) whenever
/// a signed statement appears on the Statement Store whose signer equals
/// `signer` and whose `topics` list contains `topic`.
///
/// At the host level the effective key is `(product, signer, topic)`: rules
/// are scoped per calling product, so two products can register the same
/// `(signer, topic)` pair independently and never see each other's rules.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PushSubscriptionRule {
    /// Signer the matching statement must be signed by.
    pub signer: StatementSigner,
    /// Topic the matching statement must carry in its `topics` list.
    pub topic: Topic,
}

/// Request to register one or more subscription rules with the host. Each
/// rule is added independently; existing rules are not touched.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushAddRulesRequest {
    /// Rules to register.
    pub rules: Vec<PushSubscriptionRule>,
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

/// Request to remove one or more previously registered subscription rules.
/// Rules not currently active are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushRemoveRulesRequest {
    /// Rules to remove.
    pub rules: Vec<PushSubscriptionRule>,
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

/// Snapshot of the calling product's currently registered subscription rules.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushListRulesResponse {
    /// Currently registered rules for this product, in unspecified order.
    pub rules: Vec<PushSubscriptionRule>,
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

/// Atomic replace of the calling product's full rule set with the supplied
/// vector. After a successful call, the product's active rules are exactly
/// `rules`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushSetRulesRequest {
    /// Rules that should be active for this product after the call.
    pub rules: Vec<PushSubscriptionRule>,
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
