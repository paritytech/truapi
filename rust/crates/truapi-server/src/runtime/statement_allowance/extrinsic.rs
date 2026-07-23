//! `Resources.set_statement_store_account` call + unsigned General (v5)
//! extrinsic assembly. Mirrors signing-bot `allocation.ts` / `extrinsic-submit.ts`.
//! Dispatch and variant indices are resolved by name from the fetched runtime
//! metadata, so a re-indexed runtime fails loudly instead of encoding a wrong
//! call.

use parity_scale_codec::{Compact, Encode};

use super::extension::{ChainState, Metadata};

/// General-transaction preamble byte: `0b01` (General) | version 5.
const GENERAL_V5_PREAMBLE: u8 = 0x45;
/// Current signed-extension version byte.
const EXTENSION_VERSION: u8 = 0x00;
/// `Option::Some` discriminant for the `AsResources` extension `extra`.
const OPTION_SOME: u8 = 0x01;

/// Encode `Resources.set_statement_store_account(period, seq, target)`:
/// `pallet ‖ call ‖ period_u32LE ‖ seq_u32LE ‖ target[32]`, with the dispatch
/// indices resolved from `metadata`.
pub fn build_set_statement_store_account_call(
    metadata: &Metadata,
    period: u32,
    seq: u32,
    target: &[u8; 32],
) -> Result<Vec<u8>, String> {
    let indices = metadata.call_indices("Resources", "set_statement_store_account")?;
    let mut call = Vec::with_capacity(2 + 4 + 4 + 32);
    call.extend_from_slice(&indices);
    call.extend_from_slice(&period.to_le_bytes());
    call.extend_from_slice(&seq.to_le_bytes());
    call.extend_from_slice(target);
    Ok(call)
}

/// Encode `Resources.claim_long_term_storage(period, counter, account_id)`:
/// `pallet ‖ call ‖ period_u32LE ‖ counter_u8 ‖ account_id[32]`, with the
/// dispatch indices resolved from `metadata`.
pub fn build_claim_long_term_storage_call(
    metadata: &Metadata,
    period: u32,
    counter: u8,
    account_id: &[u8; 32],
) -> Result<Vec<u8>, String> {
    let indices = metadata.call_indices("Resources", "claim_long_term_storage")?;
    let mut call = Vec::with_capacity(2 + 4 + 1 + 32);
    call.extend_from_slice(&indices);
    call.extend_from_slice(&period.to_le_bytes());
    call.push(counter);
    call.extend_from_slice(account_id);
    Ok(call)
}

/// Encode the `AsResources` extension `extra` for a statement-store allowance:
/// `Some(RegisterStatementStoreAllowance { proof, ring_index, LitePeople })`,
/// with the variant indices resolved from `metadata`.
pub fn build_as_resources_extra(
    metadata: &Metadata,
    proof: &[u8],
    ring_index: u32,
) -> Result<Vec<u8>, String> {
    let (info_index, lite_people) =
        metadata.as_resources_variant_indices("RegisterStatementStoreAllowance")?;
    let mut extra = Vec::with_capacity(2 + 2 + proof.len() + 4 + 1);
    extra.push(OPTION_SOME);
    extra.push(info_index);
    extra.extend_from_slice(&Compact(proof.len() as u32).encode());
    extra.extend_from_slice(proof);
    extra.extend_from_slice(&ring_index.to_le_bytes());
    extra.push(lite_people);
    Ok(extra)
}

/// Encode the `AsResources` extension `extra` for a long-term storage claim:
/// `Some(ClaimLongTermStorage { proof, ring_index, revision, LitePeople })`,
/// with the variant indices resolved from `metadata`.
pub fn build_long_term_storage_extra(
    metadata: &Metadata,
    proof: &[u8],
    ring_index: u32,
    revision: u32,
) -> Result<Vec<u8>, String> {
    let (info_index, lite_people) =
        metadata.as_resources_variant_indices("ClaimLongTermStorage")?;
    let mut extra = Vec::with_capacity(2 + 2 + proof.len() + 4 + 4 + 1);
    extra.push(OPTION_SOME);
    extra.push(info_index);
    extra.extend_from_slice(&Compact(proof.len() as u32).encode());
    extra.extend_from_slice(proof);
    extra.extend_from_slice(&ring_index.to_le_bytes());
    extra.extend_from_slice(&revision.to_le_bytes());
    extra.push(lite_people);
    Ok(extra)
}

