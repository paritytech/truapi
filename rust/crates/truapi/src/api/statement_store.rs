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
    /// import { from, take } from "rxjs";
    ///
    /// // Create and submit a statement first so the subscription has a match.
    /// const proofResult = await truapi.statementStore.createProof({
    ///   productAccountId: {
    ///     dotNsIdentifier: "truapi-playground.dot",
    ///     derivationIndex: 0,
    ///   },
    ///   statement: { topics: [] },
    /// });
    ///
    /// if (proofResult.isErr()) {
    ///   console.error("createProof failed:", proofResult.error);
    /// } else {
    ///   await truapi.statementStore.submit({
    ///     proof: proofResult.value.proof,
    ///     topics: [],
    ///   });
    ///   from(
    ///     truapi.statementStore.subscribe({
    ///       request: { tag: "MatchAll", value: [] },
    ///     }),
    ///   )
    ///     .pipe(take(1))
    ///     .subscribe({
    ///       next: (statements) => console.log(statements),
    ///       error: (error) => console.error("subscribe failed:", error),
    ///       complete: () => console.log("completed"),
    ///     });
    /// }
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
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
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
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
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
    /// if (proofResult.isErr()) {
    ///   console.error("createProof failed:", proofResult.error);
    /// } else {
    ///   const result = await truapi.statementStore.submit({
    ///     proof: proofResult.value.proof,
    ///     topics: [],
    ///   });
    ///   result.match(
    ///     () => console.log("ok"),
    ///     (error) => console.error("submit failed:", error),
    ///   );
    /// }
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
