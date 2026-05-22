# Coinage Layer — Work-in-Progress Notes

Working handoff for continuing the Coinage Layer design + Quint formal-spec work.
Read this top-to-bottom and you should have everything needed to resume.

## 1. Repo state

- Branch: `add-coinage-design`, PR #122 open against `main`.
- Last committed work: `61c61f5` "docs: add coinage management component design" (the original unified design doc, before the bottom-layer split). PR is open with that commit.
- **Uncommitted** work-in-progress on disk:
  - `docs/design/coinage-layer.md` — the bottom-layer design (the one we want to land).
  - `docs/specs/coinage-layer.qnt` — the Quint formal spec (working skeleton).
  - `docs/specs/COINAGE-LAYER-WORK-NOTES.md` — this file.
- The earlier doc `docs/design/coinage-management.md` + `docs/design/coinage-management-contract.md` are the original *unified* design. The user explicitly asked NOT to touch them; the new bottom-layer split lives in `coinage-layer.md`.

## 2. Context — what this is about

The user is rebuilding the protocol-layer design for the Triangle Host's coinage subsystem. Two existing implementations grew organically without a design:
- iOS app `paritytech/polkadot-app-ios-v2` (branches `develop` and `feature/payment-request`)
- Rust crate `paritytech/useragent-kit`

Goal: write the design that SHOULD have come first, split into a bottom layer (this work) and a top RFC‑17 layer (later). Then formally verify both layers via Quint, and eventually verify a Rust implementation against the spec.

## 3. Architecture: two-layer split

- **Bottom layer (Coinage Layer)** — self-contained coinage. Owns coins, recycler entries, purses, recycling, selection, unload tokens. Knows nothing about RFC‑17 product concepts.
- **Top layer (Coinage Payment / RFC‑17)** — adds receivables, cheques, refunds, RFC‑17 product-facing API. Built on bottom-layer primitives.
- **Layer seam**: `export_coins` and `import_coins`. The only API points where coin secrets cross the boundary. Top layer wraps these to build RFC‑17 cheques.

## 4. Design decisions baked into `coinage-layer.md`

All committed dialog answers, in one place:

| Decision | Choice |
|-|-|
| Scope | Internal layer, contract-aware (interface boundary fully specified) |
| Purse model | Purse-aware (one component, many purses, main purse has reserved id) |
| Recycling | Both — payment-folded + periodic backstop sweep |
| State sync | Reactive subscriptions |
| Local locks | Full lifecycle states for coins and entries |
| Coin index allocation | Strict no-reuse invariant |
| Entry index allocation | Same no-reuse invariant |
| Balance model | Spendable + spendable_strict + pending per purse |
| Payment primitives | Both direct transfer AND cheque (via export/import seam) |
| Voucher jitter | SHOULD (recommended), not MUST |
| Anonymity floor | Explicit Ready / Degraded; global to layer (not per-purse) |
| Unload concurrency | Design-agnostic (constraints + per-group outcomes specified) |
| Memo classification | First-class primitive; memo bytes opaque |
| Refunds | First-class with stored return context (top layer concern) |
| Funding source | Abstract origin (opaque signing authority) |
| Cheque transport | Blob in/out; transport external to layer |
| API boundary | No raw crypto across API except export_coins as named exception |
| Unload tokens | Free + paid, automatic fallback |
| Recovery | Required from entropy alone; mechanism in appendix |
| Purse metadata | Layer owns full metadata (id, name only — creator etc. is RFC-17 layer) |
| Receipt shape | Per-extrinsic with affected coin account IDs and outcome |
| Status stream | Internal lifecycle (Preparing/Submitted/InBlock/Finalized/Waiting/Done/Failed) |
| Selection | Three-tier prescribed (exact / split / unload); deterministic order |
| Restart durability | Full — operations resume; subscriptions torn down |
| Cancellation | Pre-submission and Waiting only |
| Errors | Internal taxonomy; RFC-17 layer maps |
| Sweep triggers | Periodic + opportunistic |
| Observation | Per-purse balance + per-op status + typed event stream |
| Direct-transfer memo | Component models as opaque blobs |
| Index scope | Per-purse |
| Terminology | "Recycler entry" (not "voucher") |
| Derivation paths | `//coinage//coin//<P>//<PAGE>//<I>` and `//coinage//ring-vrf//<P>//<PAGE>//<I>` — all hard junctions, page=0 for now, main purse uses purse id 0 |
| External offload | Multi-phase planner-driven; auto-recycles coins; supports Waiting state; surplus always atomically reloaded as fresh entries; defaults `allow_degraded = false` |
| `host_payment_request` mapping | Maps to `external_offload`, NOT `export_coins` |
| Ring-expiration rescue | Second autonomous sweep (entry → coin via `unload_recycler_into_coins`) — mandated; iOS bug to be filed |
| Maintenance sweep API | Unified `run_maintenance_sweep` covering both age-recycle AND ring-rescue |

