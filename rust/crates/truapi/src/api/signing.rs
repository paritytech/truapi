//! Unified [`Signing`] trait.

use crate::versioned::signing::{
    HostSignPayloadError, HostSignPayloadRequest, HostSignPayloadResponse,
    HostSignPayloadWithLegacyAccountError, HostSignPayloadWithLegacyAccountRequest,
    HostSignPayloadWithLegacyAccountResponse, HostSignRawError, HostSignRawRequest,
    HostSignRawResponse, HostSignRawWithLegacyAccountError, HostSignRawWithLegacyAccountRequest,
    HostSignRawWithLegacyAccountResponse,
};
use crate::versioned::signing::{
    HostCreateTransactionError, HostCreateTransactionRequest, HostCreateTransactionResponse,
    HostCreateTransactionWithLegacyAccountError, HostCreateTransactionWithLegacyAccountRequest,
    HostCreateTransactionWithLegacyAccountResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Signing operations.
#[async_trait::async_trait]
pub trait Signing: Send + Sync {
    /// Construct a signed transaction for a product account.
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
    ///         callData: "0x0000",
    ///         extensions: [],
    ///         txExtVersion: 0,
    ///         context: {
    ///           metadata: "0x",
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
    async fn create_transaction(
        &self,
        _cx: &CallContext,
        _request: HostCreateTransactionRequest,
    ) -> Result<HostCreateTransactionResponse, CallError<HostCreateTransactionError>> {
        Err(CallError::unavailable())
    }

    /// Construct a signed transaction for a non-product account.
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
    ///         callData: "0x0000",
    ///         extensions: [],
    ///         txExtVersion: 0,
    ///         context: {
    ///           metadata: "0x",
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
    async fn create_transaction_with_legacy_account(
        &self,
        _cx: &CallContext,
        _request: HostCreateTransactionWithLegacyAccountRequest,
    ) -> Result<
        HostCreateTransactionWithLegacyAccountResponse,
        CallError<HostCreateTransactionWithLegacyAccountError>,
    > {
        Err(CallError::unavailable())
    }

    /// Sign raw bytes with a non-product account.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostSignPayloadResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function signRawWithLegacyAccount(
    ///   truapi: Client,
    /// ): Promise<HostSignPayloadResponse> {
    ///   const result = await truapi.signing.signRawWithLegacyAccount({
    ///     signer: "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    ///     payload: {
    ///       tag: "Bytes",
    ///       value: { bytes: "0x48656c6c6f" },
    ///     },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 34)]
    async fn sign_raw_with_legacy_account(
        &self,
        _cx: &CallContext,
        _request: HostSignRawWithLegacyAccountRequest,
    ) -> Result<HostSignRawWithLegacyAccountResponse, CallError<HostSignRawWithLegacyAccountError>>
    {
        Err(CallError::unavailable())
    }

    /// Sign an extrinsic payload with a non-product account.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostSignPayloadResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function signPayloadWithLegacyAccount(
    ///   truapi: Client,
    /// ): Promise<HostSignPayloadResponse> {
    ///   const result = await truapi.signing.signPayloadWithLegacyAccount({
    ///     signer: "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    ///     payload: {
    ///       account: { dotNsIdentifier: "truapi-playground.dot", derivationIndex: 0 },
    ///       blockHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///       blockNumber: "0x00000000",
    ///       era: "0x00",
    ///       genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///       method: "0x0000",
    ///       nonce: "0x00000000",
    ///       signedExtensions: [],
    ///       specVersion: "0x00000000",
    ///       tip: "0x00000000000000000000000000000000",
    ///       transactionVersion: "0x00000000",
    ///       version: 4,
    ///     },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 36)]
    async fn sign_payload_with_legacy_account(
        &self,
        _cx: &CallContext,
        _request: HostSignPayloadWithLegacyAccountRequest,
    ) -> Result<
        HostSignPayloadWithLegacyAccountResponse,
        CallError<HostSignPayloadWithLegacyAccountError>,
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
    ///     payload: {
    ///       tag: "Bytes",
    ///       value: {
    ///         bytes: "0x48656c6c6f2c20776f726c6421",
    ///       },
    ///     },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 114)]
    async fn sign_raw(
        &self,
        _cx: &CallContext,
        _request: HostSignRawRequest,
    ) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
        Err(CallError::unavailable())
    }

    /// Sign an extrinsic payload.
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
    ///     blockHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     blockNumber: "0x00000000",
    ///     era: "0x00",
    ///     genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     method: "0x00003448656c6c6f2c20776f726c6421",
    ///     nonce: "0x00000000",
    ///     signedExtensions: [],
    ///     specVersion: "0x00000000",
    ///     tip: "0x00000000000000000000000000000000",
    ///     transactionVersion: "0x00000000",
    ///     version: 4,
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 116)]
    async fn sign_payload(
        &self,
        _cx: &CallContext,
        _request: HostSignPayloadRequest,
    ) -> Result<HostSignPayloadResponse, CallError<HostSignPayloadError>> {
        Err(CallError::unavailable())
    }
}
