---
title: "Coinage Management Component — API Contract"
status: "Draft"
---

# Coinage Management Component — API Contract

Companion to [`coinage-management.md`](./coinage-management.md). Names and shapes here are normative.

## 1. Notation

- Pseudocode is Rust-flavoured. `T?` = `Option<T>`. `Stream<T>` = async stream.
- MUST / SHOULD / MAY per RFC 2119.
- "Caller" = the RFC‑6 / RFC‑17 layer plus the cheque transport adapter. Not a product.
- Integer widths and byte lengths below are illustrative unless chain-fixed.

## 2. Durable records

All records MUST survive restart. Storage layout is implementation-defined.

### 2.1 Purse

```text
Purse {
    id:          PurseId,
    name:        String,
    creator:     CreatorId,
    created_at:  Timestamp,
}
```

Invariants:

- `id` unique. Never reused after deletion.
- Exactly one `Purse` with `id = MAIN_PURSE_ID`; its `creator` is a reserved value.
- `name`, `creator` are caller-supplied, not interpreted.

### 2.2 Coin

```text
Coin {
    purse:             PurseId,
    derivation_index:  CoinIndex,
    exponent:          DenominationExponent,
    age:               Age?,
    state:             CoinState,
}

enum CoinState {
    Pending,
    Available,
    LockedFor(OperationHandle),
    Spent,
}
```

State transitions: §3.1.

Invariants:

- `(purse, derivation_index)` unique.
- `derivation_index` within a `purse` never reused.
- `age` is `None` until first chain observation.
- `exponent` fixed for record lifetime.
- `state` per §3.1.

### 2.3 Recycler entry

```text
RecyclerEntry {
    purse:             PurseId,
    derivation_index:  RecyclerEntryIndex,
    exponent:          DenominationExponent,
    allocated_at:      Timestamp,
    ready_at:          Timestamp,
    placement:         RecyclerPlacement?,
    on_chain_state:    RecyclerEntryOnChainState,
    local_state:       RecyclerEntryLocalState,
}

RecyclerPlacement {
    ring_index:    RingIndex,
    member_count:  u32,
}

enum RecyclerEntryOnChainState {
    Missing,
    Waiting,
    Ready,
    Degraded { member_count: u32 },
}

enum RecyclerEntryLocalState {
    Available,
    LockedFor(OperationHandle),
    Consumed,
}
```

State transitions: §3.2.

Invariants:

- `(purse, derivation_index)` unique. Index never reused.
- `ready_at = allocated_at + jitter_delay` (overview §A.3).
- `placement` is `None` until the chain confirms ring assignment.
- Selectable iff `local_state = Available` ∧ `on_chain_state ∈ {Ready, Degraded}` ∧ `ready_at ≤ now`.

### 2.4 Receivable

```text
Receivable {
    id:                ReceivablePublicKey,
    purse:             PurseId,
    secret_handle:     SecretHandle,    // never crosses API
    created_at:        Timestamp,
    state:             ReceivableState, // Open | Closed
    return_context:    ReturnContext?,
}
```

Invariants:

- `id` unique.
- A `Closed` receivable accepts no further deposits. Implementations MAY garbage-collect long-idle receivables but MUST preserve any with non-empty `return_context`.
- `return_context` populated by `deposit_cheque` when a return hint is supplied (§6.6).

### 2.5 Operation

```text
Operation {
    handle:           OperationHandle,
    kind:             OperationKind,
    purse:            PurseId,
    locked_coins:     Set<(PurseId, CoinIndex)>,
    locked_entries:   Set<(PurseId, RecyclerEntryIndex)>,
    submitted:        Vec<ExtrinsicSubmission>,
    status:           OperationStatus,
    created_at:       Timestamp,
    updated_at:       Timestamp,
}

ExtrinsicSubmission {
    extrinsic_hash:   ExtrinsicHash,
    block_hash:       BlockHash?,
    affected_coins:   Vec<CoinAccountId>,
    affected_entries: Vec<MemberKey>,
}
```

Invariants:

- `handle` unique, stable across restart.
- `kind` fixed at start.
- Locks released exactly when `status` is terminal.
- Every coin in `locked_coins` has `state = LockedFor(handle)`; same for entries.
- `submitted` is append-only.
- `status` per §3.3.

### 2.6 Cross-record invariants

- Every `Coin`, `RecyclerEntry`, `Receivable` references an existing `Purse`.
- A purse cannot be deleted while it has open receivables.
- Every lock in `Operation` corresponds to exactly one record in `LockedFor(handle)` state, and vice versa.

## 3. State machines

