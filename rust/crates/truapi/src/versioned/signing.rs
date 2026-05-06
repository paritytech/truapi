//! Versioned wrappers for [`Signing`](crate::api::Signing) methods.

use crate::{v01, v02};

versioned_type! {
    /// Request wrapper for `host_sign_payload`.
    ///
    /// V1 uses the legacy `address: String` shape; V2 carries
    /// `account: ProductAccountId` per RFC-0005. Cross-version conversion is
    /// lossy in both directions, so `into_version` only succeeds
    /// when the requested version matches the active variant.
    pub enum HostSignPayloadRequest { V1 => v01::SigningPayload, V2 => v02::SigningPayload }
    /// Response wrapper for `host_sign_payload`.
    pub enum HostSignPayloadResponse { V1 => v01::SigningResult, V2 => v01::SigningResult }
    /// Error wrapper for `host_sign_payload`.
    pub enum HostSignPayloadError { V1 => v01::SigningError, V2 => v01::SigningError }
    /// Request wrapper for `host_sign_raw`.
    ///
    /// V1 uses the legacy `address: String` shape; V2 carries
    /// `account: ProductAccountId` per RFC-0005. Cross-version conversion is
    /// lossy in both directions.
    pub enum HostSignRawRequest { V1 => v01::SigningRawPayload, V2 => v02::SigningRawPayload }
    /// Response wrapper for `host_sign_raw`.
    pub enum HostSignRawResponse { V1 => v01::SigningResult, V2 => v01::SigningResult }
    /// Error wrapper for `host_sign_raw`.
    pub enum HostSignRawError { V1 => v01::SigningError, V2 => v01::SigningError }
    /// Request wrapper for `host_create_transaction`.
    pub enum HostCreateTransactionRequest { V1 => v01::CreateTransactionRequest }
    /// Response wrapper for `host_create_transaction`.
    pub enum HostCreateTransactionResponse { V1 => v01::Bytes }
    /// Error wrapper for `host_create_transaction`.
    pub enum HostCreateTransactionError { V1 => v01::CreateTransactionError }
    /// Request wrapper for `host_create_transaction_with_non_product_account`.
    pub enum HostCreateTransactionWithNonProductAccountRequest { V1 => v01::VersionedTxPayload }
    /// Response wrapper for `host_create_transaction_with_non_product_account`.
    pub enum HostCreateTransactionWithNonProductAccountResponse { V1 => v01::Bytes }
    /// Error wrapper for `host_create_transaction_with_non_product_account`.
    pub enum HostCreateTransactionWithNonProductAccountError { V1 => v01::CreateTransactionError }
}
