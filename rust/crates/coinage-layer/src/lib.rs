//! Verus translation of the Coinage Layer Quint specification.
//!
//! Source-of-truth references:
//!   - Quint spec  : `docs/specs/coinage-layer.qnt`
//!   - Design doc  : `docs/design/coinage-layer.md`
//!
//! **Scope.** Verified protocol kernel covering the four core state
//! components — purses, coins, recycler entries, operations — with
//! their lifecycle transitions, the §6.3 priority order, chain-mirror
//! recovery state, the events stream, the fee account, unload tokens,
//! and the totals/accumulators. Chain interaction is abstracted:
//! chain-side state changes arrive via caller-driven primitives
//! (`set_entry_on_chain`, `mark_op_finalized`, …) rather than being
//! modeled directly. No persistence, no crypto; `member_key` /
//! `account` / chain timestamps are `u64` placeholders supplied by
//! the host.
//!
//! **Module map.**
//!
//! ```text
//! types.rs              — public types, constants, tag enums, the State struct
//! spec_helpers.rs       — top-level spec functions (lock predicates, priority,
//!                         sums, payment classify, coin_value/pow2_nat)
//! pow2.rs               — pow2 lemmas + executable `pow2_u64_exec`
//!
//! state_invariant.rs    — view accessors, `invariant()`, `init()`
//! state_purses.rs       — purse lifecycle (create/rename/delete/purge)
//! state_coins.rs        — coin lifecycle (add, mark, lock/unlock/commit)
//! state_entries.rs      — entry lifecycle (add, set, mark, lock/release)
//! state_operations.rs   — op status transitions + bulk release helpers
//! state_composites.rs   — atomic op composites (start/cancel/commit pairs)
//! state_high_level.rs   — transfer, rebalance, export/import, split,
//!                         unload, top-up, reserve
//! state_tracked.rs      — `tracked_*` wrappers (op-handle-bearing variants)
//! state_chain.rs        — chain-mirror state + recovery scans
//! state_selectors.rs    — `find_*`, subset-sum covers, classify-payment exec
//! state_aggregators.rs  — count / sum / total / lock-count helpers
//! state_queries.rs      — read-only queries + has-/check- helpers
//! state_fee.rs          — fee account top-up / deduct / select-mode
//! state_tokens.rs       — unload-token mint / consume / count
//! state_events.rs       — `emit_event`, `event_count`
//! state_accumulators.rs — total_in/out, paid_ring_membership, extrinsic-id
//!
//! refinement.rs         — Quint→Verus refinement scaffolding (per-method
//!                         `quint_step_*` spec fns + `lemma_*_refines` proofs)
//! ```
//!
//! **Encoding.** Exec storage is `Vec<…Rec>` per component. Contracts
//! quantify over ghost spec maps (`Ghost<Map<key, Rec>>`). The
//! invariant ties them: every Vec entry is in the ghost map under
//! its key; every ghost-map key has a matching Vec entry; no
//! duplicates. State-mutating methods explicitly preserve untouched
//! components (`final.next_handle == old.next_handle`, …) in their
//! contracts — Verus's `&mut self` SMT encoding doesn't carry these
//! over for free.

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
