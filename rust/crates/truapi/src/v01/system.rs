use parity_scale_codec::{Decode, Encode};

use super::common::GenericErr;

/// Request to query whether a feature is supported by the host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostFeatureSupportedRequest {
    /// Ask whether the host can interact with the chain identified by genesis hash.
    Chain {
        /// Chain genesis hash.
        genesis_hash: Vec<u8>,
    },
}

/// Error from [`crate::api::System::navigate_to`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostNavigateToError {
    /// User denied the navigation prompt.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}

/// Error from [`crate::api::System::handshake`] (RFC 0009).
///
/// The handshake is the first call on a fresh connection; it does not require
/// user authentication and is used to negotiate the wire codec version.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostHandshakeError {
    /// Host did not complete the handshake in time.
    Timeout,
    /// Host does not speak the codec version requested by the product.
    UnsupportedProtocolVersion,
    /// Catch-all.
    Unknown(GenericErr),
}

/// Wire-codec negotiation payload sent by the product (RFC 0009).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostHandshakeRequest {
    /// Wire codec version requested by the product.
    pub codec_version: u8,
}

/// Response to a feature-support query.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostFeatureSupportedResponse {
    /// Whether the feature is supported.
    pub supported: bool,
}

/// Request to navigate the host to an external URL.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNavigateToRequest {
    /// URL to open.
    pub url: String,
}
