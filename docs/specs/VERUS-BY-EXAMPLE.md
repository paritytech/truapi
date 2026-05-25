# Verus by Example — patterns from the coinage-layer pilot

A working developer's reference for the proof-engineering patterns that have repeatedly paid off in `rust/crates/coinage-layer/`. Not a Verus tutorial — assumes you've read [the Verus tutorial](https://verus-lang.github.io/verus/guide/) and have the toolchain running.

Every pattern below is grounded in real code in `rust/crates/coinage-layer/src/lib.rs`. The reference numbers like (e), (k) are invariant-clause labels from that file.

## 1. Installing Verus

`cargo install verus` does **not** work — produces a wrapper without a verusroot. Use the release binary:

```bash
gh release download --repo verus-lang/verus --pattern '*macos-arm64.zip' --dir ~/Downloads
unzip ~/Downloads/verus-*-arm64-macos.zip -d ~/tools/
mv ~/tools/verus-* ~/tools/verus
echo 'export PATH="$HOME/tools/verus:$PATH"' >> ~/.zshrc
exec zsh
verus --version
```

Verify a crate with `cargo verus verify` from inside the crate directory.

## 2. State struct + ghost-map shape

The single most useful pattern: keep an exec storage (Vec or HashMap) and a ghost map (the contract surface). Tie them with a refinement invariant.

```rust
pub struct State {
    pub purses: Vec<PurseRec>,                            // exec
    pub next_purse_id: u64,                               // exec
    pub spec_purses: Ghost<Map<PurseId, PurseRecSpec>>,   // ghost — contract surface
}
```

All fields must be `pub` so that `open spec fn` accessors can read them across crate boundaries. Verus treats a struct with even one private field as fully opaque externally; you can't have a public `open spec` body that touches a private field.

External code can write to these fields and break the invariant, but every method's `requires self.invariant()` makes that state stuck — they get no useful operation. The invariant is the only valid entry point.

## 3. The `view()` lift

For exec records that contain non-Copy data (`Vec<u8>` names, etc.), define a spec twin and lift via `view()`:

```rust
pub struct PurseRec {
    pub id: PurseId,
    pub name: Vec<u8>,
    pub next_coin_idx: u64,
    pub next_entry_idx: u64,
}

pub struct PurseRecSpec {
    pub id: PurseId,
    pub name: Seq<u8>,
    pub next_coin_idx: nat,
    pub next_entry_idx: nat,
}

impl PurseRec {
    pub open spec fn view(&self) -> PurseRecSpec {
        PurseRecSpec {
            id: self.id,
            name: self.name@,
            next_coin_idx: self.next_coin_idx as nat,
            next_entry_idx: self.next_entry_idx as nat,
        }
    }
}
```

Then `rec@` in spec contexts gives the spec twin.

## 4. The pre-state ghost capture trio

At the top of every method that mutates state, capture the relevant pre-state:

```rust
fn create_purse(&mut self, name: Vec<u8>) -> (new_id: PurseId)
    requires
        old(self).invariant(),
        old(self).has_create_capacity(),
    ensures /* ... */,
{
    let new_id = self.next_purse_id;
    let ghost old_v = self.purses@;
    let ghost old_m = self.spec_purses@;
    // ... rest of body
}
```

The captures serve two purposes:
- **Bridging `old(self).X` to in-body reasoning.** Inside a loop body, `old(self)` is the *function* entry, not the iteration start; the ghost capture lets you talk about a specific snapshot.
- **Re-asserting unchanged-fields equality after partial mutation.** If a method modifies one field, Verus does NOT automatically conclude the others are unchanged. After the mutation:
  ```rust
  assert(self.purses@         == old_purses_vec);
  assert(self.spec_purses@    == old_spec_purses);
  assert(self.next_purse_id   == old_next_purse_id);
  ```
  These three lines turn `assert(self.invariant())` from failing to discharging instantly.

## 5. Trigger choice

The single rule that saves the most debugging time:

**For `forall|k| ... ==> P(k)` over keys/indices, choose `#[trigger]` to be an expression Verus already needs to talk about when evaluating `P(k)`** — typically `Map::dom().contains(k)`, `Set::contains(k)`, `Seq::index(k)`. Reserve bound-variable-only triggers (e.g. `exp_seq@[j]`) for places where the conclusion structurally returns a value from that sequence.

Example that works:

```rust
forall|j: int| 0 <= j < n ==>
    #[trigger] self.coins().dom().contains(make_key(j))
    && self.coins()[make_key(j)].exponent == exp_seq@[j]
```

Example that fails to instantiate (Verus has no reason to fire `exp_seq@[j]` when trying to prove `coins().dom().contains(...)`):

```rust
forall|j: int| #![trigger exp_seq@[j]] 0 <= j < n ==>
    self.coins().dom().contains(make_key(j))
    && self.coins()[make_key(j)].exponent == exp_seq@[j]
```

This bit the coinage-layer pilot on `top_up_purse`: switching from the bottom form to the top form turned three failing postconditions into instant discharge.

## 6. Loop invariant template for Vec scans

For a "find-and-mutate" loop over an exec Vec, the invariant looks like:

```rust
let ghost old_v       = self.purses@;
let ghost old_m       = self.spec_purses@;
let ghost old_coins   = self.spec_coins@;

let mut i: usize = 0;
while i < self.purses.len()
    invariant
        0 <= i <= self.purses.len(),
        self.invariant(),
        // Pre-mutation Vec/map captures, propagated for the search phase.
        self.purses@        == old_v,
        self.spec_purses@   == old_m,
        self.spec_coins@    == old_coins,
        // Pin the captured ghosts to the function-entry state. These bridges
        // are required for postconditions that mention `old(self).X` and
        // proof code inside the body that needs the equality.
        old_m       == old(self).spec_purses@,
        old_v       == old(self).purses@,
        old_coins   == old(self).spec_coins@,
        self.next_purse_id == old(self).next_purse_id,
        // Searched-but-not-found facts so far.
        forall|j: int| 0 <= j < i ==> (#[trigger] self.purses@[j]).id != target_id,
    decreases self.purses.len() - i,
{
    if self.purses[i].id == target_id {
        // The branch where the mutation happens. After mutating, the per-
        // clause proof block establishes the invariant for the new state.
        return ...;
    }
    i += 1;
}
```

## 7. Per-clause `assert forall ... by { ... }` blocks

After mutating a Vec entry (push, swap_remove, IndexMut), the invariant needs to be re-established. Walk each clause separately with explicit branches:

```rust
proof {
    let new_v = self.purses@;
    let new_m = self.spec_purses@;

    // (e) every Vec entry's id is in dom
    assert forall|k: int| 0 <= k < new_v.len() implies
        new_m.dom().contains(#[trigger] new_v[k].id)
    by {
        if k == target_idx {
            assert(new_v[k].id == p);
        } else {
            assert(new_v[k] == old_v[k]);
            assert(old_m.dom().contains(old_v[k].id));
        }
    }

    // (f) every Vec entry's spec view matches its dom entry
    assert forall|k: int| 0 <= k < new_v.len() implies
        new_m[(#[trigger] new_v[k]).id] == new_v[k]@
    by {
        if k == target_idx {
            // ...
        } else {
            assert(new_v[k] == old_v[k]);
            assert(old_m[old_v[k].id] == old_v[k]@);
        }
    }

    // ... etc for (g), (h)
}
```

Two-branch structure (`if k == target_idx`) is the common shape. For swap_remove the "changed" side has a sub-case (`if target_idx < last_idx`); for full filtering the loop becomes a nested filter-and-rebuild.

## 8. Capturing constructed values before move

When constructing a struct that gets moved into a Vec, capture the spec view *before* the move so the post-mutation proof can refer to it:

```rust
let new_rec = PurseRec { id, name, next_coin_idx: 0, next_entry_idx: 0 };
let ghost new_rec_spec = new_rec@;             // capture BEFORE move
self.purses.push(new_rec);                     // moves new_rec
proof {
    // new_rec is gone in exec, but new_rec_spec persists.
    self.spec_purses = Ghost(self.spec_purses@.insert(new_id, new_rec_spec));
    assert(self.purses@[old_v.len() as int]@ == new_rec_spec);
}
```

## 9. Compositional operations (looping a smaller op)

For an operation that loops a primitive (`top_up_purse` loops `add_coin`), the inner-loop proof needs two key ingredients:

1. **Capture pre-call state** before each invocation of the primitive — even within a loop body, so post-call we can talk about what existed before.
2. **In the assert-forall body, handle "new key just added" and "old key still present" as separate branches.** The "old key" branch needs `(k != new_key)` — that's the fact that lets `insert(new_key, _)` preserve old entries.

```rust
let exp = exp_seq[k];
let ghost prev_next = self.purses()[p].next_coin_idx;
let ghost pre_coins = self.coins();              // capture BEFORE the call
let new_key = self.add_coin(p, exp);
proof {
    assert(new_key.1 == (old_p_next + k as nat) as u64);
    assert forall|j: int| 0 <= j < (k + 1) as int implies
        #[trigger] self.coins().dom().contains((p, (old_p_next + j) as u64))
        && self.coins()[(p, (old_p_next + j) as u64)].exponent == exp_seq@[j]
    by {
        let nk = (p, (old_p_next + j) as u64);
        if j == k as int {
            assert(nk == new_key);
            assert(self.coins()[new_key].exponent == exp);
        } else {
            assert(j < k as int);
            assert(pre_coins.dom().contains(nk));         // from pre-call loop inv
            assert(pre_coins[nk].exponent == exp_seq@[j]);
            assert(nk.1 != new_key.1);                    // distinct keys
        }
    }
}
```

## 10. `&mut self` postcondition syntax

In `ensures` clauses, `self` references are disambiguated:
- `old(self).X` → pre-call value
- `final(self).X` → post-call value

```rust
fn create_purse(&mut self, name: Vec<u8>) -> (new_id: PurseId)
    requires
        old(self).invariant(),                                        // pre
    ensures
        final(self).invariant(),                                      // post
        !old(self).purses().dom().contains(new_id),                   // pre
        final(self).purses() == old(self).purses().insert(new_id, _), // post relative to pre
```

## 11. Unreachable code with `vstd::pervasive::unreached()`

Some scan loops are guaranteed to find a target by the invariant (e.g. `add_coin` after a precondition `purse exists`). For the post-loop case, derive `false` from the invariant and then return:

```rust
// Cannot reach here: p is in old(self).purses().dom() by precondition,
// so invariant (g) gives a Vec witness; the scan loop would have found it.
proof {
    assert(old_m.dom().contains(p));
    let w = choose|k: int| 0 <= k < old_v.len() && #[trigger] old_v[k].id == p;
    assert(0 <= w < old_v.len());
    assert(self.purses@[w].id != p);  // contradiction with old_v[w].id == p
}
vstd::pervasive::unreached()
```

## 12. Avoiding `cargo build --workspace` regressions

Verus crates use the `vstd` dependency (`vstd = "=0.0.0-<date>-<hash>"`) which IS published on crates.io. Vanilla `cargo build` works as long as proof blocks are gated behind `#[cfg(verus_only)]` — the `verus! { ... }` macro handles this. To silence dead-code / unused-variable warnings under non-Verus builds, scope `#[allow(dead_code)]` to specific fields and `#[allow(unused_variables)]` to functions whose parameter is only consumed in proof blocks.

```rust
pub struct State {
    pub purses: Vec<PurseRec>,
    pub next_purse_id: u64,
    #[allow(dead_code)]
    pub spec_purses: Ghost<Map<PurseId, PurseRecSpec>>,
}

#[allow(unused_variables)]
pub fn mark_coin_pending_spend(&mut self, key: (PurseId, u64))
    // ... `key` is consumed by ghost code only in this pilot
```

## 13. Proof economy reality check

For the coinage-layer pilot (9 operations, 11 invariant clauses):

| | Lines |
|---|---|
| Executable code | ~80 |
| Spec / contracts | ~140 |
| Proof blocks (assert-forall, ghost captures) | ~600 |

Roughly **6:1 proof-to-exec ratio** for strong contracts. Each new operation adds ~50 lines of exec + ~120 lines of proof. The cost is converging because the invariant is stable; once the patterns above are in muscle memory, the marginal cost per op is bounded.

## 14. When to stop and ship

Decision rule: **if a proof block exceeds ~150 lines for a single operation, the operation is probably trying to do too much.** Decompose it into smaller primitives that each fit the standard pattern.

Real-world example: `delete_purse` with an inner Vec-filter loop for coin removal blew past 200 lines of proof attempting to maintain Vec ↔ ghost refinement through the filter. The right move was to decompose: separate `delete_purse_record` (just removes the purse from purses) from `purge_coins_of_purse` (filters the coin Vec), and prove each independently. (This decomposition is queued as stage-5 work and is not in the pilot.)
