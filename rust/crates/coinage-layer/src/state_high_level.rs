//! High-level ops: transfer, rebalance, export/import, split, unload, top-up, reserve.

use vstd::prelude::*;

use crate::*;

verus! {

impl State {
    /// Composite operation: `transfer(from, to, min_exp)` selects an
    /// `Available` coin in purse `from` with `exponent >= min_exp`, walks
    /// it through `PendingSpend → Spent` (simulating chain settlement),
    /// then mints a fresh coin in purse `to` with the same exponent.
    ///
    /// Returns the new coin's `(to, idx)` key, or `None` if no suitable
    /// coin was available in `from`.
    pub fn transfer(&mut self, from: PurseId, to: PurseId, min_exp: u8)
        -> (res: Option<(PurseId, u64)>)
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(to),
            old(self).purses()[to].next_coin_idx < u64::MAX,
            old(self).next_age < u64::MAX,
            old(self).events@.len() + 2 <= u64::MAX as nat,
        ensures
            final(self).invariant(),
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).entries() == old(self).entries(),
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            match res {
                Some(new_key) =>
                    new_key.0 == to
                    && new_key.1 == old(self).purses()[to].next_coin_idx
                    && final(self).next_age == old(self).next_age + 1
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
                            .push(Event::CoinSpent {
                                purse: from,
                                exponent: old(self).coins()[src_key].exponent,
                            })
                            .push(Event::CoinAvailable {
                                purse: to,
                                exponent: old(self).coins()[src_key].exponent,
                            })),
                None =>
                    // No Available coin in `from` met the threshold.
                    final(self).next_age == old(self).next_age
                    && final(self).purses() == old(self).purses()
                    && final(self).coins() == old(self).coins()
                    && final(self).events@ == old(self).events@
                    && (forall|k: (PurseId, u64)|
                        #[trigger] old(self).coins().dom().contains(k)
                        && k.0 == from
                        && old(self).coins()[k].state == CoinState::Available
                        ==> old(self).coins()[k].exponent < min_exp),
            },
    {
        match self.select_coin(from, min_exp) {
            None => None,
            Some(key) => {
                let exp = self.read_coin_exponent(key);
                self.mark_coin_pending_spend(key);
                self.mark_coin_spent(key);
                let new_key = self.add_coin(to, exp);
                self.mark_coin_observed(new_key);
                Some(new_key)
            }
        }
    }


    /// Export a coin: the layer surrenders custody of a specific
    /// `Available` coin (the host has handed its secret to an external
    /// party). The coin transitions Available → PendingSpend → Spent;
    /// no new coin is minted. Quint analog: `exportCoin`.
    pub fn export_coin(&mut self, key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::Available,
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
                state: CoinState::Spent,
            }),
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::CoinSpent {
                purse: key.0,
                exponent: old(self).coins()[key].exponent,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        self.mark_coin_pending_spend(key);
        self.mark_coin_spent(key);
    }


    /// Import a coin: an external (account, secret) pair becomes a
    /// fresh `Available` coin in purse `p` carrying that account.
    /// Quint analog: `importCoin`. The coin skips the Pending →
    /// Available chain-observation gap (the host has already verified
    /// the coin exists on-chain via the imported secret).
    pub fn import_coin(&mut self, p: PurseId, exponent: u8, account: u64)
        -> (key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(p),
            old(self).purses()[p].next_coin_idx < u64::MAX,
            old(self).next_age < u64::MAX,
            old(self).events@.len() < u64::MAX as nat,
            exponent <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            key.0 == p,
            key.1 == old(self).purses()[p].next_coin_idx,
            final(self).coins() == old(self).coins().insert(key, CoinRec {
                purse: p,
                idx: key.1,
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
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).events@ == old(self).events@.push(Event::CoinAvailable {
                purse: p,
                exponent,
            }),
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let key = self.add_coin_with_account(p, exponent, account);
        self.mark_coin_observed(key);
        key
    }


    /// Rebalance: move one specific `Available` coin from purse `src` to
    /// purse `dst`. The source coin transitions Available → PendingSpend
    /// → Spent; a fresh `Available` coin with the same exponent is minted
    /// in `dst`'s namespace. Quint §6.1.3 `rebalancePurse`.
    ///
    /// Differs from `transfer` in that the caller selects the specific
    /// coin (no min-exp search), and `src != dst` is required.
    #[allow(unused_variables)]
    pub fn rebalance(&mut self, src: PurseId, dst: PurseId, key: (PurseId, u64))
        -> (new_key: (PurseId, u64))
        requires
            old(self).invariant(),
            src != dst,
            key.0 == src,
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::Available,
            old(self).purses().dom().contains(dst),
            old(self).purses()[dst].next_coin_idx < u64::MAX,
            old(self).events@.len() + 2 <= u64::MAX as nat,
            old(self).next_age < u64::MAX,
        ensures
            final(self).invariant(),
            new_key.0 == dst,
            new_key.1 == old(self).purses()[dst].next_coin_idx,
            final(self).coins() == old(self).coins()
                .insert(key, CoinRec {
                    purse: old(self).coins()[key].purse,
                    idx: old(self).coins()[key].idx,
                    exponent: old(self).coins()[key].exponent,
                    age: old(self).coins()[key].age,
                    account: old(self).coins()[key].account,
                    state: CoinState::Spent,
                })
                .insert(new_key, CoinRec {
                    purse: dst,
                    idx: new_key.1,
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
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).events@ == old(self).events@
                .push(Event::CoinSpent {
                    purse: src,
                    exponent: old(self).coins()[key].exponent,
                })
                .push(Event::CoinAvailable {
                    purse: dst,
                    exponent: old(self).coins()[key].exponent,
                }),
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let exp = self.read_coin_exponent(key);
        self.mark_coin_pending_spend(key);
        self.mark_coin_spent(key);
        let new_key = self.add_coin(dst, exp);
        self.mark_coin_observed(new_key);
        new_key
    }


    /// Split a single `Available` coin into a batch of fresh coins in the
    /// same purse, one per element of `new_exponents`. Quint analog: the
    /// Tier-2 split step of three-tier selection.
    ///
    /// The source coin walks Available → PendingSpend → Spent. The new
    /// coins arrive in `Pending` state (chain settlement is simulated by
    /// the existing `add_coin` semantics; the caller invokes
    /// `mark_coin_observed` on each later if needed).
    ///
    /// **Pilot scope:** no value-preservation check between the source
    /// coin's exponent and the sum of new exponents. The design requires
    /// `sum(coin_value(new_exp)) == coin_value(old_exp)`; verifying this
    /// requires the real `2^exp` semantics (deferred — see stage 7c).
    pub fn split_coin(&mut self, key: (PurseId, u64), new_exponents: Vec<u8>)
        requires
            old(self).invariant(),
            old(self).coins().dom().contains(key),
            old(self).coins()[key].state == CoinState::Available,
            old(self).purses().dom().contains(key.0),
            old(self).purses()[key.0].next_coin_idx as nat + new_exponents@.len()
                <= u64::MAX as nat,
            old(self).next_age as nat + new_exponents@.len() <= u64::MAX as nat,
            old(self).events@.len() < u64::MAX as nat,
            forall|j: int| 0 <= j < new_exponents@.len() ==>
                (#[trigger] new_exponents@[j]) <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            // Source coin: same key, state flipped to Spent, other fields preserved.
            final(self).coins()[key] == (CoinRec {
                purse: old(self).coins()[key].purse,
                idx: old(self).coins()[key].idx,
                exponent: old(self).coins()[key].exponent,
                age: old(self).coins()[key].age,
                account: old(self).coins()[key].account,
                state: CoinState::Spent,
            }),
            // New coins: full records matching the bulk-mint pattern.
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
            // Coins domain: old keys (each preserving its old record, except
            // for `key` which is now Spent) plus the new contiguous range.
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
            // Purses: only key.0's next_coin_idx advances.
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
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).events@ == old(self).events@.push(Event::CoinSpent {
                purse: key.0,
                exponent: old(self).coins()[key].exponent,
            }),
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let ghost old_coins = self.coins();
        self.mark_coin_pending_spend(key);
        self.mark_coin_spent(key);
        let ghost pre_top_up_coins = self.coins();
        let ghost pre_top_up_purses = self.purses();
        self.top_up_purse(key.0, new_exponents);
        proof {
            // top_up_purse preserves existing keys: key is still in dom with
            // its Spent state.
            assert(pre_top_up_coins.dom().contains(key));
            assert(pre_top_up_coins[key].state == CoinState::Spent);
            // For every old key k != key: the two mark_coin_* calls preserve
            // it (they only insert at `key`), and top_up_purse preserves all
            // existing keys.
            assert forall|k: (PurseId, u64)| #[trigger] old_coins.dom().contains(k)
                && k != key
                implies self.coins()[k] == old_coins[k]
            by {
                assert(pre_top_up_coins.dom().contains(k));
                assert(pre_top_up_coins[k] == old_coins[k]);
            }
        }
    }


    /// Tier-3 unload: consume a `Ready` recycler entry to mint a fresh
    /// `Available` coin in the same purse. The entry walks
    /// `LocalAvailable → LocalLockedFor → LocalConsumed`; the new coin
    /// walks `Pending → Available` via observation.
    ///
    /// Quint analog: the local-state effect of `startExternalOffload`
    /// (without the external account / chain-side bookkeeping).
    pub fn unload_via_entry(&mut self, key: (PurseId, u64), handle: OpHandle)
        -> (new_coin_key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).entries().dom().contains(key),
            old(self).entries()[key].local == EntryLocal::LocalAvailable,
            old(self).entries()[key].on_chain == EntryOnChain::Ready,
            old(self).purses().dom().contains(key.0),
            old(self).purses()[key.0].next_coin_idx < u64::MAX,
            old(self).next_age < u64::MAX,
            old(self).events@.len() < u64::MAX as nat,
        ensures
            final(self).invariant(),
            new_coin_key.0 == key.0,
            new_coin_key.1 == old(self).purses()[key.0].next_coin_idx,
            final(self).entries() == old(self).entries().insert(key, EntryRec {
                local: EntryLocal::LocalConsumed,
                ..old(self).entries()[key]
            }),
            final(self).coins() == old(self).coins().insert(new_coin_key, CoinRec {
                purse: key.0,
                idx: new_coin_key.1,
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
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).events@ == old(self).events@.push(Event::CoinAvailable {
                purse: key.0,
                exponent: old(self).entries()[key].exponent,
            }),
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let exp = self.read_entry_exponent(key);
        self.set_entry_local(key, EntryLocal::LocalLockedFor(handle));
        self.set_entry_local(key, EntryLocal::LocalConsumed);
        let ghost post_consume_entries = self.entries();
        let new_key = self.add_coin(key.0, exp);
        self.mark_coin_observed(new_key);
        proof {
            // add_coin and mark_coin_observed preserve entries (sibling-field
            // stability). The entry's local==Consumed survives unchanged.
            assert(self.entries() == post_consume_entries);
            assert(post_consume_entries.dom().contains(key));
            assert(post_consume_entries[key].local == EntryLocal::LocalConsumed);
        }
        new_key
    }


    /// Top-up via recycler entry (Quint `topUp`): allocate a fresh
    /// recycler entry of `exponent` in purse `p`, in the `Waiting` /
    /// `LocalAvailable` state. Caller supplies the chain-side
    /// bookkeeping (`member_key`, `allocated_at`, `ready_at`,
    /// `ring_idx`) — these come from the host's chain abstraction
    /// (e.g. derive `member_key` from the purse's anonymity-ring
    /// secret, `ready_at = allocated_at + JitterMax`).
    ///
    /// This is the entry-side bottom-layer effect of the design §8.2
    /// top-up — funds entering via a recycler ring rather than as
    /// direct coins. Pair with `set_entry_on_chain` once the chain
    /// confirms ring-membership floor → entry becomes `Ready`.
    pub fn top_up_via_entry(
        &mut self,
        p: PurseId,
        exponent: u8,
        member_key: u64,
        allocated_at: u64,
        ready_at: u64,
        ring_idx: u64,
    ) -> (key: (PurseId, u64))
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(p),
            old(self).purses()[p].next_entry_idx < u64::MAX,
            old(self).events@.len() < u64::MAX as nat,
            exponent <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            key.0 == p,
            key.1 == old(self).purses()[p].next_entry_idx,
            final(self).entries() == old(self).entries().insert(key, EntryRec {
                purse: p,
                idx: key.1,
                exponent,
                on_chain: EntryOnChain::Waiting,
                local: EntryLocal::LocalAvailable,
                member_key,
                allocated_at,
                ready_at,
                ring_idx,
            }),
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            final(self).purses().dom() =~= old(self).purses().dom(),
            final(self).purses()[p].id == p,
            final(self).purses()[p].name == old(self).purses()[p].name,
            final(self).purses()[p].next_coin_idx
                == old(self).purses()[p].next_coin_idx,
            final(self).purses()[p].next_entry_idx
                == old(self).purses()[p].next_entry_idx + 1,
            forall|q: PurseId| q != p && #[trigger] old(self).purses().dom().contains(q)
                ==> final(self).purses()[q] == old(self).purses()[q],
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).events@ == old(self).events@.push(Event::EntryAllocated {
                purse: p,
                exponent,
            }),
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let key = self.add_entry_with_meta(
            p,
            exponent,
            EntryOnChain::Waiting,
            EntryLocal::LocalAvailable,
            member_key,
            allocated_at,
            ready_at,
            ring_idx,
        );
        self.emit_event(Event::EntryAllocated {
            purse: p,
            exponent,
        });
        key
    }


    /// Top-up: allocate `exp_seq.len()` fresh coins in purse `p`, one per
    /// exponent in `exp_seq` (in order). Each call to `add_coin` allocates the
    /// next available coin index, so the resulting coin keys are
    /// `(p, old_next_coin_idx)`, `(p, old_next_coin_idx + 1)`, …
    ///
    /// This is the design §8.2 top-up reduced to its bottom-layer effect:
    /// produce a batch of new coins under the purse's namespace. The chain
    /// interaction, fee handling, and `FundingOrigin` plumbing are deferred.
    pub fn top_up_purse(&mut self, p: PurseId, exp_seq: Vec<u8>)
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(p),
            old(self).purses()[p].next_coin_idx as nat + exp_seq@.len() <= u64::MAX as nat,
            old(self).next_age as nat + exp_seq@.len() <= u64::MAX as nat,
            forall|j: int| 0 <= j < exp_seq@.len() ==>
                (#[trigger] exp_seq@[j]) <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            final(self).purses().dom() =~= old(self).purses().dom(),
            final(self).purses()[p].next_coin_idx
                == old(self).purses()[p].next_coin_idx + exp_seq@.len(),
            final(self).purses()[p].id == p,
            final(self).purses()[p].name == old(self).purses()[p].name,
            final(self).purses()[p].next_entry_idx == old(self).purses()[p].next_entry_idx,
            forall|q: PurseId| q != p && #[trigger] old(self).purses().dom().contains(q)
                ==> final(self).purses()[q] == old(self).purses()[q],
            // Existing coins preserved.
            forall|k: (PurseId, u64)| #[trigger] old(self).coins().dom().contains(k)
                ==> final(self).coins().dom().contains(k)
                    && final(self).coins()[k] == old(self).coins()[k],
            // New coin keys are in the dom; record fields match the request.
            forall|j: int| 0 <= j < exp_seq@.len() ==>
                #[trigger] final(self).coins().dom().contains(
                    (p, (old(self).purses()[p].next_coin_idx + j) as u64)
                )
                && final(self).coins()[
                    (p, (old(self).purses()[p].next_coin_idx + j) as u64)
                ].exponent == exp_seq@[j],
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).entries() == old(self).entries(),
            final(self).entries@ == old(self).entries@,
            final(self).spec_entries@ == old(self).spec_entries@,
            final(self).events@ == old(self).events@,
            final(self).next_age == old(self).next_age + exp_seq@.len(),
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
            // Domain-equality form: every key in the final coins map is
            // either an old key (with its old record) or one of the new
            // (p, old_next + j) keys (with its exp_seq[j] record).
            final(self).coins().dom() =~= old(self).coins().dom().union(
                Set::new(|k: (PurseId, u64)|
                    k.0 == p
                    && (old(self).purses()[p].next_coin_idx as int) <= (k.1 as int)
                    && (k.1 as int) < (old(self).purses()[p].next_coin_idx as int)
                                       + exp_seq@.len() as int)
            ),
            forall|j: int| 0 <= j < exp_seq@.len() ==>
                #[trigger] final(self).coins()[
                    (p, (old(self).purses()[p].next_coin_idx + j) as u64)
                ] == (CoinRec {
                    purse: p,
                    idx: (old(self).purses()[p].next_coin_idx + j) as u64,
                    exponent: exp_seq@[j],
                    state: CoinState::Pending,
                    age: (old(self).next_age + j) as u64,
                    account: 0,
                }),
    {
        let ghost old_p_next = old(self).purses()[p].next_coin_idx;
        let ghost old_next_age = old(self).next_age;
        let ghost old_purses_map = old(self).purses();
        let ghost old_coins_map = old(self).coins();
        let ghost old_operations_map = old(self).operations();
        let ghost old_operations_vec = old(self).operations@;
        let ghost old_spec_operations = old(self).spec_operations@;
        let ghost old_entries_map = old(self).entries();
        let ghost old_entries_vec = old(self).entries@;
        let ghost old_spec_entries = old(self).spec_entries@;
        let ghost old_next_handle = old(self).next_handle;
        let ghost old_events = old(self).events@;
        let n = exp_seq.len();

        let mut k: usize = 0;
        while k < n
            invariant
                0 <= k <= n,
                n == exp_seq@.len(),
                self.invariant(),
                self.events@ == old_events,
                forall|j: int| 0 <= j < exp_seq@.len() ==>
                    (#[trigger] exp_seq@[j]) <= MAX_EXPONENT,
                self.purses().dom() =~= old_purses_map.dom(),
                old_purses_map.dom().contains(p),
                self.purses()[p].next_coin_idx == old_p_next + k as nat,
                self.purses()[p].id == p,
                self.purses()[p].name == old_purses_map[p].name,
                self.purses()[p].next_entry_idx == old_purses_map[p].next_entry_idx,
                old_p_next == old_purses_map[p].next_coin_idx,
                old_p_next as nat + n as nat <= u64::MAX as nat,
                self.next_age == old_next_age + k as nat,
                old_next_age == old(self).next_age,
                old_next_age as nat + n as nat <= u64::MAX as nat,
                self.operations() == old_operations_map,
                self.operations@ == old_operations_vec,
                self.spec_operations@ == old_spec_operations,
                self.next_handle == old_next_handle,
                self.entries() == old_entries_map,
                self.entries@ == old_entries_vec,
                self.spec_entries@ == old_spec_entries,
                self.fee_balance == old(self).fee_balance,
                self.next_extrinsic_id == old(self).next_extrinsic_id,
                self.paid_ring_membership == old(self).paid_ring_membership,
                self.total_in == old(self).total_in,
                self.total_out == old(self).total_out,
                self.tokens@ == old(self).tokens@,
                self.chain_coins@ == old(self).chain_coins@,
                self.chain_entries@ == old(self).chain_entries@,
                // Cumulative new coins so far have their full records.
                forall|j: int| 0 <= j < k as int ==>
                    #[trigger] self.coins()[(p, (old_p_next + j) as u64)]
                        == (CoinRec {
                            purse: p,
                            idx: (old_p_next + j) as u64,
                            exponent: exp_seq@[j],
                            state: CoinState::Pending,
                            age: (old_next_age + j) as u64,
                            account: 0,
                        }),
                // Cumulative new-key domain.
                self.coins().dom() =~= old_coins_map.dom().union(
                    Set::new(|kk: (PurseId, u64)|
                        kk.0 == p
                        && (old_p_next as int) <= (kk.1 as int)
                        && (kk.1 as int) < (old_p_next as int) + k as int)
                ),
                old_operations_map == old(self).operations(),
                old_operations_vec == old(self).operations@,
                old_spec_operations == old(self).spec_operations@,
                old_next_handle == old(self).next_handle,
                old_entries_map == old(self).entries(),
                old_entries_vec == old(self).entries@,
                old_spec_entries == old(self).spec_entries@,
                forall|q: PurseId| q != p && #[trigger] old_purses_map.dom().contains(q)
                    ==> self.purses()[q] == old_purses_map[q],
                forall|key: (PurseId, u64)| #[trigger] old_coins_map.dom().contains(key)
                    ==> self.coins().dom().contains(key)
                        && self.coins()[key] == old_coins_map[key],
                forall|j: int| 0 <= j < k as int ==>
                    #[trigger] self.coins().dom().contains((p, (old_p_next + j) as u64))
                    && self.coins()[(p, (old_p_next + j) as u64)].exponent == exp_seq@[j],
            decreases n - k,
        {
            let exp = exp_seq[k];
            let ghost prev_next_coin_idx = self.purses()[p].next_coin_idx;
            let ghost pre_coins = self.coins();
            assert(prev_next_coin_idx == old_p_next + k as nat);
            assert(prev_next_coin_idx < u64::MAX);
            #[allow(unused_variables)]
            let new_key = self.add_coin(p, exp);
            proof {
                assert(new_key == (p, (old_p_next + k as nat) as u64));
                // Forall j in [0, k+1), the expected key is in coins.dom.
                // j == k is the just-added coin; j < k is an existing coin
                // that survives `insert(new_key, _)` since keys differ.
                assert forall|j: int| 0 <= j < (k + 1) as int implies
                    #[trigger] self.coins().dom().contains((p, (old_p_next + j) as u64))
                    && self.coins()[(p, (old_p_next + j) as u64)].exponent == exp_seq@[j]
                by {
                    let nk = (p, (old_p_next + j) as u64);
                    if j == k as int {
                        assert(nk == new_key);
                        assert(self.coins()[new_key].exponent == exp);
                        assert(exp == exp_seq@[k as int]);
                    } else {
                        assert(j < k as int);
                        assert(pre_coins.dom().contains(nk));
                        assert(pre_coins[nk].exponent == exp_seq@[j]);
                        assert(nk.1 != new_key.1);
                    }
                }
            }
            k += 1;
        }
    }


    /// Reserve: allocate `exp_seq.len()` fresh recycler entries in purse `p`,
    /// one per exponent in `exp_seq` (in order). Mirror of `top_up_purse` for
    /// the entry side. New entries start in `(on_chain=Waiting,
    /// local=LocalAvailable)`.
    pub fn reserve_entries(&mut self, p: PurseId, exp_seq: Vec<u8>)
        requires
            old(self).invariant(),
            old(self).purses().dom().contains(p),
            old(self).purses()[p].next_entry_idx as nat + exp_seq@.len() <= u64::MAX as nat,
            forall|j: int| 0 <= j < exp_seq@.len() ==>
                (#[trigger] exp_seq@[j]) <= MAX_EXPONENT,
        ensures
            final(self).invariant(),
            final(self).purses().dom() =~= old(self).purses().dom(),
            final(self).purses()[p].next_entry_idx
                == old(self).purses()[p].next_entry_idx + exp_seq@.len(),
            final(self).purses()[p].id == p,
            final(self).purses()[p].name == old(self).purses()[p].name,
            final(self).purses()[p].next_coin_idx == old(self).purses()[p].next_coin_idx,
            forall|q: PurseId| q != p && #[trigger] old(self).purses().dom().contains(q)
                ==> final(self).purses()[q] == old(self).purses()[q],
            // Coins entirely untouched.
            final(self).coins() == old(self).coins(),
            final(self).coins@ == old(self).coins@,
            // Existing entries preserved.
            forall|k: (PurseId, u64)| #[trigger] old(self).entries().dom().contains(k)
                ==> final(self).entries().dom().contains(k)
                    && final(self).entries()[k] == old(self).entries()[k],
            // New entry keys are in the dom; full records match the request.
            forall|j: int| 0 <= j < exp_seq@.len() ==>
                #[trigger] final(self).entries()[
                    (p, (old(self).purses()[p].next_entry_idx + j) as u64)
                ] == (EntryRec {
                    purse: p,
                    idx: (old(self).purses()[p].next_entry_idx + j) as u64,
                    exponent: exp_seq@[j],
                    on_chain: EntryOnChain::Waiting,
                    local: EntryLocal::LocalAvailable,
                    member_key: 0,
                    allocated_at: 0,
                    ready_at: 0,
                    ring_idx: 0,
                }),
            // Domain-union form: old keys plus the new contiguous range.
            final(self).entries().dom() =~= old(self).entries().dom().union(
                Set::new(|k: (PurseId, u64)|
                    k.0 == p
                    && (old(self).purses()[p].next_entry_idx as int) <= (k.1 as int)
                    && (k.1 as int) < (old(self).purses()[p].next_entry_idx as int)
                                       + exp_seq@.len() as int)
            ),
            final(self).operations() == old(self).operations(),
            final(self).operations@ == old(self).operations@,
            final(self).spec_operations@ == old(self).spec_operations@,
            final(self).next_handle == old(self).next_handle,
            final(self).next_age == old(self).next_age,
            final(self).events@ == old(self).events@,
            final(self).fee_balance == old(self).fee_balance,
            final(self).next_extrinsic_id == old(self).next_extrinsic_id,
            final(self).paid_ring_membership == old(self).paid_ring_membership,
            final(self).total_in == old(self).total_in,
            final(self).total_out == old(self).total_out,
            final(self).tokens@ == old(self).tokens@,
            final(self).chain_coins@ == old(self).chain_coins@,
            final(self).chain_entries@ == old(self).chain_entries@,
    {
        let ghost old_p_next = old(self).purses()[p].next_entry_idx;
        let ghost old_purses_map = old(self).purses();
        let ghost old_entries_map = old(self).entries();
        let n = exp_seq.len();

        let mut k: usize = 0;
        while k < n
            invariant
                0 <= k <= n,
                n == exp_seq@.len(),
                self.invariant(),
                forall|j: int| 0 <= j < exp_seq@.len() ==>
                    (#[trigger] exp_seq@[j]) <= MAX_EXPONENT,
                self.purses().dom() =~= old_purses_map.dom(),
                old_purses_map.dom().contains(p),
                self.purses()[p].next_entry_idx == old_p_next + k as nat,
                self.purses()[p].id == p,
                self.purses()[p].name == old_purses_map[p].name,
                self.purses()[p].next_coin_idx == old_purses_map[p].next_coin_idx,
                old_p_next == old_purses_map[p].next_entry_idx,
                old_p_next as nat + n as nat <= u64::MAX as nat,
                forall|q: PurseId| q != p && #[trigger] old_purses_map.dom().contains(q)
                    ==> self.purses()[q] == old_purses_map[q],
                self.coins() == old(self).coins(),
                self.coins@ == old(self).coins@,
                self.operations() == old(self).operations(),
                self.operations@ == old(self).operations@,
                self.spec_operations@ == old(self).spec_operations@,
                self.next_handle == old(self).next_handle,
                self.next_age == old(self).next_age,
                self.events@ == old(self).events@,
                self.fee_balance == old(self).fee_balance,
                self.next_extrinsic_id == old(self).next_extrinsic_id,
                self.paid_ring_membership == old(self).paid_ring_membership,
                self.total_in == old(self).total_in,
                self.total_out == old(self).total_out,
                self.tokens@ == old(self).tokens@,
                self.chain_coins@ == old(self).chain_coins@,
                self.chain_entries@ == old(self).chain_entries@,
                forall|key: (PurseId, u64)| #[trigger] old_entries_map.dom().contains(key)
                    ==> self.entries().dom().contains(key)
                        && self.entries()[key] == old_entries_map[key],
                forall|j: int| 0 <= j < k as int ==>
                    #[trigger] self.entries()[(p, (old_p_next + j) as u64)]
                        == (EntryRec {
                            purse: p,
                            idx: (old_p_next + j) as u64,
                            exponent: exp_seq@[j],
                            on_chain: EntryOnChain::Waiting,
                            local: EntryLocal::LocalAvailable,
                            member_key: 0,
                            allocated_at: 0,
                            ready_at: 0,
                            ring_idx: 0,
                        }),
                self.entries().dom() =~= old_entries_map.dom().union(
                    Set::new(|kk: (PurseId, u64)|
                        kk.0 == p
                        && (old_p_next as int) <= (kk.1 as int)
                        && (kk.1 as int) < (old_p_next as int) + k as int)
                ),
            decreases n - k,
        {
            let exp = exp_seq[k];
            let ghost prev_next_entry_idx = self.purses()[p].next_entry_idx;
            let ghost pre_entries = self.entries();
            assert(prev_next_entry_idx == old_p_next + k as nat);
            assert(prev_next_entry_idx < u64::MAX);
            #[allow(unused_variables)]
            let new_key = self.add_entry(
                p,
                exp,
                EntryOnChain::Waiting,
                EntryLocal::LocalAvailable,
            );
            proof {
                assert(new_key == (p, (old_p_next + k as nat) as u64));
                assert forall|j: int| 0 <= j < (k + 1) as int implies
                    #[trigger] self.entries().dom().contains((p, (old_p_next + j) as u64))
                    && self.entries()[(p, (old_p_next + j) as u64)].exponent == exp_seq@[j]
                by {
                    let nk = (p, (old_p_next + j) as u64);
                    if j == k as int {
                        assert(nk == new_key);
                        assert(self.entries()[new_key].exponent == exp);
                        assert(exp == exp_seq@[k as int]);
                    } else {
                        assert(j < k as int);
                        assert(pre_entries.dom().contains(nk));
                        assert(pre_entries[nk].exponent == exp_seq@[j]);
                        assert(nk.1 != new_key.1);
                    }
                }
            }
            k += 1;
        }
    }

}

} // verus!
