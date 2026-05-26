use parity_scale_codec::{Decode, Encode};

/// Device-capability permission requested from the host (RFC 0002).
///
/// The user's decision is persisted indefinitely after the first prompt and
/// survives app restarts, whether the decision was grant or deny; the host
/// does not re-prompt on subsequent requests for the same capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[allow(clippy::upper_case_acronyms)]
pub enum HostDevicePermissionRequest {
    Notifications,
    Camera,
    Microphone,
    Bluetooth,
    NFC,
    Location,
    Clipboard,
    OpenUrl,
    Biometrics,
}

/// One remote-operation permission requested by the product (RFC 0002).
///
/// `ChainSubmit`, `PreimageSubmit`, and `StatementSubmit` are also triggered
/// implicitly by the corresponding business calls when not yet granted.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemotePermission {
    /// Outbound HTTP/WebSocket access to a set of domains.
    Remote {
        /// Domain patterns requested by the product.
        domains: Vec<String>,
    },
    /// WebRTC media access.
    WebRtc,
    /// Submitting transactions on behalf of the user via `remote_chain_transaction_broadcast`.
    ChainSubmit,
    /// Submitting preimages on behalf of the user via `remote_preimage_submit`.
    PreimageSubmit,
    /// Submitting statements on behalf of the user via `remote_statement_store_submit`.
    StatementSubmit,
}

/// Batched remote-permission request (RFC 0002).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePermissionRequest {
    /// Permissions requested by the product.
    pub permissions: Vec<RemotePermission>,
}

/// Outcome of a device-permission request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostDevicePermissionResponse {
    /// Whether the permission was granted.
    pub granted: bool,
}

/// Outcome of a remote-permission batch request. The decision applies to the
/// whole batch.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePermissionResponse {
    /// Whether the permission was granted.
    pub granted: bool,
}
