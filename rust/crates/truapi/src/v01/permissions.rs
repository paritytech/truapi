use derive_more::Display;
use parity_scale_codec::{Decode, Encode};

/// Device-capability permission requested from the host (RFC 0002).
///
/// The user's decision is persisted indefinitely after the first prompt and
/// survives app restarts, whether the decision was grant or deny; the host
/// does not re-prompt on subsequent requests for the same capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Display)]
#[allow(clippy::upper_case_acronyms)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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
/// `ChainSubmit`, `PreimageSubmit`, and `StatementSubmit` are also triggered
/// implicitly by the corresponding business calls when not yet granted.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Display)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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
    /// Outbound access to one method and path on one domain, carrying a
    /// personhood proof the host attaches (RFC 0025).
    ///
    /// Appended last on purpose. This enum is SCALE-encoded into the persisted
    /// permission key, so changing an existing variant would invalidate every
    /// decision a user has already made.
    #[display("{method} {domain}{path}")]
    Credential {
        /// Domain the grant covers. Covered requests must be `https`, since
        /// the proof would otherwise travel in plaintext.
        domain: String,
        /// Exact path the grant covers. No wildcards.
        path: String,
        /// HTTP method the grant covers.
        method: String,
    },
}

/// remote-permission request (RFC 0002).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Display)]
#[display("{permission}")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 0025 appends `Credential` rather than amending `Remote`, because
    /// this enum is SCALE-encoded into the persisted permission key. Pin every
    /// discriminant: a reorder silently invalidates decisions users already made.
    #[test]
    fn remote_permission_discriminants_are_pinned() {
        assert_eq!(
            RemotePermission::Remote { domains: vec![] }.encode()[0],
            0,
            "Remote must stay at index 0"
        );
        assert_eq!(RemotePermission::WebRtc.encode()[0], 1);
        assert_eq!(RemotePermission::ChainSubmit.encode()[0], 2);
        assert_eq!(RemotePermission::PreimageSubmit.encode()[0], 3);
        assert_eq!(RemotePermission::StatementSubmit.encode()[0], 4);
        assert_eq!(
            RemotePermission::Credential {
                domain: "onramp.example.com".into(),
                path: "/session".into(),
                method: "POST".into(),
            }
            .encode()[0],
            5,
            "Credential must be appended last"
        );
    }

    /// A grant stored before RFC 0025, encoded by the previous definition, must
    /// still decode to the same value. These bytes are literal on purpose: a
    /// round trip through the current code would pass even if the shape moved.
    #[test]
    fn grant_stored_before_this_rfc_still_decodes() {
        // Remote { domains: ["example.com"] }: variant 0, Vec len 1, str len 11.
        let stored: Vec<u8> = [&[0u8, 0x04, 0x2c][..], b"example.com"].concat();

        let decoded = RemotePermission::decode(&mut &stored[..]).expect("pre-RFC grant decodes");
        assert_eq!(
            decoded,
            RemotePermission::Remote {
                domains: vec!["example.com".to_string()],
            }
        );
    }

    #[test]
    fn credential_round_trips_and_displays() {
        let original = RemotePermission::Credential {
            domain: "onramp.example.com".into(),
            path: "/meld/session".into(),
            method: "POST".into(),
        };
        let decoded =
            RemotePermission::decode(&mut &original.encode()[..]).expect("Credential decodes");
        assert_eq!(decoded, original);
        assert_eq!(
            original.to_string(),
            "POST onramp.example.com/meld/session",
            "the prompt names the operation"
        );
    }
}
