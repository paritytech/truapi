//! Extrinsic signing preimages assembled from pre-encoded payload fields.
//!
//! Signing hosts receive the pre-encoded fields of [`HostSignPayloadData`],
//! so no chain metadata is needed: the preimage is the polkadot-js
//! `ExtrinsicPayloadV4` byte layout. The optional `ChargeAssetTxPayment`
//! (`asset_id` after `tip`) and `CheckMetadataHash` (`mode` extra plus the
//! `Option`-encoded `metadata_hash` implicit) fields participate when the
//! payload's signed-extension list names them. Extensions outside that set
//! are assumed to carry no extra or implicit bytes, matching polkadot-js
//! without user extensions. Preimages longer than 256 bytes are BLAKE2b-256
//! hashed before signing (standard Substrate signed-payload rule).

use truapi::latest::HostSignPayloadData;

/// Preimages longer than this are hashed before signing.
const MAX_SIGNED_PREIMAGE_LEN: usize = 256;

/// Signed extension contributing the `asset_id` extra after `tip`.
const CHARGE_ASSET_TX_PAYMENT: &str = "ChargeAssetTxPayment";
/// Signed extension contributing the `mode` extra and `metadata_hash` implicit.
const CHECK_METADATA_HASH: &str = "CheckMetadataHash";

/// Signing preimage for an extrinsic payload, in the polkadot-js
/// `ExtrinsicPayloadV4` field order: `method ++ era ++ nonce ++ tip ++
/// [asset_id] ++ [mode] ++ spec_version ++ transaction_version ++
/// genesis_hash ++ block_hash ++ [metadata_hash]`.
pub fn extrinsic_payload_preimage(payload: &HostSignPayloadData) -> Result<Vec<u8>, String> {
    let has_extension = |id: &str| payload.signed_extensions.iter().any(|ext| ext == id);
    let charge_asset = has_extension(CHARGE_ASSET_TX_PAYMENT) || payload.asset_id.is_some();
    let check_metadata_hash = has_extension(CHECK_METADATA_HASH) || payload.metadata_hash.is_some();

    let mut preimage = Vec::new();
    preimage.extend_from_slice(&payload.method);
    preimage.extend_from_slice(&payload.era);
    preimage.extend_from_slice(&payload.nonce);
    preimage.extend_from_slice(&payload.tip);
    if charge_asset {
        // The wire carries the chain's `TAssetConversion` encoding (itself
        // option-typed on asset chains); an absent field means `None`.
        match &payload.asset_id {
            Some(asset_id) => preimage.extend_from_slice(asset_id),
            None => preimage.push(0),
        }
    }
    if check_metadata_hash {
        let mode = payload.mode.unwrap_or(0);
        let mode = u8::try_from(mode)
            .map_err(|_| format!("CheckMetadataHash mode {mode} does not fit in a u8"))?;
        preimage.push(mode);
    }
    preimage.extend_from_slice(&payload.spec_version);
    preimage.extend_from_slice(&payload.transaction_version);
    preimage.extend_from_slice(&payload.genesis_hash);
    preimage.extend_from_slice(&payload.block_hash);
    if check_metadata_hash {
        // `Option<[u8; 32]>` implicit; the wire carries the raw hash.
        match &payload.metadata_hash {
            Some(hash) => {
                preimage.push(1);
                preimage.extend_from_slice(hash);
            }
            None => preimage.push(0),
        }
    }
    Ok(hash_large_preimage(preimage))
}

