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

/// Push notification payload.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct PushNotification {
    /// Notification text.
    pub text: String,
    /// Optional URL to open on tap.
    pub deeplink: Option<String>,
}

/// Device capability to request access to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum DevicePermissionRequest {
    Camera,
    Microphone,
    Bluetooth,
    Location,
}

/// Pre-RFC-0001 remote operation permission, as shipped by
/// `@novasamatech/host-api@0.6.x`. Suffixed with `V1` so the simple name
/// doesn't collide with the versioned wrapper `RemotePermissionRequest`
/// when both reach the codegen output (rustdoc flattens module paths).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemotePermissionRequestV1 {
    /// URL the product wants to fetch.
    ExternalRequest(String),
    /// Product wants to submit a transaction.
    TransactionSubmit,
}
