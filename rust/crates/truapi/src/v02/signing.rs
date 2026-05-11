use parity_scale_codec::{Decode, Encode};

use crate::v01::{ProductAccountId, RawPayload};

/// Full Substrate extrinsic signing payload with all fields needed for signature
/// generation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSignPayloadRequest {
    /// Product account that will sign this payload.
    ///
    /// V0.2: replaces the previous `address: String` field per [RFC 0005],
    /// aligning with all other TrUAPI account-related methods.
    ///
    /// [RFC 0005]: https://github.com/paritytech/triangle-js-sdks/pull/82
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

impl TryFrom<crate::v01::HostSignPayloadRequest> for HostSignPayloadRequest {
    type Error = ();

    fn try_from(_value: crate::v01::HostSignPayloadRequest) -> Result<Self, Self::Error> {
        Err(())
    }
}

impl TryFrom<HostSignPayloadRequest> for crate::v01::HostSignPayloadRequest {
    type Error = ();

    fn try_from(_value: HostSignPayloadRequest) -> Result<Self, Self::Error> {
        Err(())
    }
}

/// A raw signing request pairing an account with the payload to sign.
///
/// V0.2: `address` replaced with `account: ProductAccountId` per [RFC 0005];
/// the `data` field was also renamed to `payload`.
///
/// [RFC 0005]: https://github.com/paritytech/triangle-js-sdks/pull/82
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSignRawRequest {
    /// Product account that will sign this payload.
    pub account: ProductAccountId,
    /// The payload to sign.
    pub payload: RawPayload,
}

impl TryFrom<crate::v01::HostSignRawRequest> for HostSignRawRequest {
    type Error = ();

    fn try_from(_value: crate::v01::HostSignRawRequest) -> Result<Self, Self::Error> {
        Err(())
    }
}

impl TryFrom<HostSignRawRequest> for crate::v01::HostSignRawRequest {
    type Error = ();

    fn try_from(_value: HostSignRawRequest) -> Result<Self, Self::Error> {
        Err(())
    }
}

/// Sign a Substrate extrinsic payload with a non-product (legacy) account.
///
/// V0.2: the inner `payload` now uses the V0.2 [`HostSignPayloadRequest`]
/// (with `account: ProductAccountId` instead of `address: String`).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSignPayloadWithLegacyAccountRequest {
    /// Signer address (SS58 or hex) of the legacy account.
    pub signer: String,
    /// The extrinsic payload to sign.
    pub payload: HostSignPayloadRequest,
}

impl TryFrom<crate::v01::HostSignPayloadWithLegacyAccountRequest>
    for HostSignPayloadWithLegacyAccountRequest
{
    type Error = ();

    fn try_from(
        _value: crate::v01::HostSignPayloadWithLegacyAccountRequest,
    ) -> Result<Self, Self::Error> {
        Err(())
    }
}

impl TryFrom<HostSignPayloadWithLegacyAccountRequest>
    for crate::v01::HostSignPayloadWithLegacyAccountRequest
{
    type Error = ();

    fn try_from(
        _value: HostSignPayloadWithLegacyAccountRequest,
    ) -> Result<Self, Self::Error> {
        Err(())
    }
}
