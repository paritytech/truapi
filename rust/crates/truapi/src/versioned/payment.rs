//! Versioned wrappers for [`Payment`](crate::api::Payment) methods.

use crate::v01;

versioned_type! {
    /// Versioned wrapper for unit.
    pub enum HostPaymentBalanceSubscribeRequest { V1 }
    /// Versioned wrapper for [`v01::HostPaymentBalanceSubscribeItem`].
    pub enum HostPaymentBalanceSubscribeItem { V1 => v01::HostPaymentBalanceSubscribeItem }
    /// Versioned wrapper for [`v01::HostPaymentBalanceSubscribeError`].
    pub enum HostPaymentBalanceSubscribeError { V1 => v01::HostPaymentBalanceSubscribeError }
    /// Versioned wrapper for [`v01::HostPaymentTopUpRequest`].
    pub enum HostPaymentTopUpRequest { V1 => v01::HostPaymentTopUpRequest }
    /// Versioned wrapper for unit.
    pub enum HostPaymentTopUpResponse { V1 }
    /// Versioned wrapper for [`v01::HostPaymentTopUpError`].
    pub enum HostPaymentTopUpError { V1 => v01::HostPaymentTopUpError }
    /// Versioned wrapper for [`v01::HostPaymentRequestRequest`].
    pub enum HostPaymentRequestRequest { V1 => v01::HostPaymentRequestRequest }
    /// Versioned wrapper for [`v01::HostPaymentRequestResponse`].
    pub enum HostPaymentRequestResponse { V1 => v01::HostPaymentRequestResponse }
    /// Versioned wrapper for [`v01::HostPaymentRequestError`].
    pub enum HostPaymentRequestError { V1 => v01::HostPaymentRequestError }
    /// Versioned wrapper for [`v01::HostPaymentStatusSubscribeRequest`].
    pub enum HostPaymentStatusSubscribeRequest { V1 => v01::HostPaymentStatusSubscribeRequest }
    /// Versioned wrapper for [`v01::HostPaymentStatusSubscribeItem`].
    pub enum HostPaymentStatusSubscribeItem { V1 => v01::HostPaymentStatusSubscribeItem }
    /// Versioned wrapper for [`v01::HostPaymentStatusSubscribeError`].
    pub enum HostPaymentStatusSubscribeError { V1 => v01::HostPaymentStatusSubscribeError }
}
