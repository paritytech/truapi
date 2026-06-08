//! Fee-account top-up / deduct / read / `FeeMode` selection.

use vstd::prelude::*;

use crate::*;

verus! {

impl State {
    /// Top up the fee-account reservoir. Quint `topUpFeeAccount`.
    pub fn top_up_fee_account(&mut self, amount: u64)
        requires
            old(self).invariant(),
            old(self).fee_balance <= u64::MAX - amount,
        ensures
            final(self).invariant(),
            final(self).fee_balance == old(self).fee_balance + amount,
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
        let ghost old_coins_vec = self.coins@;
        let ghost old_spec_coins = self.spec_coins@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_spec_entries = self.spec_entries@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_spec_operations = self.spec_operations@;
        self.fee_balance = self.fee_balance + amount;
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
    }


    /// Spend from the fee-account reservoir.
    pub fn deduct_fee(&mut self, amount: u64) -> (res: Result<(), Error>)
        requires
            old(self).invariant(),
        ensures
            final(self).invariant(),
            match res {
                Ok(()) =>
                    old(self).fee_balance >= amount
                    && final(self).fee_balance == old(self).fee_balance - amount,
                Err(Error::InsufficientFunds { requested, available }) =>
                    old(self).fee_balance < amount
                    && requested == amount
                    && available == old(self).fee_balance
                    && final(self).fee_balance == old(self).fee_balance,
                Err(_) => false,
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
        let ghost old_coins_vec = self.coins@;
        let ghost old_spec_coins = self.spec_coins@;
        let ghost old_entries_vec = self.entries@;
        let ghost old_spec_entries = self.spec_entries@;
        let ghost old_operations_vec = self.operations@;
        let ghost old_spec_operations = self.spec_operations@;
        let res = if self.fee_balance >= amount {
            self.fee_balance = self.fee_balance - amount;
            Ok(())
        } else {
            Err(Error::InsufficientFunds {
                requested: amount,
                available: self.fee_balance,
            })
        };
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
        res
    }


    /// Synchronous read of the fee-account balance.
    pub fn read_fee_balance(&self) -> (b: u64)
        requires
            self.invariant(),
        ensures
            b == self.fee_balance,
    {
        self.fee_balance
    }


    /// Auto-pick a `FeeMode` based on the current reservoir.
    pub fn select_fee_mode(&self, fee: u64) -> (mode: FeeMode)
        requires
            self.invariant(),
        ensures
            match mode {
                FeeMode::Prepaid => self.fee_balance >= fee,
                FeeMode::FromOutput => self.fee_balance < fee,
            },
    {
        if self.fee_balance >= fee {
            FeeMode::Prepaid
        } else {
            FeeMode::FromOutput
        }
    }

}

} // verus!
