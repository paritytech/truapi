use parity_scale_codec::{Decode, Encode};

use super::{Hex, ProductAccountId};

/// A signed extension for a transaction payload.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct TxPayloadExtensionV1 {
    /// Extension name (e.g., `"CheckSpecVersion"`).
    pub id: String,
    /// SCALE-encoded extra data (in extrinsic body).
    pub extra: Hex,
    /// SCALE-encoded implicit data (signed, not in body).
    pub additional_signed: Hex,
}

/// Context information for transaction construction.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct TxPayloadContextV1 {
    /// `RuntimeMetadataPrefixed` blob (SCALE).
    pub metadata: Hex,
    /// Native token symbol.
    pub token_symbol: String,
    /// Native token decimals.
    pub token_decimals: u32,
    /// Highest known block number.
    pub best_block_height: u32,
}

/// Version 1 transaction payload with all data needed to construct a signed
/// extrinsic.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct TxPayloadV1 {
    /// Signer hint (address/name), `None` = host picks.
    pub signer: Option<String>,
    /// SCALE-encoded Call data.
    pub call_data: Hex,
    /// Signed extensions.
    pub extensions: Vec<TxPayloadExtensionV1>,
    /// 0 for Extrinsic V4, any for V5.
    pub tx_ext_version: u8,
    /// Transaction context.
    pub context: TxPayloadContextV1,
}

/// Versioned transaction payload envelope.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum VersionedTxPayload {
    /// Version 1 payload.
    V1(TxPayloadV1),
}

/// Request to create a transaction for a product account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct CreateTransactionRequest {
    /// Product account that will sign the transaction.
    pub product_account_id: ProductAccountId,
    /// Versioned transaction payload.
    pub payload: VersionedTxPayload,
}

/// Transaction creation error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum CreateTransactionError {
    /// Payload could not be deserialized.
    FailedToDecode,
    /// User rejected.
    Rejected,
    /// Unsupported payload version or extension.
    NotSupported(String),
    /// Not authenticated.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}

pub type HostCreateTransactionRequest = CreateTransactionRequest;
pub type HostCreateTransactionError = CreateTransactionError;
pub type HostCreateTransactionWithNonProductAccountError = CreateTransactionError;
