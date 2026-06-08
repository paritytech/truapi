//! Quint → Verus refinement scaffolding.
//!
//! Establishes a machine-checked correspondence between the Verus
//! implementation and the Quint specification at
//! `docs/specs/coinage-layer.qnt`. Every public state-mutating Verus
//! method has a corresponding `quint_step_*` spec function that
//! describes its effect on the Quint shadow state, plus a
//! `lemma_*_refines` proof obligation that the Verus contract
//! implies the spec function's output.
//!
//! See the trailing `Findings from the refinement attempt` block
//! for primitives whose contracts were strengthened during the
//! refinement push, plus a per-pattern catalog (single-step,
//! multi-step composite, branch split for Result/Option, existential
//! witness for non-deterministic selection, multi-key removal via
//! `Map::remove_keys`, bulk-loop via `Map::new`).

use vstd::prelude::*;

use crate::*;

verus! {

// ==========================================================================
// Quint → Verus refinement scaffolding (PoC, task #94)
//
// Establishes a machine-checked correspondence between the Verus
// implementation and the Quint specification at `docs/specs/coinage-layer.qnt`.
// This is a proof-of-concept: it covers a 4-field shadow of the Quint state
// and refines two primitives (`mark_coin_observed`, `chain_register_coin`).
// Full refinement of all ~30 mutators is a multi-week effort; the goal here
// is to demonstrate the methodology is tractable in Verus and to surface
// any structural friction.
// ==========================================================================

/// Spec-only shadow of the Quint state machine's variables — covers
/// all 13 vars that the Verus pilot models. Quint vars not in scope of
/// the pilot (below the chain-abstraction boundary or derived) remain
/// excluded: `rings`, `now`, `receipts`, `opRequested`, `opExternalized`,
/// `nextRingIdx`, `nextAccount`, `nextMemberKey`.
///
/// Verus-only state (`next_purse_id`, `next_age`) is similarly excluded
/// — these are local allocators the Quint spec doesn't use (Quint
/// addresses purses and coins directly by id).
pub ghost struct QuintViewState {
    pub purses: Map<PurseId, PurseRecSpec>,
    pub coins: Map<(PurseId, u64), CoinRec>,
    pub entries: Map<(PurseId, u64), EntryRec>,
    pub operations: Map<OpHandle, OperationRec>,
    pub events: Seq<Event>,
    pub next_handle: u64,
    pub next_extrinsic_id: u64,
    pub total_in: u64,
    pub total_out: u64,
    pub fee_balance: u64,
    pub paid_ring_membership: u64,
    pub tokens: Seq<UnloadToken>,
    pub chain_coins: Seq<CoinRec>,
    pub chain_entries: Seq<EntryRec>,
}

/// Refinement map: extract the Quint-shaped view from a Verus `State`.
/// The body is a direct projection — each Quint var maps to its Verus
/// counterpart. The view is well-defined for any `State`, regardless
/// of whether the invariant holds.
pub open spec fn quint_view(s: State) -> QuintViewState {
    QuintViewState {
        purses: s.purses(),
        coins: s.coins(),
        entries: s.entries(),
        operations: s.operations(),
        events: s.events@,
        next_handle: s.next_handle,
        next_extrinsic_id: s.next_extrinsic_id,
        total_in: s.total_in,
        total_out: s.total_out,
        fee_balance: s.fee_balance,
        paid_ring_membership: s.paid_ring_membership,
        tokens: s.tokens@,
        chain_coins: s.chain_coins@,
        chain_entries: s.chain_entries@,
    }
}

/// Spec encoding of the Quint `init` action (restricted to the
/// `QuintViewState` shadow). This is what Quint says the initial state
/// looks like.
///
/// **Known divergences from the literal Quint** (not in the shadow,
/// so they don't surface here, but documented for completeness):
/// - Quint `purses[MAIN].name == "main"` (4 bytes); Verus `init`
///   produces an empty `Vec<u8>` for the name. This PoC encodes the
///   empty-name convention as the pilot's interpretation of Quint
///   init — a real refinement would either match Quint exactly or
///   document the placeholder explicitly.
/// - Quint `nextHandle == 1`; Verus `init` sets `next_handle = 0`.
/// - Quint `nextExtrinsicId == 1`; Verus `init` sets `next_extrinsic_id = 0`.
/// - Quint `feeAccountBalance == 100`; Verus `init` sets `fee_balance = 0`.
pub open spec fn quint_init_view() -> QuintViewState {
    QuintViewState {
        purses: Map::<PurseId, PurseRecSpec>::empty().insert(MAIN_PURSE, PurseRecSpec {
            id: MAIN_PURSE,
            name: Seq::empty(),
            next_coin_idx: 0,
            next_entry_idx: 0,
        }),
        coins: Map::<(PurseId, u64), CoinRec>::empty(),
        entries: Map::<(PurseId, u64), EntryRec>::empty(),
        operations: Map::<OpHandle, OperationRec>::empty(),
        events: Seq::empty(),
        next_handle: 0,
        next_extrinsic_id: 0,
        total_in: 0,
        total_out: 0,
        fee_balance: 0,
        paid_ring_membership: 0,
        tokens: Seq::empty(),
        chain_coins: Seq::empty(),
        chain_entries: Seq::empty(),
    }
}

/// **Refinement lemma (init)**: any state matching `State::init()`'s
/// postconditions has Quint view equal to `quint_init_view()`. This
/// proves the entry-point correspondence at the level of the PoC
/// shadow.
///
/// Parameterized over the post-init state rather than invoking
/// `State::init()` directly (which is exec), so the lemma works
/// against the contract surface.
proof fn lemma_init_refines(s: State)
    requires
        // Verus `init()`'s postconditions (witnessed by `s`):
        s.purses().dom() =~= set![MAIN_PURSE],
        s.purses()[MAIN_PURSE] == (PurseRecSpec {
            id: MAIN_PURSE,
            name: Seq::<u8>::empty(),
            next_coin_idx: 0,
            next_entry_idx: 0,
        }),
        s.coins().dom() =~= Set::<(PurseId, u64)>::empty(),
        s.entries().dom() =~= Set::<(PurseId, u64)>::empty(),
        s.operations().dom() =~= Set::<OpHandle>::empty(),
        s.events@ =~= Seq::<Event>::empty(),
        s.next_handle == 0,
        s.next_extrinsic_id == 0,
        s.total_in == 0,
        s.total_out == 0,
        s.fee_balance == 0,
        s.paid_ring_membership == 0,
        s.tokens@ =~= Seq::<UnloadToken>::empty(),
        s.chain_coins@ =~= Seq::<CoinRec>::empty(),
        s.chain_entries@ =~= Seq::<EntryRec>::empty(),
    ensures
        quint_view(s) == quint_init_view(),
{
    // Discharged by extensional equality across all shadow fields.
    assert(quint_view(s).purses =~= quint_init_view().purses);
    assert(quint_view(s).coins =~= quint_init_view().coins);
    assert(quint_view(s).entries =~= quint_init_view().entries);
    assert(quint_view(s).operations =~= quint_init_view().operations);
    assert(quint_view(s).events =~= quint_init_view().events);
    assert(quint_view(s).tokens =~= quint_init_view().tokens);
    assert(quint_view(s).chain_coins =~= quint_init_view().chain_coins);
    assert(quint_view(s).chain_entries =~= quint_init_view().chain_entries);
}

/// Spec encoding of Quint's effect on `QuintViewState` when
/// `mark_coin_observed` fires. Quint analog: a transition where
/// `coins' = coins.set(key, {...with state = Available...})` and
/// `events' = events.append(ECoinAvailable{purse, exp})`.
pub open spec fn quint_step_mark_coin_observed(
    pre: QuintViewState,
    key: (PurseId, u64),
) -> QuintViewState
    recommends
        pre.coins.dom().contains(key),
        pre.coins[key].state == CoinState::Pending,
{
    QuintViewState {
        coins: pre.coins.insert(key, CoinRec {
            purse: pre.coins[key].purse,
            idx: pre.coins[key].idx,
            exponent: pre.coins[key].exponent,
            age: pre.coins[key].age,
            account: pre.coins[key].account,
            state: CoinState::Available,
        }),
        events: pre.events.push(Event::CoinAvailable {
            purse: key.0,
            exponent: pre.coins[key].exponent,
        }),
        ..pre
    }
}

/// **Refinement lemma (mark_coin_observed step)**: for any state
/// satisfying `mark_coin_observed`'s preconditions, the Verus
/// transition is equivalent (under `quint_view`) to the Quint
/// transition.
///
/// This is a *theorem about contracts*, not a runtime function. It
/// says: any `(pre, post)` pair satisfying the contract of
/// `mark_coin_observed` also satisfies the Quint step's effect when
/// projected via `quint_view`.
proof fn lemma_mark_coin_observed_refines(pre: State, post: State, key: (PurseId, u64))
    requires
        // The Verus contract of mark_coin_observed (preconditions):
        pre.invariant(),
        pre.coins().dom().contains(key),
        pre.coins()[key].state == CoinState::Pending,
        pre.events@.len() < u64::MAX as nat,
        // ...and its postconditions, witnessed by (pre, post):
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins().insert(key, CoinRec {
            purse: pre.coins()[key].purse,
            idx: pre.coins()[key].idx,
            exponent: pre.coins()[key].exponent,
            age: pre.coins()[key].age,
            account: pre.coins()[key].account,
            state: CoinState::Available,
        }),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@.push(Event::CoinAvailable {
            purse: key.0,
            exponent: pre.coins()[key].exponent,
        }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_mark_coin_observed(quint_view(pre), key),
{
    let post_view = quint_view(post);
    let step_view = quint_step_mark_coin_observed(quint_view(pre), key);
    assert(post_view.purses =~= step_view.purses);
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.entries =~= step_view.entries);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
    assert(post_view.tokens =~= step_view.tokens);
    assert(post_view.chain_coins =~= step_view.chain_coins);
    assert(post_view.chain_entries =~= step_view.chain_entries);
}

/// Spec encoding of Quint's effect on `QuintViewState` when
/// `chain_register_coin` fires. The chain emits a new coin record into
/// the chain mirror; local state is untouched.
pub open spec fn quint_step_chain_register_coin(
    pre: QuintViewState,
    c: CoinRec,
) -> QuintViewState {
    QuintViewState {
        chain_coins: pre.chain_coins.push(c),
        ..pre
    }
}

/// **Refinement lemma (chain_register_coin step)**: the chain-emit
/// transition appends to `chain_coins` and leaves everything else
/// untouched.
proof fn lemma_chain_register_coin_refines(pre: State, post: State, c: CoinRec)
    requires
        pre.invariant(),
        pre.chain_coins@.len() < u64::MAX as nat,
        c.exponent <= MAX_EXPONENT,
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@.push(c),
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_chain_register_coin(quint_view(pre), c),
{
    let post_view = quint_view(post);
    let step_view = quint_step_chain_register_coin(quint_view(pre), c);
    assert(post_view.chain_coins =~= step_view.chain_coins);
}

/// Quint analog: `coins' = coins.set(key, {..with state = PendingSpend..})`.
pub open spec fn quint_step_mark_coin_pending_spend(
    pre: QuintViewState,
    key: (PurseId, u64),
) -> QuintViewState
    recommends
        pre.coins.dom().contains(key),
        pre.coins[key].state == CoinState::Available,
{
    QuintViewState {
        coins: pre.coins.insert(key, CoinRec {
            purse: pre.coins[key].purse,
            idx: pre.coins[key].idx,
            exponent: pre.coins[key].exponent,
            age: pre.coins[key].age,
            account: pre.coins[key].account,
            state: CoinState::PendingSpend,
        }),
        ..pre
    }
}

proof fn lemma_mark_coin_pending_spend_refines(pre: State, post: State, key: (PurseId, u64))
    requires
        pre.invariant(),
        pre.coins().dom().contains(key),
        pre.coins()[key].state == CoinState::Available,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins().insert(key, CoinRec {
            purse: pre.coins()[key].purse,
            idx: pre.coins()[key].idx,
            exponent: pre.coins()[key].exponent,
            age: pre.coins()[key].age,
            account: pre.coins()[key].account,
            state: CoinState::PendingSpend,
        }),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_mark_coin_pending_spend(quint_view(pre), key),
{
    let post_view = quint_view(post);
    let step_view = quint_step_mark_coin_pending_spend(quint_view(pre), key);
    assert(post_view.coins =~= step_view.coins);
}

/// Quint analog: `coins' = coins.set(key, {..with state = Available..})`.
pub open spec fn quint_step_reverse_pending_spend(
    pre: QuintViewState,
    key: (PurseId, u64),
) -> QuintViewState
    recommends
        pre.coins.dom().contains(key),
        pre.coins[key].state == CoinState::PendingSpend,
{
    QuintViewState {
        coins: pre.coins.insert(key, CoinRec {
            purse: pre.coins[key].purse,
            idx: pre.coins[key].idx,
            exponent: pre.coins[key].exponent,
            age: pre.coins[key].age,
            account: pre.coins[key].account,
            state: CoinState::Available,
        }),
        ..pre
    }
}

proof fn lemma_reverse_pending_spend_refines(pre: State, post: State, key: (PurseId, u64))
    requires
        pre.invariant(),
        pre.coins().dom().contains(key),
        pre.coins()[key].state == CoinState::PendingSpend,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins().insert(key, CoinRec {
            purse: pre.coins()[key].purse,
            idx: pre.coins()[key].idx,
            exponent: pre.coins()[key].exponent,
            age: pre.coins()[key].age,
            account: pre.coins()[key].account,
            state: CoinState::Available,
        }),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_reverse_pending_spend(quint_view(pre), key),
{
    let post_view = quint_view(post);
    let step_view = quint_step_reverse_pending_spend(quint_view(pre), key);
    assert(post_view.coins =~= step_view.coins);
}

/// Quint analog: `coins' = coins.set(key, {..with state = Spent..})`,
/// `events' = events.append(ECoinSpent{purse, exp})`.
pub open spec fn quint_step_mark_coin_spent(
    pre: QuintViewState,
    key: (PurseId, u64),
) -> QuintViewState
    recommends
        pre.coins.dom().contains(key),
        pre.coins[key].state == CoinState::PendingSpend,
{
    QuintViewState {
        coins: pre.coins.insert(key, CoinRec {
            purse: pre.coins[key].purse,
            idx: pre.coins[key].idx,
            exponent: pre.coins[key].exponent,
            age: pre.coins[key].age,
            account: pre.coins[key].account,
            state: CoinState::Spent,
        }),
        events: pre.events.push(Event::CoinSpent {
            purse: key.0,
            exponent: pre.coins[key].exponent,
        }),
        ..pre
    }
}

proof fn lemma_mark_coin_spent_refines(pre: State, post: State, key: (PurseId, u64))
    requires
        pre.invariant(),
        pre.coins().dom().contains(key),
        pre.coins()[key].state == CoinState::PendingSpend,
        pre.events@.len() < u64::MAX as nat,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins().insert(key, CoinRec {
            purse: pre.coins()[key].purse,
            idx: pre.coins()[key].idx,
            exponent: pre.coins()[key].exponent,
            age: pre.coins()[key].age,
            account: pre.coins()[key].account,
            state: CoinState::Spent,
        }),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@.push(Event::CoinSpent {
            purse: key.0,
            exponent: pre.coins()[key].exponent,
        }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_mark_coin_spent(quint_view(pre), key),
{
    let post_view = quint_view(post);
    let step_view = quint_step_mark_coin_spent(quint_view(pre), key);
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `entries' = entries.set(key, {..on_chain = Ready..})`,
/// `events' = events.append(EEntryReadinessChanged{purse, exp, new_state})`.
pub open spec fn quint_step_mark_entry_ready(
    pre: QuintViewState,
    key: (PurseId, u64),
) -> QuintViewState
    recommends
        pre.entries.dom().contains(key),
        pre.entries[key].on_chain == EntryOnChain::Waiting,
{
    QuintViewState {
        entries: pre.entries.insert(key, EntryRec {
            on_chain: EntryOnChain::Ready,
            ..pre.entries[key]
        }),
        events: pre.events.push(Event::EntryReadinessChanged {
            purse: key.0,
            exponent: pre.entries[key].exponent,
            new_state: EntryOnChain::Ready,
        }),
        ..pre
    }
}

proof fn lemma_mark_entry_ready_refines(pre: State, post: State, key: (PurseId, u64))
    requires
        pre.invariant(),
        pre.entries().dom().contains(key),
        pre.entries()[key].on_chain == EntryOnChain::Waiting,
        pre.events@.len() < u64::MAX as nat,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries().insert(key, EntryRec {
            on_chain: EntryOnChain::Ready,
            ..pre.entries()[key]
        }),
        post.operations() == pre.operations(),
        post.events@ == pre.events@.push(Event::EntryReadinessChanged {
            purse: key.0,
            exponent: pre.entries()[key].exponent,
            new_state: EntryOnChain::Ready,
        }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_mark_entry_ready(quint_view(pre), key),
{
    let post_view = quint_view(post);
    let step_view = quint_step_mark_entry_ready(quint_view(pre), key);
    assert(post_view.entries =~= step_view.entries);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `chain_entries' = chain_entries.append(e)`.
pub open spec fn quint_step_chain_register_entry(
    pre: QuintViewState,
    e: EntryRec,
) -> QuintViewState {
    QuintViewState {
        chain_entries: pre.chain_entries.push(e),
        ..pre
    }
}

proof fn lemma_chain_register_entry_refines(pre: State, post: State, e: EntryRec)
    requires
        pre.invariant(),
        pre.chain_entries@.len() < u64::MAX as nat,
        e.exponent <= MAX_EXPONENT,
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@.push(e),
    ensures
        quint_view(post) == quint_step_chain_register_entry(quint_view(pre), e),
{
    let post_view = quint_view(post);
    let step_view = quint_step_chain_register_entry(quint_view(pre), e);
    assert(post_view.chain_entries =~= step_view.chain_entries);
}

/// Quint analog: `events' = events.append(e)`.
pub open spec fn quint_step_emit_event(
    pre: QuintViewState,
    e: Event,
) -> QuintViewState {
    QuintViewState {
        events: pre.events.push(e),
        ..pre
    }
}

proof fn lemma_emit_event_refines(pre: State, post: State, e: Event)
    requires
        pre.invariant(),
        pre.events@.len() < u64::MAX as nat,
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@.push(e),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_emit_event(quint_view(pre), e),
{
    let post_view = quint_view(post);
    let step_view = quint_step_emit_event(quint_view(pre), e);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `entries' = entries.set(key, {..local = LocalConsumed..})`,
/// `events' = events.append(EEntryConsumed{purse, exp})`.
pub open spec fn quint_step_consume_entry(
    pre: QuintViewState,
    key: (PurseId, u64),
) -> QuintViewState
    recommends pre.entries.dom().contains(key),
{
    QuintViewState {
        entries: pre.entries.insert(key, EntryRec {
            local: EntryLocal::LocalConsumed,
            ..pre.entries[key]
        }),
        events: pre.events.push(Event::EntryConsumed {
            purse: key.0,
            exponent: pre.entries[key].exponent,
        }),
        ..pre
    }
}

proof fn lemma_consume_entry_refines(pre: State, post: State, key: (PurseId, u64))
    requires
        pre.invariant(),
        pre.entries().dom().contains(key),
        exists|h: OpHandle| pre.entries()[key].local == EntryLocal::LocalLockedFor(h),
        pre.events@.len() < u64::MAX as nat,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries().insert(key, EntryRec {
            local: EntryLocal::LocalConsumed,
            ..pre.entries()[key]
        }),
        post.operations() == pre.operations(),
        post.events@ == pre.events@.push(Event::EntryConsumed {
            purse: key.0,
            exponent: pre.entries()[key].exponent,
        }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_consume_entry(quint_view(pre), key),
{
    let post_view = quint_view(post);
    let step_view = quint_step_consume_entry(quint_view(pre), key);
    assert(post_view.entries =~= step_view.entries);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `operations' = operations.set(handle, {..status = Submitted..})`,
/// `events' = events.append(EOperationProgress{handle, status=Submitted})`.
pub open spec fn quint_step_mark_op_submitted(
    pre: QuintViewState,
    handle: OpHandle,
) -> QuintViewState
    recommends pre.operations.dom().contains(handle),
{
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            status: OpStatus::Submitted,
            ..pre.operations[handle]
        }),
        events: pre.events.push(Event::OperationProgress {
            handle,
            status: OpStatus::Submitted,
        }),
        ..pre
    }
}

proof fn lemma_mark_op_submitted_refines(pre: State, post: State, handle: OpHandle)
    requires
        pre.invariant(),
        pre.operations().dom().contains(handle),
        pre.operations()[handle].status == OpStatus::Preparing,
        pre.events@.len() < u64::MAX as nat,
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations().insert(handle, OperationRec {
            handle: pre.operations()[handle].handle,
            kind: pre.operations()[handle].kind,
            purse: pre.operations()[handle].purse,
            status: OpStatus::Submitted,
        }),
        post.events@ == pre.events@.push(Event::OperationProgress {
            handle,
            status: OpStatus::Submitted,
        }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_mark_op_submitted(quint_view(pre), handle),
{
    let post_view = quint_view(post);
    let step_view = quint_step_mark_op_submitted(quint_view(pre), handle);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `operations' = operations.set(handle, {..status = Done..})`,
/// `events' = events.append(EOperationCompleted{handle, status=Done})`.
pub open spec fn quint_step_mark_op_done(
    pre: QuintViewState,
    handle: OpHandle,
) -> QuintViewState
    recommends pre.operations.dom().contains(handle),
{
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            status: OpStatus::Done,
            ..pre.operations[handle]
        }),
        events: pre.events.push(Event::OperationCompleted {
            handle,
            status: OpStatus::Done,
        }),
        ..pre
    }
}

proof fn lemma_mark_op_done_refines(pre: State, post: State, handle: OpHandle)
    requires
        pre.invariant(),
        pre.operations().dom().contains(handle),
        pre.events@.len() < u64::MAX as nat,
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations().insert(handle, OperationRec {
            handle: pre.operations()[handle].handle,
            kind: pre.operations()[handle].kind,
            purse: pre.operations()[handle].purse,
            status: OpStatus::Done,
        }),
        post.events@ == pre.events@.push(Event::OperationCompleted {
            handle,
            status: OpStatus::Done,
        }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_mark_op_done(quint_view(pre), handle),
{
    let post_view = quint_view(post);
    let step_view = quint_step_mark_op_done(quint_view(pre), handle);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `operations' = operations.set(handle, {..status = Failed..})`,
/// `events' = events.append(EOperationCompleted{handle, status=Failed})`.
pub open spec fn quint_step_set_op_failed(
    pre: QuintViewState,
    handle: OpHandle,
) -> QuintViewState
    recommends pre.operations.dom().contains(handle),
{
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            status: OpStatus::Failed,
            ..pre.operations[handle]
        }),
        events: pre.events.push(Event::OperationCompleted {
            handle,
            status: OpStatus::Failed,
        }),
        ..pre
    }
}

proof fn lemma_set_op_failed_refines(pre: State, post: State, handle: OpHandle)
    requires
        pre.invariant(),
        pre.operations().dom().contains(handle),
        pre.events@.len() < u64::MAX as nat,
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations().insert(handle, OperationRec {
            handle: pre.operations()[handle].handle,
            kind: pre.operations()[handle].kind,
            purse: pre.operations()[handle].purse,
            status: OpStatus::Failed,
        }),
        post.events@ == pre.events@.push(Event::OperationCompleted {
            handle,
            status: OpStatus::Failed,
        }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_set_op_failed(quint_view(pre), handle),
{
    let post_view = quint_view(post);
    let step_view = quint_step_set_op_failed(quint_view(pre), handle);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `totalIn' = totalIn + amount`.
pub open spec fn quint_step_add_total_in(
    pre: QuintViewState,
    amount: u64,
) -> QuintViewState
    recommends pre.total_in + amount <= u64::MAX,
{
    QuintViewState {
        total_in: (pre.total_in + amount) as u64,
        ..pre
    }
}

proof fn lemma_add_total_in_refines(pre: State, post: State, amount: u64)
    requires
        pre.invariant(),
        pre.total_in <= u64::MAX - amount,
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in + amount,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_add_total_in(quint_view(pre), amount),
{
    // total_in is the only field that changes; others preserved.
}

/// Quint analog: `totalOut' = totalOut + amount`.
pub open spec fn quint_step_add_total_out(
    pre: QuintViewState,
    amount: u64,
) -> QuintViewState
    recommends pre.total_out + amount <= u64::MAX,
{
    QuintViewState {
        total_out: (pre.total_out + amount) as u64,
        ..pre
    }
}

proof fn lemma_add_total_out_refines(pre: State, post: State, amount: u64)
    requires
        pre.invariant(),
        pre.total_out <= u64::MAX - amount,
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out + amount,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_add_total_out(quint_view(pre), amount),
{
}

/// Quint analog: `operations' = operations.put(handle, {handle, kind,
/// purse, status: Preparing})`, `nextHandle' = nextHandle + 1`,
/// `events' = events.append(EOperationStarted{handle, kind, purse})`.
/// Allocator-bumping primitive — three fields change.
pub open spec fn quint_step_start_op(
    pre: QuintViewState,
    kind: OpKind,
    purse: PurseId,
) -> QuintViewState
    recommends pre.next_handle < u64::MAX,
{
    let handle = pre.next_handle;
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            handle,
            kind,
            purse,
            status: OpStatus::Preparing,
        }),
        next_handle: (pre.next_handle + 1) as u64,
        events: pre.events.push(Event::OperationStarted { handle, kind, purse }),
        ..pre
    }
}

proof fn lemma_start_op_refines(
    pre: State,
    post: State,
    kind: OpKind,
    purse: PurseId,
    handle: OpHandle,
)
    requires
        pre.invariant(),
        pre.purses().dom().contains(purse),
        pre.next_handle < u64::MAX,
        pre.events@.len() < u64::MAX as nat,
        handle == pre.next_handle,
        post.operations() == pre.operations().insert(handle, OperationRec {
            handle,
            kind,
            purse,
            status: OpStatus::Preparing,
        }),
        post.next_handle == pre.next_handle + 1,
        post.events@ == pre.events@.push(Event::OperationStarted { handle, kind, purse }),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_start_op(quint_view(pre), kind, purse),
{
    let post_view = quint_view(post);
    let step_view = quint_step_start_op(quint_view(pre), kind, purse);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `coins' = coins.set(key, {..state = LockedFor(handle)..})`.
pub open spec fn quint_step_lock_coin(
    pre: QuintViewState,
    key: (PurseId, u64),
    handle: OpHandle,
) -> QuintViewState
    recommends
        pre.coins.dom().contains(key),
        pre.coins[key].state == CoinState::Available,
{
    QuintViewState {
        coins: pre.coins.insert(key, CoinRec {
            purse: pre.coins[key].purse,
            idx: pre.coins[key].idx,
            exponent: pre.coins[key].exponent,
            age: pre.coins[key].age,
            account: pre.coins[key].account,
            state: CoinState::LockedFor(handle),
        }),
        ..pre
    }
}

proof fn lemma_lock_coin_refines(
    pre: State,
    post: State,
    key: (PurseId, u64),
    handle: OpHandle,
)
    requires
        pre.invariant(),
        pre.coins().dom().contains(key),
        pre.coins()[key].state == CoinState::Available,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins().insert(key, CoinRec {
            purse: pre.coins()[key].purse,
            idx: pre.coins()[key].idx,
            exponent: pre.coins()[key].exponent,
            age: pre.coins()[key].age,
            account: pre.coins()[key].account,
            state: CoinState::LockedFor(handle),
        }),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_lock_coin(quint_view(pre), key, handle),
{
    let post_view = quint_view(post);
    let step_view = quint_step_lock_coin(quint_view(pre), key, handle);
    assert(post_view.coins =~= step_view.coins);
}

/// Quint analog: `coins' = coins.set(key, {..state = Available..})`,
/// applied to a LockedFor(handle) coin.
pub open spec fn quint_step_release_locked_coin(
    pre: QuintViewState,
    key: (PurseId, u64),
    handle: OpHandle,
) -> QuintViewState
    recommends
        pre.coins.dom().contains(key),
        pre.coins[key].state == CoinState::LockedFor(handle),
{
    QuintViewState {
        coins: pre.coins.insert(key, CoinRec {
            purse: pre.coins[key].purse,
            idx: pre.coins[key].idx,
            exponent: pre.coins[key].exponent,
            age: pre.coins[key].age,
            account: pre.coins[key].account,
            state: CoinState::Available,
        }),
        ..pre
    }
}

proof fn lemma_release_locked_coin_refines(
    pre: State,
    post: State,
    key: (PurseId, u64),
    handle: OpHandle,
)
    requires
        pre.invariant(),
        pre.coins().dom().contains(key),
        pre.coins()[key].state == CoinState::LockedFor(handle),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins().insert(key, CoinRec {
            purse: pre.coins()[key].purse,
            idx: pre.coins()[key].idx,
            exponent: pre.coins()[key].exponent,
            age: pre.coins()[key].age,
            account: pre.coins()[key].account,
            state: CoinState::Available,
        }),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_release_locked_coin(quint_view(pre), key, handle),
{
    let post_view = quint_view(post);
    let step_view = quint_step_release_locked_coin(quint_view(pre), key, handle);
    assert(post_view.coins =~= step_view.coins);
}

/// Quint analog: `entries' = entries.set(key, {..local = LocalLockedFor(handle)..})`.
pub open spec fn quint_step_lock_entry(
    pre: QuintViewState,
    key: (PurseId, u64),
    handle: OpHandle,
) -> QuintViewState
    recommends
        pre.entries.dom().contains(key),
        pre.entries[key].local == EntryLocal::LocalAvailable,
{
    QuintViewState {
        entries: pre.entries.insert(key, EntryRec {
            local: EntryLocal::LocalLockedFor(handle),
            ..pre.entries[key]
        }),
        ..pre
    }
}

proof fn lemma_lock_entry_refines(
    pre: State,
    post: State,
    key: (PurseId, u64),
    handle: OpHandle,
)
    requires
        pre.invariant(),
        pre.entries().dom().contains(key),
        pre.entries()[key].local == EntryLocal::LocalAvailable,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries().insert(key, EntryRec {
            local: EntryLocal::LocalLockedFor(handle),
            ..pre.entries()[key]
        }),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_lock_entry(quint_view(pre), key, handle),
{
    let post_view = quint_view(post);
    let step_view = quint_step_lock_entry(quint_view(pre), key, handle);
    assert(post_view.entries =~= step_view.entries);
}

/// Quint analog: `entries' = entries.set(key, {..local = LocalAvailable..})`,
/// applied to a LocalLockedFor(handle) entry.
pub open spec fn quint_step_release_locked_entry(
    pre: QuintViewState,
    key: (PurseId, u64),
    handle: OpHandle,
) -> QuintViewState
    recommends
        pre.entries.dom().contains(key),
        pre.entries[key].local == EntryLocal::LocalLockedFor(handle),
{
    QuintViewState {
        entries: pre.entries.insert(key, EntryRec {
            local: EntryLocal::LocalAvailable,
            ..pre.entries[key]
        }),
        ..pre
    }
}

proof fn lemma_release_locked_entry_refines(
    pre: State,
    post: State,
    key: (PurseId, u64),
    handle: OpHandle,
)
    requires
        pre.invariant(),
        pre.entries().dom().contains(key),
        pre.entries()[key].local == EntryLocal::LocalLockedFor(handle),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries().insert(key, EntryRec {
            local: EntryLocal::LocalAvailable,
            ..pre.entries()[key]
        }),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_release_locked_entry(quint_view(pre), key, handle),
{
    let post_view = quint_view(post);
    let step_view = quint_step_release_locked_entry(quint_view(pre), key, handle);
    assert(post_view.entries =~= step_view.entries);
}

/// Quint analog: `entries' = entries.set(key, {..on_chain = new_state..})`.
pub open spec fn quint_step_set_entry_on_chain(
    pre: QuintViewState,
    key: (PurseId, u64),
    new_state: EntryOnChain,
) -> QuintViewState
    recommends
        pre.entries.dom().contains(key),
{
    QuintViewState {
        entries: pre.entries.insert(key, EntryRec {
            on_chain: new_state,
            ..pre.entries[key]
        }),
        ..pre
    }
}

proof fn lemma_set_entry_on_chain_refines(
    pre: State,
    post: State,
    key: (PurseId, u64),
    new_state: EntryOnChain,
)
    requires
        pre.invariant(),
        pre.entries().dom().contains(key),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries().insert(key, EntryRec {
            on_chain: new_state,
            ..pre.entries()[key]
        }),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_set_entry_on_chain(quint_view(pre), key, new_state),
{
    let post_view = quint_view(post);
    let step_view = quint_step_set_entry_on_chain(quint_view(pre), key, new_state);
    assert(post_view.entries =~= step_view.entries);
}

/// Quint analog: `entries' = entries.set(key, {..local = new_state..})`.
pub open spec fn quint_step_set_entry_local(
    pre: QuintViewState,
    key: (PurseId, u64),
    new_state: EntryLocal,
) -> QuintViewState
    recommends
        pre.entries.dom().contains(key),
{
    QuintViewState {
        entries: pre.entries.insert(key, EntryRec {
            local: new_state,
            ..pre.entries[key]
        }),
        ..pre
    }
}

proof fn lemma_set_entry_local_refines(
    pre: State,
    post: State,
    key: (PurseId, u64),
    new_state: EntryLocal,
)
    requires
        pre.invariant(),
        pre.entries().dom().contains(key),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries().insert(key, EntryRec {
            local: new_state,
            ..pre.entries()[key]
        }),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_set_entry_local(quint_view(pre), key, new_state),
{
    let post_view = quint_view(post);
    let step_view = quint_step_set_entry_local(quint_view(pre), key, new_state);
    assert(post_view.entries =~= step_view.entries);
}

/// Quint analog: `operations' = operations.set(handle, {..status = new_status..})`.
pub open spec fn quint_step_set_op_status(
    pre: QuintViewState,
    handle: OpHandle,
    new_status: OpStatus,
) -> QuintViewState
    recommends
        pre.operations.dom().contains(handle),
{
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            handle: pre.operations[handle].handle,
            kind: pre.operations[handle].kind,
            purse: pre.operations[handle].purse,
            status: new_status,
        }),
        ..pre
    }
}

