//! Versioned wrappers for [`Payment`](crate::api::Payment) methods.

use crate::v02;

versioned_type! {
    /// Versioned wrapper for unit and older versions.
    pub enum HostPaymentBalanceSubscribeRequest { V2 }
    /// Versioned wrapper for [`v02::HostPaymentBalanceSubscribeItem`] and older versions.
    pub enum HostPaymentBalanceSubscribeItem { V2 => v02::HostPaymentBalanceSubscribeItem }
    /// Versioned wrapper for [`v02::HostPaymentBalanceSubscribeError`] and older versions.
    pub enum HostPaymentBalanceSubscribeError { V2 => v02::HostPaymentBalanceSubscribeError }
    /// Versioned wrapper for [`v02::HostPaymentTopUpRequest`] and older versions.
    pub enum HostPaymentTopUpRequest { V2 => v02::HostPaymentTopUpRequest }
    /// Versioned wrapper for unit and older versions.
    pub enum HostPaymentTopUpResponse { V2 }
    /// Versioned wrapper for [`v02::HostPaymentTopUpError`] and older versions.
    pub enum HostPaymentTopUpError { V2 => v02::HostPaymentTopUpError }
    /// Versioned wrapper for [`v02::HostPaymentRequestRequest`] and older versions.
    pub enum HostPaymentRequestRequest { V2 => v02::HostPaymentRequestRequest }
    /// Versioned wrapper for [`v02::HostPaymentRequestResponse`] and older versions.
    pub enum HostPaymentRequestResponse { V2 => v02::HostPaymentRequestResponse }
    /// Versioned wrapper for [`v02::HostPaymentRequestError`] and older versions.
    pub enum HostPaymentRequestError { V2 => v02::HostPaymentRequestError }
    /// Versioned wrapper for [`v02::HostPaymentStatusSubscribeRequest`] and older versions.
    pub enum HostPaymentStatusSubscribeRequest { V2 => v02::HostPaymentStatusSubscribeRequest }
    /// Versioned wrapper for [`v02::HostPaymentStatusSubscribeItem`] and older versions.
    pub enum HostPaymentStatusSubscribeItem { V2 => v02::HostPaymentStatusSubscribeItem }
    /// Versioned wrapper for [`v02::HostPaymentStatusSubscribeError`] and older versions.
    pub enum HostPaymentStatusSubscribeError { V2 => v02::HostPaymentStatusSubscribeError }
}
