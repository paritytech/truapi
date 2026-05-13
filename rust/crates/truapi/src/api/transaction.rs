//! Unified [`Transaction`] trait.

use crate::versioned::transaction::{
    HostCreateTransactionError, HostCreateTransactionRequest, HostCreateTransactionResponse,
    HostCreateTransactionWithLegacyAccountError, HostCreateTransactionWithLegacyAccountRequest,
    HostCreateTransactionWithLegacyAccountResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Transaction construction operations.
#[async_trait::async_trait]
pub trait Transaction: Send + Sync {
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
    ///   const result = await truapi.transaction.create({
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
    async fn create(
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
    ///   const result = await truapi.transaction.createWithLegacyAccount({
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
    async fn create_with_legacy_account(
        &self,
        _cx: &CallContext,
        _request: HostCreateTransactionWithLegacyAccountRequest,
    ) -> Result<
        HostCreateTransactionWithLegacyAccountResponse,
        CallError<HostCreateTransactionWithLegacyAccountError>,
    > {
        Err(CallError::unavailable())
    }
}