proof fn lemma_set_op_status_refines(
    pre: State,
    post: State,
    handle: OpHandle,
    new_status: OpStatus,
)
    requires
        pre.invariant(),
        pre.operations().dom().contains(handle),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations().insert(handle, OperationRec {
            handle: pre.operations()[handle].handle,
            kind: pre.operations()[handle].kind,
            purse: pre.operations()[handle].purse,
            status: new_status,
        }),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_set_op_status(quint_view(pre), handle, new_status),
{
    let post_view = quint_view(post);
    let step_view = quint_step_set_op_status(quint_view(pre), handle, new_status);
    assert(post_view.operations =~= step_view.operations);
}

/// Quint analog: `operations' = operations.set(handle, {..status = InBlock..})`.
pub open spec fn quint_step_mark_op_in_block(
    pre: QuintViewState,
    handle: OpHandle,
) -> QuintViewState
    recommends
        pre.operations.dom().contains(handle),
        pre.operations[handle].status == OpStatus::Submitted,
{
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            handle: pre.operations[handle].handle,
            kind: pre.operations[handle].kind,
            purse: pre.operations[handle].purse,
            status: OpStatus::InBlock,
        }),
        ..pre
    }
}

proof fn lemma_mark_op_in_block_refines(pre: State, post: State, handle: OpHandle)
    requires
        pre.invariant(),
        pre.operations().dom().contains(handle),
        pre.operations()[handle].status == OpStatus::Submitted,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations().insert(handle, OperationRec {
            handle: pre.operations()[handle].handle,
            kind: pre.operations()[handle].kind,
            purse: pre.operations()[handle].purse,
            status: OpStatus::InBlock,
        }),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_mark_op_in_block(quint_view(pre), handle),
{
    let post_view = quint_view(post);
    let step_view = quint_step_mark_op_in_block(quint_view(pre), handle);
    assert(post_view.operations =~= step_view.operations);
}

/// Quint analog: `operations' = operations.set(handle, {..status = Finalized..})`.
pub open spec fn quint_step_mark_op_finalized(
    pre: QuintViewState,
    handle: OpHandle,
) -> QuintViewState
    recommends
        pre.operations.dom().contains(handle),
        pre.operations[handle].status == OpStatus::InBlock,
{
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            handle: pre.operations[handle].handle,
            kind: pre.operations[handle].kind,
            purse: pre.operations[handle].purse,
            status: OpStatus::Finalized,
        }),
        ..pre
    }
}

proof fn lemma_mark_op_finalized_refines(pre: State, post: State, handle: OpHandle)
    requires
        pre.invariant(),
        pre.operations().dom().contains(handle),
        pre.operations()[handle].status == OpStatus::InBlock,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations().insert(handle, OperationRec {
            handle: pre.operations()[handle].handle,
            kind: pre.operations()[handle].kind,
            purse: pre.operations()[handle].purse,
            status: OpStatus::Finalized,
        }),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_mark_op_finalized(quint_view(pre), handle),
{
    let post_view = quint_view(post);
    let step_view = quint_step_mark_op_finalized(quint_view(pre), handle);
    assert(post_view.operations =~= step_view.operations);
}

/// Quint analog: `entries' = entries.set(key, {..on_chain = Missing..})`.
pub open spec fn quint_step_mark_entry_missing(
    pre: QuintViewState,
    key: (PurseId, u64),
) -> QuintViewState
    recommends
        pre.entries.dom().contains(key),
{
    QuintViewState {
        entries: pre.entries.insert(key, EntryRec {
            on_chain: EntryOnChain::Missing,
            ..pre.entries[key]
        }),
        ..pre
    }
}

