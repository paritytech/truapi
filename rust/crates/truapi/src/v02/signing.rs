use parity_scale_codec::{Decode, Encode};

use crate::v01::{GenesisHash, Hex, ProductAccountId, RawPayload};

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

impl TryFrom<crate::v01::SigningPayload> for SigningPayload {
    type Error = ();

    fn try_from(_value: crate::v01::SigningPayload) -> Result<Self, Self::Error> {
        Err(())
    }
}

impl TryFrom<SigningPayload> for crate::v01::SigningPayload {
    type Error = ();

    fn try_from(_value: SigningPayload) -> Result<Self, Self::Error> {
        Err(())
    }
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

impl TryFrom<crate::v01::SigningRawPayload> for SigningRawPayload {
    type Error = ();

    fn try_from(_value: crate::v01::SigningRawPayload) -> Result<Self, Self::Error> {
        Err(())
    }
}

impl TryFrom<SigningRawPayload> for crate::v01::SigningRawPayload {
    type Error = ();

    fn try_from(_value: SigningRawPayload) -> Result<Self, Self::Error> {
        Err(())
    }
}
