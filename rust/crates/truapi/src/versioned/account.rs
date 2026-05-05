//! Versioned wrappers for [`AccountManagement`](super::super::v02::AccountManagement) methods.

use parity_scale_codec::{Decode, Encode};

use super::Versioned;
use crate::v02::{
    Account, AccountConnectionStatus, Bytes, ContextualAlias, ProductAccountId, RingLocation,
    RingVrfProof, UserIdentity,
};

/// Request wrapper for `host_account_get`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostAccountGetRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ProductAccountId),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ProductAccountId),
}

impl Versioned for HostAccountGetRequest {
    type Inner = ProductAccountId;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Response wrapper for `host_account_get`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostAccountGetResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(Account),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(Account),
}

impl Versioned for HostAccountGetResponse {
    type Inner = Account;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Request wrapper for `host_account_get_alias`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostAccountGetAliasRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ProductAccountId),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ProductAccountId),
}

impl Versioned for HostAccountGetAliasRequest {
    type Inner = ProductAccountId;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Response wrapper for `host_account_get_alias`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostAccountGetAliasResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ContextualAlias),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ContextualAlias),
}

impl Versioned for HostAccountGetAliasResponse {
    type Inner = ContextualAlias;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Request wrapper for `host_account_create_proof`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostAccountCreateProofRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1 {
        product_account_id: ProductAccountId,
        ring_location: RingLocation,
        context: Bytes,
    },
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2 {
        product_account_id: ProductAccountId,
        ring_location: RingLocation,
        context: Bytes,
    },
}

impl Versioned for HostAccountCreateProofRequest {
    type Inner = (ProductAccountId, RingLocation, Bytes);
    fn wrap(version: u8, (product_account_id, ring_location, context): Self::Inner) -> Self {
        match version {
            1 => Self::V1 {
                product_account_id,
                ring_location,
                context,
            },
            _ => Self::V2 {
                product_account_id,
                ring_location,
                context,
            },
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1 {
                product_account_id,
                ring_location,
                context,
            }
            | Self::V2 {
                product_account_id,
                ring_location,
                context,
            } => (product_account_id, ring_location, context),
        }
    }
}

/// Response wrapper for `host_account_create_proof`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostAccountCreateProofResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(RingVrfProof),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(RingVrfProof),
}

impl Versioned for HostAccountCreateProofResponse {
    type Inner = RingVrfProof;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Request wrapper for `host_get_non_product_accounts`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostGetNonProductAccountsRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1,
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2,
}

impl Versioned for HostGetNonProductAccountsRequest {
    type Inner = ();
    fn wrap(version: u8, _: ()) -> Self {
        match version {
            1 => Self::V1,
            _ => Self::V2,
        }
    }
    fn into_inner(self) {}
}

/// Response wrapper for `host_get_non_product_accounts`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostGetNonProductAccountsResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(Vec<Account>),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(Vec<Account>),
}

impl Versioned for HostGetNonProductAccountsResponse {
    type Inner = Vec<Account>;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Subscription item wrapper for `host_account_connection_status_subscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostAccountConnectionStatusItem {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(AccountConnectionStatus),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(AccountConnectionStatus),
}

impl Versioned for HostAccountConnectionStatusItem {
    type Inner = AccountConnectionStatus;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Request wrapper for `host_get_user_id` (V0.2+).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostGetUserIdRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1,
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2,
}

impl Versioned for HostGetUserIdRequest {
    type Inner = ();
    fn wrap(version: u8, _: ()) -> Self {
        match version {
            1 => Self::V1,
            _ => Self::V2,
        }
    }
    fn into_inner(self) {}
}

/// Response wrapper for `host_get_user_id` (V0.2+).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostGetUserIdResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(UserIdentity),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(UserIdentity),
}

impl Versioned for HostGetUserIdResponse {
    type Inner = UserIdentity;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}