### 3.1 Coin

```text
            allocated by an operation
                       │
                       v
                 ┌───────────┐
                 │  Pending  │   (not yet observed on chain; age = None)
                 └─────┬─────┘
                       │ chain confirms account with age
                       v
                 ┌───────────┐
                 │ Available │ ◄──────────────────────────┐
                 └─────┬─────┘                            │
                       │ operation locks                  │
                       │                                  │ release on
                       v                                  │ pre-submission
              ┌──────────────────┐                        │ abort or cancel
              │ LockedFor(opid)  │────────────────────────┘
              └─────┬────────────┘
                    │ operation finalizes, chain shows consumed
                    v
                ┌─────────┐
                │  Spent  │   (terminal; retained for no-reuse;
                └─────────┘    GC by policy)
```

- Selection takes only `Available` coins.
- `Pending` → `Available` on first chain observation.
- `LockedFor` → `Available` on pre-submission release.
- `LockedFor` → `Spent` on chain-confirmed consumption.

### 3.2 Recycler entry

Two dimensions, observed independently.

**On-chain:**

```text
        ┌──────────┐  chain shows          ┌─────────┐
        │ Missing  │ ─ recycler location ► │ Waiting │
        └──────────┘                       └────┬────┘
                                                │ ring member-count ≥ floor
                                                v
                                         ┌──────────────┐
                                         │    Ready     │
                                         └──────────────┘
                                                ▲
                                                │ floor breach (rare)
                                                │
                                         ┌──────────────────────┐
                                         │ Degraded(n), n<floor │
                                         └──────────────────────┘
```

- `Missing`: no chain location (load not finalized, or entry consumed → delete record).
- `Waiting`: chain location present, ring still onboarding / readiness unmet.
- `Ready`: ring meets the anonymity floor (overview §A.2).
- `Degraded(n)`: usable, but caller SHOULD surface to user.

**Local lifecycle:**

```text
                 ┌───────────┐
                 │ Available │ ◄────────────── pre-submission release
                 └─────┬─────┘
                       │ operation locks
                       v
              ┌──────────────────┐
              │ LockedFor(opid)  │
              └─────┬────────────┘
                    │ unload finalizes
                    v
                ┌──────────┐
                │ Consumed │   (record deleted)
                └──────────┘
```

### 3.3 Operation

```text
   ┌────────────┐  cancel        ┌─────────────┐
   │ Preparing  │ ─────────────► │ Failed(...) │
   └─────┬──────┘                └─────────────┘
         │ first extrinsic submitted
         v
   ┌────────────┐  chain reject  ┌─────────────┐
   │ Submitted  │ ─────────────► │ Failed(...) │
   └─────┬──────┘                └─────────────┘
         │ included
         v
   ┌────────────┐
   │  InBlock   │
   └─────┬──────┘
         │ finalized
         v
   ┌────────────┐ more work     ┌──────────────┐
   │ Finalized  │─────────────► │  Submitted   │
   └─────┬──────┘               └──────────────┘
         │ all reconciled
         v
   ┌──────────┐
   │   Done   │
   └──────────┘
```

`Submitted`/`InBlock`/`Finalized` may recur. `Done` is reached only after all chain effects are reconciled with local records. `Done` and `Failed` close the stream.

## 4. Subscriptions

All reactive. Initial value emitted at subscribe; new values on change.

### 4.1 Purse balance

```text
fn subscribe_purse_balance(purse: PurseId) -> Stream<PurseBalance>

struct PurseBalance {
    spendable:  Amount,
    pending:    Amount,
}
```

- `spendable` = sum of `Available` coins + value of recycler entries that are `(Ready | Degraded) ∧ local_state = Available ∧ ready_at ≤ now`.
- `pending` = sum of `LockedFor` coins + entries that are `Waiting | Missing | LockedFor | ready_at > now`.

### 4.2 Operation status

```text
fn subscribe_operation_status(handle: OperationHandle) -> Stream<OperationStatus>
```

Closes after emitting a terminal status. Handle remains valid for synchronous reads.

### 4.3 Component events

```text
fn subscribe_events() -> Stream<ComponentEvent>
```

Taxonomy in §9. Emits a `Resynced` event after post-restart reconciliation completes.

## 5. Identifiers and amounts

```text
type PurseId              = u32        // MAIN_PURSE_ID reserved
type CoinIndex            = u32
type RecyclerEntryIndex   = u32
type OperationHandle      = OpaqueBytes
type ReceivablePublicKey  = [u8; 32]
type CoinAccountId        = [u8; 32]
type MemberKey            = [u8; 32]
type RingIndex            = u32
type ExtrinsicHash        = [u8; 32]
type BlockHash            = [u8; 32]
type CreatorId            = String
type Age                  = u16
type Timestamp            = i64        // Unix seconds
type DenominationExponent = i8
type Amount               = u64        // coinage cents; overview §5.5
```

