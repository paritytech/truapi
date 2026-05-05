//! Versioned wrappers for [`ChainInteraction`](super::super::v02::ChainInteraction) methods.

use parity_scale_codec::{Decode, Encode};

use super::Versioned;
use crate::v02::{
    ChainHeadBlockRequest, ChainHeadCallRequest, ChainHeadEvent, ChainHeadFollowRequest,
    ChainHeadOperationRequest, ChainHeadStorageRequest, ChainHeadUnpinRequest,
    ChainTransactionBroadcastRequest, ChainTransactionStopRequest, GenesisHash, Hex,
    OperationStartedResult,
};

/// Subscription request wrapper for `remote_chain_head_follow`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainHeadFollowRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ChainHeadFollowRequest),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ChainHeadFollowRequest),
}

impl Versioned for RemoteChainHeadFollowRequest {
    type Inner = ChainHeadFollowRequest;
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

/// Subscription item wrapper for `remote_chain_head_follow`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainHeadFollowItem {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ChainHeadEvent),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ChainHeadEvent),
}

impl Versioned for RemoteChainHeadFollowItem {
    type Inner = ChainHeadEvent;
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

/// Request wrapper for `remote_chain_head_header`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainHeadHeaderRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ChainHeadBlockRequest),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ChainHeadBlockRequest),
}

impl Versioned for RemoteChainHeadHeaderRequest {
    type Inner = ChainHeadBlockRequest;
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

/// Response wrapper for `remote_chain_head_header`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainHeadHeaderResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(Option<Hex>),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(Option<Hex>),
}

impl Versioned for RemoteChainHeadHeaderResponse {
    type Inner = Option<Hex>;
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

/// Request wrapper for `remote_chain_head_body`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainHeadBodyRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ChainHeadBlockRequest),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ChainHeadBlockRequest),
}

impl Versioned for RemoteChainHeadBodyRequest {
    type Inner = ChainHeadBlockRequest;
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

/// Response wrapper for `remote_chain_head_body`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainHeadBodyResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(OperationStartedResult),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(OperationStartedResult),
}

impl Versioned for RemoteChainHeadBodyResponse {
    type Inner = OperationStartedResult;
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

/// Request wrapper for `remote_chain_head_storage`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainHeadStorageRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ChainHeadStorageRequest),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ChainHeadStorageRequest),
}

impl Versioned for RemoteChainHeadStorageRequest {
    type Inner = ChainHeadStorageRequest;
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

/// Response wrapper for `remote_chain_head_storage`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainHeadStorageResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(OperationStartedResult),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(OperationStartedResult),
}

impl Versioned for RemoteChainHeadStorageResponse {
    type Inner = OperationStartedResult;
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

/// Request wrapper for `remote_chain_head_call`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainHeadCallRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ChainHeadCallRequest),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ChainHeadCallRequest),
}

impl Versioned for RemoteChainHeadCallRequest {
    type Inner = ChainHeadCallRequest;
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

/// Response wrapper for `remote_chain_head_call`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainHeadCallResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(OperationStartedResult),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(OperationStartedResult),
}

impl Versioned for RemoteChainHeadCallResponse {
    type Inner = OperationStartedResult;
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

/// Request wrapper for `remote_chain_head_unpin`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainHeadUnpinRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ChainHeadUnpinRequest),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ChainHeadUnpinRequest),
}

impl Versioned for RemoteChainHeadUnpinRequest {
    type Inner = ChainHeadUnpinRequest;
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

/// Response wrapper for `remote_chain_head_unpin`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainHeadUnpinResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1,
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2,
}

impl Versioned for RemoteChainHeadUnpinResponse {
    type Inner = ();
    fn wrap(version: u8, _: ()) -> Self {
        match version {
            1 => Self::V1,
            _ => Self::V2,
        }
    }
    fn into_inner(self) {}
}

/// Request wrapper for `remote_chain_head_continue`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainHeadContinueRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ChainHeadOperationRequest),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ChainHeadOperationRequest),
}

impl Versioned for RemoteChainHeadContinueRequest {
    type Inner = ChainHeadOperationRequest;
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

/// Response wrapper for `remote_chain_head_continue`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainHeadContinueResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1,
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2,
}

impl Versioned for RemoteChainHeadContinueResponse {
    type Inner = ();
    fn wrap(version: u8, _: ()) -> Self {
        match version {
            1 => Self::V1,
            _ => Self::V2,
        }
    }
    fn into_inner(self) {}
}

/// Request wrapper for `remote_chain_head_stop_operation`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainHeadStopOperationRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ChainHeadOperationRequest),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ChainHeadOperationRequest),
}

impl Versioned for RemoteChainHeadStopOperationRequest {
    type Inner = ChainHeadOperationRequest;
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

/// Response wrapper for `remote_chain_head_stop_operation`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainHeadStopOperationResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1,
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2,
}

impl Versioned for RemoteChainHeadStopOperationResponse {
    type Inner = ();
    fn wrap(version: u8, _: ()) -> Self {
        match version {
            1 => Self::V1,
            _ => Self::V2,
        }
    }
    fn into_inner(self) {}
}

/// Request wrapper for `remote_chain_spec_genesis_hash`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainSpecGenesisHashRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(GenesisHash),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(GenesisHash),
}

impl Versioned for RemoteChainSpecGenesisHashRequest {
    type Inner = GenesisHash;
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

/// Response wrapper for `remote_chain_spec_genesis_hash`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainSpecGenesisHashResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(Hex),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(Hex),
}

impl Versioned for RemoteChainSpecGenesisHashResponse {
    type Inner = Hex;
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

/// Request wrapper for `remote_chain_spec_chain_name`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainSpecChainNameRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(GenesisHash),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(GenesisHash),
}

impl Versioned for RemoteChainSpecChainNameRequest {
    type Inner = GenesisHash;
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

/// Response wrapper for `remote_chain_spec_chain_name`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainSpecChainNameResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(String),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(String),
}

impl Versioned for RemoteChainSpecChainNameResponse {
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

/// Request wrapper for `remote_chain_spec_properties`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainSpecPropertiesRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(GenesisHash),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(GenesisHash),
}

impl Versioned for RemoteChainSpecPropertiesRequest {
    type Inner = GenesisHash;
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

/// Response wrapper for `remote_chain_spec_properties`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainSpecPropertiesResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(String),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(String),
}

impl Versioned for RemoteChainSpecPropertiesResponse {
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

/// Request wrapper for `remote_chain_transaction_broadcast`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainTransactionBroadcastRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ChainTransactionBroadcastRequest),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ChainTransactionBroadcastRequest),
}

impl Versioned for RemoteChainTransactionBroadcastRequest {
    type Inner = ChainTransactionBroadcastRequest;
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

/// Response wrapper for `remote_chain_transaction_broadcast`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainTransactionBroadcastResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(Option<String>),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(Option<String>),
}

impl Versioned for RemoteChainTransactionBroadcastResponse {
    type Inner = Option<String>;
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

/// Request wrapper for `remote_chain_transaction_stop`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainTransactionStopRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(ChainTransactionStopRequest),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(ChainTransactionStopRequest),
}

impl Versioned for RemoteChainTransactionStopRequest {
    type Inner = ChainTransactionStopRequest;
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

/// Response wrapper for `remote_chain_transaction_stop`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemoteChainTransactionStopResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1,
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2,
}

impl Versioned for RemoteChainTransactionStopResponse {
    type Inner = ();
    fn wrap(version: u8, _: ()) -> Self {
        match version {
            1 => Self::V1,
            _ => Self::V2,
        }
    }
    fn into_inner(self) {}
}
