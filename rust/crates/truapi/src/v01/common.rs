use parity_scale_codec::{Decode, Encode};

/// Generic error payload carrying a human-readable reason string.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct GenericErr {
    pub reason: String,
}

/// Single-variant error enum wrapping [`GenericErr`]. Used by many methods as a
/// catch-all error type.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum GenericError {
    GenericError(GenericErr),
}
