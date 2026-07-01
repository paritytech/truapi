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
    /// import { firstValueFrom, from } from "rxjs";
    /// import type { Statement } from "@parity/truapi";
    ///
    /// const bytes = crypto.getRandomValues(new Uint8Array(32));
    /// const topic: `0x${string}` = `0x${bytes.toHex()}`;
    /// const expiry = BigInt(Math.floor(Date.now() / 1000) + 86400) << 32n;
    /// const statement: Statement = { expiry, topics: [topic] };
    ///
    /// const proofResult = await truapi.statementStore.createProof({
    ///   productAccountId: { derivationIndex: 0 },
    ///   statement,
    /// });
    /// assert(proofResult.isOk(), "createProof failed:", proofResult);
    ///
    /// statement.proof = proofResult.value.proof;
    /// console.log("submitting statement:", statement);
    /// const submitted = await truapi.statementStore.submit({
    ///   ...statement,
    ///   proof: proofResult.value.proof,
    /// });
    /// assert(submitted.isOk(), "failed to submit statement:", submitted);
    /// console.log("statement submitted");
    ///
    /// const statements = await firstValueFrom(
    ///   from(
    ///     truapi.statementStore.subscribe({
    ///       request: { tag: "MatchAll", value: [topic] },
    ///     }),
    ///   ),
    /// );
    /// console.log("subscribe received", statements);
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
    /// const statement = { expiry, topics: [topic] };
    /// const result = await truapi.statementStore.createProof({
    ///   productAccountId: { derivationIndex: 0 },
    ///   statement,
    /// });
    /// assert(result.isOk(), "createProof failed:", result);
    /// console.log("proof created:", result.value);
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
    /// const statement = { expiry, topics: [topic] };
    ///
    /// const result = await truapi.statementStore.createProofAuthorized(statement);
    /// assert(result.isOk(), "createProof failed:", result);
    /// console.log("proof created:", result.value);
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
    /// const expiry = BigInt(Math.floor(Date.now() / 1000) + 86400) << 32n;
    /// const statement = { expiry, topics: [topic] };
    ///
    /// const proofResult = await truapi.statementStore.createProof({
    ///   productAccountId: { derivationIndex: 0 },
    ///   statement,
    /// });
    /// assert(proofResult.isOk(), "createProof failed:", proofResult);
    ///
    /// const result = await truapi.statementStore.submit({
    ///   proof: proofResult.value.proof,
    ///   ...statement,
    /// });
    /// assert(result.isOk(), "submit failed:", result);
    /// console.log("statement submitted");
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
