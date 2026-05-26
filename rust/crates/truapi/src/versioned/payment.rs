//! Versioned wrappers for [`Payment`](crate::api::Payment) methods.

use crate::v01;

versioned_type! {
    pub enum HostPaymentBalanceSubscribeRequest { V1 => v01::HostPaymentBalanceSubscribeRequest }
    pub enum HostPaymentBalanceSubscribeItem { V1 => v01::HostPaymentBalanceSubscribeItem }
    pub enum HostPaymentBalanceSubscribeError { V1 => v01::HostPaymentBalanceSubscribeError }
    pub enum HostPaymentTopUpRequest { V1 => v01::HostPaymentTopUpRequest }
    pub enum HostPaymentTopUpResponse { V1 }
    pub enum HostPaymentTopUpError { V1 => v01::HostPaymentTopUpError }
    pub enum HostPaymentRequest { V1 => v01::HostPaymentRequest }
    pub enum HostPaymentResponse { V1 => v01::HostPaymentResponse }
    pub enum HostPaymentError { V1 => v01::HostPaymentError }
    pub enum HostPaymentStatusSubscribeRequest { V1 => v01::HostPaymentStatusSubscribeRequest }
    pub enum HostPaymentStatusSubscribeItem { V1 => v01::HostPaymentStatusSubscribeItem }
    pub enum HostPaymentStatusSubscribeError { V1 => v01::HostPaymentStatusSubscribeError }
}
