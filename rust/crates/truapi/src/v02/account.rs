use parity_scale_codec::{Decode, Encode};

/// The user's primary DotNS account identity.
///
/// V0.2.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostGetUserIdResponse {
    /// The user's primary DotNS identifier.
    pub dot_ns_identifier: String,
    /// The user's primary public key.
    pub public_key: Vec<u8>,
}

/// Error from [`crate::api::AccountManagement::host_get_user_id`].
///
/// V0.2.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostGetUserIdError {
    /// User denied the identity disclosure request.
    Rejected,
    /// User is not logged in.
    NotConnected,
    /// Catch-all.
    Unknown { reason: String },
}