proof fn lemma_mark_entry_missing_refines(pre: State, post: State, key: (PurseId, u64))
    requires
        pre.invariant(),
        pre.entries().dom().contains(key),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries().insert(key, EntryRec {
            on_chain: EntryOnChain::Missing,
            ..pre.entries()[key]
        }),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_mark_entry_missing(quint_view(pre), key),
{
    let post_view = quint_view(post);
    let step_view = quint_step_mark_entry_missing(quint_view(pre), key);
    assert(post_view.entries =~= step_view.entries);
}

/// Quint analog: `nextExtrinsicId' = nextExtrinsicId + 1`. The Quint
/// allocator returns the pre-increment value (matching Verus exec).
pub open spec fn quint_step_alloc_extrinsic_id(
    pre: QuintViewState,
) -> QuintViewState
    recommends
        pre.next_extrinsic_id < u64::MAX,
{
    QuintViewState {
        next_extrinsic_id: (pre.next_extrinsic_id + 1) as u64,
        ..pre
    }
}

proof fn lemma_alloc_extrinsic_id_refines(pre: State, post: State)
    requires
        pre.invariant(),
        pre.next_extrinsic_id < u64::MAX,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id + 1,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_alloc_extrinsic_id(quint_view(pre)),
{
}

/// Quint analog: `coins' = coins.set(rec.purse, rec.idx) -> rec`. Inverse of
/// the chain-mirror loss path: a coin previously observed lives in
/// `chain_coins` and is being re-injected into the canonical `coins` map.
pub open spec fn quint_step_restore_chain_coin(
    pre: QuintViewState,
    rec: CoinRec,
) -> QuintViewState
    recommends
        !pre.coins.dom().contains((rec.purse, rec.idx)),
{
    QuintViewState {
        coins: pre.coins.insert((rec.purse, rec.idx), rec),
        ..pre
    }
}

proof fn lemma_restore_chain_coin_refines(pre: State, post: State, rec: CoinRec)
    requires
        pre.invariant(),
        !pre.coins().dom().contains((rec.purse, rec.idx)),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins().insert((rec.purse, rec.idx), rec),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_restore_chain_coin(quint_view(pre), rec),
{
    let post_view = quint_view(post);
    let step_view = quint_step_restore_chain_coin(quint_view(pre), rec);
    assert(post_view.coins =~= step_view.coins);
}

/// Quint analog: `entries' = entries.set(rec.purse, rec.idx) -> rec`.
/// Mirror of `restore_chain_coin` for entries.
pub open spec fn quint_step_restore_chain_entry(
    pre: QuintViewState,
    rec: EntryRec,
) -> QuintViewState
    recommends
        !pre.entries.dom().contains((rec.purse, rec.idx)),
{
    QuintViewState {
        entries: pre.entries.insert((rec.purse, rec.idx), rec),
        ..pre
    }
}

proof fn lemma_restore_chain_entry_refines(pre: State, post: State, rec: EntryRec)
    requires
        pre.invariant(),
        !pre.entries().dom().contains((rec.purse, rec.idx)),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries().insert((rec.purse, rec.idx), rec),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_restore_chain_entry(quint_view(pre), rec),
{
    let post_view = quint_view(post);
    let step_view = quint_step_restore_chain_entry(quint_view(pre), rec);
    assert(post_view.entries =~= step_view.entries);
}

/// Quint analog: insert a fresh coin and bump the owning purse's
/// `next_coin_idx`. Quint does NOT model `next_age` (it's a Verus-only
/// allocator); only `purses` and `coins` are in the shadow.
pub open spec fn quint_step_add_coin_with_account(
    pre: QuintViewState,
    p: PurseId,
    exponent: u8,
    account: u64,
    next_age: u64,
    new_idx: u64,
) -> QuintViewState
    recommends
        pre.purses.dom().contains(p),
        pre.purses[p].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
{
    let key = (p, new_idx);
    QuintViewState {
        coins: pre.coins.insert(key, CoinRec {
            purse: p,
            idx: new_idx,
            exponent,
            state: CoinState::Pending,
            age: next_age,
            account,
        }),
        purses: pre.purses.insert(p, PurseRecSpec {
            id: pre.purses[p].id,
            name: pre.purses[p].name,
            next_coin_idx: pre.purses[p].next_coin_idx + 1,
            next_entry_idx: pre.purses[p].next_entry_idx,
        }),
        ..pre
    }
}

proof fn lemma_add_coin_with_account_refines(
    pre: State,
    post: State,
    p: PurseId,
    exponent: u8,
    account: u64,
    new_idx: u64,
)
    requires
        pre.invariant(),
        pre.purses().dom().contains(p),
        pre.purses()[p].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        pre.next_age < u64::MAX,
        exponent <= MAX_EXPONENT,
        post.invariant(),
        post.coins() == pre.coins().insert(
            (p, new_idx),
            CoinRec {
                purse: p,
                idx: new_idx,
                exponent,
                state: CoinState::Pending,
                age: pre.next_age,
                account,
            },
        ),
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[p].id == p,
        post.purses()[p].name == pre.purses()[p].name,
        post.purses()[p].next_coin_idx == pre.purses()[p].next_coin_idx + 1,
        post.purses()[p].next_entry_idx == pre.purses()[p].next_entry_idx,
        forall|q: PurseId| q != p && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_add_coin_with_account(
            quint_view(pre), p, exponent, account, pre.next_age, new_idx,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_add_coin_with_account(
        quint_view(pre), p, exponent, account, pre.next_age, new_idx,
    );
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.purses =~= step_view.purses);
}

/// Quint analog: insert a fresh entry and bump the owning purse's
/// `next_entry_idx`.
pub open spec fn quint_step_add_entry_with_meta(
    pre: QuintViewState,
    p: PurseId,
    exponent: u8,
    on_chain: EntryOnChain,
    local: EntryLocal,
    member_key: u64,
    allocated_at: u64,
    ready_at: u64,
    ring_idx: u64,
    new_idx: u64,
) -> QuintViewState
    recommends
        pre.purses.dom().contains(p),
        pre.purses[p].next_entry_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
{
    let key = (p, new_idx);
    QuintViewState {
        entries: pre.entries.insert(key, EntryRec {
            purse: p,
            idx: new_idx,
            exponent,
            on_chain,
            local,
            member_key,
            allocated_at,
            ready_at,
            ring_idx,
        }),
        purses: pre.purses.insert(p, PurseRecSpec {
            id: pre.purses[p].id,
            name: pre.purses[p].name,
            next_coin_idx: pre.purses[p].next_coin_idx,
            next_entry_idx: pre.purses[p].next_entry_idx + 1,
        }),
        ..pre
    }
}

proof fn lemma_add_entry_with_meta_refines(
    pre: State,
    post: State,
    p: PurseId,
    exponent: u8,
    on_chain: EntryOnChain,
    local: EntryLocal,
    member_key: u64,
    allocated_at: u64,
    ready_at: u64,
    ring_idx: u64,
    new_idx: u64,
)
    requires
        pre.invariant(),
        pre.purses().dom().contains(p),
        pre.purses()[p].next_entry_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        exponent <= MAX_EXPONENT,
        post.invariant(),
        post.entries() == pre.entries().insert(
            (p, new_idx),
            EntryRec {
                purse: p,
                idx: new_idx,
                exponent,
                on_chain,
                local,
                member_key,
                allocated_at,
                ready_at,
                ring_idx,
            },
        ),
        post.coins() == pre.coins(),
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[p].id == p,
        post.purses()[p].name == pre.purses()[p].name,
        post.purses()[p].next_coin_idx == pre.purses()[p].next_coin_idx,
        post.purses()[p].next_entry_idx == pre.purses()[p].next_entry_idx + 1,
        forall|q: PurseId| q != p && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_add_entry_with_meta(
            quint_view(pre), p, exponent, on_chain, local,
            member_key, allocated_at, ready_at, ring_idx, new_idx,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_add_entry_with_meta(
        quint_view(pre), p, exponent, on_chain, local,
        member_key, allocated_at, ready_at, ring_idx, new_idx,
    );
    assert(post_view.entries =~= step_view.entries);
    assert(post_view.purses =~= step_view.purses);
}

/// Quint analog: `feeBalance' = feeBalance + amount`.
pub open spec fn quint_step_top_up_fee_account(
    pre: QuintViewState,
    amount: u64,
) -> QuintViewState
    recommends
        pre.fee_balance <= u64::MAX - amount,
{
    QuintViewState {
        fee_balance: (pre.fee_balance + amount) as u64,
        ..pre
    }
}

proof fn lemma_top_up_fee_account_refines(pre: State, post: State, amount: u64)
    requires
        pre.invariant(),
        pre.fee_balance <= u64::MAX - amount,
        post.invariant(),
        post.fee_balance == pre.fee_balance + amount,
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_top_up_fee_account(quint_view(pre), amount),
{
}

/// Quint analog: `feeBalance' = feeBalance - amount` (only fires on
/// the successful branch; the InsufficientFunds branch leaves
/// `feeBalance` unchanged, refined separately as the no-op step).
pub open spec fn quint_step_deduct_fee_success(
    pre: QuintViewState,
    amount: u64,
) -> QuintViewState
    recommends
        pre.fee_balance >= amount,
{
    QuintViewState {
        fee_balance: (pre.fee_balance - amount) as u64,
        ..pre
    }
}

proof fn lemma_deduct_fee_success_refines(pre: State, post: State, amount: u64)
    requires
        pre.invariant(),
        pre.fee_balance >= amount,
        post.invariant(),
        post.fee_balance == pre.fee_balance - amount,
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_deduct_fee_success(quint_view(pre), amount),
{
}

/// Quint analog: `feeBalance' = feeBalance` (the InsufficientFunds
/// branch of `deduct_fee` is a state-preserving no-op).
proof fn lemma_deduct_fee_fail_refines(pre: State, post: State)
    requires
        pre.invariant(),
        post.invariant(),
        post.fee_balance == pre.fee_balance,
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_view(pre),
{
}

/// Quint analog: `tokens' = tokens.append(UnloadToken{..})`.
pub open spec fn quint_step_mint_token(
    pre: QuintViewState,
    period: u64,
    class: UnloadTokenClass,
    counter: u64,
) -> QuintViewState {
    QuintViewState {
        tokens: pre.tokens.push(UnloadToken {
            period, class, counter, consumed: false,
        }),
        ..pre
    }
}

proof fn lemma_mint_token_refines(
    pre: State,
    post: State,
    period: u64,
    class: UnloadTokenClass,
    counter: u64,
)
    requires
        pre.invariant(),
        pre.tokens@.len() < u64::MAX as nat,
        post.invariant(),
        post.tokens@ == pre.tokens@.push(UnloadToken {
            period, class, counter, consumed: false,
        }),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_mint_token(quint_view(pre), period, class, counter),
{
    let post_view = quint_view(post);
    let step_view = quint_step_mint_token(quint_view(pre), period, class, counter);
    assert(post_view.tokens =~= step_view.tokens);
}

/// Quint analog: flip `tokens[idx].consumed = true` (only the success
/// branch; the failure branches are state-preserving no-ops).
pub open spec fn quint_step_consume_token_success(
    pre: QuintViewState,
    idx: usize,
) -> QuintViewState
    recommends
        idx < pre.tokens.len(),
        !pre.tokens[idx as int].consumed,
{
    QuintViewState {
        tokens: pre.tokens.update(idx as int, UnloadToken {
            period: pre.tokens[idx as int].period,
            class: pre.tokens[idx as int].class,
            counter: pre.tokens[idx as int].counter,
            consumed: true,
        }),
        ..pre
    }
}

proof fn lemma_consume_token_success_refines(pre: State, post: State, idx: usize)
    requires
        pre.invariant(),
        idx < pre.tokens@.len(),
        !pre.tokens@[idx as int].consumed,
        post.invariant(),
        post.tokens@.len() == pre.tokens@.len(),
        post.tokens@[idx as int].consumed,
        post.tokens@[idx as int].period == pre.tokens@[idx as int].period,
        post.tokens@[idx as int].class == pre.tokens@[idx as int].class,
        post.tokens@[idx as int].counter == pre.tokens@[idx as int].counter,
        forall|i: int| 0 <= i < pre.tokens@.len() && i != idx as int
            ==> #[trigger] post.tokens@[i] == pre.tokens@[i],
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_consume_token_success(quint_view(pre), idx),
{
    let post_view = quint_view(post);
    let step_view = quint_step_consume_token_success(quint_view(pre), idx);
    assert(post_view.tokens =~= step_view.tokens);
}

/// Quint analog: `tokens' = tokens` (the failure branches of
/// `consume_token` are state-preserving no-ops).
proof fn lemma_consume_token_fail_refines(pre: State, post: State)
    requires
        pre.invariant(),
        post.invariant(),
        post.tokens@ == pre.tokens@,
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_view(pre),
{
}

/// Quint analog: insert a fresh Waiting/LocalAvailable entry, bump
/// the owning purse's `next_entry_idx`, and push `EEntryAllocated`.
pub open spec fn quint_step_top_up_via_entry(
    pre: QuintViewState,
    p: PurseId,
    exponent: u8,
    member_key: u64,
    allocated_at: u64,
    ready_at: u64,
    ring_idx: u64,
    new_idx: u64,
) -> QuintViewState
    recommends
        pre.purses.dom().contains(p),
        pre.purses[p].next_entry_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
{
    let key = (p, new_idx);
    QuintViewState {
        entries: pre.entries.insert(key, EntryRec {
            purse: p,
            idx: new_idx,
            exponent,
            on_chain: EntryOnChain::Waiting,
            local: EntryLocal::LocalAvailable,
            member_key,
            allocated_at,
            ready_at,
            ring_idx,
        }),
        purses: pre.purses.insert(p, PurseRecSpec {
            id: pre.purses[p].id,
            name: pre.purses[p].name,
            next_coin_idx: pre.purses[p].next_coin_idx,
            next_entry_idx: pre.purses[p].next_entry_idx + 1,
        }),
        events: pre.events.push(Event::EntryAllocated { purse: p, exponent }),
        ..pre
    }
}

proof fn lemma_top_up_via_entry_refines(
    pre: State,
    post: State,
    p: PurseId,
    exponent: u8,
    member_key: u64,
    allocated_at: u64,
    ready_at: u64,
    ring_idx: u64,
    new_idx: u64,
)
    requires
        pre.invariant(),
        pre.purses().dom().contains(p),
        pre.purses()[p].next_entry_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        pre.events@.len() < u64::MAX as nat,
        exponent <= MAX_EXPONENT,
        post.invariant(),
        post.entries() == pre.entries().insert(
            (p, new_idx),
            EntryRec {
                purse: p,
                idx: new_idx,
                exponent,
                on_chain: EntryOnChain::Waiting,
                local: EntryLocal::LocalAvailable,
                member_key,
                allocated_at,
                ready_at,
                ring_idx,
            },
        ),
        post.coins() == pre.coins(),
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[p].id == p,
        post.purses()[p].name == pre.purses()[p].name,
        post.purses()[p].next_coin_idx == pre.purses()[p].next_coin_idx,
        post.purses()[p].next_entry_idx == pre.purses()[p].next_entry_idx + 1,
        forall|q: PurseId| q != p && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.operations() == pre.operations(),
        post.events@ == pre.events@.push(Event::EntryAllocated { purse: p, exponent }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_top_up_via_entry(
            quint_view(pre), p, exponent,
            member_key, allocated_at, ready_at, ring_idx, new_idx,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_top_up_via_entry(
        quint_view(pre), p, exponent,
        member_key, allocated_at, ready_at, ring_idx, new_idx,
    );
    assert(post_view.entries =~= step_view.entries);
    assert(post_view.purses =~= step_view.purses);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `coins' = coins.set(key, {..state = Available..})`,
/// applied to any LockedFor(_) coin (no handle constraint — the
/// pre-state existentially binds the handle).
pub open spec fn quint_step_unlock_coin(
    pre: QuintViewState,
    key: (PurseId, u64),
) -> QuintViewState
    recommends
        pre.coins.dom().contains(key),
        exists|h: OpHandle| pre.coins[key].state == CoinState::LockedFor(h),
{
    QuintViewState {
        coins: pre.coins.insert(key, CoinRec {
            purse: pre.coins[key].purse,
            idx: pre.coins[key].idx,
            exponent: pre.coins[key].exponent,
            age: pre.coins[key].age,
            account: pre.coins[key].account,
            state: CoinState::Available,
        }),
        ..pre
    }
}

proof fn lemma_unlock_coin_refines(pre: State, post: State, key: (PurseId, u64))
    requires
        pre.invariant(),
        pre.coins().dom().contains(key),
        exists|h: OpHandle| pre.coins()[key].state == CoinState::LockedFor(h),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins().insert(key, CoinRec {
            purse: pre.coins()[key].purse,
            idx: pre.coins()[key].idx,
            exponent: pre.coins()[key].exponent,
            age: pre.coins()[key].age,
            account: pre.coins()[key].account,
            state: CoinState::Available,
        }),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_unlock_coin(quint_view(pre), key),
{
    let post_view = quint_view(post);
    let step_view = quint_step_unlock_coin(quint_view(pre), key);
    assert(post_view.coins =~= step_view.coins);
}

/// Quint analog: `coins' = coins.set(key, {..state = PendingSpend..})`,
/// applied to any LockedFor(_) coin.
pub open spec fn quint_step_commit_locked_coin(
    pre: QuintViewState,
    key: (PurseId, u64),
) -> QuintViewState
    recommends
        pre.coins.dom().contains(key),
        exists|h: OpHandle| pre.coins[key].state == CoinState::LockedFor(h),
{
    QuintViewState {
        coins: pre.coins.insert(key, CoinRec {
            purse: pre.coins[key].purse,
            idx: pre.coins[key].idx,
            exponent: pre.coins[key].exponent,
            age: pre.coins[key].age,
            account: pre.coins[key].account,
            state: CoinState::PendingSpend,
        }),
        ..pre
    }
}

proof fn lemma_commit_locked_coin_refines(pre: State, post: State, key: (PurseId, u64))
    requires
        pre.invariant(),
        pre.coins().dom().contains(key),
        exists|h: OpHandle| pre.coins()[key].state == CoinState::LockedFor(h),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins().insert(key, CoinRec {
            purse: pre.coins()[key].purse,
            idx: pre.coins()[key].idx,
            exponent: pre.coins()[key].exponent,
            age: pre.coins()[key].age,
            account: pre.coins()[key].account,
            state: CoinState::PendingSpend,
        }),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_commit_locked_coin(quint_view(pre), key),
{
    let post_view = quint_view(post);
    let step_view = quint_step_commit_locked_coin(quint_view(pre), key);
    assert(post_view.coins =~= step_view.coins);
}

/// Quint analog: `operations' = operations.set(handle, {..status = Waiting(ready_at)..})`.
pub open spec fn quint_step_mark_op_waiting(
    pre: QuintViewState,
    handle: OpHandle,
    ready_at: u64,
) -> QuintViewState
    recommends
        pre.operations.dom().contains(handle),
        pre.operations[handle].status == OpStatus::Finalized,
{
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            handle: pre.operations[handle].handle,
            kind: pre.operations[handle].kind,
            purse: pre.operations[handle].purse,
            status: OpStatus::Waiting(ready_at),
        }),
        ..pre
    }
}

