//! Unified [`Payment`] trait (V0.2+).

use crate::versioned::payment::{
    HostPaymentBalanceSubscribeItem, HostPaymentBalanceSubscribeRequest, HostPaymentRequestError,
    HostPaymentRequestRequest, HostPaymentRequestResponse, HostPaymentStatusSubscribeItem,
    HostPaymentStatusSubscribeRequest, HostPaymentTopUpError, HostPaymentTopUpRequest,
    HostPaymentTopUpResponse,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Payment operations.
///
/// Default methods return [`CallError::HostFailure`] with an `unavailable`
/// reason. Hosts override only the methods they actually support.
#[async_trait::async_trait]
pub trait Payment: Send + Sync {
    /// Subscribe to payment balance updates.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export function watchPaymentBalance(truapi: Client) {
    ///   return truapi.payment.paymentBalanceSubscribe({
    ///     onData: (balance) => console.log(balance),
    ///     onError: console.error,
    ///     onInterrupt: () => console.log("interrupted"),
    ///     onClose: console.error,
    ///   });
    /// }
    /// ```
    #[wire(start_id = 118)]
    async fn host_payment_balance_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPaymentBalanceSubscribeRequest,
    ) -> Subscription<HostPaymentBalanceSubscribeItem> {
        Subscription::empty()
    }

    /// Request a payment from the user.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function requestPayment(truapi: Client) {
    ///   const result = await truapi.payment.paymentRequest({
    ///     amount: 0n,
    ///     destination: new Uint8Array(),
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 124)]
    async fn host_payment_request(
        &self,
        _cx: &CallContext,
        _request: HostPaymentRequestRequest,
    ) -> Result<HostPaymentRequestResponse, CallError<HostPaymentRequestError>> {
        Err(CallError::unavailable())
    }

    /// Subscribe to payment lifecycle updates for a specific payment.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export function watchPaymentStatus(truapi: Client) {
    ///   return truapi.payment.paymentStatusSubscribe({
    ///     request: { paymentId: "payment-id" },
    ///     onData: (status) => console.log(status),
    ///     onError: console.error,
    ///     onInterrupt: () => console.log("interrupted"),
    ///     onClose: console.error,
    ///   });
    /// }
    /// ```
    #[wire(start_id = 126)]
    async fn host_payment_status_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPaymentStatusSubscribeRequest,
    ) -> Subscription<HostPaymentStatusSubscribeItem> {
        Subscription::empty()
    }

    /// Top up the user's payment balance.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function topUpPaymentBalance(truapi: Client) {
    ///   const result = await truapi.payment.paymentTopUp({
    ///     amount: 0n,
    ///     source: { tag: "ProductAccount", value: { derivationIndex: 0 } },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 122)]
    async fn host_payment_top_up(
        &self,
        _cx: &CallContext,
        _request: HostPaymentTopUpRequest,
    ) -> Result<HostPaymentTopUpResponse, CallError<HostPaymentTopUpError>> {
        Err(CallError::unavailable())
    }
}
