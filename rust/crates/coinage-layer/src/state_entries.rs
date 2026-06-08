//! Entry lifecycle: `add_entry*`, `set_entry_*`, lock/release/consume, helpers.

use vstd::prelude::*;

use crate::*;

verus! {

impl State {
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
            final(self).entries() == old(self).entries().insert(key, EntryRec {
                on_chain: EntryOnChain::Missing,
                ..old(self).entries()[key]
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
                local: EntryLocal::LocalAvailable,
                ..old(self).entries()[key]
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


    /// Internal: read the `exponent` of a recycler entry known to exist by `key`.
    pub(crate) fn read_entry_exponent(&self, key: (PurseId, u64)) -> (exp: u8)
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

}

} // verus!
