//! Unified [`Payment`] trait.

use crate::versioned::payment::{
    HostPaymentBalanceSubscribeError, HostPaymentBalanceSubscribeItem,
    HostPaymentBalanceSubscribeRequest, HostPaymentError, HostPaymentRequest, HostPaymentResponse,
    HostPaymentStatusSubscribeError, HostPaymentStatusSubscribeItem,
    HostPaymentStatusSubscribeRequest, HostPaymentTopUpError, HostPaymentTopUpRequest,
    HostPaymentTopUpResponse,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Payment request and balance/status subscription methods.
pub trait Payment: Send + Sync {
    /// Subscribe to payment balance updates.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const balance = await firstValueFrom(
    ///   from(truapi.payment.balanceSubscribe({ request: {} })),
    /// );
    /// console.log("balance received:", balance);
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
    /// ```ts
    /// // Fund the balance first so the request is not rejected for lack of funds.
    /// const topUp = await truapi.payment.topUp({
    ///   amount: 1000n,
    ///   source: { tag: "ProductAccount", value: { derivationIndex: { tag: "Left", value: 0 } } },
    /// });
    /// assert(topUp.isOk(), "topUp failed:", topUp);
    ///
    /// const result = await truapi.payment.request({
    ///   amount: 1000n,
    ///   destination:
    ///     "0x0000000000000000000000000000000000000000000000000000000000000000",
    /// });
    /// assert(result.isOk(), "request failed:", result);
    /// console.log("payment requested:", result.value);
    /// ```
    #[wire(request_id = 124)]
    async fn request(
        &self,
        _cx: &CallContext,
        _request: HostPaymentRequest,
    ) -> Result<HostPaymentResponse, CallError<HostPaymentError>> {
        Err(CallError::unavailable())
    }

    /// Subscribe to payment lifecycle updates for a specific payment.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// // Fund the balance and start a payment first so there is a status to watch.
    /// const topUp = await truapi.payment.topUp({
    ///   amount: 1000n,
    ///   source: { tag: "ProductAccount", value: { derivationIndex: { tag: "Left", value: 0 } } },
    /// });
    /// assert(topUp.isOk(), "topUp failed:", topUp);
    ///
    /// const requested = await truapi.payment.request({
    ///   amount: 1000n,
    ///   destination:
    ///     "0x0000000000000000000000000000000000000000000000000000000000000000",
    /// });
    /// assert(requested.isOk(), "request failed:", requested);
    ///
    /// const status = await firstValueFrom(
    ///   from(
    ///     truapi.payment.statusSubscribe({
    ///       request: { paymentId: requested.value.id },
    ///     }),
    ///   ),
    /// );
    /// console.log("payment status received:", status);
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
    /// ```ts
    /// const result = await truapi.payment.topUp({
    ///   amount: 1000n,
    ///   source: { tag: "ProductAccount", value: { derivationIndex: { tag: "Left", value: 0 } } },
    /// });
    /// assert(result.isOk(), "topUp failed:", result);
    /// console.log("balance topped up");
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
