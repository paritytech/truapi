//! Versioned wrappers for [`Permissions`](crate::api::Permissions) methods.

use crate::v01;
use core::fmt::{self, Display, Formatter};

versioned_type! {
    pub enum HostDevicePermissionRequest { V1 => v01::HostDevicePermissionRequest }
    pub enum HostDevicePermissionResponse { V1 => v01::HostDevicePermissionResponse }
    pub enum HostDevicePermissionError { V1 => v01::GenericError }
    pub enum RemotePermissionRequest { V1 => v01::RemotePermissionRequest }
    pub enum RemotePermissionResponse { V1 => v01::RemotePermissionResponse }
    pub enum RemotePermissionError { V1 => v01::GenericError }
}

impl Display for HostDevicePermissionRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::V1(inner) => write!(f, "{inner}"),
        }
    }
}

impl Display for RemotePermissionRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::V1(inner) => write!(f, "{inner}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_device_permission_request_delegates_to_inner() {
        let inner = v01::HostDevicePermissionRequest::Camera;
        let versioned = HostDevicePermissionRequest::V1(inner);
        assert_eq!(format!("{versioned}"), format!("{inner}"));
    }

    #[test]
    fn remote_permission_request_delegates_to_inner() {
        let inner = v01::RemotePermissionRequest {
            permissions: vec![
                v01::RemotePermission::WebRtc,
                v01::RemotePermission::Remote {
                    domains: vec!["example.com".into()],
                },
            ],
        };
        let versioned = RemotePermissionRequest::V1(inner.clone());
        assert_eq!(format!("{versioned}"), format!("{inner}"));
    }
}
