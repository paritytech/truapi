//! Unified [`StatementStore`] trait.

use crate::v02::{GenericError, StatementProof, StatementProofError};
use crate::versioned::statement_store::{
    RemoteStatementStoreCreateProofRequest, RemoteStatementStoreCreateProofResponse,
    RemoteStatementStoreSubmitRequest, RemoteStatementStoreSubmitResponse,
    RemoteStatementStoreSubscribeItem, RemoteStatementStoreSubscribeRequest,
};
use crate::wire;
use crate::{CallContext, Subscription};

/// Statement store operations. Unified counterpart of
/// [`crate::v02::StatementStore`].
///
/// Every method has a default body that flags the call as unavailable through
/// [`CallContext::fail_unavailable`] and returns a placeholder value. Hosts
/// override only the methods they actually support.
#[async_trait::async_trait]
pub trait StatementStore: Send + Sync {
    /// Subscribe to statements matching a topic filter.
    #[wire(id = 56)]
    async fn remote_statement_store_subscribe(
        &self,
        cx: &CallContext,
        _request: RemoteStatementStoreSubscribeRequest,
    ) -> Subscription<RemoteStatementStoreSubscribeItem> {
        cx.fail_unavailable();
        Subscription::empty()
    }

    /// Create a proof for a statement.
    #[wire(id = 60)]
    async fn remote_statement_store_create_proof(
        &self,
        cx: &CallContext,
        _request: RemoteStatementStoreCreateProofRequest,
    ) -> Result<RemoteStatementStoreCreateProofResponse, StatementProofError> {
        cx.fail_unavailable();
        Ok(RemoteStatementStoreCreateProofResponse::V2(
            StatementProof::Sr25519 {
                signature: [0u8; 64],
                signer: [0u8; 32],
            },
        ))
    }

    /// Submit an encoded signed statement to the network.
    #[wire(id = 62)]
    async fn remote_statement_store_submit(
        &self,
        cx: &CallContext,
        _request: RemoteStatementStoreSubmitRequest,
    ) -> Result<RemoteStatementStoreSubmitResponse, GenericError> {
        cx.fail_unavailable();
        Ok(RemoteStatementStoreSubmitResponse::V2(String::new()))
    }
}
