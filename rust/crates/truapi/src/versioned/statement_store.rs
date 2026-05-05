//! Versioned wrappers for [`StatementStore`](super::super::v02::StatementStore) methods.

use parity_scale_codec::{Decode, Encode};

use super::Versioned;
use crate::v02::{
    Bytes, ProductAccountId, SignedStatement, Statement, StatementProof, TopicFilter,
};

/// Subscription request wrapper for `remote_statement_store_subscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteStatementStoreSubscribeRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(TopicFilter),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(TopicFilter),
}

impl Versioned for RemoteStatementStoreSubscribeRequest {
    type Inner = TopicFilter;
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

/// Subscription item wrapper for `remote_statement_store_subscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteStatementStoreSubscribeItem {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(Vec<SignedStatement>),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(Vec<SignedStatement>),
}

impl Versioned for RemoteStatementStoreSubscribeItem {
    type Inner = Vec<SignedStatement>;
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

/// Request wrapper for `remote_statement_store_create_proof`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteStatementStoreCreateProofRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1 {
        product_account_id: ProductAccountId,
        statement: Statement,
    },
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2 {
        product_account_id: ProductAccountId,
        statement: Statement,
    },
}

impl Versioned for RemoteStatementStoreCreateProofRequest {
    type Inner = (ProductAccountId, Statement);
    fn wrap(version: u8, (product_account_id, statement): Self::Inner) -> Self {
        match version {
            1 => Self::V1 {
                product_account_id,
                statement,
            },
            _ => Self::V2 {
                product_account_id,
                statement,
            },
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1 {
                product_account_id,
                statement,
            }
            | Self::V2 {
                product_account_id,
                statement,
            } => (product_account_id, statement),
        }
    }
}

/// Response wrapper for `remote_statement_store_create_proof`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteStatementStoreCreateProofResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(StatementProof),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(StatementProof),
}

impl Versioned for RemoteStatementStoreCreateProofResponse {
    type Inner = StatementProof;
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

/// Request wrapper for `remote_statement_store_submit`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteStatementStoreSubmitRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(Bytes),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(Bytes),
}

impl Versioned for RemoteStatementStoreSubmitRequest {
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

/// Response wrapper for `remote_statement_store_submit`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteStatementStoreSubmitResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(String),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(String),
}

impl Versioned for RemoteStatementStoreSubmitResponse {
    type Inner = String;
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
