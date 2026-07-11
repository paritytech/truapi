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

/// The current allowance period for `now_seconds`.
pub fn current_period(now_seconds: u64) -> u32 {
    (now_seconds / STATEMENT_STORE_PERIOD_SECONDS) as u32
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

/// The slot alias for our `entropy` at `(period, seq)`.
pub fn slot_alias(entropy: [u8; 32], period: u32, seq: u32) -> Result<[u8; 32], String> {
    let secret = BandersnatchVrfVerifiable::new_secret(entropy);
    let context = derive_slot_context(period, seq);
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

/// The account id occupying a slot entry, if the storage value is present.
/// Entry = `account_id(32) ‖ seq(u32 LE) ‖ since(u64 LE)`.
fn entry_account_id(bytes: &[u8]) -> Option<[u8; 32]> {
    bytes.get(..32).map(|s| s.try_into().expect("32 bytes"))
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
    fn period_is_utc_day_index() {
        assert_eq!(current_period(86_400 * 20_000 + 5), 20_000);
    }
}
