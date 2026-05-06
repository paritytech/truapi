//! Unified [`Payment`] trait (V0.2+).

use crate::v02::PaymentReceipt;
use crate::versioned::payment::{
    HostPaymentBalanceError, HostPaymentBalanceItem, HostPaymentBalanceSubscribeRequest,
    HostPaymentRequestError, HostPaymentRequestRequest, HostPaymentRequestResponse,
    HostPaymentStatusError, HostPaymentStatusItem, HostPaymentStatusSubscribeRequest,
    HostPaymentTopUpError, HostPaymentTopUpRequest, HostPaymentTopUpResponse,
};
use crate::wire;
use crate::{CallContext, Subscription};

/// Payment operations.
///
/// Every method has a default body that flags the call as unavailable through
/// [`CallContext::fail_unavailable`] and returns a placeholder value. Hosts
/// override only the methods they actually support.
#[async_trait::async_trait]
pub trait Payment: Send + Sync {
    /// Subscribe to payment balance updates.
    #[wire(id = 110)]
    async fn host_payment_balance_subscribe(
        &self,
        cx: &CallContext,
        _request: HostPaymentBalanceSubscribeRequest,
    ) -> Result<Subscription<HostPaymentBalanceItem>, HostPaymentBalanceError> {
        cx.fail_unavailable();
        Ok(Subscription::empty())
    }

    /// Top up the user's payment balance.
    #[wire(id = 120)]
    async fn host_payment_top_up(
        &self,
        cx: &CallContext,
        _request: HostPaymentTopUpRequest,
    ) -> Result<HostPaymentTopUpResponse, HostPaymentTopUpError> {
        cx.fail_unavailable();
        Ok(HostPaymentTopUpResponse::V2)
    }

    /// Request a payment from the user.
    #[wire(id = 114)]
    async fn host_payment_request(
        &self,
        cx: &CallContext,
        _request: HostPaymentRequestRequest,
    ) -> Result<HostPaymentRequestResponse, HostPaymentRequestError> {
        cx.fail_unavailable();
        Ok(HostPaymentRequestResponse::V2(PaymentReceipt {
            id: String::new(),
        }))
    }

    /// Subscribe to payment lifecycle updates for a specific payment.
    #[wire(id = 116)]
    async fn host_payment_status_subscribe(
        &self,
        cx: &CallContext,
        _request: HostPaymentStatusSubscribeRequest,
    ) -> Result<Subscription<HostPaymentStatusItem>, HostPaymentStatusError> {
        cx.fail_unavailable();
        Ok(Subscription::empty())
    }
}
