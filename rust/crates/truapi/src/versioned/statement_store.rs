//! Versioned wrappers for [`StatementStore`](crate::api::StatementStore) methods.

use crate::{v01, v02};

versioned_type! {
    /// Versioned wrapper for [`v02::RemoteStatementStoreSubscribeRequest`] and older versions.
    pub enum RemoteStatementStoreSubscribeRequest { V1 => v01::RemoteStatementStoreSubscribeRequest, V2 => v02::RemoteStatementStoreSubscribeRequest }
    /// Versioned wrapper for [`v02::RemoteStatementStoreSubscribeItem`] and older versions.
    pub enum RemoteStatementStoreSubscribeItem { V1 => v01::RemoteStatementStoreSubscribeItem, V2 => v02::RemoteStatementStoreSubscribeItem }
    /// Versioned wrapper for [`v01::RemoteStatementStoreCreateProofRequest`] and older versions.
    pub enum RemoteStatementStoreCreateProofRequest { V1 => v01::RemoteStatementStoreCreateProofRequest }
    /// Versioned wrapper for [`v01::RemoteStatementStoreCreateProofResponse`] and older versions.
    pub enum RemoteStatementStoreCreateProofResponse { V1 => v01::RemoteStatementStoreCreateProofResponse }
    /// Versioned wrapper for [`v01::RemoteStatementStoreCreateProofError`] and older versions.
    pub enum RemoteStatementStoreCreateProofError { V1 => v01::RemoteStatementStoreCreateProofError }
    /// Versioned wrapper for [`v01::RemoteStatementStoreSubmitRequest`] and older versions.
    pub enum RemoteStatementStoreSubmitRequest { V1 => v01::RemoteStatementStoreSubmitRequest }
    /// Versioned wrapper for [`v01::RemoteStatementStoreSubmitResponse`] and older versions.
    pub enum RemoteStatementStoreSubmitResponse { V1 => v01::RemoteStatementStoreSubmitResponse }
    /// Versioned wrapper for [`v01::GenericError`] and older versions.
    pub enum RemoteStatementStoreSubmitError { V1 => v01::GenericError }
}
