//! Top-level spec functions.
//!
//! - Lock-handle extractors: [`coin_lock_handle`], [`entry_lock_handle`].
//! - Lock-counting predicates: [`count_coin_locks_in_vec`],
//!   [`count_entry_locks_in_vec`].
//! - Cross-state lock referential integrity: [`lock_refint`].
//! - Op-status classifiers: [`is_terminal_op_status`],
//!   [`is_cancellable_op_status`], [`is_mid_op_status`].
//! - Payment-memo helpers: [`count_matched_memos`],
//!   [`classify_incoming_payment`].
//! - Coin/entry §6.3 priority orders: [`coin_priority_lt`],
//!   [`entry_priority_lt`].
//! - Prefix-sum aggregators (used by exec-side aggregator
//!   implementations): the `sum_*_prefix` family + [`sum_of_coin_values`].
//! - The `2^exp` coin-value spec family: [`coin_value`], [`pow2_nat`],
//!   [`coin_value_pow2`].

use vstd::prelude::*;

use crate::*;

verus! {

/// Spec helper: extract the lock handle from a coin's state, if any.
/// Returns `Some(h)` for `LockedFor(h)`, `None` otherwise. Avoids
/// match-bound variables in proof contexts — see Phase 1d note in
/// project memory.
pub open spec fn coin_lock_handle(state: CoinState) -> Option<OpHandle> {
    match state {
        CoinState::LockedFor(h) => Some(h),
        _ => None,
    }
}

/// Spec-only: count the number of Vec coins currently `LockedFor(handle)`
/// within the prefix `v[0..j]`. Used as a decreases measure for
/// bulk-sweep loops.
pub open spec fn count_coin_locks_in_vec(
    v: Seq<CoinRec>,
    handle: OpHandle,
    j: nat,
) -> nat
    decreases j
{
    if j == 0 {
        0
    } else {
        let prev = count_coin_locks_in_vec(v, handle, (j - 1) as nat);
        if v[(j - 1) as int].state == CoinState::LockedFor(handle) {
            prev + 1
        } else {
            prev
        }
    }
}

/// Spec-only: count the number of Vec entries currently
/// `LocalLockedFor(handle)` within the prefix `v[0..j]`.
pub open spec fn count_entry_locks_in_vec(
    v: Seq<EntryRec>,
    handle: OpHandle,
    j: nat,
) -> nat
    decreases j
{
    if j == 0 {
        0
    } else {
        let prev = count_entry_locks_in_vec(v, handle, (j - 1) as nat);
        if v[(j - 1) as int].local == EntryLocal::LocalLockedFor(handle) {
            prev + 1
        } else {
            prev
        }
    }
}

/// Spec helper: extract the lock handle from an entry's local state,
/// if any. Returns `Some(h)` for `LocalLockedFor(h)`, `None` otherwise.
pub open spec fn entry_lock_handle(local: EntryLocal) -> Option<OpHandle> {
    match local {
        EntryLocal::LocalLockedFor(h) => Some(h),
        _ => None,
    }
}

/// Cross-state lock referential integrity (Phase 1d-deferred
/// invariant). Every coin in `LockedFor(h)` references an existing
/// operation `h`; same for every entry in `LocalLockedFor(h)`.
///
/// Not part of the State's main `invariant()` predicate — that would
/// cascade through every method's proof. Instead this is an *opt-in*
/// predicate that callers can preserve themselves and pass as a
/// precondition to primitives that need it (e.g. a future bulk-sweep
/// `cancel_op` that wants to assert "after release, no LockedFor(h)
/// references h").
pub open spec fn lock_refint(
    coins: Map<(PurseId, u64), CoinRec>,
    entries: Map<(PurseId, u64), EntryRec>,
    operations: Map<OpHandle, OperationRec>,
) -> bool {
    (forall|k: (PurseId, u64)|
        #[trigger] coins.dom().contains(k)
        ==> {
            let h_opt = coin_lock_handle(coins[k].state);
            h_opt.is_none() || operations.dom().contains(h_opt.unwrap())
        })
    && (forall|k: (PurseId, u64)|
        #[trigger] entries.dom().contains(k)
        ==> {
            let h_opt = entry_lock_handle(entries[k].local);
            h_opt.is_none() || operations.dom().contains(h_opt.unwrap())
        })
}

/// True iff `status` is a terminal op state (no further transitions
/// follow). Quint `isTerminal`.
pub open spec fn is_terminal_op_status(status: OpStatus) -> bool {
    match status {
        OpStatus::Done => true,
        OpStatus::Failed => true,
        _ => false,
    }
}

/// True iff an op in `status` can transition to `Failed` via
/// `set_op_failed`. Mirrors the Quint `isCancellable` predicate.
pub open spec fn is_cancellable_op_status(status: OpStatus) -> bool {
    match status {
        OpStatus::Preparing => true,
        OpStatus::Waiting(_) => true,
        _ => false,
    }
}

/// True iff `status` is a mid-flight chain state (extrinsic in transit
/// or just landed). Quint `isMid`.
pub open spec fn is_mid_op_status(status: OpStatus) -> bool {
    match status {
        OpStatus::Submitted => true,
        OpStatus::InBlock => true,
        OpStatus::Finalized => true,
        _ => false,
    }
}

/// Spec-only: count memos whose `recipient_account` matches the
/// account of some coin in the global coin map. Used by
/// [`classify_incoming_payment`] to decide between Matched / Received
/// / Unmatched.
pub open spec fn count_matched_memos(
    memos: Seq<MemoEntry>,
    coins: Map<(PurseId, u64), CoinRec>,
    j: nat,
) -> nat
    decreases j
{
    if j == 0 {
        0
    } else {
        let prev = count_matched_memos(memos, coins, (j - 1) as nat);
        let m = memos[(j - 1) as int];
        if exists|k: (PurseId, u64)|
            #[trigger] coins.dom().contains(k)
            && coins[k].account == m.recipient_account
        {
            prev + 1
        } else {
            prev
        }
    }
}

/// Synchronous classification of an incoming chain payment (Quint
/// `classifyIncomingPayment`, §8.8). Returns:
/// - `Unmatched`   if `memos` is empty or no memo matches a local coin.
/// - `Matched`     if every memo matches a local coin.
/// - `Received`    if some but not all memos match.
pub open spec fn classify_incoming_payment(
    memos: Seq<MemoEntry>,
    coins: Map<(PurseId, u64), CoinRec>,
) -> PaymentClassification {
    let n = memos.len();
    let matched = count_matched_memos(memos, coins, n);
    if n == 0 {
        PaymentClassification::Unmatched
    } else if matched == 0 {
        PaymentClassification::Unmatched
    } else if matched == n {
        PaymentClassification::Matched
    } else {
        PaymentClassification::Received
    }
}

/// Spec-only coin value. **Pilot scheme: `coin_value(exp) = exp + 1`**
/// — linear, monotone in `exp`, no overflow under any realistic `Vec`
/// size. Real semantics is `2^exp` (Quint `coinValue`); the spec for
/// that is `coin_value_pow2` below, kept parallel so the protocol's
/// design-faithful value model is documented even while the exec
/// arithmetic uses the pilot scheme. Switching exec to real `2^exp`
/// requires bounded-exponent invariants + saturating-`u64` (or `u128`)
/// arithmetic plumbing; tracked as a dedicated future stage.
pub open spec fn coin_value(exp: u8) -> nat {
    pow2_nat(exp as nat)
}

/// Recursive `2^exp` over `nat`. Used by `coin_value_pow2`.
pub open spec fn pow2_nat(exp: nat) -> nat
    decreases exp
{
    if exp == 0 { 1 } else { 2 * pow2_nat((exp - 1) as nat) }
}

/// Spec-only **real** coin value (Quint `coinValue`). `2^exp` per the
/// design. Available as a parallel definition; not yet wired to the
/// exec arithmetic.
pub open spec fn coin_value_pow2(exp: u8) -> nat {
    pow2_nat(exp as nat)
}

/// Lexicographic priority comparison for two coins (Quint §6.3
/// `coinOrderLT`). Returns true if `a` has *higher* priority than `b`
/// (smaller rank tuple). The rank tuple is `(MaxExp - exp, MaxAge - age,
/// idx)` — bigger exponent wins, then older (smaller age), then
/// smaller idx as tiebreaker.
pub open spec fn coin_priority_lt(a: CoinRec, b: CoinRec) -> bool {
    a.exponent > b.exponent
        || (a.exponent == b.exponent && a.age < b.age)
        || (a.exponent == b.exponent && a.age == b.age && a.idx < b.idx)
}

/// Lexicographic priority comparison for two entries (Quint §6.3
/// `entryOrderLT`). Returns true if `a` has *higher* priority than
/// `b`. The rank tuple is `(MaxExp - exp, ring_idx, idx)` — bigger
/// exponent wins, then smaller ring_idx, then smaller idx.
pub open spec fn entry_priority_lt(a: EntryRec, b: EntryRec) -> bool {
    a.exponent > b.exponent
        || (a.exponent == b.exponent && a.ring_idx < b.ring_idx)
        || (a.exponent == b.exponent && a.ring_idx == b.ring_idx
            && a.idx < b.idx)
}

/// Spec-only recursive sum: total spendable value across `v[0..j]`
/// among coins that are `Available` and belong to purse `p`.
pub open spec fn sum_avail_prefix(v: Seq<CoinRec>, p: PurseId, j: nat) -> nat
    decreases j
{
    if j == 0 {
        0
    } else {
        let prev = sum_avail_prefix(v, p, (j - 1) as nat);
        if v[(j - 1) as int].purse == p
            && v[(j - 1) as int].state == CoinState::Available
        {
            prev + coin_value(v[(j - 1) as int].exponent)
        } else {
            prev
        }
    }
}

/// Spec-only recursive sum: total spendable value across `v[0..j]`
/// using the **real** Quint coin value `2^exp` (Quint `coinValue`).
/// Companion to `sum_avail_prefix` (pilot scheme).
pub open spec fn sum_avail_real_prefix(v: Seq<CoinRec>, p: PurseId, j: nat) -> nat
    decreases j
{
    if j == 0 {
        0
    } else {
        let prev = sum_avail_real_prefix(v, p, (j - 1) as nat);
        if v[(j - 1) as int].purse == p
            && v[(j - 1) as int].state == CoinState::Available
        {
            prev + coin_value_pow2(v[(j - 1) as int].exponent)
        } else {
            prev
        }
    }
}

/// Spec-only recursive sum: total pending entry value across `v[0..j]`
/// among entries that belong to purse `p`, are `LocalAvailable`, and
/// are either `Waiting` or `Missing` on-chain (Quint `pursePending`).
pub open spec fn sum_pending_prefix(v: Seq<EntryRec>, p: PurseId, j: nat) -> nat
    decreases j
{
    if j == 0 {
        0
    } else {
        let prev = sum_pending_prefix(v, p, (j - 1) as nat);
        let e = v[(j - 1) as int];
        if e.purse == p
            && e.local == EntryLocal::LocalAvailable
            && (e.on_chain == EntryOnChain::Waiting
                || e.on_chain == EntryOnChain::Missing)
        {
            prev + coin_value(e.exponent)
        } else {
            prev
        }
    }
}

/// Real-value (2^exp) variant of [`sum_pending_prefix`].
pub open spec fn sum_pending_real_prefix(v: Seq<EntryRec>, p: PurseId, j: nat) -> nat
    decreases j
{
    if j == 0 {
        0
    } else {
        let prev = sum_pending_real_prefix(v, p, (j - 1) as nat);
        let e = v[(j - 1) as int];
        if e.purse == p
            && e.local == EntryLocal::LocalAvailable
            && (e.on_chain == EntryOnChain::Waiting
                || e.on_chain == EntryOnChain::Missing)
        {
            prev + coin_value_pow2(e.exponent)
        } else {
            prev
        }
    }
}

/// Spec-only recursive sum: total ready entry value across `v[0..j]`
/// among entries that belong to purse `p`, are `LocalAvailable`, and
/// are `Ready` on-chain. Used by the strict-spendable aggregation
/// (Quint `purseSpendableStrict`'s entry component).
pub open spec fn sum_ready_prefix(v: Seq<EntryRec>, p: PurseId, j: nat) -> nat
    decreases j
{
    if j == 0 {
        0
    } else {
        let prev = sum_ready_prefix(v, p, (j - 1) as nat);
        let e = v[(j - 1) as int];
        if e.purse == p
            && e.local == EntryLocal::LocalAvailable
            && e.on_chain == EntryOnChain::Ready
        {
            prev + coin_value(e.exponent)
        } else {
            prev
        }
    }
}

/// Real-value (2^exp) variant of [`sum_ready_prefix`].
pub open spec fn sum_ready_real_prefix(v: Seq<EntryRec>, p: PurseId, j: nat) -> nat
    decreases j
{
    if j == 0 {
        0
    } else {
        let prev = sum_ready_real_prefix(v, p, (j - 1) as nat);
        let e = v[(j - 1) as int];
        if e.purse == p
            && e.local == EntryLocal::LocalAvailable
            && e.on_chain == EntryOnChain::Ready
        {
            prev + coin_value_pow2(e.exponent)
        } else {
            prev
        }
    }
}

/// Spec-only sum of coin values across a sequence of keys, looked up
/// in the coin map. Used to describe selection results.
pub open spec fn sum_of_coin_values(
    coins: Map<(PurseId, u64), CoinRec>,
    keys: Seq<(PurseId, u64)>,
) -> nat
    decreases keys.len()
{
    if keys.len() == 0 {
        0
    } else {
        let last_idx = (keys.len() - 1) as int;
        let last_key = keys[last_idx];
        let rest = sum_of_coin_values(coins, keys.subrange(0, last_idx));
        if coins.dom().contains(last_key) {
            rest + coin_value(coins[last_key].exponent)
        } else {
            rest
        }
    }
}

} // verus!
