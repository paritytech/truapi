use parity_scale_codec::{Decode, Encode};

use super::common::GenericErr;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFeatureSupportedRequest {
    Chain {
        /// Chain genesis hash.
        genesis_hash: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostNavigateToError {
    PermissionDenied,
    Unknown { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushNotificationRequest {
    /// Notification text.
    pub text: String,
    /// Optional URL to open on tap.
    pub deeplink: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostHandshakeError {
    Timeout,
    UnsupportedProtocolVersion,
    Unknown(GenericErr),
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostHandshakeRequest {
    /// Wire codec version requested by the peer.
    pub codec_version: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFeatureSupportedResponse {
    /// Whether the feature is supported.
    pub supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNavigateToRequest {
    /// URL to open.
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRouteGetResponse {
    /// Current route the host holds for this app, or `None` when the app is at its home.
    pub route: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRouteSetRequest {
    /// Opaque route segment defined by the app.
    pub route: String,
    /// `true` replaces the current history entry; `false` pushes a new one.
    pub replace: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRouteChangedItem {
    /// New route, or `None` when the user is at the app's home.
    pub route: Option<String>,
}
