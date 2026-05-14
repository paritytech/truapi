//! Versioned wrappers for [`Chain`](crate::api::Chain) methods.

use crate::v01;

versioned_type! {
    pub enum RemoteChainHeadFollowRequest { V1 => v01::RemoteChainHeadFollowRequest }
    pub enum RemoteChainHeadFollowItem { V1 => v01::RemoteChainHeadFollowItem }
    pub enum RemoteChainHeadHeaderRequest { V1 => v01::RemoteChainHeadHeaderRequest }
    pub enum RemoteChainHeadHeaderResponse { V1 => v01::RemoteChainHeadHeaderResponse }
    pub enum RemoteChainHeadHeaderError { V1 => v01::GenericError }
    pub enum RemoteChainHeadBodyRequest { V1 => v01::RemoteChainHeadBodyRequest }
    pub enum RemoteChainHeadBodyResponse { V1 => v01::RemoteChainHeadBodyResponse }
    pub enum RemoteChainHeadBodyError { V1 => v01::GenericError }
    pub enum RemoteChainHeadStorageRequest { V1 => v01::RemoteChainHeadStorageRequest }
    pub enum RemoteChainHeadStorageResponse { V1 => v01::RemoteChainHeadStorageResponse }
    pub enum RemoteChainHeadStorageError { V1 => v01::GenericError }
    pub enum RemoteChainHeadCallRequest { V1 => v01::RemoteChainHeadCallRequest }
    pub enum RemoteChainHeadCallResponse { V1 => v01::RemoteChainHeadCallResponse }
    pub enum RemoteChainHeadCallError { V1 => v01::GenericError }
    pub enum RemoteChainHeadUnpinRequest { V1 => v01::RemoteChainHeadUnpinRequest }
    pub enum RemoteChainHeadUnpinResponse { V1 }
    pub enum RemoteChainHeadUnpinError { V1 => v01::GenericError }
    pub enum RemoteChainHeadContinueRequest { V1 => v01::RemoteChainHeadContinueRequest }
    pub enum RemoteChainHeadContinueResponse { V1 }
    pub enum RemoteChainHeadContinueError { V1 => v01::GenericError }
    pub enum RemoteChainHeadStopOperationRequest { V1 => v01::RemoteChainHeadStopOperationRequest }
    pub enum RemoteChainHeadStopOperationResponse { V1 }
    pub enum RemoteChainHeadStopOperationError { V1 => v01::GenericError }
    pub enum RemoteChainSpecGenesisHashRequest { V1 => v01::RemoteChainSpecGenesisHashRequest }
    pub enum RemoteChainSpecGenesisHashResponse { V1 => v01::RemoteChainSpecGenesisHashResponse }
    pub enum RemoteChainSpecGenesisHashError { V1 => v01::GenericError }
    pub enum RemoteChainSpecChainNameRequest { V1 => v01::RemoteChainSpecChainNameRequest }
    pub enum RemoteChainSpecChainNameResponse { V1 => v01::RemoteChainSpecChainNameResponse }
    pub enum RemoteChainSpecChainNameError { V1 => v01::GenericError }
    pub enum RemoteChainSpecPropertiesRequest { V1 => v01::RemoteChainSpecPropertiesRequest }
    pub enum RemoteChainSpecPropertiesResponse { V1 => v01::RemoteChainSpecPropertiesResponse }
    pub enum RemoteChainSpecPropertiesError { V1 => v01::GenericError }
    pub enum RemoteChainTransactionBroadcastRequest { V1 => v01::RemoteChainTransactionBroadcastRequest }
    pub enum RemoteChainTransactionBroadcastResponse { V1 => v01::RemoteChainTransactionBroadcastResponse }
    pub enum RemoteChainTransactionBroadcastError { V1 => v01::GenericError }
    pub enum RemoteChainTransactionStopRequest { V1 => v01::RemoteChainTransactionStopRequest }
    pub enum RemoteChainTransactionStopResponse { V1 }
    pub enum RemoteChainTransactionStopError { V1 => v01::GenericError }
}
