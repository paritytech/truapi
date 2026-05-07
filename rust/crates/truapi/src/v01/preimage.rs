use parity_scale_codec::{Decode, Encode};

use super::Hex;

/// Hash of the preimage.
pub type PreimageKey = Hex;

/// The preimage data.
pub type PreimageValue = Vec<u8>;

/// Preimage submission error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum PreimageSubmitError {
    /// Catch-all.
    Unknown { reason: String },
}

pub type RemotePreimageLookupSubscribeRequest = PreimageKey;
pub type RemotePreimageLookupSubscribeItem = Option<PreimageValue>;
