use derive_more::Display;
use parity_scale_codec::{Decode, Encode};

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

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Display)]
pub enum RemotePermission {
    #[display("access to {}", format_domains(domains))]
    Remote {
        /// Domain patterns requested by the product.
        domains: Vec<String>,
    },
    #[display("WebRTC connections")]
    WebRtc,
    #[display("submit chain transactions")]
    ChainSubmit,
    #[display("submit preimages")]
    PreimageSubmit,
    #[display("submit statements")]
    StatementSubmit,
}

fn format_domains(domains: &[String]) -> String {
    if domains.is_empty() {
        return "(no domains)".into();
    }

    let mut sorted: Vec<&str> = domains.iter().map(String::as_str).collect();
    sorted.sort();
    sorted.join(", ")
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Display)]
#[display("{}", format_permissions(permissions))]
pub struct RemotePermissionRequest {
    /// Permissions requested by the product.
    pub permissions: Vec<RemotePermission>,
}

fn format_permissions(permissions: &[RemotePermission]) -> String {
    if permissions.is_empty() {
        return "(empty)".into();
    }

    permissions
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostDevicePermissionResponse {
    /// Whether the permission was granted.
    pub granted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePermissionResponse {
    /// Whether the permission was granted.
    pub granted: bool,
}