/// Assemble the unsigned General (v5) extrinsic:
/// `compact(len) ‖ 0x45 ‖ 0x00 ‖ Σ(all extra, AsResources = Some(info)) ‖ call`.
pub fn build_unsigned_extrinsic(
    metadata: &Metadata,
    state: &ChainState,
    call_data: &[u8],
    as_resources_extra: &[u8],
) -> Result<Vec<u8>, String> {
    let all = metadata.encode_signed_extensions(state);
    let as_resources_index = metadata
        .as_resources_index()
        .ok_or_else(|| "AsResources extension not found in metadata".to_string())?;

    let mut body = vec![GENERAL_V5_PREAMBLE, EXTENSION_VERSION];
    for (i, ext) in all.iter().enumerate() {
        if i == as_resources_index {
            body.extend_from_slice(as_resources_extra);
        } else {
            body.extend_from_slice(&ext.extra);
        }
    }
    body.extend_from_slice(call_data);

    let mut extrinsic = Compact(body.len() as u32).encode();
    extrinsic.extend_from_slice(&body);
    Ok(extrinsic)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/paseo-next-v2-metadata.scale");

    fn fixture_state() -> ChainState {
        ChainState {
            spec_version: 1_000_000,
            transaction_version: 1,
            genesis_hash: [0xab; 32],
            nonce: 0,
        }
    }

    #[test]
    fn call_layout_is_pallet_call_period_seq_target() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let call = build_set_statement_store_account_call(&metadata, 7, 0, &[0u8; 32]).unwrap();
        assert_eq!(
            call,
            [
                vec![0x3f, 0x0a],
                7u32.to_le_bytes().to_vec(),
                0u32.to_le_bytes().to_vec(),
                vec![0u8; 32],
            ]
            .concat()
        );
    }

    #[test]
    fn long_term_storage_call_layout_is_pallet_call_period_counter_account() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let call = build_claim_long_term_storage_call(&metadata, 7, 3, &[0u8; 32]).unwrap();
        assert_eq!(
            call,
            [
                vec![0x3f, 0x0c],
                7u32.to_le_bytes().to_vec(),
                vec![3],
                vec![0u8; 32],
            ]
            .concat()
        );
    }

    #[test]
    fn as_resources_extra_wraps_proof_as_bytes() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let proof = vec![0xEE; 785];
        let extra = build_as_resources_extra(&metadata, &proof, 3).unwrap();
        // Some(0x01) ‖ variant(0x02) ‖ compact(785)=0x45,0x0c ‖ 785 bytes ‖ ringIndex LE ‖ LitePeople.
        assert_eq!(
            extra,
            [
                vec![0x01, 0x02],
                Compact(785u32).encode(),
                proof,
                3u32.to_le_bytes().to_vec(),
                vec![0x01],
            ]
            .concat()
        );
    }

    #[test]
    fn long_term_storage_extra_wraps_revision() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let proof = vec![0xEE; 785];
        let extra = build_long_term_storage_extra(&metadata, &proof, 3, 9).unwrap();
        // Some(0x01) ‖ variant(0x03) ‖ compact(785)=0x45,0x0c ‖ proof
        // ‖ ringIndex LE ‖ revision LE ‖ LitePeople.
        assert_eq!(
            extra,
            [
                vec![0x01, 0x03],
                Compact(785u32).encode(),
                proof,
                3u32.to_le_bytes().to_vec(),
                9u32.to_le_bytes().to_vec(),
                vec![0x01],
            ]
            .concat()
        );
    }

    #[test]
    fn extrinsic_has_general_v5_preamble_and_embeds_call() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let call = build_set_statement_store_account_call(&metadata, 7, 0, &[0u8; 32]).unwrap();
        let extra = build_as_resources_extra(&metadata, &[0xEE; 785], 0).unwrap();
        let xt = build_unsigned_extrinsic(&metadata, &fixture_state(), &call, &extra).unwrap();

        // Strip the compact length prefix and check the body head + tail.
        let body = &xt[compact_prefix_len(&xt)..];
        assert_eq!(&body[..2], &[GENERAL_V5_PREAMBLE, EXTENSION_VERSION]);
        assert_eq!(&body[body.len() - call.len()..], &call[..]);
        // The Some(info) extra appears verbatim in the body.
        assert!(
            body.windows(extra.len()).any(|w| w == extra),
            "AsResources Some(info) extra should appear in the body",
        );
    }

    /// Length of the SCALE compact prefix at the head of `xt`.
    fn compact_prefix_len(xt: &[u8]) -> usize {
        match xt[0] & 0b11 {
            0b00 => 1,
            0b01 => 2,
            0b10 => 4,
            _ => 5,
        }
    }
}
