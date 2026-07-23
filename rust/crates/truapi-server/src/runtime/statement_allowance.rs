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

use std::time::{Duration, Instant};

use futures::FutureExt;
use parity_scale_codec::Decode;
use serde_json::{Value, json};
use sp_crypto_hashing::twox_128;
use tracing::{debug, warn};

use extension::{ChainState, Metadata};
use ring::RingParams;
use rpc::RpcClient;
use slot::SlotSelection;

/// Bandersnatch entropy for a bip39 entropy: `blake2b256(bip39_entropy)`.
pub fn bandersnatch_entropy(bip39_entropy: &[u8]) -> [u8; 32] {
    blake2b_simd::Params::new()
        .hash_length(32)
        .hash(bip39_entropy)
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
#[derive(Debug)]
pub enum RegistrationOutcome {
    /// The extrinsic reached a block and the slot entry was verified at that
    /// block: the target now holds slot `seq`.
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

/// Target and slot-selection inputs for one statement-store registration.
pub struct RegistrationParams<'a> {
    /// Account that should receive the statement-store registration.
    pub target: &'a [u8; 32],
    /// Statement-store period for which the registration is requested.
    pub period: u32,
    /// Ring parameters used to build the membership proof.
    pub ring: &'a RingParams,
    /// Whether an existing registration for this period may be reused.
    pub reuse_existing: bool,
}

/// Result of a long-term storage claim attempt.
pub enum LongTermStorageOutcome {
    /// The extrinsic reached a block; the target should receive Bulletin
    /// authorization once XCM/chain propagation completes.
    Claimed {
        /// Block hash the extrinsic landed in.
        block_hash: String,
        /// Claimed counter within the long-term storage period.
        counter: u8,
        /// Ring index the proof was built against.
        ring_index: u32,
    },
}

/// Bulletin authorization state for one account.
#[derive(Debug, Clone, Copy)]
pub struct BulletinAllowanceInfo {
    /// Number of preimage bytes that remain available.
    pub remained_size: u64,
    /// Number of preimage submissions that remain available.
    pub remained_transactions: u32,
    /// Block at which the allowance expires.
    pub expires_in: u32,
    /// Block at which this allowance snapshot was fetched.
    pub fetched_at: u32,
}

impl BulletinAllowanceInfo {
    /// Returns whether the snapshot still permits at least one submission.
    pub fn available(self) -> bool {
        self.remained_size > 0
            && self.remained_transactions > 0
            && self.fetched_at < self.expires_in
    }
}

/// Find the newest ring (scanning up to `lookback` back from the current index)
/// that includes our member key. Reads the ring exponent once and stops at the
/// first match. Every read is pinned to one finalized block so the snapshot is
/// internally consistent; the pinned hash is recorded on the returned
/// [`RingParams`].
pub async fn find_including_ring(
    rpc: &RpcClient,
    metadata: &Metadata,
    entropy: [u8; 32],
    lookback: u32,
) -> Result<Option<RingParams>, String> {
    let member = proof::member_key(entropy);
    let at = rpc.finalized_head().await?;
    let exponent = ring::read_ring_exponent(rpc, metadata, &at).await?;
    let current = ring::read_current_ring_index_at(rpc, &at).await?;
    let oldest = current.saturating_sub(lookback);
    for ring_index in (oldest..=current).rev() {
        let members = ring::read_ring_members_at(rpc, ring_index, &at).await?;
        if members.contains(&member) {
            return Ok(Some(RingParams {
                members,
                exponent,
                ring_index,
                block_hash: at,
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
    params: RegistrationParams<'_>,
) -> Result<RegistrationOutcome, String> {
    let mut skipped_duplicate_slots = Vec::new();
    loop {
        let seq = match slot::scan_slot_excluding(
            rpc,
            metadata,
            entropy,
            params.period,
            params.target,
            &skipped_duplicate_slots,
            params.reuse_existing,
        )
        .await?
        {
            SlotSelection::AlreadyAllocated(seq) => {
                return Ok(RegistrationOutcome::AlreadyAllocated { seq });
            }
            SlotSelection::Free(seq) => seq,
        };

        let context = slot::derive_slot_context(params.period, seq);
        let call = extrinsic::build_set_statement_store_account_call(
            metadata,
            params.period,
            seq,
            params.target,
        )?;
        let message = extension::build_proof_message(metadata, &call, chain_state)?;
        let domain = proof::domain_for_ring_exponent(params.ring.exponent)?;
        let ring_proof =
            proof::ring_vrf_proof(domain, entropy, &params.ring.members, &context, &message)?;
        let as_resources_extra =
            extrinsic::build_as_resources_extra(metadata, &ring_proof, params.ring.ring_index)?;
        let extrinsic =
            extrinsic::build_unsigned_extrinsic(metadata, chain_state, &call, &as_resources_extra)?;

        match rpc.submit_and_watch(&extrinsic).await {
            Ok(block_hash) => {
                if slot::read_slot_account_at(rpc, entropy, params.period, seq, &block_hash).await?
                    != Some(*params.target)
                {
                    return Err(format!(
                        "registration reached block {block_hash} but slot (period {}, \
                         seq {seq}) is not held by the target account",
                        params.period
                    ));
                }
                return Ok(RegistrationOutcome::Registered {
                    block_hash,
                    seq,
                    ring_index: params.ring.ring_index,
                });
            }
            Err(err) if duplicate_submit_error(&err) => {
                skipped_duplicate_slots.push(seq);
            }
            Err(err) => return Err(err.to_string()),
        }
    }
}

/// Claim long-term Bulletin storage authorization for `target`, proving
/// membership in the already-located `ring`, at People-chain `period`.
pub async fn claim_long_term_storage(
    rpc: &RpcClient,
    metadata: &Metadata,
    chain_state: &ChainState,
    entropy: [u8; 32],
    target: &[u8; 32],
    period: u32,
    ring: &RingParams,
) -> Result<LongTermStorageOutcome, String> {
    let revision =
        ring::read_ring_revision(rpc, metadata, ring.ring_index, &ring.block_hash).await?;
    let mut skipped_duplicate_counters = Vec::new();
    loop {
        let counter = slot::scan_long_term_storage_counter_excluding(
            rpc,
            metadata,
            entropy,
            period,
            &skipped_duplicate_counters,
        )
        .await?;

        let context = slot::derive_long_term_storage_context(period, counter);
        let call =
            extrinsic::build_claim_long_term_storage_call(metadata, period, counter, target)?;
        let message = extension::build_proof_message(metadata, &call, chain_state)?;
        let domain = proof::domain_for_ring_exponent(ring.exponent)?;
        let ring_proof = proof::ring_vrf_proof(domain, entropy, &ring.members, &context, &message)?;
        let as_resources_extra = extrinsic::build_long_term_storage_extra(
            metadata,
            &ring_proof,
            ring.ring_index,
            revision,
        )?;
        let extrinsic =
            extrinsic::build_unsigned_extrinsic(metadata, chain_state, &call, &as_resources_extra)?;
        debug!(
            period,
            counter,
            ring_index = ring.ring_index,
            revision,
            "submitting Bulletin long-term-storage claim"
        );

        match rpc.submit_and_watch(&extrinsic).await {
            Ok(block_hash) => {
                return Ok(LongTermStorageOutcome::Claimed {
                    block_hash,
                    counter,
                    ring_index: ring.ring_index,
                });
            }
            Err(err) if duplicate_submit_error(&err) => {
                skipped_duplicate_counters.push(counter);
            }
            Err(err) => {
                warn!(
                    period,
                    counter,
                    ring_index = ring.ring_index,
                    revision,
                    %err,
                    "Bulletin long-term-storage claim failed"
                );
                return Err(err.to_string());
            }
        }
    }
}

/// Fetch Bulletin `TransactionStorage.Authorizations[Account(target)]`.
pub async fn fetch_bulletin_allowance(
    rpc: &RpcClient,
    target: &[u8; 32],
) -> Result<Option<BulletinAllowanceInfo>, String> {
    let Some(bytes) = rpc
        .get_storage(&bulletin_authorization_key(target))
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    let fetched_at = fetch_block_number(rpc).await?;
    decode_bulletin_allowance(&bytes, fetched_at).map(Some)
}

/// Wait until Bulletin authorization is available and fresher than `current`.
pub async fn wait_bulletin_authorization(
    rpc: &RpcClient,
    target: &[u8; 32],
    current: Option<BulletinAllowanceInfo>,
    timeout: Duration,
) -> Result<BulletinAllowanceInfo, String> {
    let started = Instant::now();
    let baseline = current.filter(|info| info.available());
    loop {
        let Some(info) = fetch_bulletin_allowance(rpc, target).await? else {
            wait_before_next_bulletin_authorization_poll(started, timeout).await?;
            continue;
        };
        if authorization_refreshed(info, baseline) {
            return Ok(info);
        }
        wait_before_next_bulletin_authorization_poll(started, timeout).await?;
    }
}

async fn wait_before_next_bulletin_authorization_poll(
    started: Instant,
    timeout: Duration,
) -> Result<(), String> {
    let Some(remaining) = timeout.checked_sub(started.elapsed()) else {
        return Err("timed out waiting for Bulletin authorization".to_string());
    };
    let delay = futures_timer::Delay::new(remaining.min(Duration::from_secs(2))).fuse();
    futures::pin_mut!(delay);
    delay.await;
    Ok(())
}

fn authorization_refreshed(
    info: BulletinAllowanceInfo,
    baseline: Option<BulletinAllowanceInfo>,
) -> bool {
    if !info.available() {
        return false;
    }
    match baseline {
        None => true,
        Some(current) => {
            info.remained_transactions > current.remained_transactions
                || info.remained_size > current.remained_size
                || info.expires_in > current.expires_in
        }
    }
}

/// `TransactionStorage.Authorizations[AuthorizationScope::Account(target)]`.
fn bulletin_authorization_key(target: &[u8; 32]) -> Vec<u8> {
    let mut scope = Vec::with_capacity(1 + 32);
    scope.push(0x00);
    scope.extend_from_slice(target);
    [
        twox_128(b"TransactionStorage").as_slice(),
        twox_128(b"Authorizations").as_slice(),
        &ring::blake2_128_concat(&scope),
    ]
    .concat()
}

fn decode_bulletin_allowance(
    bytes: &[u8],
    fetched_at: u32,
) -> Result<BulletinAllowanceInfo, String> {
    let mut input = bytes;
    let transactions =
        u32::decode(&mut input).map_err(|err| format!("authorization transactions: {err}"))?;
    let transactions_allowance = u32::decode(&mut input)
        .map_err(|err| format!("authorization transactions_allowance: {err}"))?;
    let bytes_used =
        u64::decode(&mut input).map_err(|err| format!("authorization bytes: {err}"))?;
    let _bytes_permanent =
        u64::decode(&mut input).map_err(|err| format!("authorization bytes_permanent: {err}"))?;
    let bytes_allowance =
        u64::decode(&mut input).map_err(|err| format!("authorization bytes_allowance: {err}"))?;
    let expires_in =
        u32::decode(&mut input).map_err(|err| format!("authorization expiration: {err}"))?;
    Ok(BulletinAllowanceInfo {
        remained_size: bytes_allowance.saturating_sub(bytes_used),
        remained_transactions: transactions_allowance.saturating_sub(transactions),
        expires_in,
        fetched_at,
    })
}

async fn fetch_block_number(rpc: &RpcClient) -> Result<u32, String> {
    let header = rpc
        .call("chain_getHeader", json!([]))
        .await
        .map_err(|err| err.to_string())?;
    let number = header
        .get("number")
        .and_then(Value::as_str)
        .ok_or_else(|| "chain_getHeader returned no number".to_string())?;
    u32::from_str_radix(number.trim_start_matches("0x"), 16)
        .map_err(|err| format!("chain_getHeader number: {err}"))
}

/// Pool responses meaning an equivalent claim already occupies the pool, so
/// the scan should move to the next slot. Bans and validity failures are hard
/// errors for the caller.
fn duplicate_submit_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("priority is too low") || message.contains("already imported")
}

#[cfg(test)]
mod tests {
    use subxt_rpcs::RpcClient as HostRpcClient;

    use super::rpc::testing::ScriptedRpc;
    use super::*;

    /// Fixture metadata captured from paseo-next-v2 (raw `RuntimeMetadataPrefixed`).
    const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/paseo-next-v2-metadata.scale");

    fn allowance(
        remained_size: u64,
        remained_transactions: u32,
        expires_in: u32,
    ) -> BulletinAllowanceInfo {
        BulletinAllowanceInfo {
            remained_size,
            remained_transactions,
            expires_in,
            fetched_at: 10,
        }
    }

    #[test]
    fn bulletin_refresh_accepts_available_state_when_baseline_was_unusable() {
        let exhausted_by_size = allowance(0, 4, 100);
        let refreshed_same_transactions = allowance(4096, 4, 100);

        assert!(!exhausted_by_size.available());
        assert!(authorization_refreshed(
            refreshed_same_transactions,
            Some(exhausted_by_size).filter(|info| info.available()),
        ));
    }

    #[test]
    fn bulletin_refresh_accepts_size_only_increase() {
        let baseline = allowance(128, 4, 100);
        let refreshed = allowance(4096, 4, 100);

        assert!(authorization_refreshed(refreshed, Some(baseline)));
    }

    #[test]
    fn bulletin_refresh_rejects_unchanged_available_state() {
        let baseline = allowance(128, 4, 100);

        assert!(!authorization_refreshed(baseline, Some(baseline)));
    }

    #[test]
    fn banned_submissions_are_not_classified_as_duplicates() {
        let classified: Vec<bool> = [
            "Priority is too low: (100 vs 100)",
            "Transaction Already Imported",
            "Transaction is temporarily banned",
            "Invalid Transaction",
        ]
        .into_iter()
        .map(duplicate_submit_error)
        .collect();

        assert_eq!(classified, vec![true, true, false, false]);
    }

    /// `StmtStoreAllowanceEntry { account_id, seq: 0, since: 0 }` as a scripted
    /// JSON storage result.
    fn slot_entry(account: [u8; 32]) -> String {
        let mut entry = account.to_vec();
        entry.extend_from_slice(&0u32.to_le_bytes());
        entry.extend_from_slice(&0u64.to_le_bytes());
        format!(r#""0x{}""#, hex::encode(entry))
    }

    /// Run `register_statement_account` against a scripted chain: all ten
    /// slots free, the extrinsic reaches block `0xb10c`, and the verification
    /// read at that block returns `verified_entry`.
    fn scripted_registration(
        verified_entry: &str,
    ) -> (Result<RegistrationOutcome, String>, ScriptedRpc) {
        let metadata = Metadata::decode(FIXTURE).unwrap();
        let chain_state = ChainState {
            spec_version: 1_000_000,
            transaction_version: 1,
            genesis_hash: [0xab; 32],
            nonce: 0,
        };
        let entropy = [0x11; 32];
        let ring = RingParams {
            members: vec![proof::member_key(entropy)],
            exponent: 9,
            ring_index: 0,
            block_hash: "0xfinal".to_string(),
        };

        let mut responses = vec!["null"; 10];
        responses.push(verified_entry);
        let scripted = ScriptedRpc::new(responses);
        scripted.script_subscription([r#"{"inBlock":"0xb10c"}"#]);
        let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

        let outcome = futures::executor::block_on(register_statement_account(
            &rpc,
            &metadata,
            &chain_state,
            entropy,
            RegistrationParams {
                target: &[0x22; 32],
                period: 7,
                ring: &ring,
                reuse_existing: true,
            },
        ));
        (outcome, scripted)
    }

    #[test]
    fn registration_is_verified_at_the_included_block() {
        let (outcome, scripted) = scripted_registration(&slot_entry([0x22; 32]));

        assert!(matches!(
            outcome.unwrap(),
            RegistrationOutcome::Registered { block_hash, seq: 0, ring_index: 0 }
                if block_hash == "0xb10c"
        ));
        let (method, params) = scripted.calls().last().cloned().unwrap();
        assert_eq!(method, "state_getStorage");
        assert!(
            params.ends_with(r#","0xb10c"]"#),
            "verification read not pinned to the included block: {params}"
        );
    }

    #[test]
    fn registration_fails_when_the_included_block_lacks_the_slot() {
        let (outcome, _scripted) = scripted_registration(&slot_entry([0x99; 32]));

        let err = outcome.unwrap_err();
        assert!(err.contains("0xb10c"), "unexpected error: {err}");
    }
}
