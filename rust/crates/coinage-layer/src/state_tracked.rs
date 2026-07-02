//! `tracked_*` wrappers: same effect as the unwrapped op, plus an `OpHandle`.

use vstd::prelude::*;

use crate::*;

verus! {

impl State {
    /// Tracked transfer: same effect as `transfer`, but wrapped in an
    /// operation handle so the upper layer can correlate the transfer
    /// with chain confirmation, cancellation, and status streams.
    ///
    /// Lifecycle: an operation record is created in `Preparing`, walked
    /// through `Submitted`, and ends in `Done` (on Some) or `Failed`
    /// (on None — no Available coin met the threshold).
    pub fn tracked_transfer(&mut self, from: PurseId, to: PurseId, min_exp: u8)
        -> (res: (OpHandle, Option<(PurseId, u64)>))
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(from),
            old(self).purses().dom().contains(to),
            old(self).purses()[to].next_coin_idx < u64::MAX,
            old(self).next_handle < u64::MAX,
            old(self).next_age < u64::MAX,
            old(self).events@.len() + 3 <= u64::MAX as nat,
        ensures
            final(self).invariant(),
            res.0 == old(self).next_handle,
            !old(self).operations().dom().contains(res.0),
            final(self).next_handle == old(self).next_handle + 1,
            final(self).entries() == old(self).entries(),
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            match res.1 {
                Some(new_key) =>
                    new_key.0 == to
                    && new_key.1 == old(self).purses()[to].next_coin_idx
                    && final(self).next_age == old(self).next_age + 1
                    && final(self).operations() == old(self).operations().insert(res.0, OperationRec {
                        handle: res.0,
                        kind: OpKind::Transfer,
                        purse: from,
                        status: OpStatus::Done,
                    })
                    && final(self).purses().dom() =~= old(self).purses().dom()
                    && final(self).purses()[to].id == to
                    && final(self).purses()[to].name == old(self).purses()[to].name
                    && final(self).purses()[to].next_coin_idx
                        == old(self).purses()[to].next_coin_idx + 1
                    && final(self).purses()[to].next_entry_idx
                        == old(self).purses()[to].next_entry_idx
                    && (forall|q: PurseId| q != to
                        && #[trigger] old(self).purses().dom().contains(q)
                        ==> final(self).purses()[q] == old(self).purses()[q])
                    && (exists|src_key: (PurseId, u64)|
                        #[trigger] old(self).coins().dom().contains(src_key)
                        && src_key.0 == from
                        && old(self).coins()[src_key].state == CoinState::Available
                        && old(self).coins()[src_key].exponent >= min_exp
                        && final(self).coins() == old(self).coins()
                            .insert(src_key, CoinRec {
                                purse: old(self).coins()[src_key].purse,
                                idx: old(self).coins()[src_key].idx,
                                exponent: old(self).coins()[src_key].exponent,
                                age: old(self).coins()[src_key].age,
                                account: old(self).coins()[src_key].account,
                                state: CoinState::Spent,
                            })
                            .insert(new_key, CoinRec {
                                purse: to,
                                idx: new_key.1,
                                exponent: old(self).coins()[src_key].exponent,
                                state: CoinState::Available,
                                age: old(self).next_age,
                                account: 0,
                            })
                        && final(self).events@ == old(self).events@
                            .push(Event::OperationStarted {
                                handle: res.0,
                                kind: OpKind::Transfer,
                                purse: from,
                            })
                            .push(Event::CoinSpent {
                                purse: from,
                                exponent: old(self).coins()[src_key].exponent,
                            })
                            .push(Event::CoinAvailable {
                                purse: to,
                                exponent: old(self).coins()[src_key].exponent,
                            })),
                None =>
                    final(self).next_age == old(self).next_age
                    && final(self).operations() == old(self).operations().insert(res.0, OperationRec {
                        handle: res.0,
                        kind: OpKind::Transfer,
                        purse: from,
                        status: OpStatus::Failed,
                    })
                    && final(self).purses() == old(self).purses()
                    && final(self).coins() == old(self).coins()
                    && final(self).events@ == old(self).events@
                        .push(Event::OperationStarted {
                            handle: res.0,
                            kind: OpKind::Transfer,
                            purse: from,
                        })
                    && (forall|k: (PurseId, u64)|
                        #[trigger] old(self).coins().dom().contains(k)
                        && k.0 == from
                        && old(self).coins()[k].state == CoinState::Available
                        ==> old(self).coins()[k].exponent < min_exp),
            },
    {
        let handle = self.start_op(OpKind::Transfer, from);
        proof {
            assert(self.operations()[handle].kind == OpKind::Transfer);
            assert(self.operations()[handle].purse == from);
        }
        self.set_op_status(handle, OpStatus::Submitted);
        proof {
            assert(self.operations()[handle].kind == OpKind::Transfer);
            assert(self.operations()[handle].purse == from);
        }
        let result = self.transfer(from, to, min_exp);
        proof {
            assert(self.operations()[handle].kind == OpKind::Transfer);
            assert(self.operations()[handle].purse == from);
        }
        match result {
            Some(_) => self.set_op_status(handle, OpStatus::Done),
            None => self.set_op_status(handle, OpStatus::Failed),
        }
        proof {
            assert(self.operations()[handle].kind == OpKind::Transfer);
            assert(self.operations()[handle].purse == from);
        }
        (handle, result)
    }


    /// Tracked export: wraps [`Self::export_coin`] in a `KExport`
    /// operation. Returns the op handle so the caller can correlate
    /// later chain events to this op.
    pub fn tracked_export_coin(&mut self, key: (PurseId, u64))
        -> (handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::Available,
            old(self).next_handle < u64::MAX,
            old(self).events@.len() + 3 <= u64::MAX as nat,
        ensures
            final(self).invariant(),
            handle == old(self).next_handle,
            !old(self).operations().dom().contains(handle),
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle,
                kind: OpKind::Export,
                purse: key.0,
                status: OpStatus::Submitted,
            }),
            final(self).coins() == old(self).coins().insert(key, CoinRec {
                purse: old(self).coins()[key].purse,
                idx: old(self).coins()[key].idx,
                exponent: old(self).coins()[key].exponent,
                age: old(self).coins()[key].age,
                account: old(self).coins()[key].account,
                state: CoinState::Spent,
            }),
            final(self).purses() == old(self).purses(),
            final(self).entries() == old(self).entries(),
            final(self).next_handle == old(self).next_handle + 1,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@
                .push(Event::OperationStarted {
                    handle,
                    kind: OpKind::Export,
                    purse: key.0,
                })
                .push(Event::CoinSpent {
                    purse: key.0,
                    exponent: old(self).coins()[key].exponent,
                })
                .push(Event::OperationProgress {
                    handle,
                    status: OpStatus::Submitted,
                }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let h = self.start_op(OpKind::Export, key.0);
        proof {
            assert(self.operations()[h].kind == OpKind::Export);
            assert(self.operations()[h].purse == key.0);
        }
        self.export_coin(key);
        proof {
            assert(self.operations()[h].kind == OpKind::Export);
            assert(self.operations()[h].purse == key.0);
        }
        self.mark_op_submitted(h);
        h
    }


    /// Tracked import: wraps [`Self::import_coin`] in a `KImport`
    /// operation. Returns `(handle, new_coin_key)`.
    pub fn tracked_import_coin(&mut self, p: PurseId, exponent: u8, account: u64)
        -> (res: (OpHandle, (PurseId, u64)))
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(p),
            old(self).purses()[p].next_coin_idx < u64::MAX,
            old(self).next_age < u64::MAX,
            old(self).next_handle < u64::MAX,
            old(self).events@.len() + 3 <= u64::MAX as nat,
            exponent <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            res.0 == old(self).next_handle,
            !old(self).operations().dom().contains(res.0),
            res.1.0 == p,
            res.1.1 == old(self).purses()[p].next_coin_idx,
            final(self).operations() == old(self).operations().insert(res.0, OperationRec {
                handle: res.0,
                kind: OpKind::Import,
                purse: p,
                status: OpStatus::Submitted,
            }),
            final(self).coins() == old(self).coins().insert(res.1, CoinRec {
                purse: p,
                idx: res.1.1,
                exponent,
                state: CoinState::Available,
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
            final(self).entries() == old(self).entries(),
            final(self).next_handle == old(self).next_handle + 1,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@
                .push(Event::OperationStarted {
                    handle: res.0,
                    kind: OpKind::Import,
                    purse: p,
                })
                .push(Event::CoinAvailable { purse: p, exponent })
                .push(Event::OperationProgress {
                    handle: res.0,
                    status: OpStatus::Submitted,
                }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let h = self.start_op(OpKind::Import, p);
        proof {
            assert(self.operations()[h].kind == OpKind::Import);
            assert(self.operations()[h].purse == p);
        }
        let new_key = self.import_coin(p, exponent, account);
        proof {
            assert(self.operations()[h].kind == OpKind::Import);
            assert(self.operations()[h].purse == p);
        }
        self.mark_op_submitted(h);
        (h, new_key)
    }


    /// Tracked rebalance: wraps [`Self::rebalance`] in a `KRebalance`
    /// operation. Allocates the op handle, runs the rebalance (src
    /// coin → spent, dst coin minted), advances the op to `Submitted`.
    /// Returns `(handle, new_coin_key)` so the caller can correlate
    /// later chain events to this op.
    pub fn tracked_rebalance(
        &mut self,
        src: PurseId,
        dst: PurseId,
        key: (PurseId, u64),
    ) -> (res: (OpHandle, (PurseId, u64)))
        requires
            old(self).invariant(),
            src != dst,
            key.0 == src,
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::Available,
            old(self).purses().dom().contains(src),
            old(self).purses().dom().contains(dst),
            old(self).purses()[dst].next_coin_idx < u64::MAX,
            old(self).next_age < u64::MAX,
            old(self).next_handle < u64::MAX,
            old(self).events@.len() + 4 <= u64::MAX as nat,
        ensures
            final(self).invariant(),
            res.0 == old(self).next_handle,
            !old(self).operations().dom().contains(res.0),
            res.1.0 == dst,
            res.1.1 == old(self).purses()[dst].next_coin_idx,
            final(self).operations() == old(self).operations().insert(res.0, OperationRec {
                handle: res.0,
                kind: OpKind::Rebalance,
                purse: src,
                status: OpStatus::Submitted,
            }),
            final(self).coins() == old(self).coins()
                .insert(key, CoinRec {
                    purse: old(self).coins()[key].purse,
                    idx: old(self).coins()[key].idx,
                    exponent: old(self).coins()[key].exponent,
                    age: old(self).coins()[key].age,
                    account: old(self).coins()[key].account,
                    state: CoinState::Spent,
                })
                .insert(res.1, CoinRec {
                    purse: dst,
                    idx: res.1.1,
                    exponent: old(self).coins()[key].exponent,
                    state: CoinState::Available,
                    age: old(self).next_age,
                    account: 0,
                }),
            final(self).next_age == old(self).next_age + 1,
            final(self).purses().dom() =~= old(self).purses().dom(),
            final(self).purses()[dst].id == dst,
            final(self).purses()[dst].name == old(self).purses()[dst].name,
            final(self).purses()[dst].next_coin_idx
                == old(self).purses()[dst].next_coin_idx + 1,
            final(self).purses()[dst].next_entry_idx
                == old(self).purses()[dst].next_entry_idx,
            forall|q: PurseId| q != dst && #[trigger] old(self).purses().dom().contains(q)
                ==> final(self).purses()[q] == old(self).purses()[q],
            final(self).entries() == old(self).entries(),
            final(self).next_handle == old(self).next_handle + 1,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@
                .push(Event::OperationStarted {
                    handle: res.0,
                    kind: OpKind::Rebalance,
                    purse: src,
                })
                .push(Event::CoinSpent {
                    purse: src,
                    exponent: old(self).coins()[key].exponent,
                })
                .push(Event::CoinAvailable {
                    purse: dst,
                    exponent: old(self).coins()[key].exponent,
                })
                .push(Event::OperationProgress {
                    handle: res.0,
                    status: OpStatus::Submitted,
                }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let handle = self.start_op(OpKind::Rebalance, src);
        proof {
            assert(self.operations()[handle].kind == OpKind::Rebalance);
            assert(self.operations()[handle].purse == src);
        }
        let new_key = self.rebalance(src, dst, key);
        proof {
            assert(self.operations()[handle].kind == OpKind::Rebalance);
            assert(self.operations()[handle].purse == src);
        }
        self.mark_op_submitted(handle);
        (handle, new_key)
    }


    /// Tracked split: wraps [`Self::split_coin`] in a `KMaintenance`
    /// operation. Returns the op handle. Used when the host wants the
    /// chain to settle the split before the new coins are committed.
    pub fn tracked_split_coin(
        &mut self,
        key: (PurseId, u64),
        new_exponents: Vec<u8>,
    ) -> (handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::Available,
            old(self).purses().dom().contains(key.0),
            old(self).purses()[key.0].next_coin_idx as nat + new_exponents@.len()
                <= u64::MAX as nat,
            old(self).next_age as nat + new_exponents@.len() <= u64::MAX as nat,
            old(self).next_handle < u64::MAX,
            old(self).events@.len() + 3 <= u64::MAX as nat,
            forall|j: int| 0 <= j < new_exponents@.len() ==>
                (#[trigger] new_exponents@[j]) <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            handle == old(self).next_handle,
            !old(self).operations().dom().contains(handle),
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle,
                kind: OpKind::Maintenance,
                purse: key.0,
                status: OpStatus::Submitted,
            }),
            final(self).coins()[key] == (CoinRec {
                purse: old(self).coins()[key].purse,
                idx: old(self).coins()[key].idx,
                exponent: old(self).coins()[key].exponent,
                age: old(self).coins()[key].age,
                account: old(self).coins()[key].account,
                state: CoinState::Spent,
            }),
            forall|j: int| 0 <= j < new_exponents@.len() ==>
                #[trigger] final(self).coins()[
                    (key.0, (old(self).purses()[key.0].next_coin_idx + j) as u64)
                ] == (CoinRec {
                    purse: key.0,
                    idx: (old(self).purses()[key.0].next_coin_idx + j) as u64,
                    exponent: new_exponents@[j],
                    state: CoinState::Pending,
                    age: (old(self).next_age + j) as u64,
                    account: 0,
                }),
            final(self).coins().dom() =~= old(self).coins().dom().union(
                Set::new(|k: (PurseId, u64)|
                    k.0 == key.0
                    && (old(self).purses()[key.0].next_coin_idx as int) <= (k.1 as int)
                    && (k.1 as int) < (old(self).purses()[key.0].next_coin_idx as int)
                                       + new_exponents@.len() as int)
            ),
            forall|k: (PurseId, u64)| #[trigger] old(self).coins().dom().contains(k)
                && k != key
                ==> final(self).coins()[k] == old(self).coins()[k],
            final(self).purses().dom() =~= old(self).purses().dom(),
            final(self).purses()[key.0].id == key.0,
            final(self).purses()[key.0].name == old(self).purses()[key.0].name,
            final(self).purses()[key.0].next_coin_idx
                == old(self).purses()[key.0].next_coin_idx + new_exponents@.len(),
            final(self).purses()[key.0].next_entry_idx
                == old(self).purses()[key.0].next_entry_idx,
            forall|q: PurseId| q != key.0 && #[trigger] old(self).purses().dom().contains(q)
                ==> final(self).purses()[q] == old(self).purses()[q],
            final(self).next_age == old(self).next_age + new_exponents@.len(),
            final(self).next_handle == old(self).next_handle + 1,
            final(self).entries() == old(self).entries(),
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@
                .push(Event::OperationStarted {
                    handle,
                    kind: OpKind::Maintenance,
                    purse: key.0,
                })
                .push(Event::CoinSpent {
                    purse: key.0,
                    exponent: old(self).coins()[key].exponent,
                })
                .push(Event::OperationProgress {
                    handle,
                    status: OpStatus::Submitted,
                }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let h = self.start_op(OpKind::Maintenance, key.0);
        proof {
            assert(self.operations()[h].kind == OpKind::Maintenance);
            assert(self.operations()[h].purse == key.0);
            assert(self.coins()[key].state == CoinState::Available);
        }
        self.split_coin(key, new_exponents);
        proof {
            assert(self.operations()[h].kind == OpKind::Maintenance);
            assert(self.operations()[h].purse == key.0);
        }
        self.mark_op_submitted(h);
        h
    }


    /// Tracked unload via entry: wraps [`Self::unload_via_entry`] in a
    /// `KExternalOffload` operation. Allocates the op handle, runs the
    /// unload (entry → coin), then advances the op to `Submitted`.
    /// Returns `(handle, new_coin_key)` so callers can correlate later
    /// chain events to this operation.
    ///
    /// Quint analog: the full lifecycle of `startExternalOffload`
    /// reduced to its local-state effects.
    pub fn tracked_unload_via_entry(&mut self, key: (PurseId, u64))
        -> (res: (OpHandle, (PurseId, u64)))
        requires
            old(self).invariant(),
            old(self).entries().dom().contains(key),
            old(self).entries()[key].local == EntryLocal::LocalAvailable,
            old(self).entries()[key].on_chain == EntryOnChain::Ready,
            old(self).purses().dom().contains(key.0),
            old(self).purses()[key.0].next_coin_idx < u64::MAX,
            old(self).next_age < u64::MAX,
            old(self).next_handle < u64::MAX,
            old(self).events@.len() + 3 <= u64::MAX as nat,
        ensures
            final(self).invariant(),
            res.0 == old(self).next_handle,
            !old(self).operations().dom().contains(res.0),
            res.1.0 == key.0,
            res.1.1 == old(self).purses()[key.0].next_coin_idx,
            final(self).operations() == old(self).operations().insert(res.0, OperationRec {
                handle: res.0,
                kind: OpKind::ExternalOffload,
                purse: key.0,
                status: OpStatus::Submitted,
            }),
            final(self).entries() == old(self).entries().insert(key, EntryRec {
                local: EntryLocal::LocalConsumed,
                ..old(self).entries()[key]
            }),
            final(self).coins() == old(self).coins().insert(res.1, CoinRec {
                purse: key.0,
                idx: res.1.1,
                exponent: old(self).entries()[key].exponent,
                state: CoinState::Available,
                age: old(self).next_age,
                account: 0,
            }),
            final(self).purses().dom() =~= old(self).purses().dom(),
            final(self).purses()[key.0].id == key.0,
            final(self).purses()[key.0].name == old(self).purses()[key.0].name,
            final(self).purses()[key.0].next_coin_idx
                == old(self).purses()[key.0].next_coin_idx + 1,
            final(self).purses()[key.0].next_entry_idx
                == old(self).purses()[key.0].next_entry_idx,
            forall|q: PurseId| q != key.0 && #[trigger] old(self).purses().dom().contains(q)
                ==> final(self).purses()[q] == old(self).purses()[q],
            final(self).next_age == old(self).next_age + 1,
            final(self).next_handle == old(self).next_handle + 1,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@
                .push(Event::OperationStarted {
                    handle: res.0,
                    kind: OpKind::ExternalOffload,
                    purse: key.0,
                })
                .push(Event::CoinAvailable {
                    purse: key.0,
                    exponent: old(self).entries()[key].exponent,
                })
                .push(Event::OperationProgress {
                    handle: res.0,
                    status: OpStatus::Submitted,
                }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let handle = self.start_op(OpKind::ExternalOffload, key.0);
        proof {
            assert(self.operations()[handle].kind == OpKind::ExternalOffload);
            assert(self.operations()[handle].purse == key.0);
        }
        let new_coin_key = self.unload_via_entry(key, handle);
        proof {
            assert(self.operations()[handle].kind == OpKind::ExternalOffload);
            assert(self.operations()[handle].purse == key.0);
        }
        self.mark_op_submitted(handle);
        (handle, new_coin_key)
    }


    /// Tracked top-up via entry: wraps [`Self::top_up_via_entry`] in
    /// a `KTopUp` operation that starts in `Preparing` and immediately
    /// advances to `Submitted` (the extrinsic creating the entry has
    /// been broadcast to the chain). The op's later transitions
    /// (`InBlock`, `Finalized`, `Waiting(ready_at)`, `Done`) fire as
    /// chain notifications arrive — those are driven by the host via
    /// the `mark_op_*` primitives.
    ///
    /// Quint analog: the combination of `startTopUp` + `opCommitTopUp`.
    pub fn tracked_top_up_via_entry(
        &mut self,
        p: PurseId,
        exponent: u8,
        member_key: u64,
        allocated_at: u64,
        ready_at: u64,
        ring_idx: u64,
    ) -> (res: (OpHandle, (PurseId, u64)))
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(p),
            old(self).purses()[p].next_entry_idx < u64::MAX,
            old(self).next_handle < u64::MAX,
            old(self).events@.len() + 3 <= u64::MAX as nat,
            exponent <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            res.0 == old(self).next_handle,
            !old(self).operations().dom().contains(res.0),
            res.1.0 == p,
            res.1.1 == old(self).purses()[p].next_entry_idx,
            final(self).operations() == old(self).operations().insert(res.0, OperationRec {
                handle: res.0,
                kind: OpKind::TopUp,
                purse: p,
                status: OpStatus::Submitted,
            }),
            final(self).entries() == old(self).entries().insert(res.1, EntryRec {
                purse: p,
                idx: res.1.1,
                exponent,
                on_chain: EntryOnChain::Waiting,
                local: EntryLocal::LocalAvailable,
                member_key,
                allocated_at,
                ready_at,
                ring_idx,
            }),
            final(self).coins() == old(self).coins(),
            final(self).purses().dom() =~= old(self).purses().dom(),
            final(self).purses()[p].id == p,
            final(self).purses()[p].name == old(self).purses()[p].name,
            final(self).purses()[p].next_coin_idx
                == old(self).purses()[p].next_coin_idx,
            final(self).purses()[p].next_entry_idx
                == old(self).purses()[p].next_entry_idx + 1,
            forall|q: PurseId| q != p && #[trigger] old(self).purses().dom().contains(q)
                ==> final(self).purses()[q] == old(self).purses()[q],
            final(self).next_age == old(self).next_age,
            final(self).next_handle == old(self).next_handle + 1,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@
                .push(Event::OperationStarted {
                    handle: res.0,
                    kind: OpKind::TopUp,
                    purse: p,
                })
                .push(Event::EntryAllocated { purse: p, exponent })
                .push(Event::OperationProgress {
                    handle: res.0,
                    status: OpStatus::Submitted,
                }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let handle = self.start_op(OpKind::TopUp, p);
        let key = self.top_up_via_entry(
            p, exponent, member_key, allocated_at, ready_at, ring_idx,
        );
        proof {
            assert(self.operations()[handle].kind == OpKind::TopUp);
            assert(self.operations()[handle].purse == p);
        }
        self.mark_op_submitted(handle);
        (handle, key)
    }

}

} // verus!
