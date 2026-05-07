use parity_scale_codec::{Decode, Encode};

use super::{GenesisHash, Hex};

/// Full Substrate extrinsic signing payload with all fields needed for signature
/// generation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct SigningPayload {
    /// Signer address (SS58 or hex).
    pub address: String,
    /// Reference block hash.
    pub block_hash: Hex,
    /// Reference block number.
    pub block_number: Hex,
    /// Mortality era encoding.
    pub era: Hex,
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// SCALE-encoded call data.
    pub method: Hex,
    /// Account nonce.
    pub nonce: Hex,
    /// Runtime spec version.
    pub spec_version: Hex,
    /// Transaction tip.
    pub tip: Hex,
    /// Transaction format version.
    pub transaction_version: Hex,
    /// Extension identifiers.
    pub signed_extensions: Vec<String>,
    /// Extrinsic version.
    pub version: u32,
    /// For multi-asset tips.
    pub asset_id: Option<Hex>,
    /// CheckMetadataHash extension.
    pub metadata_hash: Option<Hex>,
    /// Metadata mode.
    pub mode: Option<u32>,
    /// Request signed transaction back.
    pub with_signed_transaction: Option<bool>,
}

/// Raw data to sign -- either binary bytes or a string message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RawPayload {
    /// Raw binary data to sign.
    Bytes(Vec<u8>),
    /// String message to sign.
    Payload(String),
}

/// A raw signing request pairing an address with raw data.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct SigningRawPayload {
    /// Signer address.
    pub address: String,
    /// The data to sign.
    pub data: RawPayload,
}

/// Result of a signing operation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct SigningResult {
    /// The cryptographic signature.
    pub signature: Hex,
    /// Full signed transaction, if requested.
    pub signed_transaction: Option<Hex>,
}

/// Signing operation error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum SigningError {
    /// Payload could not be deserialized.
    FailedToDecode,
    /// User rejected signing.
    Rejected,
    /// Not authenticated.
    PermissionDenied,
    /// Catch-all.
    Unknown { reason: String },
}

pub type HostSignPayloadRequest = SigningPayload;
pub type HostSignPayloadResponse = SigningResult;
pub type HostSignPayloadError = SigningError;
pub type HostSignRawRequest = SigningRawPayload;
pub type HostSignRawResponse = SigningResult;
pub type HostSignRawError = SigningError;
pub type HostCreateTransactionResponse = Hex;
pub type HostCreateTransactionWithNonProductAccountRequest = super::VersionedTxPayload;
pub type HostCreateTransactionWithNonProductAccountResponse = Hex;
