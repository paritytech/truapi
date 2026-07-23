//! StatementStore allowance slot selection.
//!
//! An allowance is claimed at `(period, seq)`. The slot is bound to a 32-byte
//! `SSS_SLOT` context; occupancy is read from
//! `Resources.StatementStoreAllowances[period][alias]`, where the alias is
//! derived from OUR bandersnatch entropy in that slot context. Mirrors
//! signing-bot `allowance.ts` / `allowance-slots.ts`.

use sp_crypto_hashing::twox_128;
use verifiable::GenerateVerifiable;
use verifiable::ring::bandersnatch::BandersnatchVrfVerifiable;

use super::extension::Metadata;
use super::ring::blake2_128_concat;
use super::rpc::RpcClient;

/// StatementStore allowance period: one UTC day, in seconds.
pub const STATEMENT_STORE_PERIOD_SECONDS: u64 = 86_400;
/// Bulletin long-term-storage claim context prefix.
const LONG_TERM_STORAGE_CONTEXT_PREFIX: &[u8] = b"pop:polkadot.net/rsc-lts";

/// The current allowance period for `now_seconds`.
pub fn current_period(now_seconds: u64) -> u32 {
    (now_seconds / STATEMENT_STORE_PERIOD_SECONDS) as u32
}

/// The current long-term-storage period for `now_seconds`.
pub fn current_long_term_storage_period(
    now_seconds: u64,
    period_duration: u32,
) -> Result<u32, String> {
    if period_duration == 0 {
        return Err("Resources.LongTermStoragePeriodDuration is zero".to_string());
    }
    Ok((now_seconds / u64::from(period_duration)) as u32)
}

/// Derive the 32-byte StatementStore slot context:
/// `"SSS_SLOT:" ‖ u32be(period) ‖ u32be(seq) ‖ 0x20 fill`.
pub fn derive_slot_context(period: u32, seq: u32) -> [u8; 32] {
    let mut ctx = [0x20u8; 32];
    ctx[..9].copy_from_slice(b"SSS_SLOT:");
    ctx[9..13].copy_from_slice(&period.to_be_bytes());
    ctx[13..17].copy_from_slice(&seq.to_be_bytes());
    ctx
}

/// Derive the 32-byte Bulletin long-term-storage slot context:
/// `"pop:polkadot.net/rsc-lts" ‖ u32be(period) ‖ counter ‖ zero fill`.
pub fn derive_long_term_storage_context(period: u32, counter: u8) -> [u8; 32] {
    let mut ctx = [0u8; 32];
    ctx[..LONG_TERM_STORAGE_CONTEXT_PREFIX.len()].copy_from_slice(LONG_TERM_STORAGE_CONTEXT_PREFIX);
    let offset = LONG_TERM_STORAGE_CONTEXT_PREFIX.len();
    ctx[offset..offset + 4].copy_from_slice(&period.to_be_bytes());
    ctx[offset + 4] = counter;
    ctx
}

/// The slot alias for our `entropy` at `(period, seq)`.
pub fn slot_alias(entropy: [u8; 32], period: u32, seq: u32) -> Result<[u8; 32], String> {
    let secret = BandersnatchVrfVerifiable::new_secret(entropy);
    let context = derive_slot_context(period, seq);
    BandersnatchVrfVerifiable::alias_in_context(&secret, &context)
        .map_err(|err| format!("alias_in_context failed: {err:?}"))
}

/// The long-term-storage slot alias for our `entropy` at `(period, counter)`.
pub fn long_term_storage_alias(
    entropy: [u8; 32],
    period: u32,
    counter: u8,
) -> Result<[u8; 32], String> {
    let secret = BandersnatchVrfVerifiable::new_secret(entropy);
    let context = derive_long_term_storage_context(period, counter);
    BandersnatchVrfVerifiable::alias_in_context(&secret, &context)
        .map_err(|err| format!("alias_in_context failed: {err:?}"))
}

/// `Resources.StatementStoreAllowances[period][alias]` storage key.
/// key1 = Identity(u32be period); key2 = Blake2_128Concat(alias).
fn statement_store_allowance_key(period: u32, alias: &[u8; 32]) -> Vec<u8> {
    [
        twox_128(b"Resources").as_slice(),
        twox_128(b"StatementStoreAllowances").as_slice(),
        &period.to_be_bytes(),
        &blake2_128_concat(alias),
    ]
    .concat()
}

/// `Resources.SpentLongTermStorageAliases[period][alias]` storage key.
/// key1 = Identity(u32be period); key2 = Blake2_128Concat(alias).
fn spent_long_term_storage_alias_key(period: u32, alias: &[u8; 32]) -> Vec<u8> {
    [
        twox_128(b"Resources").as_slice(),
        twox_128(b"SpentLongTermStorageAliases").as_slice(),
        &period.to_be_bytes(),
        &blake2_128_concat(alias),
    ]
    .concat()
}

/// Max StatementStore slots per period from `Resources.LiteStmtStoreSlotsPerPeriod`.
fn max_slots(metadata: &Metadata) -> Result<u32, String> {
    let bytes = metadata
        .constant("Resources", "LiteStmtStoreSlotsPerPeriod")
        .ok_or_else(|| "Resources.LiteStmtStoreSlotsPerPeriod constant missing".to_string())?;
    let mut buf = [0u8; 4];
    let n = bytes.len().min(4);
    buf[..n].copy_from_slice(&bytes[..n]);
    Ok(u32::from_le_bytes(buf))
}

