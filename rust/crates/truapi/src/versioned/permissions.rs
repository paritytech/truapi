//! Versioned wrappers for [`Permissions`](crate::api::Permissions) methods.

use crate::v01;

versioned_type! {
    #[derive(derive_more::Display)]
    #[display("{_0}")]
    pub enum HostDevicePermissionRequest { V1 => v01::HostDevicePermissionRequest }
    pub enum HostDevicePermissionResponse { V1 => v01::HostDevicePermissionResponse }
    pub enum HostDevicePermissionError { V1 => v01::GenericError }
    #[derive(derive_more::Display)]
    #[display("{_0}")]
    pub enum RemotePermissionRequest { V1 => v01::RemotePermissionRequest }
    pub enum RemotePermissionResponse { V1 => v01::RemotePermissionResponse }
    pub enum RemotePermissionError { V1 => v01::GenericError }
}
