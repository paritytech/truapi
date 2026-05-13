use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostDeriveEntropyError {
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostDeriveEntropyRequest {
    pub context: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostDeriveEntropyResponse {
    pub entropy: [u8; 32],
}