## 6. Operation primitives

All long-running primitives return:

```text
struct OperationStart {
    handle:  OperationHandle,
    status:  Stream<OperationStatus>,
}
```

### 6.1 Purse lifecycle

```text
fn create_purse(name: String, creator: CreatorId)
    -> Result<PurseId, ComponentError>

fn query_purse(purse: PurseId)
    -> Result<PurseInfo, ComponentError>

struct PurseInfo {
    id:         PurseId,
    name:       String,
    creator:    CreatorId,
    created_at: Timestamp,
    spendable:  Amount,
    pending:    Amount,
}

fn rebalance_purse(from: PurseId, to: PurseId, amount: Amount)
    -> Result<OperationStart, ComponentError>

fn delete_purse(target: PurseId, drain_into: PurseId)
    -> Result<OperationStart, ComponentError>
```

`create_purse`, `query_purse`: synchronous, no chain interaction.

`rebalance_purse`: on-chain transfer between purse derivation namespaces. Source records become `Spent`/`Consumed`; destination records appear in `to`'s namespace.

`delete_purse`: drains via rebalance then closes the purse record. Main purse cannot be deleted; purse with open receivables cannot be deleted.

Errors: `PurseNotFound`, `InsufficientFunds`, `NoReadyVouchers`, `CannotDeleteMainPurse`, `PurseHasOpenReceivables`, `ChainRejection`, `Cancelled`.

### 6.2 Funding

```text
trait FundingOrigin {
    fn external_account(&self) -> ExternalAccountId;
    fn sign_payload(&self, payload: &[u8]) -> Signature;
}

fn top_up(into: PurseId, amount: Amount, origin: &dyn FundingOrigin)
    -> Result<OperationStart, ComponentError>
```

Decomposes `amount` into recycler-entry denominations, allocates fresh indices in `into`, and submits one external-asset load per denomination signed by `origin`. Per-entry outcomes are reported in the status stream; partial success does not roll back successes.

Errors: `PurseNotFound`, `InsufficientExternalFunds`, `ChainRejection`.

### 6.3 Direct transfer

```text
fn transfer(
    from:                  PurseId,
    amount:                Amount,
    recipient_outputs:     Vec<RecipientOutput>,
    sender_memo_callback:  Option<MemoCallback>,
) -> Result<OperationStart, ComponentError>

struct RecipientOutput {
    exponent: DenominationExponent,
    account:  CoinAccountId,
}

type MemoCallback = fn(memo_entries: Vec<MemoEntry>);
```

Total of `recipient_outputs` MUST equal `amount`. Component selects from `from` and routes to the supplied recipient accounts via the three-tier strategy (overview §5.6).

If `sender_memo_callback` is supplied, the component invokes it once per executed transfer with `MemoEntry` values; the component itself does not encode or transmit memos (Appendix C).

Errors: `PurseNotFound`, `InsufficientFunds`, `NoReadyVouchers`, `OutputsDoNotSumToAmount`, `ChainRejection`, `Cancelled`.

### 6.4 Receivable lifecycle

```text
fn create_receivable(into: PurseId)
    -> Result<ReceivablePublicKey, ComponentError>

fn close_receivable(receivable: ReceivablePublicKey)
    -> Result<(), ComponentError>
```

`create_receivable`: fresh keypair, secret retained internally, persisted under `into`.

`close_receivable`: no further deposits accepted. A purse cannot be deleted while it has open receivables.

### 6.5 Create cheque

```text
fn create_cheque(from: PurseId, to: ReceivablePublicKey, amount: Amount)
    -> Result<ChequeStart, ComponentError>

struct ChequeStart {
    handle:  OperationHandle,
    status:  Stream<OperationStatus>,
    cheque:  Stream<ChequeBlob>,   // emits once, then closes
}

type ChequeBlob = OpaqueBytes;
```

Selects from `from`, executes any required split / unload-into-coins extrinsics so the chosen coins exist as separate accounts at the right denominations, then encrypts their secrets to `to` and emits the blob.

The selected coins remain locked in `from` and remain under `from`'s keys on chain until the receiver deposits. The receiver gains control only on deposit (§6.6).

Errors: `PurseNotFound`, `ReceivableNotFound`, `InsufficientFunds`, `NoReadyVouchers`, `ChainRejection`, `Cancelled`.

