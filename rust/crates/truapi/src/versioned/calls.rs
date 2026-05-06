//! Versioned wrappers for [`TrUApiCalls`](crate::api::TrUApiCalls) methods.

use crate::{v01, v02};

versioned_type! {
    /// Request wrapper for `host_handshake`. Inner u8 is the codec version.
    pub enum HostHandshakeRequest { V1 => u8 }
    /// Response wrapper for `host_handshake`.
    pub enum HostHandshakeResponse { V1 }
    /// Error wrapper for `host_handshake`.
    pub enum HostHandshakeError { V1 => v02::HandshakeError }
    /// Request wrapper for `host_feature_supported`.
    pub enum HostFeatureSupportedRequest { V1 => v01::Feature }
    /// Response wrapper for `host_feature_supported`.
    pub enum HostFeatureSupportedResponse { V1 => bool }
    /// Error wrapper for `host_feature_supported`.
    pub enum HostFeatureSupportedError { V1 => v01::GenericError }
    /// Request wrapper for `host_navigate_to`.
    pub enum HostNavigateToRequest { V1 => String }
    /// Response wrapper for `host_navigate_to`.
    pub enum HostNavigateToResponse { V1 }
    /// Error wrapper for `host_navigate_to`.
    pub enum HostNavigateToError { V1 => v01::NavigateToError }
    /// Request wrapper for `host_push_notification`.
    pub enum HostPushNotificationRequest { V1 => v01::PushNotification }
    /// Response wrapper for `host_push_notification`.
    pub enum HostPushNotificationResponse { V1 }
    /// Error wrapper for `host_push_notification`.
    pub enum HostPushNotificationError { V1 => v01::GenericError }
}
