use parity_scale_codec::{Decode, Encode};

use super::ProductAccountId;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TxPayloadExtensionV1 {
    pub id: String,
    pub extra: Vec<u8>,
    pub additional_signed: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TxPayloadContextV1 {
    pub metadata: Vec<u8>,
    pub token_symbol: String,
    pub token_decimals: u32,
    pub best_block_height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TxPayloadV1 {
    pub signer: Option<String>,
    pub call_data: Vec<u8>,
    pub extensions: Vec<TxPayloadExtensionV1>,
    pub tx_ext_version: u8,
    pub context: TxPayloadContextV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum VersionedTxPayload {
    V1(TxPayloadV1),
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCreateTransactionRequest {
    pub product_account_id: ProductAccountId,
    pub payload: VersionedTxPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostCreateTransactionError {
    FailedToDecode,
    Rejected,
    NotSupported { reason: String },
    PermissionDenied,
    Unknown { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCreateTransactionResponse {
    pub transaction: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCreateTransactionWithLegacyAccountRequest {
    pub payload: VersionedTxPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCreateTransactionWithLegacyAccountResponse {
    pub transaction: Vec<u8>,
}
