use parity_scale_codec::{Decode, Encode};

/// Error from [`crate::api::Entropy::derive`] (RFC 0007).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostDeriveEntropyError {
    /// Catch-all.
    Unknown { reason: String },
}

/// Request to derive deterministic per-product entropy (RFC 0007).
///
/// The host derives 32 bytes from product-scoped seed material and `context`.
/// Repeated calls with the same `context` for the same product yield the same
/// entropy.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostDeriveEntropyRequest {
    /// Domain-separated derivation context.
    pub context: Vec<u8>,
}

/// Response carrying 32 bytes of deterministically derived entropy.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostDeriveEntropyResponse {
    /// 32 bytes of derived entropy.
    pub entropy: [u8; 32],
}
