//! Versioned wrappers for [`Payment`](crate::api::Payment) methods.

use crate::v01;

versioned_type! {
    pub enum HostPaymentBalanceSubscribeRequest { V1 }
    pub enum HostPaymentBalanceSubscribeItem { V1 => v01::HostPaymentBalanceSubscribeItem }
    pub enum HostPaymentBalanceSubscribeError { V1 => v01::HostPaymentBalanceSubscribeError }
    pub enum HostPaymentTopUpRequest { V1 => v01::HostPaymentTopUpRequest }
    pub enum HostPaymentTopUpResponse { V1 }
    pub enum HostPaymentTopUpError { V1 => v01::HostPaymentTopUpError }
    pub enum HostPaymentRequestRequest { V1 => v01::HostPaymentRequestRequest }
    pub enum HostPaymentRequestResponse { V1 => v01::HostPaymentRequestResponse }
    pub enum HostPaymentRequestError { V1 => v01::HostPaymentRequestError }
    pub enum HostPaymentStatusSubscribeRequest { V1 => v01::HostPaymentStatusSubscribeRequest }
    pub enum HostPaymentStatusSubscribeItem { V1 => v01::HostPaymentStatusSubscribeItem }
    pub enum HostPaymentStatusSubscribeError { V1 => v01::HostPaymentStatusSubscribeError }
}