/// Max long-term-storage claims per period from
/// `Resources.LongTermStorageClaimsPerPeriod`.
fn long_term_storage_claims_per_period(metadata: &Metadata) -> Result<u8, String> {
    metadata
        .constant("Resources", "LongTermStorageClaimsPerPeriod")
        .and_then(|bytes| bytes.first().copied())
        .ok_or_else(|| "Resources.LongTermStorageClaimsPerPeriod constant missing".to_string())
}

/// Long-term-storage period duration in seconds from
/// `Resources.LongTermStoragePeriodDuration`.
pub fn long_term_storage_period_duration(metadata: &Metadata) -> Result<u32, String> {
    let bytes = metadata
        .constant("Resources", "LongTermStoragePeriodDuration")
        .ok_or_else(|| "Resources.LongTermStoragePeriodDuration constant missing".to_string())?;
    let mut buf = [0u8; 4];
    let n = bytes.len().min(4);
    buf[..n].copy_from_slice(&bytes[..n]);
    Ok(u32::from_le_bytes(buf))
}

/// The account id occupying a slot entry, if the storage value is present.
/// Entry = `account_id(32) ‖ seq(u32 LE) ‖ since(u64 LE)`.
fn entry_account_id(bytes: &[u8]) -> Option<[u8; 32]> {
    bytes.get(..32).map(|s| s.try_into().expect("32 bytes"))
}

/// The account holding our alias slot `(period, seq)`, read pinned to
/// `block_hash` (`None` when the slot entry is absent).
pub async fn read_slot_account_at(
    rpc: &RpcClient,
    entropy: [u8; 32],
    period: u32,
    seq: u32,
    block_hash: &str,
) -> Result<Option<[u8; 32]>, String> {
    let alias = slot_alias(entropy, period, seq)?;
    let key = statement_store_allowance_key(period, &alias);
    Ok(rpc
        .get_storage_at(&key, block_hash)
        .await
        .map_err(|e| e.to_string())?
        .and_then(|bytes| entry_account_id(&bytes)))
}

/// Outcome of scanning for a slot to register `target` in.
pub enum SlotSelection {
    /// A free `seq` we should claim.
    Free(u32),
    /// `target` already holds `seq` this period; no registration needed.
    AlreadyAllocated(u32),
}

/// Scan slots `0..max` for `period`, returning the first non-excluded free seq
/// (or detecting that `target` already holds one). `entropy` is our
/// bandersnatch entropy.
pub async fn scan_slot_excluding(
    rpc: &RpcClient,
    metadata: &Metadata,
    entropy: [u8; 32],
    period: u32,
    target: &[u8; 32],
    excluded: &[u32],
) -> Result<SlotSelection, String> {
    let max = max_slots(metadata)?;
    let mut first_free: Option<u32> = None;
    for seq in 0..max {
        let alias = slot_alias(entropy, period, seq)?;
        let key = statement_store_allowance_key(period, &alias);
        match rpc.get_storage(&key).await.map_err(|e| e.to_string())? {
            None => {
                if first_free.is_none() && !excluded.contains(&seq) {
                    first_free = Some(seq);
                }
            }
            Some(bytes) => {
                if entry_account_id(&bytes) == Some(*target) {
                    return Ok(SlotSelection::AlreadyAllocated(seq));
                }
            }
        }
    }
    first_free
        .map(SlotSelection::Free)
        .ok_or_else(|| format!("no free StatementStore slot in period {period} (max {max})"))
}

/// Scan long-term-storage aliases `0..max` for `period`, returning the first
/// free counter not listed in `excluded`. `entropy` is our bandersnatch entropy.
pub async fn scan_long_term_storage_counter_excluding(
    rpc: &RpcClient,
    metadata: &Metadata,
    entropy: [u8; 32],
    period: u32,
    excluded: &[u8],
) -> Result<u8, String> {
    let max = long_term_storage_claims_per_period(metadata)?;
    for counter in 0..max {
        if excluded.contains(&counter) {
            continue;
        }
        let alias = long_term_storage_alias(entropy, period, counter)?;
        let key = spent_long_term_storage_alias_key(period, &alias);
        if rpc
            .get_storage(&key)
            .await
            .map_err(|e| e.to_string())?
            .is_none()
        {
            return Ok(counter);
        }
    }
    Err(format!(
        "no free long-term-storage slot in period {period} (max {max})"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_context_layout() {
        let ctx = derive_slot_context(7, 3);
        assert_eq!(&ctx[..9], b"SSS_SLOT:");
        assert_eq!(&ctx[9..13], &7u32.to_be_bytes());
        assert_eq!(&ctx[13..17], &3u32.to_be_bytes());
        assert!(ctx[17..].iter().all(|&b| b == 0x20));
    }

    #[test]
    fn long_term_storage_context_layout() {
        let ctx = derive_long_term_storage_context(7, 3);
        assert_eq!(&ctx[..24], b"pop:polkadot.net/rsc-lts");
        assert_eq!(&ctx[24..28], &7u32.to_be_bytes());
        assert_eq!(ctx[28], 3);
        assert!(ctx[29..].iter().all(|&b| b == 0));
    }

    #[test]
    fn period_is_utc_day_index() {
        assert_eq!(current_period(86_400 * 20_000 + 5), 20_000);
    }

    #[test]
    fn long_term_storage_period_uses_chain_duration() {
        assert_eq!(
            current_long_term_storage_period(1_209_600 * 20 + 5, 1_209_600).unwrap(),
            20,
        );
    }
}
