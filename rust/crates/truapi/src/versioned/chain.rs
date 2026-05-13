//! Versioned wrappers for [`ChainInteraction`](crate::api::ChainInteraction) methods.

use crate::v01;

versioned_type! {
    pub enum HostSignPayloadRequest { V1 => v01::HostSignPayloadRequest }
    pub enum HostSignPayloadResponse { V1 => v01::HostSignPayloadResponse }
    pub enum HostSignPayloadError { V1 => v01::HostSignPayloadError }
    pub enum HostSignRawRequest { V1 => v01::HostSignRawRequest }
    pub enum HostSignRawResponse { V1 => v01::HostSignPayloadResponse }
    pub enum HostSignRawError { V1 => v01::HostSignPayloadError }
    pub enum HostSignRawWithLegacyAccountRequest { V1 => v01::HostSignRawWithLegacyAccountRequest }
    pub enum HostSignRawWithLegacyAccountResponse { V1 => v01::HostSignPayloadResponse }
    pub enum HostSignRawWithLegacyAccountError { V1 => v01::HostSignPayloadError }
    pub enum HostSignPayloadWithLegacyAccountRequest { V1 => v01::HostSignPayloadWithLegacyAccountRequest }
    pub enum HostSignPayloadWithLegacyAccountResponse { V1 => v01::HostSignPayloadResponse }
    pub enum HostSignPayloadWithLegacyAccountError { V1 => v01::HostSignPayloadError }
    pub enum HostCreateTransactionRequest { V1 => v01::HostCreateTransactionRequest }
    pub enum HostCreateTransactionResponse { V1 => v01::HostCreateTransactionResponse }
    pub enum HostCreateTransactionError { V1 => v01::HostCreateTransactionError }
    pub enum HostCreateTransactionWithLegacyAccountRequest { V1 => v01::HostCreateTransactionWithLegacyAccountRequest }
    pub enum HostCreateTransactionWithLegacyAccountResponse { V1 => v01::HostCreateTransactionWithLegacyAccountResponse }
    pub enum HostCreateTransactionWithLegacyAccountError { V1 => v01::HostCreateTransactionError }
    pub enum HostJsonrpcMessageSendRequest { V1 => v01::HostJsonrpcMessageSendRequest }
    pub enum HostJsonrpcMessageSendResponse { V1 }
    pub enum HostJsonrpcMessageSendError { V1 => v01::GenericError }
    pub enum HostJsonrpcMessageSubscribeRequest { V1 => v01::HostJsonrpcMessageSubscribeRequest }
    pub enum HostJsonrpcMessageSubscribeItem { V1 => v01::HostJsonrpcMessageSubscribeItem }
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
