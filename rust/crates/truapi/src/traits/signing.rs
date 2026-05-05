//! Unified [`Signing`] trait.

use crate::v02::{CreateTransactionError, SigningError, SigningResult};
use crate::versioned::signing::{
    HostCreateTransactionRequest, HostCreateTransactionResponse,
    HostCreateTransactionWithNonProductAccountRequest,
    HostCreateTransactionWithNonProductAccountResponse, HostSignPayloadRequest,
    HostSignPayloadResponse, HostSignRawRequest, HostSignRawResponse,
};
use crate::wire;
use crate::CallContext;

/// Signing and transaction construction. Unified counterpart of
/// [`crate::v02::Signing`].
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
    ) -> Result<HostSignPayloadResponse, SigningError> {
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
    ) -> Result<HostSignRawResponse, SigningError> {
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
    ) -> Result<HostCreateTransactionResponse, CreateTransactionError> {
        cx.fail_unavailable();
        Ok(HostCreateTransactionResponse::V2(Vec::new()))
    }

    /// Construct a signed extrinsic for a non-product account.
    #[wire(id = 32)]
    async fn host_create_transaction_with_non_product_account(
        &self,
        cx: &CallContext,
        _request: HostCreateTransactionWithNonProductAccountRequest,
    ) -> Result<HostCreateTransactionWithNonProductAccountResponse, CreateTransactionError> {
        cx.fail_unavailable();
        Ok(HostCreateTransactionWithNonProductAccountResponse::V2(
            Vec::new(),
        ))
    }
}
