use parity_scale_codec::{Decode, Encode};

use super::ProductAccountId;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TxPayloadExtensionV1 {
    /// Extension name (e.g., `"CheckSpecVersion"`).
    pub id: String,
    /// SCALE-encoded extra data (in extrinsic body).
    pub extra: Vec<u8>,
    /// SCALE-encoded implicit data (signed, not in body).
    pub additional_signed: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TxPayloadContextV1 {
    /// `RuntimeMetadataPrefixed` blob (SCALE).
    pub metadata: Vec<u8>,
    /// Native token symbol.
    pub token_symbol: String,
    /// Native token decimals.
    pub token_decimals: u32,
    /// Highest known block number.
    pub best_block_height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TxPayloadV1 {
    /// Signer hint (address/name), `None` = host picks.
    pub signer: Option<String>,
    /// SCALE-encoded Call data.
    pub call_data: Vec<u8>,
    /// Signed extensions.
    pub extensions: Vec<TxPayloadExtensionV1>,
    /// 0 for Extrinsic V4, any for V5.
    pub tx_ext_version: u8,
    /// Transaction context.
    pub context: TxPayloadContextV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum VersionedTxPayload {
    V1(TxPayloadV1),
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCreateTransactionRequest {
    /// Product account that will sign the transaction.
    pub product_account_id: ProductAccountId,
    /// Versioned transaction payload.
    pub payload: VersionedTxPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostCreateTransactionError {
    FailedToDecode,
    Rejected,
    NotSupported {
        /// Unsupported payload or extension reason.
        reason: String,
    },
    PermissionDenied,
    Unknown {
        reason: String,
    },
}
