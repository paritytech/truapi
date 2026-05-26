//! Read-only queries: `*_record`, `query_*`, `op_meta`, `op_status`, `coin_state`, `entry_*_state`, has-* checks.

use vstd::prelude::*;

use crate::*;

verus! {

impl State {
    /// Exec witness for the [`Self::has_live_coin_in`] spec predicate:
    /// `true` iff at least one coin in purse `p` is in any non-`Spent`
    /// state. Pair with [`Self::has_in_flight_op_for_purse`] before
    /// `delete_purse` to surface "purse not empty" as an early bail
    /// instead of a precondition trap.
    pub fn check_has_live_coin_in(&self, p: PurseId) -> (res: bool)
        requires
            self.invariant(),
        ensures
            res == self.has_live_coin_in(p),
    {
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).purse != p
                    || self.coins@[jj].state == CoinState::Spent,
            decreases self.coins.len() - j,
        {
            let c = &self.coins[j];
            let is_spent = matches!(c.state, CoinState::Spent);
            if c.purse == p && !is_spent {
                #[allow(unused_variables)]
                let key = (c.purse, c.idx);
                proof {
                    assert(self.spec_coins@.dom().contains(key));
                    assert(self.coins()[key].state == self.coins@[j as int].state);
                }
                return true;
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                && k.0 == p
                implies self.coins()[k].state == CoinState::Spent
            by {
                let w = choose|jj: int|
                    0 <= jj < self.coins@.len()
                    && #[trigger] self.coins@[jj].purse == k.0
                    && self.coins@[jj].idx == k.1;
                assert(self.coins@[w].purse == p);
                assert(self.coins@[w].state == self.coins()[k].state);
            }
        }
        false
    }


    /// Read the **real** entry value for `key` (Quint `coinValue` over
    /// the entry's exponent). Entry parallel of
    /// [`Self::read_coin_value_real`].
    pub fn read_entry_value_real(&self, key: (PurseId, u64)) -> (res: Option<u64>)
        requires
            self.invariant(),
            forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                ==> self.entries()[k].exponent <= MAX_EXPONENT,
        ensures
            match res {
                Some(v) =>
                    self.entries().dom().contains(key)
                    && v as nat == coin_value_pow2(self.entries()[key].exponent),
                None => !self.entries().dom().contains(key),
            },
    {
        match self.entry_record(key) {
            Some(e) => {
                proof {
                    assert(self.entries()[key].exponent <= MAX_EXPONENT);
                    assert(e.exponent == self.entries()[key].exponent);
                }
                Some(pow2_u64_exec(e.exponent))
            }
            None => None,
        }
    }


    /// Read the **real** coin value for `key` using `2^exp` arithmetic
    /// (Quint `coinValue`). Requires the coin's exponent to satisfy the
    /// `MAX_EXPONENT` bound. Returns `None` if no such coin exists.
    ///
    /// Companion to the pilot-scheme aggregations (which use
    /// `coin_value(exp) = exp + 1`) — this one reflects the production
    /// scheme. Callers wiring up the real arithmetic switch can compose
    /// this with their own sums; the existing per-purse aggregations
    /// (sum_available_in etc.) still use the pilot scheme.
    pub fn read_coin_value_real(&self, key: (PurseId, u64)) -> (res: Option<u64>)
        requires
            self.invariant(),
            forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                ==> self.coins()[k].exponent <= MAX_EXPONENT,
        ensures
            match res {
                Some(v) =>
                    self.coins().dom().contains(key)
                    && v as nat == coin_value_pow2(self.coins()[key].exponent),
                None => !self.coins().dom().contains(key),
            },
    {
        match self.coin_record(key) {
            Some(c) => {
                proof {
                    assert(self.coins()[key].exponent <= MAX_EXPONENT);
                    assert(c.exponent == self.coins()[key].exponent);
                }
                Some(pow2_u64_exec(c.exponent))
            }
            None => None,
        }
    }


    /// Synchronous read: state of the coin keyed `key`, or `None` if
    /// no such coin exists. Quint analog: `coins.get(key).state`.
    pub fn coin_state(&self, key: (PurseId, u64)) -> (res: Option<CoinState>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(s) =>
                    self.coins().dom().contains(key)
                    && s == self.coins()[key].state,
                None => !self.coins().dom().contains(key),
            },
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
                    assert(self.spec_coins@.dom().contains(key));
                }
                return Some(self.coins[j].state);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                implies k != key
            by {
                let w = choose|jj: int|
                    0 <= jj < self.coins@.len()
                    && #[trigger] self.coins@[jj].purse == k.0
                    && self.coins@[jj].idx == k.1;
                assert(self.coins@[w].purse == k.0);
            }
        }
        None
    }


    /// Synchronous read: local state of the entry keyed `key`, or
    /// `None` if no such entry exists. Quint analog:
    /// `entries.get(key).local`.
    pub fn entry_local_state(&self, key: (PurseId, u64))
        -> (res: Option<EntryLocal>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(s) =>
                    self.entries().dom().contains(key)
                    && s == self.entries()[key].local,
                None => !self.entries().dom().contains(key),
            },
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
                    assert(self.spec_entries@.dom().contains(key));
                }
                return Some(self.entries[j].local);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                implies k != key
            by {
                let w = choose|jj: int|
                    0 <= jj < self.entries@.len()
                    && #[trigger] self.entries@[jj].purse == k.0
                    && self.entries@[jj].idx == k.1;
                assert(self.entries@[w].purse == k.0);
            }
        }
        None
    }


    /// Synchronous read: on-chain state of the entry keyed `key`, or
    /// `None` if no such entry exists. Quint analog:
    /// `entries.get(key).onChain`.
    pub fn entry_on_chain_state(&self, key: (PurseId, u64))
        -> (res: Option<EntryOnChain>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(s) =>
                    self.entries().dom().contains(key)
                    && s == self.entries()[key].on_chain,
                None => !self.entries().dom().contains(key),
            },
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
                    assert(self.spec_entries@.dom().contains(key));
                }
                return Some(self.entries[j].on_chain);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                implies k != key
            by {
                let w = choose|jj: int|
                    0 <= jj < self.entries@.len()
                    && #[trigger] self.entries@[jj].purse == k.0
                    && self.entries@[jj].idx == k.1;
                assert(self.entries@[w].purse == k.0);
            }
        }
        None
    }


    /// Synchronous read: the full `CoinRec` for `key`, or `None` if the
    /// coin doesn't exist. Avoids repeated per-field lookup calls.
    pub fn coin_record(&self, key: (PurseId, u64)) -> (res: Option<CoinRec>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(c) =>
                    self.coins().dom().contains(key)
                    && c == self.coins()[key],
                None => !self.coins().dom().contains(key),
            },
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
                    assert(self.spec_coins@.dom().contains(key));
                }
                return Some(self.coins[j]);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                implies k != key
            by {
                let w = choose|jj: int|
                    0 <= jj < self.coins@.len()
                    && #[trigger] self.coins@[jj].purse == k.0
                    && self.coins@[jj].idx == k.1;
                assert(self.coins@[w].purse == k.0);
            }
        }
        None
    }


    /// Synchronous read: the full `EntryRec` for `key`, or `None` if
    /// the entry doesn't exist.
    pub fn entry_record(&self, key: (PurseId, u64)) -> (res: Option<EntryRec>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(e) =>
                    self.entries().dom().contains(key)
                    && e == self.entries()[key],
                None => !self.entries().dom().contains(key),
            },
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
                    assert(self.spec_entries@.dom().contains(key));
                }
                return Some(self.entries[j]);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                implies k != key
            by {
                let w = choose|jj: int|
                    0 <= jj < self.entries@.len()
                    && #[trigger] self.entries@[jj].purse == k.0
                    && self.entries@[jj].idx == k.1;
                assert(self.entries@[w].purse == k.0);
            }
        }
        None
    }


    /// Result-returning variant of `op_status`. Returns
    /// `Err(OperationNotFound(handle))` when the op handle is unknown
    /// — the surface a host's RPC layer typically needs.
    pub fn query_op_status(&self, handle: OpHandle) -> (res: Result<OpStatus, Error>)
        requires
            self.invariant(),
        ensures
            match res {
                Ok(s) =>
                    self.operations().dom().contains(handle)
                    && s == self.operations()[handle].status,
                Err(Error::OperationNotFound(h)) =>
                    !self.operations().dom().contains(handle) && h == handle,
                Err(_) => false,
            },
    {
        match self.op_status(handle) {
            Some(s) => Ok(s),
            None => Err(Error::OperationNotFound(handle)),
        }
    }


    /// Result-returning variant of `coin_record`. Errors with
    /// `Internal` when the coin doesn't exist (callers that want a
    /// distinguishing error variant should match on `None` from
    /// `coin_record` directly).
    pub fn query_coin_record(&self, key: (PurseId, u64))
        -> (res: Result<CoinRec, Error>)
        requires
            self.invariant(),
        ensures
            match res {
                Ok(c) =>
                    self.coins().dom().contains(key)
                    && c == self.coins()[key],
                Err(_) => !self.coins().dom().contains(key),
            },
    {
        match self.coin_record(key) {
            Some(c) => Ok(c),
            None => Err(Error::Internal(Vec::new())),
        }
    }


    /// Result-returning variant of `entry_record`.
    pub fn query_entry_record(&self, key: (PurseId, u64))
        -> (res: Result<EntryRec, Error>)
        requires
            self.invariant(),
        ensures
            match res {
                Ok(e) =>
                    self.entries().dom().contains(key)
                    && e == self.entries()[key],
                Err(_) => !self.entries().dom().contains(key),
            },
    {
        match self.entry_record(key) {
            Some(e) => Ok(e),
            None => Err(Error::Internal(Vec::new())),
        }
    }


    /// Check: does any *non-terminal* operation target purse `p`?
    /// Returns `true` iff at least one operation has `purse == p` and a
    /// status in {Preparing, Submitted, InBlock, Finalized, Waiting(_)}.
    /// Useful for delete-purse readiness checks where terminal ops can
    /// be ignored.
    pub fn has_in_flight_op_for_purse(&self, p: PurseId) -> (res: bool)
        requires
            self.invariant(),
        ensures
            res == exists|h: OpHandle|
                #[trigger] self.operations().dom().contains(h)
                && self.operations()[h].purse == p
                && !is_terminal_op_status(self.operations()[h].status),
    {
        let mut j: usize = 0;
        while j < self.operations.len()
            invariant
                0 <= j <= self.operations.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.operations@[jj]).purse != p
                    || is_terminal_op_status(self.operations@[jj].status),
            decreases self.operations.len() - j,
        {
            let op = &self.operations[j];
            let is_terminal = match op.status {
                OpStatus::Done => true,
                OpStatus::Failed => true,
                _ => false,
            };
            if op.purse == p && !is_terminal {
                #[allow(unused_variables)]
                let h = op.handle;
                proof {
                    assert(self.spec_operations@.dom().contains(h));
                    assert(self.operations()[h].purse == p);
                    assert(!is_terminal_op_status(self.operations()[h].status));
                }
                return true;
            }
            j = j + 1;
        }
        proof {
            assert forall|h: OpHandle|
                #[trigger] self.operations().dom().contains(h)
                && self.operations()[h].purse == p
                implies is_terminal_op_status(self.operations()[h].status)
            by {
                let w = choose|jj: int|
                    0 <= jj < self.operations@.len()
                    && #[trigger] self.operations@[jj].handle == h;
                assert(self.operations@[w].handle == h);
            }
        }
        false
    }


    /// Check: does any operation target purse `p`? Returns `true` iff
    /// at least one operation has `op.purse == p`. Useful as a pre-flight
    /// guard before `delete_purse`, which requires no targeting ops.
    pub fn has_op_targeting_purse(&self, p: PurseId) -> (res: bool)
        requires
            self.invariant(),
        ensures
            res == exists|h: OpHandle|
                #[trigger] self.operations().dom().contains(h)
                && self.operations()[h].purse == p,
    {
        let mut j: usize = 0;
        while j < self.operations.len()
            invariant
                0 <= j <= self.operations.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.operations@[jj]).purse != p,
            decreases self.operations.len() - j,
        {
            if self.operations[j].purse == p {
                #[allow(unused_variables)]
                let h = self.operations[j].handle;
                proof {
                    assert(self.spec_operations@.dom().contains(h));
                    assert(self.operations()[h].purse == p);
                }
                return true;
            }
            j = j + 1;
        }
        proof {
            assert forall|h: OpHandle|
                #[trigger] self.operations().dom().contains(h)
                implies self.operations()[h].purse != p
            by {
                let w = choose|jj: int|
                    0 <= jj < self.operations@.len()
                    && #[trigger] self.operations@[jj].handle == h;
                assert(self.operations@[w].handle == h);
            }
        }
        false
    }


    /// Result-returning variant of `op_meta`.
    pub fn query_op_meta(&self, handle: OpHandle)
        -> (res: Result<(OpKind, PurseId), Error>)
        requires
            self.invariant(),
        ensures
            match res {
                Ok((k, p)) =>
                    self.operations().dom().contains(handle)
                    && k == self.operations()[handle].kind
                    && p == self.operations()[handle].purse,
                Err(Error::OperationNotFound(h)) =>
                    !self.operations().dom().contains(handle) && h == handle,
                Err(_) => false,
            },
    {
        match self.op_meta(handle) {
            Some(m) => Ok(m),
            None => Err(Error::OperationNotFound(handle)),
        }
    }


    /// Synchronous read: the `(kind, purse)` pair of the operation
    /// `handle`, or `None` if no such operation exists. Used to route
    /// chain events back to the right purse / op-kind handler.
    pub fn op_meta(&self, handle: OpHandle) -> (res: Option<(OpKind, PurseId)>)
        requires
            self.invariant(),
        ensures
            match res {
                Some((k, p)) =>
                    self.operations().dom().contains(handle)
                    && k == self.operations()[handle].kind
                    && p == self.operations()[handle].purse,
                None => !self.operations().dom().contains(handle),
            },
    {
        let mut j: usize = 0;
        while j < self.operations.len()
            invariant
                0 <= j <= self.operations.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.operations@[jj]).handle != handle,
            decreases self.operations.len() - j,
        {
            if self.operations[j].handle == handle {
                proof {
                    assert(self.spec_operations@.dom().contains(handle));
                }
                return Some((self.operations[j].kind, self.operations[j].purse));
            }
            j = j + 1;
        }
        proof {
            assert forall|h: OpHandle|
                #[trigger] self.operations().dom().contains(h)
                implies h != handle
            by {
                let w = choose|jj: int|
                    0 <= jj < self.operations@.len()
                    && #[trigger] self.operations@[jj].handle == h;
                assert(self.operations@[w].handle == h);
            }
        }
        None
    }


    /// Synchronous read: status of the operation `handle`, or `None`
    /// if no such operation exists. Quint analog: `operations.get(h).status`.
    pub fn op_status(&self, handle: OpHandle) -> (res: Option<OpStatus>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(s) =>
                    self.operations().dom().contains(handle)
                    && s == self.operations()[handle].status,
                None => !self.operations().dom().contains(handle),
            },
    {
        let mut j: usize = 0;
        while j < self.operations.len()
            invariant
                0 <= j <= self.operations.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.operations@[jj]).handle != handle,
            decreases self.operations.len() - j,
        {
            if self.operations[j].handle == handle {
                proof {
                    assert(self.spec_operations@.dom().contains(handle));
                }
                return Some(self.operations[j].status);
            }
            j = j + 1;
        }
        proof {
            assert forall|h: OpHandle|
                #[trigger] self.operations().dom().contains(h)
                implies h != handle
            by {
                let w = choose|jj: int|
                    0 <= jj < self.operations@.len()
                    && #[trigger] self.operations@[jj].handle == h;
                assert(self.operations@[w].handle == h);
            }
        }
        None
    }


    /// Convenience: sum of `Available` coins + ALL LocalAvailable
    /// entries (Ready + Waiting + Missing), using real `2^exp` values.
    /// Quint analog: `spendableWhenReady(p) = purseSpendable(p) +
    /// pursePending(p)`.
    ///
    /// Used to distinguish "insufficient funds now" from "insufficient
    /// even if all in-flight top-ups mature".
    pub fn spendable_when_ready_real(&self, p: PurseId) -> (total: u64)
        requires
            self.invariant(),
            forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                ==> self.coins()[k].exponent <= MAX_EXPONENT,
            forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                ==> self.entries()[k].exponent <= MAX_EXPONENT,
            self.coins@.len() <= (u64::MAX / 1073741824) as nat,
            self.entries@.len() <= (u64::MAX / 1073741824) as nat,
            (self.coins@.len() as nat + self.entries@.len() as nat)
                <= (u64::MAX / 1073741824) as nat,
        ensures
            total as nat ==
                sum_avail_real_prefix(self.coins@, p, self.coins@.len() as nat)
                + sum_pending_real_prefix(self.entries@, p, self.entries@.len() as nat),
    {
        let spendable = self.sum_available_real_in(p);
        let pending = self.sum_pending_real_in(p);
        proof {
            assert(spendable as nat <= self.coins@.len() as nat * 1073741824);
            assert(pending as nat <= self.entries@.len() as nat * 1073741824);
        }
        spendable + pending
    }


    /// Real-value (2^exp) variant of [`Self::query_purse`]. Reports
    /// `spendable`, `spendable_strict`, and `pending` using Quint's
    /// production `coinValue = 2^exp` arithmetic via the
    /// `sum_*_real_in` aggregations. Requires all exponents in state
    /// to satisfy MAX_EXPONENT and the Vec sizes to fit cumulative
    /// u64 sums.
    pub fn query_purse_real(&self, p: PurseId) -> (info: Result<PurseInfo, Error>)
        requires
            self.invariant(),
            forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                ==> self.coins()[k].exponent <= MAX_EXPONENT,
            forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                ==> self.entries()[k].exponent <= MAX_EXPONENT,
            self.coins@.len() <= (u64::MAX / 1073741824) as nat,
            self.entries@.len() <= (u64::MAX / 1073741824) as nat,
            (self.coins@.len() as nat + self.entries@.len() as nat)
                <= (u64::MAX / 1073741824) as nat,
        ensures
            match info {
                Ok(i) =>
                    self.purses().dom().contains(p)
                    && i.id == p
                    && i.name@ == self.purses()[p].name
                    && i.spendable as nat
                        == sum_avail_real_prefix(self.coins@, p, self.coins@.len() as nat)
                    && i.spendable_strict as nat
                        == sum_avail_real_prefix(self.coins@, p, self.coins@.len() as nat)
                            + sum_ready_real_prefix(self.entries@, p,
                                                    self.entries@.len() as nat)
                    && i.pending as nat
                        == sum_pending_real_prefix(self.entries@, p,
                                                   self.entries@.len() as nat),
                Err(Error::PurseNotFound(q)) =>
                    !self.purses().dom().contains(p) && q == p,
                Err(_) => false,
            },
    {
        let mut i: usize = 0;
        while i < self.purses.len()
            invariant
                0 <= i <= self.purses.len(),
                self.invariant(),
                forall|k: (PurseId, u64)|
                    #[trigger] self.coins().dom().contains(k)
                    ==> self.coins()[k].exponent <= MAX_EXPONENT,
                forall|k: (PurseId, u64)|
                    #[trigger] self.entries().dom().contains(k)
                    ==> self.entries()[k].exponent <= MAX_EXPONENT,
                self.coins@.len() <= (u64::MAX / 1073741824) as nat,
                self.entries@.len() <= (u64::MAX / 1073741824) as nat,
                (self.coins@.len() as nat + self.entries@.len() as nat)
                    <= (u64::MAX / 1073741824) as nat,
                forall|j: int| 0 <= j < i ==> (#[trigger] self.purses@[j]).id != p,
            decreases self.purses.len() - i,
        {
            if self.purses[i].id == p {
                let spendable = self.sum_available_real_in(p);
                let ready = self.sum_ready_real_in(p);
                let pending = self.sum_pending_real_in(p);
                proof {
                    assert(spendable as nat <= self.coins@.len() as nat * 1073741824);
                    assert(ready as nat <= self.entries@.len() as nat * 1073741824);
                }
                let rec = &self.purses[i];
                let name_copy: Vec<u8> = rec.name.clone();
                assert(name_copy@ == rec.name@);
                return Ok(PurseInfo {
                    id: rec.id,
                    name: name_copy,
                    spendable,
                    spendable_strict: spendable + ready,
                    pending,
                });
            }
            i += 1;
        }
        Err(Error::PurseNotFound(p))
    }


    /// 6.1 `queryPurse` (Quint lines 603-612; design §8.1 `query_purse`).
    ///
    /// Returns a synchronous snapshot:
    /// - `spendable`        — sum of Available-coin values in `p`.
    /// - `spendable_strict` — `spendable + sum of Ready-entry values`
    ///                        (entries fully matured into the
    ///                        anonymity ring).
    /// - `pending`          — sum of LocalAvailable entries in `p`
    ///                        that are Waiting or Missing on-chain
    ///                        (in-flight top-ups not yet matured).
    ///
    /// Preconditions bound coin / entry Vec sizes so the cumulative
    /// `u64` aggregations don't overflow under the pilot value scheme.
    pub fn query_purse(&self, p: PurseId) -> (info: Result<PurseInfo, Error>)
        requires
            self.invariant(),
            self.coins@.len() <= (u64::MAX / 1073741824) as nat,
            self.entries@.len() <= (u64::MAX / 1073741824) as nat,
            // spendable + ready_entries must fit in u64.
            (self.coins@.len() as nat + self.entries@.len() as nat)
                <= (u64::MAX / 1073741824) as nat,
        ensures
            match info {
                Ok(i) =>
                    self.purses().dom().contains(p)
                    && i.id == p
                    && i.name@ == self.purses()[p].name
                    && i.spendable as nat
                        == sum_avail_prefix(self.coins@, p, self.coins@.len() as nat)
                    && i.spendable_strict as nat
                        == sum_avail_prefix(self.coins@, p, self.coins@.len() as nat)
                            + sum_ready_prefix(self.entries@, p,
                                               self.entries@.len() as nat)
                    && i.pending as nat
                        == sum_pending_prefix(self.entries@, p,
                                              self.entries@.len() as nat),
                Err(Error::PurseNotFound(q)) =>
                    !self.purses().dom().contains(p) && q == p,
                Err(_) => false,
            },
    {
        let mut i: usize = 0;
        while i < self.purses.len()
            invariant
                0 <= i <= self.purses.len(),
                self.invariant(),
                self.coins@.len() <= (u64::MAX / 1073741824) as nat,
                self.entries@.len() <= (u64::MAX / 1073741824) as nat,
                (self.coins@.len() as nat + self.entries@.len() as nat)
                    <= (u64::MAX / 1073741824) as nat,
                forall|j: int| 0 <= j < i ==> (#[trigger] self.purses@[j]).id != p,
            decreases
                self.purses.len() - i,
        {
            if self.purses[i].id == p {
                let spendable = self.sum_available_in(p);
                let ready = self.sum_ready_in(p);
                let pending = self.sum_pending_in(p);
                proof {
                    // sum_avail_prefix is bounded by len * 2^30; same for ready.
                    // Together they fit in u64 because (coins.len + entries.len)
                    // <= u64::MAX/2^30 was given by the precondition.
                    assert(spendable as nat <= self.coins@.len() as nat * 1073741824);
                    assert(ready as nat <= self.entries@.len() as nat * 1073741824);
                }
                let rec = &self.purses[i];
                let name_copy: Vec<u8> = rec.name.clone();
                assert(name_copy@ == rec.name@);
                return Ok(PurseInfo {
                    id: rec.id,
                    name: name_copy,
                    spendable,
                    spendable_strict: spendable + ready,
                    pending,
                });
            }
            i += 1;
        }
        Err(Error::PurseNotFound(p))
    }
}

} // verus!
