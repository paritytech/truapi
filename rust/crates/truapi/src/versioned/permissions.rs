//! Versioned wrappers for [`Permissions`](crate::api::Permissions) methods.

use crate::{v01, v02};

versioned_type! {
    /// Request wrapper for `host_device_permission`.
    ///
    /// V1 mirrors `host-api@0.6.x`'s `Status('Camera', 'Microphone', 'Bluetooth',
    /// 'Location')` enum; V2 widens it per RFC-0001.
    pub enum HostDevicePermissionRequest {
        /// Pre-RFC-0001 four-variant enum, as shipped by `@novasamatech/host-api@0.6.x`.
        V1 => v01::DevicePermissionRequest,
        /// RFC-0001 nine-variant enum.
        V2 => v02::DevicePermission,
    }
    /// Response wrapper for `host_device_permission`.
    pub enum HostDevicePermissionResponse { V1 => bool, V2 => bool }
    /// Error wrapper for `host_device_permission`.
    pub enum HostDevicePermissionError { V1 => v01::GenericError, V2 => v01::GenericError }
    /// Request wrapper for `remote_permission`.
    ///
    /// V1 mirrors `host-api@0.6.x`'s single-permission `ExternalRequest(String) |
    /// TransactionSubmit`; V2 batches multiple [`RemotePermission`](v02::RemotePermission)
    /// entries per RFC-0001.
    pub enum RemotePermissionRequest {
        /// Pre-RFC-0001 single-permission request.
        V1 => v01::RemotePermissionRequestV1,
        /// RFC-0001 batch request.
        V2 => Vec<v02::RemotePermission>,
    }
    /// Response wrapper for `remote_permission`.
    pub enum RemotePermissionResponse { V1 => bool, V2 => bool }
    /// Error wrapper for `remote_permission`.
    pub enum RemotePermissionError { V1 => v01::GenericError, V2 => v01::GenericError }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::versioned::{IntoVersion, Version};

    #[test]
    fn v1_external_request_upgrades_to_v2_remote_domain() {
        let v1 = RemotePermissionRequest::V1(v01::RemotePermissionRequestV1::ExternalRequest(
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
        let v1 = RemotePermissionRequest::V1(v01::RemotePermissionRequestV1::TransactionSubmit);
        assert_eq!(
            v1.into_version(Version::V2),
            Ok(RemotePermissionRequest::V2(vec![
                v02::RemotePermission::ChainSubmit
            ])),
        );
    }

    #[test]
    fn v1_external_request_with_unparseable_url_falls_back_to_raw() {
        let v1 = RemotePermissionRequest::V1(v01::RemotePermissionRequestV1::ExternalRequest(
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
        let v2 = HostDevicePermissionRequest::V2(v02::DevicePermission::Notifications);
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
