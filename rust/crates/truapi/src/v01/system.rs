use parity_scale_codec::{Decode, Encode};

use super::common::GenericErr;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFeatureSupportedRequest {
    Chain {
        /// Chain genesis hash.
        genesis_hash: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostNavigateToError {
    PermissionDenied,
    Unknown { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostHandshakeError {
    Timeout,
    UnsupportedProtocolVersion,
    Unknown(GenericErr),
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostHandshakeRequest {
    /// Wire codec version requested by the peer.
    pub codec_version: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFeatureSupportedResponse {
    /// Whether the feature is supported.
    pub supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNavigateToRequest {
    /// URL to open.
    pub url: String,
}
