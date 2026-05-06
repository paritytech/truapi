use parity_scale_codec::{Decode, Encode};

use crate::v01::{DotNsIdentifier, PublicKey};

/// The user's primary DotNS account identity.
///
/// V0.2.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct UserIdentity {
    /// The user's primary DotNS identifier.
    pub dot_ns_identifier: DotNsIdentifier,
    /// The user's primary public key.
    pub public_key: PublicKey,
}

/// Error from [`crate::api::AccountManagement::host_get_user_id`].
///
/// V0.2.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum UserIdentityError {
    /// User denied the identity disclosure request.
    Rejected,
    /// User is not logged in.
    NotConnected,
    /// Catch-all.
    Unknown { reason: String },
}
