//! `Resources.set_statement_store_account` call + unsigned General (v5)
//! extrinsic assembly. Mirrors signing-bot `allocation.ts` / `extrinsic-submit.ts`.
//! Dispatch and variant indices are resolved by name from the fetched runtime
//! metadata, so a re-indexed runtime fails loudly instead of encoding a wrong
//! call.

use parity_scale_codec::{Decode, Encode};

use super::extension::{ChainState, Metadata};

/// General-transaction preamble byte: `0b01` (General) | version 5.
const GENERAL_V5_PREAMBLE: u8 = 0x45;
/// Current signed-extension version byte.
const EXTENSION_VERSION: u8 = 0x00;
/// `Option::Some` discriminant for the `AsResources` extension `extra`.
const OPTION_SOME: u8 = 0x01;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
struct SetStatementStoreAccountCallArgs {
    period: u32,
    seq: u32,
    target: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
struct ClaimLongTermStorageCallArgs {
    period: u32,
    counter: u8,
    account_id: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct RegisterStatementStoreAllowanceInfo {
    proof: Vec<u8>,
    ring_index: u32,
    personhood: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct ClaimLongTermStorageInfo {
    proof: Vec<u8>,
    ring_index: u32,
    revision: u32,
    personhood: u8,
}

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
    SetStatementStoreAccountCallArgs {
        period,
        seq,
        target: *target,
    }
    .encode_to(&mut call);
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
    ClaimLongTermStorageCallArgs {
        period,
        counter,
        account_id: *account_id,
    }
    .encode_to(&mut call);
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
    RegisterStatementStoreAllowanceInfo {
        proof: proof.to_vec(),
        ring_index,
        personhood: lite_people,
    }
    .encode_to(&mut extra);
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
    ClaimLongTermStorageInfo {
        proof: proof.to_vec(),
        ring_index,
        revision,
        personhood: lite_people,
    }
    .encode_to(&mut extra);
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

    Ok(body.encode())
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::Compact;

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
        assert_eq!(
            SetStatementStoreAccountCallArgs::decode(&mut &call[2..]).unwrap(),
            SetStatementStoreAccountCallArgs {
                period: 7,
                seq: 0,
                target: [0; 32],
            }
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
        assert_eq!(
            ClaimLongTermStorageCallArgs::decode(&mut &call[2..]).unwrap(),
            ClaimLongTermStorageCallArgs {
                period: 7,
                counter: 3,
                account_id: [0; 32],
            }
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
        assert_eq!(
            RegisterStatementStoreAllowanceInfo::decode(&mut &extra[2..]).unwrap(),
            RegisterStatementStoreAllowanceInfo {
                proof: vec![0xEE; 785],
                ring_index: 3,
                personhood: 1,
            }
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
        assert_eq!(
            ClaimLongTermStorageInfo::decode(&mut &extra[2..]).unwrap(),
            ClaimLongTermStorageInfo {
                proof: vec![0xEE; 785],
                ring_index: 3,
                revision: 9,
                personhood: 1,
            }
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
