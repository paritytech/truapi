//! Versioned wrappers for [`StatementStore`](crate::api::StatementStore) methods.

use crate::v01;

versioned_type! {
    pub enum RemoteStatementStoreSubscribeRequest { V1 => v01::RemoteStatementStoreSubscribeRequest }
    pub enum RemoteStatementStoreSubscribeItem { V1 => v01::RemoteStatementStoreSubscribeItem }
    pub enum RemoteStatementStoreCreateProofRequest { V1 => v01::RemoteStatementStoreCreateProofRequest }
    pub enum RemoteStatementStoreCreateProofResponse { V1 => v01::RemoteStatementStoreCreateProofResponse }
    pub enum RemoteStatementStoreCreateProofError { V1 => v01::RemoteStatementStoreCreateProofError }
    pub enum RemoteStatementStoreCreateProofAuthorizedRequest { V1 => v01::Statement }
    pub enum RemoteStatementStoreCreateProofAuthorizedResponse { V1 => v01::RemoteStatementStoreCreateProofResponse }
    pub enum RemoteStatementStoreCreateProofAuthorizedError { V1 => v01::RemoteStatementStoreCreateProofError }
    pub enum RemoteStatementStoreSubmitRequest { V1 => v01::SignedStatement }
    pub enum RemoteStatementStoreSubmitError { V1 => v01::GenericError }
}
