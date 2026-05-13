//! Unified [`Payment`] trait.

use crate::versioned::payment::{
    HostPaymentBalanceSubscribeError, HostPaymentBalanceSubscribeItem,
    HostPaymentBalanceSubscribeRequest, HostPaymentRequestError, HostPaymentRequestRequest,
    HostPaymentRequestResponse, HostPaymentStatusSubscribeError, HostPaymentStatusSubscribeItem,
    HostPaymentStatusSubscribeRequest, HostPaymentTopUpError, HostPaymentTopUpRequest,
    HostPaymentTopUpResponse,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Payment request and balance/status subscription methods.
#[async_trait::async_trait]
pub trait Payment: Send + Sync {
    /// Subscribe to payment balance updates.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostPaymentBalanceSubscribeError,
    ///   type HostPaymentBalanceSubscribeItem,
    ///   type Subscription,
    ///   type SubscriptionError,
    /// } from "@parity/truapi";
    ///
    /// export function watchPaymentBalance(truapi: Client): Subscription {
    ///   return truapi.payment.balanceSubscribe().subscribe({
    ///     next: (balance: HostPaymentBalanceSubscribeItem) =>
    ///       console.log(balance),
    ///     error: (error: SubscriptionError<HostPaymentBalanceSubscribeError>) =>
    ///       console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
    /// }
    /// ```
    #[wire(start_id = 118)]
    async fn balance_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPaymentBalanceSubscribeRequest,
    ) -> Result<
        Subscription<HostPaymentBalanceSubscribeItem>,
        CallError<HostPaymentBalanceSubscribeError>,
    > {
        Err(CallError::unavailable())
    }

    /// Request a payment from the user.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostPaymentRequestResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function requestPayment(
    ///   truapi: Client,
    /// ): Promise<HostPaymentRequestResponse> {
    ///   const result = await truapi.payment.request({
    ///     amount: 1000000000000n,
    ///     destination: "0x0000000000000000000000000000000000000000000000000000000000000000",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 124)]
    async fn request(
        &self,
        _cx: &CallContext,
        _request: HostPaymentRequestRequest,
    ) -> Result<HostPaymentRequestResponse, CallError<HostPaymentRequestError>> {
        Err(CallError::unavailable())
    }

    /// Subscribe to payment lifecycle updates for a specific payment.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostPaymentStatusSubscribeError,
    ///   type HostPaymentStatusSubscribeItem,
    ///   type Subscription,
    ///   type SubscriptionError,
    /// } from "@parity/truapi";
    ///
    /// export function watchPaymentStatus(truapi: Client): Subscription {
    ///   return truapi.payment
    ///     .statusSubscribe({
    ///       request: { paymentId: "payment-id" },
    ///     })
    ///     .subscribe({
    ///       next: (status: HostPaymentStatusSubscribeItem) =>
    ///         console.log(status),
    ///       error: (error: SubscriptionError<HostPaymentStatusSubscribeError>) =>
    ///         console.error(error),
    ///       complete: () => console.log("completed"),
    ///     });
    /// }
    /// ```
    #[wire(start_id = 126)]
    async fn status_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPaymentStatusSubscribeRequest,
    ) -> Result<
        Subscription<HostPaymentStatusSubscribeItem>,
        CallError<HostPaymentStatusSubscribeError>,
    > {
        Err(CallError::unavailable())
    }

    /// Top up the user's payment balance.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function topUpPaymentBalance(truapi: Client): Promise<void> {
    ///   const result = await truapi.payment.topUp({
    ///     amount: 1000000000000n,
    ///     source: { tag: "ProductAccount", value: { derivationIndex: 0 } },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 122)]
    async fn top_up(
        &self,
        _cx: &CallContext,
        _request: HostPaymentTopUpRequest,
    ) -> Result<HostPaymentTopUpResponse, CallError<HostPaymentTopUpError>> {
        Err(CallError::unavailable())
    }
}
