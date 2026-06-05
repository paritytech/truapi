use parity_scale_codec::{Decode, Encode};

/// Preimage submission error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum PreimageSubmitError {
    /// Catch-all.
    Unknown { reason: String },
}

/// Request to subscribe to preimage lookup results.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageLookupSubscribeRequest {
    /// Hash of the preimage.
    pub key: Vec<u8>,
}

/// Item containing an optional preimage lookup result.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageLookupSubscribeItem {
    /// Preimage data, if found.
    pub value: Option<Vec<u8>>,
}
