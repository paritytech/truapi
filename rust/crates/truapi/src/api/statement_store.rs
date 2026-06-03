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
    /// // Submit a statement under a fresh random topic, then match on it.
    /// const bytes = crypto.getRandomValues(new Uint8Array(32));
    /// const topic: `0x${string}` = `0x${bytes.toHex()}`;
    ///
    /// const proofResult = await truapi.statementStore.createProofAuthorized({
    ///   topics: [topic],
    /// });
    ///
    /// if (proofResult.isErr()) {
    ///   console.error("createProof failed:", proofResult.error);
    /// } else {
    ///   console.log("submitting proof:\n", proofResult.value.proof);
    ///   const submitted = await truapi.statementStore.submit({
    ///     proof: proofResult.value.proof,
    ///     topics: [topic],
    ///   });
    ///   submitted.match(
    ///     (value) => console.log("proof submitted:", value),
    ///     (error) => console.error("failed to submit proof:", error),
    ///   );
    ///   from(
    ///     truapi.statementStore.subscribe({
    ///       request: { tag: "MatchAll", value: [topic] },
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
    /// **Deprecated:** use [`create_proof_authorized`](Self::create_proof_authorized)
    /// instead, which uses a pre-allocated allowance account and does not
    /// require a per-call signing prompt.
    ///
    /// ```ts
    /// // Expiry packs a Unix-seconds timestamp in the high 32 bits; a day out
    /// // keeps the statement unexpired when it is submitted.
    /// const expiry = BigInt(Math.floor(Date.now() / 1000) + 86400) << 32n;
    /// const bytes = crypto.getRandomValues(new Uint8Array(32));
    /// const topic: `0x${string}` = `0x${bytes.toHex()}`;
    /// const result = await truapi.statementStore.createProof({
    ///   productAccountId: {
    ///     dotNsIdentifier: "truapi-playground.dot",
    ///     derivationIndex: 0,
    ///   },
    ///   statement: {
    ///     expiry,
    ///     topics: [topic],
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
    /// // Expiry packs a Unix-seconds timestamp in the high 32 bits; a day out
    /// // keeps the statement unexpired when it is submitted.
    /// const expiry = BigInt(Math.floor(Date.now() / 1000) + 86400) << 32n;
    /// const bytes = crypto.getRandomValues(new Uint8Array(32));
    /// const topic: `0x${string}` = `0x${bytes.toHex()}`;
    /// const result = await truapi.statementStore.createProofAuthorized({
    ///   expiry,
    ///   topics: [topic],
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
    /// const bytes = crypto.getRandomValues(new Uint8Array(32));
    /// const topic: `0x${string}` = `0x${bytes.toHex()}`;
    /// const proofResult = await truapi.statementStore.createProofAuthorized({
    ///   topics: [topic],
    /// });
    ///
    /// if (proofResult.isErr()) {
    ///   console.error("createProof failed:", proofResult.error);
    /// } else {
    ///   const result = await truapi.statementStore.submit({
    ///     proof: proofResult.value.proof,
    ///     topics: [topic],
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