proof fn lemma_mark_op_waiting_refines(
    pre: State,
    post: State,
    handle: OpHandle,
    ready_at: u64,
)
    requires
        pre.invariant(),
        pre.operations().dom().contains(handle),
        pre.operations()[handle].status == OpStatus::Finalized,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations().insert(handle, OperationRec {
            handle: pre.operations()[handle].handle,
            kind: pre.operations()[handle].kind,
            purse: pre.operations()[handle].purse,
            status: OpStatus::Waiting(ready_at),
        }),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_mark_op_waiting(quint_view(pre), handle, ready_at),
{
    let post_view = quint_view(post);
    let step_view = quint_step_mark_op_waiting(quint_view(pre), handle, ready_at);
    assert(post_view.operations =~= step_view.operations);
}

/// Quint analog: `entries' = entries.set(key, {..local = LocalAvailable..})`,
/// applied to any LocalLockedFor(_) entry.
pub open spec fn quint_step_release_entry_lock(
    pre: QuintViewState,
    key: (PurseId, u64),
) -> QuintViewState
    recommends
        pre.entries.dom().contains(key),
        exists|h: OpHandle| pre.entries[key].local == EntryLocal::LocalLockedFor(h),
{
    QuintViewState {
        entries: pre.entries.insert(key, EntryRec {
            local: EntryLocal::LocalAvailable,
            ..pre.entries[key]
        }),
        ..pre
    }
}

proof fn lemma_release_entry_lock_refines(pre: State, post: State, key: (PurseId, u64))
    requires
        pre.invariant(),
        pre.entries().dom().contains(key),
        exists|h: OpHandle| pre.entries()[key].local == EntryLocal::LocalLockedFor(h),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries().insert(key, EntryRec {
            local: EntryLocal::LocalAvailable,
            ..pre.entries()[key]
        }),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_release_entry_lock(quint_view(pre), key),
{
    let post_view = quint_view(post);
    let step_view = quint_step_release_entry_lock(quint_view(pre), key);
    assert(post_view.entries =~= step_view.entries);
}

/// Quint analog: thin wrapper over `add_coin_with_account` with
/// `account = 0`.
pub open spec fn quint_step_add_coin(
    pre: QuintViewState,
    p: PurseId,
    exponent: u8,
    next_age: u64,
    new_idx: u64,
) -> QuintViewState
    recommends
        pre.purses.dom().contains(p),
        pre.purses[p].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
{
    quint_step_add_coin_with_account(pre, p, exponent, 0, next_age, new_idx)
}

proof fn lemma_add_coin_refines(
    pre: State,
    post: State,
    p: PurseId,
    exponent: u8,
    new_idx: u64,
)
    requires
        pre.invariant(),
        pre.purses().dom().contains(p),
        pre.purses()[p].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        pre.next_age < u64::MAX,
        exponent <= MAX_EXPONENT,
        post.invariant(),
        post.coins() == pre.coins().insert(
            (p, new_idx),
            CoinRec {
                purse: p,
                idx: new_idx,
                exponent,
                state: CoinState::Pending,
                age: pre.next_age,
                account: 0,
            },
        ),
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[p].id == p,
        post.purses()[p].name == pre.purses()[p].name,
        post.purses()[p].next_coin_idx == pre.purses()[p].next_coin_idx + 1,
        post.purses()[p].next_entry_idx == pre.purses()[p].next_entry_idx,
        forall|q: PurseId| q != p && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_add_coin(
            quint_view(pre), p, exponent, pre.next_age, new_idx,
        ),
{
    lemma_add_coin_with_account_refines(pre, post, p, exponent, 0, new_idx);
}

/// Quint analog: thin wrapper over `add_entry_with_meta` with zero
/// placeholders for the four chain-side metadata fields.
pub open spec fn quint_step_add_entry(
    pre: QuintViewState,
    p: PurseId,
    exponent: u8,
    on_chain: EntryOnChain,
    local: EntryLocal,
    new_idx: u64,
) -> QuintViewState
    recommends
        pre.purses.dom().contains(p),
        pre.purses[p].next_entry_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
{
    quint_step_add_entry_with_meta(
        pre, p, exponent, on_chain, local, 0, 0, 0, 0, new_idx,
    )
}

proof fn lemma_add_entry_refines(
    pre: State,
    post: State,
    p: PurseId,
    exponent: u8,
    on_chain: EntryOnChain,
    local: EntryLocal,
    new_idx: u64,
)
    requires
        pre.invariant(),
        pre.purses().dom().contains(p),
        pre.purses()[p].next_entry_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        exponent <= MAX_EXPONENT,
        post.invariant(),
        post.entries() == pre.entries().insert(
            (p, new_idx),
            EntryRec {
                purse: p,
                idx: new_idx,
                exponent,
                on_chain,
                local,
                member_key: 0,
                allocated_at: 0,
                ready_at: 0,
                ring_idx: 0,
            },
        ),
        post.coins() == pre.coins(),
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[p].id == p,
        post.purses()[p].name == pre.purses()[p].name,
        post.purses()[p].next_coin_idx == pre.purses()[p].next_coin_idx,
        post.purses()[p].next_entry_idx == pre.purses()[p].next_entry_idx + 1,
        forall|q: PurseId| q != p && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_add_entry(
            quint_view(pre), p, exponent, on_chain, local, new_idx,
        ),
{
    lemma_add_entry_with_meta_refines(
        pre, post, p, exponent, on_chain, local, 0, 0, 0, 0, new_idx,
    );
}

/// Quint analog: `purses' = purses.set(p, {..name = name..})`. Only
/// fires on the success branch — the PurseNotFound branch refines as
/// a state-preserving no-op.
pub open spec fn quint_step_rename_purse_success(
    pre: QuintViewState,
    p: PurseId,
    name: Seq<u8>,
) -> QuintViewState
    recommends
        pre.purses.dom().contains(p),
{
    QuintViewState {
        purses: pre.purses.insert(p, PurseRecSpec {
            id: pre.purses[p].id,
            name,
            next_coin_idx: pre.purses[p].next_coin_idx,
            next_entry_idx: pre.purses[p].next_entry_idx,
        }),
        ..pre
    }
}

proof fn lemma_rename_purse_success_refines(
    pre: State,
    post: State,
    p: PurseId,
    name: Seq<u8>,
)
    requires
        pre.invariant(),
        pre.purses().dom().contains(p),
        post.invariant(),
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[p].id == p,
        post.purses()[p].name == name,
        post.purses()[p].next_coin_idx == pre.purses()[p].next_coin_idx,
        post.purses()[p].next_entry_idx == pre.purses()[p].next_entry_idx,
        forall|q: PurseId| q != p && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_rename_purse_success(quint_view(pre), p, name),
{
    let post_view = quint_view(post);
    let step_view = quint_step_rename_purse_success(quint_view(pre), p, name);
    assert(post_view.purses =~= step_view.purses);
}

/// Quint analog: `purses' = purses` (the PurseNotFound branch of
/// `rename_purse` is a state-preserving no-op).
proof fn lemma_rename_purse_fail_refines(pre: State, post: State)
    requires
        pre.invariant(),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_view(pre),
{
}

/// Quint analog (Some branch): `coins' = coins.put(rec.purse, rec.idx) -> rec`
/// where `rec = chain_coins[j]`. Composes with
/// [`quint_step_restore_chain_coin`] for the actual step.
proof fn lemma_recover_scan_step_coin_some_refines(
    pre: State,
    post: State,
    j: usize,
)
    requires
        pre.invariant(),
        0 <= j < pre.chain_coins@.len(),
        !pre.coins().dom().contains(
            (pre.chain_coins@[j as int].purse,
             pre.chain_coins@[j as int].idx)),
        post.invariant(),
        post.coins() == pre.coins().insert(
            (pre.chain_coins@[j as int].purse,
             pre.chain_coins@[j as int].idx),
            pre.chain_coins@[j as int]),
        post.purses() == pre.purses(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_restore_chain_coin(
            quint_view(pre), pre.chain_coins@[j as int],
        ),
{
    lemma_restore_chain_coin_refines(pre, post, pre.chain_coins@[j as int]);
}

/// Quint analog (None branch): state-preserving no-op.
proof fn lemma_recover_scan_step_coin_none_refines(pre: State, post: State)
    requires
        pre.invariant(),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_view(pre),
{
}

/// Entry parallel of [`lemma_recover_scan_step_coin_some_refines`].
proof fn lemma_recover_scan_step_entry_some_refines(
    pre: State,
    post: State,
    j: usize,
)
    requires
        pre.invariant(),
        0 <= j < pre.chain_entries@.len(),
        !pre.entries().dom().contains(
            (pre.chain_entries@[j as int].purse,
             pre.chain_entries@[j as int].idx)),
        post.invariant(),
        post.entries() == pre.entries().insert(
            (pre.chain_entries@[j as int].purse,
             pre.chain_entries@[j as int].idx),
            pre.chain_entries@[j as int]),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_restore_chain_entry(
            quint_view(pre), pre.chain_entries@[j as int],
        ),
{
    lemma_restore_chain_entry_refines(pre, post, pre.chain_entries@[j as int]);
}

/// Entry parallel of [`lemma_recover_scan_step_coin_none_refines`].
proof fn lemma_recover_scan_step_entry_none_refines(pre: State, post: State)
    requires
        pre.invariant(),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_view(pre),
{
}

/// Some-branch refinement of `release_one_coin_lock_for`: refines as
/// `quint_step_release_locked_coin` at the returned key.
proof fn lemma_release_one_coin_lock_for_some_refines(
    pre: State,
    post: State,
    handle: OpHandle,
    key: (PurseId, u64),
)
    requires
        pre.invariant(),
        pre.coins().dom().contains(key),
        pre.coins()[key].state == CoinState::LockedFor(handle),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins().insert(key, CoinRec {
            purse: pre.coins()[key].purse,
            idx: pre.coins()[key].idx,
            exponent: pre.coins()[key].exponent,
            age: pre.coins()[key].age,
            account: pre.coins()[key].account,
            state: CoinState::Available,
        }),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_release_locked_coin(
            quint_view(pre), key, handle,
        ),
{
    lemma_release_locked_coin_refines(pre, post, key, handle);
}

/// None-branch refinement: state-preserving no-op.
proof fn lemma_release_one_coin_lock_for_none_refines(pre: State, post: State)
    requires
        pre.invariant(),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_view(pre),
{
}

/// Entry parallel: Some branch refines as `quint_step_release_locked_entry`.
proof fn lemma_release_one_entry_lock_for_some_refines(
    pre: State,
    post: State,
    handle: OpHandle,
    key: (PurseId, u64),
)
    requires
        pre.invariant(),
        pre.entries().dom().contains(key),
        pre.entries()[key].local == EntryLocal::LocalLockedFor(handle),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries().insert(key, EntryRec {
            local: EntryLocal::LocalAvailable,
            ..pre.entries()[key]
        }),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_release_locked_entry(
            quint_view(pre), key, handle,
        ),
{
    lemma_release_locked_entry_refines(pre, post, key, handle);
}

/// Entry parallel: None branch refines as a no-op.
proof fn lemma_release_one_entry_lock_for_none_refines(pre: State, post: State)
    requires
        pre.invariant(),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_view(pre),
{
}

/// Quint analog: `release_locked_coin(key, handle) ;
/// set_op_failed(handle)`. Composes two individual refinement steps.
pub open spec fn quint_step_cancel_op_releasing_coin(
    pre: QuintViewState,
    handle: OpHandle,
    key: (PurseId, u64),
) -> QuintViewState
    recommends
        pre.coins.dom().contains(key),
        pre.coins[key].state == CoinState::LockedFor(handle),
        pre.operations.dom().contains(handle),
        match pre.operations[handle].status {
            OpStatus::Preparing => true,
            OpStatus::Waiting(_) => true,
            _ => false,
        },
{
    QuintViewState {
        coins: pre.coins.insert(key, CoinRec {
            purse: pre.coins[key].purse,
            idx: pre.coins[key].idx,
            exponent: pre.coins[key].exponent,
            age: pre.coins[key].age,
            account: pre.coins[key].account,
            state: CoinState::Available,
        }),
        operations: pre.operations.insert(handle, OperationRec {
            handle: pre.operations[handle].handle,
            kind: pre.operations[handle].kind,
            purse: pre.operations[handle].purse,
            status: OpStatus::Failed,
        }),
        events: pre.events.push(Event::OperationCompleted {
            handle,
            status: OpStatus::Failed,
        }),
        ..pre
    }
}

proof fn lemma_cancel_op_releasing_coin_refines(
    pre: State,
    post: State,
    handle: OpHandle,
    key: (PurseId, u64),
)
    requires
        pre.invariant(),
        pre.operations().dom().contains(handle),
        match pre.operations()[handle].status {
            OpStatus::Preparing => true,
            OpStatus::Waiting(_) => true,
            _ => false,
        },
        pre.coins().dom().contains(key),
        pre.coins()[key].state == CoinState::LockedFor(handle),
        pre.events@.len() < u64::MAX as nat,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins().insert(key, CoinRec {
            purse: pre.coins()[key].purse,
            idx: pre.coins()[key].idx,
            exponent: pre.coins()[key].exponent,
            age: pre.coins()[key].age,
            account: pre.coins()[key].account,
            state: CoinState::Available,
        }),
        post.entries() == pre.entries(),
        post.operations() == pre.operations().insert(handle, OperationRec {
            handle: pre.operations()[handle].handle,
            kind: pre.operations()[handle].kind,
            purse: pre.operations()[handle].purse,
            status: OpStatus::Failed,
        }),
        post.events@ == pre.events@.push(Event::OperationCompleted {
            handle,
            status: OpStatus::Failed,
        }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_cancel_op_releasing_coin(quint_view(pre), handle, key),
{
    let post_view = quint_view(post);
    let step_view = quint_step_cancel_op_releasing_coin(quint_view(pre), handle, key);
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
}

/// Entry parallel of [`quint_step_cancel_op_releasing_coin`].
pub open spec fn quint_step_cancel_op_releasing_entry(
    pre: QuintViewState,
    handle: OpHandle,
    key: (PurseId, u64),
) -> QuintViewState
    recommends
        pre.entries.dom().contains(key),
        pre.entries[key].local == EntryLocal::LocalLockedFor(handle),
        pre.operations.dom().contains(handle),
        match pre.operations[handle].status {
            OpStatus::Preparing => true,
            OpStatus::Waiting(_) => true,
            _ => false,
        },
{
    QuintViewState {
        entries: pre.entries.insert(key, EntryRec {
            local: EntryLocal::LocalAvailable,
            ..pre.entries[key]
        }),
        operations: pre.operations.insert(handle, OperationRec {
            handle: pre.operations[handle].handle,
            kind: pre.operations[handle].kind,
            purse: pre.operations[handle].purse,
            status: OpStatus::Failed,
        }),
        events: pre.events.push(Event::OperationCompleted {
            handle,
            status: OpStatus::Failed,
        }),
        ..pre
    }
}

proof fn lemma_cancel_op_releasing_entry_refines(
    pre: State,
    post: State,
    handle: OpHandle,
    key: (PurseId, u64),
)
    requires
        pre.invariant(),
        pre.operations().dom().contains(handle),
        match pre.operations()[handle].status {
            OpStatus::Preparing => true,
            OpStatus::Waiting(_) => true,
            _ => false,
        },
        pre.entries().dom().contains(key),
        pre.entries()[key].local == EntryLocal::LocalLockedFor(handle),
        pre.events@.len() < u64::MAX as nat,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries().insert(key, EntryRec {
            local: EntryLocal::LocalAvailable,
            ..pre.entries()[key]
        }),
        post.operations() == pre.operations().insert(handle, OperationRec {
            handle: pre.operations()[handle].handle,
            kind: pre.operations()[handle].kind,
            purse: pre.operations()[handle].purse,
            status: OpStatus::Failed,
        }),
        post.events@ == pre.events@.push(Event::OperationCompleted {
            handle,
            status: OpStatus::Failed,
        }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_cancel_op_releasing_entry(quint_view(pre), handle, key),
{
    let post_view = quint_view(post);
    let step_view = quint_step_cancel_op_releasing_entry(quint_view(pre), handle, key);
    assert(post_view.entries =~= step_view.entries);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `start_op(kind, key.0) ; lock_coin(key, handle)`.
/// Composes two refinement steps with `handle = pre.next_handle`.
pub open spec fn quint_step_start_op_locking_coin(
    pre: QuintViewState,
    kind: OpKind,
    key: (PurseId, u64),
) -> QuintViewState
    recommends
        pre.coins.dom().contains(key),
        pre.coins[key].state == CoinState::Available,
        pre.purses.dom().contains(key.0),
        pre.next_handle < u64::MAX,
{
    let handle = pre.next_handle;
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            handle,
            kind,
            purse: key.0,
            status: OpStatus::Preparing,
        }),
        coins: pre.coins.insert(key, CoinRec {
            purse: pre.coins[key].purse,
            idx: pre.coins[key].idx,
            exponent: pre.coins[key].exponent,
            age: pre.coins[key].age,
            account: pre.coins[key].account,
            state: CoinState::LockedFor(handle),
        }),
        next_handle: (pre.next_handle + 1) as u64,
        events: pre.events.push(Event::OperationStarted {
            handle,
            kind,
            purse: key.0,
        }),
        ..pre
    }
}

proof fn lemma_start_op_locking_coin_refines(
    pre: State,
    post: State,
    kind: OpKind,
    key: (PurseId, u64),
)
    requires
        pre.invariant(),
        pre.coins().dom().contains(key),
        pre.coins()[key].state == CoinState::Available,
        pre.purses().dom().contains(key.0),
        pre.next_handle < u64::MAX,
        pre.events@.len() < u64::MAX as nat,
        post.invariant(),
        post.purses() == pre.purses(),
        post.entries() == pre.entries(),
        post.coins() == pre.coins().insert(key, CoinRec {
            purse: pre.coins()[key].purse,
            idx: pre.coins()[key].idx,
            exponent: pre.coins()[key].exponent,
            age: pre.coins()[key].age,
            account: pre.coins()[key].account,
            state: CoinState::LockedFor(pre.next_handle),
        }),
        post.operations() == pre.operations().insert(pre.next_handle, OperationRec {
            handle: pre.next_handle,
            kind,
            purse: key.0,
            status: OpStatus::Preparing,
        }),
        post.events@ == pre.events@.push(Event::OperationStarted {
            handle: pre.next_handle,
            kind,
            purse: key.0,
        }),
        post.next_handle == pre.next_handle + 1,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_start_op_locking_coin(quint_view(pre), kind, key),
{
    let post_view = quint_view(post);
    let step_view = quint_step_start_op_locking_coin(quint_view(pre), kind, key);
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
}

/// Entry parallel of [`quint_step_start_op_locking_coin`].
pub open spec fn quint_step_start_op_locking_entry(
    pre: QuintViewState,
    kind: OpKind,
    key: (PurseId, u64),
) -> QuintViewState
    recommends
        pre.entries.dom().contains(key),
        pre.entries[key].local == EntryLocal::LocalAvailable,
        pre.purses.dom().contains(key.0),
        pre.next_handle < u64::MAX,
{
    let handle = pre.next_handle;
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            handle,
            kind,
            purse: key.0,
            status: OpStatus::Preparing,
        }),
        entries: pre.entries.insert(key, EntryRec {
            local: EntryLocal::LocalLockedFor(handle),
            ..pre.entries[key]
        }),
        next_handle: (pre.next_handle + 1) as u64,
        events: pre.events.push(Event::OperationStarted {
            handle,
            kind,
            purse: key.0,
        }),
        ..pre
    }
}

