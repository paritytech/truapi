//! Versioned wrappers for [`Payment`](crate::api::Payment) methods.

use crate::v02;

versioned_type! {
    /// Subscription request wrapper for `host_payment_balance_subscribe` (V0.2+ only).
    pub enum HostPaymentBalanceSubscribeRequest { V2 }
    /// Subscription item wrapper for `host_payment_balance_subscribe` (V0.2+ only).
    pub enum HostPaymentBalanceSubscribeItem { V2 => v02::PaymentBalance }
    /// Error wrapper for `host_payment_balance_subscribe` (V0.2+ only).
    pub enum HostPaymentBalanceSubscribeError { V2 => v02::PaymentBalanceError }
    /// Request wrapper for `host_payment_top_up` (V0.2+ only).
    pub enum HostPaymentTopUpRequest { V2 => v02::PaymentTopUpRequest }
    /// Response wrapper for `host_payment_top_up` (V0.2+ only).
    pub enum HostPaymentTopUpResponse { V2 }
    /// Error wrapper for `host_payment_top_up` (V0.2+ only).
    pub enum HostPaymentTopUpError { V2 => v02::PaymentTopUpError }
    /// Request wrapper for `host_payment_request` (V0.2+ only).
    pub enum HostPaymentRequestRequest { V2 => v02::PaymentRequest }
    /// Response wrapper for `host_payment_request` (V0.2+ only).
    pub enum HostPaymentRequestResponse { V2 => v02::PaymentReceipt }
    /// Error wrapper for `host_payment_request` (V0.2+ only).
    pub enum HostPaymentRequestError { V2 => v02::PaymentRequestError }
    /// Subscription request wrapper for `host_payment_status_subscribe` (V0.2+ only).
    pub enum HostPaymentStatusSubscribeRequest { V2 => v02::PaymentId }
    /// Subscription item wrapper for `host_payment_status_subscribe` (V0.2+ only).
    pub enum HostPaymentStatusSubscribeItem { V2 => v02::PaymentStatus }
    /// Error wrapper for `host_payment_status_subscribe` (V0.2+ only).
    pub enum HostPaymentStatusSubscribeError { V2 => v02::PaymentStatusError }
}
