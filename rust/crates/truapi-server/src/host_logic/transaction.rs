//! Extrinsic signing preimages and v4 signed-extrinsic assembly.
//!
//! Signing hosts receive pre-encoded payload fields (`HostSignPayloadData`)
//! or pre-encoded transaction extensions (`TxPayloadExtension`), so no chain
//! metadata is needed: the preimage is a byte concatenation and the signed
//! extrinsic is assembled mechanically. Matches the polkadot-app signing
//! convention: preimages longer than 256 bytes are BLAKE2b-256 hashed before
//! signing.

use parity_scale_codec::{Compact, Encode};
use truapi::latest::{HostSignPayloadData, TxPayloadExtension};

/// Preimages longer than this are hashed before signing (standard Substrate
/// signed-payload rule).
const MAX_SIGNED_PREIMAGE_LEN: usize = 256;

/// Extrinsic version 4 with the signed bit set.
const EXTRINSIC_V4_SIGNED: u8 = 0x84;
/// `MultiAddress::Id` variant index.
const MULTI_ADDRESS_ID: u8 = 0x00;
/// `MultiSignature::Sr25519` variant index.
const MULTI_SIGNATURE_SR25519: u8 = 0x01;

/// Signing preimage for an extrinsic payload assembled from pre-encoded
/// fields, in the polkadot-app field order. Empty optional fields are
/// skipped, mirroring the JS falsy-field rule.
pub fn extrinsic_payload_preimage(payload: &HostSignPayloadData) -> Vec<u8> {
    let parts: [&[u8]; 8] = [
        &payload.method,
        &payload.era,
        &payload.nonce,
        &payload.tip,
        &payload.spec_version,
        &payload.transaction_version,
        &payload.genesis_hash,
        &payload.block_hash,
    ];
    let mut preimage = Vec::new();
    for part in parts {
        preimage.extend_from_slice(part);
    }
    if let Some(asset_id) = &payload.asset_id {
        preimage.extend_from_slice(asset_id);
    }
    if let Some(metadata_hash) = &payload.metadata_hash {
        preimage.extend_from_slice(metadata_hash);
    }
    hash_large_preimage(preimage)
}

/// Signing preimage for a transaction built from pre-encoded extensions:
/// call data, then every extension's `extra`, then every extension's
/// `additional_signed`.
pub fn transaction_signing_preimage(
    call_data: &[u8],
    extensions: &[TxPayloadExtension],
) -> Vec<u8> {
    let mut preimage = call_data.to_vec();
    for extension in extensions {
        preimage.extend_from_slice(&extension.extra);
    }
    for extension in extensions {
        preimage.extend_from_slice(&extension.additional_signed);
    }
    hash_large_preimage(preimage)
}

/// Assemble a v4 signed extrinsic from a signer public key, an sr25519
/// signature over [`transaction_signing_preimage`], the pre-encoded
/// extension `extra` data, and the call data.
pub fn build_v4_signed_extrinsic(
    signer_public_key: [u8; 32],
    signature: [u8; 64],
    extensions: &[TxPayloadExtension],
    call_data: &[u8],
) -> Vec<u8> {
    let mut body = Vec::with_capacity(2 + 32 + 1 + 64 + call_data.len());
    body.push(EXTRINSIC_V4_SIGNED);
    body.push(MULTI_ADDRESS_ID);
    body.extend_from_slice(&signer_public_key);
    body.push(MULTI_SIGNATURE_SR25519);
    body.extend_from_slice(&signature);
    for extension in extensions {
        body.extend_from_slice(&extension.extra);
    }
    body.extend_from_slice(call_data);

    let mut extrinsic = Compact(body.len() as u32).encode();
    extrinsic.extend_from_slice(&body);
    extrinsic
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
    fn payload_preimage_uses_polkadot_app_field_order() {
        // method, era, nonce, tip, spec_version, transaction_version,
        // genesis_hash, block_hash. block_number is not part of the preimage.
        assert_eq!(
            extrinsic_payload_preimage(&payload()),
            vec![0x4D, 0xE1, 0x4E, 0x54, 0x51, 0x56, 0x61, 0x62, 0xB1, 0xB2]
        );
    }

    #[test]
    fn payload_preimage_appends_asset_id_and_metadata_hash() {
        let mut payload = payload();
        payload.asset_id = Some(vec![0xAA]);
        payload.metadata_hash = Some(vec![0xBB]);

        assert_eq!(
            extrinsic_payload_preimage(&payload),
            vec![
                0x4D, 0xE1, 0x4E, 0x54, 0x51, 0x56, 0x61, 0x62, 0xB1, 0xB2, 0xAA, 0xBB
            ]
        );
    }

    #[test]
    fn long_preimages_are_blake2b_hashed() {
        let mut payload = payload();
        payload.method = vec![0x4D; 300];

        let preimage = extrinsic_payload_preimage(&payload);

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

    #[test]
    fn transaction_preimage_orders_call_extra_then_implicit() {
        let extensions = vec![
            TxPayloadExtension {
                id: "CheckNonce".to_string(),
                extra: vec![0x01],
                additional_signed: vec![0x02],
            },
            TxPayloadExtension {
                id: "CheckSpecVersion".to_string(),
                extra: vec![0x03],
                additional_signed: vec![0x04],
            },
        ];

        assert_eq!(
            transaction_signing_preimage(&[0xCA, 0x11], &extensions),
            vec![0xCA, 0x11, 0x01, 0x03, 0x02, 0x04]
        );
    }

    #[test]
    fn builds_v4_signed_extrinsic_layout() {
        let extensions = vec![TxPayloadExtension {
            id: "CheckNonce".to_string(),
            extra: vec![0xEE],
            additional_signed: vec![0xDD],
        }];

        let extrinsic =
            build_v4_signed_extrinsic([0xAB; 32], [0xCD; 64], &extensions, &[0xCA, 0x11]);

        let body_len = 1 + 1 + 32 + 1 + 64 + 1 + 2;
        assert_eq!(extrinsic[..2], Compact(body_len as u32).encode()[..]);
        let body = &extrinsic[2..];
        assert_eq!(body.len(), body_len);
        assert_eq!(body[0], 0x84);
        assert_eq!(body[1], 0x00);
        assert_eq!(&body[2..34], &[0xAB; 32]);
        assert_eq!(body[34], 0x01);
        assert_eq!(&body[35..99], &[0xCD; 64]);
        assert_eq!(body[99], 0xEE);
        assert_eq!(&body[100..], &[0xCA, 0x11]);
    }
}
