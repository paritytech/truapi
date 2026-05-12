//! Versioned wrappers for [`TrUApiCalls`](crate::api::TrUApiCalls) methods.

use crate::v01;

versioned_type! {
    /// Versioned wrapper for [`v01::HostHandshakeRequest`].
    pub enum HostHandshakeRequest { V1 => v01::HostHandshakeRequest }
    /// Versioned wrapper for unit.
    pub enum HostHandshakeResponse { V1 }
    /// Versioned wrapper for [`v01::HostHandshakeError`].
    pub enum HostHandshakeError { V1 => v01::HostHandshakeError }
    /// Versioned wrapper for [`v01::HostFeatureSupportedRequest`].
    pub enum HostFeatureSupportedRequest { V1 => v01::HostFeatureSupportedRequest }
    /// Versioned wrapper for [`v01::HostFeatureSupportedResponse`].
    pub enum HostFeatureSupportedResponse { V1 => v01::HostFeatureSupportedResponse }
    /// Versioned wrapper for [`v01::GenericError`].
    pub enum HostFeatureSupportedError { V1 => v01::GenericError }
    /// Versioned wrapper for [`v01::HostNavigateToRequest`].
    pub enum HostNavigateToRequest { V1 => v01::HostNavigateToRequest }
    /// Versioned wrapper for unit.
    pub enum HostNavigateToResponse { V1 }
    /// Versioned wrapper for [`v01::HostNavigateToError`].
    pub enum HostNavigateToError { V1 => v01::HostNavigateToError }
    /// Versioned wrapper for [`v01::HostPushNotificationRequest`].
    pub enum HostPushNotificationRequest { V1 => v01::HostPushNotificationRequest }
    /// Versioned wrapper for unit.
    pub enum HostPushNotificationResponse { V1 }
    /// Versioned wrapper for [`v01::GenericError`].
    pub enum HostPushNotificationError { V1 => v01::GenericError }
}
