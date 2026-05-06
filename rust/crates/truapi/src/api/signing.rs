//! Unified [`Signing`] trait.

use crate::v01::SigningResult;
use crate::versioned::signing::{
    HostCreateTransactionError, HostCreateTransactionRequest, HostCreateTransactionResponse,
    HostCreateTransactionWithNonProductAccountError,
    HostCreateTransactionWithNonProductAccountRequest,
    HostCreateTransactionWithNonProductAccountResponse, HostSignPayloadError,
    HostSignPayloadRequest, HostSignPayloadResponse, HostSignRawError, HostSignRawRequest,
    HostSignRawResponse,
};
use crate::wire;
use crate::CallContext;

/// Signing and transaction construction.
///
/// Every method has a default body that flags the call as unavailable through
/// [`CallContext::fail_unavailable`] and returns a placeholder value. Hosts
/// override only the methods they actually support.
#[async_trait::async_trait]
pub trait Signing: Send + Sync {
    /// Sign a Substrate extrinsic payload.
    #[wire(id = 36)]
    async fn host_sign_payload(
        &self,
        cx: &CallContext,
        _request: HostSignPayloadRequest,
    ) -> Result<HostSignPayloadResponse, HostSignPayloadError> {
        cx.fail_unavailable();
        Ok(HostSignPayloadResponse::V2(SigningResult {
            signature: Vec::new(),
            signed_transaction: None,
        }))
    }

    /// Sign raw bytes or a message.
    #[wire(id = 34)]
    async fn host_sign_raw(
        &self,
        cx: &CallContext,
        _request: HostSignRawRequest,
    ) -> Result<HostSignRawResponse, HostSignRawError> {
        cx.fail_unavailable();
        Ok(HostSignRawResponse::V2(SigningResult {
            signature: Vec::new(),
            signed_transaction: None,
        }))
    }

    /// Construct a signed extrinsic for a product account.
    #[wire(id = 30)]
    async fn host_create_transaction(
        &self,
        cx: &CallContext,
        _request: HostCreateTransactionRequest,
    ) -> Result<HostCreateTransactionResponse, HostCreateTransactionError> {
        cx.fail_unavailable();
        Ok(HostCreateTransactionResponse::V1(Vec::new()))
    }

    /// Construct a signed extrinsic for a non-product account.
    #[wire(id = 32)]
    async fn host_create_transaction_with_non_product_account(
        &self,
        cx: &CallContext,
        _request: HostCreateTransactionWithNonProductAccountRequest,
    ) -> Result<
        HostCreateTransactionWithNonProductAccountResponse,
        HostCreateTransactionWithNonProductAccountError,
    > {
        cx.fail_unavailable();
        Ok(HostCreateTransactionWithNonProductAccountResponse::V1(
            Vec::new(),
        ))
    }
}
