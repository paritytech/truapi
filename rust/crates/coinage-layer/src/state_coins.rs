//! Coin lifecycle: `add_coin*`, mark/lock/unlock/commit, helpers.

use vstd::prelude::*;

use crate::*;

verus! {

impl State {
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
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
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
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        self.add_coin_with_account(p, exponent, 0)
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
    pub(crate) fn transition_coin_state(&mut self, key: (PurseId, u64), new_state: CoinState)
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


    /// Internal: read the `exponent` of a coin known to exist by `key`.
    pub(crate) fn read_coin_exponent(&self, key: (PurseId, u64)) -> (exp: u8)
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

}

} // verus!
