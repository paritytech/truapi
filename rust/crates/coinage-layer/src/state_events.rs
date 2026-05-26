//! Event emission and event-count readers.

use vstd::prelude::*;

use crate::*;

verus! {

impl State {
    /// Append an event to the layer event stream. Quint analog: any
    /// `events' = events.append(e)` clause. Callers compose this with
    /// state-mutating ops to declare emissions (note: the existing
    /// mutators don't emit yet — this is the primitive on which to
    /// build event-emitting wrappers).
    pub fn emit_event(&mut self, e: Event)
        requires
            old(self).invariant(),
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).events@ == old(self).events@.push(e),
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
        let ghost old_tokens = self.tokens@;
        let ghost old_chain_coins = self.chain_coins@;
        let ghost old_chain_entries = self.chain_entries@;
        self.events.push(e);
        proof {
            assert(self.purses@ == old_purses_vec);
            assert(self.spec_purses@ == old_spec_purses);
            assert(self.coins@ == old_coins_vec);
            assert(self.spec_coins@ == old_spec_coins);
            assert(self.entries@ == old_entries_vec);
            assert(self.spec_entries@ == old_spec_entries);
            assert(self.operations@ == old_operations_vec);
            assert(self.spec_operations@ == old_spec_operations);
            assert(self.tokens@ == old_tokens);
            assert(self.chain_coins@ == old_chain_coins);
            assert(self.chain_entries@ == old_chain_entries);
        }
    }


    /// Number of events emitted so far. Quint `events.length()`.
    pub fn event_count(&self) -> (n: usize)
        requires
            self.invariant(),
        ensures
            n == self.events@.len(),
    {
        self.events.len()
    }

}

} // verus!
