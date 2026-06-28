use parity_scale_codec::{Decode, Encode};

/// Framework error variants that the debug-only Testing API can force.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum TestingFrameworkError {
    Denied,
    Unsupported,
    MalformedFrame,
    HostFailure,
}

/// V1 request payload for the debug-only Testing API.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TestingProbeRequest {
    pub message: String,
}

/// Request payload for forcing a framework-level error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TestingFrameworkErrorRequest {
    pub error: TestingFrameworkError,
}

/// V1 response payload for the debug-only Testing API.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TestingProbeResponse {
    pub received_version: u8,
    pub message: String,
}

/// Domain error for the debug-only Testing API.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum TestingProbeError {
    Unknown { reason: String },
}
