use parity_scale_codec::{Decode, Encode};

use super::common::GenericErr;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFeatureSupportedRequest {
    Chain { genesis_hash: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostNavigateToError {
    PermissionDenied,
    Unknown { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPushNotificationRequest {
    pub text: String,
    pub deeplink: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostHandshakeError {
    Timeout,
    UnsupportedProtocolVersion,
    Unknown(GenericErr),
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostHandshakeRequest {
    pub codec_version: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFeatureSupportedResponse {
    pub supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNavigateToRequest {
    pub url: String,
}
