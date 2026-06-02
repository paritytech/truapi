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
    /// import { from, take } from "rxjs";
    ///
    /// from(truapi.payment.balanceSubscribe({ request: {} }))
    ///   .pipe(take(1))
    ///   .subscribe({
    ///     next: (balance) => console.log(balance),
    ///     error: (error) => console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
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
    ///   source: { tag: "ProductAccount", value: { derivationIndex: 0 } },
    /// });
    ///
    /// if (topUp.isErr()) {
    ///   console.error("topUp failed:", topUp.error);
    /// } else {
    ///   const result = await truapi.payment.request({
    ///     amount: 1000n,
    ///     destination:
    ///       "0x0000000000000000000000000000000000000000000000000000000000000000",
    ///   });
    ///   result.match(
    ///     (value) => console.log(value),
    ///     (error) => console.error("request failed:", error),
    ///   );
    /// }
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
    /// import { from, take } from "rxjs";
    ///
    /// // Fund the balance and start a payment first so there is a status to watch.
    /// const topUp = await truapi.payment.topUp({
    ///   amount: 1000n,
    ///   source: { tag: "ProductAccount", value: { derivationIndex: 0 } },
    /// });
    ///
    /// if (topUp.isErr()) {
    ///   console.error("topUp failed:", topUp.error);
    /// } else {
    ///   const requested = await truapi.payment.request({
    ///     amount: 1000n,
    ///     destination:
    ///       "0x0000000000000000000000000000000000000000000000000000000000000000",
    ///   });
    ///   if (requested.isErr()) {
    ///     console.error("request failed:", requested.error);
    ///   } else {
    ///     from(
    ///       truapi.payment.statusSubscribe({
    ///         request: { paymentId: requested.value.id },
    ///       }),
    ///     )
    ///       .pipe(take(1))
    ///       .subscribe({
    ///         next: (status) => console.log(status),
    ///         error: (error) => console.error("statusSubscribe failed:", error),
    ///         complete: () => console.log("completed"),
    ///       });
    ///   }
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
    /// ```ts
    /// const result = await truapi.payment.topUp({
    ///   amount: 1000n,
    ///   source: { tag: "ProductAccount", value: { derivationIndex: 0 } },
    /// });
    /// result.match(
    ///   () => console.log("ok"),
    ///   (error) => console.error(error),
    /// );
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