## 5. Key bug discovered

**iOS silent loss-of-funds bug** (both `develop` and `feature/payment-request`):

iOS's `CoinageRecyclingService` only recycles coins INTO entries. It never unloads entries OUT. If a user tops up (creating entries) and doesn't open the app long enough for the ring to be cleaned up by chain (`immutableSince + RecyclerExpirationTime`), the entry's backing value is destroyed silently.

This is what motivated the **ring-expiration rescue sweep** in §6.4 of the new design.

Should be filed as a security-grade bug against the iOS app independent of this design work.

## 6. Quint spec status

File: `docs/specs/coinage-layer.qnt`. Currently ~2100 lines. **All 12 work-plan steps complete.**

### What's modeled
- **State machines** — coin lifecycle, entry on-chain readiness (with anonymity floor), entry local lifecycle, operation status (full Submitted/InBlock/Finalized/Waiting/Done/SFailed(reason) progression).
- **Primitives** — `createPurse`, `renamePurse`, `deletePurse`, `rebalancePurse`, `topUp`, `transfer`, `transferAmount` (multi-coin selection), `exportCoin`, `importCoin`, `startExternalOffload`, `cancelOp`, `opOffboard`, `opEnterWait`, `opWake`, `opAdvanceToSubmitted`, `opAdvanceToInBlock`, `opAdvanceToFinalized`, `opChainReject`, `coinAgeRecycle`, `ringExpirationRescue`, `runMaintenanceSweep`, `restart`, `joinPaidRing`, `topUpFeeAccount`, `recover`, `extendScan`, `chainPromoteToReady`, `chainSealRing`, `chainExpireRing`, `tick`.
- **Functional defs** — `queryPurse`, `purseSpendable`, `purseSpendableStrict`, `pursePending`, `classifyIncomingPayment`, `selectionFeasible` (3-tier covering predicate), `deriveCoinAccount`, `deriveMemberKey` (Appendix B derivation).
- **State variables** — purses, coins, entries, operations, rings, receipts (per-handle list of `ExtrinsicRecord`), events (typed log per §11), tokens (period × class × counter), paid-ring membership, fee account balance, time.
- **Failure modes** — `FailureReason` enum (`FRSnipedCoin`, `FRChainRejected`, `FRCancelled`, `FRInterruptedPreSubmission`, `FRStorage`, `FRSubscription`, `FRRecovery`, `FRInternal`).
- **Unload tokens** — free (per-period, indexed counter) and paid (with one-time ring-join fee). Fee-mode pick (`FMPrepaid` vs `FMFromOutput`) follows §6.6.

### Invariants (all pass under simulator, 5000 traces × 60 steps)

| Invariant | What it asserts |
|-|-|
| `coinIndexBounded`, `entryIndexBounded` | No-reuse namespace invariant |
| `lockConsistency` | Coin/entry locks ↔ operation `lockedCoins`/`lockedEntries` agree |
| `coinAgeBound` | Available coins never reach `MaxAge` |
| `conservation` | `totalIn − totalOut == liveValue` |
| `terminalReleasesLocks` | Terminal ops release all locks |
| `noEntryOnExpiredRing` | The rescue-sweep contract |
| `mainPurseExists` | Main purse never deleted |
| `operationsPurseExists` | Active ops reference live purses |
| `externalOffloadLocksEntriesOnly` | KExternalOffload never locks coins |
| `liveRecordsRefExistingPurse` | Live records reference live purses |
| `terminalOpsHaveReceipts` | Terminal ops have at least one receipt |
| `receiptOutcomeMatchesStatus` | SDone → some XSucceeded; SFailed → some XRejected |
| `terminalOpsHaveCompletedEvent` | Every terminal op has an `EOperationCompleted` event |
| `feeBalanceNonNegative` | Fee account never goes negative |
| `tokenRecordsConsistent` | Token-map keys match record fields |
| `midSubmissionHoldsLocks` | Submitted/InBlock/Finalized ops still hold locks |
| `derivationDeterministic` | Every coin/entry's account/key matches `derive*` |
| `derivationInjective` | Distinct (purse, idx) ⇒ distinct account/key |
| `handleMonotone` | `nextHandle` > every issued handle |
| `ringIntegrity` | `rings.get(r).idx == r` |
| `consumedFreeTokensInRange` | Free-token counters within search range |
| `eventOrderOpStartBeforeComplete` | Op-completed events preceded by op-started events |
| `noCoinResurrection`, `noEntryResurrection` | Records keyed at their own purse+idx |

