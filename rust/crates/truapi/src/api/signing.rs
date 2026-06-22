//! Unified [`Signing`] trait.

use crate::versioned::signing::{
    HostCreateTransactionError, HostCreateTransactionRequest, HostCreateTransactionResponse,
    HostCreateTransactionWithLegacyAccountError, HostCreateTransactionWithLegacyAccountRequest,
    HostCreateTransactionWithLegacyAccountResponse,
};
use crate::versioned::signing::{
    HostSignPayloadError, HostSignPayloadRequest, HostSignPayloadResponse,
    HostSignPayloadWithLegacyAccountError, HostSignPayloadWithLegacyAccountRequest,
    HostSignPayloadWithLegacyAccountResponse, HostSignRawError, HostSignRawRequest,
    HostSignRawResponse, HostSignRawWithLegacyAccountError, HostSignRawWithLegacyAccountRequest,
    HostSignRawWithLegacyAccountResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Signing operations.
pub trait Signing: Send + Sync {
    /// Construct a signed transaction for a product account.
    ///
    /// # Permissions
    ///
    /// - **auth**: required
    /// - **prompt**: signing confirmation
    /// - **denial_error**: Rejected
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// const result = await truapi.signing.createTransaction({
    ///   signer: {
    ///     dotNsIdentifier: "truapi-playground.dot",
    ///     derivationIndex: 0,
    ///   },
    ///   genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    ///   callData: "0x0000",
    ///   extensions: [],
    ///   txExtVersion: 0,
    /// });
    /// assert(result.isOk(), "createTransaction failed:", result);
    /// console.log("transaction created:", result.value);
    /// ```
    #[wire(request_id = 30)]
    async fn create_transaction(
        &self,
        _cx: &CallContext,
        _request: HostCreateTransactionRequest,
    ) -> Result<HostCreateTransactionResponse, CallError<HostCreateTransactionError>> {
        Err(CallError::unavailable())
    }

    /// Construct a signed transaction for a non-product (legacy) account.
    ///
    /// # Permissions
    ///
    /// - **auth**: required
    /// - **prompt**: signing confirmation
    /// - **denial_error**: Rejected
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// const signerResult = await accountIdForDotNsUsername();
    /// assert(signerResult.isOk(), "accountIdForDotNsUsername failed:", signerResult);
    /// console.log("fetched user account:", signerResult.value);
    ///
    /// const result = await truapi.signing.createTransactionWithLegacyAccount({
    ///   signer: signerResult.value,
    ///   genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    ///   callData: "0x0000",
    ///   extensions: [],
    ///   txExtVersion: 0,
    /// });
    /// assert(result.isOk(), "createTransactionWithLegacyAccount failed:", result);
    /// console.log("transaction created:", result.value);
    /// ```
    #[wire(request_id = 32)]
    async fn create_transaction_with_legacy_account(
        &self,
        _cx: &CallContext,
        _request: HostCreateTransactionWithLegacyAccountRequest,
    ) -> Result<
        HostCreateTransactionWithLegacyAccountResponse,
        CallError<HostCreateTransactionWithLegacyAccountError>,
    > {
        Err(CallError::unavailable())
    }

    /// Sign raw bytes with a non-product account.
    ///
    /// # Permissions
    ///
    /// - **auth**: required
    /// - **prompt**: signing confirmation
    /// - **denial_error**: Rejected
    ///
    /// ```ts
    /// const result = await truapi.signing.signRawWithLegacyAccount({
    ///   signer: "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    ///   payload: {
    ///     tag: "Bytes",
    ///     value: { bytes: "0x48656c6c6f" },
    ///   },
    /// });
    /// assert(result.isOk(), "signRawWithLegacyAccount failed:", result);
    /// console.log("raw bytes signed:", result.value);
    /// ```
    #[wire(request_id = 34)]
    async fn sign_raw_with_legacy_account(
        &self,
        _cx: &CallContext,
        _request: HostSignRawWithLegacyAccountRequest,
    ) -> Result<HostSignRawWithLegacyAccountResponse, CallError<HostSignRawWithLegacyAccountError>>
    {
        Err(CallError::unavailable())
    }

    /// Sign an extrinsic payload with a non-product account.
    ///
    /// # Permissions
    ///
    /// - **auth**: required
    /// - **prompt**: signing confirmation
    /// - **denial_error**: Rejected
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// const result = await truapi.signing.signPayloadWithLegacyAccount({
    ///   signer: "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    ///   payload: {
    ///     blockHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     blockNumber: "0x00000000",
    ///     era: "0x00",
    ///     genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    ///     method: "0x0000",
    ///     nonce: "0x00000000",
    ///     signedExtensions: [],
    ///     specVersion: "0x00000000",
    ///     tip: "0x00000000000000000000000000000000",
    ///     transactionVersion: "0x00000000",
    ///     version: 4,
    ///   },
    /// });
    /// assert(result.isOk(), "signPayloadWithLegacyAccount failed:", result);
    /// console.log("payload signed:", result.value);
    /// ```
    #[wire(request_id = 36)]
    async fn sign_payload_with_legacy_account(
        &self,
        _cx: &CallContext,
        _request: HostSignPayloadWithLegacyAccountRequest,
    ) -> Result<
        HostSignPayloadWithLegacyAccountResponse,
        CallError<HostSignPayloadWithLegacyAccountError>,
    > {
        Err(CallError::unavailable())
    }

    /// Sign raw bytes or a message.
    ///
    /// # Permissions
    ///
    /// - **auth**: required
    /// - **prompt**: signing confirmation
    /// - **denial_error**: Rejected
    ///
    /// ```ts
    /// const result = await truapi.signing.signRaw({
    ///   account: { dotNsIdentifier: "truapi-playground.dot", derivationIndex: 0 },
    ///   payload: {
    ///     tag: "Bytes",
    ///     value: {
    ///       bytes: "0x48656c6c6f2c20776f726c6421",
    ///     },
    ///   },
    /// });
    /// assert(result.isOk(), "signRaw failed:", result);
    /// console.log("raw bytes signed:", result.value);
    /// ```
    #[wire(request_id = 114)]
    async fn sign_raw(
        &self,
        _cx: &CallContext,
        _request: HostSignRawRequest,
    ) -> Result<HostSignRawResponse, CallError<HostSignRawError>> {
        Err(CallError::unavailable())
    }

    /// Sign an extrinsic payload.
    ///
    /// # Permissions
    ///
    /// - **auth**: required
    /// - **prompt**: signing confirmation
    /// - **denial_error**: Rejected
    ///
    /// ```ts
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// const result = await truapi.signing.signPayload({
    ///   account: { dotNsIdentifier: "truapi-playground.dot", derivationIndex: 0 },
    ///   payload: {
    ///     blockHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     blockNumber: "0x00000000",
    ///     era: "0x00",
    ///     genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    ///     method: "0x00003448656c6c6f2c20776f726c6421",
    ///     nonce: "0x00000000",
    ///     signedExtensions: [],
    ///     specVersion: "0x00000000",
    ///     tip: "0x00000000000000000000000000000000",
    ///     transactionVersion: "0x00000000",
    ///     version: 4,
    ///   },
    /// });
    /// assert(result.isOk(), "signPayload failed:", result);
    /// console.log("payload signed:", result.value);
    /// ```
    #[wire(request_id = 116)]
    async fn sign_payload(
        &self,
        _cx: &CallContext,
        _request: HostSignPayloadRequest,
    ) -> Result<HostSignPayloadResponse, CallError<HostSignPayloadError>> {
        Err(CallError::unavailable())
    }
}
