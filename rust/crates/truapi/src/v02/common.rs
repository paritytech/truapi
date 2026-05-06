use parity_scale_codec::{Decode, Encode};

use crate::v01::GenericErr;

/// Handshake error. Mirrors Novasama's `HandshakeErr` byte-for-byte so that
/// pre-codegen products (built against `@novasamatech/host-api`) can decode
/// `host_handshake_response` frames produced by this host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HandshakeError {
    Timeout,
    UnsupportedProtocolVersion,
    Unknown(GenericErr),
}

/// Device capability to request access to.
///
/// V0.2: extended with `Notifications`, `NFC`, `Clipboard`, `OpenUrl`, and
/// `Biometrics` per [RFC 0001] (JIT permissions).
///
/// [RFC 0001]: https://github.com/paritytech/triangle-js-sdks/pull/66
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
#[allow(clippy::upper_case_acronyms)]
pub enum DevicePermission {
    /// Push notification delivery permission.
    Notifications,
    Camera,
    Microphone,
    Bluetooth,
    /// Near-field communication access.
    NFC,
    Location,
    /// System clipboard access.
    Clipboard,
    /// Open a URL in an external browser.
    OpenUrl,
    /// Biometric authentication (fingerprint, face ID).
    Biometrics,
}

impl TryFrom<crate::v01::DevicePermissionRequest> for DevicePermission {
    type Error = ();

    fn try_from(value: crate::v01::DevicePermissionRequest) -> Result<Self, Self::Error> {
        Ok(match value {
            crate::v01::DevicePermissionRequest::Camera => Self::Camera,
            crate::v01::DevicePermissionRequest::Microphone => Self::Microphone,
            crate::v01::DevicePermissionRequest::Bluetooth => Self::Bluetooth,
            crate::v01::DevicePermissionRequest::Location => Self::Location,
        })
    }
}

impl TryFrom<DevicePermission> for crate::v01::DevicePermissionRequest {
    type Error = ();

    fn try_from(value: DevicePermission) -> Result<Self, Self::Error> {
        match value {
            DevicePermission::Camera => Ok(Self::Camera),
            DevicePermission::Microphone => Ok(Self::Microphone),
            DevicePermission::Bluetooth => Ok(Self::Bluetooth),
            DevicePermission::Location => Ok(Self::Location),
            DevicePermission::Notifications
            | DevicePermission::NFC
            | DevicePermission::Clipboard
            | DevicePermission::OpenUrl
            | DevicePermission::Biometrics => Err(()),
        }
    }
}

/// A single remote-operation permission entry.
///
/// V0.2: replaces `RemotePermissionRequest`. The [`crate::api::Permissions::remote_permission`] method
/// now accepts a `Vec<RemotePermission>` so products can batch multiple
/// permission requests into a single prompt.
///
/// See [RFC 0001] and [issue #64].
///
/// [RFC 0001]: https://github.com/paritytech/triangle-js-sdks/pull/66
/// [issue #64]: https://github.com/paritytech/triangle-js-sdks/issues/64
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemotePermission {
    /// HTTP/HTTPS/WS/WSS access to specific domains. Each string is a domain
    /// pattern: `"api.example.com"` (exact), `"*.example.com"` (wildcard
    /// subdomain), or `"*"` (all hosts).
    Remote(Vec<String>),
    /// WebRTC access — can expose the user's IP address.
    WebRtc,
    /// Broadcast signed transactions via
    /// [`crate::api::ChainInteraction::remote_chain_transaction_broadcast`].
    ChainSubmit,
    /// Submit statements via [`crate::api::StatementStore::remote_statement_store_submit`].
    StatementSubmit,
}

impl TryFrom<crate::v01::RemotePermissionRequestV1> for Vec<RemotePermission> {
    type Error = ();

    fn try_from(value: crate::v01::RemotePermissionRequestV1) -> Result<Self, Self::Error> {
        Ok(match value {
            crate::v01::RemotePermissionRequestV1::ExternalRequest(url) => {
                let host = url_host(&url).unwrap_or(url);
                vec![RemotePermission::Remote(vec![host])]
            }
            crate::v01::RemotePermissionRequestV1::TransactionSubmit => {
                vec![RemotePermission::ChainSubmit]
            }
        })
    }
}

impl TryFrom<Vec<RemotePermission>> for crate::v01::RemotePermissionRequestV1 {
    type Error = ();

    fn try_from(_value: Vec<RemotePermission>) -> Result<Self, Self::Error> {
        Err(())
    }
}

/// Extract the host portion of a URL. Tiny hand-rolled parse to avoid pulling
/// the `url` crate into the trait crate.
fn url_host(input: &str) -> Option<String> {
    let after_scheme = input.split_once("://")?.1;
    let host = after_scheme.split(['/', '?', '#', ':']).next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}
