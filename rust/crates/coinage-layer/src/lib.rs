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

/// Coin record (Quint `CoinRec`, design §3.2).
/// Pilot scope: only the fields needed to express referential integrity
/// against purses are modeled. `account`, `age`, `state` are deferred.
pub struct CoinRec {
    pub purse: PurseId,
    pub idx: u64,
    pub exponent: u8,
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
    pub next_purse_id: u64,
    #[allow(dead_code)]
    pub spec_purses: Ghost<Map<PurseId, PurseRecSpec>>,
    /// Ghost coin map keyed by `(purse, idx)`. No exec mirror yet; coins
    /// are introduced as pure ghost state for now, so the integrity
    /// invariant can be exercised before a real `add_coin` primitive lands.
    #[allow(dead_code)]
    pub spec_coins: Ghost<Map<(PurseId, u64), CoinRec>>,
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

    /// True iff some coin currently lives in purse `p`.
    pub open spec fn has_coin_in(&self, p: PurseId) -> bool {
        exists|k: (PurseId, u64)| #[trigger] self.coins().dom().contains(k) && k.0 == p
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
        let s = State {
            purses,
            next_purse_id: 1,
            spec_purses: Ghost(Map::<PurseId, PurseRecSpec>::empty().insert(MAIN_PURSE, main_spec)),
            spec_coins: Ghost(Map::<(PurseId, u64), CoinRec>::empty()),
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
            forall|k: (PurseId, u64)| #[trigger] old(self).coins().dom().contains(k) ==> k.0 != p,
        ensures
            final(self).invariant(),
            match res {
                Ok(()) =>
                    old(self).purses().dom().contains(p)
                    && p != MAIN_PURSE
                    && final(self).purses() == old(self).purses().remove(p),
                Err(Error::CannotDeleteMainPurse) =>
                    p == MAIN_PURSE
                    && final(self).purses() == old(self).purses(),
                Err(Error::PurseNotFound(q)) =>
                    p != MAIN_PURSE
                    && !old(self).purses().dom().contains(p)
                    && q == p
                    && final(self).purses() == old(self).purses(),
            },
    {
        if p == MAIN_PURSE {
            return Err(Error::CannotDeleteMainPurse);
        }

        let ghost old_v = self.purses@;
        let ghost old_m = self.spec_purses@;
        let ghost old_coins = self.spec_coins@;

        let mut i: usize = 0;
        while i < self.purses.len()
            invariant
                0 <= i <= self.purses.len(),
                self.invariant(),
                self.purses@ == old_v,
                self.spec_purses@ == old_m,
                self.spec_coins@ == old_coins,
                old_m == old(self).spec_purses@,
                old_v == old(self).purses@,
                old_coins == old(self).spec_coins@,
                self.next_purse_id == old(self).next_purse_id,
                p != MAIN_PURSE,
                forall|k: (PurseId, u64)| #[trigger] old(self).coins().dom().contains(k) ==> k.0 != p,
                forall|j: int| 0 <= j < i ==> (#[trigger] self.purses@[j]).id != p,
            decreases self.purses.len() - i,
        {
            if self.purses[i].id == p {
                let ghost target_idx = i as int;
                let _removed = self.purses.swap_remove(i);
                proof {
                    self.spec_purses = Ghost(self.spec_purses@.remove(p));

                    let new_v = self.purses@;
                    let new_m = self.spec_purses@;
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

                    // (j) coin referential integrity preserved: every coin's
                    // purse is != p (by the no-coin-in-p precondition) and was
                    // in old_m.dom, so it remains in new_m.dom == old_m \ {p}.
                    assert(self.spec_coins@ == old_coins);
                    assert(old_coins == old(self).spec_coins@);
                    assert(old(self).coins() == old_coins);
                    assert forall|k: (PurseId, u64)| #[trigger] self.spec_coins@.dom().contains(k)
                        implies new_m.dom().contains(k.0)
                    by {
                        assert(old(self).coins().dom().contains(k));
                        assert(k.0 != p);
                        assert(old_m.dom().contains(k.0));
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

    /// 6.1 `queryPurse` (Quint lines 603-612; design §8.1 `query_purse`).
    ///
    /// Returns a synchronous snapshot. In the pilot scope (no coins/entries),
    /// the three amount fields are always 0.
    pub fn query_purse(&self, p: PurseId) -> (info: Result<PurseInfo, Error>)
        requires
            self.invariant(),
        ensures
            match info {
                Ok(i) =>
                    self.purses().dom().contains(p)
                    && i.id == p
                    && i.name@ == self.purses()[p].name
                    && i.spendable == 0
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
                let rec = &self.purses[i];
                let name_copy: Vec<u8> = rec.name.clone();
                assert(name_copy@ == rec.name@);
                return Ok(PurseInfo {
                    id: rec.id,
                    name: name_copy,
                    spendable: 0,
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
