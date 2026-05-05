use super::*;
use parity_scale_codec::{Decode, Encode};

/// Full Substrate extrinsic signing payload with all fields needed for signature
/// generation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct SigningPayload {
    /// Product account that will sign this payload.
    ///
    /// V0.2: replaces the previous `address: String` field per [RFC 0005],
    /// aligning with all other TrUAPI account-related methods.
    ///
    /// [RFC 0005]: https://github.com/paritytech/triangle-js-sdks/pull/82
    pub account: ProductAccountId,
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

/// Raw data to sign — either binary bytes or a string message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RawPayload {
    /// Raw binary data to sign.
    Bytes(Vec<u8>),
    /// String message to sign.
    Payload(String),
}

/// A raw signing request pairing an account with raw data.
///
/// V0.2: `address` replaced with `account: ProductAccountId` per [RFC 0005].
///
/// [RFC 0005]: https://github.com/paritytech/triangle-js-sdks/pull/82
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct SigningRawPayload {
    /// Product account that will sign this data.
    pub account: ProductAccountId,
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