### Key modeling abstraction
`chainExpireRing` is gated on `ringEntriesAllConsumed(ridx)` — the chain action only fires after every entry on the ring is in terminal state. This encodes the **design contract** that the host rescues entries before the chain destroys them. Without this gate, the simulator finds traces matching the iOS silent-loss bug.

### Verification workflow followed per step
1. `quint typecheck docs/specs/coinage-layer.qnt` — must be clean.
2. `quint run docs/specs/coinage-layer.qnt --invariant=safety --max-samples=2000 --max-steps=50` — must pass.
3. New invariants — additionally checked individually with `--invariant=NAME`.

### Possible future tightening
- **Apalache check.** `quint verify --apalache` for bounded-state symbolic checking. Currently simulator-only.
- **Per-tier selection witnesses.** `transferAmount` consumes a caller-supplied subset; tier 2 (split) and tier 3 (unload-into-coins) are subsumed by the predicate `selectionFeasible` but not split into separate actions.
- **Subscription state.** Streams are not modeled (§8.9). Only the underlying event log is.
- **Anonymity-floor enforcement.** The spec carries `OnDegraded` as a state but never produces traces with ring sizes < `AnonymityFloor`. Add ring-size tracking + a chain-side action that promotes to `OnDegraded` if floor not met.
- **Recovery realism.** `recover` currently re-creates an empty purse record; a more faithful model would also reconstruct expected coin/entry records from a hypothetical chain side.

## 7. Verification workflow

Every change to the spec:
1. `quint typecheck docs/specs/coinage-layer.qnt` — must be clean.
2. `quint run docs/specs/coinage-layer.qnt --invariant=safety --max-samples=2000 --max-steps=50` — must pass.
3. For new invariants: also check individually with `--invariant=NAME`.

## 8. Quint syntax cheat sheet (learned the hard way)

- Action params need return type: `action foo(x: int): bool = ...`
- Local `val`s must be **before** an `all { ... }` block, not inside it. Pattern: `action foo(x): bool = { val a = ...; val b = ...; all { conds, effects } }`.
- Record update is `r.with("field", value)`, NOT `{ ...r, field: v }`.
- `Rec` is a built-in name; use `MyRec` etc.
- `nondet x = oneOf(set)\n action(x),` separates branches inside `any { ... }`.
- Set methods include `.fold(init, fn)`, `.filter`, `.map`, `.forall`, `.exists`, `.exclude`, `.contains`.
- Quint CLI: `--invariant=NAME` is on `run` and `verify`, not on `test`.

## 9. Known iOS / useragent-kit references

When in doubt about a design point, the following code is the existing-reality reference:

- `polkadot-app-ios-v2`:
  - `Packages/Coinage/Sources/Recycling/CoinageRecyclingService.swift` — periodic + foreground recycling
  - `Packages/Coinage/Sources/Transfer/CoinSelection/CoinSelector.swift` — three-tier selection
  - `Packages/Coinage/Sources/CoinageBackupRecoveryService.swift` — gap-limit recovery
  - `Packages/Coinage/Sources/ExternalPayment/Planner/ExternalPaymentPlanner.swift` (on `feature/payment-request` branch only) — the planner this design's §8.6 mirrors
  - Derivation paths: `//pps//coin//<i>` and `//pps//ring-vrf//<i>` (legacy; new design uses `//coinage//...`)
- `useragent-kit`:
  - `crates/host-coinage/src/selection.rs` — three-tier selection
  - `crates/host-coinage/src/chain.rs` — recovery, query, transfer
  - `crates/host-coinage/src/unload.rs` — unload token contexts

## 10. Open follow-ups (not yet acted on)

- File the iOS silent-loss-of-funds bug as a security issue.
- The PR #122 description currently describes the original *unified* design. Once the bottom-layer split is finalized, the PR should be updated to reflect the split (or split into two PRs).
- Top-layer (RFC‑17 / Coinage Payment) design has not been written; user wanted to finish the bottom-layer spec first.

## 11. Continuing the work

To resume:
1. Read this file.
2. Read `docs/design/coinage-layer.md` (current design).
3. Read `docs/specs/coinage-layer.qnt` (current spec).
4. Run `quint typecheck docs/specs/coinage-layer.qnt && quint run docs/specs/coinage-layer.qnt --invariant=safety --max-samples=5000 --max-steps=50` to confirm clean baseline.
5. Pick up at step 1 of §6 of this file.