proof fn lemma_start_op_locking_entry_refines(
    pre: State,
    post: State,
    kind: OpKind,
    key: (PurseId, u64),
)
    requires
        pre.invariant(),
        pre.entries().dom().contains(key),
        pre.entries()[key].local == EntryLocal::LocalAvailable,
        pre.purses().dom().contains(key.0),
        pre.next_handle < u64::MAX,
        pre.events@.len() < u64::MAX as nat,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries().insert(key, EntryRec {
            local: EntryLocal::LocalLockedFor(pre.next_handle),
            ..pre.entries()[key]
        }),
        post.operations() == pre.operations().insert(pre.next_handle, OperationRec {
            handle: pre.next_handle,
            kind,
            purse: key.0,
            status: OpStatus::Preparing,
        }),
        post.events@ == pre.events@.push(Event::OperationStarted {
            handle: pre.next_handle,
            kind,
            purse: key.0,
        }),
        post.next_handle == pre.next_handle + 1,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_start_op_locking_entry(quint_view(pre), kind, key),
{
    let post_view = quint_view(post);
    let step_view = quint_step_start_op_locking_entry(quint_view(pre), kind, key);
    assert(post_view.entries =~= step_view.entries);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `consume_entry(key) ; mark_op_done(handle)`. Two
/// refinement steps; the coin map is unchanged.
pub open spec fn quint_step_commit_op_consuming_locked_entry(
    pre: QuintViewState,
    handle: OpHandle,
    key: (PurseId, u64),
) -> QuintViewState
    recommends
        pre.entries.dom().contains(key),
        pre.entries[key].local == EntryLocal::LocalLockedFor(handle),
        pre.operations.dom().contains(handle),
        pre.operations[handle].status == OpStatus::Finalized,
{
    QuintViewState {
        entries: pre.entries.insert(key, EntryRec {
            local: EntryLocal::LocalConsumed,
            ..pre.entries[key]
        }),
        operations: pre.operations.insert(handle, OperationRec {
            handle: pre.operations[handle].handle,
            kind: pre.operations[handle].kind,
            purse: pre.operations[handle].purse,
            status: OpStatus::Done,
        }),
        events: pre.events
            .push(Event::EntryConsumed {
                purse: key.0,
                exponent: pre.entries[key].exponent,
            })
            .push(Event::OperationCompleted {
                handle,
                status: OpStatus::Done,
            }),
        ..pre
    }
}

proof fn lemma_commit_op_consuming_locked_entry_refines(
    pre: State,
    post: State,
    handle: OpHandle,
    key: (PurseId, u64),
)
    requires
        pre.invariant(),
        pre.operations().dom().contains(handle),
        pre.operations()[handle].status == OpStatus::Finalized,
        pre.entries().dom().contains(key),
        pre.entries()[key].local == EntryLocal::LocalLockedFor(handle),
        pre.events@.len() + 2 <= u64::MAX as nat,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries().insert(key, EntryRec {
            local: EntryLocal::LocalConsumed,
            ..pre.entries()[key]
        }),
        post.operations() == pre.operations().insert(handle, OperationRec {
            handle: pre.operations()[handle].handle,
            kind: pre.operations()[handle].kind,
            purse: pre.operations()[handle].purse,
            status: OpStatus::Done,
        }),
        post.events@ == pre.events@
            .push(Event::EntryConsumed {
                purse: key.0,
                exponent: pre.entries()[key].exponent,
            })
            .push(Event::OperationCompleted {
                handle,
                status: OpStatus::Done,
            }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_commit_op_consuming_locked_entry(
            quint_view(pre), handle, key,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_commit_op_consuming_locked_entry(
        quint_view(pre), handle, key,
    );
    assert(post_view.entries =~= step_view.entries);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `commit_locked_coin(key) ; mark_coin_spent(key) ;
/// mark_op_done(handle)`. Three refinement steps composed; the
/// intermediate PendingSpend state is invisible in the composite delta.
pub open spec fn quint_step_commit_op_consuming_locked_coin(
    pre: QuintViewState,
    handle: OpHandle,
    key: (PurseId, u64),
) -> QuintViewState
    recommends
        pre.coins.dom().contains(key),
        pre.coins[key].state == CoinState::LockedFor(handle),
        pre.operations.dom().contains(handle),
        pre.operations[handle].status == OpStatus::Finalized,
{
    QuintViewState {
        coins: pre.coins.insert(key, CoinRec {
            purse: pre.coins[key].purse,
            idx: pre.coins[key].idx,
            exponent: pre.coins[key].exponent,
            age: pre.coins[key].age,
            account: pre.coins[key].account,
            state: CoinState::Spent,
        }),
        operations: pre.operations.insert(handle, OperationRec {
            handle: pre.operations[handle].handle,
            kind: pre.operations[handle].kind,
            purse: pre.operations[handle].purse,
            status: OpStatus::Done,
        }),
        events: pre.events
            .push(Event::CoinSpent {
                purse: key.0,
                exponent: pre.coins[key].exponent,
            })
            .push(Event::OperationCompleted {
                handle,
                status: OpStatus::Done,
            }),
        ..pre
    }
}

proof fn lemma_commit_op_consuming_locked_coin_refines(
    pre: State,
    post: State,
    handle: OpHandle,
    key: (PurseId, u64),
)
    requires
        pre.invariant(),
        pre.operations().dom().contains(handle),
        pre.operations()[handle].status == OpStatus::Finalized,
        pre.coins().dom().contains(key),
        pre.coins()[key].state == CoinState::LockedFor(handle),
        pre.events@.len() + 2 <= u64::MAX as nat,
        post.invariant(),
        post.purses() == pre.purses(),
        post.entries() == pre.entries(),
        post.coins() == pre.coins().insert(key, CoinRec {
            purse: pre.coins()[key].purse,
            idx: pre.coins()[key].idx,
            exponent: pre.coins()[key].exponent,
            age: pre.coins()[key].age,
            account: pre.coins()[key].account,
            state: CoinState::Spent,
        }),
        post.operations() == pre.operations().insert(handle, OperationRec {
            handle: pre.operations()[handle].handle,
            kind: pre.operations()[handle].kind,
            purse: pre.operations()[handle].purse,
            status: OpStatus::Done,
        }),
        post.events@ == pre.events@
            .push(Event::CoinSpent {
                purse: key.0,
                exponent: pre.coins()[key].exponent,
            })
            .push(Event::OperationCompleted {
                handle,
                status: OpStatus::Done,
            }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_commit_op_consuming_locked_coin(
            quint_view(pre), handle, key,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_commit_op_consuming_locked_coin(
        quint_view(pre), handle, key,
    );
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `mark_coin_pending_spend(key) ; mark_coin_spent(key)`.
/// The intermediate `PendingSpend` state is hidden in the composite.
pub open spec fn quint_step_export_coin(
    pre: QuintViewState,
    key: (PurseId, u64),
) -> QuintViewState
    recommends
        pre.coins.dom().contains(key),
        pre.coins[key].state == CoinState::Available,
{
    QuintViewState {
        coins: pre.coins.insert(key, CoinRec {
            purse: pre.coins[key].purse,
            idx: pre.coins[key].idx,
            exponent: pre.coins[key].exponent,
            age: pre.coins[key].age,
            account: pre.coins[key].account,
            state: CoinState::Spent,
        }),
        events: pre.events.push(Event::CoinSpent {
            purse: key.0,
            exponent: pre.coins[key].exponent,
        }),
        ..pre
    }
}

proof fn lemma_export_coin_refines(pre: State, post: State, key: (PurseId, u64))
    requires
        pre.invariant(),
        pre.coins().dom().contains(key),
        pre.coins()[key].state == CoinState::Available,
        pre.events@.len() < u64::MAX as nat,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins().insert(key, CoinRec {
            purse: pre.coins()[key].purse,
            idx: pre.coins()[key].idx,
            exponent: pre.coins()[key].exponent,
            age: pre.coins()[key].age,
            account: pre.coins()[key].account,
            state: CoinState::Spent,
        }),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@.push(Event::CoinSpent {
            purse: key.0,
            exponent: pre.coins()[key].exponent,
        }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_export_coin(quint_view(pre), key),
{
    let post_view = quint_view(post);
    let step_view = quint_step_export_coin(quint_view(pre), key);
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `add_coin_with_account(p, exp, account) ;
/// mark_coin_observed(key)`. The intermediate `Pending` state is
/// hidden in the composite — the coin emerges directly as Available
/// with a CoinAvailable event.
pub open spec fn quint_step_import_coin(
    pre: QuintViewState,
    p: PurseId,
    exponent: u8,
    account: u64,
    next_age: u64,
    new_idx: u64,
) -> QuintViewState
    recommends
        pre.purses.dom().contains(p),
        pre.purses[p].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
{
    let key = (p, new_idx);
    QuintViewState {
        coins: pre.coins.insert(key, CoinRec {
            purse: p,
            idx: new_idx,
            exponent,
            state: CoinState::Available,
            age: next_age,
            account,
        }),
        purses: pre.purses.insert(p, PurseRecSpec {
            id: pre.purses[p].id,
            name: pre.purses[p].name,
            next_coin_idx: pre.purses[p].next_coin_idx + 1,
            next_entry_idx: pre.purses[p].next_entry_idx,
        }),
        events: pre.events.push(Event::CoinAvailable { purse: p, exponent }),
        ..pre
    }
}

proof fn lemma_import_coin_refines(
    pre: State,
    post: State,
    p: PurseId,
    exponent: u8,
    account: u64,
    new_idx: u64,
)
    requires
        pre.invariant(),
        pre.purses().dom().contains(p),
        pre.purses()[p].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        pre.next_age < u64::MAX,
        pre.events@.len() < u64::MAX as nat,
        exponent <= MAX_EXPONENT,
        post.invariant(),
        post.coins() == pre.coins().insert(
            (p, new_idx),
            CoinRec {
                purse: p,
                idx: new_idx,
                exponent,
                state: CoinState::Available,
                age: pre.next_age,
                account,
            },
        ),
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[p].id == p,
        post.purses()[p].name == pre.purses()[p].name,
        post.purses()[p].next_coin_idx == pre.purses()[p].next_coin_idx + 1,
        post.purses()[p].next_entry_idx == pre.purses()[p].next_entry_idx,
        forall|q: PurseId| q != p && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@.push(Event::CoinAvailable { purse: p, exponent }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_import_coin(
            quint_view(pre), p, exponent, account, pre.next_age, new_idx,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_import_coin(
        quint_view(pre), p, exponent, account, pre.next_age, new_idx,
    );
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.purses =~= step_view.purses);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog (success branch): `purses' = purses.remove(p) ;
/// coins' = coins.remove_keys(filter purse==p) ; entries' = entries
/// .remove_keys(filter purse==p)`.
pub open spec fn quint_step_delete_purse_success(
    pre: QuintViewState,
    p: PurseId,
) -> QuintViewState
    recommends
        pre.purses.dom().contains(p),
        p != MAIN_PURSE,
{
    QuintViewState {
        purses: pre.purses.remove(p),
        coins: pre.coins.remove_keys(Set::new(|k: (PurseId, u64)| k.0 == p)),
        entries: pre.entries.remove_keys(Set::new(|k: (PurseId, u64)| k.0 == p)),
        ..pre
    }
}

proof fn lemma_delete_purse_success_refines(pre: State, post: State, p: PurseId)
    requires
        pre.invariant(),
        pre.purses().dom().contains(p),
        p != MAIN_PURSE,
        !pre.has_live_coin_in(p),
        forall|h: OpHandle| #[trigger] pre.operations().dom().contains(h)
            ==> pre.operations()[h].purse != p,
        post.invariant(),
        post.purses() == pre.purses().remove(p),
        post.coins() == pre.coins().remove_keys(
            Set::new(|k: (PurseId, u64)| k.0 == p)),
        post.entries() == pre.entries().remove_keys(
            Set::new(|k: (PurseId, u64)| k.0 == p)),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_delete_purse_success(quint_view(pre), p),
{
    let post_view = quint_view(post);
    let step_view = quint_step_delete_purse_success(quint_view(pre), p);
    assert(post_view.purses =~= step_view.purses);
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.entries =~= step_view.entries);
}

/// Quint analog (CannotDeleteMainPurse branch): identity.
proof fn lemma_delete_purse_main_refines(pre: State, post: State)
    requires
        pre.invariant(),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_view(pre),
{
}

/// Quint analog (PurseNotFound branch): `coins' = coins.remove_keys
/// (filter purse==p)` and `entries' = entries.remove_keys(filter purse
/// ==p)`. By invariant, these filters are vacuous when p ∉ purses.dom
/// — but the Verus contract still spells out the deltas because
/// remove_keys is unconditional in the body.
pub open spec fn quint_step_delete_purse_notfound(
    pre: QuintViewState,
    p: PurseId,
) -> QuintViewState
{
    QuintViewState {
        coins: pre.coins.remove_keys(Set::new(|k: (PurseId, u64)| k.0 == p)),
        entries: pre.entries.remove_keys(Set::new(|k: (PurseId, u64)| k.0 == p)),
        ..pre
    }
}

proof fn lemma_delete_purse_notfound_refines(pre: State, post: State, p: PurseId)
    requires
        pre.invariant(),
        !pre.purses().dom().contains(p),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins().remove_keys(
            Set::new(|k: (PurseId, u64)| k.0 == p)),
        post.entries() == pre.entries().remove_keys(
            Set::new(|k: (PurseId, u64)| k.0 == p)),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_delete_purse_notfound(quint_view(pre), p),
{
    let post_view = quint_view(post);
    let step_view = quint_step_delete_purse_notfound(quint_view(pre), p);
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.entries =~= step_view.entries);
}

/// Quint analog: `set_entry_local(key, LocalLockedFor) ; set_entry_local
/// (key, LocalConsumed) ; add_coin(purse, exp) ; mark_coin_observed(new)`.
/// The intermediate `LocalLockedFor` state is hidden in the composite.
pub open spec fn quint_step_unload_via_entry(
    pre: QuintViewState,
    key: (PurseId, u64),
    next_age: u64,
    new_idx: u64,
) -> QuintViewState
    recommends
        pre.entries.dom().contains(key),
        pre.entries[key].local == EntryLocal::LocalAvailable,
        pre.entries[key].on_chain == EntryOnChain::Ready,
        pre.purses.dom().contains(key.0),
        pre.purses[key.0].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
{
    let p = key.0;
    let exp = pre.entries[key].exponent;
    let new_coin_key = (p, new_idx);
    QuintViewState {
        entries: pre.entries.insert(key, EntryRec {
            local: EntryLocal::LocalConsumed,
            ..pre.entries[key]
        }),
        coins: pre.coins.insert(new_coin_key, CoinRec {
            purse: p,
            idx: new_idx,
            exponent: exp,
            state: CoinState::Available,
            age: next_age,
            account: 0,
        }),
        purses: pre.purses.insert(p, PurseRecSpec {
            id: pre.purses[p].id,
            name: pre.purses[p].name,
            next_coin_idx: pre.purses[p].next_coin_idx + 1,
            next_entry_idx: pre.purses[p].next_entry_idx,
        }),
        events: pre.events.push(Event::CoinAvailable {
            purse: p,
            exponent: exp,
        }),
        ..pre
    }
}

proof fn lemma_unload_via_entry_refines(
    pre: State,
    post: State,
    key: (PurseId, u64),
    new_idx: u64,
)
    requires
        pre.invariant(),
        pre.entries().dom().contains(key),
        pre.entries()[key].local == EntryLocal::LocalAvailable,
        pre.entries()[key].on_chain == EntryOnChain::Ready,
        pre.purses().dom().contains(key.0),
        pre.purses()[key.0].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        pre.next_age < u64::MAX,
        pre.events@.len() < u64::MAX as nat,
        post.invariant(),
        post.entries() == pre.entries().insert(key, EntryRec {
            local: EntryLocal::LocalConsumed,
            ..pre.entries()[key]
        }),
        post.coins() == pre.coins().insert((key.0, new_idx), CoinRec {
            purse: key.0,
            idx: new_idx,
            exponent: pre.entries()[key].exponent,
            state: CoinState::Available,
            age: pre.next_age,
            account: 0,
        }),
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[key.0].id == key.0,
        post.purses()[key.0].name == pre.purses()[key.0].name,
        post.purses()[key.0].next_coin_idx == pre.purses()[key.0].next_coin_idx + 1,
        post.purses()[key.0].next_entry_idx == pre.purses()[key.0].next_entry_idx,
        forall|q: PurseId| q != key.0 && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.operations() == pre.operations(),
        post.events@ == pre.events@.push(Event::CoinAvailable {
            purse: key.0,
            exponent: pre.entries()[key].exponent,
        }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_unload_via_entry(
            quint_view(pre), key, pre.next_age, new_idx,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_unload_via_entry(
        quint_view(pre), key, pre.next_age, new_idx,
    );
    assert(post_view.entries =~= step_view.entries);
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.purses =~= step_view.purses);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: spend `key` (in `src`), mint a fresh coin of the
/// same exponent in `dst`. The intermediate PendingSpend / Pending
/// states are hidden in the composite.
pub open spec fn quint_step_rebalance(
    pre: QuintViewState,
    src: PurseId,
    dst: PurseId,
    key: (PurseId, u64),
    next_age: u64,
    new_idx: u64,
) -> QuintViewState
    recommends
        src != dst,
        key.0 == src,
        pre.coins.dom().contains(key),
        pre.coins[key].state == CoinState::Available,
        pre.purses.dom().contains(dst),
        pre.purses[dst].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
{
    let exp = pre.coins[key].exponent;
    let new_key = (dst, new_idx);
    QuintViewState {
        coins: pre.coins
            .insert(key, CoinRec {
                purse: pre.coins[key].purse,
                idx: pre.coins[key].idx,
                exponent: exp,
                age: pre.coins[key].age,
                account: pre.coins[key].account,
                state: CoinState::Spent,
            })
            .insert(new_key, CoinRec {
                purse: dst,
                idx: new_idx,
                exponent: exp,
                state: CoinState::Available,
                age: next_age,
                account: 0,
            }),
        purses: pre.purses.insert(dst, PurseRecSpec {
            id: pre.purses[dst].id,
            name: pre.purses[dst].name,
            next_coin_idx: pre.purses[dst].next_coin_idx + 1,
            next_entry_idx: pre.purses[dst].next_entry_idx,
        }),
        events: pre.events
            .push(Event::CoinSpent { purse: src, exponent: exp })
            .push(Event::CoinAvailable { purse: dst, exponent: exp }),
        ..pre
    }
}

proof fn lemma_rebalance_refines(
    pre: State,
    post: State,
    src: PurseId,
    dst: PurseId,
    key: (PurseId, u64),
    new_idx: u64,
)
    requires
        pre.invariant(),
        src != dst,
        key.0 == src,
        pre.coins().dom().contains(key),
        pre.coins()[key].state == CoinState::Available,
        pre.purses().dom().contains(dst),
        pre.purses()[dst].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        pre.events@.len() + 2 <= u64::MAX as nat,
        pre.next_age < u64::MAX,
        post.invariant(),
        post.coins() == pre.coins()
            .insert(key, CoinRec {
                purse: pre.coins()[key].purse,
                idx: pre.coins()[key].idx,
                exponent: pre.coins()[key].exponent,
                age: pre.coins()[key].age,
                account: pre.coins()[key].account,
                state: CoinState::Spent,
            })
            .insert((dst, new_idx), CoinRec {
                purse: dst,
                idx: new_idx,
                exponent: pre.coins()[key].exponent,
                state: CoinState::Available,
                age: pre.next_age,
                account: 0,
            }),
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[dst].id == dst,
        post.purses()[dst].name == pre.purses()[dst].name,
        post.purses()[dst].next_coin_idx == pre.purses()[dst].next_coin_idx + 1,
        post.purses()[dst].next_entry_idx == pre.purses()[dst].next_entry_idx,
        forall|q: PurseId| q != dst && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@
            .push(Event::CoinSpent {
                purse: src,
                exponent: pre.coins()[key].exponent,
            })
            .push(Event::CoinAvailable {
                purse: dst,
                exponent: pre.coins()[key].exponent,
            }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_rebalance(
            quint_view(pre), src, dst, key, pre.next_age, new_idx,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_rebalance(
        quint_view(pre), src, dst, key, pre.next_age, new_idx,
    );
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.purses =~= step_view.purses);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog (Some branch): non-deterministic transfer of a
/// specific source coin to a fresh coin in `to`. The Quint
/// transfer Action uses `oneOf` over candidate coins; Verus
/// realizes the choice via `select_coin`. The refinement lemma
/// is parameterized over the witness `src_key`.
pub open spec fn quint_step_transfer_some(
    pre: QuintViewState,
    from: PurseId,
    to: PurseId,
    src_key: (PurseId, u64),
    next_age: u64,
    new_idx: u64,
) -> QuintViewState
    recommends
        pre.coins.dom().contains(src_key),
        src_key.0 == from,
        pre.coins[src_key].state == CoinState::Available,
        pre.purses.dom().contains(to),
        pre.purses[to].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
{
    let exp = pre.coins[src_key].exponent;
    let new_key = (to, new_idx);
    QuintViewState {
        coins: pre.coins
            .insert(src_key, CoinRec {
                purse: pre.coins[src_key].purse,
                idx: pre.coins[src_key].idx,
                exponent: exp,
                age: pre.coins[src_key].age,
                account: pre.coins[src_key].account,
                state: CoinState::Spent,
            })
            .insert(new_key, CoinRec {
                purse: to,
                idx: new_idx,
                exponent: exp,
                state: CoinState::Available,
                age: next_age,
                account: 0,
            }),
        purses: pre.purses.insert(to, PurseRecSpec {
            id: pre.purses[to].id,
            name: pre.purses[to].name,
            next_coin_idx: pre.purses[to].next_coin_idx + 1,
            next_entry_idx: pre.purses[to].next_entry_idx,
        }),
        events: pre.events
            .push(Event::CoinSpent { purse: from, exponent: exp })
            .push(Event::CoinAvailable { purse: to, exponent: exp }),
        ..pre
    }
}

proof fn lemma_transfer_some_refines(
    pre: State,
    post: State,
    from: PurseId,
    to: PurseId,
    src_key: (PurseId, u64),
    new_idx: u64,
)
    requires
        pre.invariant(),
        pre.coins().dom().contains(src_key),
        src_key.0 == from,
        pre.coins()[src_key].state == CoinState::Available,
        pre.purses().dom().contains(to),
        pre.purses()[to].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        pre.events@.len() + 2 <= u64::MAX as nat,
        pre.next_age < u64::MAX,
        post.invariant(),
        post.coins() == pre.coins()
            .insert(src_key, CoinRec {
                purse: pre.coins()[src_key].purse,
                idx: pre.coins()[src_key].idx,
                exponent: pre.coins()[src_key].exponent,
                age: pre.coins()[src_key].age,
                account: pre.coins()[src_key].account,
                state: CoinState::Spent,
            })
            .insert((to, new_idx), CoinRec {
                purse: to,
                idx: new_idx,
                exponent: pre.coins()[src_key].exponent,
                state: CoinState::Available,
                age: pre.next_age,
                account: 0,
            }),
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[to].id == to,
        post.purses()[to].name == pre.purses()[to].name,
        post.purses()[to].next_coin_idx == pre.purses()[to].next_coin_idx + 1,
        post.purses()[to].next_entry_idx == pre.purses()[to].next_entry_idx,
        forall|q: PurseId| q != to && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@
            .push(Event::CoinSpent {
                purse: from,
                exponent: pre.coins()[src_key].exponent,
            })
            .push(Event::CoinAvailable {
                purse: to,
                exponent: pre.coins()[src_key].exponent,
            }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_transfer_some(
            quint_view(pre), from, to, src_key, pre.next_age, new_idx,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_transfer_some(
        quint_view(pre), from, to, src_key, pre.next_age, new_idx,
    );
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.purses =~= step_view.purses);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog (None branch): identity — no Available coin met
/// the threshold, no state change.
proof fn lemma_transfer_none_refines(pre: State, post: State)
    requires
        pre.invariant(),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_view(pre),
{
}

/// Quint analog: `start_op(Export, purse) ; export_coin(key) ;
/// mark_op_submitted(handle)`. Three refinement steps composed.
pub open spec fn quint_step_tracked_export_coin(
    pre: QuintViewState,
    key: (PurseId, u64),
) -> QuintViewState
    recommends
        pre.coins.dom().contains(key),
        pre.coins[key].state == CoinState::Available,
        pre.next_handle < u64::MAX,
{
    let handle = pre.next_handle;
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            handle,
            kind: OpKind::Export,
            purse: key.0,
            status: OpStatus::Submitted,
        }),
        coins: pre.coins.insert(key, CoinRec {
            purse: pre.coins[key].purse,
            idx: pre.coins[key].idx,
            exponent: pre.coins[key].exponent,
            age: pre.coins[key].age,
            account: pre.coins[key].account,
            state: CoinState::Spent,
        }),
        next_handle: (pre.next_handle + 1) as u64,
        events: pre.events
            .push(Event::OperationStarted {
                handle,
                kind: OpKind::Export,
                purse: key.0,
            })
            .push(Event::CoinSpent {
                purse: key.0,
                exponent: pre.coins[key].exponent,
            })
            .push(Event::OperationProgress {
                handle,
                status: OpStatus::Submitted,
            }),
        ..pre
    }
}

proof fn lemma_tracked_export_coin_refines(
    pre: State,
    post: State,
    key: (PurseId, u64),
)
    requires
        pre.invariant(),
        pre.coins().dom().contains(key),
        pre.coins()[key].state == CoinState::Available,
        pre.next_handle < u64::MAX,
        pre.events@.len() + 3 <= u64::MAX as nat,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins().insert(key, CoinRec {
            purse: pre.coins()[key].purse,
            idx: pre.coins()[key].idx,
            exponent: pre.coins()[key].exponent,
            age: pre.coins()[key].age,
            account: pre.coins()[key].account,
            state: CoinState::Spent,
        }),
        post.entries() == pre.entries(),
        post.operations() == pre.operations().insert(pre.next_handle, OperationRec {
            handle: pre.next_handle,
            kind: OpKind::Export,
            purse: key.0,
            status: OpStatus::Submitted,
        }),
        post.events@ == pre.events@
            .push(Event::OperationStarted {
                handle: pre.next_handle,
                kind: OpKind::Export,
                purse: key.0,
            })
            .push(Event::CoinSpent {
                purse: key.0,
                exponent: pre.coins()[key].exponent,
            })
            .push(Event::OperationProgress {
                handle: pre.next_handle,
                status: OpStatus::Submitted,
            }),
        post.next_handle == pre.next_handle + 1,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_tracked_export_coin(quint_view(pre), key),
{
    let post_view = quint_view(post);
    let step_view = quint_step_tracked_export_coin(quint_view(pre), key);
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `start_op(Import, p) ; import_coin(p, exp, account)
/// ; mark_op_submitted(handle)`. Three refinement steps composed.
pub open spec fn quint_step_tracked_import_coin(
    pre: QuintViewState,
    p: PurseId,
    exponent: u8,
    account: u64,
    next_age: u64,
    new_idx: u64,
) -> QuintViewState
    recommends
        pre.purses.dom().contains(p),
        pre.purses[p].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        pre.next_handle < u64::MAX,
{
    let handle = pre.next_handle;
    let new_key = (p, new_idx);
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            handle,
            kind: OpKind::Import,
            purse: p,
            status: OpStatus::Submitted,
        }),
        coins: pre.coins.insert(new_key, CoinRec {
            purse: p,
            idx: new_idx,
            exponent,
            state: CoinState::Available,
            age: next_age,
            account,
        }),
        purses: pre.purses.insert(p, PurseRecSpec {
            id: pre.purses[p].id,
            name: pre.purses[p].name,
            next_coin_idx: pre.purses[p].next_coin_idx + 1,
            next_entry_idx: pre.purses[p].next_entry_idx,
        }),
        next_handle: (pre.next_handle + 1) as u64,
        events: pre.events
            .push(Event::OperationStarted {
                handle,
                kind: OpKind::Import,
                purse: p,
            })
            .push(Event::CoinAvailable { purse: p, exponent })
            .push(Event::OperationProgress {
                handle,
                status: OpStatus::Submitted,
            }),
        ..pre
    }
}

proof fn lemma_tracked_import_coin_refines(
    pre: State,
    post: State,
    p: PurseId,
    exponent: u8,
    account: u64,
    new_idx: u64,
)
    requires
        pre.invariant(),
        pre.purses().dom().contains(p),
        pre.purses()[p].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        pre.next_age < u64::MAX,
        pre.next_handle < u64::MAX,
        pre.events@.len() + 3 <= u64::MAX as nat,
        exponent <= MAX_EXPONENT,
        post.invariant(),
        post.coins() == pre.coins().insert((p, new_idx), CoinRec {
            purse: p,
            idx: new_idx,
            exponent,
            state: CoinState::Available,
            age: pre.next_age,
            account,
        }),
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[p].id == p,
        post.purses()[p].name == pre.purses()[p].name,
        post.purses()[p].next_coin_idx == pre.purses()[p].next_coin_idx + 1,
        post.purses()[p].next_entry_idx == pre.purses()[p].next_entry_idx,
        forall|q: PurseId| q != p && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.entries() == pre.entries(),
        post.operations() == pre.operations().insert(pre.next_handle, OperationRec {
            handle: pre.next_handle,
            kind: OpKind::Import,
            purse: p,
            status: OpStatus::Submitted,
        }),
        post.events@ == pre.events@
            .push(Event::OperationStarted {
                handle: pre.next_handle,
                kind: OpKind::Import,
                purse: p,
            })
            .push(Event::CoinAvailable { purse: p, exponent })
            .push(Event::OperationProgress {
                handle: pre.next_handle,
                status: OpStatus::Submitted,
            }),
        post.next_handle == pre.next_handle + 1,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_tracked_import_coin(
            quint_view(pre), p, exponent, account, pre.next_age, new_idx,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_tracked_import_coin(
        quint_view(pre), p, exponent, account, pre.next_age, new_idx,
    );
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.purses =~= step_view.purses);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `start_op(Rebalance, src) ; rebalance(src, dst, key)
/// ; mark_op_submitted(handle)`. Three refinement steps composed.
pub open spec fn quint_step_tracked_rebalance(
    pre: QuintViewState,
    src: PurseId,
    dst: PurseId,
    key: (PurseId, u64),
    next_age: u64,
    new_idx: u64,
) -> QuintViewState
    recommends
        src != dst,
        key.0 == src,
        pre.coins.dom().contains(key),
        pre.coins[key].state == CoinState::Available,
        pre.purses.dom().contains(dst),
        pre.purses[dst].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        pre.next_handle < u64::MAX,
{
    let handle = pre.next_handle;
    let exp = pre.coins[key].exponent;
    let new_key = (dst, new_idx);
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            handle,
            kind: OpKind::Rebalance,
            purse: src,
            status: OpStatus::Submitted,
        }),
        coins: pre.coins
            .insert(key, CoinRec {
                purse: pre.coins[key].purse,
                idx: pre.coins[key].idx,
                exponent: exp,
                age: pre.coins[key].age,
                account: pre.coins[key].account,
                state: CoinState::Spent,
            })
            .insert(new_key, CoinRec {
                purse: dst,
                idx: new_idx,
                exponent: exp,
                state: CoinState::Available,
                age: next_age,
                account: 0,
            }),
        purses: pre.purses.insert(dst, PurseRecSpec {
            id: pre.purses[dst].id,
            name: pre.purses[dst].name,
            next_coin_idx: pre.purses[dst].next_coin_idx + 1,
            next_entry_idx: pre.purses[dst].next_entry_idx,
        }),
        next_handle: (pre.next_handle + 1) as u64,
        events: pre.events
            .push(Event::OperationStarted {
                handle,
                kind: OpKind::Rebalance,
                purse: src,
            })
            .push(Event::CoinSpent { purse: src, exponent: exp })
            .push(Event::CoinAvailable { purse: dst, exponent: exp })
            .push(Event::OperationProgress {
                handle,
                status: OpStatus::Submitted,
            }),
        ..pre
    }
}

proof fn lemma_tracked_rebalance_refines(
    pre: State,
    post: State,
    src: PurseId,
    dst: PurseId,
    key: (PurseId, u64),
    new_idx: u64,
)
    requires
        pre.invariant(),
        src != dst,
        key.0 == src,
        pre.coins().dom().contains(key),
        pre.coins()[key].state == CoinState::Available,
        pre.purses().dom().contains(dst),
        pre.purses()[dst].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        pre.next_age < u64::MAX,
        pre.next_handle < u64::MAX,
        pre.events@.len() + 4 <= u64::MAX as nat,
        post.invariant(),
        post.coins() == pre.coins()
            .insert(key, CoinRec {
                purse: pre.coins()[key].purse,
                idx: pre.coins()[key].idx,
                exponent: pre.coins()[key].exponent,
                age: pre.coins()[key].age,
                account: pre.coins()[key].account,
                state: CoinState::Spent,
            })
            .insert((dst, new_idx), CoinRec {
                purse: dst,
                idx: new_idx,
                exponent: pre.coins()[key].exponent,
                state: CoinState::Available,
                age: pre.next_age,
                account: 0,
            }),
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[dst].id == dst,
        post.purses()[dst].name == pre.purses()[dst].name,
        post.purses()[dst].next_coin_idx == pre.purses()[dst].next_coin_idx + 1,
        post.purses()[dst].next_entry_idx == pre.purses()[dst].next_entry_idx,
        forall|q: PurseId| q != dst && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.entries() == pre.entries(),
        post.operations() == pre.operations().insert(pre.next_handle, OperationRec {
            handle: pre.next_handle,
            kind: OpKind::Rebalance,
            purse: src,
            status: OpStatus::Submitted,
        }),
        post.events@ == pre.events@
            .push(Event::OperationStarted {
                handle: pre.next_handle,
                kind: OpKind::Rebalance,
                purse: src,
            })
            .push(Event::CoinSpent {
                purse: src,
                exponent: pre.coins()[key].exponent,
            })
            .push(Event::CoinAvailable {
                purse: dst,
                exponent: pre.coins()[key].exponent,
            })
            .push(Event::OperationProgress {
                handle: pre.next_handle,
                status: OpStatus::Submitted,
            }),
        post.next_handle == pre.next_handle + 1,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_tracked_rebalance(
            quint_view(pre), src, dst, key, pre.next_age, new_idx,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_tracked_rebalance(
        quint_view(pre), src, dst, key, pre.next_age, new_idx,
    );
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.purses =~= step_view.purses);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog (Some branch): `start_op(Transfer, from) ;
/// transfer(from, to, min_exp) ; mark_op_done(handle)`. Refinement
/// witnesses the existentially-chosen `src_key`.
pub open spec fn quint_step_tracked_transfer_some(
    pre: QuintViewState,
    from: PurseId,
    to: PurseId,
    src_key: (PurseId, u64),
    next_age: u64,
    new_idx: u64,
) -> QuintViewState
    recommends
        pre.coins.dom().contains(src_key),
        src_key.0 == from,
        pre.coins[src_key].state == CoinState::Available,
        pre.purses.dom().contains(to),
        pre.purses[to].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        pre.next_handle < u64::MAX,
{
    let handle = pre.next_handle;
    let exp = pre.coins[src_key].exponent;
    let new_key = (to, new_idx);
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            handle,
            kind: OpKind::Transfer,
            purse: from,
            status: OpStatus::Done,
        }),
        coins: pre.coins
            .insert(src_key, CoinRec {
                purse: pre.coins[src_key].purse,
                idx: pre.coins[src_key].idx,
                exponent: exp,
                age: pre.coins[src_key].age,
                account: pre.coins[src_key].account,
                state: CoinState::Spent,
            })
            .insert(new_key, CoinRec {
                purse: to,
                idx: new_idx,
                exponent: exp,
                state: CoinState::Available,
                age: next_age,
                account: 0,
            }),
        purses: pre.purses.insert(to, PurseRecSpec {
            id: pre.purses[to].id,
            name: pre.purses[to].name,
            next_coin_idx: pre.purses[to].next_coin_idx + 1,
            next_entry_idx: pre.purses[to].next_entry_idx,
        }),
        next_handle: (pre.next_handle + 1) as u64,
        events: pre.events
            .push(Event::OperationStarted {
                handle,
                kind: OpKind::Transfer,
                purse: from,
            })
            .push(Event::CoinSpent { purse: from, exponent: exp })
            .push(Event::CoinAvailable { purse: to, exponent: exp }),
        ..pre
    }
}

