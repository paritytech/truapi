use parity_scale_codec::{Decode, Encode};

use super::ProductAccountId;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSignPayloadRequest {
    pub account: ProductAccountId,
    pub block_hash: Vec<u8>,
    pub block_number: Vec<u8>,
    pub era: Vec<u8>,
    pub genesis_hash: Vec<u8>,
    pub method: Vec<u8>,
    pub nonce: Vec<u8>,
    pub spec_version: Vec<u8>,
    pub tip: Vec<u8>,
    pub transaction_version: Vec<u8>,
    pub signed_extensions: Vec<String>,
    pub version: u32,
    pub asset_id: Option<Vec<u8>>,
    pub metadata_hash: Option<Vec<u8>>,
    pub mode: Option<u32>,
    pub with_signed_transaction: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RawPayload {
    Bytes { bytes: Vec<u8> },
    Payload { payload: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSignRawRequest {
    pub account: ProductAccountId,
    pub payload: RawPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSignPayloadResponse {
    pub signature: Vec<u8>,
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
    pub signer: String,
    pub payload: RawPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSignPayloadWithLegacyAccountRequest {
    pub signer: String,
    pub payload: HostSignPayloadRequest,
}
