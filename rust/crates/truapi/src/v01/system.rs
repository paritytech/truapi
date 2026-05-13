use parity_scale_codec::{Decode, Encode};

use super::common::GenericErr;

/// Feature to check for host support.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFeatureSupportedRequest {
    /// Is this blockchain supported?
    Chain {
        /// Chain genesis hash.
        genesis_hash: Vec<u8>,
    },
}

/// Navigation error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostNavigateToError {
    /// Navigation not allowed.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}

/// Push notification payload.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushNotificationRequest {
    /// Notification text.
    pub text: String,
    /// Optional URL to open on tap.
    pub deeplink: Option<String>,
}

/// Handshake error. Mirrors Novasama's `HandshakeErr` byte-for-byte so that
/// pre-codegen products (built against `@novasamatech/host-api`) can decode
/// `host_handshake_response` frames produced by this host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostHandshakeError {
    Timeout,
    UnsupportedProtocolVersion,
    Unknown(GenericErr),
}

/// Request to negotiate the wire codec version.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostHandshakeRequest {
    /// Wire codec version requested by the peer.
    pub codec_version: u8,
}

/// Response indicating whether a host feature is supported.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFeatureSupportedResponse {
    /// Whether the feature is supported.
    pub supported: bool,
}

/// Request to navigate to a URL.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNavigateToRequest {
    /// URL to open.
    pub url: String,
}

/// Device capability to request access to.
///
/// Extended with `Notifications`, `NFC`, `Clipboard`, `OpenUrl`, and
/// `Biometrics` per [RFC 0001] (JIT permissions).
///
/// [RFC 0001]: https://github.com/paritytech/triangle-js-sdks/pull/66
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[allow(clippy::upper_case_acronyms)]
pub enum HostDevicePermissionRequest {
    /// Push notification delivery permission.
    Notifications,
    Camera,
    Microphone,
    Bluetooth,
    /// Near-field communication access.
    NFC,
    Location,
    /// System clipboard access.
    Clipboard,
    /// Open a URL in an external browser.
    OpenUrl,
    /// Biometric authentication (fingerprint, face ID).
    Biometrics,
}

/// A single remote-operation permission entry.
///
/// The [`crate::api::System::remote_permission`] method accepts a
/// `Vec<RemotePermission>` so products can batch multiple permission requests
/// into a single prompt.
///
/// See [RFC 0001] and [issue #64].
///
/// [RFC 0001]: https://github.com/paritytech/triangle-js-sdks/pull/66
/// [issue #64]: https://github.com/paritytech/triangle-js-sdks/issues/64
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemotePermission {
    /// HTTP/HTTPS/WS/WSS access to specific domains. Each string is a domain
    /// pattern: `"api.example.com"` (exact), `"*.example.com"` (wildcard
    /// subdomain), or `"*"` (all hosts).
    Remote {
        /// Domain patterns requested by the product.
        domains: Vec<String>,
    },
    /// WebRTC access, can expose the user's IP address.
    WebRtc,
    /// Broadcast signed transactions via
    /// [`crate::api::ChainInteraction::remote_chain_transaction_broadcast`].
    ChainSubmit,
    /// Submit a preimage via [`crate::api::Preimage::remote_preimage_submit`].
    PreimageSubmit,
    /// Submit statements via [`crate::api::StatementStore::remote_statement_store_submit`].
    StatementSubmit,
}

/// Request containing batched remote-operation permissions.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePermissionRequest {
    /// Permissions requested by the product.
    pub permissions: Vec<RemotePermission>,
}

/// Response indicating whether a device permission was granted.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostDevicePermissionResponse {
    /// Whether the permission was granted.
    pub granted: bool,
}

/// Response indicating whether a remote permission was granted.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePermissionResponse {
    /// Whether the permission was granted.
    pub granted: bool,
}

/// Host UI theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum Theme {
    /// Light appearance.
    Light,
    /// Dark appearance.
    Dark,
}

/// Item emitted by the theme subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct HostThemeSubscribeItem {
    /// Current theme.
    pub theme: Theme,
}

/// Error from [`crate::api::System::host_derive_entropy`].
///
/// Under normal operation the function always succeeds; `Unknown` indicates an
/// unrecoverable internal host error.
///
/// See [RFC 0007].
///
/// [RFC 0007]: https://github.com/paritytech/triangle-js-sdks/pull/95
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostDeriveEntropyError {
    /// An unexpected error occurred in the host.
    Unknown,
}

/// Request to derive deterministic entropy.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostDeriveEntropyRequest {
    /// Domain-separated derivation context.
    pub context: Vec<u8>,
}

/// Response containing derived deterministic entropy.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostDeriveEntropyResponse {
    /// 32 bytes of derived entropy.
    pub entropy: [u8; 32],
}

/// A resource the product can request the host to pre-allocate.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum AllocatableResource {
    /// Statement store allowance.
    StatementStoreAllowance,
    /// Bulletin board allowance.
    BulletinAllowance,
    /// Smart contract allowance with a derivation index.
    SmartContractAllowance(u32),
    /// Auto-signing capability.
    AutoSigning,
}

/// Outcome of a resource allocation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum AllocationOutcome {
    /// Resource was allocated.
    Allocated,
    /// User or host rejected the allocation.
    Rejected,
    /// Resource type is not available on this host.
    NotAvailable,
}

/// Request to allocate one or more resources.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRequestResourceAllocationRequest {
    /// Resources to allocate.
    pub resources: Vec<AllocatableResource>,
}

/// Response containing the outcome for each requested resource.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRequestResourceAllocationResponse {
    /// Per-resource allocation outcomes, in the same order as the request.
    pub outcomes: Vec<AllocationOutcome>,
}

/// Resource allocation error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ResourceAllocationError {
    /// Catch-all.
    Unknown { reason: String },
}
