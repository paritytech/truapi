//! Versioned wrappers for [`Transaction`](crate::api::Transaction) methods.

use crate::v01;

versioned_type! {
    pub enum HostCreateTransactionRequest { V1 => v01::HostCreateTransactionRequest }
    pub enum HostCreateTransactionResponse { V1 => v01::HostCreateTransactionResponse }
    pub enum HostCreateTransactionError { V1 => v01::HostCreateTransactionError }
    pub enum HostCreateTransactionWithLegacyAccountRequest { V1 => v01::HostCreateTransactionWithLegacyAccountRequest }
    pub enum HostCreateTransactionWithLegacyAccountResponse { V1 => v01::HostCreateTransactionWithLegacyAccountResponse }
    pub enum HostCreateTransactionWithLegacyAccountError { V1 => v01::HostCreateTransactionError }
}
