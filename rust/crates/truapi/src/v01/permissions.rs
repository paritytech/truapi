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

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemotePermission {
    Remote { domains: Vec<String> },
    WebRtc,
    ChainSubmit,
    PreimageSubmit,
    StatementSubmit,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePermissionRequest {
    pub permissions: Vec<RemotePermission>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostDevicePermissionResponse {
    pub granted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePermissionResponse {
    pub granted: bool,
}
