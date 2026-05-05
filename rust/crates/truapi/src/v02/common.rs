use parity_scale_codec::{Decode, Encode};

/// Hex-encoded arbitrary bytes (SCALE length-prefixed on the wire).
pub type Hex = Vec<u8>;

/// Arbitrary binary data (SCALE length-prefixed on the wire).
pub type Bytes = Vec<u8>;

/// Blockchain genesis hash, used to identify a specific chain.
pub type GenesisHash = Hex;

/// Generic error payload carrying a human-readable reason string.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct GenericErr {
    pub reason: String,
}

/// Single-variant error enum wrapping [`GenericErr`]. Used by many methods as a
/// catch-all error type.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum GenericError {
    GenericError(GenericErr),
}

/// Feature to check for host support.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum Feature {
    /// Is this blockchain supported?
    Chain(GenesisHash),
}

/// Navigation error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum NavigateToError {
    /// Navigation not allowed.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}

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

/// Push notification payload.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct PushNotification {
    /// Notification text.
    pub text: String,
    /// Optional URL to open on tap.
    pub deeplink: Option<String>,
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

/// A single remote-operation permission entry.
///
/// V0.2: replaces `RemotePermissionRequest`. The [`super::Permissions::remote_permission`] method
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
    /// [`super::ChainInteraction::remote_chain_transaction_broadcast`].
    ChainSubmit,
    /// Submit statements via [`super::StatementStore::remote_statement_store_submit`].
    StatementSubmit,
}
