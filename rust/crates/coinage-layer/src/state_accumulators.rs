//! Accumulators (`total_in`, `total_out`, `paid_ring_membership`) and the extrinsic-id allocator.

use vstd::prelude::*;

use crate::*;

verus! {

impl State {
    /// Increment `total_in` by `amount` (Quint accumulator advance on
    /// inflow: top-up, import).
    pub fn add_total_in(&mut self, amount: u64)
        requires
            old(self).invariant(),
            old(self).total_in <= u64::MAX - amount,
        ensures
            final(self).invariant(),
            final(self).total_in == old(self).total_in + amount,
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
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
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
        self.total_in = self.total_in + amount;
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
        }
    }


    /// Increment `total_out` by `amount` (Quint accumulator advance on
    /// outflow: export, cross-host transfer-out).
    pub fn add_total_out(&mut self, amount: u64)
        requires
            old(self).invariant(),
            old(self).total_out <= u64::MAX - amount,
        ensures
            final(self).invariant(),
            final(self).total_out == old(self).total_out + amount,
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
        self.total_out = self.total_out + amount;
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
        }
    }


    /// Read total_in.
    pub fn read_total_in(&self) -> (v: u64)
        requires self.invariant(),
        ensures v == self.total_in,
    { self.total_in }


    /// Read total_out.
    pub fn read_total_out(&self) -> (v: u64)
        requires self.invariant(),
        ensures v == self.total_out,
    { self.total_out }


    /// Read paid_ring_membership.
    pub fn read_paid_ring_membership(&self) -> (v: u64)
        requires self.invariant(),
        ensures v == self.paid_ring_membership,
    { self.paid_ring_membership }


    /// Allocate a fresh chain-extrinsic ID and bump the allocator.
    /// Quint `nextExtrinsicId`. Called by chain-bound op submission
    /// to identify the corresponding chain extrinsic for receipt
    /// matching.
    pub fn alloc_extrinsic_id(&mut self) -> (id: u64)
        requires
            old(self).invariant(),
            old(self).next_extrinsic_id < u64::MAX,
        ensures
            final(self).invariant(),
            id == old(self).next_extrinsic_id,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id + 1,
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
        let ghost old_coins_vec = self.coins@;
        let ghost old_spec_coins = self.spec_coins@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_spec_entries = self.spec_entries@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_spec_operations = self.spec_operations@;
        let id = self.next_extrinsic_id;
        self.next_extrinsic_id = id + 1;
        proof {
            assert(self.purses@ == old_purses_vec);
            assert(self.spec_purses@ == old_spec_purses);
            assert(self.coins@ == old_coins_vec);
            assert(self.spec_coins@ == old_spec_coins);
            assert(self.entries@ == old_entries_vec);
            assert(self.spec_entries@ == old_spec_entries);
            assert(self.operations@ == old_operations_vec);
            assert(self.spec_operations@ == old_spec_operations);
        }
        id
    }


    /// Synchronous read of `next_extrinsic_id` (the next allocator value).
    pub fn read_next_extrinsic_id(&self) -> (id: u64)
        requires
            self.invariant(),
        ensures
            id == self.next_extrinsic_id,
    {
        self.next_extrinsic_id
    }

}

} // verus!
