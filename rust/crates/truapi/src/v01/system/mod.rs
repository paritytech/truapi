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
