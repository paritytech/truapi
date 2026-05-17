use core::fmt::{self, Display, Formatter};
use parity_scale_codec::{Decode, Encode};

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

impl Display for HostDevicePermissionRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Notifications => "notifications",
            Self::Camera => "camera",
            Self::Microphone => "microphone",
            Self::Bluetooth => "bluetooth",
            Self::NFC => "NFC",
            Self::Location => "location",
            Self::Clipboard => "clipboard",
            Self::OpenUrl => "open URL",
            Self::Biometrics => "biometrics",
        };
        f.write_str(label)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemotePermission {
    Remote {
        /// Domain patterns requested by the product.
        domains: Vec<String>,
    },
    WebRtc,
    ChainSubmit,
    PreimageSubmit,
    StatementSubmit,
}

impl Display for RemotePermission {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Remote { domains } => {
                if domains.is_empty() {
                    f.write_str("access to (no domains)")
                } else {
                    let mut sorted: Vec<&str> = domains.iter().map(String::as_str).collect();
                    sorted.sort();
                    write!(f, "access to {}", sorted.join(", "))
                }
            }
            Self::WebRtc => f.write_str("WebRTC connections"),
            Self::ChainSubmit => f.write_str("submit chain transactions"),
            Self::PreimageSubmit => f.write_str("submit preimages"),
            Self::StatementSubmit => f.write_str("submit statements"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePermissionRequest {
    /// Permissions requested by the product.
    pub permissions: Vec<RemotePermission>,
}

impl Display for RemotePermissionRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.permissions.is_empty() {
            return f.write_str("(empty)");
        }
        for (i, perm) in self.permissions.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{perm}")?;
        }
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_host_device_notifications() {
        assert_eq!(
            format!("{}", HostDevicePermissionRequest::Notifications),
            "notifications"
        );
    }

    #[test]
    fn display_host_device_camera() {
        assert_eq!(format!("{}", HostDevicePermissionRequest::Camera), "camera");
    }

    #[test]
    fn display_host_device_microphone() {
        assert_eq!(
            format!("{}", HostDevicePermissionRequest::Microphone),
            "microphone"
        );
    }

    #[test]
    fn display_host_device_bluetooth() {
        assert_eq!(
            format!("{}", HostDevicePermissionRequest::Bluetooth),
            "bluetooth"
        );
    }

    #[test]
    fn display_host_device_nfc() {
        assert_eq!(format!("{}", HostDevicePermissionRequest::NFC), "NFC");
    }

    #[test]
    fn display_host_device_location() {
        assert_eq!(
            format!("{}", HostDevicePermissionRequest::Location),
            "location"
        );
    }

    #[test]
    fn display_host_device_clipboard() {
        assert_eq!(
            format!("{}", HostDevicePermissionRequest::Clipboard),
            "clipboard"
        );
    }

    #[test]
    fn display_host_device_open_url() {
        assert_eq!(
            format!("{}", HostDevicePermissionRequest::OpenUrl),
            "open URL"
        );
    }

    #[test]
    fn display_host_device_biometrics() {
        assert_eq!(
            format!("{}", HostDevicePermissionRequest::Biometrics),
            "biometrics"
        );
    }

    #[test]
    fn display_remote_permission_webrtc() {
        assert_eq!(
            format!("{}", RemotePermission::WebRtc),
            "WebRTC connections"
        );
    }

    #[test]
    fn display_remote_permission_chain_submit() {
        assert_eq!(
            format!("{}", RemotePermission::ChainSubmit),
            "submit chain transactions"
        );
    }

    #[test]
    fn display_remote_permission_preimage_submit() {
        assert_eq!(
            format!("{}", RemotePermission::PreimageSubmit),
            "submit preimages"
        );
    }

    #[test]
    fn display_remote_permission_statement_submit() {
        assert_eq!(
            format!("{}", RemotePermission::StatementSubmit),
            "submit statements"
        );
    }

    #[test]
    fn display_remote_permission_remote_empty_domains() {
        let perm = RemotePermission::Remote { domains: vec![] };
        assert_eq!(format!("{perm}"), "access to (no domains)");
    }

    #[test]
    fn display_remote_permission_remote_single_domain() {
        let perm = RemotePermission::Remote {
            domains: vec!["example.com".into()],
        };
        assert_eq!(format!("{perm}"), "access to example.com");
    }

    #[test]
    fn display_remote_permission_remote_multi_domain_sorted() {
        let perm = RemotePermission::Remote {
            domains: vec!["zeta.io".into(), "alpha.io".into(), "mid.io".into()],
        };
        assert_eq!(format!("{perm}"), "access to alpha.io, mid.io, zeta.io");
    }

    #[test]
    fn display_remote_permission_request_empty() {
        let req = RemotePermissionRequest {
            permissions: vec![],
        };
        assert_eq!(format!("{req}"), "(empty)");
    }

    #[test]
    fn display_remote_permission_request_multi_variant() {
        let req = RemotePermissionRequest {
            permissions: vec![
                RemotePermission::Remote {
                    domains: vec!["b.io".into(), "a.io".into()],
                },
                RemotePermission::WebRtc,
                RemotePermission::ChainSubmit,
            ],
        };
        assert_eq!(
            format!("{req}"),
            "access to a.io, b.io; WebRTC connections; submit chain transactions"
        );
    }
}
