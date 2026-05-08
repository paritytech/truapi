use parity_scale_codec::{Decode, Encode};

/// V0.2 product account: a public key only, no display name.
///
/// V0.2 replaces V0.1's [`crate::v01::Account`] (which carries `name:
/// Option<String>`) for `host_account_get` responses; the name is no longer
/// returned because it's not bound to the account derivation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProductAccount {
    /// The account public key (variable-length bytes).
    pub public_key: Vec<u8>,
}

impl TryFrom<crate::v01::Account> for ProductAccount {
    type Error = ();

    fn try_from(value: crate::v01::Account) -> Result<Self, Self::Error> {
        Ok(Self {
            public_key: value.public_key,
        })
    }
}

impl TryFrom<ProductAccount> for crate::v01::Account {
    type Error = ();

    fn try_from(value: ProductAccount) -> Result<Self, Self::Error> {
        Ok(Self {
            public_key: value.public_key,
            name: None,
        })
    }
}

/// V0.2 response for [`crate::api::AccountManagement::host_account_get`].
/// Wraps a [`ProductAccount`] (no name field).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountGetResponse {
    /// Retrieved product account.
    pub account: ProductAccount,
}

impl TryFrom<crate::v01::HostAccountGetResponse> for HostAccountGetResponse {
    type Error = ();

    fn try_from(value: crate::v01::HostAccountGetResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            account: value.account.try_into()?,
        })
    }
}

impl TryFrom<HostAccountGetResponse> for crate::v01::HostAccountGetResponse {
    type Error = ();

    fn try_from(value: HostAccountGetResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            account: value.account.try_into()?,
        })
    }
}

/// The user's primary DotNS account identity.
///
/// V0.2.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostGetUserIdResponse {
    /// The user's primary DotNS username.
    pub primary_username: String,
    /// The user's primary public key.
    pub public_key: Vec<u8>,
}

/// Error from [`crate::api::AccountManagement::host_get_user_id`].
///
/// V0.2.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostGetUserIdError {
    /// User denied the identity disclosure request.
    PermissionDenied,
    /// User is not logged in.
    NotConnected,
    /// Catch-all.
    Unknown { reason: String },
}
