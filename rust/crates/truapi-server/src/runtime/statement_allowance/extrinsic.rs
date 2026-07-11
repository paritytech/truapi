//! `Resources.set_statement_store_account` call + unsigned General (v5)
//! extrinsic assembly. Mirrors signing-bot `allocation.ts` / `extrinsic-submit.ts`.

use parity_scale_codec::{Compact, Encode};

use super::extension::{ChainState, Metadata};

/// Pallet + call index for `Resources.set_statement_store_account` (63 / 10).
pub const SET_STATEMENT_STORE_ACCOUNT_CALL: [u8; 2] = [0x3f, 0x0a];
/// `AsResourcesInfo::RegisterStatementStoreAllowance` variant index.
const REGISTER_STATEMENT_STORE_ALLOWANCE: u8 = 0x02;
/// `MembershipCollection::LitePeople` variant index.
const MEMBERSHIP_COLLECTION_LITE_PEOPLE: u8 = 0x01;
/// General-transaction preamble byte: `0b01` (General) | version 5.
const GENERAL_V5_PREAMBLE: u8 = 0x45;
/// Current signed-extension version byte.
const EXTENSION_VERSION: u8 = 0x00;
/// `Option::Some` discriminant for the `AsResources` extension `extra`.
const OPTION_SOME: u8 = 0x01;

/// Encode `Resources.set_statement_store_account(period, seq, target)`:
/// `3f 0a ‖ period_u32LE ‖ seq_u32LE ‖ target[32]`.
pub fn build_set_statement_store_account_call(period: u32, seq: u32, target: &[u8; 32]) -> Vec<u8> {
    let mut call = Vec::with_capacity(2 + 4 + 4 + 32);
    call.extend_from_slice(&SET_STATEMENT_STORE_ACCOUNT_CALL);
    call.extend_from_slice(&period.to_le_bytes());
    call.extend_from_slice(&seq.to_le_bytes());
    call.extend_from_slice(target);
    call
}

/// Encode the `AsResources` extension `extra` for a statement-store allowance:
/// `Some(RegisterStatementStoreAllowance { proof, ring_index, LitePeople })`.
pub fn build_as_resources_extra(proof: &[u8], ring_index: u32) -> Vec<u8> {
    let mut extra = Vec::with_capacity(2 + 2 + proof.len() + 4 + 1);
    extra.push(OPTION_SOME);
    extra.push(REGISTER_STATEMENT_STORE_ALLOWANCE);
    extra.extend_from_slice(&Compact(proof.len() as u32).encode());
    extra.extend_from_slice(proof);
    extra.extend_from_slice(&ring_index.to_le_bytes());
    extra.push(MEMBERSHIP_COLLECTION_LITE_PEOPLE);
    extra
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
        let call = build_set_statement_store_account_call(7, 0, &[0u8; 32]);
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
    fn as_resources_extra_wraps_proof_as_bytes() {
        let proof = vec![0xEE; 785];
        let extra = build_as_resources_extra(&proof, 3);
        // Some(0x01) ‖ variant(0x02) ‖ compact(785)=0x45,0x0c ‖ 785 bytes ‖ ringIndex LE ‖ LitePeople.
        assert_eq!(&extra[..2], &[0x01, 0x02]);
        assert_eq!(&extra[2..4], &Compact(785u32).encode()[..]);
        assert_eq!(&extra[4..4 + 785], &proof[..]);
        assert_eq!(&extra[4 + 785..4 + 785 + 4], &3u32.to_le_bytes());
        assert_eq!(extra[4 + 785 + 4], MEMBERSHIP_COLLECTION_LITE_PEOPLE);
    }

    #[test]
    fn extrinsic_has_general_v5_preamble_and_embeds_call() {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let call = build_set_statement_store_account_call(7, 0, &[0u8; 32]);
        let extra = build_as_resources_extra(&vec![0xEE; 785], 0);
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
