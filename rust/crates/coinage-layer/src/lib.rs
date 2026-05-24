//! Verus translation of the Coinage Layer Quint specification.
//!
//! Source-of-truth references:
//!   - Quint spec  : `docs/specs/coinage-layer.qnt`
//!   - Design doc  : `docs/design/coinage-layer.md`
//!
//! **Pilot scope.** Purse-lifecycle primitives only: `init`, `create_purse`,
//! `query_purse`. The full Quint state has many vars (`coins`, `entries`,
//! `operations`, `events`, `tokens`, ...); this crate models only the
//! `purses` map and a fresh-id allocator.
//!
//! **Encoding.** Exec storage is a `Vec<PurseRec>`. Contracts quantify over a
//! ghost spec map (`Ghost<Map<PurseId, PurseRecSpec>>`). The invariant ties
//! the two: every Vec entry is present in the ghost map under its own id,
//! every ghost-map key has a matching Vec entry, and there are no duplicate
//! ids in the Vec.

use vstd::prelude::*;

verus! {

/// Stable purse identifier (Quint `PurseId`, design §3.1).
pub type PurseId = u64;

/// Reserved identifier of the main purse (Quint `MAIN_PURSE`).
pub const MAIN_PURSE: PurseId = 0;

/// Executable purse record (mirrors Quint `PurseRec`, spec lines 89-94).
pub struct PurseRec {
    pub id: PurseId,
    pub name: Vec<u8>,
    pub next_coin_idx: u64,
    pub next_entry_idx: u64,
}

/// Spec-level twin of `PurseRec` used in contracts.
pub struct PurseRecSpec {
    pub id: PurseId,
    pub name: Seq<u8>,
    pub next_coin_idx: nat,
    pub next_entry_idx: nat,
}

impl PurseRec {
    /// Lift an executable record into its spec twin.
    pub open spec fn view(&self) -> PurseRecSpec {
        PurseRecSpec {
            id: self.id,
            name: self.name@,
            next_coin_idx: self.next_coin_idx as nat,
            next_entry_idx: self.next_entry_idx as nat,
        }
    }
}

/// Snapshot returned by `query_purse` (design §8.1 `PurseInfo`).
/// Pilot scope: `spendable`, `spendable_strict`, `pending` are always 0
/// (no coins/entries in state yet).
pub struct PurseInfo {
    pub id: PurseId,
    pub name: Vec<u8>,
    pub spendable: u64,
    pub spendable_strict: u64,
    pub pending: u64,
}

/// Layer error enum. Pilot subset of design §10.
pub enum Error {
    PurseNotFound(PurseId),
}

/// Layer state. Pilot scope: purses only.
pub struct State {
    purses: Vec<PurseRec>,
    next_purse_id: u64,
    #[allow(dead_code)]
    spec_purses: Ghost<Map<PurseId, PurseRecSpec>>,
}

impl State {
    /// Spec view of the purse map.
    pub closed spec fn purses(&self) -> Map<PurseId, PurseRecSpec> {
        self.spec_purses@
    }

    /// Whether the allocator can still mint a fresh `PurseId`.
    pub closed spec fn has_create_capacity(&self) -> bool {
        self.next_purse_id < u64::MAX
    }

    /// State well-formedness. Combines:
    ///   (a) ghost-map well-formedness (dom keys agree with `id` fields,
    ///       all ids below `next_purse_id`, MAIN_PURSE present),
    ///   (b) exec/spec refinement (Vec contents and ghost-map dom in
    ///       1-to-1 correspondence, no duplicates).
    pub closed spec fn invariant(&self) -> bool {
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
    }

    /// Initialize the layer with only the main purse.
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
        let s = State {
            purses,
            next_purse_id: 1,
            spec_purses: Ghost(Map::<PurseId, PurseRecSpec>::empty().insert(MAIN_PURSE, main_spec)),
        };
        assert(s.purses@.len() == 1);
        assert(s.purses@[0].id == MAIN_PURSE);
        assert(s.spec_purses@.dom() =~= set![MAIN_PURSE]);
        s
    }

    /// 6.1 `createPurse` (Quint lines 393-420; design §8.1 `create_purse`).
    ///
    /// Allocates a fresh `PurseId != MAIN_PURSE`, persists a new purse with
    /// the given `name`, returns the assigned id. Synchronous; no chain
    /// interaction.
    pub fn create_purse(&mut self, name: Vec<u8>) -> (new_id: PurseId)
        requires
            old(self).invariant(),
            old(self).has_create_capacity(),
        ensures
            final(self).invariant(),
            new_id != MAIN_PURSE,
            !old(self).purses().dom().contains(new_id),
            final(self).purses() == old(self).purses().insert(new_id, PurseRecSpec {
                id: new_id,
                name: name@,
                next_coin_idx: 0,
                next_entry_idx: 0,
            }),
    {
        let new_id = self.next_purse_id;
        let ghost old_v = self.purses@;
        let ghost old_m = self.spec_purses@;
        let rec = PurseRec {
            id: new_id,
            name,
            next_coin_idx: 0,
            next_entry_idx: 0,
        };
        let ghost rec_spec = rec@;

        // Every existing Vec entry's id is < new_id.
        proof {
            assert forall|i: int| 0 <= i < old_v.len() implies
                #[trigger] old_v[i].id < new_id
            by {
                assert(old_m.dom().contains(old_v[i].id));
            }
        }

        self.purses.push(rec);
        proof {
            self.spec_purses = Ghost(self.spec_purses@.insert(new_id, rec_spec));
        }
        self.next_purse_id = new_id + 1;

        proof {
            let new_v = self.purses@;
            let new_m = self.spec_purses@;
            let new_next = self.next_purse_id;

            // (a) next_purse_id != MAIN_PURSE
            assert(new_next != MAIN_PURSE);

            // (b) MAIN_PURSE in dom
            assert(new_m.dom().contains(MAIN_PURSE));

            // (c) forall p in dom. m[p].id == p
            assert forall|p: PurseId| #[trigger] new_m.dom().contains(p)
                implies new_m[p].id == p
            by {
                if p == new_id {
                    assert(new_m[new_id] == rec_spec);
                } else {
                    assert(old_m.dom().contains(p));
                }
            }

            // (d) forall p in dom. p < next_purse_id
            assert forall|p: PurseId| #[trigger] new_m.dom().contains(p)
                implies p < new_next
            by {
                if p == new_id {
                } else {
                    assert(old_m.dom().contains(p));
                }
            }

            // (e) every Vec entry's id is in dom
            assert(new_v == old_v.push(rec));
            assert forall|i: int| 0 <= i < new_v.len() implies
                new_m.dom().contains(#[trigger] new_v[i].id)
            by {
                if i < old_v.len() {
                    assert(new_v[i] == old_v[i]);
                    assert(old_m.dom().contains(old_v[i].id));
                } else {
                    assert(new_v[i].id == new_id);
                }
            }

            // (f) every Vec entry's spec view matches its dom entry
            assert forall|i: int| 0 <= i < new_v.len() implies
                new_m[(#[trigger] new_v[i]).id] == new_v[i]@
            by {
                if i < old_v.len() {
                    assert(new_v[i] == old_v[i]);
                    assert(old_v[i].id < new_id);
                    assert(old_m[old_v[i].id] == old_v[i]@);
                } else {
                    assert(new_v[i].id == new_id);
                    assert(new_v[i]@ == rec_spec);
                }
            }

            // (g) every dom key has a Vec witness
            assert forall|p: PurseId| #[trigger] new_m.dom().contains(p)
                implies exists|i: int| 0 <= i < new_v.len() && #[trigger] new_v[i].id == p
            by {
                if p == new_id {
                    let w = old_v.len() as int;
                    assert(0 <= w < new_v.len());
                    assert(new_v[w].id == new_id);
                } else {
                    assert(old_m.dom().contains(p));
                    let w = choose|i: int| 0 <= i < old_v.len() && #[trigger] old_v[i].id == p;
                    assert(new_v[w] == old_v[w]);
                }
            }

            // (h) no duplicates in Vec
            assert forall|i: int, j: int|
                0 <= i < new_v.len() && 0 <= j < new_v.len()
                && #[trigger] new_v[i].id == #[trigger] new_v[j].id
                implies i == j
            by {
                if i < old_v.len() && j < old_v.len() {
                } else if i == old_v.len() && j == old_v.len() {
                } else if i < old_v.len() {
                    assert(new_v[i] == old_v[i]);
                    assert(old_v[i].id < new_id);
                    assert(new_v[j].id == new_id);
                } else {
                    assert(new_v[j] == old_v[j]);
                    assert(old_v[j].id < new_id);
                    assert(new_v[i].id == new_id);
                }
            }
        }
        new_id
    }

    /// 6.1 `queryPurse` (Quint lines 603-612; design §8.1 `query_purse`).
    ///
    /// Returns a synchronous snapshot. In the pilot scope (no coins/entries),
    /// the three amount fields are always 0.
    pub fn query_purse(&self, p: PurseId) -> (info: Result<PurseInfo, Error>)
        requires
            self.invariant(),
        ensures
            match info {
                Ok(i) =>
                    self.purses().dom().contains(p)
                    && i.id == p
                    && i.name@ == self.purses()[p].name
                    && i.spendable == 0
                    && i.spendable_strict == 0
                    && i.pending == 0,
                Err(Error::PurseNotFound(q)) =>
                    !self.purses().dom().contains(p) && q == p,
            },
    {
        let mut i: usize = 0;
        while i < self.purses.len()
            invariant
                0 <= i <= self.purses.len(),
                self.invariant(),
                forall|j: int| 0 <= j < i ==> (#[trigger] self.purses@[j]).id != p,
            decreases
                self.purses.len() - i,
        {
            if self.purses[i].id == p {
                let rec = &self.purses[i];
                let name_copy: Vec<u8> = rec.name.clone();
                assert(name_copy@ == rec.name@);
                return Ok(PurseInfo {
                    id: rec.id,
                    name: name_copy,
                    spendable: 0,
                    spendable_strict: 0,
                    pending: 0,
                });
            }
            i += 1;
        }
        Err(Error::PurseNotFound(p))
    }
}

} // verus!