### 6.6 Deposit cheque

```text
fn deposit_cheque(blob: ChequeBlob, return_hint: Option<ReturnHint>)
    -> Result<OperationStart, ComponentError>

struct ReturnHint {
    sender_account: CoinAccountId,
    note:           OpaqueBytes?,    // caller-opaque payload
}
```

Reads the receivable id from the blob, decrypts secrets, and submits a transfer per coin moving it from the sender-controlled account into a fresh coin account in the receivable's purse. Status stream reports per-coin outcomes; partial success is possible (some coins sniped).

`return_hint`, if supplied, is persisted in `Receivable.return_context` for future refunds.

Errors: `ReceivableNotFound`, `BadCheque{reason}`, `BadCoins`, `SnipedCoins`, `ChainRejection`, `Cancelled`.

### 6.7 Refund

```text
fn refund(receivable: ReceivablePublicKey, amount: Amount?)
    -> Result<OperationStart, ComponentError>
```

Returns `amount` (or all received value if `None`) to the sender recorded in `Receivable.return_context`. Best-effort: if the originally-received coins are gone, falls back to spending other coins from the receivable's purse. Component does not earmark received coins for refunds.

Errors: `ReceivableNotFound`, `RefundUnavailable`, `InsufficientFunds`, `NoReadyVouchers`, `ChainRejection`, `Cancelled`.

### 6.8 External offload

```text
fn external_offload(from: PurseId, amount: Amount, destination: ExternalAccountId)
    -> Result<OperationStart, ComponentError>
```

Moves `amount` from `from` to a non-coinage account. Biases selection toward `unload-into-external-asset`; uses the unload-and-reload variant where leftover value would otherwise sit unprotected. Fee mode auto-selected (overview §5.9).

Errors: `PurseNotFound`, `InsufficientFunds`, `NoReadyVouchers`, `ChainRejection`, `Cancelled`.

### 6.9 Recycling sweep

```text
fn run_recycling_sweep(purses: Option<Vec<PurseId>>)
    -> Result<OperationStart, ComponentError>
```

Runs the sweep once against the listed purses (or all if `None`). Sequential per coin, oldest first. Per-coin outcomes reported in the status stream. The component also runs this autonomously on a periodic timer; this primitive forces a run.

Errors: `PurseNotFound`.

### 6.10 Payment classification

```text
fn classify_incoming_payment(parsed_entries: Vec<MemoEntry>)
    -> Result<PaymentClassification, ComponentError>

struct MemoEntry {
    sender_coin_account: CoinAccountId,
    recipient_account:   CoinAccountId,
    recipient_index:     CoinIndex,
}

enum PaymentClassification {
    Matched,    // every entry corresponds to a coin in this component
    Received,   // partial; caller should retry
    Unmatched,  // no entries correspond
    Spent,      // entries corresponded but coins are gone
}
```

Synchronous, no operation started. Empty input → `Unmatched`. Used by the chat layer to drive a payment-state UI without re-querying chain.

## 7. Receipts

```text
struct OperationReceipt {
    extrinsics: Vec<ExtrinsicRecord>,
}

struct ExtrinsicRecord {
    extrinsic_hash:  ExtrinsicHash,
    block_hash:      BlockHash,
    affected_coins:  Vec<CoinAccountId>,
}
```

Emitted in the terminal `Done` status item and retained on the operation record. RFC‑17 transforms this into `CoinPaymentClearingReference`, redacting as needed.

## 8. Errors

```text
enum ComponentError {
    // Pre-submission
    PurseNotFound(PurseId),
    ReceivableNotFound(ReceivablePublicKey),
    OperationNotFound(OperationHandle),
    InsufficientFunds        { requested: Amount, available: Amount },
    NoReadyVouchers          { requested: Amount, available_when_ready: Amount },
    InsufficientExternalFunds,
    CannotDeleteMainPurse,
    PurseHasOpenReceivables,
    OutputsDoNotSumToAmount,
    RefundUnavailable,
    BadCheque { reason: String },

    // Post-submission / chain
    BadCoins,
    SnipedCoins,
    ChainRejection { extrinsic_hash: ExtrinsicHash, reason: String },

    // Lifecycle
    Cancelled,

    // Internal
    StorageError(String),
    SubscriptionError(String),
    Internal(String),
}
```

RFC‑17 mapping (in the RFC‑17 layer):

