//! Versioned wrappers for [`Signing`](crate::api::Signing) methods.

use crate::v01;

versioned_type! {
    /// Versioned wrapper for [`v01::HostSignPayloadRequest`].
    pub enum HostSignPayloadRequest { V1 => v01::HostSignPayloadRequest }
    /// Versioned wrapper for [`v01::HostSignPayloadResponse`].
    pub enum HostSignPayloadResponse { V1 => v01::HostSignPayloadResponse }
    /// Versioned wrapper for [`v01::HostSignPayloadError`].
    pub enum HostSignPayloadError { V1 => v01::HostSignPayloadError }
    /// Versioned wrapper for [`v01::HostSignRawRequest`].
    pub enum HostSignRawRequest { V1 => v01::HostSignRawRequest }
    /// Versioned wrapper for the sign-raw response; reuses [`v01::HostSignPayloadResponse`].
    pub enum HostSignRawResponse { V1 => v01::HostSignPayloadResponse }
    /// Versioned wrapper for the sign-raw error; reuses [`v01::HostSignPayloadError`].
    pub enum HostSignRawError { V1 => v01::HostSignPayloadError }
    /// Versioned wrapper for [`v01::HostSignRawWithLegacyAccountRequest`].
    pub enum HostSignRawWithLegacyAccountRequest { V1 => v01::HostSignRawWithLegacyAccountRequest }
    /// Versioned wrapper for the legacy-account sign-raw response; reuses [`v01::HostSignPayloadResponse`].
    pub enum HostSignRawWithLegacyAccountResponse { V1 => v01::HostSignPayloadResponse }
    /// Versioned wrapper for the legacy-account sign-raw error; reuses [`v01::HostSignPayloadError`].
    pub enum HostSignRawWithLegacyAccountError { V1 => v01::HostSignPayloadError }
    /// Versioned wrapper for [`v01::HostSignPayloadWithLegacyAccountRequest`].
    pub enum HostSignPayloadWithLegacyAccountRequest { V1 => v01::HostSignPayloadWithLegacyAccountRequest }
    /// Versioned wrapper for the legacy-account sign-payload response; reuses [`v01::HostSignPayloadResponse`].
    pub enum HostSignPayloadWithLegacyAccountResponse { V1 => v01::HostSignPayloadResponse }
    /// Versioned wrapper for the legacy-account sign-payload error; reuses [`v01::HostSignPayloadError`].
    pub enum HostSignPayloadWithLegacyAccountError { V1 => v01::HostSignPayloadError }
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
