//! Versioned wrappers for [`Signing`](crate::api::Signing) methods.

use crate::{v01, v02};

versioned_type! {
    /// Versioned wrapper covering both v0.1 and v0.2 sign-payload requests.
    pub enum HostSignPayloadRequest { V1 => v01::HostSignPayloadRequest, V2 => v02::HostSignPayloadRequest }
    /// Versioned wrapper for the sign-payload response (shared across v0.1/v0.2).
    pub enum HostSignPayloadResponse { V1 => v01::HostSignPayloadResponse, V2 => v01::HostSignPayloadResponse }
    /// Versioned wrapper for the sign-payload error (shared across v0.1/v0.2).
    pub enum HostSignPayloadError { V1 => v01::HostSignPayloadError, V2 => v01::HostSignPayloadError }
    /// Versioned wrapper covering both v0.1 and v0.2 sign-raw requests.
    pub enum HostSignRawRequest { V1 => v01::HostSignRawRequest, V2 => v02::HostSignRawRequest }
    /// Versioned wrapper for the sign-raw response; reuses [`v01::HostSignPayloadResponse`].
    pub enum HostSignRawResponse { V1 => v01::HostSignPayloadResponse, V2 => v01::HostSignPayloadResponse }
    /// Versioned wrapper for the sign-raw error; reuses [`v01::HostSignPayloadError`].
    pub enum HostSignRawError { V1 => v01::HostSignPayloadError, V2 => v01::HostSignPayloadError }
    /// Versioned wrapper for [`v01::HostCreateTransactionRequest`].
    pub enum HostCreateTransactionRequest { V1 => v01::HostCreateTransactionRequest }
    /// Versioned wrapper for [`v01::HostCreateTransactionResponse`].
    pub enum HostCreateTransactionResponse { V1 => v01::HostCreateTransactionResponse }
    /// Versioned wrapper for [`v01::HostCreateTransactionError`].
    pub enum HostCreateTransactionError { V1 => v01::HostCreateTransactionError }
    /// Versioned wrapper for [`v01::HostCreateTransactionWithLegacyAccountRequest`].
    pub enum HostCreateTransactionWithLegacyAccountRequest { V1 => v01::HostCreateTransactionWithLegacyAccountRequest }
    /// Versioned wrapper for [`v01::HostCreateTransactionWithLegacyAccountResponse`].
    pub enum HostCreateTransactionWithLegacyAccountResponse { V1 => v01::HostCreateTransactionWithLegacyAccountResponse }
    /// Versioned wrapper for the legacy-account create-transaction error path; reuses [`v01::HostCreateTransactionError`].
    pub enum HostCreateTransactionWithLegacyAccountError { V1 => v01::HostCreateTransactionError }
}