| Component | RFC‑17 |
|-|-|
| `InsufficientFunds` | `BalanceLow` |
| `NoReadyVouchers` | UI-surfaced wait, else `BalanceLow` |
| `Cancelled` | `Denied` |
| `BadCoins` | `BadCoins` |
| `SnipedCoins` | `SnipedCoins` |
| `PurseNotFound` | `PurseNotFound` |
| `ReceivableNotFound` | `ReceivableNotFound` |
| `ChainRejection` | `Internal` (logged, not exposed) |
| `StorageError`, `SubscriptionError`, `Internal` | `Internal` |

## 9. Events

```text
enum ComponentEvent {
    Resynced,

    PurseCreated   { purse: PurseId, creator: CreatorId, name: String },
    PurseDeleted   { purse: PurseId, drained_into: PurseId, amount: Amount },

    ReceivableCreated { receivable: ReceivablePublicKey, purse: PurseId },
    ReceivableClosed  { receivable: ReceivablePublicKey },

    CoinAvailable { purse: PurseId, exponent: DenominationExponent },
    CoinSpent     { purse: PurseId, exponent: DenominationExponent },
    CoinAged      { purse: PurseId, exponent: DenominationExponent, age: Age },

    RecyclerEntryAllocated        { purse: PurseId, exponent: DenominationExponent },
    RecyclerEntryReadinessChanged { purse: PurseId, exponent: DenominationExponent,
                                    new_state: RecyclerEntryReadinessState },
    RecyclerEntryConsumed         { purse: PurseId, exponent: DenominationExponent },

    OperationStarted   { handle: OperationHandle, kind: OperationKind, purse: PurseId },
    OperationProgress  { handle: OperationHandle, status: OperationStatus },
    OperationCompleted { handle: OperationHandle, terminal: TerminalStatus },

    RecyclingSweepStarted   { purses: Vec<PurseId> },
    RecyclingSweepCompleted { recycled: u32, destroyed: u32, failed: u32 },

    IncomingChequeDeposited { receivable: ReceivablePublicKey, amount: Amount },
}
```

Records are identified by `(purse, exponent)`, not by derivation index — indices are not part of the API address space.

`Resynced` fires exactly once after post-restart reconciliation. Subscribers treat earlier events as reconstruction and later events as live changes.

## Appendix A: Derivation scheme (recommended)

Same root entropy → same coin and recycler-entry accounts. Purse-scoped paths under the coinage root:

```text
// Coin at index I in purse P:
//coinage//<P>//<page><deriv_sec>/<I>

// Recycler entry at index I in purse P:
// purse-scoped equivalent, with <P> inserted immediately after //coinage
```

Purse ID is a hard junction after `//coinage`. Main purse uses a reserved purse identifier. Matches RFC‑17 Appendix A.

Properties: non-overlapping purse namespaces; recoverable from root entropy; new purses cost only an identifier.

## Appendix B: Recovery algorithm (recommended)

Parameters `batch_size`, `gap_limit` tunable (overview §A.7, §A.8).

```text
recover():
    for each known purse id (main + product purses backed up by the layer above):
        recover_coins(purse)
        recover_recycler_entries(purse)

recover_coins(purse):
    cursor = 0
    empty_batches = 0
    while empty_batches < gap_limit:
        idxs    = [cursor .. cursor + batch_size)
        accts   = derive_coin_accounts(purse, idxs)
        results = query_coin_storage(accts)            // bulk RPC
        for (i, r) in zip(idxs, results):
            if r is Some((exponent, age)):
                persist Coin { purse, derivation_index: i,
                               exponent, age: Some(age), state: Available }
        empty_batches = (empty_batches + 1) if all None else 0
        cursor += batch_size

recover_recycler_entries(purse):
    // analogous against recycler-location storage; persisted records get
    // local_state = Available, allocated_at = now, ready_at = .distantPast
    // (jitter lost; entry eligible once chain readiness is satisfied).
```

`extend_scan(purse, start_index)` is exposed so callers can probe deeper if a gap is suspected.

Product purse identifiers are bootstrap data the layer above MUST persist alongside the user's backup; without them only the main purse is recoverable.

## Appendix C: Memo classification interface

The component does not know the wire encoding of direct-transfer memos. The chat layer owns the schema; the component owns the matching primitive (§6.10).

Adapter shape:

1. Chat layer encodes `Vec<MemoEntry>` into its wire format (e.g. SCALE-encoded `TransferMemo`) and attaches to a chat message.
2. Recipient's chat layer decodes back to `Vec<MemoEntry>`, calls `classify_incoming_payment`.
3. Chat layer drives the per-message UI state machine from the returned `PaymentClassification`.

On-chain transfer outcomes are independent of the memo — coins are owned regardless. Memo is metadata for UI only.
