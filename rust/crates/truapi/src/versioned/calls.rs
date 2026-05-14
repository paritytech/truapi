//! Versioned wrappers for [`TrUApiCalls`](crate::api::TrUApiCalls) methods.

use crate::v01;

versioned_type! {
    pub enum HostHandshakeRequest { V1 => v01::HostHandshakeRequest }
    pub enum HostHandshakeResponse { V1 }
    pub enum HostHandshakeError { V1 => v01::HostHandshakeError }
    pub enum HostFeatureSupportedRequest { V1 => v01::HostFeatureSupportedRequest }
    pub enum HostFeatureSupportedResponse { V1 => v01::HostFeatureSupportedResponse }
    pub enum HostFeatureSupportedError { V1 => v01::GenericError }
    pub enum HostNavigateToRequest { V1 => v01::HostNavigateToRequest }
    pub enum HostNavigateToResponse { V1 }
    pub enum HostNavigateToError { V1 => v01::HostNavigateToError }
    pub enum HostPushNotificationRequest { V1 => v01::HostPushNotificationRequest }
    pub enum HostPushNotificationResponse { V1 }
    pub enum HostPushNotificationError { V1 => v01::GenericError }
}
