//! Versioned wrappers for [`Permissions`](crate::api::Permissions) methods.

use crate::{v01, v02};

versioned_type! {
    /// Versioned wrapper for [`v02::HostDevicePermissionRequest`] and older versions.
    pub enum HostDevicePermissionRequest {
        V1 => v01::HostDevicePermissionRequest,
        V2 => v02::HostDevicePermissionRequest,
    }
    /// Versioned wrapper for [`v01::HostDevicePermissionResponse`] and older versions.
    pub enum HostDevicePermissionResponse { V1 => v01::HostDevicePermissionResponse, V2 => v01::HostDevicePermissionResponse }
    /// Versioned wrapper for [`v01::GenericError`] and older versions.
    pub enum HostDevicePermissionError { V1 => v01::GenericError, V2 => v01::GenericError }
    /// Versioned wrapper for [`v02::RemotePermissionRequest`] and older versions.
    pub enum RemotePermissionRequest {
        V1 => v01::RemotePermissionRequest,
        V2 => v02::RemotePermissionRequest,
    }
    /// Versioned wrapper for [`v01::RemotePermissionResponse`] and older versions.
    pub enum RemotePermissionResponse { V1 => v01::RemotePermissionResponse, V2 => v01::RemotePermissionResponse }
    /// Versioned wrapper for [`v01::GenericError`] and older versions.
    pub enum RemotePermissionError { V1 => v01::GenericError, V2 => v01::GenericError }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::versioned::{IntoVersion, Version};

    #[test]
    fn v1_external_request_upgrades_to_v2_remote_domain() {
        let v1 = RemotePermissionRequest::V1(v01::RemotePermissionRequest::ExternalRequest {
            url: "https://api.example.com/x".into(),
        });
        assert_eq!(
            v1.into_version(Version::V2),
            Ok(RemotePermissionRequest::V2(v02::RemotePermissionRequest {
                permissions: vec![v02::RemotePermission::Remote {
                    domains: vec!["api.example.com".into()]
                }],
            })),
        );
    }

    #[test]
    fn v1_transaction_submit_upgrades_to_v2_chain_submit() {
        let v1 = RemotePermissionRequest::V1(v01::RemotePermissionRequest::TransactionSubmit);
        assert_eq!(
            v1.into_version(Version::V2),
            Ok(RemotePermissionRequest::V2(v02::RemotePermissionRequest {
                permissions: vec![v02::RemotePermission::ChainSubmit],
            })),
        );
    }

    #[test]
    fn v1_external_request_with_unparseable_url_falls_back_to_raw() {
        let v1 = RemotePermissionRequest::V1(v01::RemotePermissionRequest::ExternalRequest {
            url: "not a url".into(),
        });
        assert_eq!(
            v1.into_version(Version::V2),
            Ok(RemotePermissionRequest::V2(v02::RemotePermissionRequest {
                permissions: vec![v02::RemotePermission::Remote {
                    domains: vec!["not a url".into()],
                }],
            })),
        );
    }

    #[test]
    fn device_permission_v2_to_v1_is_rejected_when_no_v1_counterpart_exists() {
        let v2 = HostDevicePermissionRequest::V2(v02::HostDevicePermissionRequest::Notifications);
        assert_eq!(v2.into_version(Version::V1), Err(()));
    }

    #[test]
    fn response_into_version_picks_target() {
        assert_eq!(
            HostDevicePermissionResponse::V1(v01::HostDevicePermissionResponse { granted: true })
                .into_version(Version::V2),
            Ok(HostDevicePermissionResponse::V2(
                v01::HostDevicePermissionResponse { granted: true }
            ))
        );
        assert_eq!(
            HostDevicePermissionResponse::V2(v01::HostDevicePermissionResponse { granted: false })
                .into_latest(),
            Ok(HostDevicePermissionResponse::V2(
                v01::HostDevicePermissionResponse { granted: false }
            ))
        );
    }
}
