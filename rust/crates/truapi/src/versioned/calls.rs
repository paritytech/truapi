//! Versioned wrappers for [`TrUApiCalls`](super::super::v02::TrUApiCalls) methods.

use parity_scale_codec::{Decode, Encode};

use super::Versioned;
use crate::v02::{Feature, PushNotification};

/// Request wrapper for `host_handshake`. Inner u8 is the codec version
/// (Novasama assigns `1` to JAM/SCALE).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostHandshakeRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(u8),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(u8),
}

impl Versioned for HostHandshakeRequest {
    type Inner = u8;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Response wrapper for `host_handshake`. Inner unit signals success; failure
/// is carried by the surrounding `Result<_, HandshakeError>`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostHandshakeResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1,
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2,
}

impl Versioned for HostHandshakeResponse {
    type Inner = ();
    fn wrap(version: u8, _: ()) -> Self {
        match version {
            1 => Self::V1,
            _ => Self::V2,
        }
    }
    fn into_inner(self) {}
}

/// Request wrapper for `host_feature_supported`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostFeatureSupportedRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(Feature),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(Feature),
}

impl Versioned for HostFeatureSupportedRequest {
    type Inner = Feature;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Response wrapper for `host_feature_supported`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostFeatureSupportedResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(bool),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(bool),
}

impl Versioned for HostFeatureSupportedResponse {
    type Inner = bool;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Request wrapper for `host_navigate_to`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostNavigateToRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(String),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(String),
}

impl Versioned for HostNavigateToRequest {
    type Inner = String;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Response wrapper for `host_navigate_to`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostNavigateToResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1,
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2,
}

impl Versioned for HostNavigateToResponse {
    type Inner = ();
    fn wrap(version: u8, _: ()) -> Self {
        match version {
            1 => Self::V1,
            _ => Self::V2,
        }
    }
    fn into_inner(self) {}
}

/// Request wrapper for `host_push_notification`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostPushNotificationRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(PushNotification),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(PushNotification),
}

impl Versioned for HostPushNotificationRequest {
    type Inner = PushNotification;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Response wrapper for `host_push_notification`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostPushNotificationResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1,
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2,
}

impl Versioned for HostPushNotificationResponse {
    type Inner = ();
    fn wrap(version: u8, _: ()) -> Self {
        match version {
            1 => Self::V1,
            _ => Self::V2,
        }
    }
    fn into_inner(self) {}
}
