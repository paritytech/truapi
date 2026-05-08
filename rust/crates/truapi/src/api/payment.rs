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
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export function watchPaymentBalance(provider: Provider) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   return truapi.payment.paymentBalanceSubscribe({
    ///     onData: (balance) => console.log(balance),
    ///     onError: console.error,
    ///     onInterrupt: () => console.log("interrupted"),
    ///     onClose: console.error,
    ///   });
    /// }
    /// ```
    #[wire(id = 118)]
    async fn host_payment_balance_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPaymentBalanceSubscribeRequest,
    ) -> Subscription<HostPaymentBalanceSubscribeItem> {
        Subscription::empty()
    }

    /// Request a payment from the user.
    ///
    /// ```truapi-playground-request
    /// { "amount": "0n", "destination": "0x0000000000000000000000000000000000000000000000000000000000000000" }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export async function requestPayment(
    ///   provider: Provider,
    ///   amount: bigint,
    ///   destination: Uint8Array,
    /// ) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.payment.paymentRequest({
    ///     amount,
    ///     destination,
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(id = 124)]
    async fn host_payment_request(
        &self,
        _cx: &CallContext,
        _request: HostPaymentRequestRequest,
    ) -> Result<HostPaymentRequestResponse, CallError<HostPaymentRequestError>> {
        Err(CallError::unavailable())
    }

    /// Subscribe to payment lifecycle updates for a specific payment.
    ///
    /// ```truapi-playground-request
    /// { "paymentId": "payment-id" }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export function watchPaymentStatus(provider: Provider, paymentId: string) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   return truapi.payment.paymentStatusSubscribe({
    ///     request: { paymentId },
    ///     onData: (status) => console.log(status),
    ///     onError: console.error,
    ///     onInterrupt: () => console.log("interrupted"),
    ///     onClose: console.error,
    ///   });
    /// }
    /// ```
    #[wire(id = 126)]
    async fn host_payment_status_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPaymentStatusSubscribeRequest,
    ) -> Subscription<HostPaymentStatusSubscribeItem> {
        Subscription::empty()
    }

    /// Top up the user's payment balance.
    ///
    /// ```truapi-playground-request
    /// { "amount": "0n", "source": { "tag": "ProductAccount", "value": { "derivationIndex": 0 } } }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export async function topUpPaymentBalance(provider: Provider, amount: bigint) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.payment.paymentTopUp({
    ///     amount,
    ///     source: {
    ///       tag: "ProductAccount",
    ///       value: { derivationIndex: 0 },
    ///     },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(id = 122)]
    async fn host_payment_top_up(
        &self,
        _cx: &CallContext,
        _request: HostPaymentTopUpRequest,
    ) -> Result<HostPaymentTopUpResponse, CallError<HostPaymentTopUpError>> {
        Err(CallError::unavailable())
    }
}
