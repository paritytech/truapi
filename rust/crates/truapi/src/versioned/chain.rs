//! Versioned wrappers for [`ChainInteraction`](crate::api::ChainInteraction) methods.

use crate::v01;

versioned_type! {
    /// Versioned wrapper for [`v01::RemoteChainHeadFollowRequest`] and older versions.
    pub enum RemoteChainHeadFollowRequest { V1 => v01::RemoteChainHeadFollowRequest }
    /// Versioned wrapper for [`v01::RemoteChainHeadFollowItem`] and older versions.
    pub enum RemoteChainHeadFollowItem { V1 => v01::RemoteChainHeadFollowItem }
    /// Versioned wrapper for [`v01::RemoteChainHeadHeaderRequest`] and older versions.
    pub enum RemoteChainHeadHeaderRequest { V1 => v01::RemoteChainHeadHeaderRequest }
    /// Versioned wrapper for [`v01::RemoteChainHeadHeaderResponse`] and older versions.
    pub enum RemoteChainHeadHeaderResponse { V1 => v01::RemoteChainHeadHeaderResponse }
    /// Versioned wrapper for [`v01::GenericError`] and older versions.
    pub enum RemoteChainHeadHeaderError { V1 => v01::GenericError }
    /// Versioned wrapper for [`v01::RemoteChainHeadBodyRequest`] and older versions.
    pub enum RemoteChainHeadBodyRequest { V1 => v01::RemoteChainHeadBodyRequest }
    /// Versioned wrapper for [`v01::RemoteChainHeadBodyResponse`] and older versions.
    pub enum RemoteChainHeadBodyResponse { V1 => v01::RemoteChainHeadBodyResponse }
    /// Versioned wrapper for [`v01::GenericError`] and older versions.
    pub enum RemoteChainHeadBodyError { V1 => v01::GenericError }
    /// Versioned wrapper for [`v01::RemoteChainHeadStorageRequest`] and older versions.
    pub enum RemoteChainHeadStorageRequest { V1 => v01::RemoteChainHeadStorageRequest }
    /// Versioned wrapper for [`v01::RemoteChainHeadStorageResponse`] and older versions.
    pub enum RemoteChainHeadStorageResponse { V1 => v01::RemoteChainHeadStorageResponse }
    /// Versioned wrapper for [`v01::GenericError`] and older versions.
    pub enum RemoteChainHeadStorageError { V1 => v01::GenericError }
    /// Versioned wrapper for [`v01::RemoteChainHeadCallRequest`] and older versions.
    pub enum RemoteChainHeadCallRequest { V1 => v01::RemoteChainHeadCallRequest }
    /// Versioned wrapper for [`v01::RemoteChainHeadCallResponse`] and older versions.
    pub enum RemoteChainHeadCallResponse { V1 => v01::RemoteChainHeadCallResponse }
    /// Versioned wrapper for [`v01::GenericError`] and older versions.
    pub enum RemoteChainHeadCallError { V1 => v01::GenericError }
    /// Versioned wrapper for [`v01::RemoteChainHeadUnpinRequest`] and older versions.
    pub enum RemoteChainHeadUnpinRequest { V1 => v01::RemoteChainHeadUnpinRequest }
    /// Versioned wrapper for unit and older versions.
    pub enum RemoteChainHeadUnpinResponse { V1 }
    /// Versioned wrapper for [`v01::GenericError`] and older versions.
    pub enum RemoteChainHeadUnpinError { V1 => v01::GenericError }
    /// Versioned wrapper for [`v01::RemoteChainHeadContinueRequest`] and older versions.
    pub enum RemoteChainHeadContinueRequest { V1 => v01::RemoteChainHeadContinueRequest }
    /// Versioned wrapper for unit and older versions.
    pub enum RemoteChainHeadContinueResponse { V1 }
    /// Versioned wrapper for [`v01::GenericError`] and older versions.
    pub enum RemoteChainHeadContinueError { V1 => v01::GenericError }
    /// Versioned wrapper for [`v01::RemoteChainHeadStopOperationRequest`] and older versions.
    pub enum RemoteChainHeadStopOperationRequest { V1 => v01::RemoteChainHeadStopOperationRequest }
    /// Versioned wrapper for unit and older versions.
    pub enum RemoteChainHeadStopOperationResponse { V1 }
    /// Versioned wrapper for [`v01::GenericError`] and older versions.
    pub enum RemoteChainHeadStopOperationError { V1 => v01::GenericError }
    /// Versioned wrapper for [`v01::RemoteChainSpecGenesisHashRequest`] and older versions.
    pub enum RemoteChainSpecGenesisHashRequest { V1 => v01::RemoteChainSpecGenesisHashRequest }
    /// Versioned wrapper for [`v01::RemoteChainSpecGenesisHashResponse`] and older versions.
    pub enum RemoteChainSpecGenesisHashResponse { V1 => v01::RemoteChainSpecGenesisHashResponse }
    /// Versioned wrapper for [`v01::GenericError`] and older versions.
    pub enum RemoteChainSpecGenesisHashError { V1 => v01::GenericError }
    /// Versioned wrapper for [`v01::RemoteChainSpecChainNameRequest`] and older versions.
    pub enum RemoteChainSpecChainNameRequest { V1 => v01::RemoteChainSpecChainNameRequest }
    /// Versioned wrapper for [`v01::RemoteChainSpecChainNameResponse`] and older versions.
    pub enum RemoteChainSpecChainNameResponse { V1 => v01::RemoteChainSpecChainNameResponse }
    /// Versioned wrapper for [`v01::GenericError`] and older versions.
    pub enum RemoteChainSpecChainNameError { V1 => v01::GenericError }
    /// Versioned wrapper for [`v01::RemoteChainSpecPropertiesRequest`] and older versions.
    pub enum RemoteChainSpecPropertiesRequest { V1 => v01::RemoteChainSpecPropertiesRequest }
    /// Versioned wrapper for [`v01::RemoteChainSpecPropertiesResponse`] and older versions.
    pub enum RemoteChainSpecPropertiesResponse { V1 => v01::RemoteChainSpecPropertiesResponse }
    /// Versioned wrapper for [`v01::GenericError`] and older versions.
    pub enum RemoteChainSpecPropertiesError { V1 => v01::GenericError }
    /// Versioned wrapper for [`v01::RemoteChainTransactionBroadcastRequest`] and older versions.
    pub enum RemoteChainTransactionBroadcastRequest { V1 => v01::RemoteChainTransactionBroadcastRequest }
    /// Versioned wrapper for [`v01::RemoteChainTransactionBroadcastResponse`] and older versions.
    pub enum RemoteChainTransactionBroadcastResponse { V1 => v01::RemoteChainTransactionBroadcastResponse }
    /// Versioned wrapper for [`v01::GenericError`] and older versions.
    pub enum RemoteChainTransactionBroadcastError { V1 => v01::GenericError }
    /// Versioned wrapper for [`v01::RemoteChainTransactionStopRequest`] and older versions.
    pub enum RemoteChainTransactionStopRequest { V1 => v01::RemoteChainTransactionStopRequest }
    /// Versioned wrapper for unit and older versions.
    pub enum RemoteChainTransactionStopResponse { V1 }
    /// Versioned wrapper for [`v01::GenericError`] and older versions.
    pub enum RemoteChainTransactionStopError { V1 => v01::GenericError }
}
