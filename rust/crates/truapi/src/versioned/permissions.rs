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
    /// Versioned wrapper for [`v01::HostDevicePermissionError`] and older versions.
    pub enum HostDevicePermissionError { V1 => v01::HostDevicePermissionError, V2 => v01::HostDevicePermissionError }
    /// Versioned wrapper for [`v02::RemotePermissionRequest`] and older versions.
    pub enum RemotePermissionRequest {
        V1 => v01::RemotePermissionRequest,
        V2 => v02::RemotePermissionRequest,
    }
    /// Versioned wrapper for [`v01::RemotePermissionResponse`] and older versions.
    pub enum RemotePermissionResponse { V1 => v01::RemotePermissionResponse, V2 => v01::RemotePermissionResponse }
    /// Versioned wrapper for [`v01::RemotePermissionError`] and older versions.
    pub enum RemotePermissionError { V1 => v01::RemotePermissionError, V2 => v01::RemotePermissionError }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::versioned::{IntoVersion, Version};

    #[test]
    fn v1_external_request_upgrades_to_v2_remote_domain() {
        let v1 = RemotePermissionRequest::V1(v01::RemotePermissionRequest::ExternalRequest(
            "https://api.example.com/x".into(),
        ));
        assert_eq!(
            v1.into_version(Version::V2),
            Ok(RemotePermissionRequest::V2(vec![
                v02::RemotePermission::Remote(vec!["api.example.com".into()])
            ])),
        );
    }

    #[test]
    fn v1_transaction_submit_upgrades_to_v2_chain_submit() {
        let v1 = RemotePermissionRequest::V1(v01::RemotePermissionRequest::TransactionSubmit);
        assert_eq!(
            v1.into_version(Version::V2),
            Ok(RemotePermissionRequest::V2(vec![
                v02::RemotePermission::ChainSubmit
            ])),
        );
    }

    #[test]
    fn v1_external_request_with_unparseable_url_falls_back_to_raw() {
        let v1 = RemotePermissionRequest::V1(v01::RemotePermissionRequest::ExternalRequest(
            "not a url".into(),
        ));
        assert_eq!(
            v1.into_version(Version::V2),
            Ok(RemotePermissionRequest::V2(vec![
                v02::RemotePermission::Remote(vec!["not a url".into()])
            ])),
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
            HostDevicePermissionResponse::V1(true).into_version(Version::V2),
            Ok(HostDevicePermissionResponse::V2(true))
        );
        assert_eq!(
            HostDevicePermissionResponse::V2(false).into_latest(),
            Ok(HostDevicePermissionResponse::V2(false))
        );
    }
}
