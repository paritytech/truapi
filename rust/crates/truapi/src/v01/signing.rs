use parity_scale_codec::{Decode, Encode};

use super::ProductAccountId;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSignPayloadRequest {
    /// Product account that will sign this payload.
    pub account: ProductAccountId,
    /// Reference block hash.
    pub block_hash: Vec<u8>,
    /// Reference block number.
    pub block_number: Vec<u8>,
    /// Mortality era encoding.
    pub era: Vec<u8>,
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// SCALE-encoded call data.
    pub method: Vec<u8>,
    /// Account nonce.
    pub nonce: Vec<u8>,
    /// Runtime spec version.
    pub spec_version: Vec<u8>,
    /// Transaction tip.
    pub tip: Vec<u8>,
    /// Transaction format version.
    pub transaction_version: Vec<u8>,
    /// Extension identifiers.
    pub signed_extensions: Vec<String>,
    /// Extrinsic version.
    pub version: u32,
    /// For multi-asset tips.
    pub asset_id: Option<Vec<u8>>,
    /// CheckMetadataHash extension.
    pub metadata_hash: Option<Vec<u8>>,
    /// Metadata mode.
    pub mode: Option<u32>,
    /// Request signed transaction back.
    pub with_signed_transaction: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RawPayload {
    Bytes {
        /// Raw binary payload bytes.
        bytes: Vec<u8>,
    },
    Payload {
        /// String payload to sign.
        payload: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSignRawRequest {
    /// Product account that will sign this payload.
    pub account: ProductAccountId,
    /// The payload to sign.
    pub payload: RawPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSignPayloadResponse {
    /// The cryptographic signature.
    pub signature: Vec<u8>,
    /// Full signed transaction, if requested.
    pub signed_transaction: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostSignPayloadError {
    FailedToDecode,
    Rejected,
    PermissionDenied,
    Unknown { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSignRawWithLegacyAccountRequest {
    /// Signer address (SS58 or hex) of the legacy account.
    pub signer: String,
    /// The data to sign.
    pub payload: RawPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSignPayloadWithLegacyAccountRequest {
    /// Signer address (SS58 or hex) of the legacy account.
    pub signer: String,
    /// The extrinsic payload to sign.
    pub payload: HostSignPayloadRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCreateTransactionResponse {
    /// SCALE-encoded signed transaction.
    pub transaction: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCreateTransactionWithLegacyAccountRequest {
    /// Versioned transaction payload to sign.
    pub payload: super::VersionedTxPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostCreateTransactionWithLegacyAccountResponse {
    /// SCALE-encoded signed transaction.
    pub transaction: Vec<u8>,
}
