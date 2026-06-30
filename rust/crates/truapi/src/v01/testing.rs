use parity_scale_codec::{Decode, Encode};

use crate::CallError;

/// V1 request payload for the debug-only Testing API.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TestingVersionProbeRequest {
    pub message: String,
}

/// Request payload for echoing a framework/domain error through the wire shape.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct EchoErrorRequest {
    pub error: CallError<TestingVersionProbeError>,
}

/// V1 response payload for the debug-only Testing API.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TestingVersionProbeResponse {
    pub received_version: u8,
    pub message: String,
}

/// Domain error for the debug-only Testing API.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum TestingVersionProbeError {
    Unknown { reason: String },
}
