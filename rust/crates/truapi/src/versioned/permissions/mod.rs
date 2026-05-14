//! Versioned wrappers for [`Permissions`](crate::api::Permissions) methods.

use crate::v01;

versioned_type! {
    pub enum HostDevicePermissionRequest { V1 => v01::HostDevicePermissionRequest }
    pub enum HostDevicePermissionResponse { V1 => v01::HostDevicePermissionResponse }
    pub enum HostDevicePermissionError { V1 => v01::GenericError }
    pub enum RemotePermissionRequest { V1 => v01::RemotePermissionRequest }
    pub enum RemotePermissionResponse { V1 => v01::RemotePermissionResponse }
    pub enum RemotePermissionError { V1 => v01::GenericError }
}
