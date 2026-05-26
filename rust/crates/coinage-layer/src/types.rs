//! Core types, constants, and tag enums for the coinage layer.
//!
//! Contains:
//! - Public constants (`MAIN_PURSE`, `MAX_EXPONENT`, jitter/recovery sizes).
//! - The four executable record types (`PurseRec`, `CoinRec`, `EntryRec`,
//!   `OperationRec`) and their spec twins.
//! - Lifecycle tag enums (`CoinState`, `EntryOnChain`, `EntryLocal`,
//!   `OpStatus`, `OpKind`).
//! - The result and error types (`PurseInfo`, `Error`, `CoinSelection`,
//!   `SubsetSumCover`, `Tier3Cover`, `PaymentClassification`, `FeeMode`,
//!   `UnloadToken`, `Event`).
//! - The central `State` struct.
//!
//! No methods on `State` live here — see the `state_*` modules.

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

/// Result of a tier-3 entry-supplemented cover search. Carries either
/// a pure-coin subset, a pure-entry subset, or a mixed coin+entry
/// subset whose values sum exactly to the requested amount. Returned
/// by [`State::find_tier3_cover_up_to_3`].
///
/// Naming convention: `CkEm` denotes k coins and m entries.
pub enum Tier3Cover {
    C1((PurseId, u64)),
    E1((PurseId, u64)),
    C2((PurseId, u64), (PurseId, u64)),
    C1E1((PurseId, u64), (PurseId, u64)),
    E2((PurseId, u64), (PurseId, u64)),
    C3((PurseId, u64), (PurseId, u64), (PurseId, u64)),
    C2E1((PurseId, u64), (PurseId, u64), (PurseId, u64)),
    C1E2((PurseId, u64), (PurseId, u64), (PurseId, u64)),
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

} // verus!
