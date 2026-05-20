use core::fmt;

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
    #[display("notifications")]
    Notifications,
    #[display("camera")]
    Camera,
    #[display("microphone")]
    Microphone,
    #[display("bluetooth")]
    Bluetooth,
    #[display("NFC")]
    NFC,
    #[display("location")]
    Location,
    #[display("clipboard")]
    Clipboard,
    #[display("open URL")]
    OpenUrl,
    #[display("biometrics")]
    Biometrics,
}

/// One remote-operation permission requested by the product (RFC 0002).
///
/// `ChainSubmit`, `PreimageSubmit`, and `StatementSubmit` are also triggered
/// implicitly by the corresponding business calls when not yet granted.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Display)]
pub enum RemotePermission {
    /// Outbound HTTP/WebSocket access to a set of domains.
    #[display("access to {}", DisplayDomains(domains))]
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
}

struct DisplayDomains<'a>(&'a [String]);

impl fmt::Display for DisplayDomains<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            return f.write_str("(no domains)");
        }
        let mut sorted: Vec<&str> = self.0.iter().map(String::as_str).collect();
        sorted.sort();
        for (i, domain) in sorted.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            f.write_str(domain)?;
        }
        Ok(())
    }
}

/// Batched remote-permission request (RFC 0002).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Display)]
#[display("{}", DisplayPermissions(permissions))]
pub struct RemotePermissionRequest {
    /// Permissions requested by the product.
    pub permissions: Vec<RemotePermission>,
}

struct DisplayPermissions<'a>(&'a [RemotePermission]);

impl fmt::Display for DisplayPermissions<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            return f.write_str("(empty)");
        }
        for (i, permission) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{permission}")?;
        }
        Ok(())
    }
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
