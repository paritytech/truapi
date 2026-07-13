//! LitePeople ring parameters from the People chain (`Members` pallet).
//!
//! Reads the on-chain ring so the membership proof is built against the same
//! members the runtime verifies against: the baked-in `included` prefix of the
//! current ring. Mirrors signing-bot `ring-proof.ts`.

use parity_scale_codec::{Compact, Decode};
use sp_crypto_hashing::{blake2_128, twox_64, twox_128};

use super::dynamic::{read_field_u32, read_field_variant_name};
use super::extension::Metadata;
use super::rpc::RpcClient;

/// LitePeople collection identifier: ASCII, exactly 32 bytes.
const LITE_PEOPLE_IDENTIFIER: &[u8; 32] = b"pop:polkadot.network/people-lite";
/// Ring member public key length.
const MEMBER_LEN: usize = 32;

/// On-chain LitePeople ring parameters for building a verifying proof.
pub struct RingParams {
    /// Ring members, sliced to the baked-in `included` prefix.
    pub members: Vec<[u8; 32]>,
    /// Ring size exponent (9 / 10 / 14).
    pub exponent: u8,
    /// Ring index these members belong to.
    pub ring_index: u32,
}

/// `Members.CurrentRingIndex[id]` storage key.
fn current_ring_index_key() -> Vec<u8> {
    [
        twox_128(b"Members").as_slice(),
        twox_128(b"CurrentRingIndex").as_slice(),
        LITE_PEOPLE_IDENTIFIER.as_slice(),
    ]
    .concat()
}

/// `Members.Collections[id]` storage key.
fn collections_key() -> Vec<u8> {
    [
        twox_128(b"Members").as_slice(),
        twox_128(b"Collections").as_slice(),
        LITE_PEOPLE_IDENTIFIER.as_slice(),
    ]
    .concat()
}

/// `Members.RingKeysStatus[(id, ring_index)]` storage key.
fn ring_keys_status_key(ring_index: u32) -> Vec<u8> {
    [
        twox_128(b"Members").as_slice(),
        twox_128(b"RingKeysStatus").as_slice(),
        LITE_PEOPLE_IDENTIFIER.as_slice(),
        &blake2_128_concat(&ring_index.to_le_bytes()),
    ]
    .concat()
}

/// `Members.Root[(id, ring_index)]` storage key.
fn ring_root_key(ring_index: u32) -> Vec<u8> {
    [
        twox_128(b"Members").as_slice(),
        twox_128(b"Root").as_slice(),
        LITE_PEOPLE_IDENTIFIER.as_slice(),
        &blake2_128_concat(&ring_index.to_le_bytes()),
    ]
    .concat()
}

/// `Members.RingKeys[(id, ring_index, page)]` storage key.
fn ring_keys_key(ring_index: u32, page: u32) -> Vec<u8> {
    [
        twox_128(b"Members").as_slice(),
        twox_128(b"RingKeys").as_slice(),
        LITE_PEOPLE_IDENTIFIER.as_slice(),
        &blake2_128_concat(&ring_index.to_le_bytes()),
        &twox_64_concat(&page.to_le_bytes()),
    ]
    .concat()
}

/// `Blake2_128Concat(x)` = `blake2_128(x) ‖ x`.
pub(super) fn blake2_128_concat(x: &[u8]) -> Vec<u8> {
    [blake2_128(x).as_slice(), x].concat()
}

/// `Twox64Concat(x)` = `twox_64(x) ‖ x`.
fn twox_64_concat(x: &[u8]) -> Vec<u8> {
    [twox_64(x).as_slice(), x].concat()
}

/// Map a `RingExponent` variant name to its exponent.
fn ring_exponent_from_name(name: &str) -> Result<u8, String> {
    match name {
        "R2e9" => Ok(9),
        "R2e10" => Ok(10),
        "R2e14" => Ok(14),
        other => Err(format!("unsupported RingExponent variant `{other}`")),
    }
}

/// Read the current LitePeople ring index (absent => 0).
pub async fn read_current_ring_index(rpc: &RpcClient) -> Result<u32, String> {
    match rpc
        .get_storage(&current_ring_index_key())
        .await
        .map_err(|e| e.to_string())?
    {
        Some(bytes) => u32::decode(&mut &bytes[..]).map_err(|e| format!("ring index: {e}")),
        None => Ok(0),
    }
}

/// Read the LitePeople ring size exponent from `Collections[LitePeople].ring_size`.
/// This is a chain constant, so read it once and reuse across ring indices.
pub async fn read_ring_exponent(rpc: &RpcClient, metadata: &Metadata) -> Result<u8, String> {
    let collection = rpc
        .get_storage(&collections_key())
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Members.Collections[LitePeople] missing".to_string())?;
    let value_type = metadata
        .storage_value_type("Members", "Collections")
        .ok_or_else(|| "Members.Collections type not in metadata".to_string())?;
    let variant =
        read_field_variant_name(metadata.registry(), value_type, "ring_size", &collection)?;
    ring_exponent_from_name(&variant)
}

/// Read the members of `ring_index`, sliced to the baked-in `included` prefix.
pub async fn read_ring_members_at(
    rpc: &RpcClient,
    ring_index: u32,
) -> Result<Vec<[u8; 32]>, String> {
    // 1. Page through RingKeys collecting raw 32-byte members.
    let mut members = Vec::new();
    for page in 0.. {
        let Some(bytes) = rpc
            .get_storage(&ring_keys_key(ring_index, page))
            .await
            .map_err(|e| e.to_string())?
        else {
            break;
        };
        let mut cursor = &bytes[..];
        let Compact(len) =
            Compact::<u32>::decode(&mut cursor).map_err(|e| format!("ring keys len: {e}"))?;
        if len == 0 {
            break;
        }
        for i in 0..len as usize {
            let start = i * MEMBER_LEN;
            let member: [u8; 32] = cursor
                .get(start..start + MEMBER_LEN)
                .ok_or_else(|| "ring keys page truncated".to_string())?
                .try_into()
                .expect("slice is 32 bytes");
            members.push(member);
        }
    }

    // 2. Slice to the baked-in `included` prefix (absent status => all included).
    if let Some(status) = rpc
        .get_storage(&ring_keys_status_key(ring_index))
        .await
        .map_err(|e| e.to_string())?
    {
        // RingStatus = { total: u32 LE, included: u32 LE, .. }.
        let included_bytes = status
            .get(4..)
            .ok_or_else(|| "ring status truncated before included field".to_string())?;
        let included =
            u32::decode(&mut &included_bytes[..]).map_err(|e| format!("ring status: {e}"))?;
        members.truncate(included as usize);
    }

    Ok(members)
}

/// Read `Members.Root[LitePeople][ring_index].revision` (absent => 0).
pub async fn read_ring_revision(
    rpc: &RpcClient,
    metadata: &Metadata,
    ring_index: u32,
) -> Result<u32, String> {
    match rpc
        .get_storage(&ring_root_key(ring_index))
        .await
        .map_err(|e| e.to_string())?
    {
        Some(bytes) => {
            let value_type = metadata
                .storage_value_type("Members", "Root")
                .ok_or_else(|| "Members.Root type not in metadata".to_string())?;
            read_field_u32(metadata.registry(), value_type, "revision", &bytes)
                .map_err(|e| format!("ring revision: {e}"))
        }
        None => Ok(0),
    }
}
