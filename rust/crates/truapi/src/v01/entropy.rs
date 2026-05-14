use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostDeriveEntropyError {
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostDeriveEntropyRequest {
    /// Domain-separated derivation context.
    pub context: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostDeriveEntropyResponse {
    /// 32 bytes of derived entropy.
    pub entropy: [u8; 32],
}
