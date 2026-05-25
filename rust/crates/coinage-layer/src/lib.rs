//! Verus translation of the Coinage Layer Quint specification.
//!
//! Source-of-truth references:
//!   - Quint spec  : `docs/specs/coinage-layer.qnt`
//!   - Design doc  : `docs/design/coinage-layer.md`
//!
//! **Pilot scope.** Purse-lifecycle primitives only: `init`, `create_purse`,
//! `query_purse`. The full Quint state has many vars (`coins`, `entries`,
//! `operations`, `events`, `tokens`, ...); this crate models only the
//! `purses` map and a fresh-id allocator.
//!
//! **Encoding.** Exec storage is a `Vec<PurseRec>`. Contracts quantify over a
//! ghost spec map (`Ghost<Map<PurseId, PurseRecSpec>>`). The invariant ties
//! the two: every Vec entry is present in the ghost map under its own id,
//! every ghost-map key has a matching Vec entry, and there are no duplicate
//! ids in the Vec.

use vstd::prelude::*;

verus! {

/// Stable purse identifier (Quint `PurseId`, design §3.1).
pub type PurseId = u64;

/// Reserved identifier of the main purse (Quint `MAIN_PURSE`).
pub const MAIN_PURSE: PurseId = 0;

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
///   * `Available` — coin can be selected for an outbound operation.
///   * `PendingSpend` — coin has been chosen by an in-flight operation.
///   * `Spent` — coin is terminally consumed; counts neither for selection
///     nor as "live" for purse-deletion purposes.
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum CoinState {
    Available,
    PendingSpend,
    Spent,
}

/// Coin record (Quint `CoinRec`, design §3.2).
/// Pilot scope: `account` and `age` are deferred.
#[derive(Copy, Clone)]
pub struct CoinRec {
    pub purse: PurseId,
    pub idx: u64,
    pub exponent: u8,
    pub state: CoinState,
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

/// Recycler entry record (Quint `EntryRec`, design §3.3).
///
/// Pilot scope: `memberKey`, `allocatedAt`, `readyAt`, `ringIdx`,
/// and the local lifecycle (`EntryLocal`) are deferred. Only the
/// on-chain state is tracked.
#[derive(Copy, Clone)]
pub struct EntryRec {
    pub purse: PurseId,
    pub idx: u64,
    pub exponent: u8,
    pub on_chain: EntryOnChain,
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

/// Layer error enum. Pilot subset of design §10.
pub enum Error {
    PurseNotFound(PurseId),
    CannotDeleteMainPurse,
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
    pub next_purse_id: u64,
    #[allow(dead_code)]
    pub spec_purses: Ghost<Map<PurseId, PurseRecSpec>>,
    #[allow(dead_code)]
    pub spec_coins: Ghost<Map<(PurseId, u64), CoinRec>>,
    #[allow(dead_code)]
    pub spec_entries: Ghost<Map<(PurseId, u64), EntryRec>>,
}

/// Spec-only recursive count: number of indices in `v[0..j]` whose
/// coin is `Available` and belongs to purse `p`.
pub open spec fn count_avail_prefix(v: Seq<CoinRec>, p: PurseId, j: nat) -> nat
    decreases j
{
    if j == 0 {
        0
    } else {
        let prev = count_avail_prefix(v, p, (j - 1) as nat);
        if v[(j - 1) as int].purse == p
            && v[(j - 1) as int].state == CoinState::Available
        {
            prev + 1
        } else {
            prev
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
        let s = State {
            purses,
            coins,
            entries,
            next_purse_id: 1,
            spec_purses: Ghost(Map::<PurseId, PurseRecSpec>::empty().insert(MAIN_PURSE, main_spec)),
            spec_coins: Ghost(Map::<(PurseId, u64), CoinRec>::empty()),
            spec_entries: Ghost(Map::<(PurseId, u64), EntryRec>::empty()),
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
                Err(Error::CannotDeleteMainPurse) => false,
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
    pub fn delete_purse(&mut self, p: PurseId) -> (res: Result<(), Error>)
        requires
            old(self).invariant(),
            !old(self).has_live_coin_in(p),
            // Stage 6a tightening: also no recycler entries in p. Relaxed
            // once `purge_entries_of_purse` lands in stage 6c.
            forall|k: (PurseId, u64)| #[trigger] old(self).entries().dom().contains(k)
                ==> k.0 != p,
        ensures
            final(self).invariant(),
            match res {
                Ok(()) =>
                    old(self).purses().dom().contains(p)
                    && p != MAIN_PURSE
                    && final(self).purses() == old(self).purses().remove(p)
                    && final(self).coins() == old(self).coins().remove_keys(
                        Set::new(|k: (PurseId, u64)| k.0 == p)
                    ),
                Err(Error::CannotDeleteMainPurse) =>
                    p == MAIN_PURSE
                    && final(self).purses() == old(self).purses()
                    && final(self).coins() == old(self).coins(),
                Err(Error::PurseNotFound(q)) =>
                    p != MAIN_PURSE
                    && !old(self).purses().dom().contains(p)
                    && q == p
                    && final(self).purses() == old(self).purses()
                    && final(self).coins() == old(self).coins().remove_keys(
                        Set::new(|k: (PurseId, u64)| k.0 == p)
                    ),
            },
    {
        if p == MAIN_PURSE {
            return Err(Error::CannotDeleteMainPurse);
        }

        // Purge any coins belonging to p (any state). The contract's
        // `!has_live_coin_in(p)` precondition allows Spent coins to remain;
        // they're removed here. If p isn't a known purse, invariant (j) ⇒
        // no coin has purse == p anyway, so this is a no-op for the coin map.
        self.purge_coins_of_purse(p);

        let ghost old_v = self.purses@;
        let ghost old_m = self.spec_purses@;
        let ghost old_coins = self.spec_coins@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_entries = self.spec_entries@;
        let ghost old_entries_vec = self.entries@;

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
                old_m == old(self).spec_purses@,
                old_v == old(self).purses@,
                old_coins == old(self).coins().remove_keys(
                    Set::new(|k: (PurseId, u64)| k.0 == p)
                ),
                old_entries == old(self).entries(),
                old_entries_vec == old(self).entries@,
                self.next_purse_id == old(self).next_purse_id,
                p != MAIN_PURSE,
                forall|k: (PurseId, u64)| #[trigger] old_coins.dom().contains(k) ==> k.0 != p,
                forall|k: (PurseId, u64)| #[trigger] old_entries.dom().contains(k) ==> k.0 != p,
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

                    // Entries are entirely untouched in this branch; entry-side
                    // invariant clauses (p, q, r, s, t) follow because no entry
                    // has purse == p (precondition) and self.entries / self.spec_entries
                    // are unchanged.
                    assert(self.entries@ == old(self).entries@);
                    assert(self.spec_entries@ == old(self).spec_entries@);
                    assert forall|k: (PurseId, u64)|
                        #[trigger] self.spec_entries@.dom().contains(k)
                    implies
                        new_m.dom().contains(k.0)
                    by {
                        assert(old(self).entries().dom().contains(k));
                        assert(k.0 != p);
                        assert(old_m.dom().contains(k.0));
                    }
                    assert forall|k: (PurseId, u64)|
                        #[trigger] self.spec_entries@.dom().contains(k)
                    implies
                        k.1 < new_m[k.0].next_entry_idx
                    by {
                        assert(old(self).entries().dom().contains(k));
                        assert(k.0 != p);
                        assert(new_m[k.0] == old_m[k.0]);
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

    /// Internal: allocate a fresh coin in purse `p` with the given `exponent`.
    ///
    /// This is the elemental coin-creating primitive. Higher-level operations
    /// (top-up, transfer, rebalance) decompose into one or more `add_coin` plus
    /// updates to coin state (`account`, `age`, `state` fields not yet modeled
    /// in this pilot). The coin's `idx` is the purse's current
    /// `next_coin_idx`, after which the allocator is bumped.
    #[allow(unused_variables)]
    pub fn add_coin(&mut self, p: PurseId, exponent: u8) -> (key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(p),
            old(self).purses()[p].next_coin_idx < u64::MAX,
        ensures
            final(self).invariant(),
            key.0 == p,
            key.1 == old(self).purses()[p].next_coin_idx,
            !old(self).coins().dom().contains(key),
            final(self).coins() == old(self).coins().insert(key, CoinRec {
                purse: p,
                idx: key.1,
                exponent,
                state: CoinState::Available,
            }),
            final(self).purses().dom() =~= old(self).purses().dom(),
            final(self).purses()[p].id == p,
            final(self).purses()[p].name == old(self).purses()[p].name,
            final(self).purses()[p].next_coin_idx
                == old(self).purses()[p].next_coin_idx + 1,
            final(self).purses()[p].next_entry_idx
                == old(self).purses()[p].next_entry_idx,
            forall|q: PurseId| q != p && #[trigger] old(self).purses().dom().contains(q)
                ==> final(self).purses()[q] == old(self).purses()[q],
    {
        let ghost old_v = self.purses@;
        let ghost old_m = self.spec_purses@;
        let ghost old_coins = self.spec_coins@;
        let ghost old_coins_vec = self.coins@;
        let ghost p_old_rec = old_m[p];

        let mut i: usize = 0;
        while i < self.purses.len()
            invariant
                0 <= i <= self.purses.len(),
                self.invariant(),
                self.purses@ == old_v,
                self.spec_purses@ == old_m,
                self.spec_coins@ == old_coins,
                self.coins@ == old_coins_vec,
                old_m == old(self).spec_purses@,
                old_v == old(self).purses@,
                old_coins == old(self).spec_coins@,
                old_coins_vec == old(self).coins@,
                self.next_purse_id == old(self).next_purse_id,
                old(self).purses().dom().contains(p),
                p_old_rec == old_m[p],
                p_old_rec.next_coin_idx < u64::MAX,
                forall|j: int| 0 <= j < i ==> (#[trigger] self.purses@[j]).id != p,
            decreases self.purses.len() - i,
        {
            if self.purses[i].id == p {
                let ghost target_idx = i as int;
                let cur_idx = self.purses[i].next_coin_idx;
                let ghost old_p_rec_at_idx = old_v[target_idx]@;
                self.purses[i].next_coin_idx = cur_idx + 1;

                let key = (p, cur_idx);
                let new_coin = CoinRec {
                    purse: p,
                    idx: cur_idx,
                    exponent,
                    state: CoinState::Available,
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
                state: CoinState::PendingSpend,
            }),
    {
        self.transition_coin_state(key, CoinState::PendingSpend);
    }

    /// Coin lifecycle: `PendingSpend` → `Spent`.
    pub fn mark_coin_spent(&mut self, key: (PurseId, u64))
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
                state: CoinState::Spent,
            }),
    {
        self.transition_coin_state(key, CoinState::Spent);
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
                state: new_state,
            }),
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_next_purse_id = self.next_purse_id;
        let ghost old_coins = self.spec_coins@;
        let ghost old_coins_vec = self.coins@;

        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                self.purses@ == old_purses_vec,
                self.spec_purses@ == old_spec_purses,
                self.next_purse_id == old_next_purse_id,
                self.spec_coins@ == old_coins,
                self.coins@ == old_coins_vec,
                old_spec_purses == old(self).spec_purses@,
                old_spec_purses == old(self).purses(),
                old_coins == old(self).spec_coins@,
                old_coins == old(self).coins(),
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
        ensures
            final(self).invariant(),
            match res {
                Some(new_key) =>
                    new_key.0 == to
                    && final(self).coins().dom().contains(new_key)
                    && final(self).coins()[new_key].state == CoinState::Available
                    && final(self).coins()[new_key].exponent >= min_exp,
                None =>
                    // No Available coin in `from` met the threshold.
                    forall|k: (PurseId, u64)|
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
                Some(new_key)
            }
        }
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
        ensures
            final(self).invariant(),
            new_key.0 == dst,
            new_key.1 == old(self).purses()[dst].next_coin_idx,
            final(self).coins().dom().contains(new_key),
            final(self).coins()[new_key].state == CoinState::Available,
            final(self).coins()[new_key].exponent == old(self).coins()[key].exponent,
            final(self).coins().dom().contains(key),
            final(self).coins()[key].state == CoinState::Spent,
    {
        let exp = self.read_coin_exponent(key);
        self.mark_coin_pending_spend(key);
        self.mark_coin_spent(key);
        self.add_coin(dst, exp)
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
    {
        let ghost old_p_next = old(self).purses()[p].next_coin_idx;
        let ghost old_purses_map = old(self).purses();
        let ghost old_coins_map = old(self).coins();
        let n = exp_seq.len();

        let mut k: usize = 0;
        while k < n
            invariant
                0 <= k <= n,
                n == exp_seq@.len(),
                self.invariant(),
                self.purses().dom() =~= old_purses_map.dom(),
                old_purses_map.dom().contains(p),
                self.purses()[p].next_coin_idx == old_p_next + k as nat,
                self.purses()[p].id == p,
                self.purses()[p].name == old_purses_map[p].name,
                self.purses()[p].next_entry_idx == old_purses_map[p].next_entry_idx,
                old_p_next == old_purses_map[p].next_coin_idx,
                old_p_next as nat + n as nat <= u64::MAX as nat,
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

    /// Count of `Available` coins in purse `p`. Scans the coin Vec; the
    /// returned count equals `count_avail_prefix(self.coins@, p, len)`.
    ///
    /// **Pilot value scheme:** spendable is the *count* of Available coins,
    /// not the sum of coin values. Real `coinValue(exp) = 2^exp` is deferred.
    fn count_available_in(&self, p: PurseId) -> (count: u64)
        requires
            self.invariant(),
        ensures
            count as nat == count_avail_prefix(self.coins@, p, self.coins@.len() as nat),
    {
        let mut count: u64 = 0;
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                count as nat == count_avail_prefix(self.coins@, p, j as nat),
                count as nat <= j as nat,
            decreases self.coins.len() - j,
        {
            let is_available = matches!(self.coins[j].state, CoinState::Available);
            proof {
                // count_avail_prefix(v, p, j+1) - count_avail_prefix(v, p, j) is
                // either 0 or 1, so count <= (j+1) is preserved.
                assert(count_avail_prefix(self.coins@, p, (j + 1) as nat)
                    <= count_avail_prefix(self.coins@, p, j as nat) + 1);
            }
            if self.coins[j].purse == p && is_available {
                count = count + 1;
            }
            j = j + 1;
        }
        count
    }

    /// 6.1 `queryPurse` (Quint lines 603-612; design §8.1 `query_purse`).
    ///
    /// Returns a synchronous snapshot. `spendable` is the count of
    /// `Available` coins in `p` (see `count_available_in`). `spendable_strict`
    /// and `pending` remain pilot-stubbed at 0 — they correspond to recycler-
    /// entry aggregations that don't exist in this pilot's state.
    pub fn query_purse(&self, p: PurseId) -> (info: Result<PurseInfo, Error>)
        requires
            self.invariant(),
        ensures
            match info {
                Ok(i) =>
                    self.purses().dom().contains(p)
                    && i.id == p
                    && i.name@ == self.purses()[p].name
                    && i.spendable as nat
                        == count_avail_prefix(self.coins@, p, self.coins@.len() as nat)
                    && i.spendable_strict == 0
                    && i.pending == 0,
                Err(Error::PurseNotFound(q)) =>
                    !self.purses().dom().contains(p) && q == p,
                Err(Error::CannotDeleteMainPurse) => false,
            },
    {
        let mut i: usize = 0;
        while i < self.purses.len()
            invariant
                0 <= i <= self.purses.len(),
                self.invariant(),
                forall|j: int| 0 <= j < i ==> (#[trigger] self.purses@[j]).id != p,
            decreases
                self.purses.len() - i,
        {
            if self.purses[i].id == p {
                let spendable = self.count_available_in(p);
                let rec = &self.purses[i];
                let name_copy: Vec<u8> = rec.name.clone();
                assert(name_copy@ == rec.name@);
                return Ok(PurseInfo {
                    id: rec.id,
                    name: name_copy,
                    spendable,
                    spendable_strict: 0,
                    pending: 0,
                });
            }
            i += 1;
        }
        Err(Error::PurseNotFound(p))
    }
}

} // verus!
