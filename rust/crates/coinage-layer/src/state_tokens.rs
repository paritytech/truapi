//! Unload-token mint / consume / count.

use vstd::prelude::*;

use crate::*;

verus! {

impl State {
    /// Mint a new unload token (chain emit). Pushed to the tokens
    /// Vec with `consumed: false`. Quint analog: any `tokens' =
    /// tokens.put(...)` in a chain-mint step.
    pub fn mint_token(&mut self, period: u64, class: UnloadTokenClass, counter: u64)
        -> (idx: usize)
        requires
            old(self).invariant(),
            old(self).tokens@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            idx == old(self).tokens@.len(),
            final(self).tokens@.len() == old(self).tokens@.len() + 1,
            final(self).tokens@[idx as int] == (UnloadToken {
                period, class, counter, consumed: false,
            }),
            forall|i: int| 0 <= i < old(self).tokens@.len() ==>
                #[trigger] final(self).tokens@[i] == old(self).tokens@[i],
            // Everything else untouched.
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
        let idx = self.tokens.len();
        self.tokens.push(UnloadToken { period, class, counter, consumed: false });
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
        idx
    }


    /// Consume an unload token (mark consumed). Idempotent against
    /// already-consumed tokens (silently no-op). Quint analog: the
    /// chain side flipping the `consumed` flag.
    pub fn consume_token(&mut self, idx: usize) -> (res: Result<(), Error>)
        requires
            old(self).invariant(),
        ensures
            final(self).invariant(),
            match res {
                Ok(()) =>
                    idx < old(self).tokens@.len()
                    && !old(self).tokens@[idx as int].consumed
                    && final(self).tokens@.len() == old(self).tokens@.len()
                    && final(self).tokens@[idx as int].consumed
                    && forall|i: int| 0 <= i < old(self).tokens@.len() && i != idx as int
                        ==> #[trigger] final(self).tokens@[i] == old(self).tokens@[i],
                Err(_) =>
                    (idx >= old(self).tokens@.len()
                     || old(self).tokens@[idx as int].consumed)
                    && final(self).tokens@ == old(self).tokens@,
            },
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
        if idx >= self.tokens.len() {
            return Err(Error::Internal(Vec::new()));
        }
        if self.tokens[idx].consumed {
            return Err(Error::Internal(Vec::new()));
        }
        self.tokens[idx].consumed = true;
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
        Ok(())
    }


    /// Number of unload tokens minted.
    pub fn token_count(&self) -> (n: usize)
        requires self.invariant(),
        ensures n == self.tokens@.len(),
    {
        self.tokens.len()
    }

}

} // verus!
