use derive_more::Display;
use parity_scale_codec::{Decode, Encode};

/// Device-capability permission requested from the host (RFC 0002).
///
/// The user's decision is persisted indefinitely after the first prompt and
/// survives app restarts, whether the decision was grant or deny; the host
/// does not re-prompt on subsequent requests for the same capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Display)]
#[allow(clippy::upper_case_acronyms)]
pub enum HostDevicePermissionRequest {
    /// Showing system notifications.
    #[display("notifications")]
    Notifications,
    /// Camera capture access.
    #[display("camera")]
    Camera,
    /// Microphone capture access.
    #[display("microphone")]
    Microphone,
    /// Bluetooth device access.
    #[display("bluetooth")]
    Bluetooth,
    /// NFC reader access.
    #[display("NFC")]
    NFC,
    /// Geolocation access.
    #[display("location")]
    Location,
    /// Clipboard access.
    #[display("clipboard")]
    Clipboard,
    /// Opening URLs outside the host.
    #[display("open URL")]
    OpenUrl,
    /// Biometric authentication.
    #[display("biometrics")]
    Biometrics,
}

/// One remote-operation permission requested by the product (RFC 0002).
///
/// `ChainSubmit`, `PreimageSubmit`, `StatementSubmit`, and `ContactSend` are
/// also triggered implicitly by the corresponding business calls when not yet
/// granted.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Display)]
pub enum RemotePermission {
    /// Outbound HTTP/WebSocket access to a set of domains.
    #[display("access to {}", domains.join(", "))]
    Remote {
        /// Domain patterns requested by the product.
        domains: Vec<String>,
    },
    /// WebRTC media access.
    #[display("WebRTC connections")]
    WebRtc,
    /// Submitting transactions on behalf of the user via `remote_chain_transaction_broadcast`.
    #[display("submit chain transactions")]
    ChainSubmit,
    /// Submitting preimages on behalf of the user via `remote_preimage_submit`.
    #[display("submit preimages")]
    PreimageSubmit,
    /// Submitting statements on behalf of the user via `remote_statement_store_submit`.
    #[display("submit statements")]
    StatementSubmit,
    /// Sending contact requests on behalf of the user via `host_contacts_send` (RFC 0022).
    #[display("send contact requests")]
    ContactSend,
}

/// remote-permission request (RFC 0002).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Display)]
#[display("{permission}")]
pub struct RemotePermissionRequest {
    /// Permission requested by the product.
    pub permission: RemotePermission,
}

/// Outcome of a device-permission request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostDevicePermissionResponse {
    /// Whether the permission was granted.
    pub granted: bool,
}

/// Outcome of a remote-permission request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePermissionResponse {
    /// Whether the permission was granted.
    pub granted: bool,
}
