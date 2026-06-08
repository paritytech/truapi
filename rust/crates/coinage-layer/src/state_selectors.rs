//! Selectors: `find_*_coin*`, `find_*_entry*`, subset-sum covers, top-priority, classify-payment.

use vstd::prelude::*;

use crate::*;

verus! {

impl State {
    /// Autonomous maintenance trigger: scan purses, return the first
    /// one whose `Available` coin count strictly exceeds `threshold`.
    /// Returns `None` if no purse is over-fragmented. Quint analog:
    /// maintenance scheduler that decides which purse to consolidate next.
    pub fn find_purse_needing_maintenance(&self, threshold: usize)
        -> (res: Option<PurseId>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(p) => self.purses().dom().contains(p),
                None => true,
            },
    {
        let mut i: usize = 0;
        while i < self.purses.len()
            invariant
                0 <= i <= self.purses.len(),
                self.invariant(),
            decreases self.purses.len() - i,
        {
            let pid = self.purses[i].id;
            let count = self.coin_count_available(pid);
            if count > threshold {
                proof {
                    assert(self.spec_purses@.dom().contains(pid));
                }
                return Some(pid);
            }
            i = i + 1;
        }
        None
    }


    /// Select the first `Available` coin in purse `p` whose `exponent`
    /// meets or exceeds `min_exponent`. Returns `None` if no such coin
    /// exists.
    pub fn select_coin(&self, p: PurseId, min_exponent: u8)
        -> (res: Option<(PurseId, u64)>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(key) =>
                    self.coins().dom().contains(key)
                    && key.0 == p
                    && self.coins()[key].state == CoinState::Available
                    && self.coins()[key].exponent >= min_exponent,
                None =>
                    forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        && k.0 == p
                        && self.coins()[k].state == CoinState::Available
                        ==> self.coins()[k].exponent < min_exponent,
            },
    {
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).purse != p
                    || self.coins@[jj].state != CoinState::Available
                    || self.coins@[jj].exponent < min_exponent,
            decreases self.coins.len() - j,
        {
            let is_avail = matches!(self.coins[j].state, CoinState::Available);
            if self.coins[j].purse == p
                && is_avail
                && self.coins[j].exponent >= min_exponent
            {
                let key = (self.coins[j].purse, self.coins[j].idx);
                proof {
                    // (l) gives us key in dom and ghost matches Vec entry.
                    assert(self.spec_coins@.dom().contains(key));
                }
                return Some(key);
            }
            j = j + 1;
        }
        // Not found in the Vec scan; lift to "no such ghost key" via (m).
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                && k.0 == p
                && self.coins()[k].state == CoinState::Available
                implies self.coins()[k].exponent < min_exponent
            by {
                // (m) gives a Vec witness w; the loop's "not found" fact then
                // forces w to have either wrong purse, wrong state, or smaller
                // exponent. The first two are ruled out by the ghost record's
                // values (which match the Vec entry by (l)), leaving exponent.
                let w = choose|jj: int|
                    0 <= jj < self.coins@.len()
                    && #[trigger] self.coins@[jj].purse == k.0
                    && self.coins@[jj].idx == k.1;
                assert(self.coins@[w].purse == p);
                assert(self.coins@[w].state == self.coins()[k].state);
                assert(self.coins@[w].exponent == self.coins()[k].exponent);
            }
        }
        None
    }


    /// Degenerate exact-cover: find an `Available` coin in purse `p` whose
    /// `coin_value(exp)` equals `requested` exactly. Returns `None` if no
    /// single coin matches.
    ///
    /// **Pilot scope:** Tier-1 exact-cover in the design (§6.3) considers
    /// multi-coin subsets summing to `requested`. This single-coin form is
    /// the simplest case. Multi-coin exact subset-sum (powerset enumeration
    /// with lex-min disambiguation) is the natural extension; deferred.
    pub fn find_exact_single_coin(&self, p: PurseId, requested: u64)
        -> (res: Option<(PurseId, u64)>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(key) =>
                    self.coins().dom().contains(key)
                    && key.0 == p
                    && self.coins()[key].state == CoinState::Available
                    && coin_value(self.coins()[key].exponent) == requested as nat,
                None =>
                    forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        && k.0 == p
                        && self.coins()[k].state == CoinState::Available
                        ==> coin_value(self.coins()[k].exponent) != requested as nat,
            },
    {
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).purse != p
                    || self.coins@[jj].state != CoinState::Available
                    || coin_value(self.coins@[jj].exponent) != requested as nat,
            decreases self.coins.len() - j,
        {
            let is_avail = matches!(self.coins[j].state, CoinState::Available);
            proof {
                let coin_key = (self.coins@[j as int].purse, self.coins@[j as int].idx);
                assert(self.spec_coins@.dom().contains(coin_key));
                assert(self.spec_coins@[coin_key] == self.coins@[j as int]);
                assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
            }
            let value: u64 = pow2_u64_exec(self.coins[j].exponent);
            if self.coins[j].purse == p && is_avail && value == requested {
                let key = (self.coins[j].purse, self.coins[j].idx);
                proof {
                    assert(self.spec_coins@.dom().contains(key));
                }
                return Some(key);
            }
            j = j + 1;
        }
        // None: lift Vec-scan "not found" to a universal claim over the ghost
        // map via invariant (m), same as `select_coin`.
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                && k.0 == p
                && self.coins()[k].state == CoinState::Available
                implies coin_value(self.coins()[k].exponent) != requested as nat
            by {
                let w = choose|jj: int|
                    0 <= jj < self.coins@.len()
                    && #[trigger] self.coins@[jj].purse == k.0
                    && self.coins@[jj].idx == k.1;
                assert(self.coins@[w].purse == p);
                assert(self.coins@[w].state == self.coins()[k].state);
                assert(self.coins@[w].exponent == self.coins()[k].exponent);
            }
        }
        None
    }


    /// Entry analog of [`Self::find_exact_single_coin`]: find a single
    /// `Ready + LocalAvailable` entry in purse `p` whose
    /// `coin_value(exp)` equals `requested` exactly. Sharp `None`.
    pub fn find_exact_single_entry(&self, p: PurseId, requested: u64)
        -> (res: Option<(PurseId, u64)>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(key) =>
                    self.entries().dom().contains(key)
                    && key.0 == p
                    && self.entries()[key].on_chain == EntryOnChain::Ready
                    && self.entries()[key].local == EntryLocal::LocalAvailable
                    && coin_value(self.entries()[key].exponent) == requested as nat,
                None =>
                    forall|k: (PurseId, u64)|
                        #[trigger] self.entries().dom().contains(k)
                        && k.0 == p
                        && self.entries()[k].on_chain == EntryOnChain::Ready
                        && self.entries()[k].local == EntryLocal::LocalAvailable
                        ==> coin_value(self.entries()[k].exponent) != requested as nat,
            },
    {
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.entries@[jj]).purse != p
                    || self.entries@[jj].on_chain != EntryOnChain::Ready
                    || self.entries@[jj].local != EntryLocal::LocalAvailable
                    || coin_value(self.entries@[jj].exponent) != requested as nat,
            decreases self.entries.len() - j,
        {
            let e = &self.entries[j];
            let is_ready = matches!(e.on_chain, EntryOnChain::Ready);
            let is_local_avail = matches!(e.local, EntryLocal::LocalAvailable);
            proof {
                let entry_key = (self.entries@[j as int].purse, self.entries@[j as int].idx);
                assert(self.spec_entries@.dom().contains(entry_key));
                assert(self.spec_entries@[entry_key] == self.entries@[j as int]);
                assert(self.entries@[j as int].exponent <= MAX_EXPONENT);
            }
            let value: u64 = pow2_u64_exec(e.exponent);
            if e.purse == p && is_ready && is_local_avail && value == requested {
                let key = (e.purse, e.idx);
                proof {
                    assert(self.spec_entries@.dom().contains(key));
                }
                return Some(key);
            }
            j = j + 1;
        }
        proof {
            // Lift Vec-scan "not found" to a universal claim over the ghost map
            // via entry invariant (s).
            assert forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                && k.0 == p
                && self.entries()[k].on_chain == EntryOnChain::Ready
                && self.entries()[k].local == EntryLocal::LocalAvailable
                implies coin_value(self.entries()[k].exponent) != requested as nat
            by {
                let w = choose|jj: int|
                    0 <= jj < self.entries@.len()
                    && #[trigger] self.entries@[jj].purse == k.0
                    && self.entries@[jj].idx == k.1;
                assert(self.entries@[w].purse == p);
                assert(self.entries@[w].on_chain == self.entries()[k].on_chain);
                assert(self.entries@[w].local == self.entries()[k].local);
                assert(self.entries@[w].exponent == self.entries()[k].exponent);
            }
        }
        None
    }


    /// Entry analog of [`Self::find_two_coin_exact_cover`]: find any
    /// pair of distinct `Ready + LocalAvailable` entries in purse `p`
    /// whose values sum exactly to `amount`. Sharp `None`.
    pub fn find_two_entry_exact_cover(&self, p: PurseId, amount: u64)
        -> (res: Option<((PurseId, u64), (PurseId, u64))>)
        requires
            self.invariant(),
        ensures
            match res {
                Some((k1, k2)) =>
                    self.entries().dom().contains(k1)
                    && self.entries().dom().contains(k2)
                    && k1 != k2
                    && k1.0 == p && k2.0 == p
                    && self.entries()[k1].on_chain == EntryOnChain::Ready
                    && self.entries()[k1].local == EntryLocal::LocalAvailable
                    && self.entries()[k2].on_chain == EntryOnChain::Ready
                    && self.entries()[k2].local == EntryLocal::LocalAvailable
                    && coin_value(self.entries()[k1].exponent)
                        + coin_value(self.entries()[k2].exponent)
                        == amount as nat,
                None =>
                    forall|i1: int, i2: int|
                        0 <= i1 < self.entries@.len()
                        && 0 <= i2 < self.entries@.len()
                        && i1 != i2
                        ==> {
                            let e1 = #[trigger] self.entries@[i1];
                            let e2 = #[trigger] self.entries@[i2];
                            e1.purse != p
                            || e1.on_chain != EntryOnChain::Ready
                            || e1.local != EntryLocal::LocalAvailable
                            || e2.purse != p
                            || e2.on_chain != EntryOnChain::Ready
                            || e2.local != EntryLocal::LocalAvailable
                            || (coin_value(e1.exponent) + coin_value(e2.exponent)
                                != amount as nat)
                        },
            },
    {
        let n = self.entries.len();
        let mut i: usize = 0;
        while i < n
            invariant
                0 <= i <= n,
                n == self.entries.len(),
                self.invariant(),
                forall|i1: int, i2: int|
                    0 <= i1 < i as int && 0 <= i2 < n as int && i1 != i2 ==> {
                        let e1 = #[trigger] self.entries@[i1];
                        let e2 = #[trigger] self.entries@[i2];
                        e1.purse != p
                        || e1.on_chain != EntryOnChain::Ready
                        || e1.local != EntryLocal::LocalAvailable
                        || e2.purse != p
                        || e2.on_chain != EntryOnChain::Ready
                        || e2.local != EntryLocal::LocalAvailable
                        || (coin_value(e1.exponent) + coin_value(e2.exponent)
                            != amount as nat)
                    },
            decreases n - i,
        {
            let e1_ref = &self.entries[i];
            let is_ready1 = matches!(e1_ref.on_chain, EntryOnChain::Ready);
            let is_local_avail1 = matches!(e1_ref.local, EntryLocal::LocalAvailable);
            if e1_ref.purse == p && is_ready1 && is_local_avail1 {
                proof {
                    let entry_key = (self.entries@[i as int].purse, self.entries@[i as int].idx);
                    assert(self.spec_entries@.dom().contains(entry_key));
                    assert(self.spec_entries@[entry_key] == self.entries@[i as int]);
                    assert(self.entries@[i as int].exponent <= MAX_EXPONENT);
                }
                let vi: u64 = pow2_u64_exec(self.entries[i].exponent);
                if vi <= amount {
                    let mut k: usize = 0;
                    while k < n
                        invariant
                            0 <= k <= n,
                            n == self.entries.len(),
                            i < n,
                            self.invariant(),
                            self.entries@[i as int].purse == p,
                            self.entries@[i as int].on_chain == EntryOnChain::Ready,
                            self.entries@[i as int].local == EntryLocal::LocalAvailable,
                            vi as nat == coin_value(self.entries@[i as int].exponent),
                            vi <= 1073741824u64,
                            vi <= amount,
                            forall|i1: int, i2: int|
                                0 <= i1 < i as int
                                && 0 <= i2 < n as int
                                && i1 != i2 ==> {
                                    let e1 = #[trigger] self.entries@[i1];
                                    let e2 = #[trigger] self.entries@[i2];
                                    e1.purse != p
                                    || e1.on_chain != EntryOnChain::Ready
                                    || e1.local != EntryLocal::LocalAvailable
                                    || e2.purse != p
                                    || e2.on_chain != EntryOnChain::Ready
                                    || e2.local != EntryLocal::LocalAvailable
                                    || (coin_value(e1.exponent) + coin_value(e2.exponent)
                                        != amount as nat)
                                },
                            forall|k2: int|
                                0 <= k2 < k as int && k2 != i as int ==>
                                (#[trigger] self.entries@[k2]).purse != p
                                || self.entries@[k2].on_chain != EntryOnChain::Ready
                                || self.entries@[k2].local != EntryLocal::LocalAvailable
                                || (coin_value(self.entries@[i as int].exponent)
                                        + coin_value(self.entries@[k2].exponent)
                                    != amount as nat),
                        decreases n - k,
                    {
                        if k != i {
                            let e2_ref = &self.entries[k];
                            let is_ready2 = matches!(e2_ref.on_chain, EntryOnChain::Ready);
                            let is_local_avail2 = matches!(e2_ref.local,
                                                           EntryLocal::LocalAvailable);
                            if e2_ref.purse == p && is_ready2 && is_local_avail2 {
                                proof {
                                    let entry_key = (self.entries@[k as int].purse,
                                                     self.entries@[k as int].idx);
                                    assert(self.spec_entries@.dom().contains(entry_key));
                                    assert(self.spec_entries@[entry_key]
                                        == self.entries@[k as int]);
                                    assert(self.entries@[k as int].exponent <= MAX_EXPONENT);
                                }
                                let vk: u64 = pow2_u64_exec(self.entries[k].exponent);
                                if vi + vk == amount {
                                    let k1 = (self.entries[i].purse, self.entries[i].idx);
                                    let k2_key = (self.entries[k].purse, self.entries[k].idx);
                                    proof {
                                        assert(self.spec_entries@.dom().contains(k1));
                                        assert(self.spec_entries@.dom().contains(k2_key));
                                        assert(k1 != k2_key);
                                    }
                                    return Some((k1, k2_key));
                                }
                            }
                        }
                        k = k + 1;
                    }
                }
            }
            i = i + 1;
        }
        None
    }


    /// Find the highest-priority selectable entry in purse `p` —
    /// Ready on-chain, LocalAvailable locally — per the §6.3
    /// `entryOrderLT` ordering. Returns `None` if no such entry
    /// exists. Tiebreakers: ring_idx ascending, then idx ascending.
    pub fn find_top_priority_entry(&self, p: PurseId)
        -> (res: Option<(PurseId, u64)>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(key) =>
                    self.entries().dom().contains(key)
                    && key.0 == p
                    && self.entries()[key].on_chain == EntryOnChain::Ready
                    && self.entries()[key].local == EntryLocal::LocalAvailable
                    && forall|k: (PurseId, u64)|
                        #[trigger] self.entries().dom().contains(k)
                        && k.0 == p
                        && self.entries()[k].on_chain == EntryOnChain::Ready
                        && self.entries()[k].local == EntryLocal::LocalAvailable
                        && k != key
                        ==> entry_priority_lt(self.entries()[key], self.entries()[k])
                            || self.entries()[key] == self.entries()[k],
                None =>
                    forall|k: (PurseId, u64)|
                        #[trigger] self.entries().dom().contains(k)
                        && k.0 == p
                        ==> self.entries()[k].on_chain != EntryOnChain::Ready
                            || self.entries()[k].local != EntryLocal::LocalAvailable,
            },
    {
        let mut best: Option<usize> = None;
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.invariant(),
                match best {
                    Some(bi) =>
                        0 <= bi < j
                        && self.entries@[bi as int].purse == p
                        && self.entries@[bi as int].on_chain == EntryOnChain::Ready
                        && self.entries@[bi as int].local == EntryLocal::LocalAvailable
                        && forall|jj: int| 0 <= jj < j ==>
                            #[trigger] self.entries@[jj].purse != p
                            || self.entries@[jj].on_chain != EntryOnChain::Ready
                            || self.entries@[jj].local != EntryLocal::LocalAvailable
                            || entry_priority_lt(self.entries@[bi as int], self.entries@[jj])
                            || self.entries@[bi as int] == self.entries@[jj],
                    None =>
                        forall|jj: int| 0 <= jj < j ==>
                            (#[trigger] self.entries@[jj]).purse != p
                            || self.entries@[jj].on_chain != EntryOnChain::Ready
                            || self.entries@[jj].local != EntryLocal::LocalAvailable,
                },
            decreases self.entries.len() - j,
        {
            let e = &self.entries[j];
            let is_ready = matches!(e.on_chain, EntryOnChain::Ready);
            let is_local_avail = matches!(e.local, EntryLocal::LocalAvailable);
            if e.purse == p && is_ready && is_local_avail {
                match best {
                    None => { best = Some(j); }
                    Some(bi) => {
                        let cur_better = self.entries[bi].exponent < e.exponent
                            || (self.entries[bi].exponent == e.exponent
                                && self.entries[bi].ring_idx > e.ring_idx)
                            || (self.entries[bi].exponent == e.exponent
                                && self.entries[bi].ring_idx == e.ring_idx
                                && self.entries[bi].idx > e.idx);
                        if cur_better {
                            best = Some(j);
                        }
                    }
                }
            }
            j = j + 1;
        }
        match best {
            None => {
                proof {
                    assert forall|k: (PurseId, u64)|
                        #[trigger] self.entries().dom().contains(k)
                        && k.0 == p
                        implies self.entries()[k].on_chain != EntryOnChain::Ready
                            || self.entries()[k].local != EntryLocal::LocalAvailable
                    by {
                        let w = choose|jj: int|
                            0 <= jj < self.entries@.len()
                            && #[trigger] self.entries@[jj].purse == k.0
                            && self.entries@[jj].idx == k.1;
                        assert(self.entries@[w].purse == p);
                        assert(self.entries@[w] == self.entries()[k]);
                    }
                }
                None
            }
            Some(bi) => {
                let key = (self.entries[bi].purse, self.entries[bi].idx);
                proof {
                    assert(self.spec_entries@.dom().contains(key));
                    assert(self.entries()[key] == self.entries@[bi as int]);
                    assert forall|k: (PurseId, u64)|
                        #[trigger] self.entries().dom().contains(k)
                        && k.0 == p
                        && self.entries()[k].on_chain == EntryOnChain::Ready
                        && self.entries()[k].local == EntryLocal::LocalAvailable
                        && k != key
                        implies entry_priority_lt(self.entries()[key], self.entries()[k])
                            || self.entries()[key] == self.entries()[k]
                    by {
                        let w = choose|jj: int|
                            0 <= jj < self.entries@.len()
                            && #[trigger] self.entries@[jj].purse == k.0
                            && self.entries@[jj].idx == k.1;
                        assert(self.entries@[w] == self.entries()[k]);
                    }
                }
                Some(key)
            }
        }
    }


    /// Find any recycler entry in purse `p` that is `Ready` on-chain and
    /// `LocalAvailable` locally — i.e., selectable for unload or
    /// transfer-via-entry. Returns the first match in Vec order, or
    /// `None` if no such entry exists.
    ///
    /// Quint analog: a witness for `selectableEntriesIn(p, false)` —
    /// the strict (non-degraded) form of the §6.3 entry selectability
    /// predicate.
    pub fn find_entry_ready(&self, p: PurseId) -> (res: Option<(PurseId, u64)>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(key) =>
                    self.entries().dom().contains(key)
                    && key.0 == p
                    && self.entries()[key].on_chain == EntryOnChain::Ready
                    && self.entries()[key].local == EntryLocal::LocalAvailable,
                None =>
                    forall|k: (PurseId, u64)|
                        #[trigger] self.entries().dom().contains(k)
                        && k.0 == p
                        ==> self.entries()[k].on_chain != EntryOnChain::Ready
                            || self.entries()[k].local != EntryLocal::LocalAvailable,
            },
    {
        let mut j: usize = 0;
        while j < self.entries.len()
            invariant
                0 <= j <= self.entries.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.entries@[jj]).purse != p
                    || self.entries@[jj].on_chain != EntryOnChain::Ready
                    || self.entries@[jj].local != EntryLocal::LocalAvailable,
            decreases self.entries.len() - j,
        {
            let e = &self.entries[j];
            let is_ready = matches!(e.on_chain, EntryOnChain::Ready);
            let is_local_avail = matches!(e.local, EntryLocal::LocalAvailable);
            if e.purse == p && is_ready && is_local_avail {
                let key = (e.purse, e.idx);
                proof {
                    assert(self.spec_entries@.dom().contains(key));
                }
                return Some(key);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.entries().dom().contains(k)
                && k.0 == p
                implies self.entries()[k].on_chain != EntryOnChain::Ready
                    || self.entries()[k].local != EntryLocal::LocalAvailable
            by {
                let w = choose|jj: int|
                    0 <= jj < self.entries@.len()
                    && #[trigger] self.entries@[jj].purse == k.0
                    && self.entries@[jj].idx == k.1;
                assert(self.entries@[w].purse == p);
                assert(self.entries@[w].on_chain == self.entries()[k].on_chain);
                assert(self.entries@[w].local == self.entries()[k].local);
            }
        }
        None
    }


    /// Exec witness for [`classify_incoming_payment`]: scan the memo
    /// list, count how many recipients map to a known local coin via
    /// [`Self::find_coin_with_account`], and apply the §8.8
    /// classification rule.
    pub fn classify_incoming_payment_exec(&self, memos: &Vec<MemoEntry>)
        -> (res: PaymentClassification)
        requires
            self.invariant(),
            memos@.len() <= u64::MAX as nat,
        ensures
            res == classify_incoming_payment(memos@, self.coins()),
    {
        let n = memos.len();
        let mut matched: u64 = 0;
        let mut i: usize = 0;
        while i < n
            invariant
                0 <= i <= n,
                n == memos@.len(),
                n <= u64::MAX as nat,
                matched as nat <= i as nat,
                self.invariant(),
                matched as nat == count_matched_memos(memos@, self.coins(), i as nat),
            decreases n - i,
        {
            let m = memos[i];
            match self.find_coin_with_account(m.recipient_account) {
                Some(_) => {
                    matched = matched + 1;
                }
                None => {}
            }
            i = i + 1;
        }
        if n == 0 {
            PaymentClassification::Unmatched
        } else if matched == 0 {
            PaymentClassification::Unmatched
        } else if matched as usize == n {
            PaymentClassification::Matched
        } else {
            PaymentClassification::Received
        }
    }


    /// Find the highest-priority `Available` coin in purse `p`,
    /// breaking ties per the §6.3 coin priority order:
    /// `(MaxExp - exp, MaxAge - age, idx)` (lex-smallest wins).
    /// Returns `None` if `p` has no Available coins.
    pub fn find_top_priority_coin(&self, p: PurseId)
        -> (res: Option<(PurseId, u64)>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(key) =>
                    self.coins().dom().contains(key)
                    && key.0 == p
                    && self.coins()[key].state == CoinState::Available
                    && forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        && k.0 == p
                        && self.coins()[k].state == CoinState::Available
                        && k != key
                        ==> coin_priority_lt(self.coins()[key], self.coins()[k])
                            || self.coins()[key] == self.coins()[k],
                None =>
                    forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        && k.0 == p
                        ==> self.coins()[k].state != CoinState::Available,
            },
    {
        let mut best: Option<usize> = None;
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                match best {
                    Some(bi) =>
                        0 <= bi < j
                        && self.coins@[bi as int].purse == p
                        && self.coins@[bi as int].state == CoinState::Available
                        && forall|jj: int| 0 <= jj < j ==>
                            #[trigger] self.coins@[jj].purse != p
                            || self.coins@[jj].state != CoinState::Available
                            || coin_priority_lt(self.coins@[bi as int], self.coins@[jj])
                            || self.coins@[bi as int] == self.coins@[jj],
                    None =>
                        forall|jj: int| 0 <= jj < j ==>
                            (#[trigger] self.coins@[jj]).purse != p
                            || self.coins@[jj].state != CoinState::Available,
                },
            decreases self.coins.len() - j,
        {
            let is_avail = matches!(self.coins[j].state, CoinState::Available);
            if self.coins[j].purse == p && is_avail {
                match best {
                    None => { best = Some(j); }
                    Some(bi) => {
                        let cur = &self.coins[j];
                        let cur_better = self.coins[bi].exponent < cur.exponent
                            || (self.coins[bi].exponent == cur.exponent
                                && self.coins[bi].age > cur.age)
                            || (self.coins[bi].exponent == cur.exponent
                                && self.coins[bi].age == cur.age
                                && self.coins[bi].idx > cur.idx);
                        if cur_better {
                            best = Some(j);
                        }
                    }
                }
            }
            j = j + 1;
        }
        match best {
            None => {
                proof {
                    assert forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        && k.0 == p
                        implies self.coins()[k].state != CoinState::Available
                    by {
                        let w = choose|jj: int|
                            0 <= jj < self.coins@.len()
                            && #[trigger] self.coins@[jj].purse == k.0
                            && self.coins@[jj].idx == k.1;
                        assert(self.coins@[w].purse == p);
                        assert(self.coins@[w].state == self.coins()[k].state);
                    }
                }
                None
            }
            Some(bi) => {
                let key = (self.coins[bi].purse, self.coins[bi].idx);
                proof {
                    assert(self.spec_coins@.dom().contains(key));
                    assert(self.coins()[key] == self.coins@[bi as int]);
                    assert forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        && k.0 == p
                        && self.coins()[k].state == CoinState::Available
                        && k != key
                        implies coin_priority_lt(self.coins()[key], self.coins()[k])
                            || self.coins()[key] == self.coins()[k]
                    by {
                        let w = choose|jj: int|
                            0 <= jj < self.coins@.len()
                            && #[trigger] self.coins@[jj].purse == k.0
                            && self.coins@[jj].idx == k.1;
                        assert(self.coins@[w] == self.coins()[k]);
                    }
                }
                Some(key)
            }
        }
    }


    /// Find any coin (of any state) whose `account` matches `target`.
    /// Returns `(purse, idx)` of the first match in Vec order, or
    /// `None`. Used by `classify_incoming_payment` to test whether a
    /// memo's `recipient_account` is known locally.
    pub fn find_coin_with_account(&self, target: u64)
        -> (res: Option<(PurseId, u64)>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(key) =>
                    self.coins().dom().contains(key)
                    && self.coins()[key].account == target,
                None =>
                    forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        ==> self.coins()[k].account != target,
            },
    {
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).account != target,
            decreases self.coins.len() - j,
        {
            if self.coins[j].account == target {
                let key = (self.coins[j].purse, self.coins[j].idx);
                proof {
                    assert(self.spec_coins@.dom().contains(key));
                }
                return Some(key);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                implies self.coins()[k].account != target
            by {
                let w = choose|jj: int|
                    0 <= jj < self.coins@.len()
                    && #[trigger] self.coins@[jj].purse == k.0
                    && self.coins@[jj].idx == k.1;
                assert(self.coins@[w].account == self.coins()[k].account);
            }
        }
        None
    }


    /// Tier-3 (entry-supplemented cover, §6.3): find any pair of one
    /// `Available` coin and one `Ready + LocalAvailable` entry in
    /// purse `p` whose values sum exactly to `amount`.
    ///
    /// This is the simplest 1-coin + 1-entry case of the powerset-based
    /// existsUnloadCover. Full tier-3 with arbitrary coin and entry
    /// subsets remains task #88; this case unblocks the common
    /// "single coin not enough but one mature entry tips it over"
    /// pattern.
    pub fn find_coin_entry_exact_cover(&self, p: PurseId, amount: u64)
        -> (res: Option<((PurseId, u64), (PurseId, u64))>)
        requires
            self.invariant(),
        ensures
            match res {
                Some((coin_key, entry_key)) =>
                    self.coins().dom().contains(coin_key)
                    && self.entries().dom().contains(entry_key)
                    && coin_key.0 == p
                    && entry_key.0 == p
                    && self.coins()[coin_key].state == CoinState::Available
                    && self.entries()[entry_key].on_chain == EntryOnChain::Ready
                    && self.entries()[entry_key].local == EntryLocal::LocalAvailable
                    && coin_value(self.coins()[coin_key].exponent)
                        + coin_value(self.entries()[entry_key].exponent)
                        == amount as nat,
                None =>
                    // Sharp: no (coin, entry) pair satisfies the cover.
                    forall|i: int, k: int|
                        0 <= i < self.coins@.len()
                        && 0 <= k < self.entries@.len()
                        ==> {
                            let c = #[trigger] self.coins@[i];
                            let e = #[trigger] self.entries@[k];
                            c.purse != p
                            || c.state != CoinState::Available
                            || e.purse != p
                            || e.on_chain != EntryOnChain::Ready
                            || e.local != EntryLocal::LocalAvailable
                            || (coin_value(c.exponent) + coin_value(e.exponent)
                                != amount as nat)
                        },
            },
    {
        let nc = self.coins.len();
        let ne = self.entries.len();
        let mut i: usize = 0;
        while i < nc
            invariant
                0 <= i <= nc,
                nc == self.coins.len(),
                ne == self.entries.len(),
                self.invariant(),
                // Outer accumulator: no (coin, entry) pair with coin index < i.
                forall|i1: int, k: int|
                    0 <= i1 < i as int
                    && 0 <= k < ne as int
                    ==> {
                        let c = #[trigger] self.coins@[i1];
                        let e = #[trigger] self.entries@[k];
                        c.purse != p
                        || c.state != CoinState::Available
                        || e.purse != p
                        || e.on_chain != EntryOnChain::Ready
                        || e.local != EntryLocal::LocalAvailable
                        || (coin_value(c.exponent) + coin_value(e.exponent)
                            != amount as nat)
                    },
            decreases nc - i,
        {
            let ci_avail = matches!(self.coins[i].state, CoinState::Available);
            if self.coins[i].purse == p && ci_avail {
                proof {
                    let coin_key = (self.coins@[i as int].purse, self.coins@[i as int].idx);
                    assert(self.spec_coins@.dom().contains(coin_key));
                    assert(self.spec_coins@[coin_key] == self.coins@[i as int]);
                    assert(self.coins@[i as int].exponent <= MAX_EXPONENT);
                }
                let vi: u64 = pow2_u64_exec(self.coins[i].exponent);
                if vi <= amount {
                    let mut k: usize = 0;
                    while k < ne
                        invariant
                            0 <= k <= ne,
                            nc == self.coins.len(),
                            ne == self.entries.len(),
                            i < nc,
                            self.invariant(),
                            self.coins@[i as int].purse == p,
                            self.coins@[i as int].state == CoinState::Available,
                            vi as nat == coin_value(self.coins@[i as int].exponent),
                            vi <= 1073741824u64,
                            vi <= amount,
                            // Outer accumulator carried.
                            forall|i1: int, kk: int|
                                0 <= i1 < i as int
                                && 0 <= kk < ne as int
                                ==> {
                                    let c = #[trigger] self.coins@[i1];
                                    let e = #[trigger] self.entries@[kk];
                                    c.purse != p
                                    || c.state != CoinState::Available
                                    || e.purse != p
                                    || e.on_chain != EntryOnChain::Ready
                                    || e.local != EntryLocal::LocalAvailable
                                    || (coin_value(c.exponent) + coin_value(e.exponent)
                                        != amount as nat)
                                },
                            // Inner accumulator: for all checked k2 < k,
                            // the pair (i, k2) doesn't satisfy.
                            forall|k2: int|
                                0 <= k2 < k as int
                                ==>
                                (#[trigger] self.entries@[k2]).purse != p
                                || self.entries@[k2].on_chain != EntryOnChain::Ready
                                || self.entries@[k2].local != EntryLocal::LocalAvailable
                                || (coin_value(self.coins@[i as int].exponent)
                                        + coin_value(self.entries@[k2].exponent)
                                    != amount as nat),
                        decreases ne - k,
                    {
                        let e = &self.entries[k];
                        let is_ready = matches!(e.on_chain, EntryOnChain::Ready);
                        let is_local_avail = matches!(e.local, EntryLocal::LocalAvailable);
                        if e.purse == p && is_ready && is_local_avail {
                            proof {
                                let entry_key = (self.entries@[k as int].purse,
                                                 self.entries@[k as int].idx);
                                assert(self.spec_entries@.dom().contains(entry_key));
                                assert(self.spec_entries@[entry_key] == self.entries@[k as int]);
                                assert(self.entries@[k as int].exponent <= MAX_EXPONENT);
                            }
                            let ve: u64 = pow2_u64_exec(e.exponent);
                            if vi + ve == amount {
                                let ck = (self.coins[i].purse, self.coins[i].idx);
                                let ek = (self.entries[k].purse, self.entries[k].idx);
                                proof {
                                    assert(self.spec_coins@.dom().contains(ck));
                                    assert(self.spec_entries@.dom().contains(ek));
                                }
                                return Some((ck, ek));
                            }
                        }
                        k = k + 1;
                    }
                }
            }
            i = i + 1;
        }
        None
    }


    /// Tier-3 (entry-supplemented cover, §6.3, 2-coin + 1-entry): find
    /// any pair of distinct `Available` coins and one `Ready +
    /// LocalAvailable` entry in purse `p` whose values sum exactly
    /// to `amount`. Sharp `None` postcondition.
    pub fn find_two_coin_one_entry_cover(&self, p: PurseId, amount: u64)
        -> (res: Option<((PurseId, u64), (PurseId, u64), (PurseId, u64))>)
        requires
            self.invariant(),
        ensures
            match res {
                Some((c1, c2, e)) =>
                    self.coins().dom().contains(c1)
                    && self.coins().dom().contains(c2)
                    && self.entries().dom().contains(e)
                    && c1 != c2
                    && c1.0 == p && c2.0 == p && e.0 == p
                    && self.coins()[c1].state == CoinState::Available
                    && self.coins()[c2].state == CoinState::Available
                    && self.entries()[e].on_chain == EntryOnChain::Ready
                    && self.entries()[e].local == EntryLocal::LocalAvailable
                    && coin_value(self.coins()[c1].exponent)
                        + coin_value(self.coins()[c2].exponent)
                        + coin_value(self.entries()[e].exponent)
                        == amount as nat,
                None =>
                    forall|i1: int, i2: int, k: int|
                        0 <= i1 < self.coins@.len()
                        && 0 <= i2 < self.coins@.len()
                        && 0 <= k < self.entries@.len()
                        && i1 != i2
                        ==> {
                            let c1 = #[trigger] self.coins@[i1];
                            let c2 = #[trigger] self.coins@[i2];
                            let e = #[trigger] self.entries@[k];
                            c1.purse != p
                            || c1.state != CoinState::Available
                            || c2.purse != p
                            || c2.state != CoinState::Available
                            || e.purse != p
                            || e.on_chain != EntryOnChain::Ready
                            || e.local != EntryLocal::LocalAvailable
                            || (coin_value(c1.exponent)
                                    + coin_value(c2.exponent)
                                    + coin_value(e.exponent)
                                != amount as nat)
                        },
            },
    {
        let nc = self.coins.len();
        let ne = self.entries.len();
        let mut i: usize = 0;
        while i < nc
            invariant
                0 <= i <= nc,
                nc == self.coins.len(),
                ne == self.entries.len(),
                self.invariant(),
                // Outer accumulator: no (i1, i2, k) with i1 < i works.
                forall|i1: int, i2: int, k: int|
                    0 <= i1 < i as int
                    && 0 <= i2 < nc as int
                    && 0 <= k < ne as int
                    && i1 != i2
                    ==> {
                        let c1 = #[trigger] self.coins@[i1];
                        let c2 = #[trigger] self.coins@[i2];
                        let e = #[trigger] self.entries@[k];
                        c1.purse != p
                        || c1.state != CoinState::Available
                        || c2.purse != p
                        || c2.state != CoinState::Available
                        || e.purse != p
                        || e.on_chain != EntryOnChain::Ready
                        || e.local != EntryLocal::LocalAvailable
                        || (coin_value(c1.exponent)
                                + coin_value(c2.exponent)
                                + coin_value(e.exponent)
                            != amount as nat)
                    },
            decreases nc - i,
        {
            let ci_avail = matches!(self.coins[i].state, CoinState::Available);
            if self.coins[i].purse == p && ci_avail {
                proof {
                    let coin_key = (self.coins@[i as int].purse, self.coins@[i as int].idx);
                    assert(self.spec_coins@.dom().contains(coin_key));
                    assert(self.spec_coins@[coin_key] == self.coins@[i as int]);
                    assert(self.coins@[i as int].exponent <= MAX_EXPONENT);
                }
                let vi: u64 = pow2_u64_exec(self.coins[i].exponent);
                if vi <= amount {
                    let mut j: usize = 0;
                    while j < nc
                        invariant
                            0 <= j <= nc,
                            nc == self.coins.len(),
                            ne == self.entries.len(),
                            i < nc,
                            self.invariant(),
                            self.coins@[i as int].purse == p,
                            self.coins@[i as int].state == CoinState::Available,
                            vi as nat == coin_value(self.coins@[i as int].exponent),
                            vi <= 1073741824u64,
                            vi <= amount,
                            forall|i1: int, i2: int, k: int|
                                0 <= i1 < i as int
                                && 0 <= i2 < nc as int
                                && 0 <= k < ne as int
                                && i1 != i2
                                ==> {
                                    let c1 = #[trigger] self.coins@[i1];
                                    let c2 = #[trigger] self.coins@[i2];
                                    let e = #[trigger] self.entries@[k];
                                    c1.purse != p
                                    || c1.state != CoinState::Available
                                    || c2.purse != p
                                    || c2.state != CoinState::Available
                                    || e.purse != p
                                    || e.on_chain != EntryOnChain::Ready
                                    || e.local != EntryLocal::LocalAvailable
                                    || (coin_value(c1.exponent)
                                            + coin_value(c2.exponent)
                                            + coin_value(e.exponent)
                                        != amount as nat)
                                },
                            // Middle accumulator: forall (i, j1, k) with j1 < j, j1 != i.
                            forall|j1: int, k: int|
                                0 <= j1 < j as int
                                && 0 <= k < ne as int
                                && j1 != i as int
                                ==> {
                                    let c2 = #[trigger] self.coins@[j1];
                                    let e = #[trigger] self.entries@[k];
                                    c2.purse != p
                                    || c2.state != CoinState::Available
                                    || e.purse != p
                                    || e.on_chain != EntryOnChain::Ready
                                    || e.local != EntryLocal::LocalAvailable
                                    || (coin_value(self.coins@[i as int].exponent)
                                            + coin_value(c2.exponent)
                                            + coin_value(e.exponent)
                                        != amount as nat)
                                },
                        decreases nc - j,
                    {
                        if j != i {
                            let cj_avail = matches!(self.coins[j].state, CoinState::Available);
                            if self.coins[j].purse == p && cj_avail {
                                proof {
                                    let coin_key = (self.coins@[j as int].purse,
                                                    self.coins@[j as int].idx);
                                    assert(self.spec_coins@.dom().contains(coin_key));
                                    assert(self.spec_coins@[coin_key] == self.coins@[j as int]);
                                    assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
                                }
                                let vj: u64 = pow2_u64_exec(self.coins[j].exponent);
                                if vi + vj <= amount {
                                    let mut k: usize = 0;
                                    while k < ne
                                        invariant
                                            0 <= k <= ne,
                                            nc == self.coins.len(),
                                            ne == self.entries.len(),
                                            i < nc,
                                            j < nc,
                                            i != j as usize,
                                            self.invariant(),
                                            self.coins@[i as int].purse == p,
                                            self.coins@[i as int].state == CoinState::Available,
                                            self.coins@[j as int].purse == p,
                                            self.coins@[j as int].state == CoinState::Available,
                                            vi as nat == coin_value(self.coins@[i as int].exponent),
                                            vj as nat == coin_value(self.coins@[j as int].exponent),
                                            vi <= 1073741824u64,
                                            vj <= 1073741824u64,
                                            vi + vj <= amount,
                                            // Inner accumulator: forall k2 < k checked, triple fails.
                                            forall|k2: int|
                                                0 <= k2 < k as int
                                                ==>
                                                (#[trigger] self.entries@[k2]).purse != p
                                                || self.entries@[k2].on_chain != EntryOnChain::Ready
                                                || self.entries@[k2].local != EntryLocal::LocalAvailable
                                                || (coin_value(self.coins@[i as int].exponent)
                                                        + coin_value(self.coins@[j as int].exponent)
                                                        + coin_value(self.entries@[k2].exponent)
                                                    != amount as nat),
                                        decreases ne - k,
                                    {
                                        let e = &self.entries[k];
                                        let is_ready = matches!(e.on_chain, EntryOnChain::Ready);
                                        let is_local_avail = matches!(e.local,
                                                                      EntryLocal::LocalAvailable);
                                        if e.purse == p && is_ready && is_local_avail {
                                            proof {
                                                let entry_key = (self.entries@[k as int].purse,
                                                                 self.entries@[k as int].idx);
                                                assert(self.spec_entries@.dom().contains(entry_key));
                                                assert(self.spec_entries@[entry_key]
                                                    == self.entries@[k as int]);
                                                assert(self.entries@[k as int].exponent
                                                    <= MAX_EXPONENT);
                                            }
                                            let ve: u64 = pow2_u64_exec(e.exponent);
                                            if vi + vj + ve == amount {
                                                let ck1 = (self.coins[i].purse, self.coins[i].idx);
                                                let ck2 = (self.coins[j].purse, self.coins[j].idx);
                                                let ek = (self.entries[k].purse, self.entries[k].idx);
                                                proof {
                                                    assert(self.spec_coins@.dom().contains(ck1));
                                                    assert(self.spec_coins@.dom().contains(ck2));
                                                    assert(self.spec_entries@.dom().contains(ek));
                                                    assert(ck1 != ck2);
                                                }
                                                return Some((ck1, ck2, ek));
                                            }
                                        }
                                        k = k + 1;
                                    }
                                }
                            }
                        }
                        j = j + 1;
                    }
                }
            }
            i = i + 1;
        }
        None
    }


    /// Tier-3 (entry-supplemented cover, §6.3, 1-coin + 2-entry): find
    /// any single `Available` coin and a pair of distinct `Ready +
    /// LocalAvailable` entries in purse `p` whose values sum exactly
    /// to `amount`. Sharp `None` postcondition.
    pub fn find_one_coin_two_entry_cover(&self, p: PurseId, amount: u64)
        -> (res: Option<((PurseId, u64), (PurseId, u64), (PurseId, u64))>)
        requires
            self.invariant(),
        ensures
            match res {
                Some((c, e1, e2)) =>
                    self.coins().dom().contains(c)
                    && self.entries().dom().contains(e1)
                    && self.entries().dom().contains(e2)
                    && e1 != e2
                    && c.0 == p && e1.0 == p && e2.0 == p
                    && self.coins()[c].state == CoinState::Available
                    && self.entries()[e1].on_chain == EntryOnChain::Ready
                    && self.entries()[e1].local == EntryLocal::LocalAvailable
                    && self.entries()[e2].on_chain == EntryOnChain::Ready
                    && self.entries()[e2].local == EntryLocal::LocalAvailable
                    && coin_value(self.coins()[c].exponent)
                        + coin_value(self.entries()[e1].exponent)
                        + coin_value(self.entries()[e2].exponent)
                        == amount as nat,
                None =>
                    forall|i: int, k1: int, k2: int|
                        0 <= i < self.coins@.len()
                        && 0 <= k1 < self.entries@.len()
                        && 0 <= k2 < self.entries@.len()
                        && k1 != k2
                        ==> {
                            let c = #[trigger] self.coins@[i];
                            let e1 = #[trigger] self.entries@[k1];
                            let e2 = #[trigger] self.entries@[k2];
                            c.purse != p
                            || c.state != CoinState::Available
                            || e1.purse != p
                            || e1.on_chain != EntryOnChain::Ready
                            || e1.local != EntryLocal::LocalAvailable
                            || e2.purse != p
                            || e2.on_chain != EntryOnChain::Ready
                            || e2.local != EntryLocal::LocalAvailable
                            || (coin_value(c.exponent)
                                    + coin_value(e1.exponent)
                                    + coin_value(e2.exponent)
                                != amount as nat)
                        },
            },
    {
        let nc = self.coins.len();
        let ne = self.entries.len();
        let mut i: usize = 0;
        while i < nc
            invariant
                0 <= i <= nc,
                nc == self.coins.len(),
                ne == self.entries.len(),
                self.invariant(),
                forall|i1: int, k1: int, k2: int|
                    0 <= i1 < i as int
                    && 0 <= k1 < ne as int
                    && 0 <= k2 < ne as int
                    && k1 != k2
                    ==> {
                        let c = #[trigger] self.coins@[i1];
                        let e1 = #[trigger] self.entries@[k1];
                        let e2 = #[trigger] self.entries@[k2];
                        c.purse != p
                        || c.state != CoinState::Available
                        || e1.purse != p
                        || e1.on_chain != EntryOnChain::Ready
                        || e1.local != EntryLocal::LocalAvailable
                        || e2.purse != p
                        || e2.on_chain != EntryOnChain::Ready
                        || e2.local != EntryLocal::LocalAvailable
                        || (coin_value(c.exponent)
                                + coin_value(e1.exponent)
                                + coin_value(e2.exponent)
                            != amount as nat)
                    },
            decreases nc - i,
        {
            let ci_avail = matches!(self.coins[i].state, CoinState::Available);
            if self.coins[i].purse == p && ci_avail {
                proof {
                    let coin_key = (self.coins@[i as int].purse, self.coins@[i as int].idx);
                    assert(self.spec_coins@.dom().contains(coin_key));
                    assert(self.spec_coins@[coin_key] == self.coins@[i as int]);
                    assert(self.coins@[i as int].exponent <= MAX_EXPONENT);
                }
                let vi: u64 = pow2_u64_exec(self.coins[i].exponent);
                if vi <= amount {
                    let mut j: usize = 0;
                    while j < ne
                        invariant
                            0 <= j <= ne,
                            nc == self.coins.len(),
                            ne == self.entries.len(),
                            i < nc,
                            self.invariant(),
                            self.coins@[i as int].purse == p,
                            self.coins@[i as int].state == CoinState::Available,
                            vi as nat == coin_value(self.coins@[i as int].exponent),
                            vi <= 1073741824u64,
                            vi <= amount,
                            forall|i1: int, k1: int, k2: int|
                                0 <= i1 < i as int
                                && 0 <= k1 < ne as int
                                && 0 <= k2 < ne as int
                                && k1 != k2
                                ==> {
                                    let c = #[trigger] self.coins@[i1];
                                    let e1 = #[trigger] self.entries@[k1];
                                    let e2 = #[trigger] self.entries@[k2];
                                    c.purse != p
                                    || c.state != CoinState::Available
                                    || e1.purse != p
                                    || e1.on_chain != EntryOnChain::Ready
                                    || e1.local != EntryLocal::LocalAvailable
                                    || e2.purse != p
                                    || e2.on_chain != EntryOnChain::Ready
                                    || e2.local != EntryLocal::LocalAvailable
                                    || (coin_value(c.exponent)
                                            + coin_value(e1.exponent)
                                            + coin_value(e2.exponent)
                                        != amount as nat)
                                },
                            forall|j1: int, k2: int|
                                0 <= j1 < j as int
                                && 0 <= k2 < ne as int
                                && j1 != k2
                                ==> {
                                    let e1 = #[trigger] self.entries@[j1];
                                    let e2 = #[trigger] self.entries@[k2];
                                    e1.purse != p
                                    || e1.on_chain != EntryOnChain::Ready
                                    || e1.local != EntryLocal::LocalAvailable
                                    || e2.purse != p
                                    || e2.on_chain != EntryOnChain::Ready
                                    || e2.local != EntryLocal::LocalAvailable
                                    || (coin_value(self.coins@[i as int].exponent)
                                            + coin_value(e1.exponent)
                                            + coin_value(e2.exponent)
                                        != amount as nat)
                                },
                        decreases ne - j,
                    {
                        let e1 = &self.entries[j];
                        let is_ready1 = matches!(e1.on_chain, EntryOnChain::Ready);
                        let is_local_avail1 = matches!(e1.local, EntryLocal::LocalAvailable);
                        if e1.purse == p && is_ready1 && is_local_avail1 {
                            proof {
                                let entry_key = (self.entries@[j as int].purse,
                                                 self.entries@[j as int].idx);
                                assert(self.spec_entries@.dom().contains(entry_key));
                                assert(self.spec_entries@[entry_key]
                                    == self.entries@[j as int]);
                                assert(self.entries@[j as int].exponent <= MAX_EXPONENT);
                            }
                            let ve1: u64 = pow2_u64_exec(e1.exponent);
                            if vi + ve1 <= amount {
                                let mut k: usize = 0;
                                while k < ne
                                    invariant
                                        0 <= k <= ne,
                                        nc == self.coins.len(),
                                        ne == self.entries.len(),
                                        i < nc,
                                        j < ne,
                                        self.invariant(),
                                        self.coins@[i as int].purse == p,
                                        self.coins@[i as int].state == CoinState::Available,
                                        self.entries@[j as int].purse == p,
                                        self.entries@[j as int].on_chain == EntryOnChain::Ready,
                                        self.entries@[j as int].local == EntryLocal::LocalAvailable,
                                        vi as nat == coin_value(self.coins@[i as int].exponent),
                                        ve1 as nat == coin_value(self.entries@[j as int].exponent),
                                        vi <= 1073741824u64,
                                        ve1 <= 1073741824u64,
                                        vi + ve1 <= amount,
                                        forall|k2: int|
                                            0 <= k2 < k as int
                                            && k2 != j as int
                                            ==>
                                            (#[trigger] self.entries@[k2]).purse != p
                                            || self.entries@[k2].on_chain != EntryOnChain::Ready
                                            || self.entries@[k2].local != EntryLocal::LocalAvailable
                                            || (coin_value(self.coins@[i as int].exponent)
                                                    + coin_value(self.entries@[j as int].exponent)
                                                    + coin_value(self.entries@[k2].exponent)
                                                != amount as nat),
                                    decreases ne - k,
                                {
                                    if k != j {
                                        let e2 = &self.entries[k];
                                        let is_ready2 = matches!(e2.on_chain, EntryOnChain::Ready);
                                        let is_local_avail2 = matches!(e2.local,
                                                                       EntryLocal::LocalAvailable);
                                        if e2.purse == p && is_ready2 && is_local_avail2 {
                                            proof {
                                                let entry_key = (self.entries@[k as int].purse,
                                                                 self.entries@[k as int].idx);
                                                assert(self.spec_entries@.dom().contains(entry_key));
                                                assert(self.spec_entries@[entry_key]
                                                    == self.entries@[k as int]);
                                                assert(self.entries@[k as int].exponent
                                                    <= MAX_EXPONENT);
                                            }
                                            let ve2: u64 = pow2_u64_exec(e2.exponent);
                                            if vi + ve1 + ve2 == amount {
                                                let ck = (self.coins[i].purse, self.coins[i].idx);
                                                let ek1 = (self.entries[j].purse,
                                                           self.entries[j].idx);
                                                let ek2 = (self.entries[k].purse,
                                                           self.entries[k].idx);
                                                proof {
                                                    assert(self.spec_coins@.dom().contains(ck));
                                                    assert(self.spec_entries@.dom().contains(ek1));
                                                    assert(self.spec_entries@.dom().contains(ek2));
                                                    assert(ek1 != ek2);
                                                }
                                                return Some((ck, ek1, ek2));
                                            }
                                        }
                                    }
                                    k = k + 1;
                                }
                            }
                        }
                        j = j + 1;
                    }
                }
            }
            i = i + 1;
        }
        None
    }


    /// Composite tier-3 entry-supplemented cover (§6.3) search up to
    /// total subset size 3. Tries 1-coin, 1-entry, 2-coin, 1-coin+1-entry,
    /// 2-entry, 3-coin, 2-coin+1-entry, 1-coin+2-entry in order and
    /// returns the first hit as a tagged enum (Tier3Cover). The `None`
    /// branch carries the conjoined sharp postconditions from all 8
    /// underlying primitives — no subset of total size 1, 2, or 3
    /// (any coin/entry split) in the purse sums to `amount`.
    ///
    /// Closes the practical slice of task #88. The remaining open piece
    /// — arbitrary-size powerset over the coin/entry product space —
    /// would extend coverage to larger subsets at the cost of new spec
    /// scaffolding. Sizes 1, 2, 3 cover the realistic cases.
    pub fn find_tier3_cover_up_to_3(&self, p: PurseId, amount: u64)
        -> (res: Option<Tier3Cover>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(Tier3Cover::C1(k)) =>
                    self.coins().dom().contains(k)
                    && k.0 == p
                    && self.coins()[k].state == CoinState::Available
                    && coin_value(self.coins()[k].exponent) == amount as nat,
                Some(Tier3Cover::E1(k)) =>
                    self.entries().dom().contains(k)
                    && k.0 == p
                    && self.entries()[k].on_chain == EntryOnChain::Ready
                    && self.entries()[k].local == EntryLocal::LocalAvailable
                    && coin_value(self.entries()[k].exponent) == amount as nat,
                Some(Tier3Cover::C2(k1, k2)) =>
                    self.coins().dom().contains(k1)
                    && self.coins().dom().contains(k2)
                    && k1 != k2 && k1.0 == p && k2.0 == p
                    && self.coins()[k1].state == CoinState::Available
                    && self.coins()[k2].state == CoinState::Available
                    && coin_value(self.coins()[k1].exponent)
                        + coin_value(self.coins()[k2].exponent)
                        == amount as nat,
                Some(Tier3Cover::C1E1(ck, ek)) =>
                    self.coins().dom().contains(ck)
                    && self.entries().dom().contains(ek)
                    && ck.0 == p && ek.0 == p
                    && self.coins()[ck].state == CoinState::Available
                    && self.entries()[ek].on_chain == EntryOnChain::Ready
                    && self.entries()[ek].local == EntryLocal::LocalAvailable
                    && coin_value(self.coins()[ck].exponent)
                        + coin_value(self.entries()[ek].exponent)
                        == amount as nat,
                Some(Tier3Cover::E2(k1, k2)) =>
                    self.entries().dom().contains(k1)
                    && self.entries().dom().contains(k2)
                    && k1 != k2 && k1.0 == p && k2.0 == p
                    && self.entries()[k1].on_chain == EntryOnChain::Ready
                    && self.entries()[k1].local == EntryLocal::LocalAvailable
                    && self.entries()[k2].on_chain == EntryOnChain::Ready
                    && self.entries()[k2].local == EntryLocal::LocalAvailable
                    && coin_value(self.entries()[k1].exponent)
                        + coin_value(self.entries()[k2].exponent)
                        == amount as nat,
                Some(Tier3Cover::C3(k1, k2, k3)) =>
                    self.coins().dom().contains(k1)
                    && self.coins().dom().contains(k2)
                    && self.coins().dom().contains(k3)
                    && k1 != k2 && k1 != k3 && k2 != k3
                    && k1.0 == p && k2.0 == p && k3.0 == p
                    && self.coins()[k1].state == CoinState::Available
                    && self.coins()[k2].state == CoinState::Available
                    && self.coins()[k3].state == CoinState::Available
                    && coin_value(self.coins()[k1].exponent)
                        + coin_value(self.coins()[k2].exponent)
                        + coin_value(self.coins()[k3].exponent)
                        == amount as nat,
                Some(Tier3Cover::C2E1(c1, c2, e)) =>
                    self.coins().dom().contains(c1)
                    && self.coins().dom().contains(c2)
                    && self.entries().dom().contains(e)
                    && c1 != c2
                    && c1.0 == p && c2.0 == p && e.0 == p
                    && self.coins()[c1].state == CoinState::Available
                    && self.coins()[c2].state == CoinState::Available
                    && self.entries()[e].on_chain == EntryOnChain::Ready
                    && self.entries()[e].local == EntryLocal::LocalAvailable
                    && coin_value(self.coins()[c1].exponent)
                        + coin_value(self.coins()[c2].exponent)
                        + coin_value(self.entries()[e].exponent)
                        == amount as nat,
                Some(Tier3Cover::C1E2(c, e1, e2)) =>
                    self.coins().dom().contains(c)
                    && self.entries().dom().contains(e1)
                    && self.entries().dom().contains(e2)
                    && e1 != e2
                    && c.0 == p && e1.0 == p && e2.0 == p
                    && self.coins()[c].state == CoinState::Available
                    && self.entries()[e1].on_chain == EntryOnChain::Ready
                    && self.entries()[e1].local == EntryLocal::LocalAvailable
                    && self.entries()[e2].on_chain == EntryOnChain::Ready
                    && self.entries()[e2].local == EntryLocal::LocalAvailable
                    && coin_value(self.coins()[c].exponent)
                        + coin_value(self.entries()[e1].exponent)
                        + coin_value(self.entries()[e2].exponent)
                        == amount as nat,
                None => {
                    // Conjoined sharp Nones from all 8 underlying primitives.
                    &&& forall|k: (PurseId, u64)|
                            #[trigger] self.coins().dom().contains(k)
                            && k.0 == p
                            && self.coins()[k].state == CoinState::Available
                            ==> coin_value(self.coins()[k].exponent) != amount as nat
                    &&& forall|k: (PurseId, u64)|
                            #[trigger] self.entries().dom().contains(k)
                            && k.0 == p
                            && self.entries()[k].on_chain == EntryOnChain::Ready
                            && self.entries()[k].local == EntryLocal::LocalAvailable
                            ==> coin_value(self.entries()[k].exponent) != amount as nat
                    &&& forall|i1: int, i2: int|
                            0 <= i1 < self.coins@.len()
                            && 0 <= i2 < self.coins@.len()
                            && i1 != i2
                            ==> {
                                let c1 = #[trigger] self.coins@[i1];
                                let c2 = #[trigger] self.coins@[i2];
                                c1.purse != p
                                || c1.state != CoinState::Available
                                || c2.purse != p
                                || c2.state != CoinState::Available
                                || (coin_value(c1.exponent) + coin_value(c2.exponent)
                                    != amount as nat)
                            }
                    &&& forall|i: int, k: int|
                            0 <= i < self.coins@.len()
                            && 0 <= k < self.entries@.len()
                            ==> {
                                let c = #[trigger] self.coins@[i];
                                let e = #[trigger] self.entries@[k];
                                c.purse != p
                                || c.state != CoinState::Available
                                || e.purse != p
                                || e.on_chain != EntryOnChain::Ready
                                || e.local != EntryLocal::LocalAvailable
                                || (coin_value(c.exponent) + coin_value(e.exponent)
                                    != amount as nat)
                            }
                    &&& forall|i1: int, i2: int|
                            0 <= i1 < self.entries@.len()
                            && 0 <= i2 < self.entries@.len()
                            && i1 != i2
                            ==> {
                                let e1 = #[trigger] self.entries@[i1];
                                let e2 = #[trigger] self.entries@[i2];
                                e1.purse != p
                                || e1.on_chain != EntryOnChain::Ready
                                || e1.local != EntryLocal::LocalAvailable
                                || e2.purse != p
                                || e2.on_chain != EntryOnChain::Ready
                                || e2.local != EntryLocal::LocalAvailable
                                || (coin_value(e1.exponent) + coin_value(e2.exponent)
                                    != amount as nat)
                            }
                    &&& forall|i1: int, i2: int, i3: int|
                            0 <= i1 < self.coins@.len()
                            && 0 <= i2 < self.coins@.len()
                            && 0 <= i3 < self.coins@.len()
                            && i1 != i2 && i1 != i3 && i2 != i3
                            ==> {
                                let c1 = #[trigger] self.coins@[i1];
                                let c2 = #[trigger] self.coins@[i2];
                                let c3 = #[trigger] self.coins@[i3];
                                c1.purse != p
                                || c1.state != CoinState::Available
                                || c2.purse != p
                                || c2.state != CoinState::Available
                                || c3.purse != p
                                || c3.state != CoinState::Available
                                || (coin_value(c1.exponent)
                                        + coin_value(c2.exponent)
                                        + coin_value(c3.exponent)
                                    != amount as nat)
                            }
                    &&& forall|i1: int, i2: int, k: int|
                            0 <= i1 < self.coins@.len()
                            && 0 <= i2 < self.coins@.len()
                            && 0 <= k < self.entries@.len()
                            && i1 != i2
                            ==> {
                                let c1 = #[trigger] self.coins@[i1];
                                let c2 = #[trigger] self.coins@[i2];
                                let e = #[trigger] self.entries@[k];
                                c1.purse != p
                                || c1.state != CoinState::Available
                                || c2.purse != p
                                || c2.state != CoinState::Available
                                || e.purse != p
                                || e.on_chain != EntryOnChain::Ready
                                || e.local != EntryLocal::LocalAvailable
                                || (coin_value(c1.exponent)
                                        + coin_value(c2.exponent)
                                        + coin_value(e.exponent)
                                    != amount as nat)
                            }
                    &&& forall|i: int, k1: int, k2: int|
                            0 <= i < self.coins@.len()
                            && 0 <= k1 < self.entries@.len()
                            && 0 <= k2 < self.entries@.len()
                            && k1 != k2
                            ==> {
                                let c = #[trigger] self.coins@[i];
                                let e1 = #[trigger] self.entries@[k1];
                                let e2 = #[trigger] self.entries@[k2];
                                c.purse != p
                                || c.state != CoinState::Available
                                || e1.purse != p
                                || e1.on_chain != EntryOnChain::Ready
                                || e1.local != EntryLocal::LocalAvailable
                                || e2.purse != p
                                || e2.on_chain != EntryOnChain::Ready
                                || e2.local != EntryLocal::LocalAvailable
                                || (coin_value(c.exponent)
                                        + coin_value(e1.exponent)
                                        + coin_value(e2.exponent)
                                    != amount as nat)
                            }
                },
            },
    {
        match self.find_exact_single_coin(p, amount) {
            Some(k) => return Some(Tier3Cover::C1(k)),
            None => {}
        }
        match self.find_exact_single_entry(p, amount) {
            Some(k) => return Some(Tier3Cover::E1(k)),
            None => {}
        }
        match self.find_two_coin_exact_cover(p, amount) {
            Some((k1, k2)) => return Some(Tier3Cover::C2(k1, k2)),
            None => {}
        }
        match self.find_coin_entry_exact_cover(p, amount) {
            Some((ck, ek)) => return Some(Tier3Cover::C1E1(ck, ek)),
            None => {}
        }
        match self.find_two_entry_exact_cover(p, amount) {
            Some((k1, k2)) => return Some(Tier3Cover::E2(k1, k2)),
            None => {}
        }
        match self.find_three_coin_exact_cover(p, amount) {
            Some((k1, k2, k3)) => return Some(Tier3Cover::C3(k1, k2, k3)),
            None => {}
        }
        match self.find_two_coin_one_entry_cover(p, amount) {
            Some((c1, c2, e)) => return Some(Tier3Cover::C2E1(c1, c2, e)),
            None => {}
        }
        match self.find_one_coin_two_entry_cover(p, amount) {
            Some((c, e1, e2)) => Some(Tier3Cover::C1E2(c, e1, e2)),
            None => None,
        }
    }


    /// Tier-1 multi-coin (§6.3): find any pair of distinct `Available`
    /// coins in purse `p` whose values sum exactly to `amount`. Returns
    /// the two keys in Vec order, or `None` if no such pair exists.
    ///
    /// This is the 2-coin special case of the powerset-based
    /// selectExactCoverDeterministic. Full powerset enumeration remains
    /// open (task #87); 2-coin already covers many cases that
    /// single-coin tier-1 misses (e.g. requesting amount = max_exp + 2
    /// with two coins of value max_exp + 1 / 1).
    pub fn find_two_coin_exact_cover(&self, p: PurseId, amount: u64)
        -> (res: Option<((PurseId, u64), (PurseId, u64))>)
        requires
            self.invariant(),
        ensures
            match res {
                Some((k1, k2)) =>
                    self.coins().dom().contains(k1)
                    && self.coins().dom().contains(k2)
                    && k1 != k2
                    && k1.0 == p
                    && k2.0 == p
                    && self.coins()[k1].state == CoinState::Available
                    && self.coins()[k2].state == CoinState::Available
                    && coin_value(self.coins()[k1].exponent)
                        + coin_value(self.coins()[k2].exponent)
                        == amount as nat,
                None =>
                    // Sharp: no two distinct Vec indices satisfy the pair-sum
                    // predicate. Combined with the dedup invariant (n), this
                    // is equivalent to "no two distinct coin keys with the
                    // pair-sum predicate".
                    forall|i1: int, i2: int|
                        0 <= i1 < self.coins@.len()
                        && 0 <= i2 < self.coins@.len()
                        && i1 != i2
                        ==> {
                            let c1 = #[trigger] self.coins@[i1];
                            let c2 = #[trigger] self.coins@[i2];
                            c1.purse != p
                            || c1.state != CoinState::Available
                            || c2.purse != p
                            || c2.state != CoinState::Available
                            || (coin_value(c1.exponent) + coin_value(c2.exponent)
                                != amount as nat)
                        },
            },
    {
        let n = self.coins.len();
        let mut i: usize = 0;
        while i < n
            invariant
                0 <= i <= n,
                n == self.coins.len(),
                self.invariant(),
                // No earlier outer index i1 < i forms a valid pair with any k.
                forall|i1: int, i2: int|
                    0 <= i1 < i as int && 0 <= i2 < n as int && i1 != i2 ==> {
                        let c1 = #[trigger] self.coins@[i1];
                        let c2 = #[trigger] self.coins@[i2];
                        c1.purse != p
                        || c1.state != CoinState::Available
                        || c2.purse != p
                        || c2.state != CoinState::Available
                        || (coin_value(c1.exponent) + coin_value(c2.exponent)
                            != amount as nat)
                    },
            decreases n - i,
        {
            let ci_avail = matches!(self.coins[i].state, CoinState::Available);
            if self.coins[i].purse == p && ci_avail {
                proof {
                    let coin_key = (self.coins@[i as int].purse, self.coins@[i as int].idx);
                    assert(self.spec_coins@.dom().contains(coin_key));
                    assert(self.spec_coins@[coin_key] == self.coins@[i as int]);
                    assert(self.coins@[i as int].exponent <= MAX_EXPONENT);
                }
                let vi: u64 = pow2_u64_exec(self.coins[i].exponent);
                if vi <= amount {
                    let mut k: usize = 0;
                    while k < n
                        invariant
                            0 <= k <= n,
                            n == self.coins.len(),
                            i < n,
                            self.invariant(),
                            self.coins@[i as int].purse == p,
                            self.coins@[i as int].state == CoinState::Available,
                            vi as nat == coin_value(self.coins@[i as int].exponent),
                            vi <= 1073741824u64,
                            vi <= amount,
                            // Same outer accumulator from before this inner loop.
                            forall|i1: int, i2: int|
                                0 <= i1 < i as int && 0 <= i2 < n as int
                                && i1 != i2 ==> {
                                    let c1 = #[trigger] self.coins@[i1];
                                    let c2 = #[trigger] self.coins@[i2];
                                    c1.purse != p
                                    || c1.state != CoinState::Available
                                    || c2.purse != p
                                    || c2.state != CoinState::Available
                                    || (coin_value(c1.exponent) + coin_value(c2.exponent)
                                        != amount as nat)
                                },
                            // Inner-loop accumulator: for all checked k2 < k,
                            // the pair (i, k2) doesn't satisfy the predicate.
                            forall|i2: int|
                                0 <= i2 < k as int && i2 != i as int ==>
                                (#[trigger] self.coins@[i2]).purse != p
                                || self.coins@[i2].state != CoinState::Available
                                || (coin_value(self.coins@[i as int].exponent)
                                        + coin_value(self.coins@[i2].exponent)
                                    != amount as nat),
                        decreases n - k,
                    {
                        if k != i {
                            let ck_avail = matches!(self.coins[k].state, CoinState::Available);
                            proof {
                                let coin_key = (self.coins@[k as int].purse,
                                                self.coins@[k as int].idx);
                                assert(self.spec_coins@.dom().contains(coin_key));
                                assert(self.spec_coins@[coin_key] == self.coins@[k as int]);
                                assert(self.coins@[k as int].exponent <= MAX_EXPONENT);
                            }
                            let vk: u64 = pow2_u64_exec(self.coins[k].exponent);
                            if self.coins[k].purse == p && ck_avail && vi + vk == amount {
                                let k1 = (self.coins[i].purse, self.coins[i].idx);
                                let k2 = (self.coins[k].purse, self.coins[k].idx);
                                proof {
                                    assert(self.spec_coins@.dom().contains(k1));
                                    assert(self.spec_coins@.dom().contains(k2));
                                    assert(k1 != k2);
                                }
                                return Some((k1, k2));
                            }
                        }
                        k = k + 1;
                    }
                }
                // If vi > amount, the pair-sum is also > amount and can't equal.
                // The outer-loop accumulator extends by this fact for i.
            }
            i = i + 1;
        }
        None
    }


    /// Tier-1 multi-coin (§6.3, 3-coin extension): find any triple of
    /// distinct `Available` coins in purse `p` whose values sum exactly
    /// to `amount`. Returns the three keys in Vec order, or `None` if
    /// no such triple exists.
    ///
    /// One step closer to full powerset (task #87): handles 3-coin
    /// subsets with sharp None. Full N-coin (bitmask enumeration over
    /// the first K Available coins) is still open.
    pub fn find_three_coin_exact_cover(&self, p: PurseId, amount: u64)
        -> (res: Option<((PurseId, u64), (PurseId, u64), (PurseId, u64))>)
        requires
            self.invariant(),
        ensures
            match res {
                Some((k1, k2, k3)) =>
                    self.coins().dom().contains(k1)
                    && self.coins().dom().contains(k2)
                    && self.coins().dom().contains(k3)
                    && k1 != k2 && k1 != k3 && k2 != k3
                    && k1.0 == p && k2.0 == p && k3.0 == p
                    && self.coins()[k1].state == CoinState::Available
                    && self.coins()[k2].state == CoinState::Available
                    && self.coins()[k3].state == CoinState::Available
                    && coin_value(self.coins()[k1].exponent)
                        + coin_value(self.coins()[k2].exponent)
                        + coin_value(self.coins()[k3].exponent)
                        == amount as nat,
                None =>
                    // Sharp: no three pairwise-distinct Vec indices form
                    // a triple summing to amount.
                    forall|i1: int, i2: int, i3: int|
                        0 <= i1 < self.coins@.len()
                        && 0 <= i2 < self.coins@.len()
                        && 0 <= i3 < self.coins@.len()
                        && i1 != i2 && i1 != i3 && i2 != i3
                        ==> {
                            let c1 = #[trigger] self.coins@[i1];
                            let c2 = #[trigger] self.coins@[i2];
                            let c3 = #[trigger] self.coins@[i3];
                            c1.purse != p
                            || c1.state != CoinState::Available
                            || c2.purse != p
                            || c2.state != CoinState::Available
                            || c3.purse != p
                            || c3.state != CoinState::Available
                            || (coin_value(c1.exponent)
                                    + coin_value(c2.exponent)
                                    + coin_value(c3.exponent)
                                != amount as nat)
                        },
            },
    {
        let n = self.coins.len();
        let mut i: usize = 0;
        while i < n
            invariant
                0 <= i <= n,
                n == self.coins.len(),
                self.invariant(),
                // Outer accumulator: no triple with first index < i works.
                forall|i1: int, i2: int, i3: int|
                    0 <= i1 < i as int
                    && 0 <= i2 < n as int
                    && 0 <= i3 < n as int
                    && i1 != i2 && i1 != i3 && i2 != i3
                    ==> {
                        let c1 = #[trigger] self.coins@[i1];
                        let c2 = #[trigger] self.coins@[i2];
                        let c3 = #[trigger] self.coins@[i3];
                        c1.purse != p
                        || c1.state != CoinState::Available
                        || c2.purse != p
                        || c2.state != CoinState::Available
                        || c3.purse != p
                        || c3.state != CoinState::Available
                        || (coin_value(c1.exponent)
                                + coin_value(c2.exponent)
                                + coin_value(c3.exponent)
                            != amount as nat)
                    },
            decreases n - i,
        {
            let ci_avail = matches!(self.coins[i].state, CoinState::Available);
            if self.coins[i].purse == p && ci_avail {
                proof {
                    let coin_key = (self.coins@[i as int].purse, self.coins@[i as int].idx);
                    assert(self.spec_coins@.dom().contains(coin_key));
                    assert(self.spec_coins@[coin_key] == self.coins@[i as int]);
                    assert(self.coins@[i as int].exponent <= MAX_EXPONENT);
                }
                let vi: u64 = pow2_u64_exec(self.coins[i].exponent);
                if vi <= amount {
                    let mut j: usize = 0;
                    while j < n
                        invariant
                            0 <= j <= n,
                            n == self.coins.len(),
                            i < n,
                            self.invariant(),
                            self.coins@[i as int].purse == p,
                            self.coins@[i as int].state == CoinState::Available,
                            vi as nat == coin_value(self.coins@[i as int].exponent),
                            vi <= 1073741824u64,
                            vi <= amount,
                            // Outer accumulator carried.
                            forall|i1: int, i2: int, i3: int|
                                0 <= i1 < i as int
                                && 0 <= i2 < n as int
                                && 0 <= i3 < n as int
                                && i1 != i2 && i1 != i3 && i2 != i3
                                ==> {
                                    let c1 = #[trigger] self.coins@[i1];
                                    let c2 = #[trigger] self.coins@[i2];
                                    let c3 = #[trigger] self.coins@[i3];
                                    c1.purse != p
                                    || c1.state != CoinState::Available
                                    || c2.purse != p
                                    || c2.state != CoinState::Available
                                    || c3.purse != p
                                    || c3.state != CoinState::Available
                                    || (coin_value(c1.exponent)
                                            + coin_value(c2.exponent)
                                            + coin_value(c3.exponent)
                                        != amount as nat)
                                },
                            // Middle accumulator: forall (i, j1, j3) with j1 < j, distinct.
                            forall|j1: int, j3: int|
                                0 <= j1 < j as int
                                && 0 <= j3 < n as int
                                && j1 != i as int && j3 != i as int && j1 != j3
                                ==> {
                                    let c2 = #[trigger] self.coins@[j1];
                                    let c3 = #[trigger] self.coins@[j3];
                                    c2.purse != p
                                    || c2.state != CoinState::Available
                                    || c3.purse != p
                                    || c3.state != CoinState::Available
                                    || (coin_value(self.coins@[i as int].exponent)
                                            + coin_value(c2.exponent)
                                            + coin_value(c3.exponent)
                                        != amount as nat)
                                },
                        decreases n - j,
                    {
                        if j != i {
                            let cj_avail = matches!(self.coins[j].state, CoinState::Available);
                            if self.coins[j].purse == p && cj_avail {
                                proof {
                                    let coin_key = (self.coins@[j as int].purse,
                                                    self.coins@[j as int].idx);
                                    assert(self.spec_coins@.dom().contains(coin_key));
                                    assert(self.spec_coins@[coin_key] == self.coins@[j as int]);
                                    assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
                                }
                                let vj: u64 = pow2_u64_exec(self.coins[j].exponent);
                                if vi + vj <= amount {
                                    let mut k: usize = 0;
                                    while k < n
                                        invariant
                                            0 <= k <= n,
                                            n == self.coins.len(),
                                            i < n,
                                            j < n,
                                            i != j as usize,
                                            self.invariant(),
                                            self.coins@[i as int].purse == p,
                                            self.coins@[i as int].state == CoinState::Available,
                                            self.coins@[j as int].purse == p,
                                            self.coins@[j as int].state == CoinState::Available,
                                            vi as nat == coin_value(self.coins@[i as int].exponent),
                                            vj as nat == coin_value(self.coins@[j as int].exponent),
                                            vi <= 1073741824u64,
                                            vj <= 1073741824u64,
                                            vi + vj <= amount,
                                            // Inner accumulator: forall k2 < k checked, triple fails.
                                            forall|k2: int|
                                                0 <= k2 < k as int
                                                && k2 != i as int && k2 != j as int
                                                ==>
                                                (#[trigger] self.coins@[k2]).purse != p
                                                || self.coins@[k2].state != CoinState::Available
                                                || (coin_value(self.coins@[i as int].exponent)
                                                        + coin_value(self.coins@[j as int].exponent)
                                                        + coin_value(self.coins@[k2].exponent)
                                                    != amount as nat),
                                        decreases n - k,
                                    {
                                        if k != i && k != j {
                                            let ck_avail = matches!(self.coins[k].state,
                                                                    CoinState::Available);
                                            if self.coins[k].purse == p && ck_avail {
                                                proof {
                                                    let coin_key = (self.coins@[k as int].purse,
                                                                    self.coins@[k as int].idx);
                                                    assert(self.spec_coins@.dom().contains(coin_key));
                                                    assert(self.spec_coins@[coin_key]
                                                        == self.coins@[k as int]);
                                                    assert(self.coins@[k as int].exponent
                                                        <= MAX_EXPONENT);
                                                }
                                                let vk: u64 = pow2_u64_exec(self.coins[k].exponent);
                                                if vi + vj + vk == amount {
                                                    let k1 = (self.coins[i].purse,
                                                              self.coins[i].idx);
                                                    let k2 = (self.coins[j].purse,
                                                              self.coins[j].idx);
                                                    let k3 = (self.coins[k].purse,
                                                              self.coins[k].idx);
                                                    proof {
                                                        assert(self.spec_coins@.dom().contains(k1));
                                                        assert(self.spec_coins@.dom().contains(k2));
                                                        assert(self.spec_coins@.dom().contains(k3));
                                                        assert(k1 != k2);
                                                        assert(k1 != k3);
                                                        assert(k2 != k3);
                                                    }
                                                    return Some((k1, k2, k3));
                                                }
                                            }
                                        }
                                        k = k + 1;
                                    }
                                }
                            }
                        }
                        j = j + 1;
                    }
                }
            }
            i = i + 1;
        }
        None
    }


    /// Tier-1 multi-coin (§6.3, 4-coin extension): find any quadruple of
    /// pairwise-distinct `Available` coins in purse `p` whose values sum
    /// exactly to `amount`. Sharp `None` postcondition.
    ///
    /// Same structural shape as `find_three_coin_exact_cover`, one more
    /// dimension. Continues partial closure of task #87.
    pub fn find_four_coin_exact_cover(&self, p: PurseId, amount: u64)
        -> (res: Option<((PurseId, u64), (PurseId, u64), (PurseId, u64), (PurseId, u64))>)
        requires
            self.invariant(),
        ensures
            match res {
                Some((k1, k2, k3, k4)) =>
                    self.coins().dom().contains(k1)
                    && self.coins().dom().contains(k2)
                    && self.coins().dom().contains(k3)
                    && self.coins().dom().contains(k4)
                    && k1 != k2 && k1 != k3 && k1 != k4
                    && k2 != k3 && k2 != k4 && k3 != k4
                    && k1.0 == p && k2.0 == p && k3.0 == p && k4.0 == p
                    && self.coins()[k1].state == CoinState::Available
                    && self.coins()[k2].state == CoinState::Available
                    && self.coins()[k3].state == CoinState::Available
                    && self.coins()[k4].state == CoinState::Available
                    && coin_value(self.coins()[k1].exponent)
                        + coin_value(self.coins()[k2].exponent)
                        + coin_value(self.coins()[k3].exponent)
                        + coin_value(self.coins()[k4].exponent)
                        == amount as nat,
                None =>
                    // Sharp: no four pairwise-distinct Vec indices form a
                    // quadruple summing to amount.
                    forall|i1: int, i2: int, i3: int, i4: int|
                        0 <= i1 < self.coins@.len()
                        && 0 <= i2 < self.coins@.len()
                        && 0 <= i3 < self.coins@.len()
                        && 0 <= i4 < self.coins@.len()
                        && i1 != i2 && i1 != i3 && i1 != i4
                        && i2 != i3 && i2 != i4 && i3 != i4
                        ==> {
                            let c1 = #[trigger] self.coins@[i1];
                            let c2 = #[trigger] self.coins@[i2];
                            let c3 = #[trigger] self.coins@[i3];
                            let c4 = #[trigger] self.coins@[i4];
                            c1.purse != p
                            || c1.state != CoinState::Available
                            || c2.purse != p
                            || c2.state != CoinState::Available
                            || c3.purse != p
                            || c3.state != CoinState::Available
                            || c4.purse != p
                            || c4.state != CoinState::Available
                            || (coin_value(c1.exponent)
                                    + coin_value(c2.exponent)
                                    + coin_value(c3.exponent)
                                    + coin_value(c4.exponent)
                                != amount as nat)
                        },
            },
    {
        let n = self.coins.len();
        let mut i: usize = 0;
        while i < n
            invariant
                0 <= i <= n,
                n == self.coins.len(),
                self.invariant(),
                forall|i1: int, i2: int, i3: int, i4: int|
                    0 <= i1 < i as int
                    && 0 <= i2 < n as int
                    && 0 <= i3 < n as int
                    && 0 <= i4 < n as int
                    && i1 != i2 && i1 != i3 && i1 != i4
                    && i2 != i3 && i2 != i4 && i3 != i4
                    ==> {
                        let c1 = #[trigger] self.coins@[i1];
                        let c2 = #[trigger] self.coins@[i2];
                        let c3 = #[trigger] self.coins@[i3];
                        let c4 = #[trigger] self.coins@[i4];
                        c1.purse != p
                        || c1.state != CoinState::Available
                        || c2.purse != p
                        || c2.state != CoinState::Available
                        || c3.purse != p
                        || c3.state != CoinState::Available
                        || c4.purse != p
                        || c4.state != CoinState::Available
                        || (coin_value(c1.exponent)
                                + coin_value(c2.exponent)
                                + coin_value(c3.exponent)
                                + coin_value(c4.exponent)
                            != amount as nat)
                    },
            decreases n - i,
        {
            let ci_avail = matches!(self.coins[i].state, CoinState::Available);
            if self.coins[i].purse == p && ci_avail {
                proof {
                    let coin_key = (self.coins@[i as int].purse, self.coins@[i as int].idx);
                    assert(self.spec_coins@.dom().contains(coin_key));
                    assert(self.spec_coins@[coin_key] == self.coins@[i as int]);
                    assert(self.coins@[i as int].exponent <= MAX_EXPONENT);
                }
                let vi: u64 = pow2_u64_exec(self.coins[i].exponent);
                if vi <= amount {
                    let mut j: usize = 0;
                    while j < n
                        invariant
                            0 <= j <= n,
                            n == self.coins.len(),
                            i < n,
                            self.invariant(),
                            self.coins@[i as int].purse == p,
                            self.coins@[i as int].state == CoinState::Available,
                            vi as nat == coin_value(self.coins@[i as int].exponent),
                            vi <= 1073741824u64,
                            vi <= amount,
                            forall|i1: int, i2: int, i3: int, i4: int|
                                0 <= i1 < i as int
                                && 0 <= i2 < n as int
                                && 0 <= i3 < n as int
                                && 0 <= i4 < n as int
                                && i1 != i2 && i1 != i3 && i1 != i4
                                && i2 != i3 && i2 != i4 && i3 != i4
                                ==> {
                                    let c1 = #[trigger] self.coins@[i1];
                                    let c2 = #[trigger] self.coins@[i2];
                                    let c3 = #[trigger] self.coins@[i3];
                                    let c4 = #[trigger] self.coins@[i4];
                                    c1.purse != p
                                    || c1.state != CoinState::Available
                                    || c2.purse != p
                                    || c2.state != CoinState::Available
                                    || c3.purse != p
                                    || c3.state != CoinState::Available
                                    || c4.purse != p
                                    || c4.state != CoinState::Available
                                    || (coin_value(c1.exponent)
                                            + coin_value(c2.exponent)
                                            + coin_value(c3.exponent)
                                            + coin_value(c4.exponent)
                                        != amount as nat)
                                },
                            forall|j1: int, j3: int, j4: int|
                                0 <= j1 < j as int
                                && 0 <= j3 < n as int
                                && 0 <= j4 < n as int
                                && j1 != i as int && j3 != i as int && j4 != i as int
                                && j1 != j3 && j1 != j4 && j3 != j4
                                ==> {
                                    let c2 = #[trigger] self.coins@[j1];
                                    let c3 = #[trigger] self.coins@[j3];
                                    let c4 = #[trigger] self.coins@[j4];
                                    c2.purse != p
                                    || c2.state != CoinState::Available
                                    || c3.purse != p
                                    || c3.state != CoinState::Available
                                    || c4.purse != p
                                    || c4.state != CoinState::Available
                                    || (coin_value(self.coins@[i as int].exponent)
                                            + coin_value(c2.exponent)
                                            + coin_value(c3.exponent)
                                            + coin_value(c4.exponent)
                                        != amount as nat)
                                },
                        decreases n - j,
                    {
                        if j != i {
                            let cj_avail = matches!(self.coins[j].state, CoinState::Available);
                            if self.coins[j].purse == p && cj_avail {
                                proof {
                                    let coin_key = (self.coins@[j as int].purse,
                                                    self.coins@[j as int].idx);
                                    assert(self.spec_coins@.dom().contains(coin_key));
                                    assert(self.spec_coins@[coin_key] == self.coins@[j as int]);
                                    assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
                                }
                                let vj: u64 = pow2_u64_exec(self.coins[j].exponent);
                                if vi + vj <= amount {
                                    let mut k: usize = 0;
                                    while k < n
                                        invariant
                                            0 <= k <= n,
                                            n == self.coins.len(),
                                            i < n,
                                            j < n,
                                            i != j as usize,
                                            self.invariant(),
                                            self.coins@[i as int].purse == p,
                                            self.coins@[i as int].state == CoinState::Available,
                                            self.coins@[j as int].purse == p,
                                            self.coins@[j as int].state == CoinState::Available,
                                            vi as nat == coin_value(self.coins@[i as int].exponent),
                                            vj as nat == coin_value(self.coins@[j as int].exponent),
                                            vi <= 1073741824u64,
                                            vj <= 1073741824u64,
                                            vi + vj <= amount,
                                            forall|k1: int, k4: int|
                                                0 <= k1 < k as int
                                                && 0 <= k4 < n as int
                                                && k1 != i as int && k1 != j as int
                                                && k4 != i as int && k4 != j as int
                                                && k1 != k4
                                                ==> {
                                                    let c3 = #[trigger] self.coins@[k1];
                                                    let c4 = #[trigger] self.coins@[k4];
                                                    c3.purse != p
                                                    || c3.state != CoinState::Available
                                                    || c4.purse != p
                                                    || c4.state != CoinState::Available
                                                    || (coin_value(self.coins@[i as int].exponent)
                                                            + coin_value(self.coins@[j as int].exponent)
                                                            + coin_value(c3.exponent)
                                                            + coin_value(c4.exponent)
                                                        != amount as nat)
                                                },
                                        decreases n - k,
                                    {
                                        if k != i && k != j {
                                            let ck_avail = matches!(self.coins[k].state,
                                                                    CoinState::Available);
                                            if self.coins[k].purse == p && ck_avail {
                                                proof {
                                                    let coin_key = (self.coins@[k as int].purse,
                                                                    self.coins@[k as int].idx);
                                                    assert(self.spec_coins@.dom().contains(coin_key));
                                                    assert(self.spec_coins@[coin_key]
                                                        == self.coins@[k as int]);
                                                    assert(self.coins@[k as int].exponent
                                                        <= MAX_EXPONENT);
                                                }
                                                let vk: u64 = pow2_u64_exec(self.coins[k].exponent);
                                                if vi + vj + vk <= amount {
                                                    let mut m: usize = 0;
                                                    while m < n
                                                        invariant
                                                            0 <= m <= n,
                                                            n == self.coins.len(),
                                                            i < n,
                                                            j < n,
                                                            k < n,
                                                            i != j as usize,
                                                            i != k as usize,
                                                            j != k as usize,
                                                            self.invariant(),
                                                            self.coins@[i as int].purse == p,
                                                            self.coins@[i as int].state == CoinState::Available,
                                                            self.coins@[j as int].purse == p,
                                                            self.coins@[j as int].state == CoinState::Available,
                                                            self.coins@[k as int].purse == p,
                                                            self.coins@[k as int].state == CoinState::Available,
                                                            vi as nat == coin_value(self.coins@[i as int].exponent),
                                                            vj as nat == coin_value(self.coins@[j as int].exponent),
                                                            vk as nat == coin_value(self.coins@[k as int].exponent),
                                                            vi <= 1073741824u64,
                                                            vj <= 1073741824u64,
                                                            vk <= 1073741824u64,
                                                            vi + vj + vk <= amount,
                                                            forall|m2: int|
                                                                0 <= m2 < m as int
                                                                && m2 != i as int
                                                                && m2 != j as int
                                                                && m2 != k as int
                                                                ==>
                                                                (#[trigger] self.coins@[m2]).purse != p
                                                                || self.coins@[m2].state != CoinState::Available
                                                                || (coin_value(self.coins@[i as int].exponent)
                                                                        + coin_value(self.coins@[j as int].exponent)
                                                                        + coin_value(self.coins@[k as int].exponent)
                                                                        + coin_value(self.coins@[m2].exponent)
                                                                    != amount as nat),
                                                        decreases n - m,
                                                    {
                                                        if m != i && m != j && m != k {
                                                            let cm_avail = matches!(
                                                                self.coins[m].state,
                                                                CoinState::Available);
                                                            if self.coins[m].purse == p && cm_avail {
                                                                proof {
                                                                    let coin_key = (
                                                                        self.coins@[m as int].purse,
                                                                        self.coins@[m as int].idx);
                                                                    assert(self.spec_coins@.dom()
                                                                        .contains(coin_key));
                                                                    assert(self.spec_coins@[coin_key]
                                                                        == self.coins@[m as int]);
                                                                    assert(self.coins@[m as int].exponent
                                                                        <= MAX_EXPONENT);
                                                                }
                                                                let vm: u64 = pow2_u64_exec(
                                                                    self.coins[m].exponent);
                                                                if vi + vj + vk + vm == amount {
                                                                    let k1 = (self.coins[i].purse,
                                                                              self.coins[i].idx);
                                                                    let k2 = (self.coins[j].purse,
                                                                              self.coins[j].idx);
                                                                    let k3 = (self.coins[k].purse,
                                                                              self.coins[k].idx);
                                                                    let k4 = (self.coins[m].purse,
                                                                              self.coins[m].idx);
                                                                    proof {
                                                                        assert(self.spec_coins@.dom()
                                                                            .contains(k1));
                                                                        assert(self.spec_coins@.dom()
                                                                            .contains(k2));
                                                                        assert(self.spec_coins@.dom()
                                                                            .contains(k3));
                                                                        assert(self.spec_coins@.dom()
                                                                            .contains(k4));
                                                                        assert(k1 != k2);
                                                                        assert(k1 != k3);
                                                                        assert(k1 != k4);
                                                                        assert(k2 != k3);
                                                                        assert(k2 != k4);
                                                                        assert(k3 != k4);
                                                                    }
                                                                    return Some((k1, k2, k3, k4));
                                                                }
                                                            }
                                                        }
                                                        m = m + 1;
                                                    }
                                                }
                                            }
                                        }
                                        k = k + 1;
                                    }
                                }
                            }
                        }
                        j = j + 1;
                    }
                }
            }
            i = i + 1;
        }
        None
    }


    /// Composite multi-coin subset-sum search: tries 1-, 2-, 3-, 4-coin
    /// exact covers in order and returns the first hit. The `None`
    /// branch carries the *conjoined* sharp postconditions from all
    /// four primitives — i.e. no subset of size 1, 2, 3, or 4 in the
    /// purse sums to `amount`.
    ///
    /// Practical multi-coin selector for task #87. Full N-coin powerset
    /// (any size) remains open; this covers the realistic small-K case
    /// that almost all transfers actually hit.
    pub fn find_subset_sum_up_to_4(&self, p: PurseId, amount: u64)
        -> (res: Option<SubsetSumCover>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(SubsetSumCover::One(k1)) =>
                    self.coins().dom().contains(k1)
                    && k1.0 == p
                    && self.coins()[k1].state == CoinState::Available
                    && coin_value(self.coins()[k1].exponent) == amount as nat,
                Some(SubsetSumCover::Two(k1, k2)) =>
                    self.coins().dom().contains(k1)
                    && self.coins().dom().contains(k2)
                    && k1 != k2
                    && k1.0 == p && k2.0 == p
                    && self.coins()[k1].state == CoinState::Available
                    && self.coins()[k2].state == CoinState::Available
                    && coin_value(self.coins()[k1].exponent)
                        + coin_value(self.coins()[k2].exponent)
                        == amount as nat,
                Some(SubsetSumCover::Three(k1, k2, k3)) =>
                    self.coins().dom().contains(k1)
                    && self.coins().dom().contains(k2)
                    && self.coins().dom().contains(k3)
                    && k1 != k2 && k1 != k3 && k2 != k3
                    && k1.0 == p && k2.0 == p && k3.0 == p
                    && self.coins()[k1].state == CoinState::Available
                    && self.coins()[k2].state == CoinState::Available
                    && self.coins()[k3].state == CoinState::Available
                    && coin_value(self.coins()[k1].exponent)
                        + coin_value(self.coins()[k2].exponent)
                        + coin_value(self.coins()[k3].exponent)
                        == amount as nat,
                Some(SubsetSumCover::Four(k1, k2, k3, k4)) =>
                    self.coins().dom().contains(k1)
                    && self.coins().dom().contains(k2)
                    && self.coins().dom().contains(k3)
                    && self.coins().dom().contains(k4)
                    && k1 != k2 && k1 != k3 && k1 != k4
                    && k2 != k3 && k2 != k4 && k3 != k4
                    && k1.0 == p && k2.0 == p && k3.0 == p && k4.0 == p
                    && self.coins()[k1].state == CoinState::Available
                    && self.coins()[k2].state == CoinState::Available
                    && self.coins()[k3].state == CoinState::Available
                    && self.coins()[k4].state == CoinState::Available
                    && coin_value(self.coins()[k1].exponent)
                        + coin_value(self.coins()[k2].exponent)
                        + coin_value(self.coins()[k3].exponent)
                        + coin_value(self.coins()[k4].exponent)
                        == amount as nat,
                None => {
                    // Conjoined sharp Nones from the four primitives.
                    &&& forall|k: (PurseId, u64)|
                            #[trigger] self.coins().dom().contains(k)
                            && k.0 == p
                            && self.coins()[k].state == CoinState::Available
                            ==> coin_value(self.coins()[k].exponent) != amount as nat
                    &&& forall|i1: int, i2: int|
                            0 <= i1 < self.coins@.len()
                            && 0 <= i2 < self.coins@.len()
                            && i1 != i2
                            ==> {
                                let c1 = #[trigger] self.coins@[i1];
                                let c2 = #[trigger] self.coins@[i2];
                                c1.purse != p
                                || c1.state != CoinState::Available
                                || c2.purse != p
                                || c2.state != CoinState::Available
                                || (coin_value(c1.exponent) + coin_value(c2.exponent)
                                    != amount as nat)
                            }
                    &&& forall|i1: int, i2: int, i3: int|
                            0 <= i1 < self.coins@.len()
                            && 0 <= i2 < self.coins@.len()
                            && 0 <= i3 < self.coins@.len()
                            && i1 != i2 && i1 != i3 && i2 != i3
                            ==> {
                                let c1 = #[trigger] self.coins@[i1];
                                let c2 = #[trigger] self.coins@[i2];
                                let c3 = #[trigger] self.coins@[i3];
                                c1.purse != p
                                || c1.state != CoinState::Available
                                || c2.purse != p
                                || c2.state != CoinState::Available
                                || c3.purse != p
                                || c3.state != CoinState::Available
                                || (coin_value(c1.exponent)
                                        + coin_value(c2.exponent)
                                        + coin_value(c3.exponent)
                                    != amount as nat)
                            }
                    &&& forall|i1: int, i2: int, i3: int, i4: int|
                            0 <= i1 < self.coins@.len()
                            && 0 <= i2 < self.coins@.len()
                            && 0 <= i3 < self.coins@.len()
                            && 0 <= i4 < self.coins@.len()
                            && i1 != i2 && i1 != i3 && i1 != i4
                            && i2 != i3 && i2 != i4 && i3 != i4
                            ==> {
                                let c1 = #[trigger] self.coins@[i1];
                                let c2 = #[trigger] self.coins@[i2];
                                let c3 = #[trigger] self.coins@[i3];
                                let c4 = #[trigger] self.coins@[i4];
                                c1.purse != p
                                || c1.state != CoinState::Available
                                || c2.purse != p
                                || c2.state != CoinState::Available
                                || c3.purse != p
                                || c3.state != CoinState::Available
                                || c4.purse != p
                                || c4.state != CoinState::Available
                                || (coin_value(c1.exponent)
                                        + coin_value(c2.exponent)
                                        + coin_value(c3.exponent)
                                        + coin_value(c4.exponent)
                                    != amount as nat)
                            }
                },
            },
    {
        match self.find_exact_single_coin(p, amount) {
            Some(k1) => return Some(SubsetSumCover::One(k1)),
            None => {}
        }
        match self.find_two_coin_exact_cover(p, amount) {
            Some((k1, k2)) => return Some(SubsetSumCover::Two(k1, k2)),
            None => {}
        }
        match self.find_three_coin_exact_cover(p, amount) {
            Some((k1, k2, k3)) => return Some(SubsetSumCover::Three(k1, k2, k3)),
            None => {}
        }
        match self.find_four_coin_exact_cover(p, amount) {
            Some((k1, k2, k3, k4)) =>
                Some(SubsetSumCover::Four(k1, k2, k3, k4)),
            None => None,
        }
    }


    /// Tier-2 (split cover, §6.3): find any `Available` coin in purse `p`
    /// whose `coin_value(exp)` strictly exceeds `amount`. Such a coin can
    /// be split into two coins of strictly smaller exponent (one of which
    /// covers `amount`); the remainder becomes change. Returns the first
    /// matching coin in Vec order, or `None` if none exists.
    ///
    /// Quint analog: the witness for `existsSplitCover(p, amount)`.
    pub fn find_split_cover_coin(&self, p: PurseId, amount: u64)
        -> (res: Option<(PurseId, u64)>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(key) =>
                    self.coins().dom().contains(key)
                    && key.0 == p
                    && self.coins()[key].state == CoinState::Available
                    && coin_value(self.coins()[key].exponent) > amount as nat,
                None =>
                    forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        && k.0 == p
                        && self.coins()[k].state == CoinState::Available
                        ==> coin_value(self.coins()[k].exponent) <= amount as nat,
            },
    {
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                forall|jj: int| 0 <= jj < j ==>
                    (#[trigger] self.coins@[jj]).purse != p
                    || self.coins@[jj].state != CoinState::Available
                    || coin_value(self.coins@[jj].exponent) <= amount as nat,
            decreases self.coins.len() - j,
        {
            let is_avail = matches!(self.coins[j].state, CoinState::Available);
            proof {
                let coin_key = (self.coins@[j as int].purse, self.coins@[j as int].idx);
                assert(self.spec_coins@.dom().contains(coin_key));
                assert(self.spec_coins@[coin_key] == self.coins@[j as int]);
                assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
            }
            let value: u64 = pow2_u64_exec(self.coins[j].exponent);
            if self.coins[j].purse == p && is_avail && value > amount {
                let key = (self.coins[j].purse, self.coins[j].idx);
                proof {
                    assert(self.spec_coins@.dom().contains(key));
                }
                return Some(key);
            }
            j = j + 1;
        }
        proof {
            assert forall|k: (PurseId, u64)|
                #[trigger] self.coins().dom().contains(k)
                && k.0 == p
                && self.coins()[k].state == CoinState::Available
                implies coin_value(self.coins()[k].exponent) <= amount as nat
            by {
                let w = choose|jj: int|
                    0 <= jj < self.coins@.len()
                    && #[trigger] self.coins@[jj].purse == k.0
                    && self.coins@[jj].idx == k.1;
                assert(self.coins@[w].purse == p);
                assert(self.coins@[w].state == self.coins()[k].state);
                assert(self.coins@[w].exponent == self.coins()[k].exponent);
            }
        }
        None
    }


    /// Composite single-coin selector (§6.3 tier-1 + tier-2, single-coin
    /// case). Tries the exact-cover branch first (Quint
    /// `existsExactCover`'s single-coin witness), then falls back to the
    /// split-cover branch (Quint `existsSplitCover`'s witness). Returns
    /// `None` only when no single `Available` coin in `p` has value at
    /// least `amount`.
    ///
    /// Multi-coin exact subset-sum (Quint
    /// `selectExactCoverDeterministic`) and tier-3 entry-supplemented
    /// cover are not yet wired in; their dedicated exec implementations
    /// will compose with this in later phases.
    pub fn select_single_coin_cover(&self, p: PurseId, amount: u64)
        -> (res: Option<CoinSelection>)
        requires
            self.invariant(),
        ensures
            match res {
                Some(CoinSelection::Exact { coin: key }) =>
                    self.coins().dom().contains(key)
                    && key.0 == p
                    && self.coins()[key].state == CoinState::Available
                    && coin_value(self.coins()[key].exponent) == amount as nat,
                Some(CoinSelection::Split { coin: key }) =>
                    self.coins().dom().contains(key)
                    && key.0 == p
                    && self.coins()[key].state == CoinState::Available
                    && coin_value(self.coins()[key].exponent) > amount as nat,
                None =>
                    forall|k: (PurseId, u64)|
                        #[trigger] self.coins().dom().contains(k)
                        && k.0 == p
                        && self.coins()[k].state == CoinState::Available
                        ==> coin_value(self.coins()[k].exponent) < amount as nat,
            },
    {
        match self.find_exact_single_coin(p, amount) {
            Some(key) => Some(CoinSelection::Exact { coin: key }),
            None => match self.find_split_cover_coin(p, amount) {
                Some(key) => Some(CoinSelection::Split { coin: key }),
                None => None,
            },
        }
    }


    /// Greedy multi-coin selection. Scans `Available` coins in purse `p` in
    /// Vec order, accumulating until the running total meets or exceeds
    /// `requested`. Returns the selected key list, or `None` if the total
    /// Available value in `p` is insufficient.
    ///
    /// **Pilot scope:** this is NOT the design's three-tier exact-cover
    /// selection (§6.3). Greedy may overshoot `requested` (returning more
    /// value than asked). Real exact-subset-sum requires powerset
    /// enumeration with lex-min disambiguation (Quint
    /// `selectExactCoverDeterministic`); deferred.
    pub fn select_coins_for_amount(&self, p: PurseId, requested: u64)
        -> (res: Option<Vec<(PurseId, u64)>>)
        requires
            self.invariant(),
            self.coins@.len() <= (u64::MAX / 1073741824) as nat,
            // Bound `requested` so `accumulated + value` doesn't overflow when
            // `accumulated < requested` and `value <= 2^30`.
            requested <= u64::MAX - 1073741824,
            requested >= 1,
        ensures
            match res {
                Some(keys) => {
                    &&& forall|i: int| 0 <= i < keys@.len() ==>
                            self.coins().dom().contains(#[trigger] keys@[i])
                            && keys@[i].0 == p
                            && self.coins()[keys@[i]].state == CoinState::Available
                    &&& sum_of_coin_values(self.coins(), keys@) >= requested as nat
                },
                None =>
                    sum_avail_prefix(self.coins@, p, self.coins@.len() as nat)
                        < requested as nat,
            },
    {
        let mut selected: Vec<(PurseId, u64)> = Vec::new();
        let mut accumulated: u64 = 0;
        let mut j: usize = 0;
        while j < self.coins.len()
            invariant
                0 <= j <= self.coins.len(),
                self.invariant(),
                self.coins@.len() <= (u64::MAX / 1073741824) as nat,
                requested <= u64::MAX - 1073741824,
                accumulated < requested,
                accumulated as nat == sum_avail_prefix(self.coins@, p, j as nat),
                accumulated as nat == sum_of_coin_values(self.coins(), selected@),
                forall|i: int| 0 <= i < selected@.len() ==>
                    self.coins().dom().contains(#[trigger] selected@[i])
                    && selected@[i].0 == p
                    && self.coins()[selected@[i]].state == CoinState::Available,
            decreases self.coins.len() - j,
        {
            let is_avail = matches!(self.coins[j].state, CoinState::Available);
            proof {
                // Bound the per-step delta for cumulative overflow safety.
                // Per-step coin value is at most coin_value(MAX_EXPONENT) = 2^30.
                let coin_key = (self.coins@[j as int].purse, self.coins@[j as int].idx);
                assert(self.spec_coins@.dom().contains(coin_key));
                assert(self.spec_coins@[coin_key] == self.coins@[j as int]);
                assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
                lemma_pow2_at_30();
                lemma_pow2_monotone(self.coins@[j as int].exponent as nat,
                                    MAX_EXPONENT as nat);
                assert(sum_avail_prefix(self.coins@, p, (j + 1) as nat)
                    <= sum_avail_prefix(self.coins@, p, j as nat) + 1073741824);
            }
            if self.coins[j].purse == p && is_avail {
                let key = (self.coins[j].purse, self.coins[j].idx);
                proof {
                    assert(self.spec_coins@.dom().contains(key));
                    assert(self.spec_coins@[key] == self.coins@[j as int]);
                    assert(self.coins@[j as int].exponent <= MAX_EXPONENT);
                }
                let value: u64 = pow2_u64_exec(self.coins[j].exponent);
                let ghost selected_before = selected@;
                selected.push(key);
                assert(value <= 1073741824);
                assert(accumulated < requested);
                assert(requested <= u64::MAX - 1073741824);
                accumulated = accumulated + value;
                proof {
                    // (l) gives ghost-map record matches Vec entry.
                    assert(self.spec_coins@.dom().contains(key));
                    assert(self.coins()[key].state == CoinState::Available);
                    // Append-decomposition for sum_of_coin_values.
                    assert(selected@ =~= selected_before.push(key));
                    assert(selected@.subrange(0, selected_before.len() as int)
                        =~= selected_before);
                    assert(sum_of_coin_values(self.coins(), selected@)
                        == sum_of_coin_values(self.coins(), selected_before)
                            + coin_value(self.coins()[key].exponent));
                }
                if accumulated >= requested {
                    return Some(selected);
                }
            }
            j = j + 1;
        }
        None
    }

}

} // verus!
