use parity_scale_codec::{Decode, Encode};

/// Error from [`crate::api::EntropyDerivation::host_derive_entropy`].
///
/// Under normal operation the function always succeeds; `Unknown` indicates an
/// unrecoverable internal host error.
///
/// See [RFC 0007].
///
/// [RFC 0007]: https://github.com/paritytech/triangle-js-sdks/pull/95
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostDeriveEntropyError {
    /// An unexpected error occurred in the host.
    Unknown,
}

/// Request to derive deterministic entropy.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostDeriveEntropyRequest {
    /// Domain-separated derivation context.
    pub context: Vec<u8>,
}

/// Response containing derived deterministic entropy.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostDeriveEntropyResponse {
    /// 32 bytes of derived entropy.
    pub entropy: [u8; 32],
}
