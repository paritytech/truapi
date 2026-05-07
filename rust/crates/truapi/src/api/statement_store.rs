//! Unified [`StatementStore`] trait.

use crate::versioned::statement_store::{
    RemoteStatementStoreCreateProofError, RemoteStatementStoreCreateProofRequest,
    RemoteStatementStoreCreateProofResponse, RemoteStatementStoreSubmitError,
    RemoteStatementStoreSubmitRequest, RemoteStatementStoreSubmitResponse,
    RemoteStatementStoreSubscribeItem, RemoteStatementStoreSubscribeRequest,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Statement store operations.
///
/// Default request methods return [`CallError::HostFailure`] with an
/// `unavailable` reason. Hosts override only the methods they actually support.
#[async_trait::async_trait]
pub trait StatementStore: Send + Sync {
    /// Subscribe to statements matching a topic filter.
    #[wire(id = 56)]
    async fn remote_statement_store_subscribe(
        &self,
        _cx: &CallContext,
        _request: RemoteStatementStoreSubscribeRequest,
    ) -> Subscription<RemoteStatementStoreSubscribeItem> {
        Subscription::empty()
    }

    /// Create a proof for a statement.
    #[wire(id = 60)]
    async fn remote_statement_store_create_proof(
        &self,
        _cx: &CallContext,
        _request: RemoteStatementStoreCreateProofRequest,
    ) -> Result<
        RemoteStatementStoreCreateProofResponse,
        CallError<RemoteStatementStoreCreateProofError>,
    > {
        Err(CallError::unavailable())
    }

    /// Submit an encoded signed statement to the network.
    #[wire(id = 62)]
    async fn remote_statement_store_submit(
        &self,
        _cx: &CallContext,
        _request: RemoteStatementStoreSubmitRequest,
    ) -> Result<RemoteStatementStoreSubmitResponse, CallError<RemoteStatementStoreSubmitError>>
    {
        Err(CallError::unavailable())
    }
}
