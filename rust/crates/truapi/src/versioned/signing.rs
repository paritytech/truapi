//! Versioned wrappers for [`Signing`](crate::api::Signing) methods.

use crate::{v01, v02};

versioned_type! {
    /// Versioned wrapper for [`v02::HostSignPayloadRequest`] and older versions.
    pub enum HostSignPayloadRequest { V1 => v01::HostSignPayloadRequest, V2 => v02::HostSignPayloadRequest }
    /// Versioned wrapper for [`v01::HostSignPayloadResponse`] and older versions.
    pub enum HostSignPayloadResponse { V1 => v01::HostSignPayloadResponse, V2 => v01::HostSignPayloadResponse }
    /// Versioned wrapper for [`v01::HostSignPayloadError`] and older versions.
    pub enum HostSignPayloadError { V1 => v01::HostSignPayloadError, V2 => v01::HostSignPayloadError }
    /// Versioned wrapper for [`v02::HostSignRawRequest`] and older versions.
    pub enum HostSignRawRequest { V1 => v01::HostSignRawRequest, V2 => v02::HostSignRawRequest }
    /// Versioned wrapper for [`v01::HostSignRawResponse`] and older versions.
    pub enum HostSignRawResponse { V1 => v01::HostSignRawResponse, V2 => v01::HostSignRawResponse }
    /// Versioned wrapper for [`v01::HostSignRawError`] and older versions.
    pub enum HostSignRawError { V1 => v01::HostSignRawError, V2 => v01::HostSignRawError }
    /// Versioned wrapper for [`v01::HostCreateTransactionRequest`] and older versions.
    pub enum HostCreateTransactionRequest { V1 => v01::HostCreateTransactionRequest }
    /// Versioned wrapper for [`v01::HostCreateTransactionResponse`] and older versions.
    pub enum HostCreateTransactionResponse { V1 => v01::HostCreateTransactionResponse }
    /// Versioned wrapper for [`v01::HostCreateTransactionError`] and older versions.
    pub enum HostCreateTransactionError { V1 => v01::HostCreateTransactionError }
    /// Versioned wrapper for [`v01::HostCreateTransactionWithNonProductAccountRequest`] and older versions.
    pub enum HostCreateTransactionWithNonProductAccountRequest { V1 => v01::HostCreateTransactionWithNonProductAccountRequest }
    /// Versioned wrapper for [`v01::HostCreateTransactionWithNonProductAccountResponse`] and older versions.
    pub enum HostCreateTransactionWithNonProductAccountResponse { V1 => v01::HostCreateTransactionWithNonProductAccountResponse }
    /// Versioned wrapper for [`v01::HostCreateTransactionWithNonProductAccountError`] and older versions.
    pub enum HostCreateTransactionWithNonProductAccountError { V1 => v01::HostCreateTransactionWithNonProductAccountError }
}
