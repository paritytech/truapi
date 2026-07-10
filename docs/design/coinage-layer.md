---
title: "Coinage Layer"
status: "Draft"
---

# Coinage Layer — Design

## 1. Summary

The Coinage Layer is the host's self-contained coinage subsystem. It owns every coin and recycler entry the user controls, partitions them across one or more purses, observes chain state reactively, schedules recycling, and runs the cryptographic and operational machinery for transfers, unloads, and offload. It has no knowledge of RFC‑17 product concepts (receivables, cheques, refunds, invoices); those live in the layer above.

This document is normative for the layer's behavior. Two conformant implementations operating on the same root entropy against the same chain state must produce the same on-chain effects, the same set of local records, and the same observable events.

## 2. Scope

### 2.1 In scope

Purses; coins and recycler entries (records, state machines, ages); reactive on-chain observation; selection; recycling (payment-folded plus periodic backstop); free / paid unload tokens with automatic fallback; fee-mode auto-selection; transfer to pre-arranged recipient accounts; portable coin export / import (the seam to the upper layer); external offload to a non-coinage account; rebalance between purses; payment classification for direct transfers; operation lifecycle (durable handles, status streams, cancel-before-submission, restart resumption); recovery from root entropy.

### 2.2 Out of scope

Receivables; cheques; refunds; invoices; product permissions; consent UI; cheque wire transport; multi-device synchronization; coinage pallet runtime evolution; the product-facing API surface.

### 2.3 Relationship to the upper layer

Exactly one upper layer consumes this layer's API. It is trusted (it lives inside the host) and is the only valid caller. The upper layer adds receivables, cheques, refunds, and the RFC‑6 / RFC‑17 product-facing surface, composing them out of the primitives this layer exposes.

## 3. Concepts

### 3.1 Purse

A purse is a named, firewalled coinage balance with an isolated derivation namespace. Every coin and every recycler entry belongs to exactly one purse. Balance, selection, recycling, and operations are scoped to a single purse unless explicitly cross-purse (rebalance, deletion).

Exactly one purse with a reserved identifier — the **main purse** — exists by construction once the layer is initialized. Any number of additional purses may be created.

### 3.2 Coin

A coin is a chain-level NFT representing a fixed denomination of dotUSD. It is identified on chain by an sr25519 account derived from the layer's root entropy, the coin's purse, and its derivation index. A coin carries:

- a denomination `exponent` (denomination = `2^exponent` cents);
- an integer `age` incremented by the chain on every transfer or split, capped at a chain-enforced maximum above which the coin is unusable.

A coin is consumed by transfer (to a pre-arranged recipient account), by split (into smaller coins), by recycling (into a fresh recycler entry), or by export (the coin and its secret are handed to the upper layer).

### 3.3 Recycler entry

A recycler entry is a Bandersnatch keypair the layer placed into a chain recycler ring — a privacy anonymity pool. The layer realizes the entry's value by **unloading** it: a Ring VRF proof of ring membership produces a fresh age-0 coin (or external-asset output) without revealing which entry was unloaded. An entry holds no spendable value on its own; value is realized at unload time. An entry must wait for its ring to fill before its anonymity claim is meaningful.

### 3.4 Operation

An operation is a long-running asynchronous task. The operation kinds this layer supports are: `TopUp`, `Transfer`, `Export`, `Import`, `ExternalOffload`, `Rebalance`, `MaintenanceSweep`, `DeletePurse`, `Recover`. Each operation has a durable opaque handle, a persisted record, a status stream emitted at every state transition, and a set of locked coins / recycler entries that no other operation may touch until the owning operation reaches a terminal state.

Every call to a long-running primitive starts a fresh operation. The layer does not deduplicate by argument equality; callers needing idempotency MUST track handles themselves.

### 3.5 Coin export / import (the layer seam)

The upper layer needs coin secrets to construct cheques but must not have access to the layer's derivation tree. Two primitives bracket this:

- **Export.** Selects coins in a purse summing to a requested amount, performs any necessary split / unload-into-coins extrinsics, then returns the resulting `(coin_account, coin_secret)` pairs and treats the exported coins as no longer owned by the layer.
- **Import.** Accepts an externally supplied list of `(coin_account, coin_secret)` pairs and routes each one into a purse's namespace by submitting a transfer signed with the supplied secret.

A `coin_secret` is the raw sr25519 secret-key material controlling the corresponding coin account. Two implementations exchanging exported secrets must agree on the same encoding (the recommended encoding is the raw 64-byte secret-key form).

These are the only primitives through which coin secrets cross the API. Everything in the upper layer's cheque / receivable machinery composes on top of this seam.

## 4. Identity

### 4.1 Per-purse isolation

Each purse has its own coin-index space and its own recycler-entry-index space. Index `i` in purse A and index `i` in purse B address different on-chain accounts because their derivation paths differ. A coin or entry record carries `(purse_id, index)` as its identity; purse membership is implied by derivation.

### 4.2 Derivation

All keys are deterministically derived from the root entropy supplied at initialization. The layer never generates entropy itself. Given identical entropy, two instances derive identical accounts.

The exact derivation scheme is implementation-defined. The recommended scheme is in Appendix B. Two invariants are normative regardless of scheme:

- Given the same root entropy, the same purse identifier, and the same index, the layer produces the same coin (or recycler-entry) account.
- Two distinct purses have non-overlapping derivation namespaces.

