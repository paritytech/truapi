//! On-chain statement-store allowance registration (`set_statement_store_account`).
//!
//! Mirrors how an iOS/web client obtains statement-store allowance from the real
//! People chain: build the `Resources.set_statement_store_account` call, prove
//! LitePeople ring membership with a bandersnatch ring-VRF, and submit the
//! resulting unsigned General (v5) extrinsic. Native only (needs the
//! `verifiable` prover and live chain reads).

pub mod dynamic;
pub mod extension;
pub mod extrinsic;
pub mod proof;
pub mod ring;
pub mod rpc;
pub mod slot;

use blake2_rfc::blake2b::blake2b;
use parity_scale_codec::Decode;
use serde_json::{Value, json};

use extension::{ChainState, Metadata};
use ring::RingParams;
use rpc::RpcClient;
use slot::SlotSelection;

/// Bandersnatch entropy for a bip39 entropy: `blake2b256(bip39_entropy)`.
pub fn bandersnatch_entropy(bip39_entropy: &[u8]) -> [u8; 32] {
    blake2b(32, &[], bip39_entropy)
        .as_bytes()
        .try_into()
        .expect("BLAKE2b-256 returns 32 bytes")
}

/// Fetch and decode the runtime metadata (`state_getMetadata`).
pub async fn fetch_metadata(rpc: &RpcClient) -> Result<Metadata, String> {
    let value = rpc
        .call("state_getMetadata", json!([]))
        .await
        .map_err(|e| e.to_string())?;
    let hex_str = value
        .as_str()
        .ok_or_else(|| "state_getMetadata returned non-string".to_string())?;
    let bytes = hex::decode(hex_str.strip_prefix("0x").unwrap_or(hex_str))
        .map_err(|e| format!("metadata hex: {e}"))?;
    // `state_getMetadata` may return either the raw `RuntimeMetadataPrefixed`
    // (starts with the `meta` magic) or an OpaqueMetadata wrapper
    // (`Vec<u8>` = compact(len) ‖ bytes). Strip the wrapper only when present.
    const META_MAGIC: [u8; 4] = *b"meta";
    if bytes.get(..4) == Some(&META_MAGIC) {
        Metadata::decode(&bytes)
    } else {
        let inner =
            Vec::<u8>::decode(&mut &bytes[..]).map_err(|e| format!("opaque metadata: {e}"))?;
        Metadata::decode(&inner)
    }
}

/// Fetch the chain state needed to fill the signed extensions.
pub async fn fetch_chain_state(rpc: &RpcClient) -> Result<ChainState, String> {
    let genesis_hex = rpc
        .call("chain_getBlockHash", json!([0]))
        .await
        .map_err(|e| e.to_string())?;
    let genesis_str = genesis_hex
        .as_str()
        .ok_or_else(|| "chain_getBlockHash returned non-string".to_string())?;
    let genesis = hex::decode(genesis_str.strip_prefix("0x").unwrap_or(genesis_str))
        .map_err(|e| format!("genesis hex: {e}"))?;
    let genesis_hash: [u8; 32] = genesis
        .try_into()
        .map_err(|_| "genesis hash is not 32 bytes".to_string())?;

    let runtime = rpc
        .call("state_getRuntimeVersion", json!([]))
        .await
        .map_err(|e| e.to_string())?;
    let spec_version = json_u32(&runtime, "specVersion")?;
    let transaction_version = json_u32(&runtime, "transactionVersion")?;

    Ok(ChainState {
        spec_version,
        transaction_version,
        genesis_hash,
        nonce: 0,
    })
}

/// Read a u32 field from a JSON object.
fn json_u32(value: &Value, field: &str) -> Result<u32, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| format!("missing/invalid {field}"))
}

/// Result of a statement-store allowance registration attempt.
pub enum RegistrationOutcome {
    /// The extrinsic reached a block; the target now holds slot `seq`.
    Registered {
        /// Block hash the extrinsic landed in.
        block_hash: String,
        /// Claimed slot sequence.
        seq: u32,
        /// Ring index the proof was built against.
        ring_index: u32,
    },
    /// The target already held a slot this period; nothing submitted.
    AlreadyAllocated {
        /// Existing slot sequence.
        seq: u32,
    },
}

/// Find the newest ring (scanning up to `lookback` back from the current index)
/// that includes our member key. Reads the ring exponent once and stops at the
/// first match.
pub async fn find_including_ring(
    rpc: &RpcClient,
    metadata: &Metadata,
    entropy: [u8; 32],
    lookback: u32,
) -> Result<Option<RingParams>, String> {
    let member = proof::member_key(entropy);
    let exponent = ring::read_ring_exponent(rpc, metadata).await?;
    let current = ring::read_current_ring_index(rpc).await?;
    let oldest = current.saturating_sub(lookback);
    for ring_index in (oldest..=current).rev() {
        let members = ring::read_ring_members_at(rpc, ring_index).await?;
        if members.contains(&member) {
            return Ok(Some(RingParams {
                members,
                exponent,
                ring_index,
            }));
        }
    }
    Ok(None)
}

/// Register statement-store allowance for `target`, proving membership in the
/// already-located `ring`, at UTC-day `period`.
pub async fn register_statement_account(
    rpc: &RpcClient,
    metadata: &Metadata,
    chain_state: &ChainState,
    entropy: [u8; 32],
    target: &[u8; 32],
    period: u32,
    ring: &RingParams,
) -> Result<RegistrationOutcome, String> {
    let seq = match slot::scan_slot(rpc, metadata, entropy, period, target).await? {
        SlotSelection::AlreadyAllocated(seq) => {
            return Ok(RegistrationOutcome::AlreadyAllocated { seq });
        }
        SlotSelection::Free(seq) => seq,
    };

    let context = slot::derive_slot_context(period, seq);
    let call = extrinsic::build_set_statement_store_account_call(period, seq, target);
    let message = extension::build_proof_message(metadata, &call, chain_state)?;
    let domain = proof::domain_for_ring_exponent(ring.exponent)?;
    let ring_proof = proof::ring_vrf_proof(domain, entropy, &ring.members, &context, &message)?;
    let as_resources_extra = extrinsic::build_as_resources_extra(&ring_proof, ring.ring_index);
    let extrinsic =
        extrinsic::build_unsigned_extrinsic(metadata, chain_state, &call, &as_resources_extra)?;

    let block_hash = rpc
        .submit_and_watch(&extrinsic)
        .await
        .map_err(|e| e.to_string())?;
    Ok(RegistrationOutcome::Registered {
        block_hash,
        seq,
        ring_index: ring.ring_index,
    })
}
