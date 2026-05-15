//! Unified [`CoinPayment`] trait (RFC 0017).

use crate::versioned::coin_payment::{
    HostCoinPaymentCreateChequeError, HostCoinPaymentCreateChequeRequest,
    HostCoinPaymentCreateChequeResponse, HostCoinPaymentCreatePurseError,
    HostCoinPaymentCreatePurseRequest, HostCoinPaymentCreatePurseResponse,
    HostCoinPaymentCreateReceivableError, HostCoinPaymentCreateReceivableRequest,
    HostCoinPaymentCreateReceivableResponse, HostCoinPaymentDeletePurseError,
    HostCoinPaymentDeletePurseItem, HostCoinPaymentDeletePurseRequest, HostCoinPaymentDepositError,
    HostCoinPaymentDepositItem, HostCoinPaymentDepositRequest, HostCoinPaymentListenForError,
    HostCoinPaymentListenForItem, HostCoinPaymentListenForRequest, HostCoinPaymentQueryPurseError,
    HostCoinPaymentQueryPurseRequest, HostCoinPaymentQueryPurseResponse,
    HostCoinPaymentRebalancePurseError, HostCoinPaymentRebalancePurseItem,
    HostCoinPaymentRebalancePurseRequest, HostCoinPaymentRefundError, HostCoinPaymentRefundItem,
    HostCoinPaymentRefundRequest,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// CoinPayment operations.
///
/// RFC 0017 describes `Resolvable<T>` values for long-running operations.
/// TrUAPI represents those as subscriptions whose items are the RFC status
/// updates.
pub trait CoinPayment: Send + Sync {
    /// Create a new firewalled CoinPayment purse.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function createPurse(truapi: Client): Promise<number> {
    ///   const result = await truapi.coinPayment.coinPaymentCreatePurse({
    ///     name: "Terminal purse",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value.purse;
    /// }
    /// ```
    #[wire(request_id = 134)]
    async fn host_coin_payment_create_purse(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentCreatePurseRequest,
    ) -> Result<HostCoinPaymentCreatePurseResponse, CallError<HostCoinPaymentCreatePurseError>>
    {
        Err(CallError::unavailable())
    }

    /// Query product-visible purse metadata and balance.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostCoinPaymentQueryPurseResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function queryPurse(
    ///   truapi: Client,
    ///   purse: number,
    /// ): Promise<HostCoinPaymentQueryPurseResponse> {
    ///   const result = await truapi.coinPayment.coinPaymentQueryPurse({
    ///     purse,
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 136)]
    async fn host_coin_payment_query_purse(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentQueryPurseRequest,
    ) -> Result<HostCoinPaymentQueryPurseResponse, CallError<HostCoinPaymentQueryPurseError>> {
        Err(CallError::unavailable())
    }

    /// Transfer balance between local purses.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostCoinPaymentRebalancePurseError,
    ///   type CoinPaymentStatus,
    ///   type Subscription,
    ///   type SubscriptionError,
    /// } from "@parity/truapi";
    ///
    /// export function rebalancePurse(truapi: Client): Subscription {
    ///   return truapi.coinPayment
    ///     .coinPaymentRebalancePurse({
    ///       request: { from: 1, to: 2, amount: 1000 },
    ///     })
    ///     .subscribe({
    ///       next: (status: CoinPaymentStatus) => console.log(status),
    ///       error: (error: SubscriptionError<HostCoinPaymentRebalancePurseError>) =>
    ///         console.error(error),
    ///     });
    /// }
    /// ```
    #[wire(start_id = 138)]
    async fn host_coin_payment_rebalance_purse(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentRebalancePurseRequest,
    ) -> Result<
        Subscription<HostCoinPaymentRebalancePurseItem>,
        CallError<HostCoinPaymentRebalancePurseError>,
    > {
        Err(CallError::unavailable())
    }

    /// Delete a purse after draining its balance into another local purse.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostCoinPaymentDeletePurseError,
    ///   type CoinPaymentStatus,
    ///   type Subscription,
    ///   type SubscriptionError,
    /// } from "@parity/truapi";
    ///
    /// export function deletePurse(truapi: Client): Subscription {
    ///   return truapi.coinPayment
    ///     .coinPaymentDeletePurse({
    ///       request: { target: 2, drainInto: 1 },
    ///     })
    ///     .subscribe({
    ///       next: (status: CoinPaymentStatus) => console.log(status),
    ///       error: (error: SubscriptionError<HostCoinPaymentDeletePurseError>) =>
    ///         console.error(error),
    ///     });
    /// }
    /// ```
    #[wire(start_id = 142)]
    async fn host_coin_payment_delete_purse(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentDeletePurseRequest,
    ) -> Result<
        Subscription<HostCoinPaymentDeletePurseItem>,
        CallError<HostCoinPaymentDeletePurseError>,
    > {
        Err(CallError::unavailable())
    }

    /// Create a receivable public key for depositing into a purse.
    ///
    /// ```truapi-client-example
    /// import { type Client, type CoinPaymentReceivable } from "@parity/truapi";
    ///
    /// export async function createReceivable(
    ///   truapi: Client,
    ///   purse: number,
    /// ): Promise<CoinPaymentReceivable> {
    ///   const result = await truapi.coinPayment.coinPaymentCreateReceivable({
    ///     into: purse,
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value.receivable;
    /// }
    /// ```
    #[wire(request_id = 146)]
    async fn host_coin_payment_create_receivable(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentCreateReceivableRequest,
    ) -> Result<
        HostCoinPaymentCreateReceivableResponse,
        CallError<HostCoinPaymentCreateReceivableError>,
    > {
        Err(CallError::unavailable())
    }

    /// Create a cheque paying from a local purse to a receivable.
    ///
    /// ```truapi-client-example
    /// import { type CoinPaymentCheque, type Client, type CoinPaymentReceivable } from "@parity/truapi";
    ///
    /// export async function createCheque(
    ///   truapi: Client,
    ///   receiver: CoinPaymentReceivable,
    /// ): Promise<CoinPaymentCheque> {
    ///   const result = await truapi.coinPayment.coinPaymentCreateCheque({
    ///     from: 1,
    ///     to: receiver,
    ///     amount: 1000,
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value.cheque;
    /// }
    /// ```
    #[wire(request_id = 148)]
    async fn host_coin_payment_create_cheque(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentCreateChequeRequest,
    ) -> Result<HostCoinPaymentCreateChequeResponse, CallError<HostCoinPaymentCreateChequeError>>
    {
        Err(CallError::unavailable())
    }

    /// Claim coins from a cheque into the receivable's purse.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type CoinPaymentCheque,
    ///   type Client,
    ///   type HostCoinPaymentDepositError,
    ///   type CoinPaymentStatus,
    ///   type Subscription,
    ///   type SubscriptionError,
    /// } from "@parity/truapi";
    ///
    /// export function depositCheque(truapi: Client, cheque: CoinPaymentCheque): Subscription {
    ///   return truapi.coinPayment
    ///     .coinPaymentDeposit({ request: { cheque } })
    ///     .subscribe({
    ///       next: (status: CoinPaymentStatus) => console.log(status),
    ///       error: (error: SubscriptionError<HostCoinPaymentDepositError>) =>
    ///         console.error(error),
    ///     });
    /// }
    /// ```
    #[wire(start_id = 150)]
    async fn host_coin_payment_deposit(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentDepositRequest,
    ) -> Result<Subscription<HostCoinPaymentDepositItem>, CallError<HostCoinPaymentDepositError>>
    {
        Err(CallError::unavailable())
    }

    /// Attempt to return coins associated with a receivable.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostCoinPaymentRefundError,
    ///   type CoinPaymentReceivable,
    ///   type CoinPaymentStatus,
    ///   type Subscription,
    ///   type SubscriptionError,
    /// } from "@parity/truapi";
    ///
    /// export function refundReceivable(
    ///   truapi: Client,
    ///   receivable: CoinPaymentReceivable,
    /// ): Subscription {
    ///   return truapi.coinPayment
    ///     .coinPaymentRefund({ request: { receivable } })
    ///     .subscribe({
    ///       next: (status: CoinPaymentStatus) => console.log(status),
    ///       error: (error: SubscriptionError<HostCoinPaymentRefundError>) =>
    ///         console.error(error),
    ///     });
    /// }
    /// ```
    #[wire(start_id = 154)]
    async fn host_coin_payment_refund(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentRefundRequest,
    ) -> Result<Subscription<HostCoinPaymentRefundItem>, CallError<HostCoinPaymentRefundError>>
    {
        Err(CallError::unavailable())
    }

    /// Listen for a cheque delivered through a standard transmission channel.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostCoinPaymentListenForError,
    ///   type HostCoinPaymentListenForItem,
    ///   type CoinPaymentReceivable,
    ///   type Subscription,
    ///   type SubscriptionError,
    /// } from "@parity/truapi";
    ///
    /// export function listenForCheque(
    ///   truapi: Client,
    ///   receivable: CoinPaymentReceivable,
    /// ): Subscription {
    ///   return truapi.coinPayment
    ///     .coinPaymentListenFor({ request: { receivable } })
    ///     .subscribe({
    ///       next: (item: HostCoinPaymentListenForItem) => console.log(item),
    ///       error: (error: SubscriptionError<HostCoinPaymentListenForError>) =>
    ///         console.error(error),
    ///     });
    /// }
    /// ```
    #[wire(start_id = 158)]
    async fn host_coin_payment_listen_for(
        &self,
        _cx: &CallContext,
        _request: HostCoinPaymentListenForRequest,
    ) -> Result<Subscription<HostCoinPaymentListenForItem>, CallError<HostCoinPaymentListenForError>>
    {
        Err(CallError::unavailable())
    }
}
