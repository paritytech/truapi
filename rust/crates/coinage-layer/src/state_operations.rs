//! Operation lifecycle: `start_op`, status transitions, bulk lock-release helpers.

use vstd::prelude::*;

use crate::*;

verus! {

impl State {
    /// Start a new operation in the `Preparing` state. Allocates a fresh
    /// `OpHandle` from the layer's allocator. Quint analog: the local-
    /// state effect of starting any operation kind (the chain interaction
    /// is deferred to `transition_op_status`).
    pub fn start_op(&mut self, kind: OpKind, purse: PurseId) -> (handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(purse),
            old(self).next_handle < u64::MAX,
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            handle == old(self).next_handle,
            !old(self).operations().dom().contains(handle),
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle,
                kind,
                purse,
                status: OpStatus::Preparing,
            }),
            final(self).next_handle == old(self).next_handle + 1,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::OperationStarted {
                handle,
                kind,
                purse,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            // Other state untouched.
            final(self).purses() == old(self).purses(),
            final(self).purses@ == old(self).purses@,
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).next_purse_id == old(self).next_purse_id,
            // lock_refint preservation: operations.dom strictly grows
            // (adds `handle`), and coins/entries are untouched. Every
            // existing edge in refint still points into the larger ops set.
            lock_refint(old(self).coins(), old(self).entries(), old(self).operations())
                ==> lock_refint(final(self).coins(), final(self).entries(),
                                final(self).operations()),
    {
        let ghost old_ops = self.spec_operations@;
        let ghost old_ops_vec = self.operations@;
        let ghost old_m = self.spec_purses@;
        let handle = self.next_handle;
        let new_op = OperationRec {
            handle,
            kind,
            purse,
            status: OpStatus::Preparing,
        };
        // Each existing operation's handle is strictly less than the new one
        // by old invariant (v).
        proof {
            assert forall|i: int| 0 <= i < old_ops_vec.len() implies
                #[trigger] old_ops_vec[i].handle < handle
            by {
                assert(old_ops.dom().contains(old_ops_vec[i].handle));
            }
        }
        self.operations.push(new_op);
        proof {
            self.spec_operations = Ghost(self.spec_operations@.insert(handle, new_op));
        }
        self.next_handle = handle + 1;

        proof {
            // Purses / coins / entries are entirely untouched.
            assert(self.purses@ == old(self).purses@);
            assert(self.spec_purses@ == old_m);
            assert(self.coins@ == old(self).coins@);
            assert(self.spec_coins@ == old(self).spec_coins@);
            assert(self.entries@ == old(self).entries@);
            assert(self.spec_entries@ == old(self).spec_entries@);
            assert(self.next_purse_id == old(self).next_purse_id);

            let new_ops = self.spec_operations@;
            let new_ops_vec = self.operations@;
            let last = old_ops_vec.len() as int;
            assert(new_ops_vec.len() == old_ops_vec.len() + 1);
            assert(new_ops_vec[last] == new_op);
            assert forall|i: int| 0 <= i < old_ops_vec.len() implies
                #[trigger] new_ops_vec[i] == old_ops_vec[i]
            by {}

            // (u) key consistency.
            assert forall|h: OpHandle| #[trigger] new_ops.dom().contains(h)
                implies new_ops[h].handle == h
            by {
                if h != handle { assert(old_ops.dom().contains(h)); }
            }
            // (v) handle below allocator.
            assert forall|h: OpHandle| #[trigger] new_ops.dom().contains(h)
                implies h < self.next_handle
            by {
                if h != handle { assert(old_ops.dom().contains(h)); }
            }
            // (w) refint.
            assert forall|h: OpHandle| #[trigger] new_ops.dom().contains(h)
                implies self.spec_purses@.dom().contains(new_ops[h].purse)
            by {
                if h == handle {
                    assert(new_ops[handle].purse == purse);
                } else {
                    assert(old_ops.dom().contains(h));
                }
            }
            // (x) Vec → ghost.
            assert forall|i: int| 0 <= i < new_ops_vec.len() implies
                new_ops.dom().contains((#[trigger] new_ops_vec[i]).handle)
                && new_ops[new_ops_vec[i].handle] == new_ops_vec[i]
            by {
                if i == last {
                    assert(new_ops_vec[i] == new_op);
                    assert(new_ops[handle] == new_op);
                } else {
                    assert(new_ops_vec[i] == old_ops_vec[i]);
                    assert(old_ops.dom().contains(old_ops_vec[i].handle));
                    assert(old_ops_vec[i].handle != handle);
                    assert(old_ops[old_ops_vec[i].handle] == old_ops_vec[i]);
                }
            }
            // (y) ghost → Vec.
            assert forall|h: OpHandle| #[trigger] new_ops.dom().contains(h)
                implies exists|i: int|
                    0 <= i < new_ops_vec.len()
                    && #[trigger] new_ops_vec[i].handle == h
            by {
                if h == handle {
                    let w = last;
                    assert(new_ops_vec[w].handle == handle);
                } else {
                    assert(old_ops.dom().contains(h));
                    let w = choose|i: int|
                        0 <= i < old_ops_vec.len()
                        && #[trigger] old_ops_vec[i].handle == h;
                    assert(new_ops_vec[w] == old_ops_vec[w]);
                }
            }
            // (z) no duplicates.
            assert forall|a: int, b: int|
                0 <= a < new_ops_vec.len() && 0 <= b < new_ops_vec.len()
                && (#[trigger] new_ops_vec[a]).handle
                    == (#[trigger] new_ops_vec[b]).handle
                implies a == b
            by {
                if a == last && b == last {
                } else if a == last {
                    assert(new_ops_vec[b] == old_ops_vec[b]);
                    assert(new_ops_vec[a].handle == handle);
                    assert(old_ops_vec[b].handle < handle);
                } else if b == last {
                    assert(new_ops_vec[a] == old_ops_vec[a]);
                    assert(new_ops_vec[b].handle == handle);
                    assert(old_ops_vec[a].handle < handle);
                } else {
                    assert(new_ops_vec[a] == old_ops_vec[a]);
                    assert(new_ops_vec[b] == old_ops_vec[b]);
                }
            }
        }
        self.emit_event(Event::OperationStarted { handle, kind, purse });
        handle
    }


    /// Transition the operation identified by `handle` to a new status.
    /// Mirror of `set_entry_on_chain` for operations. Used by named
    /// wrappers (`mark_op_submitted`, `mark_op_done`, `mark_op_failed`).
    pub fn set_op_status(&mut self, handle: OpHandle, new_status: OpStatus)
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
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
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: new_status,
            }),
    {
        let ghost old_purses_vec = self.purses@;
        let ghost old_spec_purses = self.spec_purses@;
        let ghost old_coins = self.spec_coins@;
        let ghost old_coins_vec = self.coins@;
        let ghost old_entries = self.spec_entries@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_operations = self.spec_operations@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_ops = self.spec_operations@;
        let ghost old_ops_vec = self.operations@;

        let mut j: usize = 0;
        while j < self.operations.len()
            invariant
                0 <= j <= self.operations.len(),
                self.invariant(),
                self.purses@ == old_purses_vec,
                self.spec_purses@ == old_spec_purses,
                self.next_purse_id == old(self).next_purse_id,
                self.spec_coins@ == old_coins,
                self.coins@ == old_coins_vec,
                self.spec_entries@ == old_entries,
                self.entries@ == old_entries_vec,
                self.spec_operations@ == old_operations,
                self.operations@ == old_operations_vec,
                self.spec_operations@ == old_ops,
                self.operations@ == old_ops_vec,
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
                old_purses_vec == old(self).purses@,
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
                old_ops == old(self).spec_operations@,
                old_ops == old(self).operations(),
                old_ops.dom().contains(handle),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.operations@[jj]).handle != handle,
            decreases self.operations.len() - j,
        {
            if self.operations[j].handle == handle {
                let ghost target_idx = j as int;
                let ghost updated = OperationRec {
                    handle: old_ops[handle].handle,
                    kind: old_ops[handle].kind,
                    purse: old_ops[handle].purse,
                    status: new_status,
                };
                self.operations[j].status = new_status;

                proof {
                    assert(old_ops[handle].handle == handle);
                    self.spec_operations = Ghost(self.spec_operations@.insert(handle, updated));

                    let new_ops_vec = self.operations@;
                    let new_ops = self.spec_operations@;

                    assert(new_ops_vec[target_idx].handle == handle);
                    assert(new_ops_vec[target_idx].kind == old_ops_vec[target_idx].kind);
                    assert(new_ops_vec[target_idx].purse == old_ops_vec[target_idx].purse);
                    assert(new_ops_vec[target_idx].status == new_status);
                    assert forall|k: int|
                        0 <= k < new_ops_vec.len() && k != target_idx implies
                        #[trigger] new_ops_vec[k] == old_ops_vec[k]
                    by {}
                    assert(old_ops_vec[target_idx].handle == handle);
                    assert forall|kk: int|
                        0 <= kk < old_ops_vec.len() && kk != target_idx implies
                        (#[trigger] old_ops_vec[kk]).handle != handle
                    by {}

                    // (u) handle consistency.
                    assert forall|h: OpHandle| #[trigger] new_ops.dom().contains(h)
                        implies new_ops[h].handle == h
                    by { if h != handle { assert(old_ops.dom().contains(h)); } }
                    // (v) handle bound.
                    assert forall|h: OpHandle| #[trigger] new_ops.dom().contains(h)
                        implies h < self.next_handle
                    by { assert(old_ops.dom().contains(h)); }
                    // (w) refint.
                    assert forall|h: OpHandle| #[trigger] new_ops.dom().contains(h)
                        implies self.spec_purses@.dom().contains(new_ops[h].purse)
                    by {
                        if h != handle { assert(old_ops.dom().contains(h)); }
                    }
                    // (x) Vec → ghost.
                    assert forall|i: int| 0 <= i < new_ops_vec.len() implies
                        new_ops.dom().contains((#[trigger] new_ops_vec[i]).handle)
                        && new_ops[new_ops_vec[i].handle] == new_ops_vec[i]
                    by {
                        if i == target_idx {
                            assert(new_ops[handle] == updated);
                            assert(updated == new_ops_vec[target_idx]);
                        } else {
                            assert(new_ops_vec[i] == old_ops_vec[i]);
                            let oo = old_ops_vec[i];
                            assert(old_ops.dom().contains(oo.handle));
                            assert(oo.handle != handle);
                            assert(old_ops[oo.handle] == oo);
                        }
                    }
                    // (y) ghost → Vec.
                    assert forall|h: OpHandle| #[trigger] new_ops.dom().contains(h)
                        implies exists|i: int|
                            0 <= i < new_ops_vec.len()
                            && #[trigger] new_ops_vec[i].handle == h
                    by {
                        if h == handle {
                            let w = target_idx;
                            assert(new_ops_vec[w].handle == h);
                        } else {
                            assert(old_ops.dom().contains(h));
                            let w = choose|i: int|
                                0 <= i < old_ops_vec.len()
                                && #[trigger] old_ops_vec[i].handle == h;
                            assert(new_ops_vec[w] == old_ops_vec[w]);
                        }
                    }
                    // (z) no duplicates.
                    assert forall|a: int, b: int|
                        0 <= a < new_ops_vec.len() && 0 <= b < new_ops_vec.len()
                        && (#[trigger] new_ops_vec[a]).handle
                            == (#[trigger] new_ops_vec[b]).handle
                        implies a == b
                    by {
                        if a == target_idx && b == target_idx {
                        } else if a == target_idx {
                            assert(new_ops_vec[b] == old_ops_vec[b]);
                        } else if b == target_idx {
                            assert(new_ops_vec[a] == old_ops_vec[a]);
                        } else {
                            assert(new_ops_vec[a] == old_ops_vec[a]);
                            assert(new_ops_vec[b] == old_ops_vec[b]);
                        }
                    }

                    // Purses / coins / entries entirely unchanged.
                    assert(self.purses@ == old(self).purses@);
                    assert(self.spec_purses@ == old(self).spec_purses@);
                    assert(self.coins@ == old(self).coins@);
                    assert(self.spec_coins@ == old(self).spec_coins@);
                    assert(self.entries@ == old(self).entries@);
                    assert(self.spec_entries@ == old(self).spec_entries@);
                }
                return;
            }
            j += 1;
        }
        proof {
            assert(old_ops.dom().contains(handle));
            let w = choose|jj: int|
                0 <= jj < old_ops_vec.len()
                && #[trigger] old_ops_vec[jj].handle == handle;
        }
        vstd::pervasive::unreached()
    }


    /// Operation lifecycle: `Preparing` → `Submitted`. Phase order
    /// gate matching Quint `submitOp`.
    pub fn mark_op_submitted(&mut self, handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            old(self).operations()[handle].status == OpStatus::Preparing,
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::OperationProgress {
                handle,
                status: OpStatus::Submitted,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: OpStatus::Submitted,
            }),
    {
        self.set_op_status(handle, OpStatus::Submitted);
        self.emit_event(Event::OperationProgress {
            handle,
            status: OpStatus::Submitted,
        });
    }


    /// Operation lifecycle: `Submitted` → `InBlock`. Fires when the
    /// extrinsic lands in a block.
    pub fn mark_op_in_block(&mut self, handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            old(self).operations()[handle].status == OpStatus::Submitted,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
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
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: OpStatus::InBlock,
            }),
    {
        self.set_op_status(handle, OpStatus::InBlock);
    }


    /// Operation lifecycle: `InBlock` → `Finalized`.
    pub fn mark_op_finalized(&mut self, handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            old(self).operations()[handle].status == OpStatus::InBlock,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
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
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: OpStatus::Finalized,
            }),
    {
        self.set_op_status(handle, OpStatus::Finalized);
    }


    /// Operation lifecycle: `Finalized` → `Waiting(ready_at)`. Used by
    /// top-up: the op waits for a freshly-allocated entry to mature
    /// before it can be marked `Done`.
    pub fn mark_op_waiting(&mut self, handle: OpHandle, ready_at: u64)
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            old(self).operations()[handle].status == OpStatus::Finalized,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
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
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: OpStatus::Waiting(ready_at),
            }),
    {
        self.set_op_status(handle, OpStatus::Waiting(ready_at));
    }


    /// Operation lifecycle: `Finalized | Waiting(_)` → `Done`. Marks
    /// the operation as successfully completed.
    pub fn mark_op_done(&mut self, handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            match old(self).operations()[handle].status {
                OpStatus::Finalized => true,
                OpStatus::Waiting(_) => true,
                _ => false,
            },
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::OperationCompleted {
                handle,
                status: OpStatus::Done,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: OpStatus::Done,
            }),
    {
        self.set_op_status(handle, OpStatus::Done);
        self.emit_event(Event::OperationCompleted {
            handle,
            status: OpStatus::Done,
        });
    }


    /// Operation lifecycle: any cancellable status (`Preparing`,
    /// `Waiting(_)`) → `Failed`. Quint analog: `cancelOp`'s status
    /// transition. The caller is responsible for releasing locks via
    /// [`Self::release_locked_coin`] / [`Self::release_locked_entry`]
    /// before or after invoking this; the bulk-sweep is not bundled
    /// here because the cross-state refint invariant that would let
    /// us prove "no LockedFor(h) remains" is not yet in the model.
    pub fn set_op_failed(&mut self, handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            match old(self).operations()[handle].status {
                OpStatus::Preparing => true,
                OpStatus::Waiting(_) => true,
                _ => false,
            },
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::OperationCompleted {
                handle,
                status: OpStatus::Failed,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: OpStatus::Failed,
            }),
    {
        self.set_op_status(handle, OpStatus::Failed);
        self.emit_event(Event::OperationCompleted {
            handle,
            status: OpStatus::Failed,
        });
    }


    /// Find and release a single coin locked for `handle`. Returns the
    /// released key, or `None` if no coin is currently `LockedFor(handle)`.
    ///
    /// Building block for bulk sweeps: callers loop until `None` to
    /// drain all locks. Decomposes the bulk-sweep proof obligation
    /// into one-step ghost map updates, which Verus discharges
    /// directly via the underlying release_locked_coin contract.
    pub fn release_one_coin_lock_for(&mut self, handle: OpHandle)
        -> (res: Option<(PurseId, u64)>)
        requires
            old(self).invariant(),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
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
            match res {
                Some(key) =>
                    old(self).coins().dom().contains(key)
                    && old(self).coins()[key].state == CoinState::LockedFor(handle)
                    && final(self).coins() ==
                        old(self).coins().insert(key, CoinRec {
                            purse: old(self).coins()[key].purse,
                            idx: old(self).coins()[key].idx,
                            exponent: old(self).coins()[key].exponent,
                            age: old(self).coins()[key].age,
                            account: old(self).coins()[key].account,
                            state: CoinState::Available,
                        }),
                None =>
                    final(self).coins() == old(self).coins()
                    && final(self).coins@ == old(self).coins@
                    && forall|k: (PurseId, u64)|
                        #[trigger] old(self).coins().dom().contains(k)
                        ==> old(self).coins()[k].state != CoinState::LockedFor(handle),
            },
    {
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                self == old(self),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).state != CoinState::LockedFor(handle),
            decreases self.coins.len() - j,
        {
            let needs_release = match self.coins[j].state {
                CoinState::LockedFor(h) => h == handle,
                _ => false,
            };
            if needs_release {
                let key = (self.coins[j].purse, self.coins[j].idx);
                proof {
                    assert(self.spec_coins@.dom().contains(key));
                    assert(self.coins()[key] == self.coins@[j as int]);
                    assert(self.coins()[key].state == CoinState::LockedFor(handle));
                }
                self.release_locked_coin(key, handle);
                return Some(key);
            }
            j = j + 1;
        }
        // No match: lift Vec-side bound to ghost map.
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] old(self).coins().dom().contains(k)
                implies old(self).coins()[k].state != CoinState::LockedFor(handle)
            by {
                let w = choose|jj: int|
                    0 <= jj < self.coins@.len()
                    && #[trigger] self.coins@[jj].purse == k.0
                    && self.coins@[jj].idx == k.1;
                assert(self.coins@[w].state == self.coins()[k].state);
            }
        }
        None
    }


    /// Find and release a single entry locally locked for `handle`.
    /// Returns the released key, or `None` if no entry is currently
    /// `LocalLockedFor(handle)`. Entry parallel of
    /// [`Self::release_one_coin_lock_for`].
    pub fn release_one_entry_lock_for(&mut self, handle: OpHandle)
        -> (res: Option<(PurseId, u64)>)
        requires
            old(self).invariant(),
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
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
            match res {
                Some(key) =>
                    old(self).entries().dom().contains(key)
                    && old(self).entries()[key].local
                        == EntryLocal::LocalLockedFor(handle)
                    && final(self).entries() ==
                        old(self).entries().insert(key, EntryRec {
                            purse: old(self).entries()[key].purse,
                            idx: old(self).entries()[key].idx,
                            exponent: old(self).entries()[key].exponent,
                            member_key: old(self).entries()[key].member_key,
                            allocated_at: old(self).entries()[key].allocated_at,
                            ready_at: old(self).entries()[key].ready_at,
                            ring_idx: old(self).entries()[key].ring_idx,
                            on_chain: old(self).entries()[key].on_chain,
                            local: EntryLocal::LocalAvailable,
                        }),
                None =>
                    final(self).entries() == old(self).entries()
                    && final(self).entries@ == old(self).entries@
                    && forall|k: (PurseId, u64)|
                        #[trigger] old(self).entries().dom().contains(k)
                        ==> old(self).entries()[k].local
                            != EntryLocal::LocalLockedFor(handle),
            },
    {
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.invariant(),
                self == old(self),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.entries@[jj]).local
                        != EntryLocal::LocalLockedFor(handle),
            decreases self.entries.len() - j,
        {
            let needs_release = match self.entries[j].local {
                EntryLocal::LocalLockedFor(h) => h == handle,
                _ => false,
            };
            if needs_release {
                let key = (self.entries[j].purse, self.entries[j].idx);
                proof {
                    assert(self.spec_entries@.dom().contains(key));
                    assert(self.entries()[key] == self.entries@[j as int]);
                    assert(self.entries()[key].local
                        == EntryLocal::LocalLockedFor(handle));
                }
                self.release_locked_entry(key, handle);
                return Some(key);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] old(self).entries().dom().contains(k)
                implies old(self).entries()[k].local
                    != EntryLocal::LocalLockedFor(handle)
            by {
                let w = choose|jj: int|
                    0 <= jj < self.entries@.len()
                    && #[trigger] self.entries@[jj].purse == k.0
                    && self.entries@[jj].idx == k.1;
                assert(self.entries@[w].local == self.entries()[k].local);
            }
        }
        None
    }

}

} // verus!
