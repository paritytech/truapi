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
    /// Versioned wrapper for the authorized proof request; uses [`v01::Statement`] directly.
    pub enum RemoteStatementStoreCreateProofAuthorizedRequest { V1 => v01::Statement }
    /// Versioned wrapper for the authorized proof response; reuses [`v01::RemoteStatementStoreCreateProofResponse`].
    pub enum RemoteStatementStoreCreateProofAuthorizedResponse { V1 => v01::RemoteStatementStoreCreateProofResponse }
    /// Versioned wrapper for the authorized proof error; reuses [`v01::RemoteStatementStoreCreateProofError`].
    pub enum RemoteStatementStoreCreateProofAuthorizedError { V1 => v01::RemoteStatementStoreCreateProofError }
    /// Versioned wrapper for [`v01::SignedStatement`] and older versions.
    /// The submit request is the signed statement itself; the host SCALE-decodes
    /// it directly without a wrapping field, matching the upstream
    /// `triangle-js-sdks` `StatementStoreSubmitV1_request = SignedStatement`.
    pub enum RemoteStatementStoreSubmitRequest { V1 => v01::SignedStatement }
    /// Versioned wrapper for [`v01::GenericError`] and older versions. Submit
    /// has no success payload (`Result<(), GenericError>`), matching upstream.
    pub enum RemoteStatementStoreSubmitError { V1 => v01::GenericError }
}
