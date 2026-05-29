//! Versioned wrappers for [`Signing`](crate::api::Signing) methods.

use crate::v01;
use truapi_macros::versioned_type;

versioned_type! {
    pub enum HostSignPayloadRequest { V1 => v01::HostSignPayloadRequest }
    pub enum HostSignPayloadResponse { V1 => v01::HostSignPayloadResponse }
    pub enum HostSignPayloadError { V1 => v01::HostSignPayloadError }
    pub enum HostSignRawRequest { V1 => v01::HostSignRawRequest }
    pub enum HostSignRawResponse { V1 => v01::HostSignPayloadResponse }
    pub enum HostSignRawError { V1 => v01::HostSignPayloadError }
    pub enum HostSignRawWithLegacyAccountRequest { V1 => v01::HostSignRawWithLegacyAccountRequest }
    pub enum HostSignRawWithLegacyAccountResponse { V1 => v01::HostSignPayloadResponse }
    pub enum HostSignRawWithLegacyAccountError { V1 => v01::HostSignPayloadError }
    pub enum HostSignPayloadWithLegacyAccountRequest { V1 => v01::HostSignPayloadWithLegacyAccountRequest }
    pub enum HostSignPayloadWithLegacyAccountResponse { V1 => v01::HostSignPayloadResponse }
    pub enum HostSignPayloadWithLegacyAccountError { V1 => v01::HostSignPayloadError }
    pub enum HostCreateTransactionRequest { V1 => v01::ProductAccountTxPayload }
    pub enum HostCreateTransactionResponse { V1 => v01::HostCreateTransactionResponse }
    pub enum HostCreateTransactionError { V1 => v01::HostCreateTransactionError }
    pub enum HostCreateTransactionWithLegacyAccountRequest { V1 => v01::LegacyAccountTxPayload }
    pub enum HostCreateTransactionWithLegacyAccountResponse { V1 => v01::HostCreateTransactionWithLegacyAccountResponse }
    pub enum HostCreateTransactionWithLegacyAccountError { V1 => v01::HostCreateTransactionError }
}
