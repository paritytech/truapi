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
    #[wire(id = 122)]
    async fn host_payment_top_up(
        &self,
        _cx: &CallContext,
        _request: HostPaymentTopUpRequest,
    ) -> Result<HostPaymentTopUpResponse, CallError<HostPaymentTopUpError>> {
        Err(CallError::unavailable())
    }
}