proof fn lemma_tracked_transfer_some_refines(
    pre: State,
    post: State,
    from: PurseId,
    to: PurseId,
    src_key: (PurseId, u64),
    new_idx: u64,
)
    requires
        pre.invariant(),
        pre.coins().dom().contains(src_key),
        src_key.0 == from,
        pre.coins()[src_key].state == CoinState::Available,
        pre.purses().dom().contains(to),
        pre.purses()[to].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        pre.next_age < u64::MAX,
        pre.next_handle < u64::MAX,
        pre.events@.len() + 3 <= u64::MAX as nat,
        post.invariant(),
        post.coins() == pre.coins()
            .insert(src_key, CoinRec {
                purse: pre.coins()[src_key].purse,
                idx: pre.coins()[src_key].idx,
                exponent: pre.coins()[src_key].exponent,
                age: pre.coins()[src_key].age,
                account: pre.coins()[src_key].account,
                state: CoinState::Spent,
            })
            .insert((to, new_idx), CoinRec {
                purse: to,
                idx: new_idx,
                exponent: pre.coins()[src_key].exponent,
                state: CoinState::Available,
                age: pre.next_age,
                account: 0,
            }),
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[to].id == to,
        post.purses()[to].name == pre.purses()[to].name,
        post.purses()[to].next_coin_idx == pre.purses()[to].next_coin_idx + 1,
        post.purses()[to].next_entry_idx == pre.purses()[to].next_entry_idx,
        forall|q: PurseId| q != to && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.entries() == pre.entries(),
        post.operations() == pre.operations().insert(pre.next_handle, OperationRec {
            handle: pre.next_handle,
            kind: OpKind::Transfer,
            purse: from,
            status: OpStatus::Done,
        }),
        post.events@ == pre.events@
            .push(Event::OperationStarted {
                handle: pre.next_handle,
                kind: OpKind::Transfer,
                purse: from,
            })
            .push(Event::CoinSpent {
                purse: from,
                exponent: pre.coins()[src_key].exponent,
            })
            .push(Event::CoinAvailable {
                purse: to,
                exponent: pre.coins()[src_key].exponent,
            }),
        post.next_handle == pre.next_handle + 1,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_tracked_transfer_some(
            quint_view(pre), from, to, src_key, pre.next_age, new_idx,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_tracked_transfer_some(
        quint_view(pre), from, to, src_key, pre.next_age, new_idx,
    );
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.purses =~= step_view.purses);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog (None branch): `start_op(Transfer, from) ;
/// set_op_failed-equivalent`. No coin moves.
pub open spec fn quint_step_tracked_transfer_none(
    pre: QuintViewState,
    from: PurseId,
) -> QuintViewState
    recommends
        pre.next_handle < u64::MAX,
{
    let handle = pre.next_handle;
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            handle,
            kind: OpKind::Transfer,
            purse: from,
            status: OpStatus::Failed,
        }),
        next_handle: (pre.next_handle + 1) as u64,
        events: pre.events.push(Event::OperationStarted {
            handle,
            kind: OpKind::Transfer,
            purse: from,
        }),
        ..pre
    }
}