### 4.3 No-reuse invariant

Within a purse, a coin derivation index, once allocated, is never reused. The same rule applies to recycler-entry derivation indices.

This invariant is unconditional: it holds after the coin is spent and the on-chain account is empty, and after the recycler entry is unloaded and removed from the ring. Implementations may realize it by retaining record stubs, by a high-water mark per purse, or by chain scanning — any mechanism that guarantees no index is allocated twice.

Rationale: a coin's account ID may have appeared in a transfer memo passed out-of-band; a recycler entry's Bandersnatch public key sits in a public ring member list. Reuse would correlate new activity with old.

## 5. State

### 5.1 Coin lifecycle

Each coin record carries a lifecycle state:

- **Pending** — created locally as a future output of an in-flight operation; chain account not yet observed.
- **Available** — chain confirms the account holds a coin with a known age. Selectable.
- **LockedFor(op)** — held by in-flight operation `op`. Not selectable.
- **Spent** — terminal. Chain confirms the account is empty (or the coin has been exported). Record retained for the no-reuse invariant; subject to garbage collection by any mechanism that still guarantees no reuse.

Transitions:

| From | To | When |
|-|-|-|
| (none) | `Pending` | Created locally as an output of an operation |
| `Pending` | `Available` | First chain observation reports the account holds a coin |
| `Available` | `LockedFor(op)` | Operation `op` locks the coin during `Preparing` |
| `LockedFor(op)` | `Available` | `op` aborts or is cancelled before submitting any extrinsic |
| `LockedFor(op)` | `Spent` | `op` reaches terminal success and the account is observed empty (or, for export, immediately after the export emits the secret) |
| `LockedFor(op)` | `Available` | `op` fails post-submission and the account is still observed populated |

### 5.2 Recycler entry — on-chain readiness and the anonymity floor

An entry's anonymity at unload time comes from its ring: a Ring VRF proof hides the prover among the ring's members, so the larger the ring, the stronger the anonymity. The chain accepts unloads from rings of any size; this layer applies its own **anonymity floor** — a minimum ring member-count below which it flags the entry as offering reduced anonymity. The floor is a single value scoped to the layer instance; it is not configurable per purse or per operation. The floor is a tunable parameter (Appendix A.2).

Each entry has an on-chain readiness state derived from chain observation:

- **Missing** — no recycler location on chain for the entry's member key. The load extrinsic has not finalized, or the entry has been consumed.
- **Waiting** — chain reports a recycler location, but the ring is in onboarding or chain-side readiness conditions are unmet.
- **Ready** — ring member-count meets or exceeds the anonymity floor.
- **Degraded(n)** — ring member-count is `n`, below the floor.

`Ready` and `Degraded` are both usable for selection. The choice of whether to use `Degraded` entries is controlled by the caller per primitive (§8).

### 5.3 Recycler entry — readiness jitter

When the layer creates a new recycler entry (top-up or recycling), it records the creation timestamp as `allocated_at` and draws a per-entry random delay `d` uniformly from `[0, D]`. The entry's `ready_at` is `allocated_at + d`; the entry is not selectable until `now ≥ ready_at`, regardless of on-chain readiness.

Without jitter, an observer with timing data could match a load to its subsequent unload. The bound `D` is tunable (Appendix A.3). The mechanism is SHOULD, not MUST: implementations may set `D = 0` if a specific deployment knowingly accepts the timing correlation.

### 5.4 Recycler entry — local lifecycle

Independent of on-chain readiness, each entry has a local lifecycle state:

- **Available** — free for selection.
- **LockedFor(op)** — held by in-flight operation `op`.
- **Consumed** — terminal. The owning operation reached terminal success and the entry was unloaded. Record retained for the no-reuse invariant; subject to garbage collection on the same terms as `Spent` coins.

An entry is **selectable** iff:

```
local_state = Available  ∧  on_chain_state ∈ {Ready, Degraded}  ∧  ready_at ≤ now
```

A caller may further restrict selection to exclude `Degraded` entries via a per-primitive flag (§8). The selectability condition above is the maximum set; flags only narrow it.

### 5.5 Operation lifecycle

Every operation traverses:

