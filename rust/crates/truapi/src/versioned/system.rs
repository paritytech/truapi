//! Versioned wrappers for [`System`](crate::api::System) methods.

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
    pub enum HostDevicePermissionRequest { V1 => v01::HostDevicePermissionRequest }
    pub enum HostDevicePermissionResponse { V1 => v01::HostDevicePermissionResponse }
    pub enum HostDevicePermissionError { V1 => v01::GenericError }
    pub enum RemotePermissionRequest { V1 => v01::RemotePermissionRequest }
    pub enum RemotePermissionResponse { V1 => v01::RemotePermissionResponse }
    pub enum RemotePermissionError { V1 => v01::GenericError }
    pub enum HostThemeSubscribeItem { V1 => v01::HostThemeSubscribeItem }
    pub enum HostDeriveEntropyRequest { V1 => v01::HostDeriveEntropyRequest }
    pub enum HostDeriveEntropyResponse { V1 => v01::HostDeriveEntropyResponse }
    pub enum HostDeriveEntropyError { V1 => v01::HostDeriveEntropyError }
    pub enum HostRequestResourceAllocationRequest { V1 => v01::HostRequestResourceAllocationRequest }
    pub enum HostRequestResourceAllocationResponse { V1 => v01::HostRequestResourceAllocationResponse }
    pub enum HostRequestResourceAllocationError { V1 => v01::ResourceAllocationError }
}
