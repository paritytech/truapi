//! Versioned wrappers for [`ChainInteraction`](crate::api::ChainInteraction) methods.

use crate::v01;

versioned_type! {
    /// Subscription request wrapper for `remote_chain_head_follow`.
    pub enum RemoteChainHeadFollowRequest { V1 => v01::ChainHeadFollowRequest }
    /// Subscription item wrapper for `remote_chain_head_follow`.
    pub enum RemoteChainHeadFollowItem { V1 => v01::ChainHeadEvent }
    /// Request wrapper for `remote_chain_head_header`.
    pub enum RemoteChainHeadHeaderRequest { V1 => v01::ChainHeadBlockRequest }
    /// Response wrapper for `remote_chain_head_header`.
    pub enum RemoteChainHeadHeaderResponse { V1 => Option<v01::Hex> }
    /// Error wrapper for `remote_chain_head_header`.
    pub enum RemoteChainHeadHeaderError { V1 => v01::GenericError }
    /// Request wrapper for `remote_chain_head_body`.
    pub enum RemoteChainHeadBodyRequest { V1 => v01::ChainHeadBlockRequest }
    /// Response wrapper for `remote_chain_head_body`.
    pub enum RemoteChainHeadBodyResponse { V1 => v01::OperationStartedResult }
    /// Error wrapper for `remote_chain_head_body`.
    pub enum RemoteChainHeadBodyError { V1 => v01::GenericError }
    /// Request wrapper for `remote_chain_head_storage`.
    pub enum RemoteChainHeadStorageRequest { V1 => v01::ChainHeadStorageRequest }
    /// Response wrapper for `remote_chain_head_storage`.
    pub enum RemoteChainHeadStorageResponse { V1 => v01::OperationStartedResult }
    /// Error wrapper for `remote_chain_head_storage`.
    pub enum RemoteChainHeadStorageError { V1 => v01::GenericError }
    /// Request wrapper for `remote_chain_head_call`.
    pub enum RemoteChainHeadCallRequest { V1 => v01::ChainHeadCallRequest }
    /// Response wrapper for `remote_chain_head_call`.
    pub enum RemoteChainHeadCallResponse { V1 => v01::OperationStartedResult }
    /// Error wrapper for `remote_chain_head_call`.
    pub enum RemoteChainHeadCallError { V1 => v01::GenericError }
    /// Request wrapper for `remote_chain_head_unpin`.
    pub enum RemoteChainHeadUnpinRequest { V1 => v01::ChainHeadUnpinRequest }
    /// Response wrapper for `remote_chain_head_unpin`.
    pub enum RemoteChainHeadUnpinResponse { V1 }
    /// Error wrapper for `remote_chain_head_unpin`.
    pub enum RemoteChainHeadUnpinError { V1 => v01::GenericError }
    /// Request wrapper for `remote_chain_head_continue`.
    pub enum RemoteChainHeadContinueRequest { V1 => v01::ChainHeadOperationRequest }
    /// Response wrapper for `remote_chain_head_continue`.
    pub enum RemoteChainHeadContinueResponse { V1 }
    /// Error wrapper for `remote_chain_head_continue`.
    pub enum RemoteChainHeadContinueError { V1 => v01::GenericError }
    /// Request wrapper for `remote_chain_head_stop_operation`.
    pub enum RemoteChainHeadStopOperationRequest { V1 => v01::ChainHeadOperationRequest }
    /// Response wrapper for `remote_chain_head_stop_operation`.
    pub enum RemoteChainHeadStopOperationResponse { V1 }
    /// Error wrapper for `remote_chain_head_stop_operation`.
    pub enum RemoteChainHeadStopOperationError { V1 => v01::GenericError }
    /// Request wrapper for `remote_chain_spec_genesis_hash`.
    pub enum RemoteChainSpecGenesisHashRequest { V1 => v01::GenesisHash }
    /// Response wrapper for `remote_chain_spec_genesis_hash`.
    pub enum RemoteChainSpecGenesisHashResponse { V1 => v01::Hex }
    /// Error wrapper for `remote_chain_spec_genesis_hash`.
    pub enum RemoteChainSpecGenesisHashError { V1 => v01::GenericError }
    /// Request wrapper for `remote_chain_spec_chain_name`.
    pub enum RemoteChainSpecChainNameRequest { V1 => v01::GenesisHash }
    /// Response wrapper for `remote_chain_spec_chain_name`.
    pub enum RemoteChainSpecChainNameResponse { V1 => String }
    /// Error wrapper for `remote_chain_spec_chain_name`.
    pub enum RemoteChainSpecChainNameError { V1 => v01::GenericError }
    /// Request wrapper for `remote_chain_spec_properties`.
    pub enum RemoteChainSpecPropertiesRequest { V1 => v01::GenesisHash }
    /// Response wrapper for `remote_chain_spec_properties`.
    pub enum RemoteChainSpecPropertiesResponse { V1 => String }
    /// Error wrapper for `remote_chain_spec_properties`.
    pub enum RemoteChainSpecPropertiesError { V1 => v01::GenericError }
    /// Request wrapper for `remote_chain_transaction_broadcast`.
    pub enum RemoteChainTransactionBroadcastRequest { V1 => v01::ChainTransactionBroadcastRequest }
    /// Response wrapper for `remote_chain_transaction_broadcast`.
    pub enum RemoteChainTransactionBroadcastResponse { V1 => Option<String> }
    /// Error wrapper for `remote_chain_transaction_broadcast`.
    pub enum RemoteChainTransactionBroadcastError { V1 => v01::GenericError }
    /// Request wrapper for `remote_chain_transaction_stop`.
    pub enum RemoteChainTransactionStopRequest { V1 => v01::ChainTransactionStopRequest }
    /// Response wrapper for `remote_chain_transaction_stop`.
    pub enum RemoteChainTransactionStopResponse { V1 }
    /// Error wrapper for `remote_chain_transaction_stop`.
    pub enum RemoteChainTransactionStopError { V1 => v01::GenericError }
}
