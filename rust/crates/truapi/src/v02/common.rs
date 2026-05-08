use parity_scale_codec::{Decode, Encode};

use crate::v01::GenericErr;

/// Handshake error. Mirrors Novasama's `HandshakeErr` byte-for-byte so that
/// pre-codegen products (built against `@novasamatech/host-api`) can decode
/// `host_handshake_response` frames produced by this host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostHandshakeError {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[allow(clippy::upper_case_acronyms)]
pub enum HostDevicePermissionRequest {
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

impl TryFrom<crate::v01::HostDevicePermissionRequest> for HostDevicePermissionRequest {
    type Error = ();

    fn try_from(value: crate::v01::HostDevicePermissionRequest) -> Result<Self, Self::Error> {
        Ok(match value {
            crate::v01::HostDevicePermissionRequest::Camera => Self::Camera,
            crate::v01::HostDevicePermissionRequest::Microphone => Self::Microphone,
            crate::v01::HostDevicePermissionRequest::Bluetooth => Self::Bluetooth,
            crate::v01::HostDevicePermissionRequest::Location => Self::Location,
        })
    }
}

impl TryFrom<HostDevicePermissionRequest> for crate::v01::HostDevicePermissionRequest {
    type Error = ();

    fn try_from(value: HostDevicePermissionRequest) -> Result<Self, Self::Error> {
        match value {
            HostDevicePermissionRequest::Camera => Ok(Self::Camera),
            HostDevicePermissionRequest::Microphone => Ok(Self::Microphone),
            HostDevicePermissionRequest::Bluetooth => Ok(Self::Bluetooth),
            HostDevicePermissionRequest::Location => Ok(Self::Location),
            HostDevicePermissionRequest::Notifications
            | HostDevicePermissionRequest::NFC
            | HostDevicePermissionRequest::Clipboard
            | HostDevicePermissionRequest::OpenUrl
            | HostDevicePermissionRequest::Biometrics => Err(()),
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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemotePermission {
    /// HTTP/HTTPS/WS/WSS access to specific domains. Each string is a domain
    /// pattern: `"api.example.com"` (exact), `"*.example.com"` (wildcard
    /// subdomain), or `"*"` (all hosts).
    Remote {
        /// Domain patterns requested by the product.
        domains: Vec<String>,
    },
    /// WebRTC access, can expose the user's IP address.
    WebRtc,
    /// Broadcast signed transactions via
    /// [`crate::api::ChainInteraction::remote_chain_transaction_broadcast`].
    ChainSubmit,
    /// Submit a preimage via [`crate::api::Preimage::remote_preimage_submit`].
    PreimageSubmit,
    /// Submit statements via [`crate::api::StatementStore::remote_statement_store_submit`].
    StatementSubmit,
}

/// Request containing batched remote-operation permissions.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePermissionRequest {
    /// Permissions requested by the product.
    pub permissions: Vec<RemotePermission>,
}

impl TryFrom<crate::v01::RemotePermissionRequest> for RemotePermissionRequest {
    type Error = ();

    fn try_from(value: crate::v01::RemotePermissionRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            permissions: match value {
                crate::v01::RemotePermissionRequest::ExternalRequest { url } => {
                    let host = url_host(&url).unwrap_or(url);
                    vec![RemotePermission::Remote {
                        domains: vec![host],
                    }]
                }
                crate::v01::RemotePermissionRequest::TransactionSubmit => {
                    vec![RemotePermission::ChainSubmit]
                }
            },
        })
    }
}

impl TryFrom<RemotePermissionRequest> for crate::v01::RemotePermissionRequest {
    type Error = ();

    fn try_from(_value: RemotePermissionRequest) -> Result<Self, Self::Error> {
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
