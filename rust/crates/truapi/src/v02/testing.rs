use parity_scale_codec::{Decode, Encode};

/// Request payload for the debug-only Testing API.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TestingProbeRequest {
    pub message: String,
    pub marker: u32,
}

/// Response payload for the debug-only Testing API.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TestingProbeResponse {
    pub received_version: u8,
    pub message: String,
    pub marker: u32,
}

/// Domain error for the debug-only Testing API.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum TestingProbeError {
    Unknown { reason: String },
}
