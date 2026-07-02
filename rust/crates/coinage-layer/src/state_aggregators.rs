//! Aggregators: count, sum, total, lock-count, in-flight helpers.

use vstd::prelude::*;

use crate::*;

verus! {

impl State {
    /// Count of coins currently `LockedFor(handle)` across the whole
    /// state. Useful for diagnostics ("how much is reserved by this
    /// in-flight op?") and for callers driving bulk-sweep loops
    /// host-side.
    pub fn coin_count_for_handle(&self, handle: OpHandle) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count as nat == count_coin_locks_in_vec(self.coins@, handle, self.coins@.len() as nat),
            count <= self.coins@.len(),
    {
        let mut c: usize = 0;
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                c <= j,
                self.invariant(),
                c as nat == count_coin_locks_in_vec(self.coins@, handle, j as nat),
            decreases self.coins.len() - j,
        {
            let is_locked_for = match self.coins[j].state {
                CoinState::LockedFor(h) => h == handle,
                _ => false,
            };
            if is_locked_for {
                c = c + 1;
            }
            j = j + 1;
        }
        c
    }


    /// Count of entries currently `LocalLockedFor(handle)` across the
    /// whole state. Mirror of `coin_count_for_handle` for the entry
    /// side.
    pub fn entry_count_for_handle(&self, handle: OpHandle) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count as nat == count_entry_locks_in_vec(self.entries@, handle, self.entries@.len() as nat),
            count <= self.entries@.len(),
    {
        let mut c: usize = 0;
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                c <= j,
                self.invariant(),
                c as nat == count_entry_locks_in_vec(self.entries@, handle, j as nat),
            decreases self.entries.len() - j,
        {
            let is_locked_for = match self.entries[j].local {
                EntryLocal::LocalLockedFor(h) => h == handle,
                _ => false,
            };
            if is_locked_for {
                c = c + 1;
            }
            j = j + 1;
        }
        c
    }


    /// Count of operations currently in-flight (non-terminal status).
    pub fn op_count_in_flight(&self) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count <= self.operations@.len(),
    {
        let mut c: usize = 0;
        let mut j: usize = 0;
        while j < self.operations.len()
            invariant
                0 <= j <= self.operations.len(),
                c <= j,
                self.invariant(),
            decreases self.operations.len() - j,
        {
            let op = &self.operations[j];
            let is_terminal = match op.status {
                OpStatus::Done => true,
                OpStatus::Failed => true,
                _ => false,
            };
            if !is_terminal {
                c = c + 1;
            }
            j = j + 1;
        }
        c
    }


    /// Count of all coins (any state) in purse `p`. Useful diagnostic
    /// for "how cluttered is this purse?". Distinguish from
    /// coin_count_available which excludes locked/spent/pending.
    pub fn coin_count_in_purse(&self, p: PurseId) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count <= self.coins@.len(),
    {
        let mut c: usize = 0;
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                c <= j,
                self.invariant(),
            decreases self.coins.len() - j,
        {
            if self.coins[j].purse == p {
                c = c + 1;
            }
            j = j + 1;
        }
        c
    }


    /// Count of all entries (any state) in purse `p`. Entry parallel
    /// of `coin_count_in_purse`.
    pub fn entry_count_in_purse(&self, p: PurseId) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count <= self.entries@.len(),
    {
        let mut c: usize = 0;
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                c <= j,
                self.invariant(),
            decreases self.entries.len() - j,
        {
            if self.entries[j].purse == p {
                c = c + 1;
            }
            j = j + 1;
        }
        c
    }


    /// Count of `Available` coins in purse `p`. Used by maintenance
    /// triggers — e.g. "if coin_count_available(p) > threshold, run
    /// rebalance to consolidate into fewer larger coins".
    pub fn coin_count_available(&self, p: PurseId) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count <= self.coins@.len(),
    {
        let mut c: usize = 0;
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                c <= j,
                self.invariant(),
            decreases self.coins.len() - j,
        {
            let is_avail = matches!(self.coins[j].state, CoinState::Available);
            if self.coins[j].purse == p && is_avail {
                c = c + 1;
            }
            j = j + 1;
        }
        c
    }


    /// Count of selectable entries (Ready + LocalAvailable) in purse
    /// `p`. Used by maintenance triggers and §6.3 selection feasibility
    /// checks.
    pub fn entry_count_selectable(&self, p: PurseId) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count <= self.entries@.len(),
    {
        let mut c: usize = 0;
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                c <= j,
                self.invariant(),
            decreases self.entries.len() - j,
        {
            let e = &self.entries[j];
            let is_ready = matches!(e.on_chain, EntryOnChain::Ready);
            let is_local_avail = matches!(e.local, EntryLocal::LocalAvailable);
            if e.purse == p && is_ready && is_local_avail {
                c = c + 1;
            }
            j = j + 1;
        }
        c
    }


    /// Number of purses in the state.
    pub fn total_purses(&self) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count == self.purses@.len(),
    {
        self.purses.len()
    }


    /// Number of coins (across all states and purses) in the state.
    pub fn total_coins(&self) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count == self.coins@.len(),
    {
        self.coins.len()
    }


    /// Number of recycler entries (across all states and purses).
    pub fn total_entries(&self) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count == self.entries@.len(),
    {
        self.entries.len()
    }


    /// Number of operations (terminal or in-flight) in the state.
    pub fn total_operations(&self) -> (count: usize)
        requires
            self.invariant(),
        ensures
            count == self.operations@.len(),
    {
        self.operations.len()
    }


    /// Sum of `coin_value(exp)` across entries in purse `p` that are
    /// LocalAvailable and Ready on-chain. Quint analog: the entry
    /// component of `purseSpendableStrict(p)`.
    pub(crate) fn sum_ready_in(&self, p: PurseId) -> (sum: u64)
        requires
            self.invariant(),
            self.entries@.len() <= (u64::MAX / 1073741824) as nat,
        ensures
            sum as nat == sum_ready_prefix(self.entries@, p, self.entries@.len() as nat),
            sum as nat <= self.entries@.len() as nat * 1073741824,
    {
        let mut sum: u64 = 0;
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.entries@.len() <= (u64::MAX / 1073741824) as nat,
                self.invariant(),
                sum as nat == sum_ready_prefix(self.entries@, p, j as nat),
                sum as nat <= (j as nat) * 1073741824,
            decreases self.entries.len() - j,
        {
            let e = &self.entries[j];
            let is_local_avail = matches!(e.local, EntryLocal::LocalAvailable);
            let is_ready = matches!(e.on_chain, EntryOnChain::Ready);
            proof {
                let entry_key = (self.entries@[j as int].purse,
                                 self.entries@[j as int].idx);
                assert(self.spec_entries@.dom().contains(entry_key));
                assert(self.spec_entries@[entry_key] == self.entries@[j as int]);
                assert(self.entries@[j as int].exponent <= MAX_EXPONENT);
                lemma_pow2_at_30();
                lemma_pow2_monotone(self.entries@[j as int].exponent as nat,
                                    MAX_EXPONENT as nat);
                assert(sum_ready_prefix(self.entries@, p, (j + 1) as nat)
                    <= sum_ready_prefix(self.entries@, p, j as nat) + 1073741824);
            }
            if e.purse == p && is_local_avail && is_ready {
                let value: u64 = pow2_u64_exec(e.exponent);
                sum = sum + value;
            }
            j = j + 1;
        }
        sum
    }


    /// Sum of `coin_value(exp)` across entries in purse `p` that are
    /// LocalAvailable and on-chain in {Waiting, Missing} — i.e. pending
    /// recycler-floor confirmation. Quint analog: `pursePending(p)`.
    pub(crate) fn sum_pending_in(&self, p: PurseId) -> (sum: u64)
        requires
            self.invariant(),
            self.entries@.len() <= (u64::MAX / 1073741824) as nat,
        ensures
            sum as nat == sum_pending_prefix(self.entries@, p, self.entries@.len() as nat),
    {
        let mut sum: u64 = 0;
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.entries@.len() <= (u64::MAX / 1073741824) as nat,
                self.invariant(),
                sum as nat == sum_pending_prefix(self.entries@, p, j as nat),
                sum as nat <= (j as nat) * 1073741824,
            decreases self.entries.len() - j,
        {
            let e = &self.entries[j];
            let is_local_avail = matches!(e.local, EntryLocal::LocalAvailable);
            let is_waiting = matches!(e.on_chain, EntryOnChain::Waiting);
            let is_missing = matches!(e.on_chain, EntryOnChain::Missing);
            proof {
                let entry_key = (self.entries@[j as int].purse,
                                 self.entries@[j as int].idx);
                assert(self.spec_entries@.dom().contains(entry_key));
                assert(self.spec_entries@[entry_key] == self.entries@[j as int]);
                assert(self.entries@[j as int].exponent <= MAX_EXPONENT);
                lemma_pow2_at_30();
                lemma_pow2_monotone(self.entries@[j as int].exponent as nat,
                                    MAX_EXPONENT as nat);
                assert(sum_pending_prefix(self.entries@, p, (j + 1) as nat)
                    <= sum_pending_prefix(self.entries@, p, j as nat) + 1073741824);
            }
            if e.purse == p && is_local_avail && (is_waiting || is_missing) {
                let value: u64 = pow2_u64_exec(e.exponent);
                sum = sum + value;
            }
            j = j + 1;
        }
        sum
    }


    /// Real-value (2^exp) variant of `sum_pending_in`. Used by callers
    /// that want production-scheme purse-pending totals.
    pub fn sum_pending_real_in(&self, p: PurseId) -> (sum: u64)
        requires
            self.invariant(),
            forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                ==> self.entries()[k].exponent <= MAX_EXPONENT,
            self.entries@.len() <= (u64::MAX / 1073741824) as nat,
        ensures
            sum as nat == sum_pending_real_prefix(self.entries@, p,
                                                  self.entries@.len() as nat),
            sum as nat <= self.entries@.len() as nat * 1073741824,
    {
        let mut sum: u64 = 0;
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.entries@.len() <= (u64::MAX / 1073741824) as nat,
                sum as nat == sum_pending_real_prefix(self.entries@, p, j as nat),
                sum as nat <= (j as nat) * 1073741824,
                forall|k: (PurseId, u64)|
                    #[trigger] self.entries().dom().contains(k)
                    ==> self.entries()[k].exponent <= MAX_EXPONENT,
                self.invariant(),
            decreases self.entries.len() - j,
        {
            let e = &self.entries[j];
            let is_local_avail = matches!(e.local, EntryLocal::LocalAvailable);
            let is_waiting = matches!(e.on_chain, EntryOnChain::Waiting);
            let is_missing = matches!(e.on_chain, EntryOnChain::Missing);
            proof {
                let entry_key = (self.entries@[j as int].purse,
                                 self.entries@[j as int].idx);
                assert(self.spec_entries@.dom().contains(entry_key));
                assert(self.entries()[entry_key] == self.entries@[j as int]);
                assert(self.entries()[entry_key].exponent <= MAX_EXPONENT);
                assert(self.entries@[j as int].exponent <= MAX_EXPONENT);
                lemma_pow2_at_30();
                lemma_pow2_monotone(self.entries@[j as int].exponent as nat, 30);
                assert(sum_pending_real_prefix(self.entries@, p, (j + 1) as nat)
                    <= sum_pending_real_prefix(self.entries@, p, j as nat) + 1073741824);
            }
            if e.purse == p && is_local_avail && (is_waiting || is_missing) {
                let value = pow2_u64_exec(e.exponent);
                sum = sum + value;
            }
            j = j + 1;
        }
        sum
    }


    /// Real-value (2^exp) variant of `sum_ready_in`.
    pub fn sum_ready_real_in(&self, p: PurseId) -> (sum: u64)
        requires
            self.invariant(),
            forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                ==> self.entries()[k].exponent <= MAX_EXPONENT,
            self.entries@.len() <= (u64::MAX / 1073741824) as nat,
        ensures
            sum as nat == sum_ready_real_prefix(self.entries@, p,
                                                self.entries@.len() as nat),
            sum as nat <= self.entries@.len() as nat * 1073741824,
    {
        let mut sum: u64 = 0;
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.entries@.len() <= (u64::MAX / 1073741824) as nat,
                sum as nat == sum_ready_real_prefix(self.entries@, p, j as nat),
                sum as nat <= (j as nat) * 1073741824,
                forall|k: (PurseId, u64)|
                    #[trigger] self.entries().dom().contains(k)
                    ==> self.entries()[k].exponent <= MAX_EXPONENT,
                self.invariant(),
            decreases self.entries.len() - j,
        {
            let e = &self.entries[j];
            let is_local_avail = matches!(e.local, EntryLocal::LocalAvailable);
            let is_ready = matches!(e.on_chain, EntryOnChain::Ready);
            proof {
                let entry_key = (self.entries@[j as int].purse,
                                 self.entries@[j as int].idx);
                assert(self.spec_entries@.dom().contains(entry_key));
                assert(self.entries()[entry_key] == self.entries@[j as int]);
                assert(self.entries()[entry_key].exponent <= MAX_EXPONENT);
                assert(self.entries@[j as int].exponent <= MAX_EXPONENT);
                lemma_pow2_at_30();
                lemma_pow2_monotone(self.entries@[j as int].exponent as nat, 30);
                assert(sum_ready_real_prefix(self.entries@, p, (j + 1) as nat)
                    <= sum_ready_real_prefix(self.entries@, p, j as nat) + 1073741824);
            }
            if e.purse == p && is_local_avail && is_ready {
                let value = pow2_u64_exec(e.exponent);
                sum = sum + value;
            }
            j = j + 1;
        }
        sum
    }


    /// Sum of **real** `coin_value_pow2(exp) = 2^exp` across `Available`
    /// coins in purse `p`. Companion to `sum_available_in` (pilot scheme).
    /// Returned sum equals `sum_avail_real_prefix(self.coins@, p, len)`.
    ///
    /// Preconditions:
    /// - Every coin in the state has `exponent <= MAX_EXPONENT` (= 30),
    ///   so each coin value <= 2^30.
    /// - Vec length bounded so the cumulative u64 sum (≤ len · 2^30)
    ///   stays within u64::MAX.
    pub fn sum_available_real_in(&self, p: PurseId) -> (sum: u64)
        requires
            self.invariant(),
            forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                ==> self.coins()[k].exponent <= MAX_EXPONENT,
            self.coins@.len() <= (u64::MAX / 1073741824) as nat,
        ensures
            sum as nat == sum_avail_real_prefix(self.coins@, p, self.coins@.len() as nat),
            sum as nat <= self.coins@.len() as nat * 1073741824,
    {
        let mut sum: u64 = 0;
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.coins@.len() <= (u64::MAX / 1073741824) as nat,
                sum as nat == sum_avail_real_prefix(self.coins@, p, j as nat),
                sum as nat <= (j as nat) * 1073741824,
                forall|k: (PurseId, u64)|
                    #[trigger] self.coins().dom().contains(k)
                    ==> self.coins()[k].exponent <= MAX_EXPONENT,
                self.invariant(),
            decreases self.coins.len() - j,
        {
            let is_available = matches!(self.coins[j].state, CoinState::Available);
            proof {
                // Per-step increment is at most 2^30, bounded by the
                // global exponent constraint via invariant (l).
                assert(self.spec_coins@.dom().contains(
                    (self.coins@[j as int].purse, self.coins@[j as int].idx)
                ));
                let coin_key = (self.coins@[j as int].purse, self.coins@[j as int].idx);
                assert(self.coins()[coin_key].exponent
                    == self.coins@[j as int].exponent);
                assert(self.coins()[coin_key].exponent <= MAX_EXPONENT);
                assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
                lemma_pow2_at_30();
                lemma_pow2_monotone(self.coins@[j as int].exponent as nat, 30);
                assert(sum_avail_real_prefix(self.coins@, p, (j + 1) as nat)
                    <= sum_avail_real_prefix(self.coins@, p, j as nat) + 1073741824);
            }
            if self.coins[j].purse == p && is_available {
                let value: u64 = pow2_u64_exec(self.coins[j].exponent);
                sum = sum + value;
            }
            j = j + 1;
        }
        sum
    }


    /// Sum of `coin_value(exp)` across `Available` coins in purse `p`.
    /// Scans the coin Vec; returned sum equals `sum_avail_prefix(self.coins@,
    /// p, len)`.
    ///
    /// **Pilot value scheme:** `coin_value(exp) = exp + 1` (linear). Real
    /// `coinValue(exp) = 2^exp` is deferred. Precondition bounds Vec size to
    /// keep the cumulative `u64` sum safe.
    pub(crate) fn sum_available_in(&self, p: PurseId) -> (sum: u64)
        requires
            self.invariant(),
            // With coin_value(exp) <= 2^30, sum is bounded by len * 2^30.
            // Bound Vec length to ensure no u64 overflow.
            self.coins@.len() <= (u64::MAX / 1073741824) as nat,
        ensures
            sum as nat == sum_avail_prefix(self.coins@, p, self.coins@.len() as nat),
            sum as nat <= self.coins@.len() as nat * 1073741824,
    {
        let mut sum: u64 = 0;
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.coins@.len() <= (u64::MAX / 1073741824) as nat,
                self.invariant(),
                sum as nat == sum_avail_prefix(self.coins@, p, j as nat),
                sum as nat <= (j as nat) * 1073741824,
            decreases self.coins.len() - j,
        {
            let is_available = matches!(self.coins[j].state, CoinState::Available);
            proof {
                let coin_key = (self.coins@[j as int].purse, self.coins@[j as int].idx);
                assert(self.spec_coins@.dom().contains(coin_key));
                assert(self.spec_coins@[coin_key] == self.coins@[j as int]);
                assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
                lemma_pow2_at_30();
                lemma_pow2_monotone(self.coins@[j as int].exponent as nat,
                                    MAX_EXPONENT as nat);
                // Per-step increment is at most coin_value(_) <= 2^30, so the
                // monotone bound `sum_avail_prefix(_, _, j+1) <= (j+1) * 2^30`
                // is preserved.
                assert(sum_avail_prefix(self.coins@, p, (j + 1) as nat)
                    <= sum_avail_prefix(self.coins@, p, j as nat) + 1073741824);
            }
            if self.coins[j].purse == p && is_available {
                let value: u64 = pow2_u64_exec(self.coins[j].exponent);
                sum = sum + value;
            }
            j = j + 1;
        }
        sum
    }

}

} // verus!
