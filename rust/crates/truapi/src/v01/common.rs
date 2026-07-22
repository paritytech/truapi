use parity_scale_codec::{Decode, Encode};

/// Generic error payload carrying a human-readable reason string. Used by many
/// methods as a catch-all error type.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct GenericError {
    /// Human-readable failure reason.
    pub reason: String,
}
