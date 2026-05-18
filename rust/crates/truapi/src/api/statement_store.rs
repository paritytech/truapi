//! Unified [`StatementStore`] trait.

use crate::versioned::statement_store::{
    RemoteStatementStoreCreateProofAuthorizedError,
    RemoteStatementStoreCreateProofAuthorizedRequest,
    RemoteStatementStoreCreateProofAuthorizedResponse, RemoteStatementStoreCreateProofError,
    RemoteStatementStoreCreateProofRequest, RemoteStatementStoreCreateProofResponse,
    RemoteStatementStoreSubmitError, RemoteStatementStoreSubmitRequest,
    RemoteStatementStoreSubscribeItem, RemoteStatementStoreSubscribeRequest,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Statement store methods.
pub trait StatementStore: Send + Sync {
    /// Subscribe to statements matching a topic filter.
    ///
    /// ```ts
    /// truapi.statementStore
    ///   .subscribe({ request: { tag: "MatchAll", value: [] } })
    ///   .subscribe({
    ///     next: (statements) => console.log(statements),
    ///     error: (error) => console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
    /// ```
    #[wire(start_id = 56)]
    async fn subscribe(
        &self,
        _cx: &CallContext,
        _request: RemoteStatementStoreSubscribeRequest,
    ) -> Subscription<RemoteStatementStoreSubscribeItem> {
        Subscription::empty()
    }

    /// Create a proof for a statement.
    ///
    /// ```ts
    /// const result = await truapi.statementStore.createProof({
    ///   productAccountId: {
    ///     dotNsIdentifier: "truapi-playground.dot",
    ///     derivationIndex: 0,
    ///   },
    ///   statement: {
    ///     expiry: 9999999999999n,
    ///     topics: [],
    ///   },
    /// });
    /// if (result.isErr()) throw result.error;
    /// console.log(result.value);
    /// ```
    #[wire(request_id = 60)]
    async fn create_proof(
        &self,
        _cx: &CallContext,
        _request: RemoteStatementStoreCreateProofRequest,
    ) -> Result<
        RemoteStatementStoreCreateProofResponse,
        CallError<RemoteStatementStoreCreateProofError>,
    > {
        Err(CallError::unavailable())
    }

    /// Create a proof for a statement using a pre-allocated allowance account,
    /// bypassing the per-call signing prompt.
    ///
    /// ```ts
    /// const result = await truapi.statementStore.createProofAuthorized({
    ///   expiry: 9999999999999n,
    ///   topics: [],
    /// });
    /// if (result.isErr()) throw result.error;
    /// console.log(result.value);
    /// ```
    #[wire(request_id = 132)]
    async fn create_proof_authorized(
        &self,
        _cx: &CallContext,
        _request: RemoteStatementStoreCreateProofAuthorizedRequest,
    ) -> Result<
        RemoteStatementStoreCreateProofAuthorizedResponse,
        CallError<RemoteStatementStoreCreateProofAuthorizedError>,
    > {
        Err(CallError::unavailable())
    }

    /// Submit a signed statement to the network. The request body is the
    /// [`SignedStatement`](crate::v01::SignedStatement) directly (no wrapping
    /// struct), matching upstream `triangle-js-sdks`.
    ///
    /// ```ts
    /// const proofResult = await truapi.statementStore.createProof({
    ///   productAccountId: {
    ///     dotNsIdentifier: "truapi-playground.dot",
    ///     derivationIndex: 0,
    ///   },
    ///   statement: {
    ///     expiry: 9999999999999n,
    ///     topics: [],
    ///   },
    /// });
    /// if (proofResult.isErr()) throw proofResult.error;
    ///
    /// const result = await truapi.statementStore.submit({
    ///   proof: proofResult.value.proof,
    ///   topics: [],
    /// });
    /// if (result.isErr()) throw result.error;
    /// console.log("ok");
    /// ```
    #[wire(request_id = 62)]
    async fn submit(
        &self,
        _cx: &CallContext,
        _request: RemoteStatementStoreSubmitRequest,
    ) -> Result<(), CallError<RemoteStatementStoreSubmitError>> {
        Err(CallError::unavailable())
    }
}
