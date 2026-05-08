//! Versioned wrappers for [`TrUApiCalls`](crate::api::TrUApiCalls) methods.

use crate::{v01, v02};

versioned_type! {
    /// Versioned wrapper for [`v01::HostHandshakeRequest`] and older versions.
    pub enum HostHandshakeRequest { V1 => v01::HostHandshakeRequest }
    /// Versioned wrapper for unit and older versions.
    pub enum HostHandshakeResponse { V1 }
    /// Versioned wrapper for [`v02::HostHandshakeError`] and older versions.
    pub enum HostHandshakeError { V1 => v02::HostHandshakeError }
    /// Versioned wrapper for [`v01::HostFeatureSupportedRequest`] and older versions.
    pub enum HostFeatureSupportedRequest { V1 => v01::HostFeatureSupportedRequest }
    /// Versioned wrapper for [`v01::HostFeatureSupportedResponse`] and older versions.
    pub enum HostFeatureSupportedResponse { V1 => v01::HostFeatureSupportedResponse }
    /// Versioned wrapper for [`v01::GenericError`] and older versions.
    pub enum HostFeatureSupportedError { V1 => v01::GenericError }
    /// Versioned wrapper for [`v01::HostNavigateToRequest`] and older versions.
    pub enum HostNavigateToRequest { V1 => v01::HostNavigateToRequest }
    /// Versioned wrapper for unit and older versions.
    pub enum HostNavigateToResponse { V1 }
    /// Versioned wrapper for [`v01::HostNavigateToError`] and older versions.
    pub enum HostNavigateToError { V1 => v01::HostNavigateToError }
    /// Versioned wrapper for [`v01::HostPushNotificationRequest`] and older versions.
    pub enum HostPushNotificationRequest { V1 => v01::HostPushNotificationRequest }
    /// Versioned wrapper for unit and older versions.
    pub enum HostPushNotificationResponse { V1 }
    /// Versioned wrapper for [`v01::GenericError`] and older versions.
    pub enum HostPushNotificationError { V1 => v01::GenericError }
}
