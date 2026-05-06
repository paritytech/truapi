//! Versioned wrappers for [`StatementStore`](crate::api::StatementStore) methods.

use crate::{v01, v02};

versioned_type! {
    /// Subscription request wrapper for `remote_statement_store_subscribe`.
    ///
    /// V0.1 took a plain `Vec<Topic>`; V0.2 promoted it to a [`TopicFilter`](v02::TopicFilter)
    /// with positional wildcard support. Upgrading V1 to V2 wraps the topics as
    /// exact-match positions; downgrading V2 to V1 succeeds only when the filter
    /// has no wildcards.
    pub enum RemoteStatementStoreSubscribeRequest { V1 => Vec<v01::Topic>, V2 => v02::TopicFilter }
    /// Subscription item wrapper for `remote_statement_store_subscribe`.
    pub enum RemoteStatementStoreSubscribeItem { V1 => Vec<v01::SignedStatement>, V2 => Vec<v01::SignedStatement> }
    /// Request wrapper for `remote_statement_store_create_proof`.
    pub enum RemoteStatementStoreCreateProofRequest { V1 => v01::StatementStoreCreateProofRequest }
    /// Response wrapper for `remote_statement_store_create_proof`.
    pub enum RemoteStatementStoreCreateProofResponse { V1 => v01::StatementProof }
    /// Error wrapper for `remote_statement_store_create_proof`.
    pub enum RemoteStatementStoreCreateProofError { V1 => v01::StatementProofError }
    /// Request wrapper for `remote_statement_store_submit`.
    pub enum RemoteStatementStoreSubmitRequest { V1 => v01::Bytes }
    /// Response wrapper for `remote_statement_store_submit`.
    pub enum RemoteStatementStoreSubmitResponse { V1 => String }
    /// Error wrapper for `remote_statement_store_submit`.
    pub enum RemoteStatementStoreSubmitError { V1 => v01::GenericError }
}