| State | Meaning |
|-|-|
| `Preparing` | Selecting, deriving, signing, building extrinsics, or re-planning between phases. No extrinsic currently in flight. |
| `Submitted` | An extrinsic has been broadcast. |
| `InBlock` | An extrinsic has been included in a non-finalized block. |
| `Finalized` | An extrinsic has been finalized. |
| `Waiting(until)` | The operation cannot progress until the indicated wall-clock time (e.g. waiting for a recycler entry's `ready_at` or for ring readiness). The layer wakes the operation at or shortly after `until` and returns to `Preparing`. |
| `Done(receipt)` | Terminal. At least one submitted extrinsic was successfully finalized. The receipt (§9) enumerates per-extrinsic outcomes (success or rejection); partial-failure interpretation is the caller's. |
| `Failed(reason)` | Terminal. Either no extrinsic was submitted (pre-submission failure), every submitted extrinsic was rejected, or the operation was cancelled. |

A long-running operation (e.g. `ExternalOffload`) may cycle through phases: `Preparing` → `Submitted` → `InBlock` → `Finalized` → `Preparing` → `Waiting` → `Preparing` → `Submitted` → … and so on until it reaches `Done` or `Failed`. Each phase transition is durably persisted; the operation resumes from the same phase across restart.

Operations that submit no extrinsics (e.g. `Recover`) emit `Preparing` followed directly by a terminal item.

## 6. Operational model

### 6.1 Reactive on-chain observation

The layer maintains continuous subscriptions to every chain storage entry backing its local records: coin storage for each known coin account, ring-member storage for each recycler entry's member key, recycler revision and member-count for the rings entries belong to, and consumed-unload-token storage relevant to the user's allowance. Subscription events update local records in place. The layer does not pull-poll; callers read its cached view, which the subscription keeps fresh.

The layer is therefore long-lived. Subscription updates must be reconciled with operation-driven changes — for example, a `LockedFor` coin observed empty on chain transitions cleanly to `Spent`.

### 6.2 Balance

Per purse, the layer exposes three values, emitted by the balance subscription on every change:

- **Spendable** — sum of values of all coins in `Available` plus all currently selectable recycler entries (`Ready` or `Degraded`).
- **Spendable strict** — same, but counting only `Ready` recycler entries. Always `≤ spendable`. The difference is the value held in `Degraded` entries.
- **Pending** — sum of values of all coins in `Pending` or `LockedFor`, plus all recycler entries that are not selectable (`Waiting`, `Missing`, `LockedFor`, or with `ready_at > now`).

### 6.3 Selection

This section describes the selection used for operations that produce coinage value at a destination *inside* coinage — transfer, export, rebalance. External offload uses a different, planner-driven strategy described in §8.6.

When the layer must produce a specified `amount` from a purse for one of these operations, it tries the following strategies in priority order, returning the first that succeeds.

Selection orders coins by `(exponent desc, age desc, derivation_index asc)` and recycler entries by `(exponent desc, ring_index asc, derivation_index asc)` before applying each strategy's heuristic. This ordering is fully deterministic — two conformant implementations with the same purse contents produce the same selection.

1. **Exact match.** Find a subset of `Available` coins (in the order above) summing exactly to `amount`. Zero extrinsics.
2. **Split.** Find the smallest single `Available` coin strictly greater than `amount`; split it into `amount` + change denominations using one extrinsic. If no single coin suffices, build a multi-coin cover with whole coins (the deterministic order naturally produces largest-first) and split the last coin that crosses the target; if that is also impossible, fall through. No unload token consumed.
3. **Unload into coins.** Use selectable recycler entries (§5.4), optionally with whole coins for partial coverage, to mint coins of the target denominations. Entries are grouped by `(denomination, ring)`; each group becomes one atomic `unload-into-coins` extrinsic carrying its own unload token. The output value of each group equals its input value (the group's own change absorbs the remainder). Prefer a single smallest sufficient entry; otherwise take entries in the deterministic order above to cover the deficit.

If all three strategies fail and the purse contains recycler entries whose summed value would have covered `amount` had they all been ready, return `NoReadyEntries` so the caller can distinguish "wait" from "insufficient funds".

Selection runs against the live local view. Selection holds locks for the lifetime of the resulting operation; two concurrent selections never disagree about availability.

If the caller has disallowed `Degraded` entries for a particular operation, the effective selectability condition narrows accordingly. If selection would have succeeded with `Degraded` entries but cannot succeed without them, return `NoReadyEntries`.

### 6.4 Autonomous lifecycle maintenance

The chain places a hard time limit on **both** states of a logical coin's value:

- A **coin** ages out at `MaximumAge` transfers/splits and becomes unusable.
- A **recycler entry** dies when its ring is cleaned up after `RecyclerExpirationTime` from the ring's `immutable_since`. Backing value of any entry that has not been unloaded by then is destroyed by the pallet (added to `TotalValueOfDestroyedCoins`).

The layer MUST run two autonomous sweeps that together form a closed loop: `coin → entry` (coin-age recycling) and `entry → coin` (ring-expiration rescue). A coin that is never spent cycles between forms indefinitely; no value is lost so long as both sweeps run regularly. Skipping either sweep causes silent loss of funds for users who don't actively spend.

**Coin-age recycling sweep (coin → entry).** A scheduler runs at a tunable interval (Appendix A.4). Per purse, the sweep scans `Available` coins whose `age ≥ recycle_at_age` (Appendix A.1), oldest first, and submits one `load_recycler_with_coin` extrinsic per coin. Post-submission failure marks the coin `Spent`; pre-submission failure releases the lock so a future sweep can retry. Each successful recycle consumes the coin (terminal `Spent`) and produces a new `Available` recycler entry whose `ready_at` is set per §5.3.

Payment-folded refresh complements this: selection (§6.3) prefers older coins, and unload-into-coins emits age-0 coins. Active wallets refresh themselves implicitly.

**Ring-expiration rescue sweep (entry → coin).** A scheduler runs at a tunable interval (Appendix A.12). Per purse, the sweep scans recycler entries whose ring is approaching expiration — i.e. `now ≥ ring.immutable_since + RecyclerExpirationTime − rescue_margin` (Appendix A.13). The sweep groups eligible entries by `(denomination, ring)` and submits one `unload_recycler_into_coins` extrinsic per group, each carrying its own unload token (§6.5). Each successful rescue consumes the entry (terminal `Consumed`) and produces a new age-0 `Available` coin in the same purse.

The ring-expiration sweep is critical: without it, entries created by the coin-age sweep (or by top-up) can expire silently if the host is unused long enough for the ring lifecycle to complete. This is the only way for value to permanently disappear from a wallet whose root entropy and chain identity are otherwise intact.

**Triggers.** For both sweeps, the periodic schedule is the contractual minimum. Implementations MAY add opportunistic triggers (e.g. on host wake / foreground; on a subscription update that brings a coin past `recycle_at_age` or an entry past the rescue margin). Both sweeps are also invoked synchronously by `run_maintenance_sweep` (§8.7).

### 6.5 Unload tokens

Every unload of a recycler entry consumes exactly one unload token. Two classes exist:

- **Free** — derived from the user's people / lite-people ring membership; per-period allowance.
- **Paid** — derived from a period-specific paid-token ring that anyone may join by paying a fee (an on-chain extrinsic).

When the layer needs `N` tokens for a multi-group unload, it resolves them in this order:

1. For each token slot needed, probe `ConsumedFreeUnloadTokens` (cached from chain) for the current period and any prior period within the lookback grace window (Appendix A.6). Pick the first counter in the search range (Appendix A.5) whose alias is not consumed.
2. If free slots run out, fall back to paid tokens. If no paid-token ring membership exists for the current period, the layer first joins the current paid ring (a pre-step extrinsic), then derives the alias.

If neither free nor paid tokens can be obtained (no people/lite-people ring membership and the fee account cannot fund joining the paid ring), the operation fails with `NoUnloadToken`.

The caller does not select the class. Per-token cost is reported in the operation's status stream.

### 6.6 Fee account and fee mode

The layer derives a single **fee account** (sr25519) from the root entropy at initialization. This account pays the on-chain fee for every unload operation across every purse — it is not per-purse, not exposed in the API, and not configurable. How the fee account is funded is outside the layer's concern; the user / upper layer is expected to keep it topped up out of band.

Unloads support two fee modes:

- **Prepaid** — fee paid in native currency / asset from the fee account, alongside the unload extrinsic.
- **From-output** — fee deducted from the unloaded value.

The layer picks the mode automatically per unload: prepaid if the fee account holds sufficient external funds at submission time, from-output otherwise. The caller does not specify.

**From-output failure recovery.** When a from-output unload's dispatch fails, the pallet temporarily locks the first (fee) alias instead of permanently consuming it. The lock duration doubles on each consecutive failure (`2^retries × base_lock_period`). The layer tracks a monotonic `retry_counter` per alias and may resubmit the same alias once its lock expires, signing the proof over `alias_proofs[1..] ++ retry_counter ++ inherited_implication`. No value is destroyed on from-output dispatch failure; the alias remains reusable until a successful dispatch finally consumes it.

## 7. Operations

### 7.1 Handles

Every operation primitive returns an opaque, durable `OperationHandle`. A handle is sufficient to subscribe to the operation's status stream, read its current status, or cancel it (§7.3). Handles are layer-issued; callers do not supply correlation keys. Two operations with disjoint lock sets may run concurrently; lock conflicts are impossible by construction.

### 7.2 Status streams

Each operation emits the state machine of §5.5. The first item is the current status at subscription time. The terminal item (`Done` or `Failed`) is emitted exactly once and the stream then closes. Dropping the subscription is always safe; the operation continues regardless of whether anyone is subscribed.

### 7.3 Cancellation

A caller may cancel an operation whenever no extrinsic is currently in flight — i.e. while the operation is in `Preparing` or `Waiting`. The layer aborts, releases all locks, and emits `Failed(Cancelled)`.

While an extrinsic is in flight (`Submitted` / `InBlock` not yet `Finalized`), the operation cannot be cancelled at the API. The caller must await the extrinsic's resolution. A multi-phase operation may become cancellable again once it returns to `Preparing` or `Waiting`.

### 7.4 Restart durability and record retention

**In-flight operations.** Each operation record is persisted at start, together with its lock set. Each extrinsic submission is appended to the operation record before broadcast. On restart, the layer:

1. Reads back every open operation record and every locked record.
2. Re-establishes chain subscriptions for the affected accounts.
3. For each open operation: if no extrinsic was submitted, fail with `Failed(InterruptedPreSubmission)` and release locks. Otherwise, reconcile each submitted extrinsic against current chain state: if all expected effects are observed, transition to `Done`; if any are rejected, transition to `Failed`; otherwise, resume watching.

Pre-submission scratch state (in-flight selection, partial signing) is not durable. A restart in `Preparing` is equivalent to a cancel.

**Subscriptions.** All subscription streams (balance, operation status, events) are torn down on restart. Callers MUST re-subscribe after restart; subscriptions are not auto-resumed.

**Terminal-operation records.** Once an operation reaches a terminal status (`Done` or `Failed`) and the terminal status item has been emitted on its status stream, the layer MAY immediately drop the operation record from durable storage. Subsequent re-subscription via the now-stale handle returns `OperationNotFound`. Callers that need to retain the receipt MUST capture it from the terminal status item; the layer does not maintain history.

## 8. Primitives

All long-running primitives return:

```text
struct OperationStart {
    handle: OperationHandle,
    status: Stream<OperationStatus>,
}
```

Errors emitted synchronously describe failure to start an operation. Errors emitted via the status stream (as `Failed(Error)`) describe terminal failure of a started operation. The full error enum is in §10.

### 8.1 Purse lifecycle

```text
fn create_purse(name: String) -> Result<PurseId, Error>
fn query_purse(purse: PurseId) -> Result<PurseInfo, Error>
fn rename_purse(purse: PurseId, name: String) -> Result<(), Error>
fn delete_purse(target: PurseId, drain_into: PurseId)
    -> Result<OperationStart, Error>
fn rebalance_purse(from: PurseId, to: PurseId, amount: Amount, allow_degraded: bool)
    -> Result<OperationStart, Error>

struct PurseInfo {
    id:               PurseId,
    name:             String,
    spendable:        Amount,
    spendable_strict: Amount,
    pending:          Amount,
}
```

`create_purse` assigns a fresh non-reserved `PurseId`, persists the purse, returns synchronously. No chain interaction.

`query_purse` returns a synchronous snapshot.

`rename_purse` updates the purse's name. No chain interaction.

`delete_purse` drains the target into `drain_into` via on-chain transfer, then closes the purse record. The main purse cannot be deleted. A purse cannot be deleted while it has in-flight operations.

`rebalance_purse` transfers `amount` from one purse to another by selection in the source purse's namespace, with destination coin accounts allocated in the target purse's namespace. `allow_degraded` controls whether `Degraded` recycler entries may be selected.

Errors: `PurseNotFound`, `CannotDeleteMainPurse`, `PurseHasInFlightOperations`, `InsufficientFunds`, `NoReadyEntries`, `ChainRejected`, `Cancelled`.

### 8.2 Top-up

```text
trait FundingOrigin {
    fn external_account(&self) -> ExternalAccountId;
    fn sign_payload(&self, payload: &[u8]) -> Signature;
}

fn top_up(into: PurseId, amount: Amount, origin: &dyn FundingOrigin)
    -> Result<OperationStart, Error>
```

Decomposes `amount` into recycler-entry denominations, allocates fresh entry indices in `into`, and submits one external-asset load extrinsic per denomination, signed by `origin`. Successful per-entry loads do not roll back failed ones; per-entry outcomes are reported in the status stream.

Errors: `PurseNotFound`, `InsufficientExternalFunds`, `ChainRejected`.

### 8.3 Transfer

```text
fn transfer(
    from:              PurseId,
    amount:            Amount,
    recipient_outputs: Vec<RecipientOutput>,
    allow_degraded:    bool,
    memo_callback:     Option<MemoCallback>,
) -> Result<OperationStart, Error>

struct RecipientOutput {
    exponent: DenominationExponent,
    account:  CoinAccountId,
}

type MemoCallback = fn(memo_entries: Vec<MemoEntry>);

struct MemoEntry {
    sender_coin_account: CoinAccountId,
    recipient_account:   CoinAccountId,
    derivation_index:    CoinIndex,
}
```

Transfers `amount` from `from` to the supplied recipient-controlled accounts. The constraint on `recipient_outputs` is:

```
Σ 2^output.exponent over recipient_outputs == amount
```

Multiple outputs with the same `exponent` are allowed (e.g. two `exponent = 3` outputs to two distinct accounts).

Selection from `from` uses the three-tier strategy (§6.3) routing the output coins to the supplied accounts. If `memo_callback` is supplied, the layer invokes it with one `MemoEntry` per transferred coin once the corresponding extrinsic reaches `InBlock` (chain inclusion, before finalization). The layer does not encode or transmit memos; the caller owns the wire format.

Errors: `PurseNotFound`, `InsufficientFunds`, `NoReadyEntries`, `OutputsDoNotSumToAmount`, `ChainRejected`, `Cancelled`.

### 8.4 Export coins

```text
fn export_coins(from: PurseId, amount: Amount, allow_degraded: bool)
    -> Result<ExportStart, Error>

struct ExportStart {
    handle: OperationHandle,
    status: Stream<OperationStatus>,
    coins:  Stream<ExportedCoin>,    // emits once per coin, then closes
}

struct ExportedCoin {
    account:  CoinAccountId,
    secret:   CoinSecret,
    exponent: DenominationExponent,
}
```

Materializes `amount` worth of coins in `from`'s namespace by selection and any required split / unload-into-coins extrinsics, then emits one `ExportedCoin` per resulting coin. Each exported coin transitions to `Spent` in this layer's view: the on-chain account still holds the coin but it is now controlled by the externally held secret.

`export_coins` is the **only** primitive through which coin secrets cross the API. The caller is responsible for the confidentiality of the emitted secrets.

Errors: `PurseNotFound`, `InsufficientFunds`, `NoReadyEntries`, `ChainRejected`, `Cancelled`.

### 8.5 Import coins

```text
fn import_coins(into: PurseId, coins: Vec<(CoinAccountId, CoinSecret)>)
    -> Result<OperationStart, Error>
```

For each supplied pair, the layer (a) reads the coin's denomination from chain, (b) allocates a fresh coin derivation index in `into`, (c) submits a transfer extrinsic from `account` (signed with the supplied secret) to the freshly derived recipient account in `into`'s namespace. The layer does not retain supplied secrets after submission. New coin records appear in `into` and become `Available` once the chain confirms.

Per-coin outcomes (`Done` / `BadCoinSecret` / `SnipedCoin` / `ChainRejected`) are reported in the status stream; partial success is possible. A pair whose `account` is already known to this layer is rejected with `BadCoinSecret`.

Errors: `PurseNotFound`, `BadCoinSecret`, `SnipedCoin`, `ChainRejected`, `Cancelled`.

### 8.6 External offload

```text
fn external_offload(
    from:           PurseId,
    amount:         Amount,
    destination:    ExternalAccountId,
    allow_degraded: bool = false,
) -> Result<OperationStart, Error>
```

Moves `amount` from `from` into a non-coinage account on chain. `allow_degraded` defaults to `false`: an external offload reveals the unloaded value to chain observers, so the anonymity set should be at full strength unless the caller explicitly opts in to `Degraded` entries.

External offload is a **multi-phase, possibly long-running** operation. The layer drives it through the loop below until a terminal state is reached. Each phase transition is durably persisted (§7.4); cancellation is permitted in `Preparing` and `Waiting` (§7.3).

1. **Plan** (status: `Preparing`). Read the current view of `from`. Choose the next phase:
   - If selectable entries (per `allow_degraded`) cover `amount` → **Offboard**.
   - Else if selectable + non-yet-ready entries together cover `amount` → **Wait** until the latest such entry's `ready_at`.
   - Else compute the deficit. If available coins (`state = Available`) cover the deficit → **Recycle**.
   - Else if non-spent coins (including coins locked by this or another operation, recycling, pending-transfer) together cover the deficit → **Wait** for a short retry interval (Appendix A.11).
   - Else fail with `InsufficientFunds`.
2. **Recycle** (status cycles `Submitted` → `InBlock` → `Finalized` per coin). Pick the coins to cover the deficit in the deterministic order of §6.3. Submit one `load_recycler_with_coin` extrinsic per coin. Each successful recycle produces a new `Available` recycler entry locked to this operation. Return to **Plan**.
3. **Wait** (status: `Waiting(until)`). Suspend until the indicated time. On wake (or operation resume after restart), return to **Plan**.
4. **Offboard** (status cycles `Submitted` → `InBlock` → `Finalized` per recycler group). Submit one `unload_recycler_into_external_asset_and_vouchers` extrinsic per `(denomination, ring)` group, each carrying its own unload token (§6.5). The total transferred to `destination` is `amount`. Any surplus from the selected entries is **always atomically reloaded** into fresh recycler entries within the same extrinsic — surplus value MUST NOT land as a free coin, because that would re-link the entry-side anonymity set to a fresh sr25519 account. Once all groups have finalized, reach `Done(receipt)`.

The operation locks every coin and recycler entry it touches throughout its lifetime, including entries produced during the **Recycle** phase. Locks are released on terminal status per §7.4.

Fee mode is auto-selected per §6.6.

Errors (via terminal `Failed`): `InsufficientFunds`, `NoUnloadToken`, `ChainRejected`, `Cancelled`.
Errors (synchronous): `PurseNotFound`.

### 8.7 Maintenance sweep

```text
fn run_maintenance_sweep(purses: Option<Vec<PurseId>>)
    -> Result<OperationStart, Error>
```

Runs both the coin-age recycling sweep and the ring-expiration rescue sweep once across the listed purses (or all purses if `None`). For each purse the layer:

1. Submits one `load_recycler_with_coin` extrinsic per eligible aging coin (oldest first).
2. Submits one `unload_recycler_into_coins` extrinsic per `(denomination, ring)` group of entries past the rescue margin.

Per-extrinsic outcomes are reported via the operation's receipt. The layer also runs both sweeps autonomously per §6.4; this primitive exists so the upper layer can force a run on demand (e.g. on app foreground).

Errors: `PurseNotFound`.

### 8.8 Payment classification

```text
fn classify_incoming_payment(entries: Vec<MemoEntry>)
    -> Result<PaymentClassification, Error>

enum PaymentClassification {
    Matched,    // every entry's recipient_account corresponds to a coin in some purse known to this layer
    Received,   // some entries' coins are present, others are not
    Unmatched,  // no entries match
}
```

Synchronous classification against the live local view. The layer treats an empty entry list as `Unmatched`. The classification is informational only; no operation is started, no record is modified.

### 8.9 Subscriptions

```text
fn subscribe_purse_balance(purse: PurseId) -> Stream<PurseBalance>
fn subscribe_operation_status(handle: OperationHandle) -> Stream<OperationStatus>
fn subscribe_events() -> Stream<LayerEvent>

struct PurseBalance {
    spendable:        Amount,
    spendable_strict: Amount,
    pending:          Amount,
}
```

Each stream emits the current value at subscribe time, then a new item on every state change. Closing the stream releases the subscription. Multiple concurrent subscriptions are independent.

### 8.10 Recovery

```text
fn recover(non_main_purse_ids: Vec<PurseId>)
    -> Result<OperationStart, Error>

fn extend_scan(
    purse:             PurseId,
    from_coin_index:   CoinIndex,
    from_entry_index:  RecyclerEntryIndex,
) -> Result<OperationStart, Error>
```

Long-running operations of kind `Recover`. Reconstruct records for the listed purses, plus the main purse (always restored). Scan chain storage using a gap-limit strategy (Appendix C). After the operation reaches `Done`, reactive observation continues from the discovered records.

The operation emits no on-chain extrinsics, so its status stream goes `Preparing` → terminal. Per-record discovery is observable via the event stream (`CoinAvailable`, `EntryAllocated`).

The layer cannot enumerate non-main purse identifiers from the chain; the caller must supply them from its own backup.

Pre-submission operation records are not recoverable; any operation mid-flight at the moment durable state was lost is gone.

Errors (via `Failed` status item): `RecoveryFailed`.

## 9. Receipts

When an operation reaches `Done`, the layer attaches a receipt summarizing the on-chain outcome of every extrinsic submitted by the operation:

```text
struct OperationReceipt {
    extrinsics: Vec<ExtrinsicRecord>,
}

struct ExtrinsicRecord {
    extrinsic_hash: ExtrinsicHash,
    outcome:        ExtrinsicOutcome,
}

enum ExtrinsicOutcome {
    Succeeded {
        block_hash:     BlockHash,
        affected_coins: Vec<CoinAccountId>,    // consumed and created together
    },
    Rejected {
        reason: String,
    },
}
```

For a multi-extrinsic operation, the receipt may contain a mix of `Succeeded` and `Rejected` records — `Done` means *at least one* extrinsic succeeded (§5.5); the caller introspects per-extrinsic outcomes here.

The receipt is emitted as part of the terminal `Done` status item. Per §7.4, the layer may drop the operation record (and the receipt) immediately after emission.

## 10. Errors

```text
enum Error {
    // Pre-submission
    PurseNotFound(PurseId),
    OperationNotFound(OperationHandle),
    CannotDeleteMainPurse,
    PurseHasInFlightOperations,
    OutputsDoNotSumToAmount,
    InsufficientFunds        { requested: Amount, available: Amount },
    InsufficientExternalFunds,
    NoReadyEntries           { requested: Amount, available_when_ready: Amount },
    NoUnloadToken,           // neither free nor paid tokens available
    BadCoinSecret,

    // Post-submission / chain
    SnipedCoin,
    ChainRejected { extrinsic_hash: ExtrinsicHash, reason: String },

    // Lifecycle
    Cancelled,
    InterruptedPreSubmission,

    // Internal
    StorageError(String),
    SubscriptionError(String),
    RecoveryFailed(String),
    Internal(String),
}
```

## 11. Events

```text
enum LayerEvent {
    Resynced,                          // post-restart reconciliation complete

    PurseCreated  { purse: PurseId, name: String },
    PurseRenamed  { purse: PurseId, name: String },
    PurseDeleted  { purse: PurseId, drained_into: PurseId, amount: Amount },

    CoinAvailable { purse: PurseId, exponent: DenominationExponent },
    CoinSpent     { purse: PurseId, exponent: DenominationExponent },
    CoinAged      { purse: PurseId, exponent: DenominationExponent, age: u16 },

    EntryAllocated         { purse: PurseId, exponent: DenominationExponent },
    EntryReadinessChanged  { purse: PurseId, exponent: DenominationExponent,
                             new_state: RecyclerEntryOnChainState },
    EntryConsumed          { purse: PurseId, exponent: DenominationExponent },

    OperationStarted   { handle: OperationHandle, kind: OperationKind, purse: PurseId },
    OperationProgress  { handle: OperationHandle, status: OperationStatus },
    OperationCompleted { handle: OperationHandle, terminal: TerminalStatus },

    MaintenanceSweepStarted   { purses: Vec<PurseId> },
    MaintenanceSweepCompleted {
        coins_recycled:   u32,    // coin → entry
        entries_rescued:  u32,    // entry → coin
        failed:           u32,
    },
}
```

Records are identified by `(purse, exponent)`, not by derivation index — derivation indices are not part of the API. `Resynced` is emitted exactly once after the layer completes post-restart reconciliation; subscribers treat earlier events as reconstruction and later events as live state changes.

## 12. Trust boundaries

### 12.1 No raw cryptography across the API

The layer holds and uses, but never returns to the caller, any signing key derived from root entropy except as the explicit return value of `export_coins`. The API otherwise exposes only structured values: balances, denominations, ages, readiness states, opaque handles, receipts, errors, events. `export_coins` is the single named exception.

### 12.2 Information surface

To the caller, the layer exposes per-purse identity, name, and balance triples; per-operation handles, status streams, and receipts; coin and recycler-entry aggregates via balance and events. Records are not individually addressable from the API.

To the chain, the layer is an ordinary coinage protocol participant.

### 12.3 Durable-state confidentiality

The layer's durable store holds operation records (with extrinsic hashes), local-only timestamps, derivation-index counters, and the root entropy (or a handle to it). Implementations MUST treat the store as confidential and SHOULD encrypt it at rest. The exact scheme is implementation-defined.

## 13. Bootstrap and recovery

The layer is initialized with root entropy supplied by the caller. The main purse exists by construction once entropy is present. No non-main purses exist on first initialization; the caller is expected to track non-main purse identifiers and supply them to `recover` if local durable state is ever lost.

Recovery from root entropy alone is mandatory: given entropy and a list of purse identifiers to restore, the layer reconstructs durable records by chain scanning (Appendix C). Recovery loses local-only state the chain cannot witness — per-entry jitter timestamps reset (entries become immediately eligible once chain readiness is satisfied), and pre-submission operation records are gone.

## 14. Open questions

- **Coinage runtime evolution.** Pallet storage / constant / fee changes are not this layer's concern; metadata-aware negotiation is not constrained here.
- **Recovery UX.** Surfacing recovery progress to the user is a layer-above concern.

---

## Appendix A: Recommended parameter values

Tunable. Implementations SHOULD start from the recommended values.

### A.1 `recycle_at_age`
**Value:** `chain_coin_max_age − 2`.
**Why:** Margin against the chain age cap absorbs one or two retry windows under congestion or downtime.

### A.2 `minimum_anonymous_ring_size`
**Value:** `10`.
**Why:** Chain enforces no minimum. A conservative floor.

### A.3 `recycler_entry_jitter_upper_bound`
**Value:** `6 h`, drawn uniformly from `[0, bound]`.
**Why:** Decorrelates load from subsequent unload.

### A.4 `recycling_sweep_interval`
**Value:** `24 h`.
**Why:** Catches anything past the threshold within a day.

### A.5 `free_token_counter_search_range`
**Value:** `[0, 10)`.
**Why:** Matches the chain per-period allowance. Must not exceed it.

### A.6 `period_lookback_grace`
**Value:** `1 h`.
**Why:** Absorbs transactions prepared near a period boundary.

### A.7 `recovery_batch_size`
**Value:** `500`.
**Why:** Balances per-batch RPC cost against gap-detection responsiveness.

### A.8 `recovery_gap_limit`
**Value:** `4 consecutive empty batches`.
**Why:** With `batch_size = 500`, tolerates gaps up to 2000 indices.

### A.9 `max_split_outputs`
**Value:** `32` (chain-enforced).
**Why:** Pallet cap on outputs per split / unload-into-coins extrinsic.

### A.10 `max_recycler_entries_per_group`
**Value:** `8` (chain-enforced; pallet `MaxConsolidation`).
**Why:** Pallet cap on entries consolidated per unload-into-coins extrinsic.

### A.11 `external_offload_retry_interval`
**Value:** `30 s`.
**Why:** Short wake-up used by `external_offload` when the deficit could be covered by coins currently in transient states (locked / recycling / pending-transfer). Long enough to give those transients a chance to settle; short enough to keep the operation responsive.

### A.12 `ring_expiration_sweep_interval`
**Value:** `24 h`.
**Why:** Periodic schedule for the ring-expiration rescue sweep (§6.4). Same cadence as the coin-age sweep — there is no reason to run them at different frequencies and a single nightly schedule simplifies operations.

### A.13 `rescue_margin`
**Value:** `25 % of RecyclerExpirationTime`, or at minimum `7 days`, whichever is larger.
**Why:** Slack between the rescue-sweep trigger time and the chain's actual ring expiration. Must be large enough to absorb (a) gaps between sweeps when the host is rarely active, (b) congestion delays for the unload extrinsic, (c) the per-entry jitter and ring-fill time of the rescued coin's eventual re-recycling. Too small → rescue races the chain cleanup. Too large → premature rescue, more unload tokens consumed than necessary.

## Appendix B: Recommended derivation scheme

Hard junctions throughout. The key-type tag separates the sr25519 sub-tree used for coin keys from the Bandersnatch sub-tree used for recycler-entry keys, so each sub-tree can be enumerated independently during recovery.

Paths:

```text
// Coin at item I in purse P:
//coinage//coin//<P>//<PAGE>//<I>

// Recycler entry at item I in purse P:
//coinage//ring-vrf//<P>//<PAGE>//<I>
```

- All segments are hard junctions.
- `<P>` is the integer purse identifier. The main purse uses a reserved purse identifier (e.g. `0`) — the purse junction is always present.
- `<PAGE>` is `0` for this version of the design. Future versions may partition a purse's index space across pages; until then, every item lives on page `0`.
- `<I>` is the item index within `(purse, page)`.

This is a clean break from the legacy iOS paths (`//pps//coin//<i>` and `//pps//ring-vrf//<i>`): the root segment changes from `pps` to `coinage`, the purse and page junctions are added, and existing main-purse coins are not on the new path.

Coin and recycler-entry index counters are maintained independently per purse. Recovery scans the coin sub-tree (sr25519, querying `Coinage::CoinsByOwner`) and the recycler-entry sub-tree (Bandersnatch, querying recycler-location storage) independently, each with its own gap-limit scan (Appendix C).

## Appendix C: Recommended recovery algorithm

Parameters: `batch_size`, `gap_limit` (Appendix A.7, A.8).

```text
recover(non_main_purse_ids):
    for purse in {MAIN_PURSE} ∪ non_main_purse_ids:
        recover_coins(purse)
        recover_entries(purse)

recover_coins(purse):
    cursor = 0
    empty_batches = 0
    while empty_batches < gap_limit:
        idxs    = [cursor, cursor + batch_size)
        accts   = derive_coin_accounts(purse, idxs)
        results = query_coin_storage(accts)     // bulk RPC
        for (i, r) in zip(idxs, results):
            if r is Some((exponent, age)):
                persist Coin { purse, derivation_index: i,
                               exponent, age: Some(age), state: Available }
        empty_batches = (empty_batches + 1) if all None else 0
        cursor += batch_size

recover_entries(purse):
    // analogous over recycler-location storage; each found entry
    // is persisted with on_chain_state derived from chain reply,
    // local_state = Available, allocated_at = now, ready_at = .distantPast.
```

`extend_scan` runs the same algorithm starting at supplied non-zero cursors, for use when a gap is suspected past the previous stopping point.
