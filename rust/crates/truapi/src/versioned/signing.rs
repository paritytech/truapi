//! Versioned wrappers for [`Signing`](super::super::v02::Signing) methods.

use parity_scale_codec::{Decode, Encode};

use super::Versioned;
use crate::v02::{
    Bytes, ProductAccountId, SigningPayload, SigningRawPayload, SigningResult, VersionedTxPayload,
};

/// Request wrapper for `host_sign_payload`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostSignPayloadRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(SigningPayload),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(SigningPayload),
}

impl Versioned for HostSignPayloadRequest {
    type Inner = SigningPayload;
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

/// Response wrapper for `host_sign_payload`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostSignPayloadResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(SigningResult),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(SigningResult),
}

impl Versioned for HostSignPayloadResponse {
    type Inner = SigningResult;
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

/// Request wrapper for `host_sign_raw`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostSignRawRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(SigningRawPayload),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(SigningRawPayload),
}

impl Versioned for HostSignRawRequest {
    type Inner = SigningRawPayload;
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

/// Response wrapper for `host_sign_raw`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostSignRawResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(SigningResult),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(SigningResult),
}

impl Versioned for HostSignRawResponse {
    type Inner = SigningResult;
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

/// Request wrapper for `host_create_transaction`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostCreateTransactionRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1 {
        product_account_id: ProductAccountId,
        payload: VersionedTxPayload,
    },
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2 {
        product_account_id: ProductAccountId,
        payload: VersionedTxPayload,
    },
}

impl Versioned for HostCreateTransactionRequest {
    type Inner = (ProductAccountId, VersionedTxPayload);
    fn wrap(version: u8, (product_account_id, payload): Self::Inner) -> Self {
        match version {
            1 => Self::V1 {
                product_account_id,
                payload,
            },
            _ => Self::V2 {
                product_account_id,
                payload,
            },
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1 {
                product_account_id,
                payload,
            }
            | Self::V2 {
                product_account_id,
                payload,
            } => (product_account_id, payload),
        }
    }
}

/// Response wrapper for `host_create_transaction`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostCreateTransactionResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(Bytes),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(Bytes),
}

impl Versioned for HostCreateTransactionResponse {
    type Inner = Bytes;
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

/// Request wrapper for `host_create_transaction_with_non_product_account`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostCreateTransactionWithNonProductAccountRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(VersionedTxPayload),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(VersionedTxPayload),
}

impl Versioned for HostCreateTransactionWithNonProductAccountRequest {
    type Inner = VersionedTxPayload;
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

/// Response wrapper for `host_create_transaction_with_non_product_account`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostCreateTransactionWithNonProductAccountResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(Bytes),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(Bytes),
}

impl Versioned for HostCreateTransactionWithNonProductAccountResponse {
    type Inner = Bytes;
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