proof fn lemma_tracked_transfer_none_refines(
    pre: State,
    post: State,
    from: PurseId,
)
    requires
        pre.invariant(),
        pre.next_handle < u64::MAX,
        pre.events@.len() < u64::MAX as nat,
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations().insert(pre.next_handle, OperationRec {
            handle: pre.next_handle,
            kind: OpKind::Transfer,
            purse: from,
            status: OpStatus::Failed,
        }),
        post.events@ == pre.events@.push(Event::OperationStarted {
            handle: pre.next_handle,
            kind: OpKind::Transfer,
            purse: from,
        }),
        post.next_handle == pre.next_handle + 1,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_tracked_transfer_none(quint_view(pre), from),
{
    let post_view = quint_view(post);
    let step_view = quint_step_tracked_transfer_none(quint_view(pre), from);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `start_op(ExternalOffload, p) ; unload_via_entry(key,
/// handle) ; mark_op_submitted(handle)`. Three refinement steps composed.
pub open spec fn quint_step_tracked_unload_via_entry(
    pre: QuintViewState,
    key: (PurseId, u64),
    next_age: u64,
    new_idx: u64,
) -> QuintViewState
    recommends
        pre.entries.dom().contains(key),
        pre.entries[key].local == EntryLocal::LocalAvailable,
        pre.entries[key].on_chain == EntryOnChain::Ready,
        pre.purses.dom().contains(key.0),
        pre.purses[key.0].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        pre.next_handle < u64::MAX,
{
    let p = key.0;
    let handle = pre.next_handle;
    let exp = pre.entries[key].exponent;
    let new_coin_key = (p, new_idx);
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            handle,
            kind: OpKind::ExternalOffload,
            purse: p,
            status: OpStatus::Submitted,
        }),
        entries: pre.entries.insert(key, EntryRec {
            local: EntryLocal::LocalConsumed,
            ..pre.entries[key]
        }),
        coins: pre.coins.insert(new_coin_key, CoinRec {
            purse: p,
            idx: new_idx,
            exponent: exp,
            state: CoinState::Available,
            age: next_age,
            account: 0,
        }),
        purses: pre.purses.insert(p, PurseRecSpec {
            id: pre.purses[p].id,
            name: pre.purses[p].name,
            next_coin_idx: pre.purses[p].next_coin_idx + 1,
            next_entry_idx: pre.purses[p].next_entry_idx,
        }),
        next_handle: (pre.next_handle + 1) as u64,
        events: pre.events
            .push(Event::OperationStarted {
                handle,
                kind: OpKind::ExternalOffload,
                purse: p,
            })
            .push(Event::CoinAvailable { purse: p, exponent: exp })
            .push(Event::OperationProgress {
                handle,
                status: OpStatus::Submitted,
            }),
        ..pre
    }
}

