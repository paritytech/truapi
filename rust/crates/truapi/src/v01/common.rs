use parity_scale_codec::{Decode, Encode};

/// Generic error payload carrying a human-readable reason string.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct GenericErr {
    pub reason: String,
}

/// Single-variant error enum wrapping [`GenericErr`]. Used by many methods as a
/// catch-all error type.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum GenericError {
    GenericError(GenericErr),
}

/// Device capability to request access to.
///
/// Extended with `Notifications`, `NFC`, `Clipboard`, `OpenUrl`, and
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

/// A single remote-operation permission entry.
///
/// The [`crate::api::Permissions::remote_permission`] method accepts a
/// `Vec<RemotePermission>` so products can batch multiple permission requests
/// into a single prompt.
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

/// Response indicating whether a device permission was granted.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostDevicePermissionResponse {
    /// Whether the permission was granted.
    pub granted: bool,
}

/// Response indicating whether a remote permission was granted.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePermissionResponse {
    /// Whether the permission was granted.
    pub granted: bool,
}
