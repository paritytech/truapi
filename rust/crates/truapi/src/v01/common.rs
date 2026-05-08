use parity_scale_codec::{Decode, Encode};

/// Generic error payload carrying a human-readable reason string.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct GenericErr {
    pub reason: String,
}

/// Single-variant error enum wrapping [`GenericErr`]. Used by many methods as a
/// catch-all error type.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum GenericError {
    GenericError(GenericErr),
}

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

/// Device capability to request access to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum HostDevicePermissionRequest {
    Camera,
    Microphone,
    Bluetooth,
    Location,
}

/// Pre-RFC-0001 remote operation permission, as shipped by
/// `@novasamatech/host-api@0.6.x`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemotePermissionRequest {
    /// URL the product wants to fetch.
    ExternalRequest {
        /// URL the product wants to fetch.
        url: String,
    },
    /// Product wants to submit a transaction.
    TransactionSubmit,
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
