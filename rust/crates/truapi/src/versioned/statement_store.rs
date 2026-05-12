//! Versioned wrappers for [`StatementStore`](crate::api::StatementStore) methods.

use crate::v01;

versioned_type! {
    /// Versioned wrapper for [`v01::RemoteStatementStoreSubscribeRequest`].
    pub enum RemoteStatementStoreSubscribeRequest { V1 => v01::RemoteStatementStoreSubscribeRequest }
    /// Versioned wrapper for [`v01::RemoteStatementStoreSubscribeItem`].
    pub enum RemoteStatementStoreSubscribeItem { V1 => v01::RemoteStatementStoreSubscribeItem }
    /// Versioned wrapper for [`v01::RemoteStatementStoreCreateProofRequest`].
    pub enum RemoteStatementStoreCreateProofRequest { V1 => v01::RemoteStatementStoreCreateProofRequest }
    /// Versioned wrapper for [`v01::RemoteStatementStoreCreateProofResponse`].
    pub enum RemoteStatementStoreCreateProofResponse { V1 => v01::RemoteStatementStoreCreateProofResponse }
    /// Versioned wrapper for [`v01::RemoteStatementStoreCreateProofError`].
    pub enum RemoteStatementStoreCreateProofError { V1 => v01::RemoteStatementStoreCreateProofError }
    /// Versioned wrapper for the authorized proof request; uses [`v01::Statement`] directly.
    pub enum RemoteStatementStoreCreateProofAuthorizedRequest { V1 => v01::Statement }
    /// Versioned wrapper for the authorized proof response; reuses [`v01::RemoteStatementStoreCreateProofResponse`].
    pub enum RemoteStatementStoreCreateProofAuthorizedResponse { V1 => v01::RemoteStatementStoreCreateProofResponse }
    /// Versioned wrapper for the authorized proof error; reuses [`v01::RemoteStatementStoreCreateProofError`].
    pub enum RemoteStatementStoreCreateProofAuthorizedError { V1 => v01::RemoteStatementStoreCreateProofError }
    /// Versioned wrapper for [`v01::SignedStatement`]. The submit request is the
    /// signed statement itself; the host SCALE-decodes it directly without a
    /// wrapping field.
    pub enum RemoteStatementStoreSubmitRequest { V1 => v01::SignedStatement }
    /// Versioned wrapper for [`v01::GenericError`]. Submit has no success payload.
    pub enum RemoteStatementStoreSubmitError { V1 => v01::GenericError }
}
