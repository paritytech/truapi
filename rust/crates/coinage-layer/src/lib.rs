//! Verus translation of the Coinage Layer Quint specification.
//!
//! Source-of-truth references:
//!   - Quint spec  : `docs/specs/coinage-layer.qnt`
//!   - Design doc  : `docs/design/coinage-layer.md`
//!
//! **Scope.** Verified protocol kernel covering the four core state
//! components — purses, coins, recycler entries, operations — with
//! their lifecycle transitions and the §6.3 priority order. Chain
//! interaction is abstracted: chain-side state changes arrive via
//! caller-driven primitives (`set_entry_on_chain`, `mark_op_finalized`,
//! …) rather than being modeled directly. No persistence, no crypto;
//! `member_key` / `account` / chain timestamps are `u64` placeholders
//! supplied by the host.
//!
//! **What's in.** Per-purse and per-coin and per-entry allocators
//! with overflow-safe contracts; full `OpStatus` phase order
//! (Preparing → Submitted → InBlock → Finalized → (Waiting →)? Done
//! | Failed) with typed transition wrappers; per-key lock/release/
//! commit primitives; six `tracked_*` lifecycle wrappers (transfer,
//! rebalance, top-up-via-entry, unload-via-entry, export, import);
//! atomic composites for kick-off (`start_op_locking_{coin,entry}`),
//! cancel (`cancel_op_releasing_{coin,entry}`), and commit
//! (`commit_op_consuming_locked_{coin,entry}`); aggregations for
//! `query_purse.{spendable, spendable_strict, pending}`; spec + exec
//! for `classify_incoming_payment`; spec + exec for the §6.3 coin
//! and entry priority orders.
//!
//! **What's deferred.** Real `2^exp` arithmetic (pilot uses
//! `coin_value(exp) = exp + 1`); cross-state lock referential-
//! integrity invariant; bulk-sweep `cancel_op` (the per-key release
//! primitives are available); multi-coin tier-1 exact subset-sum
//! exec; tier-3 entry-supplemented cover exec; the events Vec;
//! recovery flow; fee account and unload tokens.
//!
//! **Encoding.** Exec storage is `Vec<…Rec>` per component. Contracts
//! quantify over ghost spec maps (`Ghost<Map<key, Rec>>`). The
//! invariant ties them: every Vec entry is in the ghost map under
//! its key; every ghost-map key has a matching Vec entry; no
//! duplicates. State-mutating methods explicitly preserve untouched
//! components (`final.next_handle == old.next_handle`, …) in their
//! contracts — Verus's `&mut self` SMT encoding doesn't carry these
//! over for free.

use vstd::prelude::*;

verus! {

/// Stable purse identifier (Quint `PurseId`, design §3.1).
pub type PurseId = u64;

/// Reserved identifier of the main purse (Quint `MAIN_PURSE`).
pub const MAIN_PURSE: PurseId = 0;

/// Maximum coin exponent (Quint `MaxExponent`). The pilot scheme
/// `coin_value(exp) = exp + 1` requires no specific upper bound, but
/// the Quint spec caps exponents at this value to keep the design's
/// `2^exp` arithmetic in u64. Callers should reject creation requests
/// with `exponent > MAX_EXPONENT`.
pub const MAX_EXPONENT: u8 = 30;

/// Anonymity-floor jitter window (Quint `JitterMax`). After a top-up
/// entry is allocated, the chain takes between 0 and `JITTER_MAX`
/// blocks before it can be promoted to `Ready`. Hosts use this to
/// compute `ready_at = allocated_at + JITTER_MAX`.
pub const JITTER_MAX: u64 = 16;

/// Gap-limit batch size for recovery scans (Quint `BatchSize`). A
/// recovery scan iterates through coin/entry indices in batches of
/// this many slots; if every slot in `GAP_LIMIT` consecutive batches
/// is empty, the scan terminates.
pub const RECOVERY_BATCH_SIZE: u64 = 8;

/// Number of consecutive empty batches that terminate a recovery scan
/// (Quint `GapLimit`).
pub const RECOVERY_GAP_LIMIT: u64 = 4;

/// Executable purse record (mirrors Quint `PurseRec`, spec lines 89-94).
pub struct PurseRec {
    pub id: PurseId,
    pub name: Vec<u8>,
    pub next_coin_idx: u64,
    pub next_entry_idx: u64,
}

/// Spec-level twin of `PurseRec` used in contracts.
pub struct PurseRecSpec {
    pub id: PurseId,
    pub name: Seq<u8>,
    pub next_coin_idx: nat,
    pub next_entry_idx: nat,
}

impl PurseRec {
    /// Lift an executable record into its spec twin.
    pub open spec fn view(&self) -> PurseRecSpec {
        PurseRecSpec {
            id: self.id,
            name: self.name@,
            next_coin_idx: self.next_coin_idx as nat,
            next_entry_idx: self.next_entry_idx as nat,
        }
    }
}

/// Coin lifecycle state (Quint `CoinState`).
///   * `Pending` — coin has been allocated but is not yet observed as
///     existing on chain. Cannot be selected.
///   * `Available` — observed on chain; eligible for selection.
///   * `LockedFor(handle)` — coin has been reserved by operation `handle`;
///     can be released back to `Available` (cancel) or advanced to
///     `PendingSpend` (commit).
///   * `PendingSpend` — coin has been chosen by an in-flight operation.
///   * `Spent` — coin is terminally consumed; counts neither for selection
///     nor as "live" for purse-deletion purposes.
pub type OpHandle = u64;

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum CoinState {
    Pending,
    Available,
    LockedFor(OpHandle),
    PendingSpend,
    Spent,
}

/// Coin record (Quint `CoinRec`, design §3.2). `age` is the monotonic
/// allocation timestamp used by the §6.3 priority ordering — older
/// coins (smaller `age`) outrank newer ones at equal exponent.
/// `account` is the chain-account identifier the coin lives under.
/// In this pilot it is a `u64` placeholder set to 0 on allocation;
/// account-aware operations (top-up funding origin, transfer destination)
/// will populate it once the chain abstraction lands.
#[derive(Copy, Clone)]
pub struct CoinRec {
    pub purse: PurseId,
    pub idx: u64,
    pub exponent: u8,
    pub state: CoinState,
    pub age: u64,
    pub account: u64,
}

/// Recycler entry on-chain state (Quint `EntryOnChain`, design §5.2).
/// The `OnDegraded` payload is omitted in the pilot (it carries a
/// post-submission detection epoch in the design).
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum EntryOnChain {
    Missing,
    Waiting,
    Ready,
    Degraded,
}

/// Recycler entry local-side state (Quint `EntryLocal`, design §5.4).
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum EntryLocal {
    LocalAvailable,
    LocalLockedFor(OpHandle),
    LocalConsumed,
}

/// Recycler entry record (Quint `EntryRec`, design §3.3).
///
/// Recycler entry record (Quint `EntryRec`, design §5.2). Carries the
/// chain-side bookkeeping fields needed by the §6.3 selection ordering
/// and the §8 lifecycle:
/// - `member_key` — ring-membership identifier (`u64` placeholder).
/// - `allocated_at` — block height when the entry was reserved.
/// - `ready_at` — block height when the anonymity floor was reached.
/// - `ring_idx` — index within the anonymity ring; used as the
///   tiebreaker between equal-exponent entries by §6.3
///   `entryPriorityRank`.
#[derive(Copy, Clone)]
pub struct EntryRec {
    pub purse: PurseId,
    pub idx: u64,
    pub exponent: u8,
    pub on_chain: EntryOnChain,
    pub local: EntryLocal,
    pub member_key: u64,
    pub allocated_at: u64,
    pub ready_at: u64,
    pub ring_idx: u64,
}

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

/// Operation kind (Quint `OpKind`, design §3.4). Each kind drives a
/// distinct top-level operation flavor; `OpStatus` then walks every
/// kind through the same lifecycle (Preparing → Submitted → InBlock →
/// Finalized → Done | Failed).
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum OpKind {
    Transfer,
    TopUp,
    Rebalance,
    DeletePurse,
    ExternalOffload,
    Export,
    Import,
    Maintenance,
    Recover,
}

/// Operation status (Quint `OpStatus`, design §5.5). Mirrors the full
/// Quint phase order Preparing → Submitted → InBlock → Finalized →
/// (Waiting →)? Done, with `Failed` reachable from any pre-terminal
/// state. The `Waiting(t)` arm carries a `u64` placeholder for the
/// Quint `Time` payload (entry-ready timestamp).
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum OpStatus {
    Preparing,
    Submitted,
    InBlock,
    Finalized,
    Waiting(u64),
    Done,
    Failed,
}

/// Operation record (Quint `OperationRec`). Pilot scope: handle, kind,
/// status, owning purse. The Quint record also carries `lockedCoins`
/// and `lockedEntries` sets — deferred until cross-state locking lands.
#[derive(Copy, Clone)]
pub struct OperationRec {
    pub handle: OpHandle,
    pub kind: OpKind,
    pub purse: PurseId,
    pub status: OpStatus,
}

/// Incoming-payment memo entry (Quint `MemoEntry`, §8.3). The layer
/// treats memos opaquely; only `recipient_account` is used by
/// `classify_incoming_payment`.
#[derive(Copy, Clone)]
pub struct MemoEntry {
    pub sender_account: u64,
    pub recipient_account: u64,
    pub derivation_index: u64,
}

/// Classification of an incoming chain payment (Quint
/// `PaymentClassification`, §8.8).
///
/// - `Matched`: every memo's recipient is a known local coin account.
///   The payment is fully accounted for by existing coins.
/// - `Received`: some — but not all — memos match local coins. The
///   recipient has new funds beyond what's locally tracked.
/// - `Unmatched`: no memos match (or the list is empty). The payment
///   isn't for this host or originates from an unknown sender.
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum PaymentClassification {
    Matched,
    Received,
    Unmatched,
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

/// Single-coin selection result (§6.3 single-coin tier-1 / tier-2 cases).
/// `Exact` is the design's tier-1 single-coin form (coin value matches
/// the requested amount). `Split` is the tier-2 form (coin value
/// strictly exceeds the amount; caller must split the coin and emit
/// change). Multi-coin tier-1 selections and tier-3 entry-supplemented
/// selections will be carried by separate variants when their exec
/// paths land.
pub enum CoinSelection {
    Exact { coin: (PurseId, u64) },
    Split { coin: (PurseId, u64) },
}

/// Result of a bounded subset-sum search over `Available` coins:
/// either a single coin, a pair, a triple, or a quadruple of distinct
/// coin keys whose values sum exactly to the requested amount. Returned
/// by [`State::find_subset_sum_up_to_4`].
pub enum SubsetSumCover {
    One((PurseId, u64)),
    Two((PurseId, u64), (PurseId, u64)),
    Three((PurseId, u64), (PurseId, u64), (PurseId, u64)),
    Four((PurseId, u64), (PurseId, u64), (PurseId, u64), (PurseId, u64)),
}

/// Snapshot returned by `query_purse` (design §8.1 `PurseInfo`).
/// Pilot scope: `spendable`, `spendable_strict`, `pending` are always 0
/// (no coins/entries in state yet).
pub struct PurseInfo {
    pub id: PurseId,
    pub name: Vec<u8>,
    pub spendable: u64,
    pub spendable_strict: u64,
    pub pending: u64,
}

/// Layer error enum (design §10). String payloads are modeled as
/// `Vec<u8>` for Verus-compat; `ExtrinsicHash` is a `u64` placeholder.
/// `OperationHandle` is a `u64` placeholder.
pub enum Error {
    // Pre-submission
    PurseNotFound(PurseId),
    OperationNotFound(u64),
    CannotDeleteMainPurse,
    PurseHasInFlightOperations,
    OutputsDoNotSumToAmount,
    InsufficientFunds { requested: u64, available: u64 },
    InsufficientExternalFunds,
    NoReadyEntries { requested: u64, available_when_ready: u64 },
    NoUnloadToken,
    BadCoinSecret,
    // Post-submission / chain
    SnipedCoin,
    ChainRejected { extrinsic_hash: u64, reason: Vec<u8> },
    // Lifecycle
    Cancelled,
    InterruptedPreSubmission,
    // Internal
    StorageError(Vec<u8>),
    SubscriptionError(Vec<u8>),
    RecoveryFailed(Vec<u8>),
    Internal(Vec<u8>),
}

/// Layer state. Pilot scope: purses only.
///
/// Fields are public so that the `open spec fn` accessors can read them at
/// call sites outside this crate (Verus treats any struct with even one
/// private field as fully opaque externally). External writes to these
/// fields will break the invariant, which makes any further method call
/// reject via `requires`; the invariant remains the only valid entry point.
pub struct State {
    pub purses: Vec<PurseRec>,
    pub coins: Vec<CoinRec>,
    pub entries: Vec<EntryRec>,
    pub operations: Vec<OperationRec>,
    pub next_purse_id: u64,
    pub next_handle: OpHandle,
    pub next_age: u64,
    /// Quint `feeAccountBalance`. Reservoir of pre-paid chain-fee funds.
    pub fee_balance: u64,
    /// Quint `nextExtrinsicId`. Monotonically increasing counter for
    /// chain-extrinsic identifiers — bumped by every chain-bound op
    /// when its extrinsic is broadcast (Submitted transition).
    pub next_extrinsic_id: u64,
    /// Quint event stream. Append-only sequence of observations. Hosts
    /// consume this for UI notifications, test assertions, and audit
    /// trails. Every state-mutating op declares its emissions in its
    /// postcondition.
    pub events: Vec<Event>,
    /// Quint `paidRingMembership`. Total amount paid for anonymity-ring
    /// membership fees — accumulated as top-ups land.
    pub paid_ring_membership: u64,
    /// Quint `totalIn`. Total amount of funds that have entered the
    /// system (top-ups, imports). Monotonically non-decreasing.
    pub total_in: u64,
    /// Quint `totalOut`. Total amount of funds that have exited the
    /// system (transfers out, exports). Monotonically non-decreasing.
    pub total_out: u64,
    /// Quint `tokens`. Vec of unload tokens; indexed by allocation
    /// order. The chain mints these (with `consumed: false`); the
    /// layer marks consumed when the corresponding unload op commits.
    pub tokens: Vec<UnloadToken>,
    /// Quint `chainCoins`. Mirror of on-chain coin state, used by the
    /// gap-limit recovery scan to rebuild local `coins` after partial
    /// state loss. The chain side acts as the source of truth.
    pub chain_coins: Vec<CoinRec>,
    /// Quint `chainEntries`. Mirror of on-chain entry state.
    pub chain_entries: Vec<EntryRec>,
    #[allow(dead_code)]
    pub spec_purses: Ghost<Map<PurseId, PurseRecSpec>>,
    #[allow(dead_code)]
    pub spec_coins: Ghost<Map<(PurseId, u64), CoinRec>>,
    #[allow(dead_code)]
    pub spec_entries: Ghost<Map<(PurseId, u64), EntryRec>>,
    #[allow(dead_code)]
    pub spec_operations: Ghost<Map<OpHandle, OperationRec>>,
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

/// Quint `FeeMode`. The layer picks automatically: prepaid if the fee
/// account has funds, from-output otherwise.
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum FeeMode {
    Prepaid,
    FromOutput,
}

/// Quint `UnloadTokenClass`. Free tokens are granted by the chain;
/// paid tokens come from the fee account or from-output.
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum UnloadTokenClass {
    Free,
    Paid,
}

/// Quint `UnloadToken` (design §6.5). Identifies a single unload
/// authorization. The chain tracks `consumed` flags; the layer
/// mirrors them.
#[derive(Copy, Clone)]
pub struct UnloadToken {
    pub period: u64,
    pub class: UnloadTokenClass,
    pub counter: u64,
    pub consumed: bool,
}

/// Layer-level event (Quint `Event`, design §11). Append-only stream
/// of observations consumed by host UIs and tests. Each state-mutating
/// op declares its emissions in its contract; queries emit nothing.
#[derive(Copy, Clone)]
pub enum Event {
    CoinAvailable { purse: PurseId, exponent: u8 },
    CoinSpent { purse: PurseId, exponent: u8 },
    EntryAllocated { purse: PurseId, exponent: u8 },
    EntryReadinessChanged { purse: PurseId, exponent: u8, new_state: EntryOnChain },
    EntryConsumed { purse: PurseId, exponent: u8 },
    OperationStarted { handle: OpHandle, kind: OpKind, purse: PurseId },
    OperationProgress { handle: OpHandle, status: OpStatus },
    OperationCompleted { handle: OpHandle, status: OpStatus },
}

/// Spec-only lemma: `pow2_nat` is monotone (non-decreasing). Proved by
/// straightforward induction on the exponent.
pub proof fn lemma_pow2_monotone(e1: nat, e2: nat)
    requires
        e1 <= e2,
    ensures
        pow2_nat(e1) <= pow2_nat(e2),
    decreases e2,
{
    if e2 == 0 {
        // e1 == 0 too; trivially equal.
    } else if e1 == e2 {
        // trivial
    } else {
        lemma_pow2_monotone(e1, (e2 - 1) as nat);
    }
}

/// Spec-only lemma: `pow2_nat(30) == 2^30 = 1073741824`. Unrolled
/// once-per-step (Verus's default fuel is 1, so a single recursive
/// step). Used to derive the u64-overflow-safety bound for
/// `pow2_u64_exec`.
pub proof fn lemma_pow2_at_30()
    ensures
        pow2_nat(30) == 1073741824nat,
{
    reveal_with_fuel(pow2_nat, 31);
}

/// Executable real coin value (Quint `coinValue`): `2^exp` for
/// `exp <= MAX_EXPONENT`. Thin convenience wrapper over
/// [`pow2_u64_exec`] that matches the `coin_value_pow2` spec fn.
pub fn coin_value_pow2_exec(exp: u8) -> (res: u64)
    requires
        exp <= MAX_EXPONENT,
    ensures
        res as nat == coin_value_pow2(exp),
{
    pow2_u64_exec(exp)
}

/// Executable `2^exp` for `exp <= MAX_EXPONENT` (= 30). Returns the
/// real Quint `coinValue` for that exponent. Verus-verified
/// overflow-safe: `MAX_EXPONENT = 30 ⇒ 2^30 < u64::MAX`.
///
/// This is the foundational primitive for switching the pilot's
/// `coin_value(exp) = exp + 1` scheme over to real `2^exp` arithmetic
/// (task #84). Existing aggregations still use the pilot scheme — this
/// just gives callers (and a future rewrite) the safe building block.
pub fn pow2_u64_exec(exp: u8) -> (res: u64)
    requires
        exp <= MAX_EXPONENT,
    ensures
        res as nat == pow2_nat(exp as nat),
        res <= 1073741824u64,
{
    let mut result: u64 = 1;
    let mut k: u8 = 0;
    while k < exp
        invariant
            k <= exp,
            exp <= MAX_EXPONENT,
            result as nat == pow2_nat(k as nat),
            result <= 1073741824u64,
        decreases exp - k,
    {
        proof {
            // Bound `result * 2` by 2^30 = 1073741824 to keep within u64.
            // After this iteration, k+1 <= exp <= 30, so
            // pow2(k+1) <= pow2(30) = 2^30.
            lemma_pow2_at_30();
            lemma_pow2_monotone((k as nat) + 1, MAX_EXPONENT as nat);
        }
        result = result * 2;
        k = k + 1;
    }
    result
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

impl State {
    /// Spec view of the purse map.
    pub open spec fn purses(&self) -> Map<PurseId, PurseRecSpec> {
        self.spec_purses@
    }

    /// Spec view of the coin map.
    pub open spec fn coins(&self) -> Map<(PurseId, u64), CoinRec> {
        self.spec_coins@
    }

    /// Spec view of the recycler-entry map.
    pub open spec fn entries(&self) -> Map<(PurseId, u64), EntryRec> {
        self.spec_entries@
    }

    /// Spec view of the operations map.
    pub open spec fn operations(&self) -> Map<OpHandle, OperationRec> {
        self.spec_operations@
    }

    /// True iff some coin currently lives in purse `p`.
    pub open spec fn has_coin_in(&self, p: PurseId) -> bool {
        exists|k: (PurseId, u64)| #[trigger] self.coins().dom().contains(k) && k.0 == p
    }

    /// True iff some *live* (non-`Spent`) coin currently lives in purse `p`.
    pub open spec fn has_live_coin_in(&self, p: PurseId) -> bool {
        exists|k: (PurseId, u64)|
            #[trigger] self.coins().dom().contains(k)
                && k.0 == p
                && self.coins()[k].state != CoinState::Spent
    }

    /// Whether the allocator can still mint a fresh `PurseId`.
    pub open spec fn has_create_capacity(&self) -> bool {
        self.next_purse_id < u64::MAX
    }

    /// State well-formedness. Combines:
    ///   (a) ghost-map well-formedness (dom keys agree with `id` fields,
    ///       all ids below `next_purse_id`, MAIN_PURSE present),
    ///   (b) exec/spec refinement (Vec contents and ghost-map dom in
    ///       1-to-1 correspondence, no duplicates).
    pub open spec fn invariant(&self) -> bool {
        let m = self.spec_purses@;
        let v = self.purses@;
        &&& self.next_purse_id != MAIN_PURSE
        &&& m.dom().contains(MAIN_PURSE)
        &&& forall|p: PurseId| #[trigger] m.dom().contains(p) ==> m[p].id == p
        &&& forall|p: PurseId| #[trigger] m.dom().contains(p) ==> p < self.next_purse_id
        // exec → ghost: every Vec entry is in the map under its own id
        &&& forall|i: int| 0 <= i < v.len() ==> #[trigger] m.dom().contains(v[i].id)
        &&& forall|i: int| 0 <= i < v.len() ==> m[(#[trigger] v[i]).id] == v[i]@
        // ghost → exec: every map key has a matching Vec entry
        &&& forall|p: PurseId| #[trigger] m.dom().contains(p)
              ==> exists|i: int| 0 <= i < v.len() && #[trigger] v[i].id == p
        // no duplicate ids in the Vec
        &&& forall|i: int, j: int|
              0 <= i < v.len() && 0 <= j < v.len()
              && #[trigger] v[i].id == #[trigger] v[j].id ==> i == j
        // (i) coin key consistency: keyed by (purse, idx), record matches.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_coins@.dom().contains(k)
              ==> self.spec_coins@[k].purse == k.0 && self.spec_coins@[k].idx == k.1
        // (j) coin referential integrity: every coin's purse is a known purse.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_coins@.dom().contains(k)
              ==> m.dom().contains(k.0)
        // (k) coin idx is below the owning purse's allocator. Ensures
        //     `purses[p].next_coin_idx` is always a fresh coin index for p.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_coins@.dom().contains(k)
              ==> k.1 < m[k.0].next_coin_idx
        // (l) exec coin Vec → ghost: every Vec entry's (purse, idx) is in dom
        //     and matches the ghost record.
        &&& forall|i: int| 0 <= i < self.coins@.len() ==>
              #[trigger] self.spec_coins@.dom().contains(
                  (self.coins@[i].purse, self.coins@[i].idx)
              )
        &&& forall|i: int| 0 <= i < self.coins@.len() ==>
              self.spec_coins@[(#[trigger] self.coins@[i].purse, self.coins@[i].idx)]
                == self.coins@[i]
        // (m) ghost coin map → exec: every dom key has a Vec witness.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_coins@.dom().contains(k)
              ==> exists|i: int|
                    0 <= i < self.coins@.len()
                    && #[trigger] self.coins@[i].purse == k.0
                    && self.coins@[i].idx == k.1
        // (n) no duplicate (purse, idx) keys in the coin Vec.
        &&& forall|i: int, j: int|
              0 <= i < self.coins@.len() && 0 <= j < self.coins@.len()
              && (#[trigger] self.coins@[i]).purse == (#[trigger] self.coins@[j]).purse
              && self.coins@[i].idx == self.coins@[j].idx
              ==> i == j
        // (o) entry key consistency.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_entries@.dom().contains(k)
              ==> self.spec_entries@[k].purse == k.0
                  && self.spec_entries@[k].idx == k.1
        // (p) entry referential integrity: every entry's purse is in dom.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_entries@.dom().contains(k)
              ==> m.dom().contains(k.0)
        // (q) entry idx is below the owning purse's allocator.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_entries@.dom().contains(k)
              ==> k.1 < m[k.0].next_entry_idx
        // (r) exec entry Vec → ghost: every Vec entry's (purse, idx) is in dom
        //     and matches the ghost record.
        &&& forall|i: int| 0 <= i < self.entries@.len() ==>
              #[trigger] self.spec_entries@.dom().contains(
                  (self.entries@[i].purse, self.entries@[i].idx)
              )
        &&& forall|i: int| 0 <= i < self.entries@.len() ==>
              self.spec_entries@[(#[trigger] self.entries@[i].purse, self.entries@[i].idx)]
                == self.entries@[i]
        // (s) ghost entry map → exec: every dom key has a Vec witness.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_entries@.dom().contains(k)
              ==> exists|i: int|
                    0 <= i < self.entries@.len()
                    && #[trigger] self.entries@[i].purse == k.0
                    && self.entries@[i].idx == k.1
        // (t) no duplicate (purse, idx) keys in the entry Vec.
        &&& forall|i: int, j: int|
              0 <= i < self.entries@.len() && 0 <= j < self.entries@.len()
              && (#[trigger] self.entries@[i]).purse == (#[trigger] self.entries@[j]).purse
              && self.entries@[i].idx == self.entries@[j].idx
              ==> i == j
        // (u) operation key consistency: spec_operations[h].handle == h.
        &&& forall|h: OpHandle| #[trigger] self.spec_operations@.dom().contains(h)
              ==> self.spec_operations@[h].handle == h
        // (v) handle below allocator.
        &&& forall|h: OpHandle| #[trigger] self.spec_operations@.dom().contains(h)
              ==> h < self.next_handle
        // (w) operation refint to purses.
        &&& forall|h: OpHandle| #[trigger] self.spec_operations@.dom().contains(h)
              ==> m.dom().contains(self.spec_operations@[h].purse)
        // (x) exec operations Vec → ghost.
        &&& forall|i: int| 0 <= i < self.operations@.len() ==>
              #[trigger] self.spec_operations@.dom().contains(self.operations@[i].handle)
        &&& forall|i: int| 0 <= i < self.operations@.len() ==>
              self.spec_operations@[(#[trigger] self.operations@[i]).handle]
                == self.operations@[i]
        // (y) ghost → exec.
        &&& forall|h: OpHandle| #[trigger] self.spec_operations@.dom().contains(h)
              ==> exists|i: int|
                    0 <= i < self.operations@.len()
                    && #[trigger] self.operations@[i].handle == h
        // (z) no duplicate handles in operations Vec.
        &&& forall|i: int, j: int|
              0 <= i < self.operations@.len() && 0 <= j < self.operations@.len()
              && (#[trigger] self.operations@[i]).handle
                  == (#[trigger] self.operations@[j]).handle
              ==> i == j
        // (aa) every coin's exponent is bounded by MAX_EXPONENT. Foundation
        //      for real `2^exp` arithmetic safety (pow2_u64_exec(exp) doesn't
        //      overflow u64 only when exp <= 30 = MAX_EXPONENT).
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_coins@.dom().contains(k)
              ==> self.spec_coins@[k].exponent <= MAX_EXPONENT
        // (ab) every entry's exponent is bounded by MAX_EXPONENT.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_entries@.dom().contains(k)
              ==> self.spec_entries@[k].exponent <= MAX_EXPONENT
        // (ac) every chain-mirror coin's exponent is bounded too. This lets
        //      restore_chain_coin reconstruct local state without losing the
        //      exponent bound.
        &&& forall|i: int| 0 <= i < self.chain_coins@.len()
              ==> (#[trigger] self.chain_coins@[i]).exponent <= MAX_EXPONENT
        // (ad) every chain-mirror entry's exponent is bounded.
        &&& forall|i: int| 0 <= i < self.chain_entries@.len()
              ==> (#[trigger] self.chain_entries@[i]).exponent <= MAX_EXPONENT
    }

    /// Initialize the layer with only the main purse and an empty coin map.
    pub fn init() -> (s: State)
        ensures
            s.invariant(),
            s.purses().dom() =~= set![MAIN_PURSE],
            s.purses()[MAIN_PURSE] == (PurseRecSpec {
                id: MAIN_PURSE,
                name: Seq::empty(),
                next_coin_idx: 0,
                next_entry_idx: 0,
            }),
            s.coins().dom() =~= Set::<(PurseId, u64)>::empty(),
            lock_refint(s.coins(), s.entries(), s.operations()),
    {
        let main_rec = PurseRec {
            id: MAIN_PURSE,
            name: Vec::new(),
            next_coin_idx: 0,
            next_entry_idx: 0,
        };
        let ghost main_spec = main_rec@;
        let mut purses: Vec<PurseRec> = Vec::new();
        purses.push(main_rec);
        let coins: Vec<CoinRec> = Vec::new();
        let entries: Vec<EntryRec> = Vec::new();
        let operations: Vec<OperationRec> = Vec::new();
        let s = State {
            purses,
            coins,
            entries,
            operations,
            next_purse_id: 1,
            next_handle: 0,
            next_age: 0,
            fee_balance: 0,
            next_extrinsic_id: 0,
            events: Vec::new(),
            paid_ring_membership: 0,
            total_in: 0,
            total_out: 0,
            tokens: Vec::new(),
            chain_coins: Vec::new(),
            chain_entries: Vec::new(),
            spec_purses: Ghost(Map::<PurseId, PurseRecSpec>::empty().insert(MAIN_PURSE, main_spec)),
            spec_coins: Ghost(Map::<(PurseId, u64), CoinRec>::empty()),
            spec_entries: Ghost(Map::<(PurseId, u64), EntryRec>::empty()),
            spec_operations: Ghost(Map::<OpHandle, OperationRec>::empty()),
        };
        assert(s.purses@.len() == 1);
        assert(s.purses@[0].id == MAIN_PURSE);
        assert(s.spec_purses@.dom() =~= set![MAIN_PURSE]);
        s
    }

    /// 6.1 `createPurse` (Quint lines 393-420; design §8.1 `create_purse`).
    ///
    /// Allocates a fresh `PurseId != MAIN_PURSE`, persists a new purse with
    /// the given `name`, returns the assigned id. Synchronous; no chain
    /// interaction.
    pub fn create_purse(&mut self, name: Vec<u8>) -> (new_id: PurseId)
        requires
            old(self).invariant(),
            old(self).has_create_capacity(),
        ensures
            final(self).invariant(),
            new_id != MAIN_PURSE,
            !old(self).purses().dom().contains(new_id),
            final(self).purses() == old(self).purses().insert(new_id, PurseRecSpec {
                id: new_id,
                name: name@,
                next_coin_idx: 0,
                next_entry_idx: 0,
            }),
    {
        let new_id = self.next_purse_id;
        let ghost old_v = self.purses@;
        let ghost old_m = self.spec_purses@;
        let rec = PurseRec {
            id: new_id,
            name,
            next_coin_idx: 0,
            next_entry_idx: 0,
        };
        let ghost rec_spec = rec@;

        // Every existing Vec entry's id is < new_id.
        proof {
            assert forall|i: int| 0 <= i < old_v.len() implies
                #[trigger] old_v[i].id < new_id
            by {
                assert(old_m.dom().contains(old_v[i].id));
            }
        }

        self.purses.push(rec);
        proof {
            self.spec_purses = Ghost(self.spec_purses@.insert(new_id, rec_spec));
        }
        self.next_purse_id = new_id + 1;

        proof {
            let new_v = self.purses@;
            let new_m = self.spec_purses@;
            let new_next = self.next_purse_id;

            // (a) next_purse_id != MAIN_PURSE
            assert(new_next != MAIN_PURSE);

            // (b) MAIN_PURSE in dom
            assert(new_m.dom().contains(MAIN_PURSE));

            // (c) forall p in dom. m[p].id == p
            assert forall|p: PurseId| #[trigger] new_m.dom().contains(p)
                implies new_m[p].id == p
            by {
                if p == new_id {
                    assert(new_m[new_id] == rec_spec);
                } else {
                    assert(old_m.dom().contains(p));
                }
            }

            // (d) forall p in dom. p < next_purse_id
            assert forall|p: PurseId| #[trigger] new_m.dom().contains(p)
                implies p < new_next
            by {
                if p == new_id {
                } else {
                    assert(old_m.dom().contains(p));
                }
            }

            // (e) every Vec entry's id is in dom
            assert(new_v == old_v.push(rec));
            assert forall|i: int| 0 <= i < new_v.len() implies
                new_m.dom().contains(#[trigger] new_v[i].id)
            by {
                if i < old_v.len() {
                    assert(new_v[i] == old_v[i]);
                    assert(old_m.dom().contains(old_v[i].id));
                } else {
                    assert(new_v[i].id == new_id);
                }
            }

            // (f) every Vec entry's spec view matches its dom entry
            assert forall|i: int| 0 <= i < new_v.len() implies
                new_m[(#[trigger] new_v[i]).id] == new_v[i]@
            by {
                if i < old_v.len() {
                    assert(new_v[i] == old_v[i]);
                    assert(old_v[i].id < new_id);
                    assert(old_m[old_v[i].id] == old_v[i]@);
                } else {
                    assert(new_v[i].id == new_id);
                    assert(new_v[i]@ == rec_spec);
                }
            }

            // (g) every dom key has a Vec witness
            assert forall|p: PurseId| #[trigger] new_m.dom().contains(p)
                implies exists|i: int| 0 <= i < new_v.len() && #[trigger] new_v[i].id == p
            by {
                if p == new_id {
                    let w = old_v.len() as int;
                    assert(0 <= w < new_v.len());
                    assert(new_v[w].id == new_id);
                } else {
                    assert(old_m.dom().contains(p));
                    let w = choose|i: int| 0 <= i < old_v.len() && #[trigger] old_v[i].id == p;
                    assert(new_v[w] == old_v[w]);
                }
            }

            // (h) no duplicates in Vec
            assert forall|i: int, j: int|
                0 <= i < new_v.len() && 0 <= j < new_v.len()
                && #[trigger] new_v[i].id == #[trigger] new_v[j].id
                implies i == j
            by {
                if i < old_v.len() && j < old_v.len() {
                } else if i == old_v.len() && j == old_v.len() {
                } else if i < old_v.len() {
                    assert(new_v[i] == old_v[i]);
                    assert(old_v[i].id < new_id);
                    assert(new_v[j].id == new_id);
                } else {
                    assert(new_v[j] == old_v[j]);
                    assert(old_v[j].id < new_id);
                    assert(new_v[i].id == new_id);
                }
            }
        }
        new_id
    }

    /// 6.1.1 `renamePurse` (Quint lines 422-452; design §8.1 `rename_purse`).
    ///
    /// Updates the purse's name. Synchronous; no chain interaction.
    /// Returns `Err(PurseNotFound(p))` if `p` is not a known purse; the state
    /// is unchanged in that case.
    pub fn rename_purse(&mut self, p: PurseId, name: Vec<u8>) -> (res: Result<(), Error>)
        requires
            old(self).invariant(),
        ensures
            final(self).invariant(),
            match res {
                Ok(()) => {
                    &&& old(self).purses().dom().contains(p)
                    &&& final(self).purses().dom() =~= old(self).purses().dom()
                    &&& final(self).purses()[p].id == p
                    &&& final(self).purses()[p].name == name@
                    &&& final(self).purses()[p].next_coin_idx
                          == old(self).purses()[p].next_coin_idx
                    &&& final(self).purses()[p].next_entry_idx
                          == old(self).purses()[p].next_entry_idx
                    &&& forall|q: PurseId| q != p && #[trigger] old(self).purses().dom().contains(q)
                          ==> final(self).purses()[q] == old(self).purses()[q]
                },
                Err(Error::PurseNotFound(q)) =>
                    !old(self).purses().dom().contains(p)
                    && q == p
                    && final(self).purses() == old(self).purses(),
                Err(_) => false,
            },
    {
        let ghost old_v = self.purses@;
        let ghost old_m = self.spec_purses@;
        let ghost name_seq = name@;

        let mut i: usize = 0;
        while i < self.purses.len()
            invariant
                0 <= i <= self.purses.len(),
                self.invariant(),
                self.purses@ == old_v,
                self.spec_purses@ == old_m,
                old_m == old(self).spec_purses@,
                old_v == old(self).purses@,
                name_seq == name@,
                self.next_purse_id == old(self).next_purse_id,
                forall|j: int| 0 <= j < i ==> (#[trigger] self.purses@[j]).id != p,
            decreases self.purses.len() - i,
        {
            if self.purses[i].id == p {
                let ghost target_idx = i as int;
                let ghost old_p_rec = old_v[target_idx]@;
                let cur_id = self.purses[i].id;
                let cur_cidx = self.purses[i].next_coin_idx;
                let cur_eidx = self.purses[i].next_entry_idx;
                let new_rec = PurseRec {
                    id: cur_id,
                    name,
                    next_coin_idx: cur_cidx,
                    next_entry_idx: cur_eidx,
                };
                let ghost new_rec_spec = new_rec@;
                self.purses[i] = new_rec;
                proof {
                    self.spec_purses = Ghost(self.spec_purses@.insert(p, new_rec_spec));

                    let new_v = self.purses@;
                    let new_m = self.spec_purses@;

                    // The mutated entry has the new spec view.
                    assert(new_v[target_idx]@ == new_rec_spec);
                    assert(new_v[target_idx].id == p);
                    // Off-index entries are unchanged.
                    assert forall|k: int| 0 <= k < new_v.len() && k != target_idx implies
                        #[trigger] new_v[k] == old_v[k]
                    by {}
                    // The old entry at target_idx had id == p; by uniqueness it was
                    // the only one.
                    assert(old_v[target_idx].id == p);
                    assert forall|k: int| 0 <= k < old_v.len() && k != target_idx implies
                        (#[trigger] old_v[k]).id != p
                    by {}
                    // p was in old_m.dom — so insert(p, _) leaves dom unchanged.
                    assert(old_m.dom().contains(p));
                    assert(new_m.dom() =~= old_m.dom());

                    // (a) next_purse_id != MAIN_PURSE — unchanged.
                    assert(self.next_purse_id != MAIN_PURSE);
                    // (b) MAIN_PURSE in dom — preserved.
                    assert(new_m.dom().contains(MAIN_PURSE));
                    // (d) forall p in dom. p < next_purse_id — preserved.
                    assert forall|q: PurseId| #[trigger] new_m.dom().contains(q)
                        implies q < self.next_purse_id
                    by {
                        assert(old_m.dom().contains(q));
                    }

                    // (c) forall p' in dom. m[p'].id == p'
                    assert forall|q: PurseId| #[trigger] new_m.dom().contains(q)
                        implies new_m[q].id == q
                    by {
                        if q == p {
                        } else {
                            assert(old_m.dom().contains(q));
                        }
                    }

                    // (e) every Vec entry's id is in dom
                    assert forall|k: int| 0 <= k < new_v.len() implies
                        new_m.dom().contains(#[trigger] new_v[k].id)
                    by {
                        if k == target_idx {
                        } else {
                            assert(new_v[k] == old_v[k]);
                            assert(old_m.dom().contains(old_v[k].id));
                        }
                    }

                    // (f) every Vec entry's spec view matches its dom entry
                    assert forall|k: int| 0 <= k < new_v.len() implies
                        new_m[(#[trigger] new_v[k]).id] == new_v[k]@
                    by {
                        if k == target_idx {
                        } else {
                            assert(new_v[k] == old_v[k]);
                            assert(old_v[k].id != p);
                            assert(old_m[old_v[k].id] == old_v[k]@);
                        }
                    }

                    // (g) every dom key has a Vec witness
                    assert forall|q: PurseId| #[trigger] new_m.dom().contains(q)
                        implies exists|k: int| 0 <= k < new_v.len() && #[trigger] new_v[k].id == q
                    by {
                        if q == p {
                            let w = target_idx;
                            assert(new_v[w].id == p);
                        } else {
                            assert(old_m.dom().contains(q));
                            let w = choose|k: int| 0 <= k < old_v.len() && #[trigger] old_v[k].id == q;
                            assert(w != target_idx);
                            assert(new_v[w] == old_v[w]);
                        }
                    }

                    // (h) no duplicates
                    assert forall|a: int, b: int|
                        0 <= a < new_v.len() && 0 <= b < new_v.len()
                        && #[trigger] new_v[a].id == #[trigger] new_v[b].id
                        implies a == b
                    by {
                        if a == target_idx && b == target_idx {
                        } else if a == target_idx {
                            assert(new_v[b] == old_v[b]);
                        } else if b == target_idx {
                            assert(new_v[a] == old_v[a]);
                        } else {
                            assert(new_v[a] == old_v[a]);
                            assert(new_v[b] == old_v[b]);
                        }
                    }

                }
                return Ok(());
            }
            i += 1;
        }
        // Not found: prove !dom.contains(p)
        proof {
            assert forall|q: PurseId| q == p implies !old_m.dom().contains(q) by {
                if old_m.dom().contains(p) {
                    let w = choose|k: int| 0 <= k < old_v.len() && #[trigger] old_v[k].id == p;
                    assert(0 <= w < self.purses@.len());
                    assert(self.purses@[w].id != p);
                }
            }
        }
        Err(Error::PurseNotFound(p))
    }

    /// 6.1.2 `deletePurse` (Quint lines 471-506; design §8.1 `delete_purse`).
    ///
    /// **Pilot scope:** local-state-only deletion. The Quint precondition set
    /// includes `!purseHasLiveCoins(p)`, `!purseHasLiveEntries(p)`,
    /// `!purseHasInFlight(p)`. These are vacuous here because the pilot state
    /// has no coins, entries, or operations. The design's user-facing variant
    /// drains funds via a separate prior operation before this local cleanup.
    ///
    /// Returns:
    ///   - `Ok(())` if the purse is removed.
    ///   - `Err(CannotDeleteMainPurse)` if `p == MAIN_PURSE`; state unchanged.
    ///   - `Err(PurseNotFound(p))` if `p` is not a known purse; state unchanged.
    /// Chain-side mirror: register that a coin exists on chain. The
    /// chain pushes a CoinRec into `chain_coins`. Local state is not
    /// touched — local discovery happens via recovery scans. Quint
    /// analog: `chainCoins' = chainCoins.put(...)` in a chain mint.
    pub fn chain_register_coin(&mut self, c: CoinRec)
        requires
            old(self).invariant(),
            old(self).chain_coins@.len() < u64::MAX as nat,
            c.exponent <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            final(self).chain_coins@ == old(self).chain_coins@.push(c),
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).spec_purses@ == old(self).spec_purses@,
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).spec_coins@ == old(self).spec_coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).next_purse_id == old(self).next_purse_id,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_spec_coins = self.spec_coins@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_spec_entries = self.spec_entries@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_spec_operations = self.spec_operations@;
        let ghost old_events = self.events@;
        let ghost old_tokens = self.tokens@;
        self.chain_coins.push(c);
        proof {
            assert(self.purses@ == old_purses_vec);
            assert(self.spec_purses@ == old_spec_purses);
            assert(self.coins@ == old_coins_vec);
            assert(self.spec_coins@ == old_spec_coins);
            assert(self.entries@ == old_entries_vec);
            assert(self.spec_entries@ == old_spec_entries);
            assert(self.operations@ == old_operations_vec);
            assert(self.spec_operations@ == old_spec_operations);
            assert(self.events@ == old_events);
            assert(self.tokens@ == old_tokens);
        }
    }

    /// Number of chain-coin records.
    pub fn chain_coin_count(&self) -> (n: usize)
        requires self.invariant(),
        ensures n == self.chain_coins@.len(),
    {
        self.chain_coins.len()
    }

    /// Find a chain coin (by index in chain_coins) whose (purse, idx)
    /// key is not present in local `coins`. Returns the Vec index, or
    /// `None` if every chain coin is mirrored locally. Foundation for
    /// the gap-limit recovery scan.
    pub fn find_missing_chain_coin(&self) -> (res: Option<usize>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(j) =>
                    0 <= j < self.chain_coins@.len()
                    && !self.coins().dom().contains(
                        (self.chain_coins@[j as int].purse,
                         self.chain_coins@[j as int].idx)
                    ),
                None => true,
            },
    {
        let mut j: usize = 0;
        while j < self.chain_coins.len()
            invariant
                0 <= j <= self.chain_coins.len(),
                self.invariant(),
            decreases self.chain_coins.len() - j,
        {
            let c = &self.chain_coins[j];
            let key = (c.purse, c.idx);
            if self.coin_state(key).is_none() {
                return Some(j);
            }
            j = j + 1;
        }
        None
    }

    /// Restore a chain-mirror coin record into local state. Reads
    /// `chain_coins[j]` and inserts it into local `coins` (both the
    /// exec Vec and the ghost map) under its `(purse, idx)` key.
    /// The purse allocator is not touched: the slot must already be
    /// allocated, i.e.
    /// `chain_coins[j].idx < purses[chain_coins[j].purse].next_coin_idx`.
    /// This is the "restore an old slot we lost track of" primitive
    /// that composes with [`State::find_missing_chain_coin`] to form
    /// the recovery scan body.
    pub fn restore_chain_coin(&mut self, j: usize)
        requires
            old(self).invariant(),
            j < old(self).chain_coins@.len(),
            old(self).purses().dom().contains(
                old(self).chain_coins@[j as int].purse
            ),
            !old(self).coins().dom().contains(
                (old(self).chain_coins@[j as int].purse,
                 old(self).chain_coins@[j as int].idx)
            ),
            old(self).chain_coins@[j as int].idx
                < old(self).purses()[old(self).chain_coins@[j as int].purse]
                      .next_coin_idx,
        ensures
            final(self).invariant(),
            final(self).coins() == old(self).coins().insert(
                (old(self).chain_coins@[j as int].purse,
                 old(self).chain_coins@[j as int].idx),
                old(self).chain_coins@[j as int],
            ),
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).spec_purses@ == old(self).spec_purses@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).next_purse_id == old(self).next_purse_id,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let rec = self.chain_coins[j];
        let key = (rec.purse, rec.idx);

        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_coins = self.spec_coins@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_entries = self.spec_entries@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_spec_entries = self.spec_entries@;
        let ghost old_operations = self.spec_operations@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_events = self.events@;
        let ghost old_tokens = self.tokens@;
        let ghost old_chain_coins = self.chain_coins@;
        let ghost old_chain_entries = self.chain_entries@;

        self.coins.push(rec);
        proof {
            self.spec_coins = Ghost(self.spec_coins@.insert(key, rec));

            let new_coins = self.spec_coins@;
            let new_coins_vec = self.coins@;
            let last = old_coins_vec.len() as int;

            // Sibling-field stability (the ghost-field-mutation pattern).
            assert(self.purses@ == old_purses_vec);
            assert(self.spec_purses@ == old_spec_purses);
            assert(self.entries@ == old_entries_vec);
            assert(self.spec_entries@ == old_spec_entries);
            assert(self.operations@ == old_operations_vec);
            assert(self.spec_operations@ == old_operations);
            assert(self.events@ == old_events);
            assert(self.tokens@ == old_tokens);
            assert(self.chain_coins@ == old_chain_coins);
            assert(self.chain_entries@ == old_chain_entries);

            // (i) coin key consistency.
            assert forall|k: (PurseId, u64)| #[trigger] new_coins.dom().contains(k)
                implies new_coins[k].purse == k.0 && new_coins[k].idx == k.1
            by {
                if k == key {
                    assert(new_coins[k] == rec);
                } else {
                    assert(old_coins.dom().contains(k));
                }
            }

            // (j) coin referential integrity.
            assert forall|k: (PurseId, u64)| #[trigger] new_coins.dom().contains(k)
                implies old_spec_purses.dom().contains(k.0)
            by {
                if k == key {
                    assert(old(self).purses().dom().contains(rec.purse));
                } else {
                    assert(old_coins.dom().contains(k));
                }
            }

            // (k) coin idx below purse's allocator. Unchanged purses.
            assert forall|k: (PurseId, u64)| #[trigger] new_coins.dom().contains(k)
                implies k.1 < old_spec_purses[k.0].next_coin_idx
            by {
                if k == key {
                    // by precondition.
                } else {
                    assert(old_coins.dom().contains(k));
                }
            }

            // Vec post-state.
            assert(new_coins_vec.len() == old_coins_vec.len() + 1);
            assert(new_coins_vec[last] == rec);
            assert forall|k: int| 0 <= k < old_coins_vec.len() implies
                new_coins_vec[k] == #[trigger] old_coins_vec[k]
            by {}

            // (l) exec Vec → ghost.
            assert forall|jj: int| 0 <= jj < new_coins_vec.len() implies
                new_coins.dom().contains(
                    (#[trigger] new_coins_vec[jj].purse, new_coins_vec[jj].idx)
                )
                && new_coins[(new_coins_vec[jj].purse, new_coins_vec[jj].idx)]
                    == new_coins_vec[jj]
            by {
                if jj == last {
                    assert(new_coins_vec[jj] == rec);
                    assert(new_coins[key] == rec);
                } else {
                    assert(new_coins_vec[jj] == old_coins_vec[jj]);
                    let oc = old_coins_vec[jj];
                    assert(old_coins.dom().contains((oc.purse, oc.idx)));
                    assert((oc.purse, oc.idx) != key);
                    assert(old_coins[(oc.purse, oc.idx)] == oc);
                }
            }

            // (m) every dom key has a Vec witness.
            assert forall|k: (PurseId, u64)| #[trigger] new_coins.dom().contains(k)
                implies exists|jj: int|
                    0 <= jj < new_coins_vec.len()
                    && #[trigger] new_coins_vec[jj].purse == k.0
                    && new_coins_vec[jj].idx == k.1
            by {
                if k == key {
                    let w = last;
                    assert(new_coins_vec[w].purse == rec.purse);
                    assert(new_coins_vec[w].idx == rec.idx);
                } else {
                    assert(old_coins.dom().contains(k));
                    let w = choose|jj: int|
                        0 <= jj < old_coins_vec.len()
                        && #[trigger] old_coins_vec[jj].purse == k.0
                        && old_coins_vec[jj].idx == k.1;
                    assert(new_coins_vec[w] == old_coins_vec[w]);
                }
            }

            // (n) no duplicate (purse, idx) in Vec.
            assert forall|a: int, b: int|
                0 <= a < new_coins_vec.len() && 0 <= b < new_coins_vec.len()
                && (#[trigger] new_coins_vec[a]).purse
                    == (#[trigger] new_coins_vec[b]).purse
                && new_coins_vec[a].idx == new_coins_vec[b].idx
                implies a == b
            by {
                if a == last && b == last {
                } else if a == last {
                    assert(new_coins_vec[b] == old_coins_vec[b]);
                    let oc = old_coins_vec[b];
                    assert(old_coins.dom().contains((oc.purse, oc.idx)));
                    assert((oc.purse, oc.idx) != key);
                } else if b == last {
                    assert(new_coins_vec[a] == old_coins_vec[a]);
                    let oc = old_coins_vec[a];
                    assert(old_coins.dom().contains((oc.purse, oc.idx)));
                    assert((oc.purse, oc.idx) != key);
                } else {
                    assert(new_coins_vec[a] == old_coins_vec[a]);
                    assert(new_coins_vec[b] == old_coins_vec[b]);
                }
            }
        }
    }

    /// Find a chain coin (by index in `chain_coins`) whose
    /// `(purse, idx)` is not in local `coins` AND whose purse exists
    /// locally AND whose `idx` is below that purse's `next_coin_idx`.
    /// In other words: a chain coin we lost track of, that is still
    /// restorable into our current state. The returned `j` satisfies
    /// exactly the preconditions of [`State::restore_chain_coin`].
    pub fn find_restorable_missing_chain_coin(&self) -> (res: Option<usize>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(j) => {
                    &&& 0 <= j < self.chain_coins@.len()
                    &&& !self.coins().dom().contains(
                            (self.chain_coins@[j as int].purse,
                             self.chain_coins@[j as int].idx))
                    &&& self.purses().dom().contains(
                            self.chain_coins@[j as int].purse)
                    &&& self.chain_coins@[j as int].idx
                            < self.purses()[self.chain_coins@[j as int].purse]
                                  .next_coin_idx
                },
                None => true,
            },
    {
        let mut j: usize = 0;
        while j < self.chain_coins.len()
            invariant
                0 <= j <= self.chain_coins.len(),
                self.invariant(),
            decreases self.chain_coins.len() - j,
        {
            let c = self.chain_coins[j];
            let key = (c.purse, c.idx);
            if self.coin_state(key).is_none() {
                // Missing locally. Walk purses to check restorability.
                let mut i: usize = 0;
                while i < self.purses.len()
                    invariant
                        0 <= i <= self.purses.len(),
                        self.invariant(),
                        j < self.chain_coins@.len(),
                        c == self.chain_coins@[j as int],
                        key == (c.purse, c.idx),
                        !self.coins().dom().contains(key),
                    decreases self.purses.len() - i,
                {
                    if self.purses[i].id == c.purse {
                        let next_idx = self.purses[i].next_coin_idx;
                        if c.idx < next_idx {
                            proof {
                                let m = self.spec_purses@;
                                let v = self.purses@;
                                let cc = self.chain_coins@[j as int];
                                assert(cc == c);
                                assert(cc.purse == c.purse);
                                assert(cc.idx == c.idx);
                                assert(0 <= i < v.len());
                                assert(v[i as int].id == c.purse);
                                assert(m.dom().contains(v[i as int].id));
                                assert(m[v[i as int].id] == v[i as int]@);
                                assert(m[c.purse] == v[i as int]@);
                                assert(v[i as int].next_coin_idx == next_idx);
                                assert(v[i as int]@.next_coin_idx == next_idx as nat);
                                assert(m[c.purse].next_coin_idx == next_idx as nat);
                                assert(m.dom().contains(c.purse));
                                assert(self.purses().dom().contains(cc.purse));
                                assert(cc.idx < self.purses()[cc.purse].next_coin_idx);
                                assert(!self.coins().dom().contains((cc.purse, cc.idx)));
                            }
                            return Some(j);
                        }
                        // Found the purse but slot not allocated yet — skip.
                        break;
                    }
                    i = i + 1;
                }
            }
            j = j + 1;
        }
        None
    }

    /// Chain-side mirror: register that an entry exists on chain.
    /// Quint analog: `chainEntries' = chainEntries.put(...)`.
    pub fn chain_register_entry(&mut self, e: EntryRec)
        requires
            old(self).invariant(),
            old(self).chain_entries@.len() < u64::MAX as nat,
            e.exponent <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            final(self).chain_entries@ == old(self).chain_entries@.push(e),
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).spec_purses@ == old(self).spec_purses@,
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).spec_coins@ == old(self).spec_coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).next_purse_id == old(self).next_purse_id,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_spec_coins = self.spec_coins@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_spec_entries = self.spec_entries@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_spec_operations = self.spec_operations@;
        let ghost old_events = self.events@;
        let ghost old_tokens = self.tokens@;
        let ghost old_chain_coins = self.chain_coins@;
        self.chain_entries.push(e);
        proof {
            assert(self.purses@ == old_purses_vec);
            assert(self.spec_purses@ == old_spec_purses);
            assert(self.coins@ == old_coins_vec);
            assert(self.spec_coins@ == old_spec_coins);
            assert(self.entries@ == old_entries_vec);
            assert(self.spec_entries@ == old_spec_entries);
            assert(self.operations@ == old_operations_vec);
            assert(self.spec_operations@ == old_spec_operations);
            assert(self.events@ == old_events);
            assert(self.tokens@ == old_tokens);
            assert(self.chain_coins@ == old_chain_coins);
        }
    }

    /// Number of chain-entry records.
    pub fn chain_entry_count(&self) -> (n: usize)
        requires self.invariant(),
        ensures n == self.chain_entries@.len(),
    {
        self.chain_entries.len()
    }

    /// Find a chain entry whose (purse, idx) is not present in local
    /// `entries`. Entry parallel of `find_missing_chain_coin`.
    pub fn find_missing_chain_entry(&self) -> (res: Option<usize>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(j) =>
                    0 <= j < self.chain_entries@.len()
                    && !self.entries().dom().contains(
                        (self.chain_entries@[j as int].purse,
                         self.chain_entries@[j as int].idx)
                    ),
                None => true,
            },
    {
        let mut j: usize = 0;
        while j < self.chain_entries.len()
            invariant
                0 <= j <= self.chain_entries.len(),
                self.invariant(),
            decreases self.chain_entries.len() - j,
        {
            let e = &self.chain_entries[j];
            let key = (e.purse, e.idx);
            if self.entry_local_state(key).is_none() {
                return Some(j);
            }
            j = j + 1;
        }
        None
    }

    /// Restore a chain-mirror entry record into local state. Entry
    /// parallel of [`State::restore_chain_coin`]: reads
    /// `chain_entries[j]` and inserts it into local `entries`. The
    /// slot must already be allocated
    /// (`chain.idx < purses[chain.purse].next_entry_idx`).
    pub fn restore_chain_entry(&mut self, j: usize)
        requires
            old(self).invariant(),
            j < old(self).chain_entries@.len(),
            old(self).purses().dom().contains(
                old(self).chain_entries@[j as int].purse
            ),
            !old(self).entries().dom().contains(
                (old(self).chain_entries@[j as int].purse,
                 old(self).chain_entries@[j as int].idx)
            ),
            old(self).chain_entries@[j as int].idx
                < old(self).purses()[old(self).chain_entries@[j as int].purse]
                      .next_entry_idx,
        ensures
            final(self).invariant(),
            final(self).entries() == old(self).entries().insert(
                (old(self).chain_entries@[j as int].purse,
                 old(self).chain_entries@[j as int].idx),
                old(self).chain_entries@[j as int],
            ),
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).spec_purses@ == old(self).spec_purses@,
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).spec_coins@ == old(self).spec_coins@,
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).next_purse_id == old(self).next_purse_id,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let rec = self.chain_entries[j];
        let key = (rec.purse, rec.idx);

        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_coins = self.spec_coins@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_entries = self.spec_entries@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_operations = self.spec_operations@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_events = self.events@;
        let ghost old_tokens = self.tokens@;
        let ghost old_chain_coins = self.chain_coins@;
        let ghost old_chain_entries = self.chain_entries@;

        self.entries.push(rec);
        proof {
            self.spec_entries = Ghost(self.spec_entries@.insert(key, rec));

            let new_entries = self.spec_entries@;
            let new_entries_vec = self.entries@;
            let last = old_entries_vec.len() as int;

            assert(self.purses@ == old_purses_vec);
            assert(self.spec_purses@ == old_spec_purses);
            assert(self.coins@ == old_coins_vec);
            assert(self.spec_coins@ == old_coins);
            assert(self.operations@ == old_operations_vec);
            assert(self.spec_operations@ == old_operations);
            assert(self.events@ == old_events);
            assert(self.tokens@ == old_tokens);
            assert(self.chain_coins@ == old_chain_coins);
            assert(self.chain_entries@ == old_chain_entries);

            // (o) entry key consistency.
            assert forall|k: (PurseId, u64)| #[trigger] new_entries.dom().contains(k)
                implies new_entries[k].purse == k.0 && new_entries[k].idx == k.1
            by {
                if k == key {
                    assert(new_entries[k] == rec);
                } else {
                    assert(old_entries.dom().contains(k));
                }
            }

            // (p) entry referential integrity.
            assert forall|k: (PurseId, u64)| #[trigger] new_entries.dom().contains(k)
                implies old_spec_purses.dom().contains(k.0)
            by {
                if k == key {
                    assert(old(self).purses().dom().contains(rec.purse));
                } else {
                    assert(old_entries.dom().contains(k));
                }
            }

            // (q) entry idx below purse's allocator.
            assert forall|k: (PurseId, u64)| #[trigger] new_entries.dom().contains(k)
                implies k.1 < old_spec_purses[k.0].next_entry_idx
            by {
                if k == key {
                    // by precondition.
                } else {
                    assert(old_entries.dom().contains(k));
                }
            }

            // Vec post-state.
            assert(new_entries_vec.len() == old_entries_vec.len() + 1);
            assert(new_entries_vec[last] == rec);
            assert forall|k: int| 0 <= k < old_entries_vec.len() implies
                new_entries_vec[k] == #[trigger] old_entries_vec[k]
            by {}

            // (r) exec Vec → ghost.
            assert forall|jj: int| 0 <= jj < new_entries_vec.len() implies
                new_entries.dom().contains(
                    (#[trigger] new_entries_vec[jj].purse, new_entries_vec[jj].idx)
                )
                && new_entries[(new_entries_vec[jj].purse, new_entries_vec[jj].idx)]
                    == new_entries_vec[jj]
            by {
                if jj == last {
                    assert(new_entries_vec[jj] == rec);
                    assert(new_entries[key] == rec);
                } else {
                    assert(new_entries_vec[jj] == old_entries_vec[jj]);
                    let oc = old_entries_vec[jj];
                    assert(old_entries.dom().contains((oc.purse, oc.idx)));
                    assert((oc.purse, oc.idx) != key);
                    assert(old_entries[(oc.purse, oc.idx)] == oc);
                }
            }

            // (s) every dom key has a Vec witness.
            assert forall|k: (PurseId, u64)| #[trigger] new_entries.dom().contains(k)
                implies exists|jj: int|
                    0 <= jj < new_entries_vec.len()
                    && #[trigger] new_entries_vec[jj].purse == k.0
                    && new_entries_vec[jj].idx == k.1
            by {
                if k == key {
                    let w = last;
                    assert(new_entries_vec[w].purse == rec.purse);
                    assert(new_entries_vec[w].idx == rec.idx);
                } else {
                    assert(old_entries.dom().contains(k));
                    let w = choose|jj: int|
                        0 <= jj < old_entries_vec.len()
                        && #[trigger] old_entries_vec[jj].purse == k.0
                        && old_entries_vec[jj].idx == k.1;
                    assert(new_entries_vec[w] == old_entries_vec[w]);
                }
            }

            // (t) no duplicate (purse, idx) in Vec.
            assert forall|a: int, b: int|
                0 <= a < new_entries_vec.len() && 0 <= b < new_entries_vec.len()
                && (#[trigger] new_entries_vec[a]).purse
                    == (#[trigger] new_entries_vec[b]).purse
                && new_entries_vec[a].idx == new_entries_vec[b].idx
                implies a == b
            by {
                if a == last && b == last {
                } else if a == last {
                    assert(new_entries_vec[b] == old_entries_vec[b]);
                    let oc = old_entries_vec[b];
                    assert(old_entries.dom().contains((oc.purse, oc.idx)));
                    assert((oc.purse, oc.idx) != key);
                } else if b == last {
                    assert(new_entries_vec[a] == old_entries_vec[a]);
                    let oc = old_entries_vec[a];
                    assert(old_entries.dom().contains((oc.purse, oc.idx)));
                    assert((oc.purse, oc.idx) != key);
                } else {
                    assert(new_entries_vec[a] == old_entries_vec[a]);
                    assert(new_entries_vec[b] == old_entries_vec[b]);
                }
            }
        }
    }

    /// Entry parallel of [`State::find_restorable_missing_chain_coin`].
    /// Returns an index `j` such that `chain_entries[j]` is missing
    /// locally, its purse exists, and its `idx` is below the purse's
    /// `next_entry_idx` — satisfying exactly the preconditions of
    /// [`State::restore_chain_entry`].
    pub fn find_restorable_missing_chain_entry(&self) -> (res: Option<usize>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(j) => {
                    &&& 0 <= j < self.chain_entries@.len()
                    &&& !self.entries().dom().contains(
                            (self.chain_entries@[j as int].purse,
                             self.chain_entries@[j as int].idx))
                    &&& self.purses().dom().contains(
                            self.chain_entries@[j as int].purse)
                    &&& self.chain_entries@[j as int].idx
                            < self.purses()[self.chain_entries@[j as int].purse]
                                  .next_entry_idx
                },
                None => true,
            },
    {
        let mut j: usize = 0;
        while j < self.chain_entries.len()
            invariant
                0 <= j <= self.chain_entries.len(),
                self.invariant(),
            decreases self.chain_entries.len() - j,
        {
            let e = self.chain_entries[j];
            let key = (e.purse, e.idx);
            if self.entry_local_state(key).is_none() {
                let mut i: usize = 0;
                while i < self.purses.len()
                    invariant
                        0 <= i <= self.purses.len(),
                        self.invariant(),
                        j < self.chain_entries@.len(),
                        e == self.chain_entries@[j as int],
                        key == (e.purse, e.idx),
                        !self.entries().dom().contains(key),
                    decreases self.purses.len() - i,
                {
                    if self.purses[i].id == e.purse {
                        let next_idx = self.purses[i].next_entry_idx;
                        if e.idx < next_idx {
                            proof {
                                let m = self.spec_purses@;
                                let v = self.purses@;
                                let ee = self.chain_entries@[j as int];
                                assert(ee == e);
                                assert(0 <= i < v.len());
                                assert(v[i as int].id == e.purse);
                                assert(m.dom().contains(v[i as int].id));
                                assert(m[v[i as int].id] == v[i as int]@);
                                assert(m[e.purse] == v[i as int]@);
                                assert(v[i as int].next_entry_idx == next_idx);
                                assert(v[i as int]@.next_entry_idx == next_idx as nat);
                                assert(m[e.purse].next_entry_idx == next_idx as nat);
                                assert(m.dom().contains(e.purse));
                                assert(self.purses().dom().contains(ee.purse));
                                assert(ee.idx < self.purses()[ee.purse].next_entry_idx);
                                assert(!self.entries().dom().contains((ee.purse, ee.idx)));
                            }
                            return Some(j);
                        }
                        break;
                    }
                    i = i + 1;
                }
            }
            j = j + 1;
        }
        None
    }

    /// One step of the recovery scan. Looks for a restorable missing
    /// chain coin; if found, restores it and returns the chain-coin
    /// index that was processed. Returns `None` if no restorable
    /// missing chain coin exists in the current state.
    ///
    /// Recovery callers drive this in a loop until it returns `None`
    /// for both the coin and entry side, at which point the local
    /// state has absorbed every chain record it can.
    pub fn recover_scan_step_coin(&mut self) -> (res: Option<usize>)
        requires
            old(self).invariant(),
        ensures
            final(self).invariant(),
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let res = self.find_restorable_missing_chain_coin();
        match res {
            Some(j) => {
                self.restore_chain_coin(j);
                Some(j)
            }
            None => None,
        }
    }

    /// Entry parallel of [`State::recover_scan_step_coin`]. Returns
    /// the chain-entry index processed, or `None` if no restorable
    /// missing chain entry exists.
    pub fn recover_scan_step_entry(&mut self) -> (res: Option<usize>)
        requires
            old(self).invariant(),
        ensures
            final(self).invariant(),
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let res = self.find_restorable_missing_chain_entry();
        match res {
            Some(j) => {
                self.restore_chain_entry(j);
                Some(j)
            }
            None => None,
        }
    }

    /// Mint a new unload token (chain emit). Pushed to the tokens
    /// Vec with `consumed: false`. Quint analog: any `tokens' =
    /// tokens.put(...)` in a chain-mint step.
    pub fn mint_token(&mut self, period: u64, class: UnloadTokenClass, counter: u64)
        -> (idx: usize)
        requires
            old(self).invariant(),
            old(self).tokens@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            idx == old(self).tokens@.len(),
            final(self).tokens@.len() == old(self).tokens@.len() + 1,
            final(self).tokens@[idx as int] == (UnloadToken {
                period, class, counter, consumed: false,
            }),
            forall|i: int| 0 <= i < old(self).tokens@.len() ==>
                #[trigger] final(self).tokens@[i] == old(self).tokens@[i],
            // Everything else untouched.
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).spec_purses@ == old(self).spec_purses@,
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).spec_coins@ == old(self).spec_coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).next_purse_id == old(self).next_purse_id,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_spec_coins = self.spec_coins@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_spec_entries = self.spec_entries@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_spec_operations = self.spec_operations@;
        let ghost old_events = self.events@;
        let idx = self.tokens.len();
        self.tokens.push(UnloadToken { period, class, counter, consumed: false });
        proof {
            assert(self.purses@ == old_purses_vec);
            assert(self.spec_purses@ == old_spec_purses);
            assert(self.coins@ == old_coins_vec);
            assert(self.spec_coins@ == old_spec_coins);
            assert(self.entries@ == old_entries_vec);
            assert(self.spec_entries@ == old_spec_entries);
            assert(self.operations@ == old_operations_vec);
            assert(self.spec_operations@ == old_spec_operations);
            assert(self.events@ == old_events);
        }
        idx
    }

    /// Consume an unload token (mark consumed). Idempotent against
    /// already-consumed tokens (silently no-op). Quint analog: the
    /// chain side flipping the `consumed` flag.
    pub fn consume_token(&mut self, idx: usize) -> (res: Result<(), Error>)
        requires
            old(self).invariant(),
        ensures
            final(self).invariant(),
            match res {
                Ok(()) =>
                    idx < old(self).tokens@.len()
                    && !old(self).tokens@[idx as int].consumed
                    && final(self).tokens@.len() == old(self).tokens@.len()
                    && final(self).tokens@[idx as int].consumed
                    && forall|i: int| 0 <= i < old(self).tokens@.len() && i != idx as int
                        ==> #[trigger] final(self).tokens@[i] == old(self).tokens@[i],
                Err(_) =>
                    (idx >= old(self).tokens@.len()
                     || old(self).tokens@[idx as int].consumed)
                    && final(self).tokens@ == old(self).tokens@,
            },
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).spec_purses@ == old(self).spec_purses@,
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).spec_coins@ == old(self).spec_coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).next_purse_id == old(self).next_purse_id,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_spec_coins = self.spec_coins@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_spec_entries = self.spec_entries@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_spec_operations = self.spec_operations@;
        let ghost old_events = self.events@;
        if idx >= self.tokens.len() {
            return Err(Error::Internal(Vec::new()));
        }
        if self.tokens[idx].consumed {
            return Err(Error::Internal(Vec::new()));
        }
        self.tokens[idx].consumed = true;
        proof {
            assert(self.purses@ == old_purses_vec);
            assert(self.spec_purses@ == old_spec_purses);
            assert(self.coins@ == old_coins_vec);
            assert(self.spec_coins@ == old_spec_coins);
            assert(self.entries@ == old_entries_vec);
            assert(self.spec_entries@ == old_spec_entries);
            assert(self.operations@ == old_operations_vec);
            assert(self.spec_operations@ == old_spec_operations);
            assert(self.events@ == old_events);
        }
        Ok(())
    }

    /// Number of unload tokens minted.
    pub fn token_count(&self) -> (n: usize)
        requires self.invariant(),
        ensures n == self.tokens@.len(),
    {
        self.tokens.len()
    }

    /// Increment `total_in` by `amount` (Quint accumulator advance on
    /// inflow: top-up, import).
    pub fn add_total_in(&mut self, amount: u64)
        requires
            old(self).invariant(),
            old(self).total_in <= u64::MAX - amount,
        ensures
            final(self).invariant(),
            final(self).total_in == old(self).total_in + amount,
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).spec_purses@ == old(self).spec_purses@,
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).spec_coins@ == old(self).spec_coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).next_purse_id == old(self).next_purse_id,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_spec_coins = self.spec_coins@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_spec_entries = self.spec_entries@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_spec_operations = self.spec_operations@;
        let ghost old_events = self.events@;
        self.total_in = self.total_in + amount;
        proof {
            assert(self.purses@ == old_purses_vec);
            assert(self.spec_purses@ == old_spec_purses);
            assert(self.coins@ == old_coins_vec);
            assert(self.spec_coins@ == old_spec_coins);
            assert(self.entries@ == old_entries_vec);
            assert(self.spec_entries@ == old_spec_entries);
            assert(self.operations@ == old_operations_vec);
            assert(self.spec_operations@ == old_spec_operations);
            assert(self.events@ == old_events);
        }
    }

    /// Increment `total_out` by `amount` (Quint accumulator advance on
    /// outflow: export, cross-host transfer-out).
    pub fn add_total_out(&mut self, amount: u64)
        requires
            old(self).invariant(),
            old(self).total_out <= u64::MAX - amount,
        ensures
            final(self).invariant(),
            final(self).total_out == old(self).total_out + amount,
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).spec_purses@ == old(self).spec_purses@,
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).spec_coins@ == old(self).spec_coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).next_purse_id == old(self).next_purse_id,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_spec_coins = self.spec_coins@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_spec_entries = self.spec_entries@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_spec_operations = self.spec_operations@;
        let ghost old_events = self.events@;
        self.total_out = self.total_out + amount;
        proof {
            assert(self.purses@ == old_purses_vec);
            assert(self.spec_purses@ == old_spec_purses);
            assert(self.coins@ == old_coins_vec);
            assert(self.spec_coins@ == old_spec_coins);
            assert(self.entries@ == old_entries_vec);
            assert(self.spec_entries@ == old_spec_entries);
            assert(self.operations@ == old_operations_vec);
            assert(self.spec_operations@ == old_spec_operations);
            assert(self.events@ == old_events);
        }
    }

    /// Read total_in.
    pub fn read_total_in(&self) -> (v: u64)
        requires self.invariant(),
        ensures v == self.total_in,
    { self.total_in }

    /// Read total_out.
    pub fn read_total_out(&self) -> (v: u64)
        requires self.invariant(),
        ensures v == self.total_out,
    { self.total_out }

    /// Read paid_ring_membership.
    pub fn read_paid_ring_membership(&self) -> (v: u64)
        requires self.invariant(),
        ensures v == self.paid_ring_membership,
    { self.paid_ring_membership }

    /// Append an event to the layer event stream. Quint analog: any
    /// `events' = events.append(e)` clause. Callers compose this with
    /// state-mutating ops to declare emissions (note: the existing
    /// mutators don't emit yet — this is the primitive on which to
    /// build event-emitting wrappers).
    pub fn emit_event(&mut self, e: Event)
        requires
            old(self).invariant(),
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).events@ == old(self).events@.push(e),
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).spec_purses@ == old(self).spec_purses@,
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).spec_coins@ == old(self).spec_coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).next_purse_id == old(self).next_purse_id,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_spec_coins = self.spec_coins@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_spec_entries = self.spec_entries@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_spec_operations = self.spec_operations@;
        let ghost old_tokens = self.tokens@;
        let ghost old_chain_coins = self.chain_coins@;
        let ghost old_chain_entries = self.chain_entries@;
        self.events.push(e);
        proof {
            assert(self.purses@ == old_purses_vec);
            assert(self.spec_purses@ == old_spec_purses);
            assert(self.coins@ == old_coins_vec);
            assert(self.spec_coins@ == old_spec_coins);
            assert(self.entries@ == old_entries_vec);
            assert(self.spec_entries@ == old_spec_entries);
            assert(self.operations@ == old_operations_vec);
            assert(self.spec_operations@ == old_spec_operations);
            assert(self.tokens@ == old_tokens);
            assert(self.chain_coins@ == old_chain_coins);
            assert(self.chain_entries@ == old_chain_entries);
        }
    }

    /// Number of events emitted so far. Quint `events.length()`.
    pub fn event_count(&self) -> (n: usize)
        requires
            self.invariant(),
        ensures
            n == self.events@.len(),
    {
        self.events.len()
    }

    /// Allocate a fresh chain-extrinsic ID and bump the allocator.
    /// Quint `nextExtrinsicId`. Called by chain-bound op submission
    /// to identify the corresponding chain extrinsic for receipt
    /// matching.
    pub fn alloc_extrinsic_id(&mut self) -> (id: u64)
        requires
            old(self).invariant(),
            old(self).next_extrinsic_id < u64::MAX,
        ensures
            final(self).invariant(),
            id == old(self).next_extrinsic_id,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id + 1,
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).spec_purses@ == old(self).spec_purses@,
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).spec_coins@ == old(self).spec_coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).next_purse_id == old(self).next_purse_id,
            final(self).fee_balance == old(self).fee_balance,
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_spec_coins = self.spec_coins@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_spec_entries = self.spec_entries@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_spec_operations = self.spec_operations@;
        let id = self.next_extrinsic_id;
        self.next_extrinsic_id = id + 1;
        proof {
            assert(self.purses@ == old_purses_vec);
            assert(self.spec_purses@ == old_spec_purses);
            assert(self.coins@ == old_coins_vec);
            assert(self.spec_coins@ == old_spec_coins);
            assert(self.entries@ == old_entries_vec);
            assert(self.spec_entries@ == old_spec_entries);
            assert(self.operations@ == old_operations_vec);
            assert(self.spec_operations@ == old_spec_operations);
        }
        id
    }

    /// Synchronous read of `next_extrinsic_id` (the next allocator value).
    pub fn read_next_extrinsic_id(&self) -> (id: u64)
        requires
            self.invariant(),
        ensures
            id == self.next_extrinsic_id,
    {
        self.next_extrinsic_id
    }

    /// Top up the fee-account reservoir. Quint `topUpFeeAccount`.
    pub fn top_up_fee_account(&mut self, amount: u64)
        requires
            old(self).invariant(),
            old(self).fee_balance <= u64::MAX - amount,
        ensures
            final(self).invariant(),
            final(self).fee_balance == old(self).fee_balance + amount,
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).spec_purses@ == old(self).spec_purses@,
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).spec_coins@ == old(self).spec_coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).next_purse_id == old(self).next_purse_id,
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_spec_coins = self.spec_coins@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_spec_entries = self.spec_entries@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_spec_operations = self.spec_operations@;
        self.fee_balance = self.fee_balance + amount;
        proof {
            assert(self.purses@ == old_purses_vec);
            assert(self.spec_purses@ == old_spec_purses);
            assert(self.coins@ == old_coins_vec);
            assert(self.spec_coins@ == old_spec_coins);
            assert(self.entries@ == old_entries_vec);
            assert(self.spec_entries@ == old_spec_entries);
            assert(self.operations@ == old_operations_vec);
            assert(self.spec_operations@ == old_spec_operations);
        }
    }

    /// Spend from the fee-account reservoir.
    pub fn deduct_fee(&mut self, amount: u64) -> (res: Result<(), Error>)
        requires
            old(self).invariant(),
        ensures
            final(self).invariant(),
            match res {
                Ok(()) =>
                    old(self).fee_balance >= amount
                    && final(self).fee_balance == old(self).fee_balance - amount,
                Err(Error::InsufficientFunds { requested, available }) =>
                    old(self).fee_balance < amount
                    && requested == amount
                    && available == old(self).fee_balance
                    && final(self).fee_balance == old(self).fee_balance,
                Err(_) => false,
            },
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).spec_purses@ == old(self).spec_purses@,
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).spec_coins@ == old(self).spec_coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).next_purse_id == old(self).next_purse_id,
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_spec_coins = self.spec_coins@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_spec_entries = self.spec_entries@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_spec_operations = self.spec_operations@;
        let res = if self.fee_balance >= amount {
            self.fee_balance = self.fee_balance - amount;
            Ok(())
        } else {
            Err(Error::InsufficientFunds {
                requested: amount,
                available: self.fee_balance,
            })
        };
        proof {
            assert(self.purses@ == old_purses_vec);
            assert(self.spec_purses@ == old_spec_purses);
            assert(self.coins@ == old_coins_vec);
            assert(self.spec_coins@ == old_spec_coins);
            assert(self.entries@ == old_entries_vec);
            assert(self.spec_entries@ == old_spec_entries);
            assert(self.operations@ == old_operations_vec);
            assert(self.spec_operations@ == old_spec_operations);
        }
        res
    }

    /// Synchronous read of the fee-account balance.
    pub fn read_fee_balance(&self) -> (b: u64)
        requires
            self.invariant(),
        ensures
            b == self.fee_balance,
    {
        self.fee_balance
    }

    /// Auto-pick a `FeeMode` based on the current reservoir.
    pub fn select_fee_mode(&self, fee: u64) -> (mode: FeeMode)
        requires
            self.invariant(),
        ensures
            match mode {
                FeeMode::Prepaid => self.fee_balance >= fee,
                FeeMode::FromOutput => self.fee_balance < fee,
            },
    {
        if self.fee_balance >= fee {
            FeeMode::Prepaid
        } else {
            FeeMode::FromOutput
        }
    }

    /// Safe variant of [`Self::delete_purse`]: runs the safety checks
    /// first and returns a typed error if the purse can't be removed,
    /// rather than tripping a hard precondition. Composes with the
    /// existing exec pre-flight guards (`check_has_live_coin_in`,
    /// `has_op_targeting_purse`).
    ///
    /// Errors surface (in the order checked):
    ///   - PurseHasInFlightOperations — at least one op targets `p`.
    ///   - InsufficientFunds — `p` still has at least one live coin.
    ///   - Then anything delete_purse itself can return.
    pub fn delete_purse_safe(&mut self, p: PurseId) -> (res: Result<(), Error>)
        requires
            old(self).invariant(),
        ensures
            final(self).invariant(),
            match res {
                Ok(()) =>
                    !old(self).has_live_coin_in(p)
                    && (forall|h: OpHandle|
                        #[trigger] old(self).operations().dom().contains(h)
                        ==> old(self).operations()[h].purse != p)
                    && old(self).purses().dom().contains(p)
                    && p != MAIN_PURSE,
                Err(_) => true,
            },
    {
        if self.has_op_targeting_purse(p) {
            return Err(Error::PurseHasInFlightOperations);
        }
        if self.check_has_live_coin_in(p) {
            return Err(Error::InsufficientFunds {
                requested: 0,
                available: 0,
            });
        }
        self.delete_purse(p)
    }

    pub fn delete_purse(&mut self, p: PurseId) -> (res: Result<(), Error>)
        requires
            old(self).invariant(),
            !old(self).has_live_coin_in(p),
            // No operation targets purse p (operations subsystem refint).
            forall|h: OpHandle| #[trigger] old(self).operations().dom().contains(h)
                ==> old(self).operations()[h].purse != p,
        ensures
            final(self).invariant(),
            match res {
                Ok(()) =>
                    old(self).purses().dom().contains(p)
                    && p != MAIN_PURSE
                    && final(self).purses() == old(self).purses().remove(p)
                    && final(self).coins() == old(self).coins().remove_keys(
                        Set::new(|k: (PurseId, u64)| k.0 == p)
                    )
                    && final(self).entries() == old(self).entries().remove_keys(
                        Set::new(|k: (PurseId, u64)| k.0 == p)
                    ),
                Err(Error::CannotDeleteMainPurse) =>
                    p == MAIN_PURSE
                    && final(self).purses() == old(self).purses()
                    && final(self).coins() == old(self).coins()
                    && final(self).entries() == old(self).entries(),
                Err(Error::PurseNotFound(q)) =>
                    p != MAIN_PURSE
                    && !old(self).purses().dom().contains(p)
                    && q == p
                    && final(self).purses() == old(self).purses()
                    && final(self).coins() == old(self).coins().remove_keys(
                        Set::new(|k: (PurseId, u64)| k.0 == p)
                    )
                    && final(self).entries() == old(self).entries().remove_keys(
                        Set::new(|k: (PurseId, u64)| k.0 == p)
                    ),
                Err(_) => false,
            },
    {
        if p == MAIN_PURSE {
            return Err(Error::CannotDeleteMainPurse);
        }

        // Purge coins, then entries belonging to p. If p isn't a known
        // purse, invariants (j)/(p) ensure no coin/entry has purse == p so
        // these are no-ops for the maps.
        self.purge_coins_of_purse(p);
        self.purge_entries_of_purse(p);

        let ghost old_v = self.purses@;
        let ghost old_m = self.spec_purses@;
        let ghost old_coins = self.spec_coins@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_entries = self.spec_entries@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_operations = self.spec_operations@;
        let ghost old_operations_vec = self.operations@;

        let mut i: usize = 0;
        while i < self.purses.len()
            invariant
                0 <= i <= self.purses.len(),
                self.invariant(),
                self.purses@ == old_v,
                self.spec_purses@ == old_m,
                self.spec_coins@ == old_coins,
                self.coins@ == old_coins_vec,
                self.spec_entries@ == old_entries,
                self.entries@ == old_entries_vec,
                self.spec_operations@ == old_operations,
                self.operations@ == old_operations_vec,
                self.spec_operations@ == old_operations,
                self.operations@ == old_operations_vec,
                self.next_handle == old(self).next_handle,
                self.next_age == old(self).next_age,
                self.fee_balance == old(self).fee_balance,
                self.next_extrinsic_id == old(self).next_extrinsic_id,
                self.events@ == old(self).events@,
                self.paid_ring_membership == old(self).paid_ring_membership,
                self.total_in == old(self).total_in,
                self.total_out == old(self).total_out,
                self.tokens@ == old(self).tokens@,
                self.chain_coins@ == old(self).chain_coins@,
                self.chain_entries@ == old(self).chain_entries@,
                old_m == old(self).spec_purses@,
                old_v == old(self).purses@,
                old_coins == old(self).coins().remove_keys(
                    Set::new(|k: (PurseId, u64)| k.0 == p)
                ),
                old_entries == old(self).entries().remove_keys(
                    Set::new(|k: (PurseId, u64)| k.0 == p)
                ),
                old_operations == old(self).operations(),
                self.next_purse_id == old(self).next_purse_id,
                p != MAIN_PURSE,
                forall|k: (PurseId, u64)| #[trigger] old_coins.dom().contains(k) ==> k.0 != p,
                forall|k: (PurseId, u64)| #[trigger] old_entries.dom().contains(k) ==> k.0 != p,
                forall|h: OpHandle| #[trigger] old_operations.dom().contains(h)
                    ==> old_operations[h].purse != p,
                forall|j: int| 0 <= j < i ==> (#[trigger] self.purses@[j]).id != p,
            decreases self.purses.len() - i,
        {
            if self.purses[i].id == p {
                let ghost target_idx = i as int;
                let _removed = self.purses.swap_remove(i);
                proof {
                    self.spec_purses = Ghost(self.spec_purses@.remove(p));
                    // No coin removal needed: precondition forbids any coin in p.

                    let new_v = self.purses@;
                    let new_m = self.spec_purses@;
                    let new_coins_map = self.spec_coins@;
                    let last_idx = old_v.len() - 1;

                    // Vec contents after swap_remove:
                    //   - new_v[k] == old_v[k] for k != target_idx, k < new_v.len()
                    //   - new_v[target_idx] == old_v[last_idx] if target_idx < last_idx
                    assert(new_v.len() == old_v.len() - 1);
                    assert forall|k: int| 0 <= k < new_v.len() && k != target_idx implies
                        #[trigger] new_v[k] == old_v[k]
                    by {}
                    assert(target_idx < new_v.len() ==> new_v[target_idx] == old_v[last_idx]);

                    // The removed id was p; by uniqueness, no other Vec entry had id == p.
                    assert(old_v[target_idx].id == p);
                    assert forall|k: int| 0 <= k < old_v.len() && k != target_idx implies
                        (#[trigger] old_v[k]).id != p
                    by {}

                    // p was in old_m.dom; remove(p) decreases dom by exactly {p}.
                    assert(old_m.dom().contains(p));
                    assert(new_m.dom() =~= old_m.dom().remove(p));

                    // (a) next_purse_id != MAIN_PURSE — unchanged.
                    assert(self.next_purse_id != MAIN_PURSE);
                    // (b) MAIN_PURSE in dom — p != MAIN_PURSE so removal preserves it.
                    assert(new_m.dom().contains(MAIN_PURSE));

                    // (c) forall q in dom. m[q].id == q
                    assert forall|q: PurseId| #[trigger] new_m.dom().contains(q)
                        implies new_m[q].id == q
                    by {
                        assert(old_m.dom().contains(q));
                    }

                    // (d) forall q in dom. q < next_purse_id
                    assert forall|q: PurseId| #[trigger] new_m.dom().contains(q)
                        implies q < self.next_purse_id
                    by {
                        assert(old_m.dom().contains(q));
                    }

                    // (e) every Vec entry's id is in dom
                    assert forall|k: int| 0 <= k < new_v.len() implies
                        new_m.dom().contains(#[trigger] new_v[k].id)
                    by {
                        if k == target_idx {
                            assert(new_v[k] == old_v[last_idx]);
                            assert(last_idx != target_idx);
                            assert(old_v[last_idx].id != p);
                            assert(old_m.dom().contains(old_v[last_idx].id));
                        } else {
                            assert(new_v[k] == old_v[k]);
                            assert(k != target_idx);
                            assert(old_v[k].id != p);
                            assert(old_m.dom().contains(old_v[k].id));
                        }
                    }

                    // (f) every Vec entry's spec view matches its dom entry
                    assert forall|k: int| 0 <= k < new_v.len() implies
                        new_m[(#[trigger] new_v[k]).id] == new_v[k]@
                    by {
                        if k == target_idx {
                            assert(new_v[k] == old_v[last_idx]);
                            assert(old_v[last_idx].id != p);
                            assert(old_m[old_v[last_idx].id] == old_v[last_idx]@);
                        } else {
                            assert(new_v[k] == old_v[k]);
                            assert(old_v[k].id != p);
                            assert(old_m[old_v[k].id] == old_v[k]@);
                        }
                    }

                    // (g) every dom key has a Vec witness
                    assert forall|q: PurseId| #[trigger] new_m.dom().contains(q)
                        implies exists|k: int| 0 <= k < new_v.len() && #[trigger] new_v[k].id == q
                    by {
                        assert(old_m.dom().contains(q));
                        let w_old = choose|k: int| 0 <= k < old_v.len() && #[trigger] old_v[k].id == q;
                        assert(old_v[w_old].id == q);
                        assert(q != p);
                        assert(w_old != target_idx);
                        if w_old == last_idx {
                            // The last element was moved to target_idx by swap_remove.
                            assert(target_idx < new_v.len());
                            assert(new_v[target_idx] == old_v[last_idx]);
                            assert(new_v[target_idx].id == q);
                        } else {
                            // Non-last, non-target: still at its original index in new_v.
                            assert(w_old < last_idx);
                            assert(w_old < new_v.len());
                            assert(new_v[w_old] == old_v[w_old]);
                        }
                    }

                    // (h) no duplicates
                    assert forall|a: int, b: int|
                        0 <= a < new_v.len() && 0 <= b < new_v.len()
                        && #[trigger] new_v[a].id == #[trigger] new_v[b].id
                        implies a == b
                    by {
                        if a == target_idx && b == target_idx {
                        } else if a == target_idx {
                            assert(new_v[a] == old_v[last_idx]);
                            assert(new_v[b] == old_v[b]);
                            assert(b != last_idx);
                        } else if b == target_idx {
                            assert(new_v[b] == old_v[last_idx]);
                            assert(new_v[a] == old_v[a]);
                            assert(a != last_idx);
                        } else {
                            assert(new_v[a] == old_v[a]);
                            assert(new_v[b] == old_v[b]);
                        }
                    }

                    // Coins are unchanged in this branch (purge happened pre-loop).
                    // Post-purge no coin in p remains, so removing p from
                    // purse map preserves (j): every coin's purse != p.
                    assert(self.spec_coins@ == old_coins);
                    assert(self.coins@ == old_coins_vec);
                    assert forall|k: (PurseId, u64)|
                        #[trigger] new_coins_map.dom().contains(k)
                    implies
                        new_m.dom().contains(k.0)
                    by {
                        assert(old_coins.dom().contains(k));
                        assert(k.0 != p);
                        assert(old_m.dom().contains(k.0));
                    }

                    // (k) unchanged: purses untouched for non-p; no coin has purse == p.
                    assert forall|k: (PurseId, u64)|
                        #[trigger] new_coins_map.dom().contains(k)
                    implies
                        k.1 < new_m[k.0].next_coin_idx
                    by {
                        assert(old_coins.dom().contains(k));
                        assert(k.0 != p);
                        assert(new_m[k.0] == old_m[k.0]);
                    }

                    // Entries-side: spec_entries is post-purge (no key with k.0 == p);
                    // self.entries Vec unchanged in this scan loop. Invariant (p) holds
                    // because remaining entries' purses are all != p, and removing p
                    // from spec_purses leaves them in dom.
                    assert(self.spec_entries@ == old_entries);
                    assert forall|k: (PurseId, u64)|
                        #[trigger] self.spec_entries@.dom().contains(k)
                    implies
                        new_m.dom().contains(k.0)
                    by {
                        assert(old_entries.dom().contains(k));
                        assert(k.0 != p);
                        assert(old_m.dom().contains(k.0));
                    }
                    assert forall|k: (PurseId, u64)|
                        #[trigger] self.spec_entries@.dom().contains(k)
                    implies
                        k.1 < new_m[k.0].next_entry_idx
                    by {
                        assert(old_entries.dom().contains(k));
                        assert(k.0 != p);
                        assert(new_m[k.0] == old_m[k.0]);
                    }

                    // Operations-side: spec_operations untouched; no op's
                    // purse equals p (loop invariant), so refint to new
                    // purses dom holds.
                    assert(self.spec_operations@ == old_operations);
                    assert forall|h: OpHandle|
                        #[trigger] self.spec_operations@.dom().contains(h)
                    implies
                        new_m.dom().contains(self.spec_operations@[h].purse)
                    by {
                        assert(old_operations.dom().contains(h));
                        assert(old_operations[h].purse != p);
                        assert(old_m.dom().contains(old_operations[h].purse));
                    }
                }
                return Ok(());
            }
            i += 1;
        }
        // Not found
        proof {
            if old_m.dom().contains(p) {
                let w = choose|k: int| 0 <= k < old_v.len() && #[trigger] old_v[k].id == p;
                assert(0 <= w < self.purses@.len());
                assert(self.purses@[w].id != p);
            }
        }
        Err(Error::PurseNotFound(p))
    }

    /// Allocate a fresh coin in purse `p` carrying a caller-supplied
    /// chain `account`. Quint analog: the bottom-layer effect of any
    /// op that delivers a coin (top-up, transfer destination,
    /// rebalance destination) to a specific chain account. The coin's
    /// `idx` is the purse's current `next_coin_idx`, after which the
    /// per-purse allocator is bumped. The coin's `age` is the
    /// state-global `next_age`, after which the global allocator is
    /// bumped — this gives a total order on coin creation suitable
    /// for the §6.3 priority ordering.
    pub fn add_coin_with_account(&mut self, p: PurseId, exponent: u8, account: u64)
        -> (key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(p),
            old(self).purses()[p].next_coin_idx < u64::MAX,
            old(self).next_age < u64::MAX,
            exponent <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            key.0 == p,
            key.1 == old(self).purses()[p].next_coin_idx,
            !old(self).coins().dom().contains(key),
            final(self).coins() == old(self).coins().insert(key, CoinRec {
                purse: p,
                idx: key.1,
                exponent,
                state: CoinState::Pending,
                age: old(self).next_age,
                account,
            }),
            final(self).next_age == old(self).next_age + 1,
            final(self).purses().dom() =~= old(self).purses().dom(),
            final(self).purses()[p].id == p,
            final(self).purses()[p].name == old(self).purses()[p].name,
            final(self).purses()[p].next_coin_idx
                == old(self).purses()[p].next_coin_idx + 1,
            final(self).purses()[p].next_entry_idx
                == old(self).purses()[p].next_entry_idx,
            forall|q: PurseId| q != p && #[trigger] old(self).purses().dom().contains(q)
                ==> final(self).purses()[q] == old(self).purses()[q],
            // Entries untouched.
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).events@ == old(self).events@,
    {
        let ghost old_v = self.purses@;
        let ghost old_m = self.spec_purses@;
        let ghost old_coins = self.spec_coins@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_entries = self.spec_entries@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_operations = self.spec_operations@;
        let ghost old_operations_vec = self.operations@;
        let ghost p_old_rec = old_m[p];

        let mut i: usize = 0;
        while i < self.purses.len()
            invariant
                0 <= i <= self.purses.len(),
                self.invariant(),
                exponent <= MAX_EXPONENT,
                self.purses@ == old_v,
                self.spec_purses@ == old_m,
                self.spec_coins@ == old_coins,
                self.coins@ == old_coins_vec,
                self.spec_entries@ == old_entries,
                self.entries@ == old_entries_vec,
                self.spec_operations@ == old_operations,
                self.operations@ == old_operations_vec,
                self.spec_operations@ == old_operations,
                self.operations@ == old_operations_vec,
                self.next_handle == old(self).next_handle,
                self.next_age == old(self).next_age,
                self.fee_balance == old(self).fee_balance,
                self.next_extrinsic_id == old(self).next_extrinsic_id,
                self.events@ == old(self).events@,
                self.paid_ring_membership == old(self).paid_ring_membership,
                self.total_in == old(self).total_in,
                self.total_out == old(self).total_out,
                self.tokens@ == old(self).tokens@,
                self.chain_coins@ == old(self).chain_coins@,
                self.chain_entries@ == old(self).chain_entries@,
                old_m == old(self).spec_purses@,
                old_v == old(self).purses@,
                old_coins == old(self).spec_coins@,
                old_coins_vec == old(self).coins@,
                old_entries == old(self).spec_entries@,
                old_entries == old(self).entries(),
                old_entries_vec == old(self).entries@,
                old_operations == old(self).spec_operations@,
                old_operations_vec == old(self).operations@,
                old_operations == old(self).spec_operations@,
                old_operations_vec == old(self).operations@,
                self.next_purse_id == old(self).next_purse_id,
                self.next_age == old(self).next_age,
                self.fee_balance == old(self).fee_balance,
                self.next_extrinsic_id == old(self).next_extrinsic_id,
                self.events@ == old(self).events@,
                self.paid_ring_membership == old(self).paid_ring_membership,
                self.total_in == old(self).total_in,
                self.total_out == old(self).total_out,
                self.tokens@ == old(self).tokens@,
                self.chain_coins@ == old(self).chain_coins@,
                self.chain_entries@ == old(self).chain_entries@,
                old(self).purses().dom().contains(p),
                p_old_rec == old_m[p],
                p_old_rec.next_coin_idx < u64::MAX,
                old(self).next_age < u64::MAX,
                forall|j: int| 0 <= j < i ==> (#[trigger] self.purses@[j]).id != p,
            decreases self.purses.len() - i,
        {
            if self.purses[i].id == p {
                let ghost target_idx = i as int;
                let cur_idx = self.purses[i].next_coin_idx;
                let cur_age = self.next_age;
                let ghost old_p_rec_at_idx = old_v[target_idx]@;
                self.purses[i].next_coin_idx = cur_idx + 1;
                self.next_age = cur_age + 1;

                let key = (p, cur_idx);
                let new_coin = CoinRec {
                    purse: p,
                    idx: cur_idx,
                    exponent,
                    state: CoinState::Pending,
                    age: cur_age,
                    account,
                };
                self.coins.push(new_coin);

                proof {
                    let new_p_rec_spec = PurseRecSpec {
                        id: p,
                        name: old_p_rec_at_idx.name,
                        next_coin_idx: (cur_idx + 1) as nat,
                        next_entry_idx: old_p_rec_at_idx.next_entry_idx,
                    };
                    self.spec_purses = Ghost(self.spec_purses@.insert(p, new_p_rec_spec));
                    self.spec_coins = Ghost(self.spec_coins@.insert(key, new_coin));

                    let new_v = self.purses@;
                    let new_m = self.spec_purses@;
                    let new_coins = self.spec_coins@;

                    // Vec post-state: only target_idx changed; only field
                    // `next_coin_idx` differs.
                    assert(new_v[target_idx].id == p);
                    assert(new_v[target_idx].next_coin_idx == cur_idx + 1);
                    assert(new_v[target_idx].name == old_v[target_idx].name);
                    assert(new_v[target_idx].next_entry_idx == old_v[target_idx].next_entry_idx);
                    assert forall|k: int| 0 <= k < new_v.len() && k != target_idx implies
                        #[trigger] new_v[k] == old_v[k]
                    by {}
                    assert(old_v[target_idx].id == p);
                    assert forall|k: int| 0 <= k < old_v.len() && k != target_idx implies
                        (#[trigger] old_v[k]).id != p
                    by {}

                    // p was already in old_m.dom — insert leaves dom unchanged.
                    assert(old_m.dom().contains(p));
                    assert(new_m.dom() =~= old_m.dom());

                    // The new coin key was not previously a member.
                    assert forall|k: (PurseId, u64)| #[trigger] old_coins.dom().contains(k)
                        implies k != key
                    by {
                        assert(k.1 < old_m[k.0].next_coin_idx);
                        if k.0 == p {
                            assert(k.1 < cur_idx);
                        }
                    }
                    assert(!old_coins.dom().contains(key));

                    // (a) next_purse_id unchanged.
                    assert(self.next_purse_id != MAIN_PURSE);
                    // (b) MAIN_PURSE in dom unchanged.
                    assert(new_m.dom().contains(MAIN_PURSE));

                    // (c) forall q in dom. m[q].id == q
                    assert forall|q: PurseId| #[trigger] new_m.dom().contains(q)
                        implies new_m[q].id == q
                    by {
                        if q == p {
                        } else {
                            assert(old_m.dom().contains(q));
                        }
                    }

                    // (d) forall q in dom. q < next_purse_id
                    assert forall|q: PurseId| #[trigger] new_m.dom().contains(q)
                        implies q < self.next_purse_id
                    by {
                        assert(old_m.dom().contains(q));
                    }

                    // (e) every Vec entry's id is in dom
                    assert forall|k: int| 0 <= k < new_v.len() implies
                        new_m.dom().contains(#[trigger] new_v[k].id)
                    by {
                        if k == target_idx {
                        } else {
                            assert(new_v[k] == old_v[k]);
                            assert(old_m.dom().contains(old_v[k].id));
                        }
                    }

                    // (f) every Vec entry's spec view matches its dom entry
                    assert forall|k: int| 0 <= k < new_v.len() implies
                        new_m[(#[trigger] new_v[k]).id] == new_v[k]@
                    by {
                        if k == target_idx {
                            assert(new_v[k].id == p);
                            assert(new_v[k]@ == new_p_rec_spec);
                        } else {
                            assert(new_v[k] == old_v[k]);
                            assert(old_v[k].id != p);
                            assert(old_m[old_v[k].id] == old_v[k]@);
                        }
                    }

                    // (g) every dom key has a Vec witness
                    assert forall|q: PurseId| #[trigger] new_m.dom().contains(q)
                        implies exists|k: int| 0 <= k < new_v.len() && #[trigger] new_v[k].id == q
                    by {
                        if q == p {
                            let w = target_idx;
                            assert(new_v[w].id == p);
                        } else {
                            assert(old_m.dom().contains(q));
                            let w = choose|k: int| 0 <= k < old_v.len() && #[trigger] old_v[k].id == q;
                            assert(w != target_idx);
                            assert(new_v[w] == old_v[w]);
                        }
                    }

                    // (h) no duplicates
                    assert forall|a: int, b: int|
                        0 <= a < new_v.len() && 0 <= b < new_v.len()
                        && #[trigger] new_v[a].id == #[trigger] new_v[b].id
                        implies a == b
                    by {
                        if a == target_idx && b == target_idx {
                        } else if a == target_idx {
                            assert(new_v[b] == old_v[b]);
                        } else if b == target_idx {
                            assert(new_v[a] == old_v[a]);
                        } else {
                            assert(new_v[a] == old_v[a]);
                            assert(new_v[b] == old_v[b]);
                        }
                    }

                    // (i) coin key consistency.
                    assert forall|k: (PurseId, u64)| #[trigger] new_coins.dom().contains(k)
                        implies new_coins[k].purse == k.0 && new_coins[k].idx == k.1
                    by {
                        if k == key {
                        } else {
                            assert(old_coins.dom().contains(k));
                        }
                    }

                    // (j) coin referential integrity.
                    assert forall|k: (PurseId, u64)| #[trigger] new_coins.dom().contains(k)
                        implies new_m.dom().contains(k.0)
                    by {
                        if k == key {
                        } else {
                            assert(old_coins.dom().contains(k));
                            assert(old_m.dom().contains(k.0));
                        }
                    }

                    // (k) coin idx below purse's allocator.
                    assert forall|k: (PurseId, u64)| #[trigger] new_coins.dom().contains(k)
                        implies k.1 < new_m[k.0].next_coin_idx
                    by {
                        if k == key {
                            assert(new_m[p].next_coin_idx == cur_idx + 1);
                        } else {
                            assert(old_coins.dom().contains(k));
                            assert(k.1 < old_m[k.0].next_coin_idx);
                            if k.0 == p {
                                assert(new_m[p].next_coin_idx == old_m[p].next_coin_idx + 1);
                            } else {
                                assert(new_m[k.0] == old_m[k.0]);
                            }
                        }
                    }

                    // (l, m, n) coin-Vec ↔ ghost refinement, post-push.
                    let new_coins_vec = self.coins@;
                    let last = old_coins_vec.len() as int;
                    assert(new_coins_vec.len() == old_coins_vec.len() + 1);
                    assert(new_coins_vec[last] == new_coin);
                    assert forall|k: int| 0 <= k < old_coins_vec.len() implies
                        new_coins_vec[k] == #[trigger] old_coins_vec[k]
                    by {}

                    // No old Vec entry could have key (p, cur_idx):
                    // by old invariant (k), every old coin's idx < old_m[purse].next_coin_idx;
                    // for purse == p, that's < cur_idx. So no collision.
                    assert forall|jj: int| 0 <= jj < old_coins_vec.len() implies
                        (#[trigger] old_coins_vec[jj]).purse != p
                        || old_coins_vec[jj].idx < cur_idx
                    by {
                        let oc = old_coins_vec[jj];
                        assert(old_coins.dom().contains((oc.purse, oc.idx)));
                        if oc.purse == p {
                            assert(old_m[p].next_coin_idx == cur_idx as nat);
                        }
                    }

                    // (l): each new Vec entry's (purse, idx) is in new dom and matches.
                    assert forall|jj: int| 0 <= jj < new_coins_vec.len() implies
                        new_coins.dom().contains(
                            (#[trigger] new_coins_vec[jj].purse, new_coins_vec[jj].idx)
                        )
                        && new_coins[(new_coins_vec[jj].purse, new_coins_vec[jj].idx)]
                            == new_coins_vec[jj]
                    by {
                        if jj == last {
                            assert(new_coins_vec[jj] == new_coin);
                            assert(new_coins[key] == new_coin);
                        } else {
                            assert(new_coins_vec[jj] == old_coins_vec[jj]);
                            let oc = old_coins_vec[jj];
                            assert(old_coins.dom().contains((oc.purse, oc.idx)));
                            assert((oc.purse, oc.idx) != key);
                            assert(old_coins[(oc.purse, oc.idx)] == oc);
                        }
                    }

                    // (m): every dom key has a Vec witness.
                    assert forall|k: (PurseId, u64)| #[trigger] new_coins.dom().contains(k)
                        implies exists|jj: int|
                            0 <= jj < new_coins_vec.len()
                            && #[trigger] new_coins_vec[jj].purse == k.0
                            && new_coins_vec[jj].idx == k.1
                    by {
                        if k == key {
                            let w = last;
                            assert(new_coins_vec[w].purse == p);
                            assert(new_coins_vec[w].idx == cur_idx);
                        } else {
                            assert(old_coins.dom().contains(k));
                            let w = choose|jj: int|
                                0 <= jj < old_coins_vec.len()
                                && #[trigger] old_coins_vec[jj].purse == k.0
                                && old_coins_vec[jj].idx == k.1;
                            assert(new_coins_vec[w] == old_coins_vec[w]);
                        }
                    }

                    // (n): no duplicate (purse, idx) in Vec.
                    assert forall|a: int, b: int|
                        0 <= a < new_coins_vec.len() && 0 <= b < new_coins_vec.len()
                        && (#[trigger] new_coins_vec[a]).purse
                            == (#[trigger] new_coins_vec[b]).purse
                        && new_coins_vec[a].idx == new_coins_vec[b].idx
                        implies a == b
                    by {
                        if a == last && b == last {
                        } else if a == last {
                            assert(new_coins_vec[b] == old_coins_vec[b]);
                            assert(new_coins_vec[a].purse == p);
                            assert(new_coins_vec[a].idx == cur_idx);
                        } else if b == last {
                            assert(new_coins_vec[a] == old_coins_vec[a]);
                            assert(new_coins_vec[b].purse == p);
                            assert(new_coins_vec[b].idx == cur_idx);
                        } else {
                            assert(new_coins_vec[a] == old_coins_vec[a]);
                            assert(new_coins_vec[b] == old_coins_vec[b]);
                        }
                    }

                    // (aa) every coin's exponent <= MAX_EXPONENT.
                    assert(new_coin.exponent == exponent);
                    assert(exponent <= MAX_EXPONENT);
                    assert forall|kk: (PurseId, u64)| #[trigger] new_coins.dom().contains(kk)
                        implies new_coins[kk].exponent <= MAX_EXPONENT
                    by {
                        if kk == key {
                            assert(new_coins[kk] == new_coin);
                        } else {
                            // kk is in old_coins (since new_coins = insert(key, _) and kk != key)
                            assert(old_coins.dom().contains(kk));
                            // Map::insert axiom: insert(k, v)[k'] == m[k'] for k' != k
                            assert(new_coins[kk] == old_coins[kk]);
                            // old (aa) gives the bound on old_coins[kk]
                            assert(old_coins[kk].exponent <= MAX_EXPONENT);
                            assert(new_coins[kk].exponent == old_coins[kk].exponent);
                        }
                    }
                }
                return key;
            }
            i += 1;
        }
        // Unreachable: p is in old(self).purses().dom() by precondition,
        // so the invariant guarantees the scan must find it.
        proof {
            assert(old_m.dom().contains(p));
            let w = choose|k: int| 0 <= k < old_v.len() && #[trigger] old_v[k].id == p;
            assert(0 <= w < old_v.len());
            assert(self.purses@[w].id != p);
        }
        vstd::pervasive::unreached()
    }

    /// Allocate a fresh coin in purse `p` without specifying its chain
    /// account. Thin wrapper over [`Self::add_coin_with_account`] that
    /// passes `account = 0` — used by callers that don't yet thread the
    /// chain side (transfer, rebalance, split_coin, top_up_purse).
    pub fn add_coin(&mut self, p: PurseId, exponent: u8) -> (key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(p),
            old(self).purses()[p].next_coin_idx < u64::MAX,
            old(self).next_age < u64::MAX,
            exponent <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            key.0 == p,
            key.1 == old(self).purses()[p].next_coin_idx,
            !old(self).coins().dom().contains(key),
            final(self).coins() == old(self).coins().insert(key, CoinRec {
                purse: p,
                idx: key.1,
                exponent,
                state: CoinState::Pending,
                age: old(self).next_age,
                account: 0,
            }),
            final(self).next_age == old(self).next_age + 1,
            final(self).purses().dom() =~= old(self).purses().dom(),
            final(self).purses()[p].id == p,
            final(self).purses()[p].name == old(self).purses()[p].name,
            final(self).purses()[p].next_coin_idx
                == old(self).purses()[p].next_coin_idx + 1,
            final(self).purses()[p].next_entry_idx
                == old(self).purses()[p].next_entry_idx,
            forall|q: PurseId| q != p && #[trigger] old(self).purses().dom().contains(q)
                ==> final(self).purses()[q] == old(self).purses()[q],
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).events@ == old(self).events@,
    {
        self.add_coin_with_account(p, exponent, 0)
    }

    /// Allocate a fresh recycler entry in purse `p` with full chain
    /// bookkeeping: `exponent`, `on_chain`/`local` lifecycle states, and
    /// the four chain-side metadata fields (`member_key`, `allocated_at`,
    /// `ready_at`, `ring_idx`). The entry's `idx` is the purse's current
    /// `next_entry_idx`, after which the allocator is bumped. Quint
    /// analog: the bottom-layer effect of `topUp`'s entry construction.
    pub fn add_entry_with_meta(
        &mut self,
        p: PurseId,
        exponent: u8,
        on_chain: EntryOnChain,
        local: EntryLocal,
        member_key: u64,
        allocated_at: u64,
        ready_at: u64,
        ring_idx: u64,
    ) -> (key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(p),
            old(self).purses()[p].next_entry_idx < u64::MAX,
            exponent <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            key.0 == p,
            key.1 == old(self).purses()[p].next_entry_idx,
            !old(self).entries().dom().contains(key),
            final(self).entries() == old(self).entries().insert(key, EntryRec {
                purse: p,
                idx: key.1,
                exponent,
                on_chain,
                local,
                member_key,
                allocated_at,
                ready_at,
                ring_idx,
            }),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).purses().dom() =~= old(self).purses().dom(),
            final(self).purses()[p].id == p,
            final(self).purses()[p].name == old(self).purses()[p].name,
            final(self).purses()[p].next_coin_idx
                == old(self).purses()[p].next_coin_idx,
            final(self).purses()[p].next_entry_idx
                == old(self).purses()[p].next_entry_idx + 1,
            forall|q: PurseId| q != p && #[trigger] old(self).purses().dom().contains(q)
                ==> final(self).purses()[q] == old(self).purses()[q],
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let ghost old_v = self.purses@;
        let ghost old_m = self.spec_purses@;
        let ghost old_entries = self.spec_entries@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_operations = self.spec_operations@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_coins = self.spec_coins@;
        let ghost p_old_rec = old_m[p];

        let mut i: usize = 0;
        while i < self.purses.len()
            invariant
                0 <= i <= self.purses.len(),
                self.invariant(),
                exponent <= MAX_EXPONENT,
                self.purses@ == old_v,
                self.spec_purses@ == old_m,
                self.spec_coins@ == old_coins,
                self.coins@ == old(self).coins@,
                self.spec_entries@ == old_entries,
                self.entries@ == old_entries_vec,
                self.spec_operations@ == old_operations,
                self.operations@ == old_operations_vec,
                old_operations == old(self).spec_operations@,
                old_operations_vec == old(self).operations@,
                self.next_handle == old(self).next_handle,
                self.next_age == old(self).next_age,
                self.fee_balance == old(self).fee_balance,
                self.next_extrinsic_id == old(self).next_extrinsic_id,
                self.events@ == old(self).events@,
                self.paid_ring_membership == old(self).paid_ring_membership,
                self.total_in == old(self).total_in,
                self.total_out == old(self).total_out,
                self.tokens@ == old(self).tokens@,
                self.chain_coins@ == old(self).chain_coins@,
                self.chain_entries@ == old(self).chain_entries@,
                old_m == old(self).spec_purses@,
                old_v == old(self).purses@,
                old_entries == old(self).spec_entries@,
                old_entries_vec == old(self).entries@,
                old_coins == old(self).spec_coins@,
                self.next_purse_id == old(self).next_purse_id,
                old(self).purses().dom().contains(p),
                p_old_rec == old_m[p],
                p_old_rec.next_entry_idx < u64::MAX,
                forall|j: int| 0 <= j < i ==> (#[trigger] self.purses@[j]).id != p,
            decreases self.purses.len() - i,
        {
            if self.purses[i].id == p {
                let ghost target_idx = i as int;
                let cur_idx = self.purses[i].next_entry_idx;
                let ghost old_p_rec_at_idx = old_v[target_idx]@;
                self.purses[i].next_entry_idx = cur_idx + 1;

                let key = (p, cur_idx);
                let new_entry = EntryRec {
                    purse: p,
                    idx: cur_idx,
                    exponent,
                    on_chain,
                    local,
                    member_key,
                    allocated_at,
                    ready_at,
                    ring_idx,
                };
                self.entries.push(new_entry);

                proof {
                    let new_p_rec_spec = PurseRecSpec {
                        id: p,
                        name: old_p_rec_at_idx.name,
                        next_coin_idx: old_p_rec_at_idx.next_coin_idx,
                        next_entry_idx: (cur_idx + 1) as nat,
                    };
                    self.spec_purses = Ghost(self.spec_purses@.insert(p, new_p_rec_spec));
                    self.spec_entries = Ghost(self.spec_entries@.insert(key, new_entry));

                    let new_v = self.purses@;
                    let new_m = self.spec_purses@;
                    let new_entries = self.spec_entries@;

                    // Purse-side post-state for (e-h).
                    assert(new_v[target_idx].id == p);
                    assert(new_v[target_idx].next_entry_idx == cur_idx + 1);
                    assert(new_v[target_idx].next_coin_idx == old_v[target_idx].next_coin_idx);
                    assert(new_v[target_idx].name == old_v[target_idx].name);
                    assert forall|k: int| 0 <= k < new_v.len() && k != target_idx implies
                        #[trigger] new_v[k] == old_v[k]
                    by {}
                    assert(old_v[target_idx].id == p);
                    assert forall|k: int| 0 <= k < old_v.len() && k != target_idx implies
                        (#[trigger] old_v[k]).id != p
                    by {}
                    assert(old_m.dom().contains(p));
                    assert(new_m.dom() =~= old_m.dom());

                    // New entry key is fresh: by (q) old, every entry's idx <
                    // old_m[purse].next_entry_idx. For purse == p that's < cur_idx.
                    assert forall|k: (PurseId, u64)| #[trigger] old_entries.dom().contains(k)
                        implies k != key
                    by {
                        assert(k.1 < old_m[k.0].next_entry_idx);
                        if k.0 == p {
                            assert(k.1 < cur_idx);
                        }
                    }
                    assert(!old_entries.dom().contains(key));

                    // Purse-side (a-h) — re-prove as in add_coin.
                    assert(self.next_purse_id != MAIN_PURSE);
                    assert(new_m.dom().contains(MAIN_PURSE));
                    assert forall|q: PurseId| #[trigger] new_m.dom().contains(q)
                        implies new_m[q].id == q
                    by { if q != p { assert(old_m.dom().contains(q)); } }
                    assert forall|q: PurseId| #[trigger] new_m.dom().contains(q)
                        implies q < self.next_purse_id
                    by { assert(old_m.dom().contains(q)); }
                    assert forall|k: int| 0 <= k < new_v.len() implies
                        new_m.dom().contains(#[trigger] new_v[k].id)
                    by {
                        if k != target_idx {
                            assert(new_v[k] == old_v[k]);
                            assert(old_m.dom().contains(old_v[k].id));
                        }
                    }
                    assert forall|k: int| 0 <= k < new_v.len() implies
                        new_m[(#[trigger] new_v[k]).id] == new_v[k]@
                    by {
                        if k == target_idx {
                            assert(new_v[k].id == p);
                            assert(new_v[k]@ == new_p_rec_spec);
                        } else {
                            assert(new_v[k] == old_v[k]);
                            assert(old_v[k].id != p);
                            assert(old_m[old_v[k].id] == old_v[k]@);
                        }
                    }
                    assert forall|q: PurseId| #[trigger] new_m.dom().contains(q)
                        implies exists|k: int| 0 <= k < new_v.len() && #[trigger] new_v[k].id == q
                    by {
                        if q == p {
                            let w = target_idx;
                            assert(new_v[w].id == p);
                        } else {
                            assert(old_m.dom().contains(q));
                            let w = choose|k: int| 0 <= k < old_v.len() && #[trigger] old_v[k].id == q;
                            assert(w != target_idx);
                            assert(new_v[w] == old_v[w]);
                        }
                    }
                    assert forall|a: int, b: int|
                        0 <= a < new_v.len() && 0 <= b < new_v.len()
                        && #[trigger] new_v[a].id == #[trigger] new_v[b].id
                        implies a == b
                    by {
                        if a == target_idx && b == target_idx {
                        } else if a == target_idx {
                            assert(new_v[b] == old_v[b]);
                        } else if b == target_idx {
                            assert(new_v[a] == old_v[a]);
                        } else {
                            assert(new_v[a] == old_v[a]);
                            assert(new_v[b] == old_v[b]);
                        }
                    }

                    // (i, j, k) coin-side unchanged since spec_coins and self.coins
                    // are untouched. Only thing to re-prove for (k): for coin keys
                    // with purse == p, new_m[p].next_coin_idx still equals old.
                    assert forall|k: (PurseId, u64)| #[trigger] self.spec_coins@.dom().contains(k)
                        implies k.1 < new_m[k.0].next_coin_idx
                    by {
                        assert(old_coins.dom().contains(k));
                        assert(k.1 < old_m[k.0].next_coin_idx);
                        if k.0 == p {
                            assert(new_m[p].next_coin_idx == old_m[p].next_coin_idx);
                        } else {
                            assert(new_m[k.0] == old_m[k.0]);
                        }
                    }

                    // (o) entry key consistency.
                    assert forall|k: (PurseId, u64)| #[trigger] new_entries.dom().contains(k)
                        implies new_entries[k].purse == k.0 && new_entries[k].idx == k.1
                    by {
                        if k != key { assert(old_entries.dom().contains(k)); }
                    }

                    // (p) entry refint.
                    assert forall|k: (PurseId, u64)| #[trigger] new_entries.dom().contains(k)
                        implies new_m.dom().contains(k.0)
                    by {
                        if k != key {
                            assert(old_entries.dom().contains(k));
                            assert(old_m.dom().contains(k.0));
                        }
                    }

                    // (q) entry idx below next_entry_idx.
                    assert forall|k: (PurseId, u64)| #[trigger] new_entries.dom().contains(k)
                        implies k.1 < new_m[k.0].next_entry_idx
                    by {
                        if k == key {
                            assert(new_m[p].next_entry_idx == cur_idx + 1);
                        } else {
                            assert(old_entries.dom().contains(k));
                            assert(k.1 < old_m[k.0].next_entry_idx);
                            if k.0 == p {
                                assert(new_m[p].next_entry_idx == old_m[p].next_entry_idx + 1);
                            } else {
                                assert(new_m[k.0] == old_m[k.0]);
                            }
                        }
                    }

                    // (r, s, t) entry Vec ↔ ghost refinement post-push.
                    let new_entries_vec = self.entries@;
                    let last = old_entries_vec.len() as int;
                    assert(new_entries_vec.len() == old_entries_vec.len() + 1);
                    assert(new_entries_vec[last] == new_entry);
                    assert forall|k: int| 0 <= k < old_entries_vec.len() implies
                        new_entries_vec[k] == #[trigger] old_entries_vec[k]
                    by {}
                    // No old Vec entry collides with the new key.
                    assert forall|jj: int| 0 <= jj < old_entries_vec.len() implies
                        (#[trigger] old_entries_vec[jj]).purse != p
                        || old_entries_vec[jj].idx < cur_idx
                    by {
                        let oe = old_entries_vec[jj];
                        assert(old_entries.dom().contains((oe.purse, oe.idx)));
                        if oe.purse == p {
                            assert(old_m[p].next_entry_idx == cur_idx as nat);
                        }
                    }
                    // (r)
                    assert forall|jj: int| 0 <= jj < new_entries_vec.len() implies
                        new_entries.dom().contains(
                            (#[trigger] new_entries_vec[jj].purse, new_entries_vec[jj].idx)
                        )
                        && new_entries[(new_entries_vec[jj].purse, new_entries_vec[jj].idx)]
                            == new_entries_vec[jj]
                    by {
                        if jj == last {
                            assert(new_entries_vec[jj] == new_entry);
                            assert(new_entries[key] == new_entry);
                        } else {
                            assert(new_entries_vec[jj] == old_entries_vec[jj]);
                            let oe = old_entries_vec[jj];
                            assert(old_entries.dom().contains((oe.purse, oe.idx)));
                            assert((oe.purse, oe.idx) != key);
                            assert(old_entries[(oe.purse, oe.idx)] == oe);
                        }
                    }
                    // (s)
                    assert forall|k: (PurseId, u64)| #[trigger] new_entries.dom().contains(k)
                        implies exists|jj: int|
                            0 <= jj < new_entries_vec.len()
                            && #[trigger] new_entries_vec[jj].purse == k.0
                            && new_entries_vec[jj].idx == k.1
                    by {
                        if k == key {
                            let w = last;
                            assert(new_entries_vec[w].purse == p);
                            assert(new_entries_vec[w].idx == cur_idx);
                        } else {
                            assert(old_entries.dom().contains(k));
                            let w = choose|jj: int|
                                0 <= jj < old_entries_vec.len()
                                && #[trigger] old_entries_vec[jj].purse == k.0
                                && old_entries_vec[jj].idx == k.1;
                            assert(new_entries_vec[w] == old_entries_vec[w]);
                        }
                    }
                    // (t)
                    assert forall|a: int, b: int|
                        0 <= a < new_entries_vec.len() && 0 <= b < new_entries_vec.len()
                        && (#[trigger] new_entries_vec[a]).purse
                            == (#[trigger] new_entries_vec[b]).purse
                        && new_entries_vec[a].idx == new_entries_vec[b].idx
                        implies a == b
                    by {
                        if a == last && b == last {
                        } else if a == last {
                            assert(new_entries_vec[b] == old_entries_vec[b]);
                            assert(new_entries_vec[a].purse == p);
                            assert(new_entries_vec[a].idx == cur_idx);
                        } else if b == last {
                            assert(new_entries_vec[a] == old_entries_vec[a]);
                            assert(new_entries_vec[b].purse == p);
                            assert(new_entries_vec[b].idx == cur_idx);
                        } else {
                            assert(new_entries_vec[a] == old_entries_vec[a]);
                            assert(new_entries_vec[b] == old_entries_vec[b]);
                        }
                    }

                    // (ab) every entry's exponent <= MAX_EXPONENT.
                    assert forall|kk: (PurseId, u64)| #[trigger] new_entries.dom().contains(kk)
                        implies new_entries[kk].exponent <= MAX_EXPONENT
                    by {
                        if kk == key {
                            assert(new_entries[key] == new_entry);
                            assert(new_entry.exponent == exponent);
                        } else {
                            assert(old_entries.dom().contains(kk));
                            assert(new_entries[kk] == old_entries[kk]);
                            assert(old_entries[kk].exponent <= MAX_EXPONENT);
                        }
                    }
                }
                return key;
            }
            i += 1;
        }
        proof {
            assert(old_m.dom().contains(p));
            let w = choose|k: int| 0 <= k < old_v.len() && #[trigger] old_v[k].id == p;
            assert(0 <= w < old_v.len());
            assert(self.purses@[w].id != p);
        }
        vstd::pervasive::unreached()
    }

    /// Atomic composite: commit an op that's holding one locked entry.
    /// Consumes the entry (`LocalLockedFor → LocalConsumed`) and
    /// marks the op `Done`. Used by the commit path of unload /
    /// external-offload when the chain has confirmed the entry-spend
    /// extrinsic.
    pub fn commit_op_consuming_locked_entry(
        &mut self,
        handle: OpHandle,
        key: (PurseId, u64),
    )
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            old(self).operations()[handle].status == OpStatus::Finalized,
            old(self).entries().dom().contains(key),
            old(self).entries()[key].local == EntryLocal::LocalLockedFor(handle),
            old(self).events@.len() + 2 <= u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).entries().dom().contains(key),
            final(self).entries()[key].local == EntryLocal::LocalConsumed,
            final(self).operations().dom().contains(handle),
            final(self).operations()[handle].status == OpStatus::Done,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@
                .push(Event::EntryConsumed {
                    purse: key.0,
                    exponent: old(self).entries()[key].exponent,
                })
                .push(Event::OperationCompleted {
                    handle,
                    status: OpStatus::Done,
                }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        self.consume_entry(key);
        self.mark_op_done(handle);
    }

    /// Atomic composite: commit an op that's holding one locked coin.
    /// Consumes the coin (`LockedFor → PendingSpend → Spent`) and
    /// marks the op `Done`. Used by the commit path of transfer /
    /// rebalance / export when the chain has finalized the spend.
    pub fn commit_op_consuming_locked_coin(
        &mut self,
        handle: OpHandle,
        key: (PurseId, u64),
    )
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            old(self).operations()[handle].status == OpStatus::Finalized,
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::LockedFor(handle),
            old(self).events@.len() + 2 <= u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins().dom().contains(key),
            final(self).coins()[key].state == CoinState::Spent,
            final(self).operations().dom().contains(handle),
            final(self).operations()[handle].status == OpStatus::Done,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@
                .push(Event::CoinSpent {
                    purse: key.0,
                    exponent: old(self).coins()[key].exponent,
                })
                .push(Event::OperationCompleted {
                    handle,
                    status: OpStatus::Done,
                }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        self.commit_locked_coin(key);
        self.mark_coin_spent(key);
        self.mark_op_done(handle);
    }

    /// Atomic composite: cancel an op that's holding one locked coin.
    /// Releases the coin back to `Available` and marks the op
    /// `Failed`. Inverse of [`Self::start_op_locking_coin`] (when the
    /// op was started and the lock holds but the op hasn't progressed
    /// beyond `Preparing` / `Waiting(_)`).
    pub fn cancel_op_releasing_coin(
        &mut self,
        handle: OpHandle,
        key: (PurseId, u64),
    )
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            match old(self).operations()[handle].status {
                OpStatus::Preparing => true,
                OpStatus::Waiting(_) => true,
                _ => false,
            },
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::LockedFor(handle),
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins().dom().contains(key),
            final(self).coins()[key].state == CoinState::Available,
            final(self).operations().dom().contains(handle),
            final(self).operations()[handle].status == OpStatus::Failed,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::OperationCompleted {
                handle,
                status: OpStatus::Failed,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        self.release_locked_coin(key, handle);
        self.set_op_failed(handle);
    }

    /// Atomic composite: cancel an op that's holding one locked entry.
    /// Releases the entry back to `LocalAvailable` and marks the op
    /// `Failed`. Inverse of [`Self::start_op_locking_entry`].
    pub fn cancel_op_releasing_entry(
        &mut self,
        handle: OpHandle,
        key: (PurseId, u64),
    )
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            match old(self).operations()[handle].status {
                OpStatus::Preparing => true,
                OpStatus::Waiting(_) => true,
                _ => false,
            },
            old(self).entries().dom().contains(key),
            old(self).entries()[key].local == EntryLocal::LocalLockedFor(handle),
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).entries().dom().contains(key),
            final(self).entries()[key].local == EntryLocal::LocalAvailable,
            final(self).operations().dom().contains(handle),
            final(self).operations()[handle].status == OpStatus::Failed,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::OperationCompleted {
                handle,
                status: OpStatus::Failed,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        self.release_locked_entry(key, handle);
        self.set_op_failed(handle);
    }

    /// Atomic composite: start a new operation and lock `key`'s coin
    /// for it. The coin must currently be `Available`; on return it
    /// is `LockedFor(handle)`, and the operation is in `Preparing`.
    ///
    /// This is the canonical entry point for op flows that reserve a
    /// specific coin upfront (transfer, rebalance, export). Avoids
    /// the temporal-gap problem of separately starting the op then
    /// locking the coin, where another concurrent call could observe
    /// the half-built state.
    /// Atomic composite: start a new operation and lock `key`'s entry
    /// for it. The entry must currently be `LocalAvailable`; on
    /// return it is `LocalLockedFor(handle)`, and the operation is
    /// in `Preparing`. Mirror of [`Self::start_op_locking_coin`] for
    /// recycler-entry-bearing op flows (unload, external offload).
    pub fn start_op_locking_entry(
        &mut self,
        kind: OpKind,
        key: (PurseId, u64),
    ) -> (handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).entries().dom().contains(key),
            old(self).entries()[key].local == EntryLocal::LocalAvailable,
            old(self).purses().dom().contains(key.0),
            old(self).next_handle < u64::MAX,
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            handle == old(self).next_handle,
            final(self).operations().dom().contains(handle),
            final(self).operations()[handle].status == OpStatus::Preparing,
            final(self).operations()[handle].kind == kind,
            final(self).operations()[handle].purse == key.0,
            final(self).entries().dom().contains(key),
            final(self).entries()[key].local == EntryLocal::LocalLockedFor(handle),
            final(self).entries()[key].on_chain == old(self).entries()[key].on_chain,
            final(self).entries()[key].exponent == old(self).entries()[key].exponent,
            final(self).next_handle == old(self).next_handle + 1,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::OperationStarted {
                handle,
                kind,
                purse: key.0,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let handle = self.start_op(kind, key.0);
        proof {
            assert(self.entries()[key].local == EntryLocal::LocalAvailable);
        }
        self.lock_entry(key, handle);
        handle
    }

    pub fn start_op_locking_coin(
        &mut self,
        kind: OpKind,
        key: (PurseId, u64),
    ) -> (handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::Available,
            old(self).purses().dom().contains(key.0),
            old(self).next_handle < u64::MAX,
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            handle == old(self).next_handle,
            final(self).operations().dom().contains(handle),
            final(self).operations()[handle].status == OpStatus::Preparing,
            final(self).operations()[handle].kind == kind,
            final(self).operations()[handle].purse == key.0,
            final(self).coins().dom().contains(key),
            final(self).coins()[key].state == CoinState::LockedFor(handle),
            final(self).coins()[key].exponent == old(self).coins()[key].exponent,
            final(self).next_handle == old(self).next_handle + 1,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::OperationStarted {
                handle,
                kind,
                purse: key.0,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let handle = self.start_op(kind, key.0);
        proof {
            assert(self.coins()[key].state == CoinState::Available);
        }
        self.lock_coin(key, handle);
        handle
    }

    /// Allocate a fresh recycler entry without chain bookkeeping. Thin
    /// wrapper over [`Self::add_entry_with_meta`] that supplies zero
    /// placeholders for `member_key`, `allocated_at`, `ready_at`, and
    /// `ring_idx`. Used by callers that don't yet model the chain side
    /// (notably `reserve_entries`).
    pub fn add_entry(
        &mut self,
        p: PurseId,
        exponent: u8,
        on_chain: EntryOnChain,
        local: EntryLocal,
    ) -> (key: (PurseId, u64))
        requires
            old(self).invariant(),
            exponent <= MAX_EXPONENT,
            old(self).purses().dom().contains(p),
            old(self).purses()[p].next_entry_idx < u64::MAX,
        ensures
            final(self).invariant(),
            key.0 == p,
            key.1 == old(self).purses()[p].next_entry_idx,
            !old(self).entries().dom().contains(key),
            final(self).entries() == old(self).entries().insert(key, EntryRec {
                purse: p,
                idx: key.1,
                exponent,
                on_chain,
                local,
                member_key: 0,
                allocated_at: 0,
                ready_at: 0,
                ring_idx: 0,
            }),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).purses().dom() =~= old(self).purses().dom(),
            final(self).purses()[p].id == p,
            final(self).purses()[p].name == old(self).purses()[p].name,
            final(self).purses()[p].next_coin_idx
                == old(self).purses()[p].next_coin_idx,
            final(self).purses()[p].next_entry_idx
                == old(self).purses()[p].next_entry_idx + 1,
            forall|q: PurseId| q != p && #[trigger] old(self).purses().dom().contains(q)
                ==> final(self).purses()[q] == old(self).purses()[q],
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        self.add_entry_with_meta(p, exponent, on_chain, local, 0, 0, 0, 0)
    }

    /// Start a new operation in the `Preparing` state. Allocates a fresh
    /// `OpHandle` from the layer's allocator. Quint analog: the local-
    /// state effect of starting any operation kind (the chain interaction
    /// is deferred to `transition_op_status`).
    pub fn start_op(&mut self, kind: OpKind, purse: PurseId) -> (handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(purse),
            old(self).next_handle < u64::MAX,
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            handle == old(self).next_handle,
            !old(self).operations().dom().contains(handle),
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle,
                kind,
                purse,
                status: OpStatus::Preparing,
            }),
            final(self).next_handle == old(self).next_handle + 1,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::OperationStarted {
                handle,
                kind,
                purse,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            // Other state untouched.
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).next_purse_id == old(self).next_purse_id,
            // lock_refint preservation: operations.dom strictly grows
            // (adds `handle`), and coins/entries are untouched. Every
            // existing edge in refint still points into the larger ops set.
            lock_refint(old(self).coins(), old(self).entries(), old(self).operations())
                ==> lock_refint(final(self).coins(), final(self).entries(),
                                final(self).operations()),
    {
        let ghost old_ops = self.spec_operations@;
        let ghost old_ops_vec = self.operations@;
        let ghost old_m = self.spec_purses@;
        let handle = self.next_handle;
        let new_op = OperationRec {
            handle,
            kind,
            purse,
            status: OpStatus::Preparing,
        };
        // Each existing operation's handle is strictly less than the new one
        // by old invariant (v).
        proof {
            assert forall|i: int| 0 <= i < old_ops_vec.len() implies
                #[trigger] old_ops_vec[i].handle < handle
            by {
                assert(old_ops.dom().contains(old_ops_vec[i].handle));
            }
        }
        self.operations.push(new_op);
        proof {
            self.spec_operations = Ghost(self.spec_operations@.insert(handle, new_op));
        }
        self.next_handle = handle + 1;

        proof {
            // Purses / coins / entries are entirely untouched.
            assert(self.purses@ == old(self).purses@);
            assert(self.spec_purses@ == old_m);
            assert(self.coins@ == old(self).coins@);
            assert(self.spec_coins@ == old(self).spec_coins@);
            assert(self.entries@ == old(self).entries@);
            assert(self.spec_entries@ == old(self).spec_entries@);
            assert(self.next_purse_id == old(self).next_purse_id);

            let new_ops = self.spec_operations@;
            let new_ops_vec = self.operations@;
            let last = old_ops_vec.len() as int;
            assert(new_ops_vec.len() == old_ops_vec.len() + 1);
            assert(new_ops_vec[last] == new_op);
            assert forall|i: int| 0 <= i < old_ops_vec.len() implies
                #[trigger] new_ops_vec[i] == old_ops_vec[i]
            by {}

            // (u) key consistency.
            assert forall|h: OpHandle| #[trigger] new_ops.dom().contains(h)
                implies new_ops[h].handle == h
            by {
                if h != handle { assert(old_ops.dom().contains(h)); }
            }
            // (v) handle below allocator.
            assert forall|h: OpHandle| #[trigger] new_ops.dom().contains(h)
                implies h < self.next_handle
            by {
                if h != handle { assert(old_ops.dom().contains(h)); }
            }
            // (w) refint.
            assert forall|h: OpHandle| #[trigger] new_ops.dom().contains(h)
                implies self.spec_purses@.dom().contains(new_ops[h].purse)
            by {
                if h == handle {
                    assert(new_ops[handle].purse == purse);
                } else {
                    assert(old_ops.dom().contains(h));
                }
            }
            // (x) Vec → ghost.
            assert forall|i: int| 0 <= i < new_ops_vec.len() implies
                new_ops.dom().contains((#[trigger] new_ops_vec[i]).handle)
                && new_ops[new_ops_vec[i].handle] == new_ops_vec[i]
            by {
                if i == last {
                    assert(new_ops_vec[i] == new_op);
                    assert(new_ops[handle] == new_op);
                } else {
                    assert(new_ops_vec[i] == old_ops_vec[i]);
                    assert(old_ops.dom().contains(old_ops_vec[i].handle));
                    assert(old_ops_vec[i].handle != handle);
                    assert(old_ops[old_ops_vec[i].handle] == old_ops_vec[i]);
                }
            }
            // (y) ghost → Vec.
            assert forall|h: OpHandle| #[trigger] new_ops.dom().contains(h)
                implies exists|i: int|
                    0 <= i < new_ops_vec.len()
                    && #[trigger] new_ops_vec[i].handle == h
            by {
                if h == handle {
                    let w = last;
                    assert(new_ops_vec[w].handle == handle);
                } else {
                    assert(old_ops.dom().contains(h));
                    let w = choose|i: int|
                        0 <= i < old_ops_vec.len()
                        && #[trigger] old_ops_vec[i].handle == h;
                    assert(new_ops_vec[w] == old_ops_vec[w]);
                }
            }
            // (z) no duplicates.
            assert forall|a: int, b: int|
                0 <= a < new_ops_vec.len() && 0 <= b < new_ops_vec.len()
                && (#[trigger] new_ops_vec[a]).handle
                    == (#[trigger] new_ops_vec[b]).handle
                implies a == b
            by {
                if a == last && b == last {
                } else if a == last {
                    assert(new_ops_vec[b] == old_ops_vec[b]);
                    assert(new_ops_vec[a].handle == handle);
                    assert(old_ops_vec[b].handle < handle);
                } else if b == last {
                    assert(new_ops_vec[a] == old_ops_vec[a]);
                    assert(new_ops_vec[b].handle == handle);
                    assert(old_ops_vec[a].handle < handle);
                } else {
                    assert(new_ops_vec[a] == old_ops_vec[a]);
                    assert(new_ops_vec[b] == old_ops_vec[b]);
                }
            }
        }
        self.emit_event(Event::OperationStarted { handle, kind, purse });
        handle
    }

    /// Transition the operation identified by `handle` to a new status.
    /// Mirror of `set_entry_on_chain` for operations. Used by named
    /// wrappers (`mark_op_submitted`, `mark_op_done`, `mark_op_failed`).
    pub fn set_op_status(&mut self, handle: OpHandle, new_status: OpStatus)
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: new_status,
            }),
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_coins = self.spec_coins@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_entries = self.spec_entries@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_operations = self.spec_operations@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_ops = self.spec_operations@;
        let ghost old_ops_vec = self.operations@;

        let mut j: usize = 0;
        while j < self.operations.len()
            invariant
                0 <= j <= self.operations.len(),
                self.invariant(),
                self.purses@ == old_purses_vec,
                self.spec_purses@ == old_spec_purses,
                self.next_purse_id == old(self).next_purse_id,
                self.spec_coins@ == old_coins,
                self.coins@ == old_coins_vec,
                self.spec_entries@ == old_entries,
                self.entries@ == old_entries_vec,
                self.spec_operations@ == old_operations,
                self.operations@ == old_operations_vec,
                self.spec_operations@ == old_ops,
                self.operations@ == old_ops_vec,
                self.next_handle == old(self).next_handle,
                self.next_age == old(self).next_age,
                self.fee_balance == old(self).fee_balance,
                self.next_extrinsic_id == old(self).next_extrinsic_id,
                self.events@ == old(self).events@,
                self.paid_ring_membership == old(self).paid_ring_membership,
                self.total_in == old(self).total_in,
                self.total_out == old(self).total_out,
                self.tokens@ == old(self).tokens@,
                self.chain_coins@ == old(self).chain_coins@,
                self.chain_entries@ == old(self).chain_entries@,
                old_purses_vec == old(self).purses@,
                old_spec_purses == old(self).spec_purses@,
                old_spec_purses == old(self).purses(),
                old_coins == old(self).spec_coins@,
                old_coins == old(self).coins(),
                old_coins_vec == old(self).coins@,
                old_entries == old(self).spec_entries@,
                old_entries == old(self).entries(),
                old_entries_vec == old(self).entries@,
                old_operations == old(self).spec_operations@,
                old_operations_vec == old(self).operations@,
                old_ops == old(self).spec_operations@,
                old_ops == old(self).operations(),
                old_ops.dom().contains(handle),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.operations@[jj]).handle != handle,
            decreases self.operations.len() - j,
        {
            if self.operations[j].handle == handle {
                let ghost target_idx = j as int;
                let ghost updated = OperationRec {
                    handle: old_ops[handle].handle,
                    kind: old_ops[handle].kind,
                    purse: old_ops[handle].purse,
                    status: new_status,
                };
                self.operations[j].status = new_status;

                proof {
                    assert(old_ops[handle].handle == handle);
                    self.spec_operations = Ghost(self.spec_operations@.insert(handle, updated));

                    let new_ops_vec = self.operations@;
                    let new_ops = self.spec_operations@;

                    assert(new_ops_vec[target_idx].handle == handle);
                    assert(new_ops_vec[target_idx].kind == old_ops_vec[target_idx].kind);
                    assert(new_ops_vec[target_idx].purse == old_ops_vec[target_idx].purse);
                    assert(new_ops_vec[target_idx].status == new_status);
                    assert forall|k: int|
                        0 <= k < new_ops_vec.len() && k != target_idx implies
                        #[trigger] new_ops_vec[k] == old_ops_vec[k]
                    by {}
                    assert(old_ops_vec[target_idx].handle == handle);
                    assert forall|kk: int|
                        0 <= kk < old_ops_vec.len() && kk != target_idx implies
                        (#[trigger] old_ops_vec[kk]).handle != handle
                    by {}

                    // (u) handle consistency.
                    assert forall|h: OpHandle| #[trigger] new_ops.dom().contains(h)
                        implies new_ops[h].handle == h
                    by { if h != handle { assert(old_ops.dom().contains(h)); } }
                    // (v) handle bound.
                    assert forall|h: OpHandle| #[trigger] new_ops.dom().contains(h)
                        implies h < self.next_handle
                    by { assert(old_ops.dom().contains(h)); }
                    // (w) refint.
                    assert forall|h: OpHandle| #[trigger] new_ops.dom().contains(h)
                        implies self.spec_purses@.dom().contains(new_ops[h].purse)
                    by {
                        if h != handle { assert(old_ops.dom().contains(h)); }
                    }
                    // (x) Vec → ghost.
                    assert forall|i: int| 0 <= i < new_ops_vec.len() implies
                        new_ops.dom().contains((#[trigger] new_ops_vec[i]).handle)
                        && new_ops[new_ops_vec[i].handle] == new_ops_vec[i]
                    by {
                        if i == target_idx {
                            assert(new_ops[handle] == updated);
                            assert(updated == new_ops_vec[target_idx]);
                        } else {
                            assert(new_ops_vec[i] == old_ops_vec[i]);
                            let oo = old_ops_vec[i];
                            assert(old_ops.dom().contains(oo.handle));
                            assert(oo.handle != handle);
                            assert(old_ops[oo.handle] == oo);
                        }
                    }
                    // (y) ghost → Vec.
                    assert forall|h: OpHandle| #[trigger] new_ops.dom().contains(h)
                        implies exists|i: int|
                            0 <= i < new_ops_vec.len()
                            && #[trigger] new_ops_vec[i].handle == h
                    by {
                        if h == handle {
                            let w = target_idx;
                            assert(new_ops_vec[w].handle == h);
                        } else {
                            assert(old_ops.dom().contains(h));
                            let w = choose|i: int|
                                0 <= i < old_ops_vec.len()
                                && #[trigger] old_ops_vec[i].handle == h;
                            assert(new_ops_vec[w] == old_ops_vec[w]);
                        }
                    }
                    // (z) no duplicates.
                    assert forall|a: int, b: int|
                        0 <= a < new_ops_vec.len() && 0 <= b < new_ops_vec.len()
                        && (#[trigger] new_ops_vec[a]).handle
                            == (#[trigger] new_ops_vec[b]).handle
                        implies a == b
                    by {
                        if a == target_idx && b == target_idx {
                        } else if a == target_idx {
                            assert(new_ops_vec[b] == old_ops_vec[b]);
                        } else if b == target_idx {
                            assert(new_ops_vec[a] == old_ops_vec[a]);
                        } else {
                            assert(new_ops_vec[a] == old_ops_vec[a]);
                            assert(new_ops_vec[b] == old_ops_vec[b]);
                        }
                    }

                    // Purses / coins / entries entirely unchanged.
                    assert(self.purses@ == old(self).purses@);
                    assert(self.spec_purses@ == old(self).spec_purses@);
                    assert(self.coins@ == old(self).coins@);
                    assert(self.spec_coins@ == old(self).spec_coins@);
                    assert(self.entries@ == old(self).entries@);
                    assert(self.spec_entries@ == old(self).spec_entries@);
                }
                return;
            }
            j += 1;
        }
        proof {
            assert(old_ops.dom().contains(handle));
            let w = choose|jj: int|
                0 <= jj < old_ops_vec.len()
                && #[trigger] old_ops_vec[jj].handle == handle;
        }
        vstd::pervasive::unreached()
    }



    /// Operation lifecycle: `Preparing` → `Submitted`. Phase order
    /// gate matching Quint `submitOp`.
    pub fn mark_op_submitted(&mut self, handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            old(self).operations()[handle].status == OpStatus::Preparing,
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::OperationProgress {
                handle,
                status: OpStatus::Submitted,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: OpStatus::Submitted,
            }),
    {
        self.set_op_status(handle, OpStatus::Submitted);
        self.emit_event(Event::OperationProgress {
            handle,
            status: OpStatus::Submitted,
        });
    }

    /// Operation lifecycle: `Submitted` → `InBlock`. Fires when the
    /// extrinsic lands in a block.
    pub fn mark_op_in_block(&mut self, handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            old(self).operations()[handle].status == OpStatus::Submitted,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: OpStatus::InBlock,
            }),
    {
        self.set_op_status(handle, OpStatus::InBlock);
    }

    /// Operation lifecycle: `InBlock` → `Finalized`.
    pub fn mark_op_finalized(&mut self, handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            old(self).operations()[handle].status == OpStatus::InBlock,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: OpStatus::Finalized,
            }),
    {
        self.set_op_status(handle, OpStatus::Finalized);
    }

    /// Operation lifecycle: `Finalized` → `Waiting(ready_at)`. Used by
    /// top-up: the op waits for a freshly-allocated entry to mature
    /// before it can be marked `Done`.
    pub fn mark_op_waiting(&mut self, handle: OpHandle, ready_at: u64)
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            old(self).operations()[handle].status == OpStatus::Finalized,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: OpStatus::Waiting(ready_at),
            }),
    {
        self.set_op_status(handle, OpStatus::Waiting(ready_at));
    }

    /// Operation lifecycle: `Finalized | Waiting(_)` → `Done`. Marks
    /// the operation as successfully completed.
    pub fn mark_op_done(&mut self, handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            match old(self).operations()[handle].status {
                OpStatus::Finalized => true,
                OpStatus::Waiting(_) => true,
                _ => false,
            },
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::OperationCompleted {
                handle,
                status: OpStatus::Done,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: OpStatus::Done,
            }),
    {
        self.set_op_status(handle, OpStatus::Done);
        self.emit_event(Event::OperationCompleted {
            handle,
            status: OpStatus::Done,
        });
    }

    /// Operation lifecycle: any cancellable status (`Preparing`,
    /// `Waiting(_)`) → `Failed`. Quint analog: `cancelOp`'s status
    /// transition. The caller is responsible for releasing locks via
    /// [`Self::release_locked_coin`] / [`Self::release_locked_entry`]
    /// before or after invoking this; the bulk-sweep is not bundled
    /// here because the cross-state refint invariant that would let
    /// us prove "no LockedFor(h) remains" is not yet in the model.
    pub fn set_op_failed(&mut self, handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            match old(self).operations()[handle].status {
                OpStatus::Preparing => true,
                OpStatus::Waiting(_) => true,
                _ => false,
            },
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::OperationCompleted {
                handle,
                status: OpStatus::Failed,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: OpStatus::Failed,
            }),
    {
        self.set_op_status(handle, OpStatus::Failed);
        self.emit_event(Event::OperationCompleted {
            handle,
            status: OpStatus::Failed,
        });
    }

    /// Find and release a single coin locked for `handle`. Returns the
    /// released key, or `None` if no coin is currently `LockedFor(handle)`.
    ///
    /// Building block for bulk sweeps: callers loop until `None` to
    /// drain all locks. Decomposes the bulk-sweep proof obligation
    /// into one-step ghost map updates, which Verus discharges
    /// directly via the underlying release_locked_coin contract.
    pub fn release_one_coin_lock_for(&mut self, handle: OpHandle)
        -> (res: Option<(PurseId, u64)>)
        requires
            old(self).invariant(),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            match res {
                Some(key) =>
                    old(self).coins().dom().contains(key)
                    && old(self).coins()[key].state == CoinState::LockedFor(handle)
                    && final(self).coins() ==
                        old(self).coins().insert(key, CoinRec {
                            purse: old(self).coins()[key].purse,
                            idx: old(self).coins()[key].idx,
                            exponent: old(self).coins()[key].exponent,
                            age: old(self).coins()[key].age,
                            account: old(self).coins()[key].account,
                            state: CoinState::Available,
                        }),
                None =>
                    final(self).coins() == old(self).coins()
                    && final(self).coins@ == old(self).coins@
                    && forall|k: (PurseId, u64)|
                        #[trigger] old(self).coins().dom().contains(k)
                        ==> old(self).coins()[k].state != CoinState::LockedFor(handle),
            },
    {
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                self == old(self),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).state != CoinState::LockedFor(handle),
            decreases self.coins.len() - j,
        {
            let needs_release = match self.coins[j].state {
                CoinState::LockedFor(h) => h == handle,
                _ => false,
            };
            if needs_release {
                let key = (self.coins[j].purse, self.coins[j].idx);
                proof {
                    assert(self.spec_coins@.dom().contains(key));
                    assert(self.coins()[key] == self.coins@[j as int]);
                    assert(self.coins()[key].state == CoinState::LockedFor(handle));
                }
                self.release_locked_coin(key, handle);
                return Some(key);
            }
            j = j + 1;
        }
        // No match: lift Vec-side bound to ghost map.
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] old(self).coins().dom().contains(k)
                implies old(self).coins()[k].state != CoinState::LockedFor(handle)
            by {
                let w = choose|jj: int|
                    0 <= jj < self.coins@.len()
                    && #[trigger] self.coins@[jj].purse == k.0
                    && self.coins@[jj].idx == k.1;
                assert(self.coins@[w].state == self.coins()[k].state);
            }
        }
        None
    }

    /// Find and release a single entry locally locked for `handle`.
    /// Returns the released key, or `None` if no entry is currently
    /// `LocalLockedFor(handle)`. Entry parallel of
    /// [`Self::release_one_coin_lock_for`].
    pub fn release_one_entry_lock_for(&mut self, handle: OpHandle)
        -> (res: Option<(PurseId, u64)>)
        requires
            old(self).invariant(),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            match res {
                Some(key) =>
                    old(self).entries().dom().contains(key)
                    && old(self).entries()[key].local
                        == EntryLocal::LocalLockedFor(handle)
                    && final(self).entries() ==
                        old(self).entries().insert(key, EntryRec {
                            purse: old(self).entries()[key].purse,
                            idx: old(self).entries()[key].idx,
                            exponent: old(self).entries()[key].exponent,
                            member_key: old(self).entries()[key].member_key,
                            allocated_at: old(self).entries()[key].allocated_at,
                            ready_at: old(self).entries()[key].ready_at,
                            ring_idx: old(self).entries()[key].ring_idx,
                            on_chain: old(self).entries()[key].on_chain,
                            local: EntryLocal::LocalAvailable,
                        }),
                None =>
                    final(self).entries() == old(self).entries()
                    && final(self).entries@ == old(self).entries@
                    && forall|k: (PurseId, u64)|
                        #[trigger] old(self).entries().dom().contains(k)
                        ==> old(self).entries()[k].local
                            != EntryLocal::LocalLockedFor(handle),
            },
    {
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.invariant(),
                self == old(self),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.entries@[jj]).local
                        != EntryLocal::LocalLockedFor(handle),
            decreases self.entries.len() - j,
        {
            let needs_release = match self.entries[j].local {
                EntryLocal::LocalLockedFor(h) => h == handle,
                _ => false,
            };
            if needs_release {
                let key = (self.entries[j].purse, self.entries[j].idx);
                proof {
                    assert(self.spec_entries@.dom().contains(key));
                    assert(self.entries()[key] == self.entries@[j as int]);
                    assert(self.entries()[key].local
                        == EntryLocal::LocalLockedFor(handle));
                }
                self.release_locked_entry(key, handle);
                return Some(key);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] old(self).entries().dom().contains(k)
                implies old(self).entries()[k].local
                    != EntryLocal::LocalLockedFor(handle)
            by {
                let w = choose|jj: int|
                    0 <= jj < self.entries@.len()
                    && #[trigger] self.entries@[jj].purse == k.0
                    && self.entries@[jj].idx == k.1;
                assert(self.entries@[w].local == self.entries()[k].local);
            }
        }
        None
    }

    /// Release a coin that's locked for `handle`, returning it to
    /// `Available`. Quint analog: the per-coin step of `cancelOp`'s
    /// `releasedCoins` fold.
    #[allow(unused_variables)]
    pub fn release_locked_coin(&mut self, key: (PurseId, u64), handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::LockedFor(handle),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins().insert(key, CoinRec {
                purse: old(self).coins()[key].purse,
                idx: old(self).coins()[key].idx,
                exponent: old(self).coins()[key].exponent,
                age: old(self).coins()[key].age,
                account: old(self).coins()[key].account,
                state: CoinState::Available,
            }),
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).coins@.len() == old(self).coins@.len(),
            lock_refint(old(self).coins(), old(self).entries(), old(self).operations())
                ==> lock_refint(final(self).coins(), final(self).entries(),
                                final(self).operations()),
    {
        self.transition_coin_state(key, CoinState::Available);
    }

    /// Release an entry that's locally locked for `handle`, returning
    /// it to `LocalAvailable`. Quint analog: per-entry step of
    /// `cancelOp`'s `releasedEntries` fold.
    #[allow(unused_variables)]
    pub fn release_locked_entry(&mut self, key: (PurseId, u64), handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).entries().dom().contains(key),
            old(self).entries()[key].local == EntryLocal::LocalLockedFor(handle),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries().insert(key, EntryRec {
                purse: old(self).entries()[key].purse,
                idx: old(self).entries()[key].idx,
                exponent: old(self).entries()[key].exponent,
                member_key: old(self).entries()[key].member_key,
                allocated_at: old(self).entries()[key].allocated_at,
                ready_at: old(self).entries()[key].ready_at,
                ring_idx: old(self).entries()[key].ring_idx,
                local: EntryLocal::LocalAvailable,
                on_chain: old(self).entries()[key].on_chain,
            }),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).entries@.len() == old(self).entries@.len(),
            lock_refint(old(self).coins(), old(self).entries(), old(self).operations())
                ==> lock_refint(final(self).coins(), final(self).entries(),
                                final(self).operations()),
    {
        self.set_entry_local(key, EntryLocal::LocalAvailable);
    }

    /// Coin lifecycle: `Pending` → `Available`. Called when chain
    /// observation confirms the coin exists on-chain.
    pub fn mark_coin_observed(&mut self, key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::Pending,
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins().insert(key, CoinRec {
                purse: old(self).coins()[key].purse,
                idx: old(self).coins()[key].idx,
                exponent: old(self).coins()[key].exponent,
                age: old(self).coins()[key].age,
                account: old(self).coins()[key].account,
                state: CoinState::Available,
            }),
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::CoinAvailable {
                purse: key.0,
                exponent: old(self).coins()[key].exponent,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let exp = self.read_coin_exponent(key);
        self.transition_coin_state(key, CoinState::Available);
        self.emit_event(Event::CoinAvailable {
            purse: key.0,
            exponent: exp,
        });
    }

    /// Coin lifecycle: `Available` → `PendingSpend`.
    pub fn mark_coin_pending_spend(&mut self, key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::Available,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins().insert(key, CoinRec {
                purse: old(self).coins()[key].purse,
                idx: old(self).coins()[key].idx,
                exponent: old(self).coins()[key].exponent,
                age: old(self).coins()[key].age,
                account: old(self).coins()[key].account,
                state: CoinState::PendingSpend,
            }),
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        self.transition_coin_state(key, CoinState::PendingSpend);
    }

    /// Coin lifecycle: `PendingSpend` → `Spent`.
    pub fn mark_coin_spent(&mut self, key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::PendingSpend,
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins().insert(key, CoinRec {
                purse: old(self).coins()[key].purse,
                idx: old(self).coins()[key].idx,
                exponent: old(self).coins()[key].exponent,
                age: old(self).coins()[key].age,
                account: old(self).coins()[key].account,
                state: CoinState::Spent,
            }),
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::CoinSpent {
                purse: key.0,
                exponent: old(self).coins()[key].exponent,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let exp = self.read_coin_exponent(key);
        self.transition_coin_state(key, CoinState::Spent);
        self.emit_event(Event::CoinSpent {
            purse: key.0,
            exponent: exp,
        });
    }

    /// Coin lifecycle: `PendingSpend` → `Available`. Called when an
    /// in-flight operation that had reserved this coin is cancelled
    /// before chain settlement; the reservation is reverted.
    pub fn reverse_pending_spend(&mut self, key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::PendingSpend,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins().insert(key, CoinRec {
                purse: old(self).coins()[key].purse,
                idx: old(self).coins()[key].idx,
                exponent: old(self).coins()[key].exponent,
                age: old(self).coins()[key].age,
                account: old(self).coins()[key].account,
                state: CoinState::Available,
            }),
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        self.transition_coin_state(key, CoinState::Available);
    }

    /// Coin lifecycle: `Available` → `LockedFor(handle)`. Reserves the
    /// coin for the operation identified by `handle`. Reversible via
    /// `unlock_coin`; commits to spending via `commit_locked_coin`.
    pub fn lock_coin(&mut self, key: (PurseId, u64), handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::Available,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins().insert(key, CoinRec {
                purse: old(self).coins()[key].purse,
                idx: old(self).coins()[key].idx,
                exponent: old(self).coins()[key].exponent,
                age: old(self).coins()[key].age,
                account: old(self).coins()[key].account,
                state: CoinState::LockedFor(handle),
            }),
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            // lock_refint preservation: if the old state satisfied
            // refint AND the handle is a known op, the new state still
            // satisfies refint (the only new LockedFor edge references h,
            // which is in operations.dom).
            (lock_refint(old(self).coins(), old(self).entries(), old(self).operations())
                && old(self).operations().dom().contains(handle))
                ==> lock_refint(final(self).coins(), final(self).entries(),
                                final(self).operations()),
    {
        self.transition_coin_state(key, CoinState::LockedFor(handle));
    }

    /// Coin lifecycle: `LockedFor(_)` → `Available`. Releases the
    /// reservation. Used when the operation that locked this coin
    /// cancels before submission.
    pub fn unlock_coin(&mut self, key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            exists|h: OpHandle| old(self).coins()[key].state == CoinState::LockedFor(h),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins().insert(key, CoinRec {
                purse: old(self).coins()[key].purse,
                idx: old(self).coins()[key].idx,
                exponent: old(self).coins()[key].exponent,
                age: old(self).coins()[key].age,
                account: old(self).coins()[key].account,
                state: CoinState::Available,
            }),
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).coins@.len() == old(self).coins@.len(),
            // lock_refint preservation: removing a LockedFor edge can
            // never break refint (no new dangling references).
            lock_refint(old(self).coins(), old(self).entries(), old(self).operations())
                ==> lock_refint(final(self).coins(), final(self).entries(),
                                final(self).operations()),
    {
        self.transition_coin_state(key, CoinState::Available);
    }

    /// Coin lifecycle: `LockedFor(_)` → `PendingSpend`. Commits a locked
    /// coin to its operation's spend pipeline (i.e., the operation has
    /// been submitted and is now in flight).
    pub fn commit_locked_coin(&mut self, key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            exists|h: OpHandle| old(self).coins()[key].state == CoinState::LockedFor(h),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins().insert(key, CoinRec {
                purse: old(self).coins()[key].purse,
                idx: old(self).coins()[key].idx,
                exponent: old(self).coins()[key].exponent,
                age: old(self).coins()[key].age,
                account: old(self).coins()[key].account,
                state: CoinState::PendingSpend,
            }),
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).coins@.len() == old(self).coins@.len(),
            // lock_refint preservation: removing a LockedFor edge.
            lock_refint(old(self).coins(), old(self).entries(), old(self).operations())
                ==> lock_refint(final(self).coins(), final(self).entries(),
                                final(self).operations()),
    {
        self.transition_coin_state(key, CoinState::PendingSpend);
    }

    /// Internal: locate the coin keyed `key` in the exec Vec and rewrite its
    /// `state` field to `new_state`; mirror to the ghost map. The state
    /// transition is unconstrained here — callers (`mark_coin_*`) enforce
    /// the valid Available → PendingSpend → Spent ordering.
    fn transition_coin_state(&mut self, key: (PurseId, u64), new_state: CoinState)
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins().insert(key, CoinRec {
                purse: old(self).coins()[key].purse,
                idx: old(self).coins()[key].idx,
                exponent: old(self).coins()[key].exponent,
                age: old(self).coins()[key].age,
                account: old(self).coins()[key].account,
                state: new_state,
            }),
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).coins@.len() == old(self).coins@.len(),
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_next_purse_id = self.next_purse_id;
        let ghost old_coins = self.spec_coins@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_entries = self.spec_entries@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_operations = self.spec_operations@;
        let ghost old_operations_vec = self.operations@;

        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                self.purses@ == old_purses_vec,
                self.spec_purses@ == old_spec_purses,
                self.next_purse_id == old_next_purse_id,
                self.next_handle == old(self).next_handle,
                self.next_age == old(self).next_age,
                self.fee_balance == old(self).fee_balance,
                self.next_extrinsic_id == old(self).next_extrinsic_id,
                self.events@ == old(self).events@,
                self.paid_ring_membership == old(self).paid_ring_membership,
                self.total_in == old(self).total_in,
                self.total_out == old(self).total_out,
                self.tokens@ == old(self).tokens@,
                self.chain_coins@ == old(self).chain_coins@,
                self.chain_entries@ == old(self).chain_entries@,
                self.spec_coins@ == old_coins,
                self.coins@ == old_coins_vec,
                self.spec_entries@ == old_entries,
                self.entries@ == old_entries_vec,
                self.spec_operations@ == old_operations,
                self.operations@ == old_operations_vec,
                old_spec_purses == old(self).spec_purses@,
                old_spec_purses == old(self).purses(),
                old_coins == old(self).spec_coins@,
                old_coins == old(self).coins(),
                old_coins_vec == old(self).coins@,
                old_entries == old(self).spec_entries@,
                old_entries == old(self).entries(),
                old_entries_vec == old(self).entries@,
                old_operations == old(self).spec_operations@,
                old_operations_vec == old(self).operations@,
                old_coins.dom().contains(key),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).purse != key.0
                    || self.coins@[jj].idx != key.1,
            decreases self.coins.len() - j,
        {
            if self.coins[j].purse == key.0 && self.coins[j].idx == key.1 {
                let ghost target_idx = j as int;
                let ghost updated = CoinRec {
                    purse: old_coins[key].purse,
                    idx: old_coins[key].idx,
                    exponent: old_coins[key].exponent,
                    state: new_state,
                    age: old_coins[key].age,
                    account: old_coins[key].account,
                };
                self.coins[j].state = new_state;

                proof {
                    assert(old_coins[key].purse == key.0);
                    assert(old_coins[key].idx == key.1);
                    self.spec_coins = Ghost(self.spec_coins@.insert(key, updated));

                    let new_coins_vec = self.coins@;
                    let new_coins = self.spec_coins@;

                    assert(self.purses@ == old_purses_vec);
                    assert(self.spec_purses@ == old_spec_purses);
                    assert(self.next_purse_id == old_next_purse_id);

                    // Vec post-mutation: only the entry at target_idx changed,
                    // and only its `state` field.
                    assert(new_coins_vec[target_idx].purse == key.0);
                    assert(new_coins_vec[target_idx].idx == key.1);
                    assert(new_coins_vec[target_idx].exponent
                        == old_coins_vec[target_idx].exponent);
                    assert(new_coins_vec[target_idx].state == new_state);
                    assert forall|k: int|
                        0 <= k < new_coins_vec.len() && k != target_idx implies
                        #[trigger] new_coins_vec[k] == old_coins_vec[k]
                    by {}

                    // The old entry at target_idx had purse/idx == key (branch
                    // guard); uniqueness gives that no other Vec entry matches.
                    assert(old_coins_vec[target_idx].purse == key.0);
                    assert(old_coins_vec[target_idx].idx == key.1);
                    assert forall|kk: int|
                        0 <= kk < old_coins_vec.len() && kk != target_idx implies
                        (#[trigger] old_coins_vec[kk]).purse != key.0
                        || old_coins_vec[kk].idx != key.1
                    by {}

                    // (i) coin key consistency.
                    assert forall|kk: (PurseId, u64)| #[trigger] new_coins.dom().contains(kk)
                        implies new_coins[kk].purse == kk.0 && new_coins[kk].idx == kk.1
                    by {
                        if kk != key {
                            assert(old_coins.dom().contains(kk));
                        }
                    }

                    // (j) coin referential integrity.
                    assert forall|kk: (PurseId, u64)| #[trigger] new_coins.dom().contains(kk)
                        implies self.spec_purses@.dom().contains(kk.0)
                    by {
                        assert(old_coins.dom().contains(kk));
                    }

                    // (k) coin idx below purse's allocator.
                    assert forall|kk: (PurseId, u64)| #[trigger] new_coins.dom().contains(kk)
                        implies kk.1 < self.spec_purses@[kk.0].next_coin_idx
                    by {
                        assert(old_coins.dom().contains(kk));
                    }

                    // (l) exec → ghost
                    assert forall|jj: int| 0 <= jj < new_coins_vec.len() implies
                        new_coins.dom().contains(
                            (#[trigger] new_coins_vec[jj].purse, new_coins_vec[jj].idx)
                        )
                        && new_coins[(new_coins_vec[jj].purse, new_coins_vec[jj].idx)]
                            == new_coins_vec[jj]
                    by {
                        if jj == target_idx {
                            assert(new_coins[key] == updated);
                            assert(updated == new_coins_vec[target_idx]);
                        } else {
                            assert(new_coins_vec[jj] == old_coins_vec[jj]);
                            let oc = old_coins_vec[jj];
                            assert(old_coins.dom().contains((oc.purse, oc.idx)));
                            assert((oc.purse, oc.idx) != key);
                            assert(old_coins[(oc.purse, oc.idx)] == oc);
                        }
                    }

                    // (m) ghost → exec
                    assert forall|kk: (PurseId, u64)| #[trigger] new_coins.dom().contains(kk)
                        implies exists|jj: int|
                            0 <= jj < new_coins_vec.len()
                            && #[trigger] new_coins_vec[jj].purse == kk.0
                            && new_coins_vec[jj].idx == kk.1
                    by {
                        if kk == key {
                            let w = target_idx;
                            assert(new_coins_vec[w].purse == kk.0);
                            assert(new_coins_vec[w].idx == kk.1);
                        } else {
                            assert(old_coins.dom().contains(kk));
                            let w = choose|jj: int|
                                0 <= jj < old_coins_vec.len()
                                && #[trigger] old_coins_vec[jj].purse == kk.0
                                && old_coins_vec[jj].idx == kk.1;
                            assert(new_coins_vec[w] == old_coins_vec[w]);
                        }
                    }

                    // (n) no duplicates — unchanged.
                    assert forall|a: int, b: int|
                        0 <= a < new_coins_vec.len() && 0 <= b < new_coins_vec.len()
                        && (#[trigger] new_coins_vec[a]).purse
                            == (#[trigger] new_coins_vec[b]).purse
                        && new_coins_vec[a].idx == new_coins_vec[b].idx
                        implies a == b
                    by {
                        if a == target_idx && b == target_idx {
                        } else if a == target_idx {
                            assert(new_coins_vec[b] == old_coins_vec[b]);
                        } else if b == target_idx {
                            assert(new_coins_vec[a] == old_coins_vec[a]);
                        } else {
                            assert(new_coins_vec[a] == old_coins_vec[a]);
                            assert(new_coins_vec[b] == old_coins_vec[b]);
                        }
                    }
                    // Vec length preservation: state field write doesn't
                    // change Vec length.
                    assert(self.coins@.len() == old_coins_vec.len());
                }
                return;
            }
            j += 1;
        }
        // Unreachable: precondition + invariant (m) guarantee a Vec witness.
        proof {
            assert(old_coins.dom().contains(key));
            let w = choose|jj: int|
                0 <= jj < old_coins_vec.len()
                && #[trigger] old_coins_vec[jj].purse == key.0
                && old_coins_vec[jj].idx == key.1;
        }
        vstd::pervasive::unreached()
    }

    /// Promote a recycler entry's on-chain state (e.g. Waiting → Ready
    /// when chain notifications confirm anonymity-floor membership).
    /// Quint analog: `chainPromoteToReady`, `chainPromoteToDegraded`.
    pub fn set_entry_on_chain(&mut self, key: (PurseId, u64), new_state: EntryOnChain)
        requires
            old(self).invariant(),
            old(self).entries().dom().contains(key),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries().insert(key, EntryRec {
                purse: old(self).entries()[key].purse,
                idx: old(self).entries()[key].idx,
                exponent: old(self).entries()[key].exponent,
                member_key: old(self).entries()[key].member_key,
                allocated_at: old(self).entries()[key].allocated_at,
                ready_at: old(self).entries()[key].ready_at,
                ring_idx: old(self).entries()[key].ring_idx,
                on_chain: new_state,
                local: old(self).entries()[key].local,
            }),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_next_purse_id = self.next_purse_id;
        let ghost old_coins = self.spec_coins@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_entries = self.spec_entries@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_operations = self.spec_operations@;
        let ghost old_operations_vec = self.operations@;

        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.invariant(),
                self.purses@ == old_purses_vec,
                self.spec_purses@ == old_spec_purses,
                self.next_purse_id == old_next_purse_id,
                self.next_handle == old(self).next_handle,
                self.next_age == old(self).next_age,
                self.fee_balance == old(self).fee_balance,
                self.next_extrinsic_id == old(self).next_extrinsic_id,
                self.events@ == old(self).events@,
                self.paid_ring_membership == old(self).paid_ring_membership,
                self.total_in == old(self).total_in,
                self.total_out == old(self).total_out,
                self.tokens@ == old(self).tokens@,
                self.chain_coins@ == old(self).chain_coins@,
                self.chain_entries@ == old(self).chain_entries@,
                self.spec_coins@ == old_coins,
                self.coins@ == old_coins_vec,
                self.spec_entries@ == old_entries,
                self.entries@ == old_entries_vec,
                self.spec_operations@ == old_operations,
                self.operations@ == old_operations_vec,
                old_spec_purses == old(self).spec_purses@,
                old_spec_purses == old(self).purses(),
                old_coins == old(self).spec_coins@,
                old_coins == old(self).coins(),
                old_coins_vec == old(self).coins@,
                old_entries == old(self).spec_entries@,
                old_entries == old(self).entries(),
                old_entries_vec == old(self).entries@,
                old_operations == old(self).spec_operations@,
                old_operations_vec == old(self).operations@,
                old_entries.dom().contains(key),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.entries@[jj]).purse != key.0
                    || self.entries@[jj].idx != key.1,
            decreases self.entries.len() - j,
        {
            if self.entries[j].purse == key.0 && self.entries[j].idx == key.1 {
                let ghost target_idx = j as int;
                let ghost updated = EntryRec {
                    purse: old_entries[key].purse,
                    idx: old_entries[key].idx,
                    exponent: old_entries[key].exponent,
                    on_chain: new_state,
                    local: old_entries[key].local,
                    member_key: old_entries[key].member_key,
                    allocated_at: old_entries[key].allocated_at,
                    ready_at: old_entries[key].ready_at,
                    ring_idx: old_entries[key].ring_idx,
                };
                self.entries[j].on_chain = new_state;

                proof {
                    assert(old_entries[key].purse == key.0);
                    assert(old_entries[key].idx == key.1);
                    self.spec_entries = Ghost(self.spec_entries@.insert(key, updated));

                    let new_entries_vec = self.entries@;
                    let new_entries = self.spec_entries@;

                    assert(self.purses@ == old_purses_vec);
                    assert(self.spec_purses@ == old_spec_purses);
                    assert(self.next_purse_id == old_next_purse_id);
                    assert(self.coins@ == old_coins_vec);
                    assert(self.spec_coins@ == old_coins);

                    assert(new_entries_vec[target_idx].purse == key.0);
                    assert(new_entries_vec[target_idx].idx == key.1);
                    assert(new_entries_vec[target_idx].exponent
                        == old_entries_vec[target_idx].exponent);
                    assert(new_entries_vec[target_idx].on_chain == new_state);
                    assert forall|k: int|
                        0 <= k < new_entries_vec.len() && k != target_idx implies
                        #[trigger] new_entries_vec[k] == old_entries_vec[k]
                    by {}
                    assert(old_entries_vec[target_idx].purse == key.0);
                    assert(old_entries_vec[target_idx].idx == key.1);
                    assert forall|kk: int|
                        0 <= kk < old_entries_vec.len() && kk != target_idx implies
                        (#[trigger] old_entries_vec[kk]).purse != key.0
                        || old_entries_vec[kk].idx != key.1
                    by {}

                    // (o) entry key consistency.
                    assert forall|kk: (PurseId, u64)| #[trigger] new_entries.dom().contains(kk)
                        implies new_entries[kk].purse == kk.0 && new_entries[kk].idx == kk.1
                    by { if kk != key { assert(old_entries.dom().contains(kk)); } }

                    // (p) entry referential integrity.
                    assert forall|kk: (PurseId, u64)| #[trigger] new_entries.dom().contains(kk)
                        implies self.spec_purses@.dom().contains(kk.0)
                    by { assert(old_entries.dom().contains(kk)); }

                    // (q) entry idx below allocator.
                    assert forall|kk: (PurseId, u64)| #[trigger] new_entries.dom().contains(kk)
                        implies kk.1 < self.spec_purses@[kk.0].next_entry_idx
                    by { assert(old_entries.dom().contains(kk)); }

                    // (r) Vec → ghost
                    assert forall|jj: int| 0 <= jj < new_entries_vec.len() implies
                        new_entries.dom().contains(
                            (#[trigger] new_entries_vec[jj].purse, new_entries_vec[jj].idx)
                        )
                        && new_entries[(new_entries_vec[jj].purse, new_entries_vec[jj].idx)]
                            == new_entries_vec[jj]
                    by {
                        if jj == target_idx {
                            assert(new_entries[key] == updated);
                            assert(updated == new_entries_vec[target_idx]);
                        } else {
                            assert(new_entries_vec[jj] == old_entries_vec[jj]);
                            let oe = old_entries_vec[jj];
                            assert(old_entries.dom().contains((oe.purse, oe.idx)));
                            assert((oe.purse, oe.idx) != key);
                            assert(old_entries[(oe.purse, oe.idx)] == oe);
                        }
                    }

                    // (s) ghost → Vec
                    assert forall|kk: (PurseId, u64)| #[trigger] new_entries.dom().contains(kk)
                        implies exists|jj: int|
                            0 <= jj < new_entries_vec.len()
                            && #[trigger] new_entries_vec[jj].purse == kk.0
                            && new_entries_vec[jj].idx == kk.1
                    by {
                        if kk == key {
                            let w = target_idx;
                            assert(new_entries_vec[w].purse == kk.0);
                            assert(new_entries_vec[w].idx == kk.1);
                        } else {
                            assert(old_entries.dom().contains(kk));
                            let w = choose|jj: int|
                                0 <= jj < old_entries_vec.len()
                                && #[trigger] old_entries_vec[jj].purse == kk.0
                                && old_entries_vec[jj].idx == kk.1;
                            assert(new_entries_vec[w] == old_entries_vec[w]);
                        }
                    }

                    // (t) no duplicates.
                    assert forall|a: int, b: int|
                        0 <= a < new_entries_vec.len() && 0 <= b < new_entries_vec.len()
                        && (#[trigger] new_entries_vec[a]).purse
                            == (#[trigger] new_entries_vec[b]).purse
                        && new_entries_vec[a].idx == new_entries_vec[b].idx
                        implies a == b
                    by {
                        if a == target_idx && b == target_idx {
                        } else if a == target_idx {
                            assert(new_entries_vec[b] == old_entries_vec[b]);
                        } else if b == target_idx {
                            assert(new_entries_vec[a] == old_entries_vec[a]);
                        } else {
                            assert(new_entries_vec[a] == old_entries_vec[a]);
                            assert(new_entries_vec[b] == old_entries_vec[b]);
                        }
                    }
                }
                return;
            }
            j += 1;
        }
        proof {
            assert(old_entries.dom().contains(key));
            let w = choose|jj: int|
                0 <= jj < old_entries_vec.len()
                && #[trigger] old_entries_vec[jj].purse == key.0
                && old_entries_vec[jj].idx == key.1;
        }
        vstd::pervasive::unreached()
    }

    /// Anonymity-floor confirmation: entry's on-chain state advances
    /// `Waiting → Ready` because the chain has confirmed sufficient
    /// ring-membership has accumulated. Quint analog:
    /// `chainPromoteToReady`.
    pub fn mark_entry_ready(&mut self, key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).entries().dom().contains(key),
            old(self).entries()[key].on_chain == EntryOnChain::Waiting,
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries().dom().contains(key),
            final(self).entries()[key].on_chain == EntryOnChain::Ready,
            final(self).entries()[key].local == old(self).entries()[key].local,
            final(self).entries()[key].exponent == old(self).entries()[key].exponent,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::EntryReadinessChanged {
                purse: key.0,
                exponent: old(self).entries()[key].exponent,
                new_state: EntryOnChain::Ready,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let exp = self.read_entry_exponent(key);
        self.set_entry_on_chain(key, EntryOnChain::Ready);
        self.emit_event(Event::EntryReadinessChanged {
            purse: key.0,
            exponent: exp,
            new_state: EntryOnChain::Ready,
        });
    }

    /// Anonymity-floor regression: entry's on-chain state degrades
    /// `Ready → Missing` because subsequent ring activity has dropped
    /// below the floor (or the entry has expired). Quint analog:
    /// `chainPromoteToDegraded`.
    pub fn mark_entry_missing(&mut self, key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).entries().dom().contains(key),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries().dom().contains(key),
            final(self).entries()[key].on_chain == EntryOnChain::Missing,
            final(self).entries()[key].local == old(self).entries()[key].local,
            final(self).entries()[key].exponent == old(self).entries()[key].exponent,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        self.set_entry_on_chain(key, EntryOnChain::Missing);
    }

    /// Entry local lifecycle: `LocalAvailable` → `LocalLockedFor`.
    /// Reserve an entry for an in-flight operation.
    pub fn lock_entry(&mut self, key: (PurseId, u64), handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).entries().dom().contains(key),
            old(self).entries()[key].local == EntryLocal::LocalAvailable,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries().insert(key, EntryRec {
                purse: old(self).entries()[key].purse,
                idx: old(self).entries()[key].idx,
                exponent: old(self).entries()[key].exponent,
                member_key: old(self).entries()[key].member_key,
                allocated_at: old(self).entries()[key].allocated_at,
                ready_at: old(self).entries()[key].ready_at,
                ring_idx: old(self).entries()[key].ring_idx,
                on_chain: old(self).entries()[key].on_chain,
                local: EntryLocal::LocalLockedFor(handle),
            }),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            // lock_refint preservation: same conditional shape as lock_coin.
            (lock_refint(old(self).coins(), old(self).entries(), old(self).operations())
                && old(self).operations().dom().contains(handle))
                ==> lock_refint(final(self).coins(), final(self).entries(),
                                final(self).operations()),
    {
        self.set_entry_local(key, EntryLocal::LocalLockedFor(handle));
    }

    /// Entry local lifecycle: `LocalLockedFor(_)` → `LocalConsumed`.
    /// Finalize an entry's consumption after settlement.
    pub fn consume_entry(&mut self, key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).entries().dom().contains(key),
            exists|h: OpHandle| old(self).entries()[key].local == EntryLocal::LocalLockedFor(h),
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries().insert(key, EntryRec {
                purse: old(self).entries()[key].purse,
                idx: old(self).entries()[key].idx,
                exponent: old(self).entries()[key].exponent,
                member_key: old(self).entries()[key].member_key,
                allocated_at: old(self).entries()[key].allocated_at,
                ready_at: old(self).entries()[key].ready_at,
                ring_idx: old(self).entries()[key].ring_idx,
                on_chain: old(self).entries()[key].on_chain,
                local: EntryLocal::LocalConsumed,
            }),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::EntryConsumed {
                purse: key.0,
                exponent: old(self).entries()[key].exponent,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).entries@.len() == old(self).entries@.len(),
            lock_refint(old(self).coins(), old(self).entries(), old(self).operations())
                ==> lock_refint(final(self).coins(), final(self).entries(),
                                final(self).operations()),
    {
        let exp = self.read_entry_exponent(key);
        self.set_entry_local(key, EntryLocal::LocalConsumed);
        self.emit_event(Event::EntryConsumed {
            purse: key.0,
            exponent: exp,
        });
    }

    /// Entry local lifecycle: `LocalLockedFor(_)` → `LocalAvailable`.
    /// Release the entry's reservation when the in-flight operation cancels.
    pub fn release_entry_lock(&mut self, key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).entries().dom().contains(key),
            exists|h: OpHandle| old(self).entries()[key].local == EntryLocal::LocalLockedFor(h),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries().insert(key, EntryRec {
                purse: old(self).entries()[key].purse,
                idx: old(self).entries()[key].idx,
                exponent: old(self).entries()[key].exponent,
                member_key: old(self).entries()[key].member_key,
                allocated_at: old(self).entries()[key].allocated_at,
                ready_at: old(self).entries()[key].ready_at,
                ring_idx: old(self).entries()[key].ring_idx,
                on_chain: old(self).entries()[key].on_chain,
                local: EntryLocal::LocalAvailable,
            }),
    {
        self.set_entry_local(key, EntryLocal::LocalAvailable);
    }

    /// Set a recycler entry's local-side state (Available → LockedFor →
    /// Consumed lifecycle). Mirror of `set_entry_on_chain` for the
    /// `local` field. Quint analog: `lockEntry`, `consumeEntry`.
    pub fn set_entry_local(&mut self, key: (PurseId, u64), new_state: EntryLocal)
        requires
            old(self).invariant(),
            old(self).entries().dom().contains(key),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries().insert(key, EntryRec {
                purse: old(self).entries()[key].purse,
                idx: old(self).entries()[key].idx,
                exponent: old(self).entries()[key].exponent,
                member_key: old(self).entries()[key].member_key,
                allocated_at: old(self).entries()[key].allocated_at,
                ready_at: old(self).entries()[key].ready_at,
                ring_idx: old(self).entries()[key].ring_idx,
                on_chain: old(self).entries()[key].on_chain,
                local: new_state,
            }),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).entries@.len() == old(self).entries@.len(),
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_next_purse_id = self.next_purse_id;
        let ghost old_coins = self.spec_coins@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_entries = self.spec_entries@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_operations = self.spec_operations@;
        let ghost old_operations_vec = self.operations@;

        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.invariant(),
                self.purses@ == old_purses_vec,
                self.spec_purses@ == old_spec_purses,
                self.next_purse_id == old_next_purse_id,
                self.next_handle == old(self).next_handle,
                self.next_age == old(self).next_age,
                self.fee_balance == old(self).fee_balance,
                self.next_extrinsic_id == old(self).next_extrinsic_id,
                self.events@ == old(self).events@,
                self.paid_ring_membership == old(self).paid_ring_membership,
                self.total_in == old(self).total_in,
                self.total_out == old(self).total_out,
                self.tokens@ == old(self).tokens@,
                self.chain_coins@ == old(self).chain_coins@,
                self.chain_entries@ == old(self).chain_entries@,
                self.spec_coins@ == old_coins,
                self.coins@ == old_coins_vec,
                self.spec_entries@ == old_entries,
                self.entries@ == old_entries_vec,
                self.spec_operations@ == old_operations,
                self.operations@ == old_operations_vec,
                old_spec_purses == old(self).spec_purses@,
                old_spec_purses == old(self).purses(),
                old_coins == old(self).spec_coins@,
                old_coins == old(self).coins(),
                old_coins_vec == old(self).coins@,
                old_entries == old(self).spec_entries@,
                old_entries == old(self).entries(),
                old_entries_vec == old(self).entries@,
                old_operations == old(self).spec_operations@,
                old_operations_vec == old(self).operations@,
                old_entries.dom().contains(key),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.entries@[jj]).purse != key.0
                    || self.entries@[jj].idx != key.1,
            decreases self.entries.len() - j,
        {
            if self.entries[j].purse == key.0 && self.entries[j].idx == key.1 {
                let ghost target_idx = j as int;
                let ghost updated = EntryRec {
                    purse: old_entries[key].purse,
                    idx: old_entries[key].idx,
                    exponent: old_entries[key].exponent,
                    on_chain: old_entries[key].on_chain,
                    local: new_state,
                    member_key: old_entries[key].member_key,
                    allocated_at: old_entries[key].allocated_at,
                    ready_at: old_entries[key].ready_at,
                    ring_idx: old_entries[key].ring_idx,
                };
                self.entries[j].local = new_state;

                proof {
                    assert(old_entries[key].purse == key.0);
                    assert(old_entries[key].idx == key.1);
                    self.spec_entries = Ghost(self.spec_entries@.insert(key, updated));

                    let new_entries_vec = self.entries@;
                    let new_entries = self.spec_entries@;

                    assert(self.purses@ == old_purses_vec);
                    assert(self.spec_purses@ == old_spec_purses);
                    assert(self.next_purse_id == old_next_purse_id);
                    assert(self.coins@ == old_coins_vec);
                    assert(self.spec_coins@ == old_coins);

                    assert(new_entries_vec[target_idx].purse == key.0);
                    assert(new_entries_vec[target_idx].idx == key.1);
                    assert(new_entries_vec[target_idx].exponent
                        == old_entries_vec[target_idx].exponent);
                    assert(new_entries_vec[target_idx].local == new_state);
                    assert forall|k: int|
                        0 <= k < new_entries_vec.len() && k != target_idx implies
                        #[trigger] new_entries_vec[k] == old_entries_vec[k]
                    by {}
                    assert(old_entries_vec[target_idx].purse == key.0);
                    assert(old_entries_vec[target_idx].idx == key.1);
                    assert forall|kk: int|
                        0 <= kk < old_entries_vec.len() && kk != target_idx implies
                        (#[trigger] old_entries_vec[kk]).purse != key.0
                        || old_entries_vec[kk].idx != key.1
                    by {}

                    // (o)
                    assert forall|kk: (PurseId, u64)| #[trigger] new_entries.dom().contains(kk)
                        implies new_entries[kk].purse == kk.0 && new_entries[kk].idx == kk.1
                    by { if kk != key { assert(old_entries.dom().contains(kk)); } }
                    // (p)
                    assert forall|kk: (PurseId, u64)| #[trigger] new_entries.dom().contains(kk)
                        implies self.spec_purses@.dom().contains(kk.0)
                    by { assert(old_entries.dom().contains(kk)); }
                    // (q)
                    assert forall|kk: (PurseId, u64)| #[trigger] new_entries.dom().contains(kk)
                        implies kk.1 < self.spec_purses@[kk.0].next_entry_idx
                    by { assert(old_entries.dom().contains(kk)); }
                    // (r)
                    assert forall|jj: int| 0 <= jj < new_entries_vec.len() implies
                        new_entries.dom().contains(
                            (#[trigger] new_entries_vec[jj].purse, new_entries_vec[jj].idx)
                        )
                        && new_entries[(new_entries_vec[jj].purse, new_entries_vec[jj].idx)]
                            == new_entries_vec[jj]
                    by {
                        if jj == target_idx {
                            assert(new_entries[key] == updated);
                            assert(updated == new_entries_vec[target_idx]);
                        } else {
                            assert(new_entries_vec[jj] == old_entries_vec[jj]);
                            let oe = old_entries_vec[jj];
                            assert(old_entries.dom().contains((oe.purse, oe.idx)));
                            assert((oe.purse, oe.idx) != key);
                            assert(old_entries[(oe.purse, oe.idx)] == oe);
                        }
                    }
                    // (s)
                    assert forall|kk: (PurseId, u64)| #[trigger] new_entries.dom().contains(kk)
                        implies exists|jj: int|
                            0 <= jj < new_entries_vec.len()
                            && #[trigger] new_entries_vec[jj].purse == kk.0
                            && new_entries_vec[jj].idx == kk.1
                    by {
                        if kk == key {
                            let w = target_idx;
                            assert(new_entries_vec[w].purse == kk.0);
                            assert(new_entries_vec[w].idx == kk.1);
                        } else {
                            assert(old_entries.dom().contains(kk));
                            let w = choose|jj: int|
                                0 <= jj < old_entries_vec.len()
                                && #[trigger] old_entries_vec[jj].purse == kk.0
                                && old_entries_vec[jj].idx == kk.1;
                            assert(new_entries_vec[w] == old_entries_vec[w]);
                        }
                    }
                    // (t)
                    assert forall|a: int, b: int|
                        0 <= a < new_entries_vec.len() && 0 <= b < new_entries_vec.len()
                        && (#[trigger] new_entries_vec[a]).purse
                            == (#[trigger] new_entries_vec[b]).purse
                        && new_entries_vec[a].idx == new_entries_vec[b].idx
                        implies a == b
                    by {
                        if a == target_idx && b == target_idx {
                        } else if a == target_idx {
                            assert(new_entries_vec[b] == old_entries_vec[b]);
                        } else if b == target_idx {
                            assert(new_entries_vec[a] == old_entries_vec[a]);
                        } else {
                            assert(new_entries_vec[a] == old_entries_vec[a]);
                            assert(new_entries_vec[b] == old_entries_vec[b]);
                        }
                    }
                }
                return;
            }
            j += 1;
        }
        proof {
            assert(old_entries.dom().contains(key));
            let w = choose|jj: int|
                0 <= jj < old_entries_vec.len()
                && #[trigger] old_entries_vec[jj].purse == key.0
                && old_entries_vec[jj].idx == key.1;
        }
        vstd::pervasive::unreached()
    }

    /// Internal: scan the coin Vec for the first entry with `purse == p`.
    /// Returns its index, or `None` if no such coin remains.
    fn find_coin_with_purse(&self, p: PurseId) -> (res: Option<usize>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(i) =>
                    (i as int) < self.coins@.len()
                    && self.coins@[i as int].purse == p,
                None =>
                    forall|j: int| 0 <= j < self.coins@.len()
                        ==> (#[trigger] self.coins@[j]).purse != p,
            },
    {
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).purse != p,
            decreases self.coins.len() - j,
        {
            if self.coins[j].purse == p {
                return Some(j);
            }
            j += 1;
        }
        None
    }

    /// Internal: remove the coin at exec-Vec index `idx`. Vec shrinks by 1
    /// (via `swap_remove`); the ghost map drops exactly the key that
    /// belonged to the removed entry.
    fn remove_coin_at(&mut self, idx: usize)
        requires
            old(self).invariant(),
            (idx as int) < old(self).coins@.len(),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).next_purse_id == old(self).next_purse_id,
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            ({
                let removed = old(self).coins@[idx as int];
                final(self).coins()
                    == old(self).coins().remove((removed.purse, removed.idx))
            }),
            final(self).coins@.len() == old(self).coins@.len() - 1,
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_next_purse_id = self.next_purse_id;
        let ghost old_coins = self.spec_coins@;
        let ghost old_coins_vec = self.coins@;
        let ghost target_idx = idx as int;
        let ghost removed_entry = old_coins_vec[target_idx];
        let ghost removed_key = (removed_entry.purse, removed_entry.idx);
        let ghost last_idx = old_coins_vec.len() - 1;

        let _ = self.coins.swap_remove(idx);
        proof {
            self.spec_coins = Ghost(self.spec_coins@.remove(removed_key));

            let new_coins_vec = self.coins@;
            let new_coins = self.spec_coins@;

            assert(self.purses@ == old_purses_vec);
            assert(self.spec_purses@ == old_spec_purses);
            assert(self.next_purse_id == old_next_purse_id);

            // Vec post-state, from swap_remove spec:
            //   new_coins_vec == old_coins_vec.update(target_idx, last).drop_last()
            assert(new_coins_vec.len() == old_coins_vec.len() - 1);
            assert forall|k: int| 0 <= k < new_coins_vec.len() && k != target_idx implies
                #[trigger] new_coins_vec[k] == old_coins_vec[k]
            by {}
            assert(target_idx < new_coins_vec.len() ==>
                new_coins_vec[target_idx] == old_coins_vec[last_idx]);

            // Old key at target_idx == removed_key; by (n) old, no other Vec
            // entry had the same (purse, idx).
            assert(old_coins_vec[target_idx].purse == removed_key.0);
            assert(old_coins_vec[target_idx].idx == removed_key.1);
            assert forall|k: int| 0 <= k < old_coins_vec.len() && k != target_idx implies
                (#[trigger] old_coins_vec[k]).purse != removed_key.0
                || old_coins_vec[k].idx != removed_key.1
            by {}

            // removed_key was in old ghost dom (by old (l)); remove decreases dom by exactly {removed_key}.
            assert(old_coins.dom().contains(removed_key));
            assert(new_coins.dom() =~= old_coins.dom().remove(removed_key));

            // (i) coin key consistency.
            assert forall|k: (PurseId, u64)| #[trigger] new_coins.dom().contains(k)
                implies new_coins[k].purse == k.0 && new_coins[k].idx == k.1
            by {
                assert(old_coins.dom().contains(k));
            }

            // (j) coin referential integrity.
            assert forall|k: (PurseId, u64)| #[trigger] new_coins.dom().contains(k)
                implies self.spec_purses@.dom().contains(k.0)
            by {
                assert(old_coins.dom().contains(k));
            }

            // (k) coin idx below allocator.
            assert forall|k: (PurseId, u64)| #[trigger] new_coins.dom().contains(k)
                implies k.1 < self.spec_purses@[k.0].next_coin_idx
            by {
                assert(old_coins.dom().contains(k));
            }

            // (l) every new Vec entry's (purse, idx) is in new ghost.
            assert forall|jj: int| 0 <= jj < new_coins_vec.len() implies
                new_coins.dom().contains(
                    (#[trigger] new_coins_vec[jj].purse, new_coins_vec[jj].idx)
                )
                && new_coins[(new_coins_vec[jj].purse, new_coins_vec[jj].idx)]
                    == new_coins_vec[jj]
            by {
                if jj == target_idx {
                    assert(new_coins_vec[jj] == old_coins_vec[last_idx]);
                    assert(last_idx != target_idx);
                    assert(old_coins_vec[last_idx].purse != removed_key.0
                        || old_coins_vec[last_idx].idx != removed_key.1);
                    let oc = old_coins_vec[last_idx];
                    assert(old_coins.dom().contains((oc.purse, oc.idx)));
                    assert((oc.purse, oc.idx) != removed_key);
                    assert(old_coins[(oc.purse, oc.idx)] == oc);
                } else {
                    assert(new_coins_vec[jj] == old_coins_vec[jj]);
                    let oc = old_coins_vec[jj];
                    assert(old_coins.dom().contains((oc.purse, oc.idx)));
                    assert((oc.purse, oc.idx) != removed_key);
                    assert(old_coins[(oc.purse, oc.idx)] == oc);
                }
            }

            // (m) every new ghost key has a Vec witness.
            assert forall|k: (PurseId, u64)| #[trigger] new_coins.dom().contains(k)
                implies exists|jj: int|
                    0 <= jj < new_coins_vec.len()
                    && #[trigger] new_coins_vec[jj].purse == k.0
                    && new_coins_vec[jj].idx == k.1
            by {
                assert(old_coins.dom().contains(k));
                assert(k != removed_key);
                let w_old = choose|jj: int|
                    0 <= jj < old_coins_vec.len()
                    && #[trigger] old_coins_vec[jj].purse == k.0
                    && old_coins_vec[jj].idx == k.1;
                assert(w_old != target_idx);
                if w_old == last_idx {
                    // Element moved to target_idx by swap_remove.
                    assert(target_idx < new_coins_vec.len());
                    assert(new_coins_vec[target_idx] == old_coins_vec[last_idx]);
                } else {
                    assert(w_old < last_idx);
                    assert(w_old < new_coins_vec.len());
                    assert(new_coins_vec[w_old] == old_coins_vec[w_old]);
                }
            }

            // (n) no duplicates in new_coins_vec.
            assert forall|a: int, b: int|
                0 <= a < new_coins_vec.len() && 0 <= b < new_coins_vec.len()
                && (#[trigger] new_coins_vec[a]).purse
                    == (#[trigger] new_coins_vec[b]).purse
                && new_coins_vec[a].idx == new_coins_vec[b].idx
                implies a == b
            by {
                if a == target_idx && b == target_idx {
                } else if a == target_idx {
                    assert(new_coins_vec[a] == old_coins_vec[last_idx]);
                    assert(new_coins_vec[b] == old_coins_vec[b]);
                    assert(b != last_idx);
                } else if b == target_idx {
                    assert(new_coins_vec[b] == old_coins_vec[last_idx]);
                    assert(new_coins_vec[a] == old_coins_vec[a]);
                    assert(a != last_idx);
                } else {
                    assert(new_coins_vec[a] == old_coins_vec[a]);
                    assert(new_coins_vec[b] == old_coins_vec[b]);
                }
            }
        }
    }

    /// Internal: read the `exponent` of a recycler entry known to exist by `key`.
    fn read_entry_exponent(&self, key: (PurseId, u64)) -> (exp: u8)
        requires
            self.invariant(),
            self.entries().dom().contains(key),
        ensures
            exp == self.entries()[key].exponent,
    {
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.invariant(),
                self.entries().dom().contains(key),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.entries@[jj]).purse != key.0
                    || self.entries@[jj].idx != key.1,
            decreases self.entries.len() - j,
        {
            if self.entries[j].purse == key.0 && self.entries[j].idx == key.1 {
                proof {
                    assert(self.spec_entries@[(self.entries@[j as int].purse, self.entries@[j as int].idx)]
                        == self.entries@[j as int]);
                }
                return self.entries[j].exponent;
            }
            j = j + 1;
        }
        proof {
            let w = choose|jj: int|
                0 <= jj < self.entries@.len()
                && #[trigger] self.entries@[jj].purse == key.0
                && self.entries@[jj].idx == key.1;
        }
        vstd::pervasive::unreached()
    }

    /// Count of coins currently `LockedFor(handle)` across the whole
    /// state. Useful for diagnostics ("how much is reserved by this
    /// in-flight op?") and for callers driving bulk-sweep loops
    /// host-side.
    pub fn coin_count_for_handle(&self, handle: OpHandle) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count as nat == count_coin_locks_in_vec(self.coins@, handle, self.coins@.len() as nat),
            count <= self.coins@.len(),
    {
        let mut c: usize = 0;
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                c <= j,
                self.invariant(),
                c as nat == count_coin_locks_in_vec(self.coins@, handle, j as nat),
            decreases self.coins.len() - j,
        {
            let is_locked_for = match self.coins[j].state {
                CoinState::LockedFor(h) => h == handle,
                _ => false,
            };
            if is_locked_for {
                c = c + 1;
            }
            j = j + 1;
        }
        c
    }

    /// Count of entries currently `LocalLockedFor(handle)` across the
    /// whole state. Mirror of `coin_count_for_handle` for the entry
    /// side.
    pub fn entry_count_for_handle(&self, handle: OpHandle) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count as nat == count_entry_locks_in_vec(self.entries@, handle, self.entries@.len() as nat),
            count <= self.entries@.len(),
    {
        let mut c: usize = 0;
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                c <= j,
                self.invariant(),
                c as nat == count_entry_locks_in_vec(self.entries@, handle, j as nat),
            decreases self.entries.len() - j,
        {
            let is_locked_for = match self.entries[j].local {
                EntryLocal::LocalLockedFor(h) => h == handle,
                _ => false,
            };
            if is_locked_for {
                c = c + 1;
            }
            j = j + 1;
        }
        c
    }

    /// Exec witness for the [`Self::has_live_coin_in`] spec predicate:
    /// `true` iff at least one coin in purse `p` is in any non-`Spent`
    /// state. Pair with [`Self::has_in_flight_op_for_purse`] before
    /// `delete_purse` to surface "purse not empty" as an early bail
    /// instead of a precondition trap.
    pub fn check_has_live_coin_in(&self, p: PurseId) -> (res: bool)
        requires
            self.invariant(),
        ensures
            res == self.has_live_coin_in(p),
    {
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).purse != p
                    || self.coins@[jj].state == CoinState::Spent,
            decreases self.coins.len() - j,
        {
            let c = &self.coins[j];
            let is_spent = matches!(c.state, CoinState::Spent);
            if c.purse == p && !is_spent {
                #[allow(unused_variables)]
                let key = (c.purse, c.idx);
                proof {
                    assert(self.spec_coins@.dom().contains(key));
                    assert(self.coins()[key].state == self.coins@[j as int].state);
                }
                return true;
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                && k.0 == p
                implies self.coins()[k].state == CoinState::Spent
            by {
                let w = choose|jj: int|
                    0 <= jj < self.coins@.len()
                    && #[trigger] self.coins@[jj].purse == k.0
                    && self.coins@[jj].idx == k.1;
                assert(self.coins@[w].purse == p);
                assert(self.coins@[w].state == self.coins()[k].state);
            }
        }
        false
    }

    /// Autonomous maintenance trigger: scan purses, return the first
    /// one whose `Available` coin count strictly exceeds `threshold`.
    /// Returns `None` if no purse is over-fragmented. Quint analog:
    /// maintenance scheduler that decides which purse to consolidate next.
    pub fn find_purse_needing_maintenance(&self, threshold: usize)
        -> (res: Option<PurseId>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(p) => self.purses().dom().contains(p),
                None => true,
            },
    {
        let mut i: usize = 0;
        while i < self.purses.len()
            invariant
                0 <= i <= self.purses.len(),
                self.invariant(),
            decreases self.purses.len() - i,
        {
            let pid = self.purses[i].id;
            let count = self.coin_count_available(pid);
            if count > threshold {
                proof {
                    assert(self.spec_purses@.dom().contains(pid));
                }
                return Some(pid);
            }
            i = i + 1;
        }
        None
    }

    /// Count of operations currently in-flight (non-terminal status).
    pub fn op_count_in_flight(&self) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count <= self.operations@.len(),
    {
        let mut c: usize = 0;
        let mut j: usize = 0;
        while j < self.operations.len()
            invariant
                0 <= j <= self.operations.len(),
                c <= j,
                self.invariant(),
            decreases self.operations.len() - j,
        {
            let op = &self.operations[j];
            let is_terminal = match op.status {
                OpStatus::Done => true,
                OpStatus::Failed => true,
                _ => false,
            };
            if !is_terminal {
                c = c + 1;
            }
            j = j + 1;
        }
        c
    }

    /// Count of all coins (any state) in purse `p`. Useful diagnostic
    /// for "how cluttered is this purse?". Distinguish from
    /// coin_count_available which excludes locked/spent/pending.
    pub fn coin_count_in_purse(&self, p: PurseId) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count <= self.coins@.len(),
    {
        let mut c: usize = 0;
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                c <= j,
                self.invariant(),
            decreases self.coins.len() - j,
        {
            if self.coins[j].purse == p {
                c = c + 1;
            }
            j = j + 1;
        }
        c
    }

    /// Count of all entries (any state) in purse `p`. Entry parallel
    /// of `coin_count_in_purse`.
    pub fn entry_count_in_purse(&self, p: PurseId) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count <= self.entries@.len(),
    {
        let mut c: usize = 0;
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                c <= j,
                self.invariant(),
            decreases self.entries.len() - j,
        {
            if self.entries[j].purse == p {
                c = c + 1;
            }
            j = j + 1;
        }
        c
    }

    /// Count of `Available` coins in purse `p`. Used by maintenance
    /// triggers — e.g. "if coin_count_available(p) > threshold, run
    /// rebalance to consolidate into fewer larger coins".
    pub fn coin_count_available(&self, p: PurseId) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count <= self.coins@.len(),
    {
        let mut c: usize = 0;
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                c <= j,
                self.invariant(),
            decreases self.coins.len() - j,
        {
            let is_avail = matches!(self.coins[j].state, CoinState::Available);
            if self.coins[j].purse == p && is_avail {
                c = c + 1;
            }
            j = j + 1;
        }
        c
    }

    /// Count of selectable entries (Ready + LocalAvailable) in purse
    /// `p`. Used by maintenance triggers and §6.3 selection feasibility
    /// checks.
    pub fn entry_count_selectable(&self, p: PurseId) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count <= self.entries@.len(),
    {
        let mut c: usize = 0;
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                c <= j,
                self.invariant(),
            decreases self.entries.len() - j,
        {
            let e = &self.entries[j];
            let is_ready = matches!(e.on_chain, EntryOnChain::Ready);
            let is_local_avail = matches!(e.local, EntryLocal::LocalAvailable);
            if e.purse == p && is_ready && is_local_avail {
                c = c + 1;
            }
            j = j + 1;
        }
        c
    }

    /// Read the **real** entry value for `key` (Quint `coinValue` over
    /// the entry's exponent). Entry parallel of
    /// [`Self::read_coin_value_real`].
    pub fn read_entry_value_real(&self, key: (PurseId, u64)) -> (res: Option<u64>)
        requires
            self.invariant(),
            forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                ==> self.entries()[k].exponent <= MAX_EXPONENT,
        ensures
            match res {
                Some(v) =>
                    self.entries().dom().contains(key)
                    && v as nat == coin_value_pow2(self.entries()[key].exponent),
                None => !self.entries().dom().contains(key),
            },
    {
        match self.entry_record(key) {
            Some(e) => {
                proof {
                    assert(self.entries()[key].exponent <= MAX_EXPONENT);
                    assert(e.exponent == self.entries()[key].exponent);
                }
                Some(pow2_u64_exec(e.exponent))
            }
            None => None,
        }
    }

    /// Read the **real** coin value for `key` using `2^exp` arithmetic
    /// (Quint `coinValue`). Requires the coin's exponent to satisfy the
    /// `MAX_EXPONENT` bound. Returns `None` if no such coin exists.
    ///
    /// Companion to the pilot-scheme aggregations (which use
    /// `coin_value(exp) = exp + 1`) — this one reflects the production
    /// scheme. Callers wiring up the real arithmetic switch can compose
    /// this with their own sums; the existing per-purse aggregations
    /// (sum_available_in etc.) still use the pilot scheme.
    pub fn read_coin_value_real(&self, key: (PurseId, u64)) -> (res: Option<u64>)
        requires
            self.invariant(),
            forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                ==> self.coins()[k].exponent <= MAX_EXPONENT,
        ensures
            match res {
                Some(v) =>
                    self.coins().dom().contains(key)
                    && v as nat == coin_value_pow2(self.coins()[key].exponent),
                None => !self.coins().dom().contains(key),
            },
    {
        match self.coin_record(key) {
            Some(c) => {
                proof {
                    assert(self.coins()[key].exponent <= MAX_EXPONENT);
                    assert(c.exponent == self.coins()[key].exponent);
                }
                Some(pow2_u64_exec(c.exponent))
            }
            None => None,
        }
    }

    /// Synchronous read: state of the coin keyed `key`, or `None` if
    /// no such coin exists. Quint analog: `coins.get(key).state`.
    pub fn coin_state(&self, key: (PurseId, u64)) -> (res: Option<CoinState>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(s) =>
                    self.coins().dom().contains(key)
                    && s == self.coins()[key].state,
                None => !self.coins().dom().contains(key),
            },
    {
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).purse != key.0
                    || self.coins@[jj].idx != key.1,
            decreases self.coins.len() - j,
        {
            if self.coins[j].purse == key.0 && self.coins[j].idx == key.1 {
                proof {
                    assert(self.spec_coins@.dom().contains(key));
                }
                return Some(self.coins[j].state);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                implies k != key
            by {
                let w = choose|jj: int|
                    0 <= jj < self.coins@.len()
                    && #[trigger] self.coins@[jj].purse == k.0
                    && self.coins@[jj].idx == k.1;
                assert(self.coins@[w].purse == k.0);
            }
        }
        None
    }

    /// Synchronous read: local state of the entry keyed `key`, or
    /// `None` if no such entry exists. Quint analog:
    /// `entries.get(key).local`.
    pub fn entry_local_state(&self, key: (PurseId, u64))
        -> (res: Option<EntryLocal>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(s) =>
                    self.entries().dom().contains(key)
                    && s == self.entries()[key].local,
                None => !self.entries().dom().contains(key),
            },
    {
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.entries@[jj]).purse != key.0
                    || self.entries@[jj].idx != key.1,
            decreases self.entries.len() - j,
        {
            if self.entries[j].purse == key.0 && self.entries[j].idx == key.1 {
                proof {
                    assert(self.spec_entries@.dom().contains(key));
                }
                return Some(self.entries[j].local);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                implies k != key
            by {
                let w = choose|jj: int|
                    0 <= jj < self.entries@.len()
                    && #[trigger] self.entries@[jj].purse == k.0
                    && self.entries@[jj].idx == k.1;
                assert(self.entries@[w].purse == k.0);
            }
        }
        None
    }

    /// Synchronous read: on-chain state of the entry keyed `key`, or
    /// `None` if no such entry exists. Quint analog:
    /// `entries.get(key).onChain`.
    pub fn entry_on_chain_state(&self, key: (PurseId, u64))
        -> (res: Option<EntryOnChain>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(s) =>
                    self.entries().dom().contains(key)
                    && s == self.entries()[key].on_chain,
                None => !self.entries().dom().contains(key),
            },
    {
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.entries@[jj]).purse != key.0
                    || self.entries@[jj].idx != key.1,
            decreases self.entries.len() - j,
        {
            if self.entries[j].purse == key.0 && self.entries[j].idx == key.1 {
                proof {
                    assert(self.spec_entries@.dom().contains(key));
                }
                return Some(self.entries[j].on_chain);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                implies k != key
            by {
                let w = choose|jj: int|
                    0 <= jj < self.entries@.len()
                    && #[trigger] self.entries@[jj].purse == k.0
                    && self.entries@[jj].idx == k.1;
                assert(self.entries@[w].purse == k.0);
            }
        }
        None
    }

    /// Synchronous read: the full `CoinRec` for `key`, or `None` if the
    /// coin doesn't exist. Avoids repeated per-field lookup calls.
    pub fn coin_record(&self, key: (PurseId, u64)) -> (res: Option<CoinRec>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(c) =>
                    self.coins().dom().contains(key)
                    && c == self.coins()[key],
                None => !self.coins().dom().contains(key),
            },
    {
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).purse != key.0
                    || self.coins@[jj].idx != key.1,
            decreases self.coins.len() - j,
        {
            if self.coins[j].purse == key.0 && self.coins[j].idx == key.1 {
                proof {
                    assert(self.spec_coins@.dom().contains(key));
                }
                return Some(self.coins[j]);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                implies k != key
            by {
                let w = choose|jj: int|
                    0 <= jj < self.coins@.len()
                    && #[trigger] self.coins@[jj].purse == k.0
                    && self.coins@[jj].idx == k.1;
                assert(self.coins@[w].purse == k.0);
            }
        }
        None
    }

    /// Synchronous read: the full `EntryRec` for `key`, or `None` if
    /// the entry doesn't exist.
    pub fn entry_record(&self, key: (PurseId, u64)) -> (res: Option<EntryRec>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(e) =>
                    self.entries().dom().contains(key)
                    && e == self.entries()[key],
                None => !self.entries().dom().contains(key),
            },
    {
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.entries@[jj]).purse != key.0
                    || self.entries@[jj].idx != key.1,
            decreases self.entries.len() - j,
        {
            if self.entries[j].purse == key.0 && self.entries[j].idx == key.1 {
                proof {
                    assert(self.spec_entries@.dom().contains(key));
                }
                return Some(self.entries[j]);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                implies k != key
            by {
                let w = choose|jj: int|
                    0 <= jj < self.entries@.len()
                    && #[trigger] self.entries@[jj].purse == k.0
                    && self.entries@[jj].idx == k.1;
                assert(self.entries@[w].purse == k.0);
            }
        }
        None
    }

    /// Number of purses in the state.
    pub fn total_purses(&self) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count == self.purses@.len(),
    {
        self.purses.len()
    }

    /// Number of coins (across all states and purses) in the state.
    pub fn total_coins(&self) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count == self.coins@.len(),
    {
        self.coins.len()
    }

    /// Number of recycler entries (across all states and purses).
    pub fn total_entries(&self) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count == self.entries@.len(),
    {
        self.entries.len()
    }

    /// Number of operations (terminal or in-flight) in the state.
    pub fn total_operations(&self) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count == self.operations@.len(),
    {
        self.operations.len()
    }

    /// Result-returning variant of `op_status`. Returns
    /// `Err(OperationNotFound(handle))` when the op handle is unknown
    /// — the surface a host's RPC layer typically needs.
    pub fn query_op_status(&self, handle: OpHandle) -> (res: Result<OpStatus, Error>)
        requires
            self.invariant(),
        ensures
            match res {
                Ok(s) =>
                    self.operations().dom().contains(handle)
                    && s == self.operations()[handle].status,
                Err(Error::OperationNotFound(h)) =>
                    !self.operations().dom().contains(handle) && h == handle,
                Err(_) => false,
            },
    {
        match self.op_status(handle) {
            Some(s) => Ok(s),
            None => Err(Error::OperationNotFound(handle)),
        }
    }

    /// Result-returning variant of `coin_record`. Errors with
    /// `Internal` when the coin doesn't exist (callers that want a
    /// distinguishing error variant should match on `None` from
    /// `coin_record` directly).
    pub fn query_coin_record(&self, key: (PurseId, u64))
        -> (res: Result<CoinRec, Error>)
        requires
            self.invariant(),
        ensures
            match res {
                Ok(c) =>
                    self.coins().dom().contains(key)
                    && c == self.coins()[key],
                Err(_) => !self.coins().dom().contains(key),
            },
    {
        match self.coin_record(key) {
            Some(c) => Ok(c),
            None => Err(Error::Internal(Vec::new())),
        }
    }

    /// Result-returning variant of `entry_record`.
    pub fn query_entry_record(&self, key: (PurseId, u64))
        -> (res: Result<EntryRec, Error>)
        requires
            self.invariant(),
        ensures
            match res {
                Ok(e) =>
                    self.entries().dom().contains(key)
                    && e == self.entries()[key],
                Err(_) => !self.entries().dom().contains(key),
            },
    {
        match self.entry_record(key) {
            Some(e) => Ok(e),
            None => Err(Error::Internal(Vec::new())),
        }
    }

    /// Check: does any *non-terminal* operation target purse `p`?
    /// Returns `true` iff at least one operation has `purse == p` and a
    /// status in {Preparing, Submitted, InBlock, Finalized, Waiting(_)}.
    /// Useful for delete-purse readiness checks where terminal ops can
    /// be ignored.
    pub fn has_in_flight_op_for_purse(&self, p: PurseId) -> (res: bool)
        requires
            self.invariant(),
        ensures
            res == exists|h: OpHandle|
                #[trigger] self.operations().dom().contains(h)
                && self.operations()[h].purse == p
                && !is_terminal_op_status(self.operations()[h].status),
    {
        let mut j: usize = 0;
        while j < self.operations.len()
            invariant
                0 <= j <= self.operations.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.operations@[jj]).purse != p
                    || is_terminal_op_status(self.operations@[jj].status),
            decreases self.operations.len() - j,
        {
            let op = &self.operations[j];
            let is_terminal = match op.status {
                OpStatus::Done => true,
                OpStatus::Failed => true,
                _ => false,
            };
            if op.purse == p && !is_terminal {
                #[allow(unused_variables)]
                let h = op.handle;
                proof {
                    assert(self.spec_operations@.dom().contains(h));
                    assert(self.operations()[h].purse == p);
                    assert(!is_terminal_op_status(self.operations()[h].status));
                }
                return true;
            }
            j = j + 1;
        }
        proof {
            assert forall|h: OpHandle|
                #[trigger] self.operations().dom().contains(h)
                && self.operations()[h].purse == p
                implies is_terminal_op_status(self.operations()[h].status)
            by {
                let w = choose|jj: int|
                    0 <= jj < self.operations@.len()
                    && #[trigger] self.operations@[jj].handle == h;
                assert(self.operations@[w].handle == h);
            }
        }
        false
    }

    /// Check: does any operation target purse `p`? Returns `true` iff
    /// at least one operation has `op.purse == p`. Useful as a pre-flight
    /// guard before `delete_purse`, which requires no targeting ops.
    pub fn has_op_targeting_purse(&self, p: PurseId) -> (res: bool)
        requires
            self.invariant(),
        ensures
            res == exists|h: OpHandle|
                #[trigger] self.operations().dom().contains(h)
                && self.operations()[h].purse == p,
    {
        let mut j: usize = 0;
        while j < self.operations.len()
            invariant
                0 <= j <= self.operations.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.operations@[jj]).purse != p,
            decreases self.operations.len() - j,
        {
            if self.operations[j].purse == p {
                #[allow(unused_variables)]
                let h = self.operations[j].handle;
                proof {
                    assert(self.spec_operations@.dom().contains(h));
                    assert(self.operations()[h].purse == p);
                }
                return true;
            }
            j = j + 1;
        }
        proof {
            assert forall|h: OpHandle|
                #[trigger] self.operations().dom().contains(h)
                implies self.operations()[h].purse != p
            by {
                let w = choose|jj: int|
                    0 <= jj < self.operations@.len()
                    && #[trigger] self.operations@[jj].handle == h;
                assert(self.operations@[w].handle == h);
            }
        }
        false
    }

    /// Result-returning variant of `op_meta`.
    pub fn query_op_meta(&self, handle: OpHandle)
        -> (res: Result<(OpKind, PurseId), Error>)
        requires
            self.invariant(),
        ensures
            match res {
                Ok((k, p)) =>
                    self.operations().dom().contains(handle)
                    && k == self.operations()[handle].kind
                    && p == self.operations()[handle].purse,
                Err(Error::OperationNotFound(h)) =>
                    !self.operations().dom().contains(handle) && h == handle,
                Err(_) => false,
            },
    {
        match self.op_meta(handle) {
            Some(m) => Ok(m),
            None => Err(Error::OperationNotFound(handle)),
        }
    }

    /// Synchronous read: the `(kind, purse)` pair of the operation
    /// `handle`, or `None` if no such operation exists. Used to route
    /// chain events back to the right purse / op-kind handler.
    pub fn op_meta(&self, handle: OpHandle) -> (res: Option<(OpKind, PurseId)>)
        requires
            self.invariant(),
        ensures
            match res {
                Some((k, p)) =>
                    self.operations().dom().contains(handle)
                    && k == self.operations()[handle].kind
                    && p == self.operations()[handle].purse,
                None => !self.operations().dom().contains(handle),
            },
    {
        let mut j: usize = 0;
        while j < self.operations.len()
            invariant
                0 <= j <= self.operations.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.operations@[jj]).handle != handle,
            decreases self.operations.len() - j,
        {
            if self.operations[j].handle == handle {
                proof {
                    assert(self.spec_operations@.dom().contains(handle));
                }
                return Some((self.operations[j].kind, self.operations[j].purse));
            }
            j = j + 1;
        }
        proof {
            assert forall|h: OpHandle|
                #[trigger] self.operations().dom().contains(h)
                implies h != handle
            by {
                let w = choose|jj: int|
                    0 <= jj < self.operations@.len()
                    && #[trigger] self.operations@[jj].handle == h;
                assert(self.operations@[w].handle == h);
            }
        }
        None
    }

    /// Synchronous read: status of the operation `handle`, or `None`
    /// if no such operation exists. Quint analog: `operations.get(h).status`.
    pub fn op_status(&self, handle: OpHandle) -> (res: Option<OpStatus>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(s) =>
                    self.operations().dom().contains(handle)
                    && s == self.operations()[handle].status,
                None => !self.operations().dom().contains(handle),
            },
    {
        let mut j: usize = 0;
        while j < self.operations.len()
            invariant
                0 <= j <= self.operations.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.operations@[jj]).handle != handle,
            decreases self.operations.len() - j,
        {
            if self.operations[j].handle == handle {
                proof {
                    assert(self.spec_operations@.dom().contains(handle));
                }
                return Some(self.operations[j].status);
            }
            j = j + 1;
        }
        proof {
            assert forall|h: OpHandle|
                #[trigger] self.operations().dom().contains(h)
                implies h != handle
            by {
                let w = choose|jj: int|
                    0 <= jj < self.operations@.len()
                    && #[trigger] self.operations@[jj].handle == h;
                assert(self.operations@[w].handle == h);
            }
        }
        None
    }

    /// Internal: read the `exponent` of a coin known to exist by `key`.
    fn read_coin_exponent(&self, key: (PurseId, u64)) -> (exp: u8)
        requires
            self.invariant(),
            self.coins().dom().contains(key),
        ensures
            exp == self.coins()[key].exponent,
    {
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                self.coins().dom().contains(key),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).purse != key.0
                    || self.coins@[jj].idx != key.1,
            decreases self.coins.len() - j,
        {
            if self.coins[j].purse == key.0 && self.coins[j].idx == key.1 {
                proof {
                    // (l) gives us that self.coins@[j] matches the ghost record at this key.
                    assert(self.spec_coins@[(self.coins@[j as int].purse, self.coins@[j as int].idx)]
                        == self.coins@[j as int]);
                }
                return self.coins[j].exponent;
            }
            j = j + 1;
        }
        // Unreachable: precondition + (m) guarantee a Vec witness.
        proof {
            let w = choose|jj: int|
                0 <= jj < self.coins@.len()
                && #[trigger] self.coins@[jj].purse == key.0
                && self.coins@[jj].idx == key.1;
        }
        vstd::pervasive::unreached()
    }

    /// True iff `key` is currently in the coin map. O(n) scan; useful for
    /// gap-limit recovery (Appendix C) which probes (purse, idx) tuples
    /// without a precomputed index.
    pub fn has_coin(&self, key: (PurseId, u64)) -> (b: bool)
        requires
            self.invariant(),
        ensures
            b == self.coins().dom().contains(key),
    {
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).purse != key.0
                    || self.coins@[jj].idx != key.1,
            decreases self.coins.len() - j,
        {
            if self.coins[j].purse == key.0 && self.coins[j].idx == key.1 {
                proof {
                    assert(self.spec_coins@.dom().contains(
                        (self.coins@[j as int].purse, self.coins@[j as int].idx)
                    ));
                }
                return true;
            }
            j = j + 1;
        }
        // No Vec witness for `key`: by (m), key not in ghost dom.
        proof {
            if self.coins().dom().contains(key) {
                let w = choose|jj: int|
                    0 <= jj < self.coins@.len()
                    && #[trigger] self.coins@[jj].purse == key.0
                    && self.coins@[jj].idx == key.1;
                assert(self.coins@[w].purse == key.0);
            }
        }
        false
    }

    /// Gap-limit recovery scan (Appendix C). Probes coin indices
    /// `0, 1, 2, …, max_idx` in purse `p`, returning each existing key.
    /// Termination: after seeing `gap_limit` consecutive missing indices,
    /// the scan stops early.
    ///
    /// **Pilot scope:** the contract guarantees soundness (every returned
    /// key is in the coin map under purse `p`) but is *not* complete with
    /// respect to "discovered all coins below `max_idx`". A coin at idx
    /// `i` may be missed if a gap of length `gap_limit` precedes it.
    /// Real recovery in the design relies on a high-enough gap_limit
    /// (per RFC-6 derivation discipline) to make this safe in practice.
    pub fn scan_with_gap_limit(&self, p: PurseId, gap_limit: u64, max_idx: u64)
        -> (found: Vec<(PurseId, u64)>)
        requires
            self.invariant(),
        ensures
            // Each returned key is in the coin map under purse `p`.
            forall|i: int| 0 <= i < found@.len() ==>
                self.coins().dom().contains(#[trigger] found@[i])
                && found@[i].0 == p,
    {
        let mut found: Vec<(PurseId, u64)> = Vec::new();
        let mut i: u64 = 0;
        let mut gap: u64 = 0;
        loop
            invariant
                self.invariant(),
                i <= max_idx + 1,
                gap <= gap_limit,
                forall|k: int| 0 <= k < found@.len() ==>
                    self.coins().dom().contains(#[trigger] found@[k])
                    && found@[k].0 == p,
            decreases
                if gap >= gap_limit || i > max_idx { 0int }
                else { (max_idx - i) as int + 1 },
        {
            if i > max_idx { break; }
            if gap >= gap_limit { break; }
            if self.has_coin((p, i)) {
                found.push((p, i));
                gap = 0;
            } else {
                gap = gap + 1;
            }
            if i == u64::MAX { break; }
            i = i + 1;
        }
        found
    }

    /// True iff `key` is currently in the entry map.
    pub fn has_entry(&self, key: (PurseId, u64)) -> (b: bool)
        requires
            self.invariant(),
        ensures
            b == self.entries().dom().contains(key),
    {
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.entries@[jj]).purse != key.0
                    || self.entries@[jj].idx != key.1,
            decreases self.entries.len() - j,
        {
            if self.entries[j].purse == key.0 && self.entries[j].idx == key.1 {
                proof {
                    assert(self.spec_entries@.dom().contains(
                        (self.entries@[j as int].purse, self.entries@[j as int].idx)
                    ));
                }
                return true;
            }
            j = j + 1;
        }
        proof {
            if self.entries().dom().contains(key) {
                let w = choose|jj: int|
                    0 <= jj < self.entries@.len()
                    && #[trigger] self.entries@[jj].purse == key.0
                    && self.entries@[jj].idx == key.1;
                assert(self.entries@[w].purse == key.0);
            }
        }
        false
    }

    /// Gap-limit recovery scan for entries. Same shape as `scan_with_gap_limit`
    /// but probing the entry map instead of the coin map.
    pub fn scan_entries_with_gap_limit(&self, p: PurseId, gap_limit: u64, max_idx: u64)
        -> (found: Vec<(PurseId, u64)>)
        requires
            self.invariant(),
        ensures
            forall|i: int| 0 <= i < found@.len() ==>
                self.entries().dom().contains(#[trigger] found@[i])
                && found@[i].0 == p,
    {
        let mut found: Vec<(PurseId, u64)> = Vec::new();
        let mut i: u64 = 0;
        let mut gap: u64 = 0;
        loop
            invariant
                self.invariant(),
                i <= max_idx + 1,
                gap <= gap_limit,
                forall|k: int| 0 <= k < found@.len() ==>
                    self.entries().dom().contains(#[trigger] found@[k])
                    && found@[k].0 == p,
            decreases
                if gap >= gap_limit || i > max_idx { 0int }
                else { (max_idx - i) as int + 1 },
        {
            if i > max_idx { break; }
            if gap >= gap_limit { break; }
            if self.has_entry((p, i)) {
                found.push((p, i));
                gap = 0;
            } else {
                gap = gap + 1;
            }
            if i == u64::MAX { break; }
            i = i + 1;
        }
        found
    }

    /// Composite operation: `transfer(from, to, min_exp)` selects an
    /// `Available` coin in purse `from` with `exponent >= min_exp`, walks
    /// it through `PendingSpend → Spent` (simulating chain settlement),
    /// then mints a fresh coin in purse `to` with the same exponent.
    ///
    /// Returns the new coin's `(to, idx)` key, or `None` if no suitable
    /// coin was available in `from`.
    pub fn transfer(&mut self, from: PurseId, to: PurseId, min_exp: u8)
        -> (res: Option<(PurseId, u64)>)
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(to),
            old(self).purses()[to].next_coin_idx < u64::MAX,
            old(self).next_age < u64::MAX,
            old(self).events@.len() + 2 <= u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            match res {
                Some(new_key) =>
                    new_key.0 == to
                    && final(self).coins().dom().contains(new_key)
                    && final(self).coins()[new_key].state == CoinState::Available
                    && final(self).coins()[new_key].exponent >= min_exp
                    && final(self).next_age == old(self).next_age + 1,
                None =>
                    // No Available coin in `from` met the threshold.
                    final(self).next_age == old(self).next_age
                    && forall|k: (PurseId, u64)|
                        #[trigger] old(self).coins().dom().contains(k)
                        && k.0 == from
                        && old(self).coins()[k].state == CoinState::Available
                        ==> old(self).coins()[k].exponent < min_exp,
            },
    {
        match self.select_coin(from, min_exp) {
            None => None,
            Some(key) => {
                let exp = self.read_coin_exponent(key);
                self.mark_coin_pending_spend(key);
                self.mark_coin_spent(key);
                let new_key = self.add_coin(to, exp);
                self.mark_coin_observed(new_key);
                Some(new_key)
            }
        }
    }

    /// Tracked transfer: same effect as `transfer`, but wrapped in an
    /// operation handle so the upper layer can correlate the transfer
    /// with chain confirmation, cancellation, and status streams.
    ///
    /// Lifecycle: an operation record is created in `Preparing`, walked
    /// through `Submitted`, and ends in `Done` (on Some) or `Failed`
    /// (on None — no Available coin met the threshold).
    pub fn tracked_transfer(&mut self, from: PurseId, to: PurseId, min_exp: u8)
        -> (res: (OpHandle, Option<(PurseId, u64)>))
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(from),
            old(self).purses().dom().contains(to),
            old(self).purses()[to].next_coin_idx < u64::MAX,
            old(self).next_handle < u64::MAX,
            old(self).next_age < u64::MAX,
            old(self).events@.len() + 3 <= u64::MAX as nat,
        ensures
            final(self).invariant(),
            res.0 == old(self).next_handle,
            final(self).operations().dom().contains(res.0),
            // Op ended in Done if Some, Failed if None.
            match res.1 {
                Some(_) => final(self).operations()[res.0].status == OpStatus::Done,
                None => final(self).operations()[res.0].status == OpStatus::Failed,
            },
            final(self).operations()[res.0].kind == OpKind::Transfer,
            final(self).operations()[res.0].purse == from,
    {
        let handle = self.start_op(OpKind::Transfer, from);
        proof {
            assert(self.operations()[handle].kind == OpKind::Transfer);
            assert(self.operations()[handle].purse == from);
        }
        self.set_op_status(handle, OpStatus::Submitted);
        proof {
            assert(self.operations()[handle].kind == OpKind::Transfer);
            assert(self.operations()[handle].purse == from);
        }
        let result = self.transfer(from, to, min_exp);
        proof {
            assert(self.operations()[handle].kind == OpKind::Transfer);
            assert(self.operations()[handle].purse == from);
        }
        match result {
            Some(_) => self.set_op_status(handle, OpStatus::Done),
            None => self.set_op_status(handle, OpStatus::Failed),
        }
        proof {
            assert(self.operations()[handle].kind == OpKind::Transfer);
            assert(self.operations()[handle].purse == from);
        }
        (handle, result)
    }

    /// Tracked export: wraps [`Self::export_coin`] in a `KExport`
    /// operation. Returns the op handle so the caller can correlate
    /// later chain events to this op.
    pub fn tracked_export_coin(&mut self, key: (PurseId, u64))
        -> (handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::Available,
            old(self).next_handle < u64::MAX,
            old(self).events@.len() + 3 <= u64::MAX as nat,
        ensures
            final(self).invariant(),
            handle == old(self).next_handle,
            final(self).operations().dom().contains(handle),
            final(self).operations()[handle].status == OpStatus::Submitted,
            final(self).operations()[handle].kind == OpKind::Export,
            final(self).operations()[handle].purse == key.0,
            final(self).coins().dom().contains(key),
            final(self).coins()[key].state == CoinState::Spent,
    {
        let h = self.start_op(OpKind::Export, key.0);
        proof {
            assert(self.operations()[h].kind == OpKind::Export);
            assert(self.operations()[h].purse == key.0);
        }
        self.export_coin(key);
        proof {
            assert(self.operations()[h].kind == OpKind::Export);
            assert(self.operations()[h].purse == key.0);
        }
        self.mark_op_submitted(h);
        h
    }

    /// Tracked import: wraps [`Self::import_coin`] in a `KImport`
    /// operation. Returns `(handle, new_coin_key)`.
    pub fn tracked_import_coin(&mut self, p: PurseId, exponent: u8, account: u64)
        -> (res: (OpHandle, (PurseId, u64)))
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(p),
            old(self).purses()[p].next_coin_idx < u64::MAX,
            old(self).next_age < u64::MAX,
            old(self).next_handle < u64::MAX,
            old(self).events@.len() + 3 <= u64::MAX as nat,
            exponent <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            res.0 == old(self).next_handle,
            final(self).operations().dom().contains(res.0),
            final(self).operations()[res.0].status == OpStatus::Submitted,
            final(self).operations()[res.0].kind == OpKind::Import,
            final(self).operations()[res.0].purse == p,
            res.1.0 == p,
            final(self).coins().dom().contains(res.1),
            final(self).coins()[res.1].state == CoinState::Available,
            final(self).coins()[res.1].exponent == exponent,
            final(self).coins()[res.1].account == account,
    {
        let h = self.start_op(OpKind::Import, p);
        proof {
            assert(self.operations()[h].kind == OpKind::Import);
            assert(self.operations()[h].purse == p);
        }
        let new_key = self.import_coin(p, exponent, account);
        proof {
            assert(self.operations()[h].kind == OpKind::Import);
            assert(self.operations()[h].purse == p);
        }
        self.mark_op_submitted(h);
        (h, new_key)
    }

    /// Export a coin: the layer surrenders custody of a specific
    /// `Available` coin (the host has handed its secret to an external
    /// party). The coin transitions Available → PendingSpend → Spent;
    /// no new coin is minted. Quint analog: `exportCoin`.
    pub fn export_coin(&mut self, key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::Available,
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins().dom().contains(key),
            final(self).coins()[key].state == CoinState::Spent,
            final(self).coins()[key].exponent == old(self).coins()[key].exponent,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::CoinSpent {
                purse: key.0,
                exponent: old(self).coins()[key].exponent,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        self.mark_coin_pending_spend(key);
        self.mark_coin_spent(key);
    }

    /// Import a coin: an external (account, secret) pair becomes a
    /// fresh `Available` coin in purse `p` carrying that account.
    /// Quint analog: `importCoin`. The coin skips the Pending →
    /// Available chain-observation gap (the host has already verified
    /// the coin exists on-chain via the imported secret).
    pub fn import_coin(&mut self, p: PurseId, exponent: u8, account: u64)
        -> (key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(p),
            old(self).purses()[p].next_coin_idx < u64::MAX,
            old(self).next_age < u64::MAX,
            old(self).events@.len() < u64::MAX as nat,
            exponent <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            key.0 == p,
            key.1 == old(self).purses()[p].next_coin_idx,
            final(self).coins().dom().contains(key),
            final(self).coins()[key].state == CoinState::Available,
            final(self).coins()[key].exponent == exponent,
            final(self).coins()[key].account == account,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).events@.len() == old(self).events@.len() + 1,
    {
        let key = self.add_coin_with_account(p, exponent, account);
        self.mark_coin_observed(key);
        key
    }

    /// Rebalance: move one specific `Available` coin from purse `src` to
    /// purse `dst`. The source coin transitions Available → PendingSpend
    /// → Spent; a fresh `Available` coin with the same exponent is minted
    /// in `dst`'s namespace. Quint §6.1.3 `rebalancePurse`.
    ///
    /// Differs from `transfer` in that the caller selects the specific
    /// coin (no min-exp search), and `src != dst` is required.
    #[allow(unused_variables)]
    pub fn rebalance(&mut self, src: PurseId, dst: PurseId, key: (PurseId, u64))
        -> (new_key: (PurseId, u64))
        requires
            old(self).invariant(),
            src != dst,
            key.0 == src,
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::Available,
            old(self).purses().dom().contains(dst),
            old(self).purses()[dst].next_coin_idx < u64::MAX,
            old(self).events@.len() + 2 <= u64::MAX as nat,
            old(self).next_age < u64::MAX,
        ensures
            final(self).invariant(),
            new_key.0 == dst,
            new_key.1 == old(self).purses()[dst].next_coin_idx,
            final(self).coins().dom().contains(new_key),
            final(self).coins()[new_key].state == CoinState::Available,
            final(self).coins()[new_key].exponent == old(self).coins()[key].exponent,
            final(self).coins().dom().contains(key),
            final(self).coins()[key].state == CoinState::Spent,
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).events@.len() == old(self).events@.len() + 2,
    {
        let exp = self.read_coin_exponent(key);
        self.mark_coin_pending_spend(key);
        self.mark_coin_spent(key);
        let new_key = self.add_coin(dst, exp);
        self.mark_coin_observed(new_key);
        new_key
    }

    /// Tracked rebalance: wraps [`Self::rebalance`] in a `KRebalance`
    /// operation. Allocates the op handle, runs the rebalance (src
    /// coin → spent, dst coin minted), advances the op to `Submitted`.
    /// Returns `(handle, new_coin_key)` so the caller can correlate
    /// later chain events to this op.
    pub fn tracked_rebalance(
        &mut self,
        src: PurseId,
        dst: PurseId,
        key: (PurseId, u64),
    ) -> (res: (OpHandle, (PurseId, u64)))
        requires
            old(self).invariant(),
            src != dst,
            key.0 == src,
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::Available,
            old(self).purses().dom().contains(dst),
            old(self).purses()[dst].next_coin_idx < u64::MAX,
            old(self).next_age < u64::MAX,
            old(self).next_handle < u64::MAX,
            old(self).events@.len() + 4 <= u64::MAX as nat,
        ensures
            final(self).invariant(),
            res.0 == old(self).next_handle,
            final(self).operations().dom().contains(res.0),
            final(self).operations()[res.0].status == OpStatus::Submitted,
            final(self).operations()[res.0].kind == OpKind::Rebalance,
            final(self).operations()[res.0].purse == src,
            res.1.0 == dst,
            final(self).coins().dom().contains(res.1),
            final(self).coins()[res.1].state == CoinState::Available,
            final(self).coins()[res.1].exponent == old(self).coins()[key].exponent,
    {
        let handle = self.start_op(OpKind::Rebalance, src);
        proof {
            assert(self.operations()[handle].kind == OpKind::Rebalance);
            assert(self.operations()[handle].purse == src);
        }
        let new_key = self.rebalance(src, dst, key);
        proof {
            assert(self.operations()[handle].kind == OpKind::Rebalance);
            assert(self.operations()[handle].purse == src);
        }
        self.mark_op_submitted(handle);
        (handle, new_key)
    }

    /// Tracked split: wraps [`Self::split_coin`] in a `KMaintenance`
    /// operation. Returns the op handle. Used when the host wants the
    /// chain to settle the split before the new coins are committed.
    pub fn tracked_split_coin(
        &mut self,
        key: (PurseId, u64),
        new_exponents: Vec<u8>,
    ) -> (handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::Available,
            old(self).purses().dom().contains(key.0),
            old(self).purses()[key.0].next_coin_idx as nat + new_exponents@.len()
                <= u64::MAX as nat,
            old(self).next_age as nat + new_exponents@.len() <= u64::MAX as nat,
            old(self).next_handle < u64::MAX,
            old(self).events@.len() + 3 <= u64::MAX as nat,
            forall|j: int| 0 <= j < new_exponents@.len() ==>
                (#[trigger] new_exponents@[j]) <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            handle == old(self).next_handle,
            final(self).operations().dom().contains(handle),
            final(self).operations()[handle].status == OpStatus::Submitted,
            final(self).operations()[handle].kind == OpKind::Maintenance,
            final(self).operations()[handle].purse == key.0,
            final(self).coins().dom().contains(key),
            final(self).coins()[key].state == CoinState::Spent,
    {
        let h = self.start_op(OpKind::Maintenance, key.0);
        proof {
            assert(self.operations()[h].kind == OpKind::Maintenance);
            assert(self.operations()[h].purse == key.0);
            assert(self.coins()[key].state == CoinState::Available);
        }
        self.split_coin(key, new_exponents);
        proof {
            assert(self.operations()[h].kind == OpKind::Maintenance);
            assert(self.operations()[h].purse == key.0);
        }
        self.mark_op_submitted(h);
        h
    }

    /// Split a single `Available` coin into a batch of fresh coins in the
    /// same purse, one per element of `new_exponents`. Quint analog: the
    /// Tier-2 split step of three-tier selection.
    ///
    /// The source coin walks Available → PendingSpend → Spent. The new
    /// coins arrive in `Pending` state (chain settlement is simulated by
    /// the existing `add_coin` semantics; the caller invokes
    /// `mark_coin_observed` on each later if needed).
    ///
    /// **Pilot scope:** no value-preservation check between the source
    /// coin's exponent and the sum of new exponents. The design requires
    /// `sum(coin_value(new_exp)) == coin_value(old_exp)`; verifying this
    /// requires the real `2^exp` semantics (deferred — see stage 7c).
    pub fn split_coin(&mut self, key: (PurseId, u64), new_exponents: Vec<u8>)
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::Available,
            old(self).purses().dom().contains(key.0),
            old(self).purses()[key.0].next_coin_idx as nat + new_exponents@.len()
                <= u64::MAX as nat,
            old(self).next_age as nat + new_exponents@.len() <= u64::MAX as nat,
            old(self).events@.len() < u64::MAX as nat,
            forall|j: int| 0 <= j < new_exponents@.len() ==>
                (#[trigger] new_exponents@[j]) <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            final(self).coins().dom().contains(key),
            final(self).coins()[key].state == CoinState::Spent,
            final(self).purses()[key.0].next_coin_idx
                == old(self).purses()[key.0].next_coin_idx + new_exponents@.len(),
            // Each new coin key sits at sequential next_coin_idx slots.
            forall|j: int| 0 <= j < new_exponents@.len() ==>
                #[trigger] final(self).coins().dom().contains(
                    (key.0, (old(self).purses()[key.0].next_coin_idx + j) as u64)
                )
                && final(self).coins()[
                    (key.0, (old(self).purses()[key.0].next_coin_idx + j) as u64)
                ].exponent == new_exponents@[j],
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).events@.len() == old(self).events@.len() + 1,
    {
        self.mark_coin_pending_spend(key);
        self.mark_coin_spent(key);
        let ghost pre_top_up_coins = self.coins();
        let ghost pre_top_up_purses = self.purses();
        self.top_up_purse(key.0, new_exponents);
        proof {
            // top_up_purse preserves existing keys: key is still in dom with
            // its Spent state.
            assert(pre_top_up_coins.dom().contains(key));
            assert(pre_top_up_coins[key].state == CoinState::Spent);
        }
    }

    /// Tracked unload via entry: wraps [`Self::unload_via_entry`] in a
    /// `KExternalOffload` operation. Allocates the op handle, runs the
    /// unload (entry → coin), then advances the op to `Submitted`.
    /// Returns `(handle, new_coin_key)` so callers can correlate later
    /// chain events to this operation.
    ///
    /// Quint analog: the full lifecycle of `startExternalOffload`
    /// reduced to its local-state effects.
    pub fn tracked_unload_via_entry(&mut self, key: (PurseId, u64))
        -> (res: (OpHandle, (PurseId, u64)))
        requires
            old(self).invariant(),
            old(self).entries().dom().contains(key),
            old(self).entries()[key].local == EntryLocal::LocalAvailable,
            old(self).entries()[key].on_chain == EntryOnChain::Ready,
            old(self).purses().dom().contains(key.0),
            old(self).purses()[key.0].next_coin_idx < u64::MAX,
            old(self).next_age < u64::MAX,
            old(self).next_handle < u64::MAX,
            old(self).events@.len() + 3 <= u64::MAX as nat,
        ensures
            final(self).invariant(),
            res.0 == old(self).next_handle,
            final(self).operations().dom().contains(res.0),
            final(self).operations()[res.0].status == OpStatus::Submitted,
            final(self).operations()[res.0].kind == OpKind::ExternalOffload,
            final(self).operations()[res.0].purse == key.0,
            res.1.0 == key.0,
            final(self).coins().dom().contains(res.1),
            final(self).coins()[res.1].state == CoinState::Available,
            final(self).coins()[res.1].exponent == old(self).entries()[key].exponent,
    {
        let handle = self.start_op(OpKind::ExternalOffload, key.0);
        proof {
            assert(self.operations()[handle].kind == OpKind::ExternalOffload);
            assert(self.operations()[handle].purse == key.0);
        }
        let new_coin_key = self.unload_via_entry(key, handle);
        proof {
            assert(self.operations()[handle].kind == OpKind::ExternalOffload);
            assert(self.operations()[handle].purse == key.0);
        }
        self.mark_op_submitted(handle);
        (handle, new_coin_key)
    }

    /// Tier-3 unload: consume a `Ready` recycler entry to mint a fresh
    /// `Available` coin in the same purse. The entry walks
    /// `LocalAvailable → LocalLockedFor → LocalConsumed`; the new coin
    /// walks `Pending → Available` via observation.
    ///
    /// Quint analog: the local-state effect of `startExternalOffload`
    /// (without the external account / chain-side bookkeeping).
    pub fn unload_via_entry(&mut self, key: (PurseId, u64), handle: OpHandle)
        -> (new_coin_key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).entries().dom().contains(key),
            old(self).entries()[key].local == EntryLocal::LocalAvailable,
            old(self).entries()[key].on_chain == EntryOnChain::Ready,
            old(self).purses().dom().contains(key.0),
            old(self).purses()[key.0].next_coin_idx < u64::MAX,
            old(self).next_age < u64::MAX,
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            // Source entry consumed.
            final(self).entries().dom().contains(key),
            final(self).entries()[key].local == EntryLocal::LocalConsumed,
            final(self).entries()[key].on_chain == EntryOnChain::Ready,
            // New coin minted in the same purse, Available, with entry's exponent.
            new_coin_key.0 == key.0,
            new_coin_key.1 == old(self).purses()[key.0].next_coin_idx,
            final(self).coins().dom().contains(new_coin_key),
            final(self).coins()[new_coin_key].state == CoinState::Available,
            final(self).coins()[new_coin_key].exponent == old(self).entries()[key].exponent,
            // Operations untouched: this is a state-mutating but op-agnostic primitive.
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).events@.len() == old(self).events@.len() + 1,
    {
        let exp = self.read_entry_exponent(key);
        self.set_entry_local(key, EntryLocal::LocalLockedFor(handle));
        self.set_entry_local(key, EntryLocal::LocalConsumed);
        let ghost post_consume_entries = self.entries();
        let new_key = self.add_coin(key.0, exp);
        self.mark_coin_observed(new_key);
        proof {
            // add_coin and mark_coin_observed preserve entries (sibling-field
            // stability). The entry's local==Consumed survives unchanged.
            assert(self.entries() == post_consume_entries);
            assert(post_consume_entries.dom().contains(key));
            assert(post_consume_entries[key].local == EntryLocal::LocalConsumed);
        }
        new_key
    }

    /// Select the first `Available` coin in purse `p` whose `exponent`
    /// meets or exceeds `min_exponent`. Returns `None` if no such coin
    /// exists.
    pub fn select_coin(&self, p: PurseId, min_exponent: u8)
        -> (res: Option<(PurseId, u64)>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(key) =>
                    self.coins().dom().contains(key)
                    && key.0 == p
                    && self.coins()[key].state == CoinState::Available
                    && self.coins()[key].exponent >= min_exponent,
                None =>
                    forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        && k.0 == p
                        && self.coins()[k].state == CoinState::Available
                        ==> self.coins()[k].exponent < min_exponent,
            },
    {
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).purse != p
                    || self.coins@[jj].state != CoinState::Available
                    || self.coins@[jj].exponent < min_exponent,
            decreases self.coins.len() - j,
        {
            let is_avail = matches!(self.coins[j].state, CoinState::Available);
            if self.coins[j].purse == p
                && is_avail
                && self.coins[j].exponent >= min_exponent
            {
                let key = (self.coins[j].purse, self.coins[j].idx);
                proof {
                    // (l) gives us key in dom and ghost matches Vec entry.
                    assert(self.spec_coins@.dom().contains(key));
                }
                return Some(key);
            }
            j = j + 1;
        }
        // Not found in the Vec scan; lift to "no such ghost key" via (m).
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                && k.0 == p
                && self.coins()[k].state == CoinState::Available
                implies self.coins()[k].exponent < min_exponent
            by {
                // (m) gives a Vec witness w; the loop's "not found" fact then
                // forces w to have either wrong purse, wrong state, or smaller
                // exponent. The first two are ruled out by the ghost record's
                // values (which match the Vec entry by (l)), leaving exponent.
                let w = choose|jj: int|
                    0 <= jj < self.coins@.len()
                    && #[trigger] self.coins@[jj].purse == k.0
                    && self.coins@[jj].idx == k.1;
                assert(self.coins@[w].purse == p);
                assert(self.coins@[w].state == self.coins()[k].state);
                assert(self.coins@[w].exponent == self.coins()[k].exponent);
            }
        }
        None
    }

    /// Degenerate exact-cover: find an `Available` coin in purse `p` whose
    /// `coin_value(exp)` equals `requested` exactly. Returns `None` if no
    /// single coin matches.
    ///
    /// **Pilot scope:** Tier-1 exact-cover in the design (§6.3) considers
    /// multi-coin subsets summing to `requested`. This single-coin form is
    /// the simplest case. Multi-coin exact subset-sum (powerset enumeration
    /// with lex-min disambiguation) is the natural extension; deferred.
    pub fn find_exact_single_coin(&self, p: PurseId, requested: u64)
        -> (res: Option<(PurseId, u64)>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(key) =>
                    self.coins().dom().contains(key)
                    && key.0 == p
                    && self.coins()[key].state == CoinState::Available
                    && coin_value(self.coins()[key].exponent) == requested as nat,
                None =>
                    forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        && k.0 == p
                        && self.coins()[k].state == CoinState::Available
                        ==> coin_value(self.coins()[k].exponent) != requested as nat,
            },
    {
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).purse != p
                    || self.coins@[jj].state != CoinState::Available
                    || coin_value(self.coins@[jj].exponent) != requested as nat,
            decreases self.coins.len() - j,
        {
            let is_avail = matches!(self.coins[j].state, CoinState::Available);
            proof {
                let coin_key = (self.coins@[j as int].purse, self.coins@[j as int].idx);
                assert(self.spec_coins@.dom().contains(coin_key));
                assert(self.spec_coins@[coin_key] == self.coins@[j as int]);
                assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
            }
            let value: u64 = pow2_u64_exec(self.coins[j].exponent);
            if self.coins[j].purse == p && is_avail && value == requested {
                let key = (self.coins[j].purse, self.coins[j].idx);
                proof {
                    assert(self.spec_coins@.dom().contains(key));
                }
                return Some(key);
            }
            j = j + 1;
        }
        // None: lift Vec-scan "not found" to a universal claim over the ghost
        // map via invariant (m), same as `select_coin`.
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                && k.0 == p
                && self.coins()[k].state == CoinState::Available
                implies coin_value(self.coins()[k].exponent) != requested as nat
            by {
                let w = choose|jj: int|
                    0 <= jj < self.coins@.len()
                    && #[trigger] self.coins@[jj].purse == k.0
                    && self.coins@[jj].idx == k.1;
                assert(self.coins@[w].purse == p);
                assert(self.coins@[w].state == self.coins()[k].state);
                assert(self.coins@[w].exponent == self.coins()[k].exponent);
            }
        }
        None
    }

    /// Find the highest-priority selectable entry in purse `p` —
    /// Ready on-chain, LocalAvailable locally — per the §6.3
    /// `entryOrderLT` ordering. Returns `None` if no such entry
    /// exists. Tiebreakers: ring_idx ascending, then idx ascending.
    pub fn find_top_priority_entry(&self, p: PurseId)
        -> (res: Option<(PurseId, u64)>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(key) =>
                    self.entries().dom().contains(key)
                    && key.0 == p
                    && self.entries()[key].on_chain == EntryOnChain::Ready
                    && self.entries()[key].local == EntryLocal::LocalAvailable
                    && forall|k: (PurseId, u64)|
                        #[trigger] self.entries().dom().contains(k)
                        && k.0 == p
                        && self.entries()[k].on_chain == EntryOnChain::Ready
                        && self.entries()[k].local == EntryLocal::LocalAvailable
                        && k != key
                        ==> entry_priority_lt(self.entries()[key], self.entries()[k])
                            || self.entries()[key] == self.entries()[k],
                None =>
                    forall|k: (PurseId, u64)|
                        #[trigger] self.entries().dom().contains(k)
                        && k.0 == p
                        ==> self.entries()[k].on_chain != EntryOnChain::Ready
                            || self.entries()[k].local != EntryLocal::LocalAvailable,
            },
    {
        let mut best: Option<usize> = None;
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.invariant(),
                match best {
                    Some(bi) =>
                        0 <= bi < j
                        && self.entries@[bi as int].purse == p
                        && self.entries@[bi as int].on_chain == EntryOnChain::Ready
                        && self.entries@[bi as int].local == EntryLocal::LocalAvailable
                        && forall|jj: int| 0 <= jj < j ==>
                            #[trigger] self.entries@[jj].purse != p
                            || self.entries@[jj].on_chain != EntryOnChain::Ready
                            || self.entries@[jj].local != EntryLocal::LocalAvailable
                            || entry_priority_lt(self.entries@[bi as int], self.entries@[jj])
                            || self.entries@[bi as int] == self.entries@[jj],
                    None =>
                        forall|jj: int| 0 <= jj < j ==>
                            (#[trigger] self.entries@[jj]).purse != p
                            || self.entries@[jj].on_chain != EntryOnChain::Ready
                            || self.entries@[jj].local != EntryLocal::LocalAvailable,
                },
            decreases self.entries.len() - j,
        {
            let e = &self.entries[j];
            let is_ready = matches!(e.on_chain, EntryOnChain::Ready);
            let is_local_avail = matches!(e.local, EntryLocal::LocalAvailable);
            if e.purse == p && is_ready && is_local_avail {
                match best {
                    None => { best = Some(j); }
                    Some(bi) => {
                        let cur_better = self.entries[bi].exponent < e.exponent
                            || (self.entries[bi].exponent == e.exponent
                                && self.entries[bi].ring_idx > e.ring_idx)
                            || (self.entries[bi].exponent == e.exponent
                                && self.entries[bi].ring_idx == e.ring_idx
                                && self.entries[bi].idx > e.idx);
                        if cur_better {
                            best = Some(j);
                        }
                    }
                }
            }
            j = j + 1;
        }
        match best {
            None => {
                proof {
                    assert forall|k: (PurseId, u64)|
                        #[trigger] self.entries().dom().contains(k)
                        && k.0 == p
                        implies self.entries()[k].on_chain != EntryOnChain::Ready
                            || self.entries()[k].local != EntryLocal::LocalAvailable
                    by {
                        let w = choose|jj: int|
                            0 <= jj < self.entries@.len()
                            && #[trigger] self.entries@[jj].purse == k.0
                            && self.entries@[jj].idx == k.1;
                        assert(self.entries@[w].purse == p);
                        assert(self.entries@[w] == self.entries()[k]);
                    }
                }
                None
            }
            Some(bi) => {
                let key = (self.entries[bi].purse, self.entries[bi].idx);
                proof {
                    assert(self.spec_entries@.dom().contains(key));
                    assert(self.entries()[key] == self.entries@[bi as int]);
                    assert forall|k: (PurseId, u64)|
                        #[trigger] self.entries().dom().contains(k)
                        && k.0 == p
                        && self.entries()[k].on_chain == EntryOnChain::Ready
                        && self.entries()[k].local == EntryLocal::LocalAvailable
                        && k != key
                        implies entry_priority_lt(self.entries()[key], self.entries()[k])
                            || self.entries()[key] == self.entries()[k]
                    by {
                        let w = choose|jj: int|
                            0 <= jj < self.entries@.len()
                            && #[trigger] self.entries@[jj].purse == k.0
                            && self.entries@[jj].idx == k.1;
                        assert(self.entries@[w] == self.entries()[k]);
                    }
                }
                Some(key)
            }
        }
    }

    /// Find any recycler entry in purse `p` that is `Ready` on-chain and
    /// `LocalAvailable` locally — i.e., selectable for unload or
    /// transfer-via-entry. Returns the first match in Vec order, or
    /// `None` if no such entry exists.
    ///
    /// Quint analog: a witness for `selectableEntriesIn(p, false)` —
    /// the strict (non-degraded) form of the §6.3 entry selectability
    /// predicate.
    pub fn find_entry_ready(&self, p: PurseId) -> (res: Option<(PurseId, u64)>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(key) =>
                    self.entries().dom().contains(key)
                    && key.0 == p
                    && self.entries()[key].on_chain == EntryOnChain::Ready
                    && self.entries()[key].local == EntryLocal::LocalAvailable,
                None =>
                    forall|k: (PurseId, u64)|
                        #[trigger] self.entries().dom().contains(k)
                        && k.0 == p
                        ==> self.entries()[k].on_chain != EntryOnChain::Ready
                            || self.entries()[k].local != EntryLocal::LocalAvailable,
            },
    {
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.entries@[jj]).purse != p
                    || self.entries@[jj].on_chain != EntryOnChain::Ready
                    || self.entries@[jj].local != EntryLocal::LocalAvailable,
            decreases self.entries.len() - j,
        {
            let e = &self.entries[j];
            let is_ready = matches!(e.on_chain, EntryOnChain::Ready);
            let is_local_avail = matches!(e.local, EntryLocal::LocalAvailable);
            if e.purse == p && is_ready && is_local_avail {
                let key = (e.purse, e.idx);
                proof {
                    assert(self.spec_entries@.dom().contains(key));
                }
                return Some(key);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                && k.0 == p
                implies self.entries()[k].on_chain != EntryOnChain::Ready
                    || self.entries()[k].local != EntryLocal::LocalAvailable
            by {
                let w = choose|jj: int|
                    0 <= jj < self.entries@.len()
                    && #[trigger] self.entries@[jj].purse == k.0
                    && self.entries@[jj].idx == k.1;
                assert(self.entries@[w].purse == p);
                assert(self.entries@[w].on_chain == self.entries()[k].on_chain);
                assert(self.entries@[w].local == self.entries()[k].local);
            }
        }
        None
    }

    /// Exec witness for [`classify_incoming_payment`]: scan the memo
    /// list, count how many recipients map to a known local coin via
    /// [`Self::find_coin_with_account`], and apply the §8.8
    /// classification rule.
    pub fn classify_incoming_payment_exec(&self, memos: &Vec<MemoEntry>)
        -> (res: PaymentClassification)
        requires
            self.invariant(),
            memos@.len() <= u64::MAX as nat,
        ensures
            res == classify_incoming_payment(memos@, self.coins()),
    {
        let n = memos.len();
        let mut matched: u64 = 0;
        let mut i: usize = 0;
        while i < n
            invariant
                0 <= i <= n,
                n == memos@.len(),
                n <= u64::MAX as nat,
                matched as nat <= i as nat,
                self.invariant(),
                matched as nat == count_matched_memos(memos@, self.coins(), i as nat),
            decreases n - i,
        {
            let m = memos[i];
            match self.find_coin_with_account(m.recipient_account) {
                Some(_) => {
                    matched = matched + 1;
                }
                None => {}
            }
            i = i + 1;
        }
        if n == 0 {
            PaymentClassification::Unmatched
        } else if matched == 0 {
            PaymentClassification::Unmatched
        } else if matched as usize == n {
            PaymentClassification::Matched
        } else {
            PaymentClassification::Received
        }
    }

    /// Find the highest-priority `Available` coin in purse `p`,
    /// breaking ties per the §6.3 coin priority order:
    /// `(MaxExp - exp, MaxAge - age, idx)` (lex-smallest wins).
    /// Returns `None` if `p` has no Available coins.
    pub fn find_top_priority_coin(&self, p: PurseId)
        -> (res: Option<(PurseId, u64)>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(key) =>
                    self.coins().dom().contains(key)
                    && key.0 == p
                    && self.coins()[key].state == CoinState::Available
                    && forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        && k.0 == p
                        && self.coins()[k].state == CoinState::Available
                        && k != key
                        ==> coin_priority_lt(self.coins()[key], self.coins()[k])
                            || self.coins()[key] == self.coins()[k],
                None =>
                    forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        && k.0 == p
                        ==> self.coins()[k].state != CoinState::Available,
            },
    {
        let mut best: Option<usize> = None;
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                match best {
                    Some(bi) =>
                        0 <= bi < j
                        && self.coins@[bi as int].purse == p
                        && self.coins@[bi as int].state == CoinState::Available
                        && forall|jj: int| 0 <= jj < j ==>
                            #[trigger] self.coins@[jj].purse != p
                            || self.coins@[jj].state != CoinState::Available
                            || coin_priority_lt(self.coins@[bi as int], self.coins@[jj])
                            || self.coins@[bi as int] == self.coins@[jj],
                    None =>
                        forall|jj: int| 0 <= jj < j ==>
                            (#[trigger] self.coins@[jj]).purse != p
                            || self.coins@[jj].state != CoinState::Available,
                },
            decreases self.coins.len() - j,
        {
            let is_avail = matches!(self.coins[j].state, CoinState::Available);
            if self.coins[j].purse == p && is_avail {
                match best {
                    None => { best = Some(j); }
                    Some(bi) => {
                        let cur = &self.coins[j];
                        let cur_better = self.coins[bi].exponent < cur.exponent
                            || (self.coins[bi].exponent == cur.exponent
                                && self.coins[bi].age > cur.age)
                            || (self.coins[bi].exponent == cur.exponent
                                && self.coins[bi].age == cur.age
                                && self.coins[bi].idx > cur.idx);
                        if cur_better {
                            best = Some(j);
                        }
                    }
                }
            }
            j = j + 1;
        }
        match best {
            None => {
                proof {
                    assert forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        && k.0 == p
                        implies self.coins()[k].state != CoinState::Available
                    by {
                        let w = choose|jj: int|
                            0 <= jj < self.coins@.len()
                            && #[trigger] self.coins@[jj].purse == k.0
                            && self.coins@[jj].idx == k.1;
                        assert(self.coins@[w].purse == p);
                        assert(self.coins@[w].state == self.coins()[k].state);
                    }
                }
                None
            }
            Some(bi) => {
                let key = (self.coins[bi].purse, self.coins[bi].idx);
                proof {
                    assert(self.spec_coins@.dom().contains(key));
                    assert(self.coins()[key] == self.coins@[bi as int]);
                    assert forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        && k.0 == p
                        && self.coins()[k].state == CoinState::Available
                        && k != key
                        implies coin_priority_lt(self.coins()[key], self.coins()[k])
                            || self.coins()[key] == self.coins()[k]
                    by {
                        let w = choose|jj: int|
                            0 <= jj < self.coins@.len()
                            && #[trigger] self.coins@[jj].purse == k.0
                            && self.coins@[jj].idx == k.1;
                        assert(self.coins@[w] == self.coins()[k]);
                    }
                }
                Some(key)
            }
        }
    }

    /// Find any coin (of any state) whose `account` matches `target`.
    /// Returns `(purse, idx)` of the first match in Vec order, or
    /// `None`. Used by `classify_incoming_payment` to test whether a
    /// memo's `recipient_account` is known locally.
    pub fn find_coin_with_account(&self, target: u64)
        -> (res: Option<(PurseId, u64)>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(key) =>
                    self.coins().dom().contains(key)
                    && self.coins()[key].account == target,
                None =>
                    forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        ==> self.coins()[k].account != target,
            },
    {
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).account != target,
            decreases self.coins.len() - j,
        {
            if self.coins[j].account == target {
                let key = (self.coins[j].purse, self.coins[j].idx);
                proof {
                    assert(self.spec_coins@.dom().contains(key));
                }
                return Some(key);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                implies self.coins()[k].account != target
            by {
                let w = choose|jj: int|
                    0 <= jj < self.coins@.len()
                    && #[trigger] self.coins@[jj].purse == k.0
                    && self.coins@[jj].idx == k.1;
                assert(self.coins@[w].account == self.coins()[k].account);
            }
        }
        None
    }

    /// Tier-3 (entry-supplemented cover, §6.3): find any pair of one
    /// `Available` coin and one `Ready + LocalAvailable` entry in
    /// purse `p` whose values sum exactly to `amount`.
    ///
    /// This is the simplest 1-coin + 1-entry case of the powerset-based
    /// existsUnloadCover. Full tier-3 with arbitrary coin and entry
    /// subsets remains task #88; this case unblocks the common
    /// "single coin not enough but one mature entry tips it over"
    /// pattern.
    pub fn find_coin_entry_exact_cover(&self, p: PurseId, amount: u64)
        -> (res: Option<((PurseId, u64), (PurseId, u64))>)
        requires
            self.invariant(),
        ensures
            match res {
                Some((coin_key, entry_key)) =>
                    self.coins().dom().contains(coin_key)
                    && self.entries().dom().contains(entry_key)
                    && coin_key.0 == p
                    && entry_key.0 == p
                    && self.coins()[coin_key].state == CoinState::Available
                    && self.entries()[entry_key].on_chain == EntryOnChain::Ready
                    && self.entries()[entry_key].local == EntryLocal::LocalAvailable
                    && coin_value(self.coins()[coin_key].exponent)
                        + coin_value(self.entries()[entry_key].exponent)
                        == amount as nat,
                None =>
                    // Sharp: no (coin, entry) pair satisfies the cover.
                    forall|i: int, k: int|
                        0 <= i < self.coins@.len()
                        && 0 <= k < self.entries@.len()
                        ==> {
                            let c = #[trigger] self.coins@[i];
                            let e = #[trigger] self.entries@[k];
                            c.purse != p
                            || c.state != CoinState::Available
                            || e.purse != p
                            || e.on_chain != EntryOnChain::Ready
                            || e.local != EntryLocal::LocalAvailable
                            || (coin_value(c.exponent) + coin_value(e.exponent)
                                != amount as nat)
                        },
            },
    {
        let nc = self.coins.len();
        let ne = self.entries.len();
        let mut i: usize = 0;
        while i < nc
            invariant
                0 <= i <= nc,
                nc == self.coins.len(),
                ne == self.entries.len(),
                self.invariant(),
                // Outer accumulator: no (coin, entry) pair with coin index < i.
                forall|i1: int, k: int|
                    0 <= i1 < i as int
                    && 0 <= k < ne as int
                    ==> {
                        let c = #[trigger] self.coins@[i1];
                        let e = #[trigger] self.entries@[k];
                        c.purse != p
                        || c.state != CoinState::Available
                        || e.purse != p
                        || e.on_chain != EntryOnChain::Ready
                        || e.local != EntryLocal::LocalAvailable
                        || (coin_value(c.exponent) + coin_value(e.exponent)
                            != amount as nat)
                    },
            decreases nc - i,
        {
            let ci_avail = matches!(self.coins[i].state, CoinState::Available);
            if self.coins[i].purse == p && ci_avail {
                proof {
                    let coin_key = (self.coins@[i as int].purse, self.coins@[i as int].idx);
                    assert(self.spec_coins@.dom().contains(coin_key));
                    assert(self.spec_coins@[coin_key] == self.coins@[i as int]);
                    assert(self.coins@[i as int].exponent <= MAX_EXPONENT);
                }
                let vi: u64 = pow2_u64_exec(self.coins[i].exponent);
                if vi <= amount {
                    let mut k: usize = 0;
                    while k < ne
                        invariant
                            0 <= k <= ne,
                            nc == self.coins.len(),
                            ne == self.entries.len(),
                            i < nc,
                            self.invariant(),
                            self.coins@[i as int].purse == p,
                            self.coins@[i as int].state == CoinState::Available,
                            vi as nat == coin_value(self.coins@[i as int].exponent),
                            vi <= 1073741824u64,
                            vi <= amount,
                            // Outer accumulator carried.
                            forall|i1: int, kk: int|
                                0 <= i1 < i as int
                                && 0 <= kk < ne as int
                                ==> {
                                    let c = #[trigger] self.coins@[i1];
                                    let e = #[trigger] self.entries@[kk];
                                    c.purse != p
                                    || c.state != CoinState::Available
                                    || e.purse != p
                                    || e.on_chain != EntryOnChain::Ready
                                    || e.local != EntryLocal::LocalAvailable
                                    || (coin_value(c.exponent) + coin_value(e.exponent)
                                        != amount as nat)
                                },
                            // Inner accumulator: for all checked k2 < k,
                            // the pair (i, k2) doesn't satisfy.
                            forall|k2: int|
                                0 <= k2 < k as int
                                ==>
                                (#[trigger] self.entries@[k2]).purse != p
                                || self.entries@[k2].on_chain != EntryOnChain::Ready
                                || self.entries@[k2].local != EntryLocal::LocalAvailable
                                || (coin_value(self.coins@[i as int].exponent)
                                        + coin_value(self.entries@[k2].exponent)
                                    != amount as nat),
                        decreases ne - k,
                    {
                        let e = &self.entries[k];
                        let is_ready = matches!(e.on_chain, EntryOnChain::Ready);
                        let is_local_avail = matches!(e.local, EntryLocal::LocalAvailable);
                        if e.purse == p && is_ready && is_local_avail {
                            proof {
                                let entry_key = (self.entries@[k as int].purse,
                                                 self.entries@[k as int].idx);
                                assert(self.spec_entries@.dom().contains(entry_key));
                                assert(self.spec_entries@[entry_key] == self.entries@[k as int]);
                                assert(self.entries@[k as int].exponent <= MAX_EXPONENT);
                            }
                            let ve: u64 = pow2_u64_exec(e.exponent);
                            if vi + ve == amount {
                                let ck = (self.coins[i].purse, self.coins[i].idx);
                                let ek = (self.entries[k].purse, self.entries[k].idx);
                                proof {
                                    assert(self.spec_coins@.dom().contains(ck));
                                    assert(self.spec_entries@.dom().contains(ek));
                                }
                                return Some((ck, ek));
                            }
                        }
                        k = k + 1;
                    }
                }
            }
            i = i + 1;
        }
        None
    }

    /// Tier-3 (entry-supplemented cover, §6.3, 2-coin + 1-entry): find
    /// any pair of distinct `Available` coins and one `Ready +
    /// LocalAvailable` entry in purse `p` whose values sum exactly
    /// to `amount`. Sharp `None` postcondition.
    pub fn find_two_coin_one_entry_cover(&self, p: PurseId, amount: u64)
        -> (res: Option<((PurseId, u64), (PurseId, u64), (PurseId, u64))>)
        requires
            self.invariant(),
        ensures
            match res {
                Some((c1, c2, e)) =>
                    self.coins().dom().contains(c1)
                    && self.coins().dom().contains(c2)
                    && self.entries().dom().contains(e)
                    && c1 != c2
                    && c1.0 == p && c2.0 == p && e.0 == p
                    && self.coins()[c1].state == CoinState::Available
                    && self.coins()[c2].state == CoinState::Available
                    && self.entries()[e].on_chain == EntryOnChain::Ready
                    && self.entries()[e].local == EntryLocal::LocalAvailable
                    && coin_value(self.coins()[c1].exponent)
                        + coin_value(self.coins()[c2].exponent)
                        + coin_value(self.entries()[e].exponent)
                        == amount as nat,
                None =>
                    forall|i1: int, i2: int, k: int|
                        0 <= i1 < self.coins@.len()
                        && 0 <= i2 < self.coins@.len()
                        && 0 <= k < self.entries@.len()
                        && i1 != i2
                        ==> {
                            let c1 = #[trigger] self.coins@[i1];
                            let c2 = #[trigger] self.coins@[i2];
                            let e = #[trigger] self.entries@[k];
                            c1.purse != p
                            || c1.state != CoinState::Available
                            || c2.purse != p
                            || c2.state != CoinState::Available
                            || e.purse != p
                            || e.on_chain != EntryOnChain::Ready
                            || e.local != EntryLocal::LocalAvailable
                            || (coin_value(c1.exponent)
                                    + coin_value(c2.exponent)
                                    + coin_value(e.exponent)
                                != amount as nat)
                        },
            },
    {
        let nc = self.coins.len();
        let ne = self.entries.len();
        let mut i: usize = 0;
        while i < nc
            invariant
                0 <= i <= nc,
                nc == self.coins.len(),
                ne == self.entries.len(),
                self.invariant(),
                // Outer accumulator: no (i1, i2, k) with i1 < i works.
                forall|i1: int, i2: int, k: int|
                    0 <= i1 < i as int
                    && 0 <= i2 < nc as int
                    && 0 <= k < ne as int
                    && i1 != i2
                    ==> {
                        let c1 = #[trigger] self.coins@[i1];
                        let c2 = #[trigger] self.coins@[i2];
                        let e = #[trigger] self.entries@[k];
                        c1.purse != p
                        || c1.state != CoinState::Available
                        || c2.purse != p
                        || c2.state != CoinState::Available
                        || e.purse != p
                        || e.on_chain != EntryOnChain::Ready
                        || e.local != EntryLocal::LocalAvailable
                        || (coin_value(c1.exponent)
                                + coin_value(c2.exponent)
                                + coin_value(e.exponent)
                            != amount as nat)
                    },
            decreases nc - i,
        {
            let ci_avail = matches!(self.coins[i].state, CoinState::Available);
            if self.coins[i].purse == p && ci_avail {
                proof {
                    let coin_key = (self.coins@[i as int].purse, self.coins@[i as int].idx);
                    assert(self.spec_coins@.dom().contains(coin_key));
                    assert(self.spec_coins@[coin_key] == self.coins@[i as int]);
                    assert(self.coins@[i as int].exponent <= MAX_EXPONENT);
                }
                let vi: u64 = pow2_u64_exec(self.coins[i].exponent);
                if vi <= amount {
                    let mut j: usize = 0;
                    while j < nc
                        invariant
                            0 <= j <= nc,
                            nc == self.coins.len(),
                            ne == self.entries.len(),
                            i < nc,
                            self.invariant(),
                            self.coins@[i as int].purse == p,
                            self.coins@[i as int].state == CoinState::Available,
                            vi as nat == coin_value(self.coins@[i as int].exponent),
                            vi <= 1073741824u64,
                            vi <= amount,
                            forall|i1: int, i2: int, k: int|
                                0 <= i1 < i as int
                                && 0 <= i2 < nc as int
                                && 0 <= k < ne as int
                                && i1 != i2
                                ==> {
                                    let c1 = #[trigger] self.coins@[i1];
                                    let c2 = #[trigger] self.coins@[i2];
                                    let e = #[trigger] self.entries@[k];
                                    c1.purse != p
                                    || c1.state != CoinState::Available
                                    || c2.purse != p
                                    || c2.state != CoinState::Available
                                    || e.purse != p
                                    || e.on_chain != EntryOnChain::Ready
                                    || e.local != EntryLocal::LocalAvailable
                                    || (coin_value(c1.exponent)
                                            + coin_value(c2.exponent)
                                            + coin_value(e.exponent)
                                        != amount as nat)
                                },
                            // Middle accumulator: forall (i, j1, k) with j1 < j, j1 != i.
                            forall|j1: int, k: int|
                                0 <= j1 < j as int
                                && 0 <= k < ne as int
                                && j1 != i as int
                                ==> {
                                    let c2 = #[trigger] self.coins@[j1];
                                    let e = #[trigger] self.entries@[k];
                                    c2.purse != p
                                    || c2.state != CoinState::Available
                                    || e.purse != p
                                    || e.on_chain != EntryOnChain::Ready
                                    || e.local != EntryLocal::LocalAvailable
                                    || (coin_value(self.coins@[i as int].exponent)
                                            + coin_value(c2.exponent)
                                            + coin_value(e.exponent)
                                        != amount as nat)
                                },
                        decreases nc - j,
                    {
                        if j != i {
                            let cj_avail = matches!(self.coins[j].state, CoinState::Available);
                            if self.coins[j].purse == p && cj_avail {
                                proof {
                                    let coin_key = (self.coins@[j as int].purse,
                                                    self.coins@[j as int].idx);
                                    assert(self.spec_coins@.dom().contains(coin_key));
                                    assert(self.spec_coins@[coin_key] == self.coins@[j as int]);
                                    assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
                                }
                                let vj: u64 = pow2_u64_exec(self.coins[j].exponent);
                                if vi + vj <= amount {
                                    let mut k: usize = 0;
                                    while k < ne
                                        invariant
                                            0 <= k <= ne,
                                            nc == self.coins.len(),
                                            ne == self.entries.len(),
                                            i < nc,
                                            j < nc,
                                            i != j as usize,
                                            self.invariant(),
                                            self.coins@[i as int].purse == p,
                                            self.coins@[i as int].state == CoinState::Available,
                                            self.coins@[j as int].purse == p,
                                            self.coins@[j as int].state == CoinState::Available,
                                            vi as nat == coin_value(self.coins@[i as int].exponent),
                                            vj as nat == coin_value(self.coins@[j as int].exponent),
                                            vi <= 1073741824u64,
                                            vj <= 1073741824u64,
                                            vi + vj <= amount,
                                            // Inner accumulator: forall k2 < k checked, triple fails.
                                            forall|k2: int|
                                                0 <= k2 < k as int
                                                ==>
                                                (#[trigger] self.entries@[k2]).purse != p
                                                || self.entries@[k2].on_chain != EntryOnChain::Ready
                                                || self.entries@[k2].local != EntryLocal::LocalAvailable
                                                || (coin_value(self.coins@[i as int].exponent)
                                                        + coin_value(self.coins@[j as int].exponent)
                                                        + coin_value(self.entries@[k2].exponent)
                                                    != amount as nat),
                                        decreases ne - k,
                                    {
                                        let e = &self.entries[k];
                                        let is_ready = matches!(e.on_chain, EntryOnChain::Ready);
                                        let is_local_avail = matches!(e.local,
                                                                      EntryLocal::LocalAvailable);
                                        if e.purse == p && is_ready && is_local_avail {
                                            proof {
                                                let entry_key = (self.entries@[k as int].purse,
                                                                 self.entries@[k as int].idx);
                                                assert(self.spec_entries@.dom().contains(entry_key));
                                                assert(self.spec_entries@[entry_key]
                                                    == self.entries@[k as int]);
                                                assert(self.entries@[k as int].exponent
                                                    <= MAX_EXPONENT);
                                            }
                                            let ve: u64 = pow2_u64_exec(e.exponent);
                                            if vi + vj + ve == amount {
                                                let ck1 = (self.coins[i].purse, self.coins[i].idx);
                                                let ck2 = (self.coins[j].purse, self.coins[j].idx);
                                                let ek = (self.entries[k].purse, self.entries[k].idx);
                                                proof {
                                                    assert(self.spec_coins@.dom().contains(ck1));
                                                    assert(self.spec_coins@.dom().contains(ck2));
                                                    assert(self.spec_entries@.dom().contains(ek));
                                                    assert(ck1 != ck2);
                                                }
                                                return Some((ck1, ck2, ek));
                                            }
                                        }
                                        k = k + 1;
                                    }
                                }
                            }
                        }
                        j = j + 1;
                    }
                }
            }
            i = i + 1;
        }
        None
    }

    /// Tier-3 (entry-supplemented cover, §6.3, 1-coin + 2-entry): find
    /// any single `Available` coin and a pair of distinct `Ready +
    /// LocalAvailable` entries in purse `p` whose values sum exactly
    /// to `amount`. Sharp `None` postcondition.
    pub fn find_one_coin_two_entry_cover(&self, p: PurseId, amount: u64)
        -> (res: Option<((PurseId, u64), (PurseId, u64), (PurseId, u64))>)
        requires
            self.invariant(),
        ensures
            match res {
                Some((c, e1, e2)) =>
                    self.coins().dom().contains(c)
                    && self.entries().dom().contains(e1)
                    && self.entries().dom().contains(e2)
                    && e1 != e2
                    && c.0 == p && e1.0 == p && e2.0 == p
                    && self.coins()[c].state == CoinState::Available
                    && self.entries()[e1].on_chain == EntryOnChain::Ready
                    && self.entries()[e1].local == EntryLocal::LocalAvailable
                    && self.entries()[e2].on_chain == EntryOnChain::Ready
                    && self.entries()[e2].local == EntryLocal::LocalAvailable
                    && coin_value(self.coins()[c].exponent)
                        + coin_value(self.entries()[e1].exponent)
                        + coin_value(self.entries()[e2].exponent)
                        == amount as nat,
                None =>
                    forall|i: int, k1: int, k2: int|
                        0 <= i < self.coins@.len()
                        && 0 <= k1 < self.entries@.len()
                        && 0 <= k2 < self.entries@.len()
                        && k1 != k2
                        ==> {
                            let c = #[trigger] self.coins@[i];
                            let e1 = #[trigger] self.entries@[k1];
                            let e2 = #[trigger] self.entries@[k2];
                            c.purse != p
                            || c.state != CoinState::Available
                            || e1.purse != p
                            || e1.on_chain != EntryOnChain::Ready
                            || e1.local != EntryLocal::LocalAvailable
                            || e2.purse != p
                            || e2.on_chain != EntryOnChain::Ready
                            || e2.local != EntryLocal::LocalAvailable
                            || (coin_value(c.exponent)
                                    + coin_value(e1.exponent)
                                    + coin_value(e2.exponent)
                                != amount as nat)
                        },
            },
    {
        let nc = self.coins.len();
        let ne = self.entries.len();
        let mut i: usize = 0;
        while i < nc
            invariant
                0 <= i <= nc,
                nc == self.coins.len(),
                ne == self.entries.len(),
                self.invariant(),
                forall|i1: int, k1: int, k2: int|
                    0 <= i1 < i as int
                    && 0 <= k1 < ne as int
                    && 0 <= k2 < ne as int
                    && k1 != k2
                    ==> {
                        let c = #[trigger] self.coins@[i1];
                        let e1 = #[trigger] self.entries@[k1];
                        let e2 = #[trigger] self.entries@[k2];
                        c.purse != p
                        || c.state != CoinState::Available
                        || e1.purse != p
                        || e1.on_chain != EntryOnChain::Ready
                        || e1.local != EntryLocal::LocalAvailable
                        || e2.purse != p
                        || e2.on_chain != EntryOnChain::Ready
                        || e2.local != EntryLocal::LocalAvailable
                        || (coin_value(c.exponent)
                                + coin_value(e1.exponent)
                                + coin_value(e2.exponent)
                            != amount as nat)
                    },
            decreases nc - i,
        {
            let ci_avail = matches!(self.coins[i].state, CoinState::Available);
            if self.coins[i].purse == p && ci_avail {
                proof {
                    let coin_key = (self.coins@[i as int].purse, self.coins@[i as int].idx);
                    assert(self.spec_coins@.dom().contains(coin_key));
                    assert(self.spec_coins@[coin_key] == self.coins@[i as int]);
                    assert(self.coins@[i as int].exponent <= MAX_EXPONENT);
                }
                let vi: u64 = pow2_u64_exec(self.coins[i].exponent);
                if vi <= amount {
                    let mut j: usize = 0;
                    while j < ne
                        invariant
                            0 <= j <= ne,
                            nc == self.coins.len(),
                            ne == self.entries.len(),
                            i < nc,
                            self.invariant(),
                            self.coins@[i as int].purse == p,
                            self.coins@[i as int].state == CoinState::Available,
                            vi as nat == coin_value(self.coins@[i as int].exponent),
                            vi <= 1073741824u64,
                            vi <= amount,
                            forall|i1: int, k1: int, k2: int|
                                0 <= i1 < i as int
                                && 0 <= k1 < ne as int
                                && 0 <= k2 < ne as int
                                && k1 != k2
                                ==> {
                                    let c = #[trigger] self.coins@[i1];
                                    let e1 = #[trigger] self.entries@[k1];
                                    let e2 = #[trigger] self.entries@[k2];
                                    c.purse != p
                                    || c.state != CoinState::Available
                                    || e1.purse != p
                                    || e1.on_chain != EntryOnChain::Ready
                                    || e1.local != EntryLocal::LocalAvailable
                                    || e2.purse != p
                                    || e2.on_chain != EntryOnChain::Ready
                                    || e2.local != EntryLocal::LocalAvailable
                                    || (coin_value(c.exponent)
                                            + coin_value(e1.exponent)
                                            + coin_value(e2.exponent)
                                        != amount as nat)
                                },
                            forall|j1: int, k2: int|
                                0 <= j1 < j as int
                                && 0 <= k2 < ne as int
                                && j1 != k2
                                ==> {
                                    let e1 = #[trigger] self.entries@[j1];
                                    let e2 = #[trigger] self.entries@[k2];
                                    e1.purse != p
                                    || e1.on_chain != EntryOnChain::Ready
                                    || e1.local != EntryLocal::LocalAvailable
                                    || e2.purse != p
                                    || e2.on_chain != EntryOnChain::Ready
                                    || e2.local != EntryLocal::LocalAvailable
                                    || (coin_value(self.coins@[i as int].exponent)
                                            + coin_value(e1.exponent)
                                            + coin_value(e2.exponent)
                                        != amount as nat)
                                },
                        decreases ne - j,
                    {
                        let e1 = &self.entries[j];
                        let is_ready1 = matches!(e1.on_chain, EntryOnChain::Ready);
                        let is_local_avail1 = matches!(e1.local, EntryLocal::LocalAvailable);
                        if e1.purse == p && is_ready1 && is_local_avail1 {
                            proof {
                                let entry_key = (self.entries@[j as int].purse,
                                                 self.entries@[j as int].idx);
                                assert(self.spec_entries@.dom().contains(entry_key));
                                assert(self.spec_entries@[entry_key]
                                    == self.entries@[j as int]);
                                assert(self.entries@[j as int].exponent <= MAX_EXPONENT);
                            }
                            let ve1: u64 = pow2_u64_exec(e1.exponent);
                            if vi + ve1 <= amount {
                                let mut k: usize = 0;
                                while k < ne
                                    invariant
                                        0 <= k <= ne,
                                        nc == self.coins.len(),
                                        ne == self.entries.len(),
                                        i < nc,
                                        j < ne,
                                        self.invariant(),
                                        self.coins@[i as int].purse == p,
                                        self.coins@[i as int].state == CoinState::Available,
                                        self.entries@[j as int].purse == p,
                                        self.entries@[j as int].on_chain == EntryOnChain::Ready,
                                        self.entries@[j as int].local == EntryLocal::LocalAvailable,
                                        vi as nat == coin_value(self.coins@[i as int].exponent),
                                        ve1 as nat == coin_value(self.entries@[j as int].exponent),
                                        vi <= 1073741824u64,
                                        ve1 <= 1073741824u64,
                                        vi + ve1 <= amount,
                                        forall|k2: int|
                                            0 <= k2 < k as int
                                            && k2 != j as int
                                            ==>
                                            (#[trigger] self.entries@[k2]).purse != p
                                            || self.entries@[k2].on_chain != EntryOnChain::Ready
                                            || self.entries@[k2].local != EntryLocal::LocalAvailable
                                            || (coin_value(self.coins@[i as int].exponent)
                                                    + coin_value(self.entries@[j as int].exponent)
                                                    + coin_value(self.entries@[k2].exponent)
                                                != amount as nat),
                                    decreases ne - k,
                                {
                                    if k != j {
                                        let e2 = &self.entries[k];
                                        let is_ready2 = matches!(e2.on_chain, EntryOnChain::Ready);
                                        let is_local_avail2 = matches!(e2.local,
                                                                       EntryLocal::LocalAvailable);
                                        if e2.purse == p && is_ready2 && is_local_avail2 {
                                            proof {
                                                let entry_key = (self.entries@[k as int].purse,
                                                                 self.entries@[k as int].idx);
                                                assert(self.spec_entries@.dom().contains(entry_key));
                                                assert(self.spec_entries@[entry_key]
                                                    == self.entries@[k as int]);
                                                assert(self.entries@[k as int].exponent
                                                    <= MAX_EXPONENT);
                                            }
                                            let ve2: u64 = pow2_u64_exec(e2.exponent);
                                            if vi + ve1 + ve2 == amount {
                                                let ck = (self.coins[i].purse, self.coins[i].idx);
                                                let ek1 = (self.entries[j].purse,
                                                           self.entries[j].idx);
                                                let ek2 = (self.entries[k].purse,
                                                           self.entries[k].idx);
                                                proof {
                                                    assert(self.spec_coins@.dom().contains(ck));
                                                    assert(self.spec_entries@.dom().contains(ek1));
                                                    assert(self.spec_entries@.dom().contains(ek2));
                                                    assert(ek1 != ek2);
                                                }
                                                return Some((ck, ek1, ek2));
                                            }
                                        }
                                    }
                                    k = k + 1;
                                }
                            }
                        }
                        j = j + 1;
                    }
                }
            }
            i = i + 1;
        }
        None
    }

    /// Tier-1 multi-coin (§6.3): find any pair of distinct `Available`
    /// coins in purse `p` whose values sum exactly to `amount`. Returns
    /// the two keys in Vec order, or `None` if no such pair exists.
    ///
    /// This is the 2-coin special case of the powerset-based
    /// selectExactCoverDeterministic. Full powerset enumeration remains
    /// open (task #87); 2-coin already covers many cases that
    /// single-coin tier-1 misses (e.g. requesting amount = max_exp + 2
    /// with two coins of value max_exp + 1 / 1).
    pub fn find_two_coin_exact_cover(&self, p: PurseId, amount: u64)
        -> (res: Option<((PurseId, u64), (PurseId, u64))>)
        requires
            self.invariant(),
        ensures
            match res {
                Some((k1, k2)) =>
                    self.coins().dom().contains(k1)
                    && self.coins().dom().contains(k2)
                    && k1 != k2
                    && k1.0 == p
                    && k2.0 == p
                    && self.coins()[k1].state == CoinState::Available
                    && self.coins()[k2].state == CoinState::Available
                    && coin_value(self.coins()[k1].exponent)
                        + coin_value(self.coins()[k2].exponent)
                        == amount as nat,
                None =>
                    // Sharp: no two distinct Vec indices satisfy the pair-sum
                    // predicate. Combined with the dedup invariant (n), this
                    // is equivalent to "no two distinct coin keys with the
                    // pair-sum predicate".
                    forall|i1: int, i2: int|
                        0 <= i1 < self.coins@.len()
                        && 0 <= i2 < self.coins@.len()
                        && i1 != i2
                        ==> {
                            let c1 = #[trigger] self.coins@[i1];
                            let c2 = #[trigger] self.coins@[i2];
                            c1.purse != p
                            || c1.state != CoinState::Available
                            || c2.purse != p
                            || c2.state != CoinState::Available
                            || (coin_value(c1.exponent) + coin_value(c2.exponent)
                                != amount as nat)
                        },
            },
    {
        let n = self.coins.len();
        let mut i: usize = 0;
        while i < n
            invariant
                0 <= i <= n,
                n == self.coins.len(),
                self.invariant(),
                // No earlier outer index i1 < i forms a valid pair with any k.
                forall|i1: int, i2: int|
                    0 <= i1 < i as int && 0 <= i2 < n as int && i1 != i2 ==> {
                        let c1 = #[trigger] self.coins@[i1];
                        let c2 = #[trigger] self.coins@[i2];
                        c1.purse != p
                        || c1.state != CoinState::Available
                        || c2.purse != p
                        || c2.state != CoinState::Available
                        || (coin_value(c1.exponent) + coin_value(c2.exponent)
                            != amount as nat)
                    },
            decreases n - i,
        {
            let ci_avail = matches!(self.coins[i].state, CoinState::Available);
            if self.coins[i].purse == p && ci_avail {
                proof {
                    let coin_key = (self.coins@[i as int].purse, self.coins@[i as int].idx);
                    assert(self.spec_coins@.dom().contains(coin_key));
                    assert(self.spec_coins@[coin_key] == self.coins@[i as int]);
                    assert(self.coins@[i as int].exponent <= MAX_EXPONENT);
                }
                let vi: u64 = pow2_u64_exec(self.coins[i].exponent);
                if vi <= amount {
                    let mut k: usize = 0;
                    while k < n
                        invariant
                            0 <= k <= n,
                            n == self.coins.len(),
                            i < n,
                            self.invariant(),
                            self.coins@[i as int].purse == p,
                            self.coins@[i as int].state == CoinState::Available,
                            vi as nat == coin_value(self.coins@[i as int].exponent),
                            vi <= 1073741824u64,
                            vi <= amount,
                            // Same outer accumulator from before this inner loop.
                            forall|i1: int, i2: int|
                                0 <= i1 < i as int && 0 <= i2 < n as int
                                && i1 != i2 ==> {
                                    let c1 = #[trigger] self.coins@[i1];
                                    let c2 = #[trigger] self.coins@[i2];
                                    c1.purse != p
                                    || c1.state != CoinState::Available
                                    || c2.purse != p
                                    || c2.state != CoinState::Available
                                    || (coin_value(c1.exponent) + coin_value(c2.exponent)
                                        != amount as nat)
                                },
                            // Inner-loop accumulator: for all checked k2 < k,
                            // the pair (i, k2) doesn't satisfy the predicate.
                            forall|i2: int|
                                0 <= i2 < k as int && i2 != i as int ==>
                                (#[trigger] self.coins@[i2]).purse != p
                                || self.coins@[i2].state != CoinState::Available
                                || (coin_value(self.coins@[i as int].exponent)
                                        + coin_value(self.coins@[i2].exponent)
                                    != amount as nat),
                        decreases n - k,
                    {
                        if k != i {
                            let ck_avail = matches!(self.coins[k].state, CoinState::Available);
                            proof {
                                let coin_key = (self.coins@[k as int].purse,
                                                self.coins@[k as int].idx);
                                assert(self.spec_coins@.dom().contains(coin_key));
                                assert(self.spec_coins@[coin_key] == self.coins@[k as int]);
                                assert(self.coins@[k as int].exponent <= MAX_EXPONENT);
                            }
                            let vk: u64 = pow2_u64_exec(self.coins[k].exponent);
                            if self.coins[k].purse == p && ck_avail && vi + vk == amount {
                                let k1 = (self.coins[i].purse, self.coins[i].idx);
                                let k2 = (self.coins[k].purse, self.coins[k].idx);
                                proof {
                                    assert(self.spec_coins@.dom().contains(k1));
                                    assert(self.spec_coins@.dom().contains(k2));
                                    assert(k1 != k2);
                                }
                                return Some((k1, k2));
                            }
                        }
                        k = k + 1;
                    }
                }
                // If vi > amount, the pair-sum is also > amount and can't equal.
                // The outer-loop accumulator extends by this fact for i.
            }
            i = i + 1;
        }
        None
    }

    /// Tier-1 multi-coin (§6.3, 3-coin extension): find any triple of
    /// distinct `Available` coins in purse `p` whose values sum exactly
    /// to `amount`. Returns the three keys in Vec order, or `None` if
    /// no such triple exists.
    ///
    /// One step closer to full powerset (task #87): handles 3-coin
    /// subsets with sharp None. Full N-coin (bitmask enumeration over
    /// the first K Available coins) is still open.
    pub fn find_three_coin_exact_cover(&self, p: PurseId, amount: u64)
        -> (res: Option<((PurseId, u64), (PurseId, u64), (PurseId, u64))>)
        requires
            self.invariant(),
        ensures
            match res {
                Some((k1, k2, k3)) =>
                    self.coins().dom().contains(k1)
                    && self.coins().dom().contains(k2)
                    && self.coins().dom().contains(k3)
                    && k1 != k2 && k1 != k3 && k2 != k3
                    && k1.0 == p && k2.0 == p && k3.0 == p
                    && self.coins()[k1].state == CoinState::Available
                    && self.coins()[k2].state == CoinState::Available
                    && self.coins()[k3].state == CoinState::Available
                    && coin_value(self.coins()[k1].exponent)
                        + coin_value(self.coins()[k2].exponent)
                        + coin_value(self.coins()[k3].exponent)
                        == amount as nat,
                None =>
                    // Sharp: no three pairwise-distinct Vec indices form
                    // a triple summing to amount.
                    forall|i1: int, i2: int, i3: int|
                        0 <= i1 < self.coins@.len()
                        && 0 <= i2 < self.coins@.len()
                        && 0 <= i3 < self.coins@.len()
                        && i1 != i2 && i1 != i3 && i2 != i3
                        ==> {
                            let c1 = #[trigger] self.coins@[i1];
                            let c2 = #[trigger] self.coins@[i2];
                            let c3 = #[trigger] self.coins@[i3];
                            c1.purse != p
                            || c1.state != CoinState::Available
                            || c2.purse != p
                            || c2.state != CoinState::Available
                            || c3.purse != p
                            || c3.state != CoinState::Available
                            || (coin_value(c1.exponent)
                                    + coin_value(c2.exponent)
                                    + coin_value(c3.exponent)
                                != amount as nat)
                        },
            },
    {
        let n = self.coins.len();
        let mut i: usize = 0;
        while i < n
            invariant
                0 <= i <= n,
                n == self.coins.len(),
                self.invariant(),
                // Outer accumulator: no triple with first index < i works.
                forall|i1: int, i2: int, i3: int|
                    0 <= i1 < i as int
                    && 0 <= i2 < n as int
                    && 0 <= i3 < n as int
                    && i1 != i2 && i1 != i3 && i2 != i3
                    ==> {
                        let c1 = #[trigger] self.coins@[i1];
                        let c2 = #[trigger] self.coins@[i2];
                        let c3 = #[trigger] self.coins@[i3];
                        c1.purse != p
                        || c1.state != CoinState::Available
                        || c2.purse != p
                        || c2.state != CoinState::Available
                        || c3.purse != p
                        || c3.state != CoinState::Available
                        || (coin_value(c1.exponent)
                                + coin_value(c2.exponent)
                                + coin_value(c3.exponent)
                            != amount as nat)
                    },
            decreases n - i,
        {
            let ci_avail = matches!(self.coins[i].state, CoinState::Available);
            if self.coins[i].purse == p && ci_avail {
                proof {
                    let coin_key = (self.coins@[i as int].purse, self.coins@[i as int].idx);
                    assert(self.spec_coins@.dom().contains(coin_key));
                    assert(self.spec_coins@[coin_key] == self.coins@[i as int]);
                    assert(self.coins@[i as int].exponent <= MAX_EXPONENT);
                }
                let vi: u64 = pow2_u64_exec(self.coins[i].exponent);
                if vi <= amount {
                    let mut j: usize = 0;
                    while j < n
                        invariant
                            0 <= j <= n,
                            n == self.coins.len(),
                            i < n,
                            self.invariant(),
                            self.coins@[i as int].purse == p,
                            self.coins@[i as int].state == CoinState::Available,
                            vi as nat == coin_value(self.coins@[i as int].exponent),
                            vi <= 1073741824u64,
                            vi <= amount,
                            // Outer accumulator carried.
                            forall|i1: int, i2: int, i3: int|
                                0 <= i1 < i as int
                                && 0 <= i2 < n as int
                                && 0 <= i3 < n as int
                                && i1 != i2 && i1 != i3 && i2 != i3
                                ==> {
                                    let c1 = #[trigger] self.coins@[i1];
                                    let c2 = #[trigger] self.coins@[i2];
                                    let c3 = #[trigger] self.coins@[i3];
                                    c1.purse != p
                                    || c1.state != CoinState::Available
                                    || c2.purse != p
                                    || c2.state != CoinState::Available
                                    || c3.purse != p
                                    || c3.state != CoinState::Available
                                    || (coin_value(c1.exponent)
                                            + coin_value(c2.exponent)
                                            + coin_value(c3.exponent)
                                        != amount as nat)
                                },
                            // Middle accumulator: forall (i, j1, j3) with j1 < j, distinct.
                            forall|j1: int, j3: int|
                                0 <= j1 < j as int
                                && 0 <= j3 < n as int
                                && j1 != i as int && j3 != i as int && j1 != j3
                                ==> {
                                    let c2 = #[trigger] self.coins@[j1];
                                    let c3 = #[trigger] self.coins@[j3];
                                    c2.purse != p
                                    || c2.state != CoinState::Available
                                    || c3.purse != p
                                    || c3.state != CoinState::Available
                                    || (coin_value(self.coins@[i as int].exponent)
                                            + coin_value(c2.exponent)
                                            + coin_value(c3.exponent)
                                        != amount as nat)
                                },
                        decreases n - j,
                    {
                        if j != i {
                            let cj_avail = matches!(self.coins[j].state, CoinState::Available);
                            if self.coins[j].purse == p && cj_avail {
                                proof {
                                    let coin_key = (self.coins@[j as int].purse,
                                                    self.coins@[j as int].idx);
                                    assert(self.spec_coins@.dom().contains(coin_key));
                                    assert(self.spec_coins@[coin_key] == self.coins@[j as int]);
                                    assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
                                }
                                let vj: u64 = pow2_u64_exec(self.coins[j].exponent);
                                if vi + vj <= amount {
                                    let mut k: usize = 0;
                                    while k < n
                                        invariant
                                            0 <= k <= n,
                                            n == self.coins.len(),
                                            i < n,
                                            j < n,
                                            i != j as usize,
                                            self.invariant(),
                                            self.coins@[i as int].purse == p,
                                            self.coins@[i as int].state == CoinState::Available,
                                            self.coins@[j as int].purse == p,
                                            self.coins@[j as int].state == CoinState::Available,
                                            vi as nat == coin_value(self.coins@[i as int].exponent),
                                            vj as nat == coin_value(self.coins@[j as int].exponent),
                                            vi <= 1073741824u64,
                                            vj <= 1073741824u64,
                                            vi + vj <= amount,
                                            // Inner accumulator: forall k2 < k checked, triple fails.
                                            forall|k2: int|
                                                0 <= k2 < k as int
                                                && k2 != i as int && k2 != j as int
                                                ==>
                                                (#[trigger] self.coins@[k2]).purse != p
                                                || self.coins@[k2].state != CoinState::Available
                                                || (coin_value(self.coins@[i as int].exponent)
                                                        + coin_value(self.coins@[j as int].exponent)
                                                        + coin_value(self.coins@[k2].exponent)
                                                    != amount as nat),
                                        decreases n - k,
                                    {
                                        if k != i && k != j {
                                            let ck_avail = matches!(self.coins[k].state,
                                                                    CoinState::Available);
                                            if self.coins[k].purse == p && ck_avail {
                                                proof {
                                                    let coin_key = (self.coins@[k as int].purse,
                                                                    self.coins@[k as int].idx);
                                                    assert(self.spec_coins@.dom().contains(coin_key));
                                                    assert(self.spec_coins@[coin_key]
                                                        == self.coins@[k as int]);
                                                    assert(self.coins@[k as int].exponent
                                                        <= MAX_EXPONENT);
                                                }
                                                let vk: u64 = pow2_u64_exec(self.coins[k].exponent);
                                                if vi + vj + vk == amount {
                                                    let k1 = (self.coins[i].purse,
                                                              self.coins[i].idx);
                                                    let k2 = (self.coins[j].purse,
                                                              self.coins[j].idx);
                                                    let k3 = (self.coins[k].purse,
                                                              self.coins[k].idx);
                                                    proof {
                                                        assert(self.spec_coins@.dom().contains(k1));
                                                        assert(self.spec_coins@.dom().contains(k2));
                                                        assert(self.spec_coins@.dom().contains(k3));
                                                        assert(k1 != k2);
                                                        assert(k1 != k3);
                                                        assert(k2 != k3);
                                                    }
                                                    return Some((k1, k2, k3));
                                                }
                                            }
                                        }
                                        k = k + 1;
                                    }
                                }
                            }
                        }
                        j = j + 1;
                    }
                }
            }
            i = i + 1;
        }
        None
    }

    /// Tier-1 multi-coin (§6.3, 4-coin extension): find any quadruple of
    /// pairwise-distinct `Available` coins in purse `p` whose values sum
    /// exactly to `amount`. Sharp `None` postcondition.
    ///
    /// Same structural shape as `find_three_coin_exact_cover`, one more
    /// dimension. Continues partial closure of task #87.
    pub fn find_four_coin_exact_cover(&self, p: PurseId, amount: u64)
        -> (res: Option<((PurseId, u64), (PurseId, u64), (PurseId, u64), (PurseId, u64))>)
        requires
            self.invariant(),
        ensures
            match res {
                Some((k1, k2, k3, k4)) =>
                    self.coins().dom().contains(k1)
                    && self.coins().dom().contains(k2)
                    && self.coins().dom().contains(k3)
                    && self.coins().dom().contains(k4)
                    && k1 != k2 && k1 != k3 && k1 != k4
                    && k2 != k3 && k2 != k4 && k3 != k4
                    && k1.0 == p && k2.0 == p && k3.0 == p && k4.0 == p
                    && self.coins()[k1].state == CoinState::Available
                    && self.coins()[k2].state == CoinState::Available
                    && self.coins()[k3].state == CoinState::Available
                    && self.coins()[k4].state == CoinState::Available
                    && coin_value(self.coins()[k1].exponent)
                        + coin_value(self.coins()[k2].exponent)
                        + coin_value(self.coins()[k3].exponent)
                        + coin_value(self.coins()[k4].exponent)
                        == amount as nat,
                None =>
                    // Sharp: no four pairwise-distinct Vec indices form a
                    // quadruple summing to amount.
                    forall|i1: int, i2: int, i3: int, i4: int|
                        0 <= i1 < self.coins@.len()
                        && 0 <= i2 < self.coins@.len()
                        && 0 <= i3 < self.coins@.len()
                        && 0 <= i4 < self.coins@.len()
                        && i1 != i2 && i1 != i3 && i1 != i4
                        && i2 != i3 && i2 != i4 && i3 != i4
                        ==> {
                            let c1 = #[trigger] self.coins@[i1];
                            let c2 = #[trigger] self.coins@[i2];
                            let c3 = #[trigger] self.coins@[i3];
                            let c4 = #[trigger] self.coins@[i4];
                            c1.purse != p
                            || c1.state != CoinState::Available
                            || c2.purse != p
                            || c2.state != CoinState::Available
                            || c3.purse != p
                            || c3.state != CoinState::Available
                            || c4.purse != p
                            || c4.state != CoinState::Available
                            || (coin_value(c1.exponent)
                                    + coin_value(c2.exponent)
                                    + coin_value(c3.exponent)
                                    + coin_value(c4.exponent)
                                != amount as nat)
                        },
            },
    {
        let n = self.coins.len();
        let mut i: usize = 0;
        while i < n
            invariant
                0 <= i <= n,
                n == self.coins.len(),
                self.invariant(),
                forall|i1: int, i2: int, i3: int, i4: int|
                    0 <= i1 < i as int
                    && 0 <= i2 < n as int
                    && 0 <= i3 < n as int
                    && 0 <= i4 < n as int
                    && i1 != i2 && i1 != i3 && i1 != i4
                    && i2 != i3 && i2 != i4 && i3 != i4
                    ==> {
                        let c1 = #[trigger] self.coins@[i1];
                        let c2 = #[trigger] self.coins@[i2];
                        let c3 = #[trigger] self.coins@[i3];
                        let c4 = #[trigger] self.coins@[i4];
                        c1.purse != p
                        || c1.state != CoinState::Available
                        || c2.purse != p
                        || c2.state != CoinState::Available
                        || c3.purse != p
                        || c3.state != CoinState::Available
                        || c4.purse != p
                        || c4.state != CoinState::Available
                        || (coin_value(c1.exponent)
                                + coin_value(c2.exponent)
                                + coin_value(c3.exponent)
                                + coin_value(c4.exponent)
                            != amount as nat)
                    },
            decreases n - i,
        {
            let ci_avail = matches!(self.coins[i].state, CoinState::Available);
            if self.coins[i].purse == p && ci_avail {
                proof {
                    let coin_key = (self.coins@[i as int].purse, self.coins@[i as int].idx);
                    assert(self.spec_coins@.dom().contains(coin_key));
                    assert(self.spec_coins@[coin_key] == self.coins@[i as int]);
                    assert(self.coins@[i as int].exponent <= MAX_EXPONENT);
                }
                let vi: u64 = pow2_u64_exec(self.coins[i].exponent);
                if vi <= amount {
                    let mut j: usize = 0;
                    while j < n
                        invariant
                            0 <= j <= n,
                            n == self.coins.len(),
                            i < n,
                            self.invariant(),
                            self.coins@[i as int].purse == p,
                            self.coins@[i as int].state == CoinState::Available,
                            vi as nat == coin_value(self.coins@[i as int].exponent),
                            vi <= 1073741824u64,
                            vi <= amount,
                            forall|i1: int, i2: int, i3: int, i4: int|
                                0 <= i1 < i as int
                                && 0 <= i2 < n as int
                                && 0 <= i3 < n as int
                                && 0 <= i4 < n as int
                                && i1 != i2 && i1 != i3 && i1 != i4
                                && i2 != i3 && i2 != i4 && i3 != i4
                                ==> {
                                    let c1 = #[trigger] self.coins@[i1];
                                    let c2 = #[trigger] self.coins@[i2];
                                    let c3 = #[trigger] self.coins@[i3];
                                    let c4 = #[trigger] self.coins@[i4];
                                    c1.purse != p
                                    || c1.state != CoinState::Available
                                    || c2.purse != p
                                    || c2.state != CoinState::Available
                                    || c3.purse != p
                                    || c3.state != CoinState::Available
                                    || c4.purse != p
                                    || c4.state != CoinState::Available
                                    || (coin_value(c1.exponent)
                                            + coin_value(c2.exponent)
                                            + coin_value(c3.exponent)
                                            + coin_value(c4.exponent)
                                        != amount as nat)
                                },
                            forall|j1: int, j3: int, j4: int|
                                0 <= j1 < j as int
                                && 0 <= j3 < n as int
                                && 0 <= j4 < n as int
                                && j1 != i as int && j3 != i as int && j4 != i as int
                                && j1 != j3 && j1 != j4 && j3 != j4
                                ==> {
                                    let c2 = #[trigger] self.coins@[j1];
                                    let c3 = #[trigger] self.coins@[j3];
                                    let c4 = #[trigger] self.coins@[j4];
                                    c2.purse != p
                                    || c2.state != CoinState::Available
                                    || c3.purse != p
                                    || c3.state != CoinState::Available
                                    || c4.purse != p
                                    || c4.state != CoinState::Available
                                    || (coin_value(self.coins@[i as int].exponent)
                                            + coin_value(c2.exponent)
                                            + coin_value(c3.exponent)
                                            + coin_value(c4.exponent)
                                        != amount as nat)
                                },
                        decreases n - j,
                    {
                        if j != i {
                            let cj_avail = matches!(self.coins[j].state, CoinState::Available);
                            if self.coins[j].purse == p && cj_avail {
                                proof {
                                    let coin_key = (self.coins@[j as int].purse,
                                                    self.coins@[j as int].idx);
                                    assert(self.spec_coins@.dom().contains(coin_key));
                                    assert(self.spec_coins@[coin_key] == self.coins@[j as int]);
                                    assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
                                }
                                let vj: u64 = pow2_u64_exec(self.coins[j].exponent);
                                if vi + vj <= amount {
                                    let mut k: usize = 0;
                                    while k < n
                                        invariant
                                            0 <= k <= n,
                                            n == self.coins.len(),
                                            i < n,
                                            j < n,
                                            i != j as usize,
                                            self.invariant(),
                                            self.coins@[i as int].purse == p,
                                            self.coins@[i as int].state == CoinState::Available,
                                            self.coins@[j as int].purse == p,
                                            self.coins@[j as int].state == CoinState::Available,
                                            vi as nat == coin_value(self.coins@[i as int].exponent),
                                            vj as nat == coin_value(self.coins@[j as int].exponent),
                                            vi <= 1073741824u64,
                                            vj <= 1073741824u64,
                                            vi + vj <= amount,
                                            forall|k1: int, k4: int|
                                                0 <= k1 < k as int
                                                && 0 <= k4 < n as int
                                                && k1 != i as int && k1 != j as int
                                                && k4 != i as int && k4 != j as int
                                                && k1 != k4
                                                ==> {
                                                    let c3 = #[trigger] self.coins@[k1];
                                                    let c4 = #[trigger] self.coins@[k4];
                                                    c3.purse != p
                                                    || c3.state != CoinState::Available
                                                    || c4.purse != p
                                                    || c4.state != CoinState::Available
                                                    || (coin_value(self.coins@[i as int].exponent)
                                                            + coin_value(self.coins@[j as int].exponent)
                                                            + coin_value(c3.exponent)
                                                            + coin_value(c4.exponent)
                                                        != amount as nat)
                                                },
                                        decreases n - k,
                                    {
                                        if k != i && k != j {
                                            let ck_avail = matches!(self.coins[k].state,
                                                                    CoinState::Available);
                                            if self.coins[k].purse == p && ck_avail {
                                                proof {
                                                    let coin_key = (self.coins@[k as int].purse,
                                                                    self.coins@[k as int].idx);
                                                    assert(self.spec_coins@.dom().contains(coin_key));
                                                    assert(self.spec_coins@[coin_key]
                                                        == self.coins@[k as int]);
                                                    assert(self.coins@[k as int].exponent
                                                        <= MAX_EXPONENT);
                                                }
                                                let vk: u64 = pow2_u64_exec(self.coins[k].exponent);
                                                if vi + vj + vk <= amount {
                                                    let mut m: usize = 0;
                                                    while m < n
                                                        invariant
                                                            0 <= m <= n,
                                                            n == self.coins.len(),
                                                            i < n,
                                                            j < n,
                                                            k < n,
                                                            i != j as usize,
                                                            i != k as usize,
                                                            j != k as usize,
                                                            self.invariant(),
                                                            self.coins@[i as int].purse == p,
                                                            self.coins@[i as int].state == CoinState::Available,
                                                            self.coins@[j as int].purse == p,
                                                            self.coins@[j as int].state == CoinState::Available,
                                                            self.coins@[k as int].purse == p,
                                                            self.coins@[k as int].state == CoinState::Available,
                                                            vi as nat == coin_value(self.coins@[i as int].exponent),
                                                            vj as nat == coin_value(self.coins@[j as int].exponent),
                                                            vk as nat == coin_value(self.coins@[k as int].exponent),
                                                            vi <= 1073741824u64,
                                                            vj <= 1073741824u64,
                                                            vk <= 1073741824u64,
                                                            vi + vj + vk <= amount,
                                                            forall|m2: int|
                                                                0 <= m2 < m as int
                                                                && m2 != i as int
                                                                && m2 != j as int
                                                                && m2 != k as int
                                                                ==>
                                                                (#[trigger] self.coins@[m2]).purse != p
                                                                || self.coins@[m2].state != CoinState::Available
                                                                || (coin_value(self.coins@[i as int].exponent)
                                                                        + coin_value(self.coins@[j as int].exponent)
                                                                        + coin_value(self.coins@[k as int].exponent)
                                                                        + coin_value(self.coins@[m2].exponent)
                                                                    != amount as nat),
                                                        decreases n - m,
                                                    {
                                                        if m != i && m != j && m != k {
                                                            let cm_avail = matches!(
                                                                self.coins[m].state,
                                                                CoinState::Available);
                                                            if self.coins[m].purse == p && cm_avail {
                                                                proof {
                                                                    let coin_key = (
                                                                        self.coins@[m as int].purse,
                                                                        self.coins@[m as int].idx);
                                                                    assert(self.spec_coins@.dom()
                                                                        .contains(coin_key));
                                                                    assert(self.spec_coins@[coin_key]
                                                                        == self.coins@[m as int]);
                                                                    assert(self.coins@[m as int].exponent
                                                                        <= MAX_EXPONENT);
                                                                }
                                                                let vm: u64 = pow2_u64_exec(
                                                                    self.coins[m].exponent);
                                                                if vi + vj + vk + vm == amount {
                                                                    let k1 = (self.coins[i].purse,
                                                                              self.coins[i].idx);
                                                                    let k2 = (self.coins[j].purse,
                                                                              self.coins[j].idx);
                                                                    let k3 = (self.coins[k].purse,
                                                                              self.coins[k].idx);
                                                                    let k4 = (self.coins[m].purse,
                                                                              self.coins[m].idx);
                                                                    proof {
                                                                        assert(self.spec_coins@.dom()
                                                                            .contains(k1));
                                                                        assert(self.spec_coins@.dom()
                                                                            .contains(k2));
                                                                        assert(self.spec_coins@.dom()
                                                                            .contains(k3));
                                                                        assert(self.spec_coins@.dom()
                                                                            .contains(k4));
                                                                        assert(k1 != k2);
                                                                        assert(k1 != k3);
                                                                        assert(k1 != k4);
                                                                        assert(k2 != k3);
                                                                        assert(k2 != k4);
                                                                        assert(k3 != k4);
                                                                    }
                                                                    return Some((k1, k2, k3, k4));
                                                                }
                                                            }
                                                        }
                                                        m = m + 1;
                                                    }
                                                }
                                            }
                                        }
                                        k = k + 1;
                                    }
                                }
                            }
                        }
                        j = j + 1;
                    }
                }
            }
            i = i + 1;
        }
        None
    }

    /// Composite multi-coin subset-sum search: tries 1-, 2-, 3-, 4-coin
    /// exact covers in order and returns the first hit. The `None`
    /// branch carries the *conjoined* sharp postconditions from all
    /// four primitives — i.e. no subset of size 1, 2, 3, or 4 in the
    /// purse sums to `amount`.
    ///
    /// Practical multi-coin selector for task #87. Full N-coin powerset
    /// (any size) remains open; this covers the realistic small-K case
    /// that almost all transfers actually hit.
    pub fn find_subset_sum_up_to_4(&self, p: PurseId, amount: u64)
        -> (res: Option<SubsetSumCover>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(SubsetSumCover::One(k1)) =>
                    self.coins().dom().contains(k1)
                    && k1.0 == p
                    && self.coins()[k1].state == CoinState::Available
                    && coin_value(self.coins()[k1].exponent) == amount as nat,
                Some(SubsetSumCover::Two(k1, k2)) =>
                    self.coins().dom().contains(k1)
                    && self.coins().dom().contains(k2)
                    && k1 != k2
                    && k1.0 == p && k2.0 == p
                    && self.coins()[k1].state == CoinState::Available
                    && self.coins()[k2].state == CoinState::Available
                    && coin_value(self.coins()[k1].exponent)
                        + coin_value(self.coins()[k2].exponent)
                        == amount as nat,
                Some(SubsetSumCover::Three(k1, k2, k3)) =>
                    self.coins().dom().contains(k1)
                    && self.coins().dom().contains(k2)
                    && self.coins().dom().contains(k3)
                    && k1 != k2 && k1 != k3 && k2 != k3
                    && k1.0 == p && k2.0 == p && k3.0 == p
                    && self.coins()[k1].state == CoinState::Available
                    && self.coins()[k2].state == CoinState::Available
                    && self.coins()[k3].state == CoinState::Available
                    && coin_value(self.coins()[k1].exponent)
                        + coin_value(self.coins()[k2].exponent)
                        + coin_value(self.coins()[k3].exponent)
                        == amount as nat,
                Some(SubsetSumCover::Four(k1, k2, k3, k4)) =>
                    self.coins().dom().contains(k1)
                    && self.coins().dom().contains(k2)
                    && self.coins().dom().contains(k3)
                    && self.coins().dom().contains(k4)
                    && k1 != k2 && k1 != k3 && k1 != k4
                    && k2 != k3 && k2 != k4 && k3 != k4
                    && k1.0 == p && k2.0 == p && k3.0 == p && k4.0 == p
                    && self.coins()[k1].state == CoinState::Available
                    && self.coins()[k2].state == CoinState::Available
                    && self.coins()[k3].state == CoinState::Available
                    && self.coins()[k4].state == CoinState::Available
                    && coin_value(self.coins()[k1].exponent)
                        + coin_value(self.coins()[k2].exponent)
                        + coin_value(self.coins()[k3].exponent)
                        + coin_value(self.coins()[k4].exponent)
                        == amount as nat,
                None => {
                    // Conjoined sharp Nones from the four primitives.
                    &&& forall|k: (PurseId, u64)|
                            #[trigger] self.coins().dom().contains(k)
                            && k.0 == p
                            && self.coins()[k].state == CoinState::Available
                            ==> coin_value(self.coins()[k].exponent) != amount as nat
                    &&& forall|i1: int, i2: int|
                            0 <= i1 < self.coins@.len()
                            && 0 <= i2 < self.coins@.len()
                            && i1 != i2
                            ==> {
                                let c1 = #[trigger] self.coins@[i1];
                                let c2 = #[trigger] self.coins@[i2];
                                c1.purse != p
                                || c1.state != CoinState::Available
                                || c2.purse != p
                                || c2.state != CoinState::Available
                                || (coin_value(c1.exponent) + coin_value(c2.exponent)
                                    != amount as nat)
                            }
                    &&& forall|i1: int, i2: int, i3: int|
                            0 <= i1 < self.coins@.len()
                            && 0 <= i2 < self.coins@.len()
                            && 0 <= i3 < self.coins@.len()
                            && i1 != i2 && i1 != i3 && i2 != i3
                            ==> {
                                let c1 = #[trigger] self.coins@[i1];
                                let c2 = #[trigger] self.coins@[i2];
                                let c3 = #[trigger] self.coins@[i3];
                                c1.purse != p
                                || c1.state != CoinState::Available
                                || c2.purse != p
                                || c2.state != CoinState::Available
                                || c3.purse != p
                                || c3.state != CoinState::Available
                                || (coin_value(c1.exponent)
                                        + coin_value(c2.exponent)
                                        + coin_value(c3.exponent)
                                    != amount as nat)
                            }
                    &&& forall|i1: int, i2: int, i3: int, i4: int|
                            0 <= i1 < self.coins@.len()
                            && 0 <= i2 < self.coins@.len()
                            && 0 <= i3 < self.coins@.len()
                            && 0 <= i4 < self.coins@.len()
                            && i1 != i2 && i1 != i3 && i1 != i4
                            && i2 != i3 && i2 != i4 && i3 != i4
                            ==> {
                                let c1 = #[trigger] self.coins@[i1];
                                let c2 = #[trigger] self.coins@[i2];
                                let c3 = #[trigger] self.coins@[i3];
                                let c4 = #[trigger] self.coins@[i4];
                                c1.purse != p
                                || c1.state != CoinState::Available
                                || c2.purse != p
                                || c2.state != CoinState::Available
                                || c3.purse != p
                                || c3.state != CoinState::Available
                                || c4.purse != p
                                || c4.state != CoinState::Available
                                || (coin_value(c1.exponent)
                                        + coin_value(c2.exponent)
                                        + coin_value(c3.exponent)
                                        + coin_value(c4.exponent)
                                    != amount as nat)
                            }
                },
            },
    {
        match self.find_exact_single_coin(p, amount) {
            Some(k1) => return Some(SubsetSumCover::One(k1)),
            None => {}
        }
        match self.find_two_coin_exact_cover(p, amount) {
            Some((k1, k2)) => return Some(SubsetSumCover::Two(k1, k2)),
            None => {}
        }
        match self.find_three_coin_exact_cover(p, amount) {
            Some((k1, k2, k3)) => return Some(SubsetSumCover::Three(k1, k2, k3)),
            None => {}
        }
        match self.find_four_coin_exact_cover(p, amount) {
            Some((k1, k2, k3, k4)) =>
                Some(SubsetSumCover::Four(k1, k2, k3, k4)),
            None => None,
        }
    }

    /// Tier-2 (split cover, §6.3): find any `Available` coin in purse `p`
    /// whose `coin_value(exp)` strictly exceeds `amount`. Such a coin can
    /// be split into two coins of strictly smaller exponent (one of which
    /// covers `amount`); the remainder becomes change. Returns the first
    /// matching coin in Vec order, or `None` if none exists.
    ///
    /// Quint analog: the witness for `existsSplitCover(p, amount)`.
    pub fn find_split_cover_coin(&self, p: PurseId, amount: u64)
        -> (res: Option<(PurseId, u64)>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(key) =>
                    self.coins().dom().contains(key)
                    && key.0 == p
                    && self.coins()[key].state == CoinState::Available
                    && coin_value(self.coins()[key].exponent) > amount as nat,
                None =>
                    forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        && k.0 == p
                        && self.coins()[k].state == CoinState::Available
                        ==> coin_value(self.coins()[k].exponent) <= amount as nat,
            },
    {
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).purse != p
                    || self.coins@[jj].state != CoinState::Available
                    || coin_value(self.coins@[jj].exponent) <= amount as nat,
            decreases self.coins.len() - j,
        {
            let is_avail = matches!(self.coins[j].state, CoinState::Available);
            proof {
                let coin_key = (self.coins@[j as int].purse, self.coins@[j as int].idx);
                assert(self.spec_coins@.dom().contains(coin_key));
                assert(self.spec_coins@[coin_key] == self.coins@[j as int]);
                assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
            }
            let value: u64 = pow2_u64_exec(self.coins[j].exponent);
            if self.coins[j].purse == p && is_avail && value > amount {
                let key = (self.coins[j].purse, self.coins[j].idx);
                proof {
                    assert(self.spec_coins@.dom().contains(key));
                }
                return Some(key);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                && k.0 == p
                && self.coins()[k].state == CoinState::Available
                implies coin_value(self.coins()[k].exponent) <= amount as nat
            by {
                let w = choose|jj: int|
                    0 <= jj < self.coins@.len()
                    && #[trigger] self.coins@[jj].purse == k.0
                    && self.coins@[jj].idx == k.1;
                assert(self.coins@[w].purse == p);
                assert(self.coins@[w].state == self.coins()[k].state);
                assert(self.coins@[w].exponent == self.coins()[k].exponent);
            }
        }
        None
    }

    /// Composite single-coin selector (§6.3 tier-1 + tier-2, single-coin
    /// case). Tries the exact-cover branch first (Quint
    /// `existsExactCover`'s single-coin witness), then falls back to the
    /// split-cover branch (Quint `existsSplitCover`'s witness). Returns
    /// `None` only when no single `Available` coin in `p` has value at
    /// least `amount`.
    ///
    /// Multi-coin exact subset-sum (Quint
    /// `selectExactCoverDeterministic`) and tier-3 entry-supplemented
    /// cover are not yet wired in; their dedicated exec implementations
    /// will compose with this in later phases.
    pub fn select_single_coin_cover(&self, p: PurseId, amount: u64)
        -> (res: Option<CoinSelection>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(CoinSelection::Exact { coin: key }) =>
                    self.coins().dom().contains(key)
                    && key.0 == p
                    && self.coins()[key].state == CoinState::Available
                    && coin_value(self.coins()[key].exponent) == amount as nat,
                Some(CoinSelection::Split { coin: key }) =>
                    self.coins().dom().contains(key)
                    && key.0 == p
                    && self.coins()[key].state == CoinState::Available
                    && coin_value(self.coins()[key].exponent) > amount as nat,
                None =>
                    forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        && k.0 == p
                        && self.coins()[k].state == CoinState::Available
                        ==> coin_value(self.coins()[k].exponent) < amount as nat,
            },
    {
        match self.find_exact_single_coin(p, amount) {
            Some(key) => Some(CoinSelection::Exact { coin: key }),
            None => match self.find_split_cover_coin(p, amount) {
                Some(key) => Some(CoinSelection::Split { coin: key }),
                None => None,
            },
        }
    }

    /// Greedy multi-coin selection. Scans `Available` coins in purse `p` in
    /// Vec order, accumulating until the running total meets or exceeds
    /// `requested`. Returns the selected key list, or `None` if the total
    /// Available value in `p` is insufficient.
    ///
    /// **Pilot scope:** this is NOT the design's three-tier exact-cover
    /// selection (§6.3). Greedy may overshoot `requested` (returning more
    /// value than asked). Real exact-subset-sum requires powerset
    /// enumeration with lex-min disambiguation (Quint
    /// `selectExactCoverDeterministic`); deferred.
    pub fn select_coins_for_amount(&self, p: PurseId, requested: u64)
        -> (res: Option<Vec<(PurseId, u64)>>)
        requires
            self.invariant(),
            self.coins@.len() <= (u64::MAX / 1073741824) as nat,
            // Bound `requested` so `accumulated + value` doesn't overflow when
            // `accumulated < requested` and `value <= 2^30`.
            requested <= u64::MAX - 1073741824,
            requested >= 1,
        ensures
            match res {
                Some(keys) => {
                    &&& forall|i: int| 0 <= i < keys@.len() ==>
                            self.coins().dom().contains(#[trigger] keys@[i])
                            && keys@[i].0 == p
                            && self.coins()[keys@[i]].state == CoinState::Available
                    &&& sum_of_coin_values(self.coins(), keys@) >= requested as nat
                },
                None =>
                    sum_avail_prefix(self.coins@, p, self.coins@.len() as nat)
                        < requested as nat,
            },
    {
        let mut selected: Vec<(PurseId, u64)> = Vec::new();
        let mut accumulated: u64 = 0;
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                self.coins@.len() <= (u64::MAX / 1073741824) as nat,
                requested <= u64::MAX - 1073741824,
                accumulated < requested,
                accumulated as nat == sum_avail_prefix(self.coins@, p, j as nat),
                accumulated as nat == sum_of_coin_values(self.coins(), selected@),
                forall|i: int| 0 <= i < selected@.len() ==>
                    self.coins().dom().contains(#[trigger] selected@[i])
                    && selected@[i].0 == p
                    && self.coins()[selected@[i]].state == CoinState::Available,
            decreases self.coins.len() - j,
        {
            let is_avail = matches!(self.coins[j].state, CoinState::Available);
            proof {
                // Bound the per-step delta for cumulative overflow safety.
                // Per-step coin value is at most coin_value(MAX_EXPONENT) = 2^30.
                let coin_key = (self.coins@[j as int].purse, self.coins@[j as int].idx);
                assert(self.spec_coins@.dom().contains(coin_key));
                assert(self.spec_coins@[coin_key] == self.coins@[j as int]);
                assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
                lemma_pow2_at_30();
                lemma_pow2_monotone(self.coins@[j as int].exponent as nat,
                                    MAX_EXPONENT as nat);
                assert(sum_avail_prefix(self.coins@, p, (j + 1) as nat)
                    <= sum_avail_prefix(self.coins@, p, j as nat) + 1073741824);
            }
            if self.coins[j].purse == p && is_avail {
                let key = (self.coins[j].purse, self.coins[j].idx);
                proof {
                    assert(self.spec_coins@.dom().contains(key));
                    assert(self.spec_coins@[key] == self.coins@[j as int]);
                    assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
                }
                let value: u64 = pow2_u64_exec(self.coins[j].exponent);
                let ghost selected_before = selected@;
                selected.push(key);
                assert(value <= 1073741824);
                assert(accumulated < requested);
                assert(requested <= u64::MAX - 1073741824);
                accumulated = accumulated + value;
                proof {
                    // (l) gives ghost-map record matches Vec entry.
                    assert(self.spec_coins@.dom().contains(key));
                    assert(self.coins()[key].state == CoinState::Available);
                    // Append-decomposition for sum_of_coin_values.
                    assert(selected@ =~= selected_before.push(key));
                    assert(selected@.subrange(0, selected_before.len() as int)
                        =~= selected_before);
                    assert(sum_of_coin_values(self.coins(), selected@)
                        == sum_of_coin_values(self.coins(), selected_before)
                            + coin_value(self.coins()[key].exponent));
                }
                if accumulated >= requested {
                    return Some(selected);
                }
            }
            j = j + 1;
        }
        None
    }

    /// Remove every coin in purse `p` (any state) from both the exec Vec
    /// and the ghost map. Purses themselves are not touched.
    pub fn purge_coins_of_purse(&mut self, p: PurseId)
        requires
            old(self).invariant(),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).next_purse_id == old(self).next_purse_id,
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).coins() == old(self).coins().remove_keys(
                Set::new(|k: (PurseId, u64)| k.0 == p)
            ),
            forall|k: (PurseId, u64)|
                #[trigger] final(self).coins().dom().contains(k) ==> k.0 != p,
    {
        let ghost initial_coins = self.spec_coins@;

        loop
            invariant
                self.invariant(),
                self.purses() == old(self).purses(),
                self.purses@ == old(self).purses@,
                self.next_purse_id == old(self).next_purse_id,
                self.entries@ == old(self).entries@,
                self.spec_entries@ == old(self).spec_entries@,
                self.operations@ == old(self).operations@,
                self.spec_operations@ == old(self).spec_operations@,
                self.next_handle == old(self).next_handle,
                self.next_age == old(self).next_age,
                self.fee_balance == old(self).fee_balance,
                self.next_extrinsic_id == old(self).next_extrinsic_id,
                self.events@ == old(self).events@,
                self.paid_ring_membership == old(self).paid_ring_membership,
                self.total_in == old(self).total_in,
                self.total_out == old(self).total_out,
                self.tokens@ == old(self).tokens@,
                self.chain_coins@ == old(self).chain_coins@,
                self.chain_entries@ == old(self).chain_entries@,
                // Current spec_coins is a subset of initial that preserves all
                // entries with purse != p.
                forall|k: (PurseId, u64)| #[trigger] self.spec_coins@.dom().contains(k)
                    ==> initial_coins.dom().contains(k)
                        && self.spec_coins@[k] == initial_coins[k],
                forall|k: (PurseId, u64)|
                    #[trigger] initial_coins.dom().contains(k) && k.0 != p
                    ==> self.spec_coins@.dom().contains(k),
                initial_coins == old(self).coins(),
            decreases self.coins.len(),
        {
            match self.find_coin_with_purse(p) {
                None => {
                    // find-None postcondition: forall j. coins@[j].purse != p.
                    proof {
                        // No spec_coins key has k.0 == p: if any did, (m) would
                        // give a Vec witness with purse == p — contradiction.
                        assert forall|k: (PurseId, u64)|
                            #[trigger] self.spec_coins@.dom().contains(k)
                            implies k.0 != p
                        by {
                            if k.0 == p {
                                let w = choose|jj: int|
                                    0 <= jj < self.coins@.len()
                                    && #[trigger] self.coins@[jj].purse == k.0
                                    && self.coins@[jj].idx == k.1;
                                assert(self.coins@[w].purse == p);
                            }
                        }
                        // Combined with loop invariants, current spec_coins is
                        // exactly initial_coins minus all keys with k.0 == p.
                        assert(self.spec_coins@
                            =~= initial_coins.remove_keys(
                                Set::new(|k: (PurseId, u64)| k.0 == p)
                            ));
                    }
                    return;
                }
                Some(idx) => {
                    let ghost removed_entry = self.coins@[idx as int];
                    let ghost removed_key = (removed_entry.purse, removed_entry.idx);
                    proof {
                        assert(self.spec_coins@.dom().contains(removed_key));
                    }
                    self.remove_coin_at(idx);
                }
            }
        }
    }

    /// Internal: scan the entry Vec for the first entry with `purse == p`.
    fn find_entry_with_purse(&self, p: PurseId) -> (res: Option<usize>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(i) =>
                    (i as int) < self.entries@.len()
                    && self.entries@[i as int].purse == p,
                None =>
                    forall|j: int| 0 <= j < self.entries@.len()
                        ==> (#[trigger] self.entries@[j]).purse != p,
            },
    {
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.entries@[jj]).purse != p,
            decreases self.entries.len() - j,
        {
            if self.entries[j].purse == p {
                return Some(j);
            }
            j += 1;
        }
        None
    }

    /// Internal: remove the entry at exec-Vec index `idx`. Vec shrinks by 1
    /// (via `swap_remove`); the ghost entry map drops exactly the key that
    /// belonged to the removed Vec entry.
    fn remove_entry_at(&mut self, idx: usize)
        requires
            old(self).invariant(),
            (idx as int) < old(self).entries@.len(),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).next_purse_id == old(self).next_purse_id,
            final(self).coins@ == old(self).coins@,
            final(self).spec_coins@ == old(self).spec_coins@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            ({
                let removed = old(self).entries@[idx as int];
                final(self).entries()
                    == old(self).entries().remove((removed.purse, removed.idx))
            }),
            final(self).entries@.len() == old(self).entries@.len() - 1,
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_next_purse_id = self.next_purse_id;
        let ghost old_coins = self.spec_coins@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_entries = self.spec_entries@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_operations = self.spec_operations@;
        let ghost old_operations_vec = self.operations@;
        let ghost target_idx = idx as int;
        let ghost removed_e = old_entries_vec[target_idx];
        let ghost removed_key = (removed_e.purse, removed_e.idx);
        let ghost last_idx = old_entries_vec.len() - 1;

        let _ = self.entries.swap_remove(idx);
        proof {
            self.spec_entries = Ghost(self.spec_entries@.remove(removed_key));

            let new_entries_vec = self.entries@;
            let new_entries = self.spec_entries@;
            let new_m = self.spec_purses@;

            assert(self.purses@ == old_purses_vec);
            assert(self.spec_purses@ == old_spec_purses);
            assert(self.next_purse_id == old_next_purse_id);
            assert(self.coins@ == old_coins_vec);
            assert(self.spec_coins@ == old_coins);

            assert(new_entries_vec.len() == old_entries_vec.len() - 1);
            assert forall|k: int| 0 <= k < new_entries_vec.len() && k != target_idx implies
                #[trigger] new_entries_vec[k] == old_entries_vec[k]
            by {}
            assert(target_idx < new_entries_vec.len() ==>
                new_entries_vec[target_idx] == old_entries_vec[last_idx]);

            assert(old_entries_vec[target_idx].purse == removed_key.0);
            assert(old_entries_vec[target_idx].idx == removed_key.1);
            assert forall|k: int|
                0 <= k < old_entries_vec.len() && k != target_idx implies
                (#[trigger] old_entries_vec[k]).purse != removed_key.0
                || old_entries_vec[k].idx != removed_key.1
            by {}

            assert(old_entries.dom().contains(removed_key));
            assert(new_entries.dom() =~= old_entries.dom().remove(removed_key));

            // (o) entry key consistency.
            assert forall|k: (PurseId, u64)| #[trigger] new_entries.dom().contains(k)
                implies new_entries[k].purse == k.0 && new_entries[k].idx == k.1
            by { assert(old_entries.dom().contains(k)); }

            // (p) entry refint.
            assert forall|k: (PurseId, u64)| #[trigger] new_entries.dom().contains(k)
                implies new_m.dom().contains(k.0)
            by { assert(old_entries.dom().contains(k)); }

            // (q) entry idx < next_entry_idx.
            assert forall|k: (PurseId, u64)| #[trigger] new_entries.dom().contains(k)
                implies k.1 < new_m[k.0].next_entry_idx
            by { assert(old_entries.dom().contains(k)); }

            // (r) Vec → ghost
            assert forall|jj: int| 0 <= jj < new_entries_vec.len() implies
                new_entries.dom().contains(
                    (#[trigger] new_entries_vec[jj].purse, new_entries_vec[jj].idx)
                )
                && new_entries[(new_entries_vec[jj].purse, new_entries_vec[jj].idx)]
                    == new_entries_vec[jj]
            by {
                if jj == target_idx {
                    assert(new_entries_vec[jj] == old_entries_vec[last_idx]);
                    assert(last_idx != target_idx);
                    let oe = old_entries_vec[last_idx];
                    assert(old_entries.dom().contains((oe.purse, oe.idx)));
                    assert((oe.purse, oe.idx) != removed_key);
                    assert(old_entries[(oe.purse, oe.idx)] == oe);
                } else {
                    assert(new_entries_vec[jj] == old_entries_vec[jj]);
                    let oe = old_entries_vec[jj];
                    assert(old_entries.dom().contains((oe.purse, oe.idx)));
                    assert((oe.purse, oe.idx) != removed_key);
                    assert(old_entries[(oe.purse, oe.idx)] == oe);
                }
            }

            // (s) ghost → Vec
            assert forall|k: (PurseId, u64)| #[trigger] new_entries.dom().contains(k)
                implies exists|jj: int|
                    0 <= jj < new_entries_vec.len()
                    && #[trigger] new_entries_vec[jj].purse == k.0
                    && new_entries_vec[jj].idx == k.1
            by {
                assert(old_entries.dom().contains(k));
                assert(k != removed_key);
                let w_old = choose|jj: int|
                    0 <= jj < old_entries_vec.len()
                    && #[trigger] old_entries_vec[jj].purse == k.0
                    && old_entries_vec[jj].idx == k.1;
                assert(w_old != target_idx);
                if w_old == last_idx {
                    assert(target_idx < new_entries_vec.len());
                    assert(new_entries_vec[target_idx] == old_entries_vec[last_idx]);
                } else {
                    assert(w_old < last_idx);
                    assert(w_old < new_entries_vec.len());
                    assert(new_entries_vec[w_old] == old_entries_vec[w_old]);
                }
            }

            // (t) no duplicates
            assert forall|a: int, b: int|
                0 <= a < new_entries_vec.len() && 0 <= b < new_entries_vec.len()
                && (#[trigger] new_entries_vec[a]).purse
                    == (#[trigger] new_entries_vec[b]).purse
                && new_entries_vec[a].idx == new_entries_vec[b].idx
                implies a == b
            by {
                if a == target_idx && b == target_idx {
                } else if a == target_idx {
                    assert(new_entries_vec[a] == old_entries_vec[last_idx]);
                    assert(new_entries_vec[b] == old_entries_vec[b]);
                    assert(b != last_idx);
                } else if b == target_idx {
                    assert(new_entries_vec[b] == old_entries_vec[last_idx]);
                    assert(new_entries_vec[a] == old_entries_vec[a]);
                    assert(a != last_idx);
                } else {
                    assert(new_entries_vec[a] == old_entries_vec[a]);
                    assert(new_entries_vec[b] == old_entries_vec[b]);
                }
            }
        }
    }

    /// Remove every entry in purse `p` (any on-chain state) from the
    /// exec Vec and the ghost map. Purses and coins untouched.
    pub fn purge_entries_of_purse(&mut self, p: PurseId)
        requires
            old(self).invariant(),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).next_purse_id == old(self).next_purse_id,
            final(self).coins@ == old(self).coins@,
            final(self).spec_coins@ == old(self).spec_coins@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).entries() == old(self).entries().remove_keys(
                Set::new(|k: (PurseId, u64)| k.0 == p)
            ),
            forall|k: (PurseId, u64)|
                #[trigger] final(self).entries().dom().contains(k) ==> k.0 != p,
    {
        let ghost initial_entries = self.spec_entries@;

        loop
            invariant
                self.invariant(),
                self.purses() == old(self).purses(),
                self.purses@ == old(self).purses@,
                self.next_purse_id == old(self).next_purse_id,
                self.coins@ == old(self).coins@,
                self.spec_coins@ == old(self).spec_coins@,
                self.operations@ == old(self).operations@,
                self.spec_operations@ == old(self).spec_operations@,
                self.next_handle == old(self).next_handle,
                self.next_age == old(self).next_age,
                self.fee_balance == old(self).fee_balance,
                self.next_extrinsic_id == old(self).next_extrinsic_id,
                self.events@ == old(self).events@,
                self.paid_ring_membership == old(self).paid_ring_membership,
                self.total_in == old(self).total_in,
                self.total_out == old(self).total_out,
                self.tokens@ == old(self).tokens@,
                self.chain_coins@ == old(self).chain_coins@,
                self.chain_entries@ == old(self).chain_entries@,
                forall|k: (PurseId, u64)| #[trigger] self.spec_entries@.dom().contains(k)
                    ==> initial_entries.dom().contains(k)
                        && self.spec_entries@[k] == initial_entries[k],
                forall|k: (PurseId, u64)|
                    #[trigger] initial_entries.dom().contains(k) && k.0 != p
                    ==> self.spec_entries@.dom().contains(k),
                initial_entries == old(self).entries(),
            decreases self.entries.len(),
        {
            match self.find_entry_with_purse(p) {
                None => {
                    proof {
                        assert forall|k: (PurseId, u64)|
                            #[trigger] self.spec_entries@.dom().contains(k)
                            implies k.0 != p
                        by {
                            if k.0 == p {
                                let w = choose|jj: int|
                                    0 <= jj < self.entries@.len()
                                    && #[trigger] self.entries@[jj].purse == k.0
                                    && self.entries@[jj].idx == k.1;
                                assert(self.entries@[w].purse == p);
                            }
                        }
                        assert(self.spec_entries@
                            =~= initial_entries.remove_keys(
                                Set::new(|k: (PurseId, u64)| k.0 == p)
                            ));
                    }
                    return;
                }
                Some(idx) => {
                    let ghost removed_e = self.entries@[idx as int];
                    let ghost removed_key = (removed_e.purse, removed_e.idx);
                    proof {
                        assert(self.spec_entries@.dom().contains(removed_key));
                    }
                    self.remove_entry_at(idx);
                }
            }
        }
    }

    /// Tracked top-up via entry: wraps [`Self::top_up_via_entry`] in
    /// a `KTopUp` operation that starts in `Preparing` and immediately
    /// advances to `Submitted` (the extrinsic creating the entry has
    /// been broadcast to the chain). The op's later transitions
    /// (`InBlock`, `Finalized`, `Waiting(ready_at)`, `Done`) fire as
    /// chain notifications arrive — those are driven by the host via
    /// the `mark_op_*` primitives.
    ///
    /// Quint analog: the combination of `startTopUp` + `opCommitTopUp`.
    pub fn tracked_top_up_via_entry(
        &mut self,
        p: PurseId,
        exponent: u8,
        member_key: u64,
        allocated_at: u64,
        ready_at: u64,
        ring_idx: u64,
    ) -> (res: (OpHandle, (PurseId, u64)))
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(p),
            old(self).purses()[p].next_entry_idx < u64::MAX,
            old(self).next_handle < u64::MAX,
            old(self).events@.len() + 3 <= u64::MAX as nat,
            exponent <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            res.0 == old(self).next_handle,
            final(self).operations().dom().contains(res.0),
            final(self).operations()[res.0].status == OpStatus::Submitted,
            final(self).operations()[res.0].kind == OpKind::TopUp,
            final(self).operations()[res.0].purse == p,
            res.1.0 == p,
            res.1.1 == old(self).purses()[p].next_entry_idx,
            final(self).entries().dom().contains(res.1),
            final(self).entries()[res.1].on_chain == EntryOnChain::Waiting,
            final(self).entries()[res.1].local == EntryLocal::LocalAvailable,
    {
        let handle = self.start_op(OpKind::TopUp, p);
        let key = self.top_up_via_entry(
            p, exponent, member_key, allocated_at, ready_at, ring_idx,
        );
        proof {
            assert(self.operations()[handle].kind == OpKind::TopUp);
            assert(self.operations()[handle].purse == p);
        }
        self.mark_op_submitted(handle);
        (handle, key)
    }

    /// Top-up via recycler entry (Quint `topUp`): allocate a fresh
    /// recycler entry of `exponent` in purse `p`, in the `Waiting` /
    /// `LocalAvailable` state. Caller supplies the chain-side
    /// bookkeeping (`member_key`, `allocated_at`, `ready_at`,
    /// `ring_idx`) — these come from the host's chain abstraction
    /// (e.g. derive `member_key` from the purse's anonymity-ring
    /// secret, `ready_at = allocated_at + JitterMax`).
    ///
    /// This is the entry-side bottom-layer effect of the design §8.2
    /// top-up — funds entering via a recycler ring rather than as
    /// direct coins. Pair with `set_entry_on_chain` once the chain
    /// confirms ring-membership floor → entry becomes `Ready`.
    pub fn top_up_via_entry(
        &mut self,
        p: PurseId,
        exponent: u8,
        member_key: u64,
        allocated_at: u64,
        ready_at: u64,
        ring_idx: u64,
    ) -> (key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(p),
            old(self).purses()[p].next_entry_idx < u64::MAX,
            old(self).events@.len() < u64::MAX as nat,
            exponent <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            key.0 == p,
            key.1 == old(self).purses()[p].next_entry_idx,
            final(self).entries().dom().contains(key),
            final(self).entries()[key] == (EntryRec {
                purse: p,
                idx: key.1,
                exponent,
                on_chain: EntryOnChain::Waiting,
                local: EntryLocal::LocalAvailable,
                member_key,
                allocated_at,
                ready_at,
                ring_idx,
            }),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::EntryAllocated {
                purse: p,
                exponent,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let key = self.add_entry_with_meta(
            p,
            exponent,
            EntryOnChain::Waiting,
            EntryLocal::LocalAvailable,
            member_key,
            allocated_at,
            ready_at,
            ring_idx,
        );
        self.emit_event(Event::EntryAllocated {
            purse: p,
            exponent,
        });
        key
    }

    /// Top-up: allocate `exp_seq.len()` fresh coins in purse `p`, one per
    /// exponent in `exp_seq` (in order). Each call to `add_coin` allocates the
    /// next available coin index, so the resulting coin keys are
    /// `(p, old_next_coin_idx)`, `(p, old_next_coin_idx + 1)`, …
    ///
    /// This is the design §8.2 top-up reduced to its bottom-layer effect:
    /// produce a batch of new coins under the purse's namespace. The chain
    /// interaction, fee handling, and `FundingOrigin` plumbing are deferred.
    pub fn top_up_purse(&mut self, p: PurseId, exp_seq: Vec<u8>)
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(p),
            old(self).purses()[p].next_coin_idx as nat + exp_seq@.len() <= u64::MAX as nat,
            old(self).next_age as nat + exp_seq@.len() <= u64::MAX as nat,
            forall|j: int| 0 <= j < exp_seq@.len() ==>
                (#[trigger] exp_seq@[j]) <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            final(self).purses().dom() =~= old(self).purses().dom(),
            final(self).purses()[p].next_coin_idx
                == old(self).purses()[p].next_coin_idx + exp_seq@.len(),
            final(self).purses()[p].id == p,
            final(self).purses()[p].name == old(self).purses()[p].name,
            final(self).purses()[p].next_entry_idx == old(self).purses()[p].next_entry_idx,
            forall|q: PurseId| q != p && #[trigger] old(self).purses().dom().contains(q)
                ==> final(self).purses()[q] == old(self).purses()[q],
            // Existing coins preserved.
            forall|k: (PurseId, u64)| #[trigger] old(self).coins().dom().contains(k)
                ==> final(self).coins().dom().contains(k)
                    && final(self).coins()[k] == old(self).coins()[k],
            // New coin keys are in the dom; record fields match the request.
            forall|j: int| 0 <= j < exp_seq@.len() ==>
                #[trigger] final(self).coins().dom().contains(
                    (p, (old(self).purses()[p].next_coin_idx + j) as u64)
                )
                && final(self).coins()[
                    (p, (old(self).purses()[p].next_coin_idx + j) as u64)
                ].exponent == exp_seq@[j],
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).events@ == old(self).events@,
    {
        let ghost old_p_next = old(self).purses()[p].next_coin_idx;
        let ghost old_next_age = old(self).next_age;
        let ghost old_purses_map = old(self).purses();
        let ghost old_coins_map = old(self).coins();
        let ghost old_operations_map = old(self).operations();
        let ghost old_operations_vec = old(self).operations@;
        let ghost old_spec_operations = old(self).spec_operations@;
        let ghost old_entries_map = old(self).entries();
        let ghost old_entries_vec = old(self).entries@;
        let ghost old_spec_entries = old(self).spec_entries@;
        let ghost old_next_handle = old(self).next_handle;
        let ghost old_events = old(self).events@;
        let n = exp_seq.len();

        let mut k: usize = 0;
        while k < n
            invariant
                0 <= k <= n,
                n == exp_seq@.len(),
                self.invariant(),
                self.events@ == old_events,
                forall|j: int| 0 <= j < exp_seq@.len() ==>
                    (#[trigger] exp_seq@[j]) <= MAX_EXPONENT,
                self.purses().dom() =~= old_purses_map.dom(),
                old_purses_map.dom().contains(p),
                self.purses()[p].next_coin_idx == old_p_next + k as nat,
                self.purses()[p].id == p,
                self.purses()[p].name == old_purses_map[p].name,
                self.purses()[p].next_entry_idx == old_purses_map[p].next_entry_idx,
                old_p_next == old_purses_map[p].next_coin_idx,
                old_p_next as nat + n as nat <= u64::MAX as nat,
                self.next_age == old_next_age + k as nat,
                old_next_age == old(self).next_age,
                old_next_age as nat + n as nat <= u64::MAX as nat,
                self.operations() == old_operations_map,
                self.operations@ == old_operations_vec,
                self.spec_operations@ == old_spec_operations,
                self.next_handle == old_next_handle,
                self.entries() == old_entries_map,
                self.entries@ == old_entries_vec,
                self.spec_entries@ == old_spec_entries,
                old_operations_map == old(self).operations(),
                old_operations_vec == old(self).operations@,
                old_spec_operations == old(self).spec_operations@,
                old_next_handle == old(self).next_handle,
                old_entries_map == old(self).entries(),
                old_entries_vec == old(self).entries@,
                old_spec_entries == old(self).spec_entries@,
                forall|q: PurseId| q != p && #[trigger] old_purses_map.dom().contains(q)
                    ==> self.purses()[q] == old_purses_map[q],
                forall|key: (PurseId, u64)| #[trigger] old_coins_map.dom().contains(key)
                    ==> self.coins().dom().contains(key)
                        && self.coins()[key] == old_coins_map[key],
                forall|j: int| 0 <= j < k as int ==>
                    #[trigger] self.coins().dom().contains((p, (old_p_next + j) as u64))
                    && self.coins()[(p, (old_p_next + j) as u64)].exponent == exp_seq@[j],
            decreases n - k,
        {
            let exp = exp_seq[k];
            let ghost prev_next_coin_idx = self.purses()[p].next_coin_idx;
            let ghost pre_coins = self.coins();
            assert(prev_next_coin_idx == old_p_next + k as nat);
            assert(prev_next_coin_idx < u64::MAX);
            #[allow(unused_variables)]
            let new_key = self.add_coin(p, exp);
            proof {
                assert(new_key == (p, (old_p_next + k as nat) as u64));
                // Forall j in [0, k+1), the expected key is in coins.dom.
                // j == k is the just-added coin; j < k is an existing coin
                // that survives `insert(new_key, _)` since keys differ.
                assert forall|j: int| 0 <= j < (k + 1) as int implies
                    #[trigger] self.coins().dom().contains((p, (old_p_next + j) as u64))
                    && self.coins()[(p, (old_p_next + j) as u64)].exponent == exp_seq@[j]
                by {
                    let nk = (p, (old_p_next + j) as u64);
                    if j == k as int {
                        assert(nk == new_key);
                        assert(self.coins()[new_key].exponent == exp);
                        assert(exp == exp_seq@[k as int]);
                    } else {
                        assert(j < k as int);
                        assert(pre_coins.dom().contains(nk));
                        assert(pre_coins[nk].exponent == exp_seq@[j]);
                        assert(nk.1 != new_key.1);
                    }
                }
            }
            k += 1;
        }
    }

    /// Reserve: allocate `exp_seq.len()` fresh recycler entries in purse `p`,
    /// one per exponent in `exp_seq` (in order). Mirror of `top_up_purse` for
    /// the entry side. New entries start in `(on_chain=Waiting,
    /// local=LocalAvailable)`.
    pub fn reserve_entries(&mut self, p: PurseId, exp_seq: Vec<u8>)
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(p),
            old(self).purses()[p].next_entry_idx as nat + exp_seq@.len() <= u64::MAX as nat,
            forall|j: int| 0 <= j < exp_seq@.len() ==>
                (#[trigger] exp_seq@[j]) <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            final(self).purses().dom() =~= old(self).purses().dom(),
            final(self).purses()[p].next_entry_idx
                == old(self).purses()[p].next_entry_idx + exp_seq@.len(),
            final(self).purses()[p].id == p,
            final(self).purses()[p].name == old(self).purses()[p].name,
            final(self).purses()[p].next_coin_idx == old(self).purses()[p].next_coin_idx,
            forall|q: PurseId| q != p && #[trigger] old(self).purses().dom().contains(q)
                ==> final(self).purses()[q] == old(self).purses()[q],
            // Coins entirely untouched.
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            // Existing entries preserved.
            forall|k: (PurseId, u64)| #[trigger] old(self).entries().dom().contains(k)
                ==> final(self).entries().dom().contains(k)
                    && final(self).entries()[k] == old(self).entries()[k],
            // New entry keys are in the dom; record fields match the request.
            forall|j: int| 0 <= j < exp_seq@.len() ==>
                #[trigger] final(self).entries().dom().contains(
                    (p, (old(self).purses()[p].next_entry_idx + j) as u64)
                )
                && final(self).entries()[
                    (p, (old(self).purses()[p].next_entry_idx + j) as u64)
                ].exponent == exp_seq@[j],
    {
        let ghost old_p_next = old(self).purses()[p].next_entry_idx;
        let ghost old_purses_map = old(self).purses();
        let ghost old_entries_map = old(self).entries();
        let n = exp_seq.len();

        let mut k: usize = 0;
        while k < n
            invariant
                0 <= k <= n,
                n == exp_seq@.len(),
                self.invariant(),
                forall|j: int| 0 <= j < exp_seq@.len() ==>
                    (#[trigger] exp_seq@[j]) <= MAX_EXPONENT,
                self.purses().dom() =~= old_purses_map.dom(),
                old_purses_map.dom().contains(p),
                self.purses()[p].next_entry_idx == old_p_next + k as nat,
                self.purses()[p].id == p,
                self.purses()[p].name == old_purses_map[p].name,
                self.purses()[p].next_coin_idx == old_purses_map[p].next_coin_idx,
                old_p_next == old_purses_map[p].next_entry_idx,
                old_p_next as nat + n as nat <= u64::MAX as nat,
                forall|q: PurseId| q != p && #[trigger] old_purses_map.dom().contains(q)
                    ==> self.purses()[q] == old_purses_map[q],
                self.coins() == old(self).coins(),
                self.coins@ == old(self).coins@,
                forall|key: (PurseId, u64)| #[trigger] old_entries_map.dom().contains(key)
                    ==> self.entries().dom().contains(key)
                        && self.entries()[key] == old_entries_map[key],
                forall|j: int| 0 <= j < k as int ==>
                    #[trigger] self.entries().dom().contains((p, (old_p_next + j) as u64))
                    && self.entries()[(p, (old_p_next + j) as u64)].exponent == exp_seq@[j],
            decreases n - k,
        {
            let exp = exp_seq[k];
            let ghost prev_next_entry_idx = self.purses()[p].next_entry_idx;
            let ghost pre_entries = self.entries();
            assert(prev_next_entry_idx == old_p_next + k as nat);
            assert(prev_next_entry_idx < u64::MAX);
            #[allow(unused_variables)]
            let new_key = self.add_entry(
                p,
                exp,
                EntryOnChain::Waiting,
                EntryLocal::LocalAvailable,
            );
            proof {
                assert(new_key == (p, (old_p_next + k as nat) as u64));
                assert forall|j: int| 0 <= j < (k + 1) as int implies
                    #[trigger] self.entries().dom().contains((p, (old_p_next + j) as u64))
                    && self.entries()[(p, (old_p_next + j) as u64)].exponent == exp_seq@[j]
                by {
                    let nk = (p, (old_p_next + j) as u64);
                    if j == k as int {
                        assert(nk == new_key);
                        assert(self.entries()[new_key].exponent == exp);
                        assert(exp == exp_seq@[k as int]);
                    } else {
                        assert(j < k as int);
                        assert(pre_entries.dom().contains(nk));
                        assert(pre_entries[nk].exponent == exp_seq@[j]);
                        assert(nk.1 != new_key.1);
                    }
                }
            }
            k += 1;
        }
    }

    /// Sum of `coin_value(exp)` across entries in purse `p` that are
    /// LocalAvailable and Ready on-chain. Quint analog: the entry
    /// component of `purseSpendableStrict(p)`.
    fn sum_ready_in(&self, p: PurseId) -> (sum: u64)
        requires
            self.invariant(),
            self.entries@.len() <= (u64::MAX / 1073741824) as nat,
        ensures
            sum as nat == sum_ready_prefix(self.entries@, p, self.entries@.len() as nat),
            sum as nat <= self.entries@.len() as nat * 1073741824,
    {
        let mut sum: u64 = 0;
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.entries@.len() <= (u64::MAX / 1073741824) as nat,
                self.invariant(),
                sum as nat == sum_ready_prefix(self.entries@, p, j as nat),
                sum as nat <= (j as nat) * 1073741824,
            decreases self.entries.len() - j,
        {
            let e = &self.entries[j];
            let is_local_avail = matches!(e.local, EntryLocal::LocalAvailable);
            let is_ready = matches!(e.on_chain, EntryOnChain::Ready);
            proof {
                let entry_key = (self.entries@[j as int].purse,
                                 self.entries@[j as int].idx);
                assert(self.spec_entries@.dom().contains(entry_key));
                assert(self.spec_entries@[entry_key] == self.entries@[j as int]);
                assert(self.entries@[j as int].exponent <= MAX_EXPONENT);
                lemma_pow2_at_30();
                lemma_pow2_monotone(self.entries@[j as int].exponent as nat,
                                    MAX_EXPONENT as nat);
                assert(sum_ready_prefix(self.entries@, p, (j + 1) as nat)
                    <= sum_ready_prefix(self.entries@, p, j as nat) + 1073741824);
            }
            if e.purse == p && is_local_avail && is_ready {
                let value: u64 = pow2_u64_exec(e.exponent);
                sum = sum + value;
            }
            j = j + 1;
        }
        sum
    }

    /// Sum of `coin_value(exp)` across entries in purse `p` that are
    /// LocalAvailable and on-chain in {Waiting, Missing} — i.e. pending
    /// recycler-floor confirmation. Quint analog: `pursePending(p)`.
    fn sum_pending_in(&self, p: PurseId) -> (sum: u64)
        requires
            self.invariant(),
            self.entries@.len() <= (u64::MAX / 1073741824) as nat,
        ensures
            sum as nat == sum_pending_prefix(self.entries@, p, self.entries@.len() as nat),
    {
        let mut sum: u64 = 0;
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.entries@.len() <= (u64::MAX / 1073741824) as nat,
                self.invariant(),
                sum as nat == sum_pending_prefix(self.entries@, p, j as nat),
                sum as nat <= (j as nat) * 1073741824,
            decreases self.entries.len() - j,
        {
            let e = &self.entries[j];
            let is_local_avail = matches!(e.local, EntryLocal::LocalAvailable);
            let is_waiting = matches!(e.on_chain, EntryOnChain::Waiting);
            let is_missing = matches!(e.on_chain, EntryOnChain::Missing);
            proof {
                let entry_key = (self.entries@[j as int].purse,
                                 self.entries@[j as int].idx);
                assert(self.spec_entries@.dom().contains(entry_key));
                assert(self.spec_entries@[entry_key] == self.entries@[j as int]);
                assert(self.entries@[j as int].exponent <= MAX_EXPONENT);
                lemma_pow2_at_30();
                lemma_pow2_monotone(self.entries@[j as int].exponent as nat,
                                    MAX_EXPONENT as nat);
                assert(sum_pending_prefix(self.entries@, p, (j + 1) as nat)
                    <= sum_pending_prefix(self.entries@, p, j as nat) + 1073741824);
            }
            if e.purse == p && is_local_avail && (is_waiting || is_missing) {
                let value: u64 = pow2_u64_exec(e.exponent);
                sum = sum + value;
            }
            j = j + 1;
        }
        sum
    }

    /// Real-value (2^exp) variant of `sum_pending_in`. Used by callers
    /// that want production-scheme purse-pending totals.
    pub fn sum_pending_real_in(&self, p: PurseId) -> (sum: u64)
        requires
            self.invariant(),
            forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                ==> self.entries()[k].exponent <= MAX_EXPONENT,
            self.entries@.len() <= (u64::MAX / 1073741824) as nat,
        ensures
            sum as nat == sum_pending_real_prefix(self.entries@, p,
                                                  self.entries@.len() as nat),
            sum as nat <= self.entries@.len() as nat * 1073741824,
    {
        let mut sum: u64 = 0;
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.entries@.len() <= (u64::MAX / 1073741824) as nat,
                sum as nat == sum_pending_real_prefix(self.entries@, p, j as nat),
                sum as nat <= (j as nat) * 1073741824,
                forall|k: (PurseId, u64)|
                    #[trigger] self.entries().dom().contains(k)
                    ==> self.entries()[k].exponent <= MAX_EXPONENT,
                self.invariant(),
            decreases self.entries.len() - j,
        {
            let e = &self.entries[j];
            let is_local_avail = matches!(e.local, EntryLocal::LocalAvailable);
            let is_waiting = matches!(e.on_chain, EntryOnChain::Waiting);
            let is_missing = matches!(e.on_chain, EntryOnChain::Missing);
            proof {
                let entry_key = (self.entries@[j as int].purse,
                                 self.entries@[j as int].idx);
                assert(self.spec_entries@.dom().contains(entry_key));
                assert(self.entries()[entry_key] == self.entries@[j as int]);
                assert(self.entries()[entry_key].exponent <= MAX_EXPONENT);
                assert(self.entries@[j as int].exponent <= MAX_EXPONENT);
                lemma_pow2_at_30();
                lemma_pow2_monotone(self.entries@[j as int].exponent as nat, 30);
                assert(sum_pending_real_prefix(self.entries@, p, (j + 1) as nat)
                    <= sum_pending_real_prefix(self.entries@, p, j as nat) + 1073741824);
            }
            if e.purse == p && is_local_avail && (is_waiting || is_missing) {
                let value = pow2_u64_exec(e.exponent);
                sum = sum + value;
            }
            j = j + 1;
        }
        sum
    }

    /// Real-value (2^exp) variant of `sum_ready_in`.
    pub fn sum_ready_real_in(&self, p: PurseId) -> (sum: u64)
        requires
            self.invariant(),
            forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                ==> self.entries()[k].exponent <= MAX_EXPONENT,
            self.entries@.len() <= (u64::MAX / 1073741824) as nat,
        ensures
            sum as nat == sum_ready_real_prefix(self.entries@, p,
                                                self.entries@.len() as nat),
            sum as nat <= self.entries@.len() as nat * 1073741824,
    {
        let mut sum: u64 = 0;
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.entries@.len() <= (u64::MAX / 1073741824) as nat,
                sum as nat == sum_ready_real_prefix(self.entries@, p, j as nat),
                sum as nat <= (j as nat) * 1073741824,
                forall|k: (PurseId, u64)|
                    #[trigger] self.entries().dom().contains(k)
                    ==> self.entries()[k].exponent <= MAX_EXPONENT,
                self.invariant(),
            decreases self.entries.len() - j,
        {
            let e = &self.entries[j];
            let is_local_avail = matches!(e.local, EntryLocal::LocalAvailable);
            let is_ready = matches!(e.on_chain, EntryOnChain::Ready);
            proof {
                let entry_key = (self.entries@[j as int].purse,
                                 self.entries@[j as int].idx);
                assert(self.spec_entries@.dom().contains(entry_key));
                assert(self.entries()[entry_key] == self.entries@[j as int]);
                assert(self.entries()[entry_key].exponent <= MAX_EXPONENT);
                assert(self.entries@[j as int].exponent <= MAX_EXPONENT);
                lemma_pow2_at_30();
                lemma_pow2_monotone(self.entries@[j as int].exponent as nat, 30);
                assert(sum_ready_real_prefix(self.entries@, p, (j + 1) as nat)
                    <= sum_ready_real_prefix(self.entries@, p, j as nat) + 1073741824);
            }
            if e.purse == p && is_local_avail && is_ready {
                let value = pow2_u64_exec(e.exponent);
                sum = sum + value;
            }
            j = j + 1;
        }
        sum
    }

    /// Sum of **real** `coin_value_pow2(exp) = 2^exp` across `Available`
    /// coins in purse `p`. Companion to `sum_available_in` (pilot scheme).
    /// Returned sum equals `sum_avail_real_prefix(self.coins@, p, len)`.
    ///
    /// Preconditions:
    /// - Every coin in the state has `exponent <= MAX_EXPONENT` (= 30),
    ///   so each coin value <= 2^30.
    /// - Vec length bounded so the cumulative u64 sum (≤ len · 2^30)
    ///   stays within u64::MAX.
    pub fn sum_available_real_in(&self, p: PurseId) -> (sum: u64)
        requires
            self.invariant(),
            forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                ==> self.coins()[k].exponent <= MAX_EXPONENT,
            self.coins@.len() <= (u64::MAX / 1073741824) as nat,
        ensures
            sum as nat == sum_avail_real_prefix(self.coins@, p, self.coins@.len() as nat),
            sum as nat <= self.coins@.len() as nat * 1073741824,
    {
        let mut sum: u64 = 0;
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.coins@.len() <= (u64::MAX / 1073741824) as nat,
                sum as nat == sum_avail_real_prefix(self.coins@, p, j as nat),
                sum as nat <= (j as nat) * 1073741824,
                forall|k: (PurseId, u64)|
                    #[trigger] self.coins().dom().contains(k)
                    ==> self.coins()[k].exponent <= MAX_EXPONENT,
                self.invariant(),
            decreases self.coins.len() - j,
        {
            let is_available = matches!(self.coins[j].state, CoinState::Available);
            proof {
                // Per-step increment is at most 2^30, bounded by the
                // global exponent constraint via invariant (l).
                assert(self.spec_coins@.dom().contains(
                    (self.coins@[j as int].purse, self.coins@[j as int].idx)
                ));
                let coin_key = (self.coins@[j as int].purse, self.coins@[j as int].idx);
                assert(self.coins()[coin_key].exponent
                    == self.coins@[j as int].exponent);
                assert(self.coins()[coin_key].exponent <= MAX_EXPONENT);
                assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
                lemma_pow2_at_30();
                lemma_pow2_monotone(self.coins@[j as int].exponent as nat, 30);
                assert(sum_avail_real_prefix(self.coins@, p, (j + 1) as nat)
                    <= sum_avail_real_prefix(self.coins@, p, j as nat) + 1073741824);
            }
            if self.coins[j].purse == p && is_available {
                let value: u64 = pow2_u64_exec(self.coins[j].exponent);
                sum = sum + value;
            }
            j = j + 1;
        }
        sum
    }

    /// Sum of `coin_value(exp)` across `Available` coins in purse `p`.
    /// Scans the coin Vec; returned sum equals `sum_avail_prefix(self.coins@,
    /// p, len)`.
    ///
    /// **Pilot value scheme:** `coin_value(exp) = exp + 1` (linear). Real
    /// `coinValue(exp) = 2^exp` is deferred. Precondition bounds Vec size to
    /// keep the cumulative `u64` sum safe.
    fn sum_available_in(&self, p: PurseId) -> (sum: u64)
        requires
            self.invariant(),
            // With coin_value(exp) <= 2^30, sum is bounded by len * 2^30.
            // Bound Vec length to ensure no u64 overflow.
            self.coins@.len() <= (u64::MAX / 1073741824) as nat,
        ensures
            sum as nat == sum_avail_prefix(self.coins@, p, self.coins@.len() as nat),
            sum as nat <= self.coins@.len() as nat * 1073741824,
    {
        let mut sum: u64 = 0;
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.coins@.len() <= (u64::MAX / 1073741824) as nat,
                self.invariant(),
                sum as nat == sum_avail_prefix(self.coins@, p, j as nat),
                sum as nat <= (j as nat) * 1073741824,
            decreases self.coins.len() - j,
        {
            let is_available = matches!(self.coins[j].state, CoinState::Available);
            proof {
                let coin_key = (self.coins@[j as int].purse, self.coins@[j as int].idx);
                assert(self.spec_coins@.dom().contains(coin_key));
                assert(self.spec_coins@[coin_key] == self.coins@[j as int]);
                assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
                lemma_pow2_at_30();
                lemma_pow2_monotone(self.coins@[j as int].exponent as nat,
                                    MAX_EXPONENT as nat);
                // Per-step increment is at most coin_value(_) <= 2^30, so the
                // monotone bound `sum_avail_prefix(_, _, j+1) <= (j+1) * 2^30`
                // is preserved.
                assert(sum_avail_prefix(self.coins@, p, (j + 1) as nat)
                    <= sum_avail_prefix(self.coins@, p, j as nat) + 1073741824);
            }
            if self.coins[j].purse == p && is_available {
                let value: u64 = pow2_u64_exec(self.coins[j].exponent);
                sum = sum + value;
            }
            j = j + 1;
        }
        sum
    }

    /// Convenience: sum of `Available` coins + ALL LocalAvailable
    /// entries (Ready + Waiting + Missing), using real `2^exp` values.
    /// Quint analog: `spendableWhenReady(p) = purseSpendable(p) +
    /// pursePending(p)`.
    ///
    /// Used to distinguish "insufficient funds now" from "insufficient
    /// even if all in-flight top-ups mature".
    pub fn spendable_when_ready_real(&self, p: PurseId) -> (total: u64)
        requires
            self.invariant(),
            forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                ==> self.coins()[k].exponent <= MAX_EXPONENT,
            forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                ==> self.entries()[k].exponent <= MAX_EXPONENT,
            self.coins@.len() <= (u64::MAX / 1073741824) as nat,
            self.entries@.len() <= (u64::MAX / 1073741824) as nat,
            (self.coins@.len() as nat + self.entries@.len() as nat)
                <= (u64::MAX / 1073741824) as nat,
        ensures
            total as nat ==
                sum_avail_real_prefix(self.coins@, p, self.coins@.len() as nat)
                + sum_pending_real_prefix(self.entries@, p, self.entries@.len() as nat),
    {
        let spendable = self.sum_available_real_in(p);
        let pending = self.sum_pending_real_in(p);
        proof {
            assert(spendable as nat <= self.coins@.len() as nat * 1073741824);
            assert(pending as nat <= self.entries@.len() as nat * 1073741824);
        }
        spendable + pending
    }

    /// Real-value (2^exp) variant of [`Self::query_purse`]. Reports
    /// `spendable`, `spendable_strict`, and `pending` using Quint's
    /// production `coinValue = 2^exp` arithmetic via the
    /// `sum_*_real_in` aggregations. Requires all exponents in state
    /// to satisfy MAX_EXPONENT and the Vec sizes to fit cumulative
    /// u64 sums.
    pub fn query_purse_real(&self, p: PurseId) -> (info: Result<PurseInfo, Error>)
        requires
            self.invariant(),
            forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                ==> self.coins()[k].exponent <= MAX_EXPONENT,
            forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                ==> self.entries()[k].exponent <= MAX_EXPONENT,
            self.coins@.len() <= (u64::MAX / 1073741824) as nat,
            self.entries@.len() <= (u64::MAX / 1073741824) as nat,
            (self.coins@.len() as nat + self.entries@.len() as nat)
                <= (u64::MAX / 1073741824) as nat,
        ensures
            match info {
                Ok(i) =>
                    self.purses().dom().contains(p)
                    && i.id == p
                    && i.name@ == self.purses()[p].name
                    && i.spendable as nat
                        == sum_avail_real_prefix(self.coins@, p, self.coins@.len() as nat)
                    && i.spendable_strict as nat
                        == sum_avail_real_prefix(self.coins@, p, self.coins@.len() as nat)
                            + sum_ready_real_prefix(self.entries@, p,
                                                    self.entries@.len() as nat)
                    && i.pending as nat
                        == sum_pending_real_prefix(self.entries@, p,
                                                   self.entries@.len() as nat),
                Err(Error::PurseNotFound(q)) =>
                    !self.purses().dom().contains(p) && q == p,
                Err(_) => false,
            },
    {
        let mut i: usize = 0;
        while i < self.purses.len()
            invariant
                0 <= i <= self.purses.len(),
                self.invariant(),
                forall|k: (PurseId, u64)|
                    #[trigger] self.coins().dom().contains(k)
                    ==> self.coins()[k].exponent <= MAX_EXPONENT,
                forall|k: (PurseId, u64)|
                    #[trigger] self.entries().dom().contains(k)
                    ==> self.entries()[k].exponent <= MAX_EXPONENT,
                self.coins@.len() <= (u64::MAX / 1073741824) as nat,
                self.entries@.len() <= (u64::MAX / 1073741824) as nat,
                (self.coins@.len() as nat + self.entries@.len() as nat)
                    <= (u64::MAX / 1073741824) as nat,
                forall|j: int| 0 <= j < i ==> (#[trigger] self.purses@[j]).id != p,
            decreases self.purses.len() - i,
        {
            if self.purses[i].id == p {
                let spendable = self.sum_available_real_in(p);
                let ready = self.sum_ready_real_in(p);
                let pending = self.sum_pending_real_in(p);
                proof {
                    assert(spendable as nat <= self.coins@.len() as nat * 1073741824);
                    assert(ready as nat <= self.entries@.len() as nat * 1073741824);
                }
                let rec = &self.purses[i];
                let name_copy: Vec<u8> = rec.name.clone();
                assert(name_copy@ == rec.name@);
                return Ok(PurseInfo {
                    id: rec.id,
                    name: name_copy,
                    spendable,
                    spendable_strict: spendable + ready,
                    pending,
                });
            }
            i += 1;
        }
        Err(Error::PurseNotFound(p))
    }

    /// 6.1 `queryPurse` (Quint lines 603-612; design §8.1 `query_purse`).
    ///
    /// Returns a synchronous snapshot:
    /// - `spendable`        — sum of Available-coin values in `p`.
    /// - `spendable_strict` — `spendable + sum of Ready-entry values`
    ///                        (entries fully matured into the
    ///                        anonymity ring).
    /// - `pending`          — sum of LocalAvailable entries in `p`
    ///                        that are Waiting or Missing on-chain
    ///                        (in-flight top-ups not yet matured).
    ///
    /// Preconditions bound coin / entry Vec sizes so the cumulative
    /// `u64` aggregations don't overflow under the pilot value scheme.
    pub fn query_purse(&self, p: PurseId) -> (info: Result<PurseInfo, Error>)
        requires
            self.invariant(),
            self.coins@.len() <= (u64::MAX / 1073741824) as nat,
            self.entries@.len() <= (u64::MAX / 1073741824) as nat,
            // spendable + ready_entries must fit in u64.
            (self.coins@.len() as nat + self.entries@.len() as nat)
                <= (u64::MAX / 1073741824) as nat,
        ensures
            match info {
                Ok(i) =>
                    self.purses().dom().contains(p)
                    && i.id == p
                    && i.name@ == self.purses()[p].name
                    && i.spendable as nat
                        == sum_avail_prefix(self.coins@, p, self.coins@.len() as nat)
                    && i.spendable_strict as nat
                        == sum_avail_prefix(self.coins@, p, self.coins@.len() as nat)
                            + sum_ready_prefix(self.entries@, p,
                                               self.entries@.len() as nat)
                    && i.pending as nat
                        == sum_pending_prefix(self.entries@, p,
                                              self.entries@.len() as nat),
                Err(Error::PurseNotFound(q)) =>
                    !self.purses().dom().contains(p) && q == p,
                Err(_) => false,
            },
    {
        let mut i: usize = 0;
        while i < self.purses.len()
            invariant
                0 <= i <= self.purses.len(),
                self.invariant(),
                self.coins@.len() <= (u64::MAX / 1073741824) as nat,
                self.entries@.len() <= (u64::MAX / 1073741824) as nat,
                (self.coins@.len() as nat + self.entries@.len() as nat)
                    <= (u64::MAX / 1073741824) as nat,
                forall|j: int| 0 <= j < i ==> (#[trigger] self.purses@[j]).id != p,
            decreases
                self.purses.len() - i,
        {
            if self.purses[i].id == p {
                let spendable = self.sum_available_in(p);
                let ready = self.sum_ready_in(p);
                let pending = self.sum_pending_in(p);
                proof {
                    // sum_avail_prefix is bounded by len * 2^30; same for ready.
                    // Together they fit in u64 because (coins.len + entries.len)
                    // <= u64::MAX/2^30 was given by the precondition.
                    assert(spendable as nat <= self.coins@.len() as nat * 1073741824);
                    assert(ready as nat <= self.entries@.len() as nat * 1073741824);
                }
                let rec = &self.purses[i];
                let name_copy: Vec<u8> = rec.name.clone();
                assert(name_copy@ == rec.name@);
                return Ok(PurseInfo {
                    id: rec.id,
                    name: name_copy,
                    spendable,
                    spendable_strict: spendable + ready,
                    pending,
                });
            }
            i += 1;
        }
        Err(Error::PurseNotFound(p))
    }
}

} // verus!
