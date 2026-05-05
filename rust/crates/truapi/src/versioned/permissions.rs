//! Versioned wrappers for [`Permissions`](super::super::v02::Permissions) methods.

use parity_scale_codec::{Decode, Encode};

use super::Versioned;
use crate::v01;
use crate::v02::{DevicePermission, RemotePermission};

/// Request wrapper for `host_device_permission`.
///
/// V1 mirrors `host-api@0.6.x`'s `Status('Camera', 'Microphone', 'Bluetooth',
/// 'Location')` enum; V2 widens it per RFC-0001.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostDevicePermissionRequest {
    /// Pre-RFC-0001 four-variant enum, as shipped by `@novasamatech/host-api@0.6.x`.
    #[codec(index = 0)]
    V1(v01::DevicePermissionRequest),
    /// RFC-0001 nine-variant enum.
    #[codec(index = 1)]
    V2(DevicePermission),
}

impl Versioned for HostDevicePermissionRequest {
    type Inner = DevicePermission;
    fn wrap(_version: u8, inner: Self::Inner) -> Self {
        // Server-side requests are decoded from the wire, never re-encoded.
        // Always producing V2 reflects the "latest version" choice for any
        // hypothetical client-side use; the v02 → v01 direction would be
        // lossy and is intentionally unsupported.
        Self::V2(inner)
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(p) => match p {
                v01::DevicePermissionRequest::Camera => DevicePermission::Camera,
                v01::DevicePermissionRequest::Microphone => DevicePermission::Microphone,
                v01::DevicePermissionRequest::Bluetooth => DevicePermission::Bluetooth,
                v01::DevicePermissionRequest::Location => DevicePermission::Location,
            },
            Self::V2(p) => p,
        }
    }
}

/// Response wrapper for `host_device_permission`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostDevicePermissionResponse {
    #[codec(index = 0)]
    V1(bool),
    #[codec(index = 1)]
    V2(bool),
}

impl Versioned for HostDevicePermissionResponse {
    type Inner = bool;
    fn wrap(version: u8, inner: bool) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> bool {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Request wrapper for `remote_permission`.
///
/// V1 mirrors `host-api@0.6.x`'s single-permission `ExternalRequest(String) |
/// TransactionSubmit`; V2 batches multiple [`RemotePermission`] entries per
/// RFC-0001.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemotePermissionRequest {
    /// Pre-RFC-0001 single-permission request.
    #[codec(index = 0)]
    V1(v01::RemotePermissionRequestV1),
    /// RFC-0001 batch request.
    #[codec(index = 1)]
    V2(Vec<RemotePermission>),
}

impl Versioned for RemotePermissionRequest {
    type Inner = Vec<RemotePermission>;
    fn wrap(_version: u8, inner: Self::Inner) -> Self {
        // See HostDevicePermissionRequest::wrap; same rationale.
        Self::V2(inner)
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(p) => match p {
                v01::RemotePermissionRequestV1::ExternalRequest(url) => {
                    let host = url_host(&url).unwrap_or(url);
                    vec![RemotePermission::Remote(vec![host])]
                }
                v01::RemotePermissionRequestV1::TransactionSubmit => {
                    vec![RemotePermission::ChainSubmit]
                }
            },
            Self::V2(v) => v,
        }
    }
}

/// Extract the host portion of a URL. Tiny hand-rolled parse to avoid
/// pulling the `url` crate into the trait crate.
fn url_host(input: &str) -> Option<String> {
    let after_scheme = input.split_once("://")?.1;
    let host = after_scheme.split(['/', '?', '#', ':']).next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

/// Response wrapper for `remote_permission`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemotePermissionResponse {
    #[codec(index = 0)]
    V1(bool),
    #[codec(index = 1)]
    V2(bool),
}

impl Versioned for RemotePermissionResponse {
    type Inner = bool;
    fn wrap(version: u8, inner: bool) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> bool {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_host_extracts_authority() {
        assert_eq!(
            url_host("https://api.example.com/v1/x?y=1"),
            Some("api.example.com".into())
        );
        assert_eq!(url_host("http://localhost:3000"), Some("localhost".into()));
        assert_eq!(
            url_host("wss://relay.example.com"),
            Some("relay.example.com".into())
        );
        assert_eq!(url_host("not a url"), None);
        assert_eq!(url_host(""), None);
    }

    #[test]
    fn v1_external_request_maps_to_single_remote_domain() {
        let v1 = RemotePermissionRequest::V1(v01::RemotePermissionRequestV1::ExternalRequest(
            "https://api.example.com/x".into(),
        ));
        assert_eq!(
            v1.into_inner(),
            vec![RemotePermission::Remote(vec!["api.example.com".into()])]
        );
    }

    #[test]
    fn v1_transaction_submit_maps_to_chain_submit() {
        let v1 = RemotePermissionRequest::V1(v01::RemotePermissionRequestV1::TransactionSubmit);
        assert_eq!(v1.into_inner(), vec![RemotePermission::ChainSubmit]);
    }

    #[test]
    fn v1_external_request_with_unparseable_url_falls_back_to_raw() {
        let v1 = RemotePermissionRequest::V1(v01::RemotePermissionRequestV1::ExternalRequest(
            "not a url".into(),
        ));
        assert_eq!(
            v1.into_inner(),
            vec![RemotePermission::Remote(vec!["not a url".into()])]
        );
    }

    #[test]
    fn response_wrap_picks_version() {
        assert!(matches!(
            HostDevicePermissionResponse::wrap(1, true),
            HostDevicePermissionResponse::V1(true)
        ));
        assert!(matches!(
            HostDevicePermissionResponse::wrap(2, false),
            HostDevicePermissionResponse::V2(false)
        ));
        // Unknown version → latest.
        assert!(matches!(
            HostDevicePermissionResponse::wrap(99, true),
            HostDevicePermissionResponse::V2(true)
        ));
    }
}