fn hash_large_preimage(preimage: Vec<u8>) -> Vec<u8> {
    if preimage.len() > MAX_SIGNED_PREIMAGE_LEN {
        blake2b_simd::Params::new()
            .hash_length(32)
            .hash(&preimage)
            .as_bytes()
            .to_vec()
    } else {
        preimage
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> HostSignPayloadData {
        HostSignPayloadData {
            block_hash: vec![0xB1, 0xB2],
            block_number: vec![0xFF],
            era: vec![0xE1],
            genesis_hash: vec![0x61, 0x62],
            method: vec![0x4D],
            nonce: vec![0x4E],
            spec_version: vec![0x51],
            tip: vec![0x54],
            transaction_version: vec![0x56],
            signed_extensions: vec![],
            version: 4,
            asset_id: None,
            metadata_hash: None,
            mode: None,
            with_signed_transaction: None,
        }
    }

    #[test]
    fn payload_preimage_uses_extrinsic_payload_v4_field_order() {
        // method, era, nonce, tip, spec_version, transaction_version,
        // genesis_hash, block_hash. block_number is not part of the preimage.
        assert_eq!(
            extrinsic_payload_preimage(&payload()).unwrap(),
            vec![0x4D, 0xE1, 0x4E, 0x54, 0x51, 0x56, 0x61, 0x62, 0xB1, 0xB2]
        );
    }

    #[test]
    fn payload_preimage_places_asset_id_after_tip_and_metadata_hash_last() {
        let mut payload = payload();
        payload.signed_extensions = vec![
            "CheckMortality".to_string(),
            "CheckNonce".to_string(),
            "ChargeAssetTxPayment".to_string(),
            "CheckMetadataHash".to_string(),
        ];
        payload.asset_id = Some(vec![0x01, 0xAA]);
        payload.mode = Some(1);
        payload.metadata_hash = Some(vec![0xBB; 32]);

        let mut expected = vec![
            0x4D, 0xE1, 0x4E, 0x54, // method, era, nonce, tip
            0x01, 0xAA, // asset_id (TAssetConversion bytes)
            0x01, // mode
            0x51, 0x56, 0x61, 0x62, 0xB1, 0xB2, // spec, tx, genesis, block
            0x01, // Some(metadata_hash)
        ];
        expected.extend_from_slice(&[0xBB; 32]);
        assert_eq!(extrinsic_payload_preimage(&payload).unwrap(), expected);
    }

    #[test]
    fn payload_preimage_defaults_listed_extensions_to_disabled() {
        // A chain that has the extensions while the payload leaves them unset
        // still signs their default encodings: assetId None, mode 0,
        // metadata_hash None.
        let mut payload = payload();
        payload.signed_extensions = vec![
            "ChargeAssetTxPayment".to_string(),
            "CheckMetadataHash".to_string(),
        ];
        payload.mode = Some(0);

        assert_eq!(
            extrinsic_payload_preimage(&payload).unwrap(),
            vec![
                0x4D, 0xE1, 0x4E, 0x54, // method, era, nonce, tip
                0x00, // asset_id None
                0x00, // mode 0
                0x51, 0x56, 0x61, 0x62, 0xB1, 0xB2, // spec, tx, genesis, block
                0x00, // metadata_hash None
            ]
        );
    }

    #[test]
    fn payload_preimage_ignores_mode_without_check_metadata_hash() {
        // polkadot-js always emits `mode: 0` in the payload JSON, even for
        // chains without CheckMetadataHash; the extension list decides.
        let mut payload = payload();
        payload.mode = Some(0);

        assert_eq!(
            extrinsic_payload_preimage(&payload).unwrap(),
            vec![0x4D, 0xE1, 0x4E, 0x54, 0x51, 0x56, 0x61, 0x62, 0xB1, 0xB2]
        );
    }

    #[test]
    fn payload_preimage_rejects_out_of_range_mode() {
        let mut payload = payload();
        payload.signed_extensions = vec!["CheckMetadataHash".to_string()];
        payload.mode = Some(256);

        assert_eq!(
            extrinsic_payload_preimage(&payload),
            Err("CheckMetadataHash mode 256 does not fit in a u8".to_string())
        );
    }

    #[test]
    fn long_preimages_are_blake2b_hashed() {
        let mut payload = payload();
        payload.method = vec![0x4D; 300];

        let preimage = extrinsic_payload_preimage(&payload).unwrap();

        assert_eq!(preimage.len(), 32);
        let mut raw = vec![0x4D; 300];
        raw.extend_from_slice(&[0xE1, 0x4E, 0x54, 0x51, 0x56, 0x61, 0x62, 0xB1, 0xB2]);
        assert_eq!(
            preimage,
            blake2b_simd::Params::new()
                .hash_length(32)
                .hash(&raw)
                .as_bytes()
                .to_vec()
        );
    }
}
