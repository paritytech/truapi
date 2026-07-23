//! Extrinsic signing preimages assembled from pre-encoded payload fields.
//!
//! Signing hosts receive the pre-encoded fields of [`HostSignPayloadData`],
//! so no chain metadata is needed for the standard Substrate extensions. The
//! payload's `signed_extensions` list supplies runtime order; extensions whose
//! bytes cannot be represented by [`HostSignPayloadData`] are rejected instead
//! of being silently omitted. Preimages longer than 256 bytes are BLAKE2b-256
//! hashed before signing (standard Substrate signed-payload rule).

use parity_scale_codec::Encode;
use sp_crypto_hashing::blake2_256;
use truapi::latest::{HostSignPayloadData, TxPayloadExtension};

/// Preimages longer than this are hashed before signing.
const MAX_SIGNED_PREIMAGE_LEN: usize = 256;

/// Signed extension contributing the `asset_id` extra after `tip`.
const CHARGE_ASSET_TX_PAYMENT: &str = "ChargeAssetTxPayment";
/// Signed extension contributing the `mode` extra and `metadata_hash` implicit.
const CHECK_METADATA_HASH: &str = "CheckMetadataHash";

/// Standard signed extensions with no extra or implicit bytes.
const EMPTY_EXTENSIONS: &[&str] = &["CheckNonZeroSender", "CheckWeight"];

/// Encode the standard signed extensions in the order declared by the target
/// runtime. Unknown extensions are rejected because this wire payload has no
/// field carrying their extra or implicit bytes.
pub(crate) fn extrinsic_payload_extensions(
    payload: &HostSignPayloadData,
) -> Result<Vec<TxPayloadExtension>, String> {
    payload
        .signed_extensions
        .iter()
        .map(|id| {
            let (extra, additional_signed) = match id.as_str() {
                id if EMPTY_EXTENSIONS.contains(&id) => (Vec::new(), Vec::new()),
                "CheckSpecVersion" => (Vec::new(), payload.spec_version.clone()),
                "CheckTxVersion" => (Vec::new(), payload.transaction_version.clone()),
                "CheckGenesis" => (Vec::new(), payload.genesis_hash.clone()),
                "CheckMortality" => (payload.era.clone(), payload.block_hash.clone()),
                "CheckNonce" => (payload.nonce.clone(), Vec::new()),
                "ChargeTransactionPayment" => (payload.tip.clone(), Vec::new()),
                CHARGE_ASSET_TX_PAYMENT => {
                    let mut extra = payload.tip.clone();
                    match &payload.asset_id {
                        Some(asset_id) => extra.extend_from_slice(asset_id),
                        None => None::<()>.encode_to(&mut extra),
                    }
                    (extra, Vec::new())
                }
                CHECK_METADATA_HASH => {
                    let mode = payload.mode.unwrap_or(0);
                    let mode = u8::try_from(mode).map_err(|_| {
                        format!("CheckMetadataHash mode {mode} does not fit in a u8")
                    })?;
                    let metadata_hash = payload
                        .metadata_hash
                        .as_deref()
                        .map(|hash| {
                            <[u8; 32]>::try_from(hash).map_err(|_| {
                                format!(
                                    "CheckMetadataHash metadata hash is {} bytes, expected 32",
                                    hash.len()
                                )
                            })
                        })
                        .transpose()?;
                    (mode.encode(), metadata_hash.encode())
                }
                unsupported => {
                    return Err(format!(
                        "unsupported signed extension `{unsupported}`: its encoded fields are not \
                         present in HostSignPayloadData"
                    ));
                }
            };
            Ok(TxPayloadExtension {
                id: id.clone(),
                extra,
                additional_signed,
            })
        })
        .collect()
}

/// Signing preimage for an extrinsic payload:
/// `method ++ Σextension.extra ++ Σextension.additional_signed`, with both
/// extension sequences following the declared runtime order.
pub fn extrinsic_payload_preimage(payload: &HostSignPayloadData) -> Result<Vec<u8>, String> {
    let extensions = extrinsic_payload_extensions(payload)?;

    let mut preimage = Vec::new();
    preimage.extend_from_slice(&payload.method);
    for extension in &extensions {
        preimage.extend_from_slice(&extension.extra);
    }
    for extension in &extensions {
        preimage.extend_from_slice(&extension.additional_signed);
    }
    Ok(hash_large_preimage(preimage))
}

fn hash_large_preimage(preimage: Vec<u8>) -> Vec<u8> {
    if preimage.len() > MAX_SIGNED_PREIMAGE_LEN {
        // This is the same primitive and threshold used by Subxt's
        // `frame_decode::extrinsics::encode_v4_signer_payload`. That builder
        // itself is not applicable here because this API receives pre-encoded
        // fields without runtime metadata or structured call arguments.
        blake2_256(&preimage).to_vec()
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
            signed_extensions: vec![
                "CheckSpecVersion".to_string(),
                "CheckTxVersion".to_string(),
                "CheckGenesis".to_string(),
                "CheckMortality".to_string(),
                "CheckNonce".to_string(),
                "ChargeTransactionPayment".to_string(),
            ],
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
            "CheckSpecVersion".to_string(),
            "CheckTxVersion".to_string(),
            "CheckGenesis".to_string(),
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
            "CheckSpecVersion".to_string(),
            "CheckTxVersion".to_string(),
            "CheckGenesis".to_string(),
            "CheckMortality".to_string(),
            "CheckNonce".to_string(),
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
    fn payload_preimage_follows_noncanonical_runtime_order() {
        let mut payload = payload();
        payload.signed_extensions = vec![
            "CheckNonce".to_string(),
            "CheckMortality".to_string(),
            "CheckGenesis".to_string(),
            "CheckSpecVersion".to_string(),
        ];

        assert_eq!(
            extrinsic_payload_preimage(&payload).unwrap(),
            vec![
                0x4D, 0x4E, 0xE1, // method, nonce extra, era extra
                0xB1, 0xB2, // mortality implicit
                0x61, 0x62, // genesis implicit
                0x51, // spec-version implicit
            ]
        );
    }

    #[test]
    fn payload_preimage_rejects_unsupported_extension() {
        let mut payload = payload();
        payload.signed_extensions = vec!["CustomExtension".to_string()];

        assert_eq!(
            extrinsic_payload_preimage(&payload),
            Err(
                "unsupported signed extension `CustomExtension`: its encoded fields are not \
                 present in HostSignPayloadData"
                    .to_string()
            )
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
    fn payload_preimage_rejects_invalid_metadata_hash_length() {
        let mut payload = payload();
        payload.signed_extensions = vec!["CheckMetadataHash".to_string()];
        payload.metadata_hash = Some(vec![0xBB; 31]);

        assert_eq!(
            extrinsic_payload_preimage(&payload),
            Err("CheckMetadataHash metadata hash is 31 bytes, expected 32".to_string())
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
