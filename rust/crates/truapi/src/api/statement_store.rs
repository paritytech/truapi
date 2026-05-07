//! Unified [`StatementStore`] trait.

use crate::versioned::statement_store::{
    RemoteStatementStoreCreateProofError, RemoteStatementStoreCreateProofRequest,
    RemoteStatementStoreCreateProofResponse, RemoteStatementStoreSubmitError,
    RemoteStatementStoreSubmitRequest, RemoteStatementStoreSubscribeItem,
    RemoteStatementStoreSubscribeRequest,
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
    ///
    /// ```truapi-playground-request
    /// { "tag": "MatchAll", "value": [] }
    /// ```
    #[wire(id = 56)]
    async fn remote_statement_store_subscribe(
        &self,
        _cx: &CallContext,
        _request: RemoteStatementStoreSubscribeRequest,
    ) -> Subscription<RemoteStatementStoreSubscribeItem> {
        Subscription::empty()
    }

    /// Create a proof for a statement.
    ///
    /// ```truapi-playground-request
    /// { "productAccountId": { "dotNsIdentifier": "truapi-playground.dot", "derivationIndex": 0 }, "statement": { "proof": null, "decryptionKey": null, "expiry": "9999999999999n", "channel": null, "topics": [], "data": null } }
    /// ```
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

    /// Submit a signed statement to the network. The request body is the
    /// [`SignedStatement`](crate::v01::SignedStatement) directly (no wrapping
    /// struct), matching upstream `triangle-js-sdks`.
    ///
    /// ```truapi-playground-request
    /// { "proof": { "tag": "Sr25519", "value": { "signature": "0x0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000", "signer": "0x0000000000000000000000000000000000000000000000000000000000000000" } }, "decryptionKey": null, "expiry": null, "channel": null, "topics": [], "data": null }
    /// ```
    #[wire(id = 62)]
    async fn remote_statement_store_submit(
        &self,
        _cx: &CallContext,
        _request: RemoteStatementStoreSubmitRequest,
    ) -> Result<(), CallError<RemoteStatementStoreSubmitError>> {
        Err(CallError::unavailable())
    }
}
