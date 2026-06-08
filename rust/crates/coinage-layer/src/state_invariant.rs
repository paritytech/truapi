//! Core `impl State` items: view accessors, invariant, init.

use vstd::prelude::*;

use crate::*;

verus! {

impl State {
    /// Spec view of the purse map.
    pub open spec fn purses(&self) -> Map<PurseId, PurseRecSpec> {
        self.spec_purses@
    }


    /// Spec view of the coin map.
    pub open spec fn coins(&self) -> Map<(PurseId, u64), CoinRec> {
        self.spec_coins@
    }


    /// Spec view of the recycler-entry map.
    pub open spec fn entries(&self) -> Map<(PurseId, u64), EntryRec> {
        self.spec_entries@
    }


    /// Spec view of the operations map.
    pub open spec fn operations(&self) -> Map<OpHandle, OperationRec> {
        self.spec_operations@
    }


    /// True iff some coin currently lives in purse `p`.
    pub open spec fn has_coin_in(&self, p: PurseId) -> bool {
        exists|k: (PurseId, u64)| #[trigger] self.coins().dom().contains(k) && k.0 == p
    }


    /// True iff some *live* (non-`Spent`) coin currently lives in purse `p`.
    pub open spec fn has_live_coin_in(&self, p: PurseId) -> bool {
        exists|k: (PurseId, u64)|
            #[trigger] self.coins().dom().contains(k)
                && k.0 == p
                && self.coins()[k].state != CoinState::Spent
    }


    /// Whether the allocator can still mint a fresh `PurseId`.
    pub open spec fn has_create_capacity(&self) -> bool {
        self.next_purse_id < u64::MAX
    }


    /// State well-formedness. Combines:
    ///   (a) ghost-map well-formedness (dom keys agree with `id` fields,
    ///       all ids below `next_purse_id`, MAIN_PURSE present),
    ///   (b) exec/spec refinement (Vec contents and ghost-map dom in
    ///       1-to-1 correspondence, no duplicates).
    pub open spec fn invariant(&self) -> bool {
        let m = self.spec_purses@;
        let v = self.purses@;
        &&& self.next_purse_id != MAIN_PURSE
        &&& m.dom().contains(MAIN_PURSE)
        &&& forall|p: PurseId| #[trigger] m.dom().contains(p) ==> m[p].id == p
        &&& forall|p: PurseId| #[trigger] m.dom().contains(p) ==> p < self.next_purse_id
        // exec → ghost: every Vec entry is in the map under its own id
        &&& forall|i: int| 0 <= i < v.len() ==> #[trigger] m.dom().contains(v[i].id)
        &&& forall|i: int| 0 <= i < v.len() ==> m[(#[trigger] v[i]).id] == v[i]@
        // ghost → exec: every map key has a matching Vec entry
        &&& forall|p: PurseId| #[trigger] m.dom().contains(p)
              ==> exists|i: int| 0 <= i < v.len() && #[trigger] v[i].id == p
        // no duplicate ids in the Vec
        &&& forall|i: int, j: int|
              0 <= i < v.len() && 0 <= j < v.len()
              && #[trigger] v[i].id == #[trigger] v[j].id ==> i == j
        // (i) coin key consistency: keyed by (purse, idx), record matches.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_coins@.dom().contains(k)
              ==> self.spec_coins@[k].purse == k.0 && self.spec_coins@[k].idx == k.1
        // (j) coin referential integrity: every coin's purse is a known purse.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_coins@.dom().contains(k)
              ==> m.dom().contains(k.0)
        // (k) coin idx is below the owning purse's allocator. Ensures
        //     `purses[p].next_coin_idx` is always a fresh coin index for p.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_coins@.dom().contains(k)
              ==> k.1 < m[k.0].next_coin_idx
        // (l) exec coin Vec → ghost: every Vec entry's (purse, idx) is in dom
        //     and matches the ghost record.
        &&& forall|i: int| 0 <= i < self.coins@.len() ==>
              #[trigger] self.spec_coins@.dom().contains(
                  (self.coins@[i].purse, self.coins@[i].idx)
              )
        &&& forall|i: int| 0 <= i < self.coins@.len() ==>
              self.spec_coins@[(#[trigger] self.coins@[i].purse, self.coins@[i].idx)]
                == self.coins@[i]
        // (m) ghost coin map → exec: every dom key has a Vec witness.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_coins@.dom().contains(k)
              ==> exists|i: int|
                    0 <= i < self.coins@.len()
                    && #[trigger] self.coins@[i].purse == k.0
                    && self.coins@[i].idx == k.1
        // (n) no duplicate (purse, idx) keys in the coin Vec.
        &&& forall|i: int, j: int|
              0 <= i < self.coins@.len() && 0 <= j < self.coins@.len()
              && (#[trigger] self.coins@[i]).purse == (#[trigger] self.coins@[j]).purse
              && self.coins@[i].idx == self.coins@[j].idx
              ==> i == j
        // (o) entry key consistency.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_entries@.dom().contains(k)
              ==> self.spec_entries@[k].purse == k.0
                  && self.spec_entries@[k].idx == k.1
        // (p) entry referential integrity: every entry's purse is in dom.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_entries@.dom().contains(k)
              ==> m.dom().contains(k.0)
        // (q) entry idx is below the owning purse's allocator.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_entries@.dom().contains(k)
              ==> k.1 < m[k.0].next_entry_idx
        // (r) exec entry Vec → ghost: every Vec entry's (purse, idx) is in dom
        //     and matches the ghost record.
        &&& forall|i: int| 0 <= i < self.entries@.len() ==>
              #[trigger] self.spec_entries@.dom().contains(
                  (self.entries@[i].purse, self.entries@[i].idx)
              )
        &&& forall|i: int| 0 <= i < self.entries@.len() ==>
              self.spec_entries@[(#[trigger] self.entries@[i].purse, self.entries@[i].idx)]
                == self.entries@[i]
        // (s) ghost entry map → exec: every dom key has a Vec witness.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_entries@.dom().contains(k)
              ==> exists|i: int|
                    0 <= i < self.entries@.len()
                    && #[trigger] self.entries@[i].purse == k.0
                    && self.entries@[i].idx == k.1
        // (t) no duplicate (purse, idx) keys in the entry Vec.
        &&& forall|i: int, j: int|
              0 <= i < self.entries@.len() && 0 <= j < self.entries@.len()
              && (#[trigger] self.entries@[i]).purse == (#[trigger] self.entries@[j]).purse
              && self.entries@[i].idx == self.entries@[j].idx
              ==> i == j
        // (u) operation key consistency: spec_operations[h].handle == h.
        &&& forall|h: OpHandle| #[trigger] self.spec_operations@.dom().contains(h)
              ==> self.spec_operations@[h].handle == h
        // (v) handle below allocator.
        &&& forall|h: OpHandle| #[trigger] self.spec_operations@.dom().contains(h)
              ==> h < self.next_handle
        // (w) operation refint to purses.
        &&& forall|h: OpHandle| #[trigger] self.spec_operations@.dom().contains(h)
              ==> m.dom().contains(self.spec_operations@[h].purse)
        // (x) exec operations Vec → ghost.
        &&& forall|i: int| 0 <= i < self.operations@.len() ==>
              #[trigger] self.spec_operations@.dom().contains(self.operations@[i].handle)
        &&& forall|i: int| 0 <= i < self.operations@.len() ==>
              self.spec_operations@[(#[trigger] self.operations@[i]).handle]
                == self.operations@[i]
        // (y) ghost → exec.
        &&& forall|h: OpHandle| #[trigger] self.spec_operations@.dom().contains(h)
              ==> exists|i: int|
                    0 <= i < self.operations@.len()
                    && #[trigger] self.operations@[i].handle == h
        // (z) no duplicate handles in operations Vec.
        &&& forall|i: int, j: int|
              0 <= i < self.operations@.len() && 0 <= j < self.operations@.len()
              && (#[trigger] self.operations@[i]).handle
                  == (#[trigger] self.operations@[j]).handle
              ==> i == j
        // (aa) every coin's exponent is bounded by MAX_EXPONENT. Foundation
        //      for real `2^exp` arithmetic safety (pow2_u64_exec(exp) doesn't
        //      overflow u64 only when exp <= 30 = MAX_EXPONENT).
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_coins@.dom().contains(k)
              ==> self.spec_coins@[k].exponent <= MAX_EXPONENT
        // (ab) every entry's exponent is bounded by MAX_EXPONENT.
        &&& forall|k: (PurseId, u64)| #[trigger] self.spec_entries@.dom().contains(k)
              ==> self.spec_entries@[k].exponent <= MAX_EXPONENT
        // (ac) every chain-mirror coin's exponent is bounded too. This lets
        //      restore_chain_coin reconstruct local state without losing the
        //      exponent bound.
        &&& forall|i: int| 0 <= i < self.chain_coins@.len()
              ==> (#[trigger] self.chain_coins@[i]).exponent <= MAX_EXPONENT
        // (ad) every chain-mirror entry's exponent is bounded.
        &&& forall|i: int| 0 <= i < self.chain_entries@.len()
              ==> (#[trigger] self.chain_entries@[i]).exponent <= MAX_EXPONENT
    }


    /// Initialize the layer with only the main purse and an empty coin map.
    pub fn init() -> (s: State)
        ensures
            s.invariant(),
            s.purses().dom() =~= set![MAIN_PURSE],
            s.purses()[MAIN_PURSE] == (PurseRecSpec {
                id: MAIN_PURSE,
                name: Seq::empty(),
                next_coin_idx: 0,
                next_entry_idx: 0,
            }),
            s.coins().dom() =~= Set::<(PurseId, u64)>::empty(),
            lock_refint(s.coins(), s.entries(), s.operations()),
    {
        let main_rec = PurseRec {
            id: MAIN_PURSE,
            name: Vec::new(),
            next_coin_idx: 0,
            next_entry_idx: 0,
        };
        let ghost main_spec = main_rec@;
        let mut purses: Vec<PurseRec> = Vec::new();
        purses.push(main_rec);
        let coins: Vec<CoinRec> = Vec::new();
        let entries: Vec<EntryRec> = Vec::new();
        let operations: Vec<OperationRec> = Vec::new();
        let s = State {
            purses,
            coins,
            entries,
            operations,
            next_purse_id: 1,
            next_handle: 0,
            next_age: 0,
            fee_balance: 0,
            next_extrinsic_id: 0,
            events: Vec::new(),
            paid_ring_membership: 0,
            total_in: 0,
            total_out: 0,
            tokens: Vec::new(),
            chain_coins: Vec::new(),
            chain_entries: Vec::new(),
            spec_purses: Ghost(Map::<PurseId, PurseRecSpec>::empty().insert(MAIN_PURSE, main_spec)),
            spec_coins: Ghost(Map::<(PurseId, u64), CoinRec>::empty()),
            spec_entries: Ghost(Map::<(PurseId, u64), EntryRec>::empty()),
            spec_operations: Ghost(Map::<OpHandle, OperationRec>::empty()),
        };
        assert(s.purses@.len() == 1);
        assert(s.purses@[0].id == MAIN_PURSE);
        assert(s.spec_purses@.dom() =~= set![MAIN_PURSE]);
        s
    }

}

} // verus!
