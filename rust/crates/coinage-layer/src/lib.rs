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


} // verus!

pub mod types;
pub mod spec_helpers;
pub mod pow2;
pub mod state_invariant;
pub mod state_purses;
pub mod state_coins;
pub mod state_entries;
pub mod state_operations;
pub mod state_composites;
pub mod state_high_level;
pub mod state_tracked;
pub mod state_chain;
pub mod state_selectors;
pub mod state_aggregators;
pub mod state_queries;
pub mod state_fee;
pub mod state_tokens;
pub mod state_events;
pub mod state_accumulators;
pub mod refinement;

pub use types::*;
pub use spec_helpers::*;
pub use pow2::*;
