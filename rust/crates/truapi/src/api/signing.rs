//! Unified [`Signing`] trait.

use crate::versioned::signing::{
    HostCreateTransactionError, HostCreateTransactionRequest, HostCreateTransactionResponse,
    HostCreateTransactionWithNonProductAccountError,
    HostCreateTransactionWithNonProductAccountRequest,
    HostCreateTransactionWithNonProductAccountResponse, HostSignPayloadError,
    HostSignPayloadRequest, HostSignPayloadResponse, HostSignRawError, HostSignRawRequest,
    HostSignRawResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Signing and transaction construction.
///
/// Default methods return [`CallError::HostFailure`] with an `unavailable`
/// reason. Hosts override only the methods they actually support.
#[async_trait::async_trait]
pub trait Signing: Send + Sync {
    /// Construct a signed extrinsic for a product account.
    #[wire(id = 30)]
    async fn host_create_transaction(
        &self,
        _cx: &CallContext,
        _request: HostCreateTransactionRequest,
    ) -> Result<HostCreateTransactionResponse, CallError<HostCreateTransactionError>> {
        Err(CallError::unavailable())
    }

    /// Construct a signed extrinsic for a non-product account.
    #[wire(id = 32)]
    async fn host_create_transaction_with_non_product_account(
        &self,
        _cx: &CallContext,
        _request: HostCreateTransactionWithNonProductAccountRequest,
    ) -> Result<
        HostCreateTransactionWithNonProductAccountResponse,
        CallError<HostCreateTransactionWithNonProductAccountError>,
    > {
        Err(CallError::unavailable())
    }

    /// Sign raw bytes or a message.
    #[wire(id = 34)]
    async fn host_sign_raw(
        &self,
        _cx: &CallContext,
        _request: HostSignRawRequest,
    ) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
        Err(CallError::unavailable())
    }

    /// Sign a Substrate extrinsic payload.
    #[wire(id = 36)]
    async fn host_sign_payload(
        &self,
        _cx: &CallContext,
        _request: HostSignPayloadRequest,
    ) -> Result<HostSignPayloadResponse, CallError<HostSignPayloadError>> {
        Err(CallError::unavailable())
    }
}
