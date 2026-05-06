use parity_scale_codec::{Decode, Encode};

/// 32 bytes of deterministic entropy derived from the user's root BIP-39
/// entropy via a three-layer BLAKE2b-256 keyed hashing scheme. The same
/// root account + product + key always yields the same output on any
/// conforming host.
///
/// See [RFC 0007].
///
/// [RFC 0007]: https://github.com/paritytech/triangle-js-sdks/pull/95
pub type Entropy = [u8; 32];

/// Error from [`crate::api::EntropyDerivation::host_derive_entropy`].
///
/// Under normal operation the function always succeeds; `Unknown` indicates an
/// unrecoverable internal host error.
///
/// See [RFC 0007].
///
/// [RFC 0007]: https://github.com/paritytech/triangle-js-sdks/pull/95
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum DeriveEntropyError {
    /// An unexpected error occurred in the host.
    Unknown,
}
