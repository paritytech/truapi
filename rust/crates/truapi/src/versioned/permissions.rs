//! Versioned wrappers for [`Permissions`](crate::api::Permissions) methods.

use crate::v01;

versioned_type! {
    /// Versioned wrapper for [`v01::HostDevicePermissionRequest`].
    pub enum HostDevicePermissionRequest { V1 => v01::HostDevicePermissionRequest }
    /// Versioned wrapper for [`v01::HostDevicePermissionResponse`].
    pub enum HostDevicePermissionResponse { V1 => v01::HostDevicePermissionResponse }
    /// Versioned wrapper for [`v01::GenericError`].
    pub enum HostDevicePermissionError { V1 => v01::GenericError }
    /// Versioned wrapper for [`v01::RemotePermissionRequest`].
    pub enum RemotePermissionRequest { V1 => v01::RemotePermissionRequest }
    /// Versioned wrapper for [`v01::RemotePermissionResponse`].
    pub enum RemotePermissionResponse { V1 => v01::RemotePermissionResponse }
    /// Versioned wrapper for [`v01::GenericError`].
    pub enum RemotePermissionError { V1 => v01::GenericError }
}
