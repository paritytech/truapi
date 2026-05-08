//! Unified [`Signing`] trait.

use crate::versioned::signing::{
    HostCreateTransactionError, HostCreateTransactionRequest, HostCreateTransactionResponse,
    HostCreateTransactionWithLegacyAccountError, HostCreateTransactionWithLegacyAccountRequest,
    HostCreateTransactionWithLegacyAccountResponse, HostSignPayloadError, HostSignPayloadRequest,
    HostSignPayloadResponse, HostSignRawError, HostSignRawRequest, HostSignRawResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Signing and transaction construction.
///
/// Default methods return [`CallError::HostFailure`] with an `unavailable`
/// reason. Hosts override only the methods they actually support.
#[async_trait::async_trait]
pub trait Signing: Send + Sync {
    /// Construct a signed extrinsic for a product account.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostCreateTransactionResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function createTransaction(
    ///   truapi: Client,
    /// ): Promise<HostCreateTransactionResponse> {
    ///   const result = await truapi.signing.createTransaction({
    ///     productAccountId: {
    ///       dotNsIdentifier: "truapi-playground.dot",
    ///       derivationIndex: 0,
    ///     },
    ///     payload: {
    ///       tag: "V1",
    ///       value: {
    ///         callData: new Uint8Array(),
    ///         extensions: [],
    ///         txExtVersion: 0,
    ///         context: {
    ///           metadata: new Uint8Array(),
    ///           tokenSymbol: "DOT",
    ///           tokenDecimals: 10,
    ///           bestBlockHeight: 0,
    ///         },
    ///       },
    ///     },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 30)]
    async fn host_create_transaction(
        &self,
        _cx: &CallContext,
        _request: HostCreateTransactionRequest,
    ) -> Result<HostCreateTransactionResponse, CallError<HostCreateTransactionError>> {
        Err(CallError::unavailable())
    }

    /// Construct a signed extrinsic for a non-product account.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostCreateTransactionWithLegacyAccountResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function createTransactionWithLegacyAccount(
    ///   truapi: Client,
    /// ): Promise<HostCreateTransactionWithLegacyAccountResponse> {
    ///   const result = await truapi.signing.createTransactionWithLegacyAccount({
    ///     payload: {
    ///       tag: "V1",
    ///       value: {
    ///         callData: new Uint8Array(),
    ///         extensions: [],
    ///         txExtVersion: 0,
    ///         context: {
    ///           metadata: new Uint8Array(),
    ///           tokenSymbol: "DOT",
    ///           tokenDecimals: 10,
    ///           bestBlockHeight: 0,
    ///         },
    ///       },
    ///     },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 32)]
    async fn host_create_transaction_with_legacy_account(
        &self,
        _cx: &CallContext,
        _request: HostCreateTransactionWithLegacyAccountRequest,
    ) -> Result<
        HostCreateTransactionWithLegacyAccountResponse,
        CallError<HostCreateTransactionWithLegacyAccountError>,
    > {
        Err(CallError::unavailable())
    }

    /// Sign raw bytes or a message.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostSignPayloadResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function signRawBytes(
    ///   truapi: Client,
    /// ): Promise<HostSignPayloadResponse> {
    ///   const result = await truapi.signing.signRaw({
    ///     account: { dotNsIdentifier: "truapi-playground.dot", derivationIndex: 0 },
    ///     payload: { tag: "Bytes", value: { bytes: new Uint8Array() } },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 114)]
    async fn host_sign_raw(
        &self,
        _cx: &CallContext,
        _request: HostSignRawRequest,
    ) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
        Err(CallError::unavailable())
    }

    /// Sign a Substrate extrinsic payload.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostSignPayloadResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function signPayload(
    ///   truapi: Client,
    /// ): Promise<HostSignPayloadResponse> {
    ///   const result = await truapi.signing.signPayload({
    ///     account: { dotNsIdentifier: "truapi-playground.dot", derivationIndex: 0 },
    ///     blockHash: new Uint8Array(),
    ///     blockNumber: new Uint8Array(),
    ///     era: new Uint8Array(),
    ///     genesisHash: new Uint8Array(),
    ///     method: new Uint8Array(),
    ///     nonce: new Uint8Array(),
    ///     signedExtensions: [],
    ///     specVersion: new Uint8Array(),
    ///     tip: new Uint8Array(),
    ///     transactionVersion: new Uint8Array(),
    ///     version: 4,
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 116)]
    async fn host_sign_payload(
        &self,
        _cx: &CallContext,
        _request: HostSignPayloadRequest,
    ) -> Result<HostSignPayloadResponse, CallError<HostSignPayloadError>> {
        Err(CallError::unavailable())
    }
}
