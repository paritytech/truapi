use parity_scale_codec::{Decode, Encode};

use super::common::GenericError;

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
    Unknown(GenericError),
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

/// Platform category a host runs on.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPlatform {
    /// Browser-embedded product (an iframe inside a web host).
    Web,
    /// Android application.
    Android,
    /// iOS application.
    Ios,
    /// Desktop application.
    Desktop,
    /// Host could not classify its platform.
    Unknown,
}

/// Identity and version of the host currently running the product.
///
/// Reported by [`crate::api::System::host_info`] so a product knows which host
/// (and which build of it) is running it — for adapting to the host,
/// telemetry, and attributing behaviour to a concrete build in diagnostics and
/// bug reports.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostInfo {
    /// Platform category the host runs on.
    pub platform: HostPlatform,
    /// Human-readable name of the host implementation, e.g. `"Polkadot
    /// Desktop"`, `"Polkadot Mobile"`, or `"dotli"`. Hosts should report a
    /// stable, non-empty name.
    pub name: String,
    /// Host-native version string, e.g. a semver such as `"1.2.3"`. Hosts
    /// should report a non-empty value; the format is the host's own.
    pub version: String,
}
