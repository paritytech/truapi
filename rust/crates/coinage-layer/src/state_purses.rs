//! Purse lifecycle: create, rename, delete (safe/forced), bulk purges.

use vstd::prelude::*;

use crate::*;

verus! {

impl State {
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
            new_id == old(self).next_purse_id,
            !old(self).purses().dom().contains(new_id),
            final(self).purses() == old(self).purses().insert(new_id, PurseRecSpec {
                id: new_id,
                name: name@,
                next_coin_idx: 0,
                next_entry_idx: 0,
            }),
            final(self).next_purse_id == old(self).next_purse_id + 1,
            // All other state preserved.
            final(self).coins() == old(self).coins(),
            final(self).entries() == old(self).entries(),
            final(self).operations() == old(self).operations(),
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
            final(self).coins() == old(self).coins(),
            final(self).entries() == old(self).entries(),
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
                self.coins() == old(self).coins(),
                self.entries() == old(self).entries(),
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
                    && p != MAIN_PURSE
                    && final(self).purses() == old(self).purses().remove(p)
                    && final(self).coins() == old(self).coins().remove_keys(
                        Set::new(|k: (PurseId, u64)| k.0 == p)
                    )
                    && final(self).entries() == old(self).entries().remove_keys(
                        Set::new(|k: (PurseId, u64)| k.0 == p)
                    ),
                Err(_) => true,
            },
            final(self).operations() == old(self).operations(),
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
            final(self).operations() == old(self).operations(),
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


    /// Internal: scan the coin Vec for the first entry with `purse == p`.
    /// Returns its index, or `None` if no such coin remains.
    pub(crate) fn find_coin_with_purse(&self, p: PurseId) -> (res: Option<usize>)
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
    pub(crate) fn remove_coin_at(&mut self, idx: usize)
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
    pub(crate) fn find_entry_with_purse(&self, p: PurseId) -> (res: Option<usize>)
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
    pub(crate) fn remove_entry_at(&mut self, idx: usize)
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

}

} // verus!
