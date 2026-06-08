//! Atomic op composites: kick-off, cancel, commit (coin and entry variants).

use vstd::prelude::*;

use crate::*;

verus! {

impl State {
    /// Atomic composite: commit an op that's holding one locked entry.
    /// Consumes the entry (`LocalLockedFor → LocalConsumed`) and
    /// marks the op `Done`. Used by the commit path of unload /
    /// external-offload when the chain has confirmed the entry-spend
    /// extrinsic.
    pub fn commit_op_consuming_locked_entry(
        &mut self,
        handle: OpHandle,
        key: (PurseId, u64),
    )
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            old(self).operations()[handle].status == OpStatus::Finalized,
            old(self).entries().dom().contains(key),
            old(self).entries()[key].local == EntryLocal::LocalLockedFor(handle),
            old(self).events@.len() + 2 <= u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).entries() == old(self).entries().insert(key, EntryRec {
                local: EntryLocal::LocalConsumed,
                ..old(self).entries()[key]
            }),
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: OpStatus::Done,
            }),
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@
                .push(Event::EntryConsumed {
                    purse: key.0,
                    exponent: old(self).entries()[key].exponent,
                })
                .push(Event::OperationCompleted {
                    handle,
                    status: OpStatus::Done,
                }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        self.consume_entry(key);
        self.mark_op_done(handle);
    }


    /// Atomic composite: commit an op that's holding one locked coin.
    /// Consumes the coin (`LockedFor → PendingSpend → Spent`) and
    /// marks the op `Done`. Used by the commit path of transfer /
    /// rebalance / export when the chain has finalized the spend.
    pub fn commit_op_consuming_locked_coin(
        &mut self,
        handle: OpHandle,
        key: (PurseId, u64),
    )
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            old(self).operations()[handle].status == OpStatus::Finalized,
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::LockedFor(handle),
            old(self).events@.len() + 2 <= u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).entries() == old(self).entries(),
            final(self).coins() == old(self).coins().insert(key, CoinRec {
                purse: old(self).coins()[key].purse,
                idx: old(self).coins()[key].idx,
                exponent: old(self).coins()[key].exponent,
                age: old(self).coins()[key].age,
                account: old(self).coins()[key].account,
                state: CoinState::Spent,
            }),
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: OpStatus::Done,
            }),
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@
                .push(Event::CoinSpent {
                    purse: key.0,
                    exponent: old(self).coins()[key].exponent,
                })
                .push(Event::OperationCompleted {
                    handle,
                    status: OpStatus::Done,
                }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        self.commit_locked_coin(key);
        self.mark_coin_spent(key);
        self.mark_op_done(handle);
    }


    /// Atomic composite: cancel an op that's holding one locked coin.
    /// Releases the coin back to `Available` and marks the op
    /// `Failed`. Inverse of [`Self::start_op_locking_coin`] (when the
    /// op was started and the lock holds but the op hasn't progressed
    /// beyond `Preparing` / `Waiting(_)`).
    pub fn cancel_op_releasing_coin(
        &mut self,
        handle: OpHandle,
        key: (PurseId, u64),
    )
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            match old(self).operations()[handle].status {
                OpStatus::Preparing => true,
                OpStatus::Waiting(_) => true,
                _ => false,
            },
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::LockedFor(handle),
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
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: OpStatus::Failed,
            }),
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
    {
        self.release_locked_coin(key, handle);
        self.set_op_failed(handle);
    }


    /// Atomic composite: cancel an op that's holding one locked entry.
    /// Releases the entry back to `LocalAvailable` and marks the op
    /// `Failed`. Inverse of [`Self::start_op_locking_entry`].
    pub fn cancel_op_releasing_entry(
        &mut self,
        handle: OpHandle,
        key: (PurseId, u64),
    )
        requires
            old(self).invariant(),
            old(self).operations().dom().contains(handle),
            match old(self).operations()[handle].status {
                OpStatus::Preparing => true,
                OpStatus::Waiting(_) => true,
                _ => false,
            },
            old(self).entries().dom().contains(key),
            old(self).entries()[key].local == EntryLocal::LocalLockedFor(handle),
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).entries() == old(self).entries().insert(key, EntryRec {
                local: EntryLocal::LocalAvailable,
                ..old(self).entries()[key]
            }),
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle: old(self).operations()[handle].handle,
                kind: old(self).operations()[handle].kind,
                purse: old(self).operations()[handle].purse,
                status: OpStatus::Failed,
            }),
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
    {
        self.release_locked_entry(key, handle);
        self.set_op_failed(handle);
    }


    /// Atomic composite: start a new operation and lock `key`'s coin
    /// for it. The coin must currently be `Available`; on return it
    /// is `LockedFor(handle)`, and the operation is in `Preparing`.
    ///
    /// This is the canonical entry point for op flows that reserve a
    /// specific coin upfront (transfer, rebalance, export). Avoids
    /// the temporal-gap problem of separately starting the op then
    /// locking the coin, where another concurrent call could observe
    /// the half-built state.
    /// Atomic composite: start a new operation and lock `key`'s entry
    /// for it. The entry must currently be `LocalAvailable`; on
    /// return it is `LocalLockedFor(handle)`, and the operation is
    /// in `Preparing`. Mirror of [`Self::start_op_locking_coin`] for
    /// recycler-entry-bearing op flows (unload, external offload).
    pub fn start_op_locking_entry(
        &mut self,
        kind: OpKind,
        key: (PurseId, u64),
    ) -> (handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).entries().dom().contains(key),
            old(self).entries()[key].local == EntryLocal::LocalAvailable,
            old(self).purses().dom().contains(key.0),
            old(self).next_handle < u64::MAX,
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            handle == old(self).next_handle,
            !old(self).operations().dom().contains(handle),
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle,
                kind,
                purse: key.0,
                status: OpStatus::Preparing,
            }),
            final(self).entries() == old(self).entries().insert(key, EntryRec {
                local: EntryLocal::LocalLockedFor(handle),
                ..old(self).entries()[key]
            }),
            final(self).purses() == old(self).purses(),
            final(self).coins() == old(self).coins(),
            final(self).next_handle == old(self).next_handle + 1,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::OperationStarted {
                handle,
                kind,
                purse: key.0,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let handle = self.start_op(kind, key.0);
        proof {
            assert(self.entries()[key].local == EntryLocal::LocalAvailable);
        }
        self.lock_entry(key, handle);
        handle
    }


    pub fn start_op_locking_coin(
        &mut self,
        kind: OpKind,
        key: (PurseId, u64),
    ) -> (handle: OpHandle)
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::Available,
            old(self).purses().dom().contains(key.0),
            old(self).next_handle < u64::MAX,
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            handle == old(self).next_handle,
            !old(self).operations().dom().contains(handle),
            final(self).operations() == old(self).operations().insert(handle, OperationRec {
                handle,
                kind,
                purse: key.0,
                status: OpStatus::Preparing,
            }),
            final(self).coins() == old(self).coins().insert(key, CoinRec {
                purse: old(self).coins()[key].purse,
                idx: old(self).coins()[key].idx,
                exponent: old(self).coins()[key].exponent,
                age: old(self).coins()[key].age,
                account: old(self).coins()[key].account,
                state: CoinState::LockedFor(handle),
            }),
            final(self).purses() == old(self).purses(),
            final(self).entries() == old(self).entries(),
            final(self).next_handle == old(self).next_handle + 1,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::OperationStarted {
                handle,
                kind,
                purse: key.0,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let handle = self.start_op(kind, key.0);
        proof {
            assert(self.coins()[key].state == CoinState::Available);
        }
        self.lock_coin(key, handle);
        handle
    }

}

} // verus!
