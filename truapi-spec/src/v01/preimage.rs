use super::Hex;

/// Hash of the preimage.
pub type PreimageKey = Hex;

/// The preimage data.
pub type PreimageValue = Vec<u8>;

/// Preimage submission error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreimageSubmitError {
    /// Catch-all.
    Unknown { reason: String },
}
