//! Chain-mirror state: registration, recovery scans, restore primitives.

use vstd::prelude::*;

use crate::*;

verus! {

impl State {
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
            match res {
                Some(j) => {
                    &&& 0 <= j < old(self).chain_coins@.len()
                    &&& !old(self).coins().dom().contains(
                            (old(self).chain_coins@[j as int].purse,
                             old(self).chain_coins@[j as int].idx))
                    &&& final(self).coins() == old(self).coins().insert(
                            (old(self).chain_coins@[j as int].purse,
                             old(self).chain_coins@[j as int].idx),
                            old(self).chain_coins@[j as int])
                },
                None =>
                    final(self).coins() == old(self).coins(),
            },
            final(self).purses() == old(self).purses(),
            final(self).entries() == old(self).entries(),
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
            match res {
                Some(j) => {
                    &&& 0 <= j < old(self).chain_entries@.len()
                    &&& !old(self).entries().dom().contains(
                            (old(self).chain_entries@[j as int].purse,
                             old(self).chain_entries@[j as int].idx))
                    &&& final(self).entries() == old(self).entries().insert(
                            (old(self).chain_entries@[j as int].purse,
                             old(self).chain_entries@[j as int].idx),
                            old(self).chain_entries@[j as int])
                },
                None =>
                    final(self).entries() == old(self).entries(),
            },
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
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

}

} // verus!
