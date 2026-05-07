//! Unified [`Signing`] trait.

use crate::versioned::signing::{
    HostCreateTransactionError, HostCreateTransactionRequest, HostCreateTransactionResponse,
    HostCreateTransactionWithLegacyAccountError, HostCreateTransactionWithLegacyAccountRequest,
    HostCreateTransactionWithLegacyAccountResponse, HostSignPayloadError, HostSignPayloadRequest,
    HostSignPayloadResponse, HostSignRawError, HostSignRawRequest, HostSignRawResponse,
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
    ///
    /// ```truapi-playground-request
    /// { "productAccountId": { "dotNsIdentifier": "truapi-playground.dot", "derivationIndex": 0 }, "payload": { "tag": "V1", "value": { "signer": null, "callData": "0x0000", "extensions": [], "txExtVersion": 0, "context": { "metadata": "0x", "tokenSymbol": "DOT", "tokenDecimals": 10, "bestBlockHeight": 0 } } } }
    /// ```
    #[wire(id = 30)]
    async fn host_create_transaction(
        &self,
        _cx: &CallContext,
        _request: HostCreateTransactionRequest,
    ) -> Result<HostCreateTransactionResponse, CallError<HostCreateTransactionError>> {
        Err(CallError::unavailable())
    }

    /// Construct a signed extrinsic for a non-product account.
    ///
    /// ```truapi-playground-request
    /// { "payload": { "tag": "V1", "value": { "signer": null, "callData": "0x0000", "extensions": [], "txExtVersion": 0, "context": { "metadata": "0x", "tokenSymbol": "DOT", "tokenDecimals": 10, "bestBlockHeight": 0 } } } }
    /// ```
    #[wire(id = 32)]
    async fn host_create_transaction_with_legacy_account(
        &self,
        _cx: &CallContext,
        _request: HostCreateTransactionWithLegacyAccountRequest,
    ) -> Result<
        HostCreateTransactionWithLegacyAccountResponse,
        CallError<HostCreateTransactionWithLegacyAccountError>,
    > {
        Err(CallError::unavailable())
    }

    /// Sign raw bytes or a message.
    ///
    /// ```truapi-playground-request
    /// { "account": { "dotNsIdentifier": "truapi-playground.dot", "derivationIndex": 0 }, "data": { "tag": "Bytes", "value": { "bytes": "0x48656c6c6f" } } }
    /// ```
    #[wire(id = 114)]
    async fn host_sign_raw(
        &self,
        _cx: &CallContext,
        _request: HostSignRawRequest,
    ) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
        Err(CallError::unavailable())
    }

    /// Sign a Substrate extrinsic payload.
    ///
    /// ```truapi-playground-request
    /// { "account": { "dotNsIdentifier": "truapi-playground.dot", "derivationIndex": 0 }, "blockHash": "0x0000000000000000000000000000000000000000000000000000000000000000", "blockNumber": "0x00000000", "era": "0x00", "genesisHash": "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2", "method": "0x00000000", "nonce": "0x00000000", "signedExtensions": [], "specVersion": "0x00000000", "tip": "0x00000000000000000000000000000000", "transactionVersion": "0x00000000", "version": 4 }
    /// ```
    #[wire(id = 116)]
    async fn host_sign_payload(
        &self,
        _cx: &CallContext,
        _request: HostSignPayloadRequest,
    ) -> Result<HostSignPayloadResponse, CallError<HostSignPayloadError>> {
        Err(CallError::unavailable())
    }
}
