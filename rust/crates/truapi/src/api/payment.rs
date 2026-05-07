//! Unified [`Payment`] trait (V0.2+).

use crate::versioned::payment::{
    HostPaymentBalanceItem, HostPaymentBalanceSubscribeRequest, HostPaymentRequestError,
    HostPaymentRequestRequest, HostPaymentRequestResponse, HostPaymentStatusItem,
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
    #[wire(id = 110)]
    async fn host_payment_balance_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPaymentBalanceSubscribeRequest,
    ) -> Subscription<HostPaymentBalanceItem> {
        Subscription::empty()
    }

    /// Request a payment from the user.
    #[wire(id = 114)]
    async fn host_payment_request(
        &self,
        _cx: &CallContext,
        _request: HostPaymentRequestRequest,
    ) -> Result<HostPaymentRequestResponse, CallError<HostPaymentRequestError>> {
        Err(CallError::unavailable())
    }

    /// Subscribe to payment lifecycle updates for a specific payment.
    #[wire(id = 116)]
    async fn host_payment_status_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostPaymentStatusSubscribeRequest,
    ) -> Subscription<HostPaymentStatusItem> {
        Subscription::empty()
    }

    /// Top up the user's payment balance.
    #[wire(id = 120)]
    async fn host_payment_top_up(
        &self,
        _cx: &CallContext,
        _request: HostPaymentTopUpRequest,
    ) -> Result<HostPaymentTopUpResponse, CallError<HostPaymentTopUpError>> {
        Err(CallError::unavailable())
    }
}