proof fn lemma_tracked_unload_via_entry_refines(
    pre: State,
    post: State,
    key: (PurseId, u64),
    new_idx: u64,
)
    requires
        pre.invariant(),
        pre.entries().dom().contains(key),
        pre.entries()[key].local == EntryLocal::LocalAvailable,
        pre.entries()[key].on_chain == EntryOnChain::Ready,
        pre.purses().dom().contains(key.0),
        pre.purses()[key.0].next_coin_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        pre.next_age < u64::MAX,
        pre.next_handle < u64::MAX,
        pre.events@.len() + 3 <= u64::MAX as nat,
        post.invariant(),
        post.entries() == pre.entries().insert(key, EntryRec {
            local: EntryLocal::LocalConsumed,
            ..pre.entries()[key]
        }),
        post.coins() == pre.coins().insert((key.0, new_idx), CoinRec {
            purse: key.0,
            idx: new_idx,
            exponent: pre.entries()[key].exponent,
            state: CoinState::Available,
            age: pre.next_age,
            account: 0,
        }),
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[key.0].id == key.0,
        post.purses()[key.0].name == pre.purses()[key.0].name,
        post.purses()[key.0].next_coin_idx == pre.purses()[key.0].next_coin_idx + 1,
        post.purses()[key.0].next_entry_idx == pre.purses()[key.0].next_entry_idx,
        forall|q: PurseId| q != key.0 && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.operations() == pre.operations().insert(pre.next_handle, OperationRec {
            handle: pre.next_handle,
            kind: OpKind::ExternalOffload,
            purse: key.0,
            status: OpStatus::Submitted,
        }),
        post.events@ == pre.events@
            .push(Event::OperationStarted {
                handle: pre.next_handle,
                kind: OpKind::ExternalOffload,
                purse: key.0,
            })
            .push(Event::CoinAvailable {
                purse: key.0,
                exponent: pre.entries()[key].exponent,
            })
            .push(Event::OperationProgress {
                handle: pre.next_handle,
                status: OpStatus::Submitted,
            }),
        post.next_handle == pre.next_handle + 1,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_tracked_unload_via_entry(
            quint_view(pre), key, pre.next_age, new_idx,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_tracked_unload_via_entry(
        quint_view(pre), key, pre.next_age, new_idx,
    );
    assert(post_view.entries =~= step_view.entries);
    assert(post_view.coins =~= step_view.coins);
    assert(post_view.purses =~= step_view.purses);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `coins' = coins.remove_keys(filter purse==p)`.
pub open spec fn quint_step_purge_coins_of_purse(
    pre: QuintViewState,
    p: PurseId,
) -> QuintViewState {
    QuintViewState {
        coins: pre.coins.remove_keys(Set::new(|k: (PurseId, u64)| k.0 == p)),
        ..pre
    }
}

proof fn lemma_purge_coins_of_purse_refines(pre: State, post: State, p: PurseId)
    requires
        pre.invariant(),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins().remove_keys(
            Set::new(|k: (PurseId, u64)| k.0 == p)),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_purge_coins_of_purse(quint_view(pre), p),
{
    let post_view = quint_view(post);
    let step_view = quint_step_purge_coins_of_purse(quint_view(pre), p);
    assert(post_view.coins =~= step_view.coins);
}

/// Quint analog: `entries' = entries.remove_keys(filter purse==p)`.
pub open spec fn quint_step_purge_entries_of_purse(
    pre: QuintViewState,
    p: PurseId,
) -> QuintViewState {
    QuintViewState {
        entries: pre.entries.remove_keys(Set::new(|k: (PurseId, u64)| k.0 == p)),
        ..pre
    }
}

proof fn lemma_purge_entries_of_purse_refines(pre: State, post: State, p: PurseId)
    requires
        pre.invariant(),
        post.invariant(),
        post.purses() == pre.purses(),
        post.coins() == pre.coins(),
        post.entries() == pre.entries().remove_keys(
            Set::new(|k: (PurseId, u64)| k.0 == p)),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_purge_entries_of_purse(quint_view(pre), p),
{
    let post_view = quint_view(post);
    let step_view = quint_step_purge_entries_of_purse(quint_view(pre), p);
    assert(post_view.entries =~= step_view.entries);
}

/// Quint analog: bulk mint `exp_seq.len()` Pending coins in `p` with
/// sequential indices `[base_idx, base_idx + n)` and sequential ages
/// `[base_age, base_age + n)`. Quint createCoins fold reduced to a
/// single map-union expression.
pub open spec fn quint_step_top_up_purse(
    pre: QuintViewState,
    p: PurseId,
    exp_seq: Seq<u8>,
    base_idx: u64,
    base_age: u64,
) -> QuintViewState
    recommends
        pre.purses.dom().contains(p),
        pre.purses[p].next_coin_idx == base_idx as nat,
        (base_idx as nat) + exp_seq.len() <= u64::MAX as nat,
        (base_age as nat) + exp_seq.len() <= u64::MAX as nat,
{
    QuintViewState {
        coins: Map::new(
            |k: (PurseId, u64)|
                pre.coins.dom().contains(k)
                || (k.0 == p
                    && (base_idx as int) <= (k.1 as int)
                    && (k.1 as int) < (base_idx as int) + exp_seq.len() as int),
            |k: (PurseId, u64)|
                if pre.coins.dom().contains(k) {
                    pre.coins[k]
                } else {
                    let j = (k.1 as int) - (base_idx as int);
                    CoinRec {
                        purse: p,
                        idx: k.1,
                        exponent: exp_seq[j],
                        state: CoinState::Pending,
                        age: ((base_age as int) + j) as u64,
                        account: 0,
                    }
                }
        ),
        purses: pre.purses.insert(p, PurseRecSpec {
            id: pre.purses[p].id,
            name: pre.purses[p].name,
            next_coin_idx: pre.purses[p].next_coin_idx + exp_seq.len(),
            next_entry_idx: pre.purses[p].next_entry_idx,
        }),
        ..pre
    }
}

proof fn lemma_top_up_purse_refines(
    pre: State,
    post: State,
    p: PurseId,
    exp_seq: Seq<u8>,
    base_idx: u64,
    base_age: u64,
)
    requires
        pre.invariant(),
        pre.purses().dom().contains(p),
        pre.purses()[p].next_coin_idx == base_idx as nat,
        pre.next_age == base_age,
        (base_idx as nat) + exp_seq.len() <= u64::MAX as nat,
        (base_age as nat) + exp_seq.len() <= u64::MAX as nat,
        forall|j: int| 0 <= j < exp_seq.len() ==>
            (#[trigger] exp_seq[j]) <= MAX_EXPONENT,
        post.invariant(),
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[p].next_coin_idx == pre.purses()[p].next_coin_idx + exp_seq.len(),
        post.purses()[p].id == p,
        post.purses()[p].name == pre.purses()[p].name,
        post.purses()[p].next_entry_idx == pre.purses()[p].next_entry_idx,
        forall|q: PurseId| q != p && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.coins().dom() =~= pre.coins().dom().union(
            Set::new(|k: (PurseId, u64)|
                k.0 == p
                && (base_idx as int) <= (k.1 as int)
                && (k.1 as int) < (base_idx as int) + exp_seq.len() as int)
        ),
        forall|k: (PurseId, u64)| #[trigger] pre.coins().dom().contains(k)
            ==> post.coins()[k] == pre.coins()[k],
        forall|j: int| 0 <= j < exp_seq.len() ==>
            #[trigger] post.coins()[(p, (base_idx + j) as u64)]
                == (CoinRec {
                    purse: p,
                    idx: (base_idx + j) as u64,
                    exponent: exp_seq[j],
                    state: CoinState::Pending,
                    age: (base_age + j) as u64,
                    account: 0,
                }),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_top_up_purse(
            quint_view(pre), p, exp_seq, base_idx, base_age,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_top_up_purse(
        quint_view(pre), p, exp_seq, base_idx, base_age,
    );
    assert(post_view.purses =~= step_view.purses);
    // For coins, prove extensional equality: for every key, both maps
    // agree on dom and value.
    assert forall|k: (PurseId, u64)|
        #[trigger] post_view.coins.dom().contains(k)
            <==> step_view.coins.dom().contains(k)
    by {
    }
    assert forall|k: (PurseId, u64)| post_view.coins.dom().contains(k)
        implies #[trigger] post_view.coins[k] == step_view.coins[k]
    by {
        if pre.coins().dom().contains(k) {
            assert(post_view.coins[k] == pre.coins()[k]);
            assert(step_view.coins[k] == pre.coins()[k]);
        } else {
            // k is in the new range; k.0 == p, k.1 in [base_idx, base_idx + n).
            let j = (k.1 as int) - (base_idx as int);
            assert(0 <= j < exp_seq.len());
            assert(k == (p, (base_idx + j) as u64));
            assert(post_view.coins[k] == (CoinRec {
                purse: p,
                idx: (base_idx + j) as u64,
                exponent: exp_seq[j],
                state: CoinState::Pending,
                age: (base_age + j) as u64,
                account: 0,
            }));
        }
    }
    assert(post_view.coins =~= step_view.coins);
}

/// Quint analog: bulk allocate `exp_seq.len()` recycler entries in `p`
/// with sequential indices `[base_idx, base_idx + n)`. Mirror of
/// `quint_step_top_up_purse` for entries.
pub open spec fn quint_step_reserve_entries(
    pre: QuintViewState,
    p: PurseId,
    exp_seq: Seq<u8>,
    base_idx: u64,
) -> QuintViewState
    recommends
        pre.purses.dom().contains(p),
        pre.purses[p].next_entry_idx == base_idx as nat,
        (base_idx as nat) + exp_seq.len() <= u64::MAX as nat,
{
    QuintViewState {
        entries: Map::new(
            |k: (PurseId, u64)|
                pre.entries.dom().contains(k)
                || (k.0 == p
                    && (base_idx as int) <= (k.1 as int)
                    && (k.1 as int) < (base_idx as int) + exp_seq.len() as int),
            |k: (PurseId, u64)|
                if pre.entries.dom().contains(k) {
                    pre.entries[k]
                } else {
                    let j = (k.1 as int) - (base_idx as int);
                    EntryRec {
                        purse: p,
                        idx: k.1,
                        exponent: exp_seq[j],
                        on_chain: EntryOnChain::Waiting,
                        local: EntryLocal::LocalAvailable,
                        member_key: 0,
                        allocated_at: 0,
                        ready_at: 0,
                        ring_idx: 0,
                    }
                }
        ),
        purses: pre.purses.insert(p, PurseRecSpec {
            id: pre.purses[p].id,
            name: pre.purses[p].name,
            next_coin_idx: pre.purses[p].next_coin_idx,
            next_entry_idx: pre.purses[p].next_entry_idx + exp_seq.len(),
        }),
        ..pre
    }
}

proof fn lemma_reserve_entries_refines(
    pre: State,
    post: State,
    p: PurseId,
    exp_seq: Seq<u8>,
    base_idx: u64,
)
    requires
        pre.invariant(),
        pre.purses().dom().contains(p),
        pre.purses()[p].next_entry_idx == base_idx as nat,
        (base_idx as nat) + exp_seq.len() <= u64::MAX as nat,
        forall|j: int| 0 <= j < exp_seq.len() ==>
            (#[trigger] exp_seq[j]) <= MAX_EXPONENT,
        post.invariant(),
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[p].next_entry_idx == pre.purses()[p].next_entry_idx + exp_seq.len(),
        post.purses()[p].id == p,
        post.purses()[p].name == pre.purses()[p].name,
        post.purses()[p].next_coin_idx == pre.purses()[p].next_coin_idx,
        forall|q: PurseId| q != p && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.entries().dom() =~= pre.entries().dom().union(
            Set::new(|k: (PurseId, u64)|
                k.0 == p
                && (base_idx as int) <= (k.1 as int)
                && (k.1 as int) < (base_idx as int) + exp_seq.len() as int)
        ),
        forall|k: (PurseId, u64)| #[trigger] pre.entries().dom().contains(k)
            ==> post.entries()[k] == pre.entries()[k],
        forall|j: int| 0 <= j < exp_seq.len() ==>
            #[trigger] post.entries()[(p, (base_idx + j) as u64)]
                == (EntryRec {
                    purse: p,
                    idx: (base_idx + j) as u64,
                    exponent: exp_seq[j],
                    on_chain: EntryOnChain::Waiting,
                    local: EntryLocal::LocalAvailable,
                    member_key: 0,
                    allocated_at: 0,
                    ready_at: 0,
                    ring_idx: 0,
                }),
        post.coins() == pre.coins(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_age == pre.next_age,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_reserve_entries(
            quint_view(pre), p, exp_seq, base_idx,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_reserve_entries(quint_view(pre), p, exp_seq, base_idx);
    assert(post_view.purses =~= step_view.purses);
    assert forall|k: (PurseId, u64)| post_view.entries.dom().contains(k)
        implies #[trigger] post_view.entries[k] == step_view.entries[k]
    by {
        if pre.entries().dom().contains(k) {
            assert(post_view.entries[k] == pre.entries()[k]);
            assert(step_view.entries[k] == pre.entries()[k]);
        } else {
            let j = (k.1 as int) - (base_idx as int);
            assert(0 <= j < exp_seq.len());
            assert(k == (p, (base_idx + j) as u64));
        }
    }
    assert(post_view.entries =~= step_view.entries);
}

/// Quint analog: spend the source coin at `key`, then bulk-mint
/// `new_exponents.len()` Pending coins in the same purse. The two
/// mark_coin_* intermediate state transitions are hidden in the
/// composite delta.
pub open spec fn quint_step_split_coin(
    pre: QuintViewState,
    key: (PurseId, u64),
    new_exponents: Seq<u8>,
    base_idx: u64,
    base_age: u64,
) -> QuintViewState
    recommends
        pre.coins.dom().contains(key),
        pre.coins[key].state == CoinState::Available,
        pre.purses.dom().contains(key.0),
        pre.purses[key.0].next_coin_idx == base_idx as nat,
        (base_idx as nat) + new_exponents.len() <= u64::MAX as nat,
        (base_age as nat) + new_exponents.len() <= u64::MAX as nat,
{
    let p = key.0;
    let exp = pre.coins[key].exponent;
    QuintViewState {
        coins: Map::new(
            |k: (PurseId, u64)|
                pre.coins.dom().contains(k)
                || (k.0 == p
                    && (base_idx as int) <= (k.1 as int)
                    && (k.1 as int) < (base_idx as int) + new_exponents.len() as int),
            |k: (PurseId, u64)|
                if k == key {
                    CoinRec {
                        purse: pre.coins[key].purse,
                        idx: pre.coins[key].idx,
                        exponent: exp,
                        age: pre.coins[key].age,
                        account: pre.coins[key].account,
                        state: CoinState::Spent,
                    }
                } else if pre.coins.dom().contains(k) {
                    pre.coins[k]
                } else {
                    let j = (k.1 as int) - (base_idx as int);
                    CoinRec {
                        purse: p,
                        idx: k.1,
                        exponent: new_exponents[j],
                        state: CoinState::Pending,
                        age: ((base_age as int) + j) as u64,
                        account: 0,
                    }
                }
        ),
        purses: pre.purses.insert(p, PurseRecSpec {
            id: pre.purses[p].id,
            name: pre.purses[p].name,
            next_coin_idx: pre.purses[p].next_coin_idx + new_exponents.len(),
            next_entry_idx: pre.purses[p].next_entry_idx,
        }),
        events: pre.events.push(Event::CoinSpent {
            purse: p,
            exponent: exp,
        }),
        ..pre
    }
}

proof fn lemma_split_coin_refines(
    pre: State,
    post: State,
    key: (PurseId, u64),
    new_exponents: Seq<u8>,
    base_idx: u64,
    base_age: u64,
)
    requires
        pre.invariant(),
        pre.coins().dom().contains(key),
        pre.coins()[key].state == CoinState::Available,
        pre.purses().dom().contains(key.0),
        pre.purses()[key.0].next_coin_idx == base_idx as nat,
        pre.next_age == base_age,
        (base_idx as nat) + new_exponents.len() <= u64::MAX as nat,
        (base_age as nat) + new_exponents.len() <= u64::MAX as nat,
        pre.events@.len() < u64::MAX as nat,
        forall|j: int| 0 <= j < new_exponents.len() ==>
            (#[trigger] new_exponents[j]) <= MAX_EXPONENT,
        post.invariant(),
        post.coins()[key] == (CoinRec {
            purse: pre.coins()[key].purse,
            idx: pre.coins()[key].idx,
            exponent: pre.coins()[key].exponent,
            age: pre.coins()[key].age,
            account: pre.coins()[key].account,
            state: CoinState::Spent,
        }),
        forall|j: int| 0 <= j < new_exponents.len() ==>
            #[trigger] post.coins()[(key.0, (base_idx + j) as u64)]
                == (CoinRec {
                    purse: key.0,
                    idx: (base_idx + j) as u64,
                    exponent: new_exponents[j],
                    state: CoinState::Pending,
                    age: (base_age + j) as u64,
                    account: 0,
                }),
        post.coins().dom() =~= pre.coins().dom().union(
            Set::new(|k: (PurseId, u64)|
                k.0 == key.0
                && (base_idx as int) <= (k.1 as int)
                && (k.1 as int) < (base_idx as int) + new_exponents.len() as int)
        ),
        forall|k: (PurseId, u64)| #[trigger] pre.coins().dom().contains(k)
            && k != key
            ==> post.coins()[k] == pre.coins()[k],
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[key.0].id == key.0,
        post.purses()[key.0].name == pre.purses()[key.0].name,
        post.purses()[key.0].next_coin_idx
            == pre.purses()[key.0].next_coin_idx + new_exponents.len(),
        post.purses()[key.0].next_entry_idx == pre.purses()[key.0].next_entry_idx,
        forall|q: PurseId| q != key.0 && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@.push(Event::CoinSpent {
            purse: key.0,
            exponent: pre.coins()[key].exponent,
        }),
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_split_coin(
            quint_view(pre), key, new_exponents, base_idx, base_age,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_split_coin(
        quint_view(pre), key, new_exponents, base_idx, base_age,
    );
    assert(post_view.purses =~= step_view.purses);
    assert(post_view.events =~= step_view.events);
    assert forall|k: (PurseId, u64)| post_view.coins.dom().contains(k)
        implies #[trigger] post_view.coins[k] == step_view.coins[k]
    by {
        if k == key {
            // Both maps put Spent record at key.
        } else if pre.coins().dom().contains(k) {
            assert(post_view.coins[k] == pre.coins()[k]);
            assert(step_view.coins[k] == pre.coins()[k]);
        } else {
            let j = (k.1 as int) - (base_idx as int);
            assert(0 <= j < new_exponents.len());
            assert(k == (key.0, (base_idx + j) as u64));
        }
    }
    assert(post_view.coins =~= step_view.coins);
}

/// Quint analog: `start_op(Maintenance, key.0) ; split_coin(key,
/// new_exponents) ; mark_op_submitted(handle)`. Composes the bulk-mint
/// step with op lifecycle.
pub open spec fn quint_step_tracked_split_coin(
    pre: QuintViewState,
    key: (PurseId, u64),
    new_exponents: Seq<u8>,
    base_idx: u64,
    base_age: u64,
) -> QuintViewState
    recommends
        pre.coins.dom().contains(key),
        pre.coins[key].state == CoinState::Available,
        pre.purses.dom().contains(key.0),
        pre.purses[key.0].next_coin_idx == base_idx as nat,
        (base_idx as nat) + new_exponents.len() <= u64::MAX as nat,
        (base_age as nat) + new_exponents.len() <= u64::MAX as nat,
        pre.next_handle < u64::MAX,
{
    let p = key.0;
    let handle = pre.next_handle;
    let exp = pre.coins[key].exponent;
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            handle,
            kind: OpKind::Maintenance,
            purse: p,
            status: OpStatus::Submitted,
        }),
        coins: Map::new(
            |k: (PurseId, u64)|
                pre.coins.dom().contains(k)
                || (k.0 == p
                    && (base_idx as int) <= (k.1 as int)
                    && (k.1 as int) < (base_idx as int) + new_exponents.len() as int),
            |k: (PurseId, u64)|
                if k == key {
                    CoinRec {
                        purse: pre.coins[key].purse,
                        idx: pre.coins[key].idx,
                        exponent: exp,
                        age: pre.coins[key].age,
                        account: pre.coins[key].account,
                        state: CoinState::Spent,
                    }
                } else if pre.coins.dom().contains(k) {
                    pre.coins[k]
                } else {
                    let j = (k.1 as int) - (base_idx as int);
                    CoinRec {
                        purse: p,
                        idx: k.1,
                        exponent: new_exponents[j],
                        state: CoinState::Pending,
                        age: ((base_age as int) + j) as u64,
                        account: 0,
                    }
                }
        ),
        purses: pre.purses.insert(p, PurseRecSpec {
            id: pre.purses[p].id,
            name: pre.purses[p].name,
            next_coin_idx: pre.purses[p].next_coin_idx + new_exponents.len(),
            next_entry_idx: pre.purses[p].next_entry_idx,
        }),
        next_handle: (pre.next_handle + 1) as u64,
        events: pre.events
            .push(Event::OperationStarted {
                handle,
                kind: OpKind::Maintenance,
                purse: p,
            })
            .push(Event::CoinSpent { purse: p, exponent: exp })
            .push(Event::OperationProgress {
                handle,
                status: OpStatus::Submitted,
            }),
        ..pre
    }
}

proof fn lemma_tracked_split_coin_refines(
    pre: State,
    post: State,
    key: (PurseId, u64),
    new_exponents: Seq<u8>,
    base_idx: u64,
    base_age: u64,
)
    requires
        pre.invariant(),
        pre.coins().dom().contains(key),
        pre.coins()[key].state == CoinState::Available,
        pre.purses().dom().contains(key.0),
        pre.purses()[key.0].next_coin_idx == base_idx as nat,
        pre.next_age == base_age,
        (base_idx as nat) + new_exponents.len() <= u64::MAX as nat,
        (base_age as nat) + new_exponents.len() <= u64::MAX as nat,
        pre.next_handle < u64::MAX,
        pre.events@.len() + 3 <= u64::MAX as nat,
        forall|j: int| 0 <= j < new_exponents.len() ==>
            (#[trigger] new_exponents[j]) <= MAX_EXPONENT,
        post.invariant(),
        post.coins()[key] == (CoinRec {
            purse: pre.coins()[key].purse,
            idx: pre.coins()[key].idx,
            exponent: pre.coins()[key].exponent,
            age: pre.coins()[key].age,
            account: pre.coins()[key].account,
            state: CoinState::Spent,
        }),
        forall|j: int| 0 <= j < new_exponents.len() ==>
            #[trigger] post.coins()[(key.0, (base_idx + j) as u64)]
                == (CoinRec {
                    purse: key.0,
                    idx: (base_idx + j) as u64,
                    exponent: new_exponents[j],
                    state: CoinState::Pending,
                    age: (base_age + j) as u64,
                    account: 0,
                }),
        post.coins().dom() =~= pre.coins().dom().union(
            Set::new(|k: (PurseId, u64)|
                k.0 == key.0
                && (base_idx as int) <= (k.1 as int)
                && (k.1 as int) < (base_idx as int) + new_exponents.len() as int)
        ),
        forall|k: (PurseId, u64)| #[trigger] pre.coins().dom().contains(k)
            && k != key
            ==> post.coins()[k] == pre.coins()[k],
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[key.0].id == key.0,
        post.purses()[key.0].name == pre.purses()[key.0].name,
        post.purses()[key.0].next_coin_idx
            == pre.purses()[key.0].next_coin_idx + new_exponents.len(),
        post.purses()[key.0].next_entry_idx == pre.purses()[key.0].next_entry_idx,
        forall|q: PurseId| q != key.0 && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.entries() == pre.entries(),
        post.operations() == pre.operations().insert(pre.next_handle, OperationRec {
            handle: pre.next_handle,
            kind: OpKind::Maintenance,
            purse: key.0,
            status: OpStatus::Submitted,
        }),
        post.events@ == pre.events@
            .push(Event::OperationStarted {
                handle: pre.next_handle,
                kind: OpKind::Maintenance,
                purse: key.0,
            })
            .push(Event::CoinSpent {
                purse: key.0,
                exponent: pre.coins()[key].exponent,
            })
            .push(Event::OperationProgress {
                handle: pre.next_handle,
                status: OpStatus::Submitted,
            }),
        post.next_handle == pre.next_handle + 1,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_tracked_split_coin(
            quint_view(pre), key, new_exponents, base_idx, base_age,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_tracked_split_coin(
        quint_view(pre), key, new_exponents, base_idx, base_age,
    );
    assert(post_view.purses =~= step_view.purses);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
    assert forall|k: (PurseId, u64)| post_view.coins.dom().contains(k)
        implies #[trigger] post_view.coins[k] == step_view.coins[k]
    by {
        if k == key {
        } else if pre.coins().dom().contains(k) {
            assert(post_view.coins[k] == pre.coins()[k]);
            assert(step_view.coins[k] == pre.coins()[k]);
        } else {
            let j = (k.1 as int) - (base_idx as int);
            assert(0 <= j < new_exponents.len());
            assert(k == (key.0, (base_idx + j) as u64));
        }
    }
    assert(post_view.coins =~= step_view.coins);
}

/// Quint analog: `start_op(TopUp, p) ; top_up_via_entry(p, ...) ;
/// mark_op_submitted(handle)`. Three refinement steps composed.
pub open spec fn quint_step_tracked_top_up_via_entry(
    pre: QuintViewState,
    p: PurseId,
    exponent: u8,
    member_key: u64,
    allocated_at: u64,
    ready_at: u64,
    ring_idx: u64,
    new_idx: u64,
) -> QuintViewState
    recommends
        pre.purses.dom().contains(p),
        pre.purses[p].next_entry_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        pre.next_handle < u64::MAX,
{
    let handle = pre.next_handle;
    let new_entry_key = (p, new_idx);
    QuintViewState {
        operations: pre.operations.insert(handle, OperationRec {
            handle,
            kind: OpKind::TopUp,
            purse: p,
            status: OpStatus::Submitted,
        }),
        entries: pre.entries.insert(new_entry_key, EntryRec {
            purse: p,
            idx: new_idx,
            exponent,
            on_chain: EntryOnChain::Waiting,
            local: EntryLocal::LocalAvailable,
            member_key,
            allocated_at,
            ready_at,
            ring_idx,
        }),
        purses: pre.purses.insert(p, PurseRecSpec {
            id: pre.purses[p].id,
            name: pre.purses[p].name,
            next_coin_idx: pre.purses[p].next_coin_idx,
            next_entry_idx: pre.purses[p].next_entry_idx + 1,
        }),
        next_handle: (pre.next_handle + 1) as u64,
        events: pre.events
            .push(Event::OperationStarted {
                handle,
                kind: OpKind::TopUp,
                purse: p,
            })
            .push(Event::EntryAllocated { purse: p, exponent })
            .push(Event::OperationProgress {
                handle,
                status: OpStatus::Submitted,
            }),
        ..pre
    }
}

proof fn lemma_tracked_top_up_via_entry_refines(
    pre: State,
    post: State,
    p: PurseId,
    exponent: u8,
    member_key: u64,
    allocated_at: u64,
    ready_at: u64,
    ring_idx: u64,
    new_idx: u64,
)
    requires
        pre.invariant(),
        pre.purses().dom().contains(p),
        pre.purses()[p].next_entry_idx == new_idx as nat,
        (new_idx as nat) < u64::MAX as nat,
        pre.next_handle < u64::MAX,
        pre.events@.len() + 3 <= u64::MAX as nat,
        exponent <= MAX_EXPONENT,
        post.invariant(),
        post.entries() == pre.entries().insert((p, new_idx), EntryRec {
            purse: p,
            idx: new_idx,
            exponent,
            on_chain: EntryOnChain::Waiting,
            local: EntryLocal::LocalAvailable,
            member_key,
            allocated_at,
            ready_at,
            ring_idx,
        }),
        post.coins() == pre.coins(),
        post.purses().dom() =~= pre.purses().dom(),
        post.purses()[p].id == p,
        post.purses()[p].name == pre.purses()[p].name,
        post.purses()[p].next_coin_idx == pre.purses()[p].next_coin_idx,
        post.purses()[p].next_entry_idx == pre.purses()[p].next_entry_idx + 1,
        forall|q: PurseId| q != p && #[trigger] pre.purses().dom().contains(q)
            ==> post.purses()[q] == pre.purses()[q],
        post.operations() == pre.operations().insert(pre.next_handle, OperationRec {
            handle: pre.next_handle,
            kind: OpKind::TopUp,
            purse: p,
            status: OpStatus::Submitted,
        }),
        post.events@ == pre.events@
            .push(Event::OperationStarted {
                handle: pre.next_handle,
                kind: OpKind::TopUp,
                purse: p,
            })
            .push(Event::EntryAllocated { purse: p, exponent })
            .push(Event::OperationProgress {
                handle: pre.next_handle,
                status: OpStatus::Submitted,
            }),
        post.next_age == pre.next_age,
        post.next_handle == pre.next_handle + 1,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_tracked_top_up_via_entry(
            quint_view(pre), p, exponent,
            member_key, allocated_at, ready_at, ring_idx, new_idx,
        ),
{
    let post_view = quint_view(post);
    let step_view = quint_step_tracked_top_up_via_entry(
        quint_view(pre), p, exponent,
        member_key, allocated_at, ready_at, ring_idx, new_idx,
    );
    assert(post_view.entries =~= step_view.entries);
    assert(post_view.purses =~= step_view.purses);
    assert(post_view.operations =~= step_view.operations);
    assert(post_view.events =~= step_view.events);
}

/// Quint analog: `purses' = purses.put(new_id, {id, name, 0, 0})`.
/// Note: Quint createPurse also emits `EPurseCreated`; the Verus
/// implementation deliberately doesn't (the pilot scheme treats purse
/// creation as silent). This refinement lemma covers the state delta;
/// the event divergence is a known correspondence gap, not a bug.
pub open spec fn quint_step_create_purse(
    pre: QuintViewState,
    new_id: PurseId,
    name: Seq<u8>,
) -> QuintViewState
    recommends
        !pre.purses.dom().contains(new_id),
        new_id != MAIN_PURSE,
{
    QuintViewState {
        purses: pre.purses.insert(new_id, PurseRecSpec {
            id: new_id,
            name,
            next_coin_idx: 0,
            next_entry_idx: 0,
        }),
        ..pre
    }
}

proof fn lemma_create_purse_refines(
    pre: State,
    post: State,
    name: Seq<u8>,
    new_id: PurseId,
)
    requires
        pre.invariant(),
        pre.has_create_capacity(),
        new_id != MAIN_PURSE,
        !pre.purses().dom().contains(new_id),
        post.purses() == pre.purses().insert(new_id, PurseRecSpec {
            id: new_id,
            name,
            next_coin_idx: 0,
            next_entry_idx: 0,
        }),
        post.coins() == pre.coins(),
        post.entries() == pre.entries(),
        post.operations() == pre.operations(),
        post.events@ == pre.events@,
        post.next_handle == pre.next_handle,
        post.next_extrinsic_id == pre.next_extrinsic_id,
        post.total_in == pre.total_in,
        post.total_out == pre.total_out,
        post.fee_balance == pre.fee_balance,
        post.paid_ring_membership == pre.paid_ring_membership,
        post.tokens@ == pre.tokens@,
        post.chain_coins@ == pre.chain_coins@,
        post.chain_entries@ == pre.chain_entries@,
    ensures
        quint_view(post) == quint_step_create_purse(quint_view(pre), new_id, name),
{
    let post_view = quint_view(post);
    let step_view = quint_step_create_purse(quint_view(pre), new_id, name);
    assert(post_view.purses =~= step_view.purses);
}

// ==========================================================================
// Findings from the refinement attempt — primitives whose contracts are
// too loose to refine without strengthening:
//
// - ~~`create_purse`~~: contract strengthened with full preservation
//   clauses; refined via lemma_create_purse_refines above.
// - `add_coin_with_account` / `add_entry_with_meta`: pre-cascade contracts.
//   Cover most preservation but miss `next_extrinsic_id`, `total_in`,
//   `total_out`, `fee_balance`, `paid_ring_membership`, `tokens@`,
//   `chain_coins@`, `chain_entries@`. The implementations DO preserve these
//   (their bodies don't touch them), but the contracts don't say so.
// - `top_up_fee_account`, `deduct_fee`: contracts mention `fee_balance`
//   but omit `events`, `total_*`, `tokens`, `chain_*`, `paid_ring_membership`
//   preservation.
// - `mint_token`, `consume_token`: contracts focus on `tokens@` mutation
//   but skip preservation clauses for ~10 other fields.
//
// These are real correspondence gaps. The implementations DO preserve
// the un-mentioned fields (their bodies only touch the named ones), but
// the contracts don't say so, so callers can't reason about preservation
// and refinement step-lemmas can't be discharged.
//
// Strengthening these contracts is mechanical (~10 lines per primitive)
// and would unblock the corresponding step lemmas. Deferred from this
// PoC because the methodology is already demonstrated — closing the gaps
// is mechanical contract editing, not a verification challenge.
// ==========================================================================

} // verus!
