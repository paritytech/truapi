---
title: "Inbound Payment TrUAPI"
owner: "@TorstenStueber"
---

# RFC 0018 — Inbound Payment TrUAPI

## Summary

This RFC extends the payment surface introduced in [RFC 0006](0006-payments.md) with a receiver-side complement: methods that let products allocate receiving targets, observe inbound payments, manage funds the host has accumulated on the product's behalf, and spend those funds onward. The new methods reuse RFC 0006's `Balance`, `PaymentId`, `PaymentReceipt`, and `PaymentStatus` types, and follow the same "abstraction over implementation" principle — the underlying private payment system (denominations, ages, recyclers, ring proofs, key derivation) stays inside the host.

## Motivation

RFC 0006 defines an outbound payment surface. A product can read the user's balance, top it up from product-controlled funds, request a payment to a destination, and track the resulting status. There is no symmetric inbound surface, so a product cannot:

- generate a way to be paid from another user;
- be notified when an inbound payment has settled;
- enumerate or use funds that have accumulated under the product's control;
- send those funds onward (for refunds, withdrawals, or product-to-product transfers).

This is a blocker for any product that needs to receive payments rather than dispatch them, including (but not specific to) donation flows, peer-to-peer settlement, bill splitting, content paywalls, micropayment streams, subscription billing, marketplace escrow, and similar use cases.

A neutral receiver-side surface lets all of these be built directly on TrUAPI without per-product host extensions. The host continues to own everything that requires user secret material, signing authority, allowance, or chain connectivity (key derivation, signing, statement-store posting, chain observation, recycling). Products own everything that does not require the host (lifecycle state, idempotency, distribution channels, metadata semantics, business policy).

### Stakeholders

- **Product developers** — consumers of TrUAPI building any product that needs to receive private payments.
- **Host implementors** — responsible for receiving-key derivation, denomination planning, chain observation, allowance management, and recycling.
- **End users** — whose privacy and key material must not leak through the receiver-side surface.

## Detailed Design

### Design Principles

1. **Symmetry with RFC 0006.** Receiving complements sending. Lifecycle types and prompt semantics are reused.
2. **Asynchronous settlement.** Like outbound payments, inbound settlement is not synchronous. Status flows through a subscription.
3. **No interpretation of product state.** The host does not understand sales, sessions, subscriptions, channels, currencies beyond the single payment asset, or any product-defined concept. It exposes plumbing only.
4. **Distribution is product-chosen.** The host produces an opaque rendezvous blob; the product decides how to deliver it (QR, NFC, statement store, deep link, custom transport).
5. **Funds are scoped per-product.** Holdings are partitioned by a product-supplied opaque tag so the host can derivation-namespace receiving keys and the product can build its own views.

### API Calls

#### 1. Allocate a receiving target

Allocates a fresh inbound target. The host derives fresh receiving keys, plans how the requested amount will be received, and bundles everything into an opaque rendezvous. The rendezvous is what a payer's host needs in order to construct an inbound payment to this target.

```rust
fn host_payment_inbound_create(
    amount: Balance,
    scope: ScopeTag,
    expires_at_ms: Option<u64>,
) -> Result<InboundPayment, InboundPaymentCreateErr>

type InboundPaymentId = str;

/// Opaque short identifier the product chooses to group receiving
/// targets. The host treats it as bytes — the recommended encoding is
/// a 32-byte hash, but anything up to 64 bytes is allowed. The host
/// uses (productId, scope, host-internal counter) as the receiving-key
/// derivation namespace.
type ScopeTag = Vec<u8>;

/// Opaque blob a payer's host needs in order to construct a payment
/// to this target. Products treat it as opaque; only host
/// implementations can decode it.
type InboundRendezvous = Vec<u8>;

struct InboundPayment {
    id: InboundPaymentId,
    rendezvous: InboundRendezvous,
    expires_at_ms: Option<u64>,
}

enum InboundPaymentCreateErr {
    /// The amount is not representable in the host's payment system.
    AmountUnsupported,
    /// expires_at_ms is in the past.
    ExpiryInPast,
    /// scope exceeds the host's maximum length.
    ScopeTooLong,
    /// The host cannot accept another inbound target right now.
    Capacity,
    Unknown(GenericErr)
}
```

The host MUST integrity-protect the rendezvous so that a payer's host can verify it has not been modified since allocation. The expiry, when set, is part of the integrity-protected payload.

The product distributes the rendezvous bytes through whatever channel it chooses. The host MUST NOT unilaterally publish the rendezvous to the statement store, the chain, or any other transport.

#### 2. Subscribe to inbound payment status

Track the lifecycle of one previously allocated inbound target. The subscription emits exactly one terminal status and then closes.

```rust
fn host_payment_inbound_status_subscribe(
    id: InboundPaymentId,
    callback: fn(InboundPaymentStatus)
) -> Result<Subscriber, InboundPaymentStatusErr>

enum InboundPaymentStatus {
    /// Target is open; no funds yet.
    Pending,
    /// Funds matching the target's amount were received within the
    /// expiry window.
    Received(InboundPaymentEvidence),
    /// Funds were received after expires_at_ms. Product policy decides
    /// whether to honour them. The evidence carries the actual amount.
    LateReceived(InboundPaymentEvidence),
    /// Window expired with no funds received.
    Expired,
}

struct InboundPaymentEvidence {
    /// Actual amount received (may be less than the target's amount in
    /// LateReceived or partial-receipt cases).
    amount: Balance,
    /// Wall-clock time the host considers the funds finalized.
    finalized_at_ms: u64,
    /// Opaque host blob that commits to the chain-anchor data
    /// underlying this receipt. The product persists it; an external
    /// auditor with chain access can verify.
    chain_anchor: Vec<u8>,
    /// Opaque bytes the payer chose to attach. Meaning is
    /// product-defined. Size is capped by the host (recommended floor:
    /// 4096 bytes).
    attached: Option<Vec<u8>>,
}

enum InboundPaymentStatusErr {
    /// id was not found or does not belong to the calling product.
    NotFound,
    Unknown(GenericErr)
}
```

A payer's host is permitted (but not required) to deliver a small opaque blob alongside an inbound payment. This blob arrives in `evidence.attached`. Its meaning is entirely product-defined; common uses include refund channels, order references, or encrypted memos. Delivery is best-effort: if the side-channel fails, the inbound target still completes with `attached: None`.

A product that wants to stop tracking a target before it terminates simply ends the active subscription via the standard `Subscriber` cancellation mechanism. Active cancellation of the target itself is not exposed: targets terminate naturally on `Received` / `LateReceived` / expiry. Funds that arrive at a target after the product has stopped subscribing are still retained by the host and surface in `host_payment_holdings_subscribe` aggregate balances.

#### 3. Pay a rendezvous

Make a payment to a rendezvous published by another product (or the same one on another device). This is the receiver-symmetric counterpart to `host_payment_request` and triggers a user authorization prompt. Returns a `PaymentReceipt` whose `PaymentId` can be tracked via `host_payment_status_subscribe` (defined in RFC 0006).

```rust
fn host_payment_to_rendezvous_request(
    amount: Balance,
    rendezvous: InboundRendezvous,
    options: OutboundPaymentOptions
) -> Result<PaymentReceipt, PaymentToRendezvousErr>

struct OutboundPaymentOptions {
    /// Opaque bytes to deliver to the receiver alongside the payment.
    /// See InboundPaymentEvidence::attached. Size is capped by the host
    /// (recommended floor: 4096 bytes).
    attached: Option<Vec<u8>>,
}

enum PaymentToRendezvousErr {
    /// User denied the payment request.
    Rejected,
    /// User's available balance is not sufficient.
    InsufficientBalance,
    /// Rendezvous bytes cannot be decoded by this host.
    RendezvousInvalid,
    /// Rendezvous expiry has clearly passed (subject to a small clock
    /// tolerance).
    RendezvousExpired,
    /// amount does not fit the rendezvous plan.
    RendezvousMismatch,
    /// attached exceeds the host's maximum size.
    AttachedTooLarge,
    Unknown(GenericErr)
}
```

The host MUST validate the rendezvous expiry against its local clock with a small tolerance (suggested: 30 seconds) before prompting the user. If the rendezvous is clearly expired, the host MUST return `RendezvousExpired` without prompting.

A successful response means the user authorized the payment and the host accepted it for processing. It does not mean the payment has settled — use `host_payment_status_subscribe`.

#### 4. Spend product-held funds

Spend funds the host holds for the calling product to a rendezvous. Used wherever a product needs to send funds onward (for example, returning funds to a payer who attached a refund rendezvous, or transferring between products that share a host).

```rust
fn host_payment_holdings_spend(
    source_scope: ScopeTag,
    amount: Balance,
    rendezvous: InboundRendezvous,
    options: OutboundPaymentOptions
) -> Result<PaymentReceipt, PaymentHoldingsSpendErr>

enum PaymentHoldingsSpendErr {
    /// Same shape as PaymentToRendezvousErr.
    Rejected,
    InsufficientBalance,
    RendezvousInvalid,
    RendezvousExpired,
    RendezvousMismatch,
    AttachedTooLarge,
    /// Caller has no holdings under source_scope.
    ScopeEmpty,
    Unknown(GenericErr)
}
```

The host MAY apply a different prompt policy than `host_payment_to_rendezvous_request` (for example, suppressing the prompt for small refund-shaped operations). Prompt policy is a host implementation choice, not part of the API contract.

#### 5. Subscribe to product holdings

Aggregate balance of funds the host holds on the calling product's behalf, optionally narrowed to one scope. On the first call, the host MUST prompt the user for permission to disclose, mirroring `host_payment_balance_subscribe`.

```rust
fn host_payment_holdings_subscribe(
    scope: Option<ScopeTag>,
    callback: fn(PaymentHoldings)
) -> Result<Subscriber, PaymentHoldingsErr>

struct PaymentHoldings {
    /// Spendable now.
    available: Balance,
    /// Received but not yet final.
    pending: Balance,
    /// Provisionally reserved by an in-flight host_payment_holdings_spend.
    reserved: Balance,
    /// Advisory hint: this much of `available` is in funds approaching
    /// internal age limits or sitting in dust. Host will internally
    /// recycle regardless; this is a UI hint.
    needs_attention: Balance,
}

enum PaymentHoldingsErr {
    /// User denied the disclosure request.
    PermissionDenied,
    Unknown(GenericErr)
}
```

The host SHOULD coalesce frequent updates; suggested debounce is ~250 ms.

### Behavioural Requirements

1. **Rendezvous integrity.** The bytes returned by `host_payment_inbound_create` MUST be tamper-evident. A payer-side host that decodes them MUST be able to verify they were produced by a conforming host implementation and have not been modified.

2. **Payer-side expiry guard.** `host_payment_to_rendezvous_request` MUST validate the rendezvous expiry locally with a small tolerance before prompting the user, and return `RendezvousExpired` when the tolerance is exceeded.

3. **Late receipts.** When funds matching an inbound target arrive after `expires_at_ms`, the host MUST emit `LateReceived` rather than `Received`. The product decides any further policy.

4. **Parallel inbound targets.** A product MAY have arbitrarily many open inbound targets at once. The host MUST namespace receiving-key derivation disjointly across targets within a `(product, scope)` tuple and observe the chain for all of them concurrently.

5. **Holdings durability.** `PaymentHoldings` MUST reflect funds under host control across host restarts. Funds that have been spent onward via `host_payment_holdings_spend` MUST NOT be counted.

6. **Spend reservation.** While `host_payment_holdings_spend` is in flight, consumed funds MUST appear in `reserved`, not in `available`. On settlement they leave holdings entirely; on failure they revert to `available`.

7. **Attached delivery.** The host MUST attempt to transmit `attached` bytes from payer to receiver out-of-band of the on-chain transfer. Delivery is best-effort: if it fails, the inbound target completes with `attached: None`. The transport is host-implementation-defined.

8. **Subscription cancellation does not retract on-chain receipts.** Ending the active subscription on a target is a hint to the host that the product is no longer tracking it; the host MAY use this to free internal resources. Funds arriving at the target afterwards are still retained by the host and surface in `PaymentHoldings`. Products that need to actively reject funds must implement that policy themselves via `host_payment_holdings_spend`.

9. **Inbound target scoping.** An `InboundPaymentId` is scoped to the product that created it. A product MUST NOT be able to query or subscribe to another product's inbound targets.

10. **Payment authorization.** `host_payment_to_rendezvous_request` MUST trigger a user-facing confirmation prompt showing amount and any host-renderable identification of the destination. Hosts MUST NOT auto-approve.

11. **Holdings disclosure consent.** `host_payment_holdings_subscribe` consent semantics mirror `host_payment_balance_subscribe`. Granularity of consent (per-session, persistent) is left to host implementation.

### Asset Assumption

This proposal inherits RFC 0006's single fixed payment asset assumption. `Balance` is interpreted according to the same asset's decimals. Multi-asset support is deferred to a future revision, in which `host_payment_inbound_create` and `PaymentHoldings` would gain an asset identifier.

### Compatibility

This RFC is purely additive. Existing RFC 0006 methods are unchanged. `host_payment_request(amount, AccountId)` continues to mean an outbound payment to a regular destination address; `host_payment_to_rendezvous_request(amount, rendezvous, ...)` is the new product-to-product path.

## Drawbacks

1. **Rendezvous wire format.** `InboundRendezvous` becomes a host-to-host wire protocol. It needs a stable version field and a clear deprecation path so that an old payer host can recognize a new receiver's rendezvous. Adding a new payment system later requires either piggybacking onto the existing rendezvous (with version negotiation) or introducing a parallel API.

2. **Stateful host.** The host now performs ongoing bookkeeping for every product that receives funds (open targets, key derivation namespaces, observed deposits, in-flight spends, scope-keyed holdings). This is the cost of keeping product code small.

3. **`attached` as a side channel.** Hosts must pick *some* mechanism for delivering `attached` bytes (statement store, encrypted preimage, custom). This may consume host-owned allowance or surface ranking decisions; products that pay or receive in volume may want visibility into the cost. A future revision could expose the underlying mechanism explicitly.

4. **No partial-receipt visibility.** Products only see `Received` / `LateReceived` / `Expired`. A product that wants to render fine-grained progress while a payment is in flight has to compute it from holdings deltas, not target events.

### Ergonomics

The API is intentionally low-level and aligned with the rest of TrUAPI. Higher-level abstractions (idempotent target creation, intent-style state machines, refund flows, currency conversion, signed receipts) are expected to live in product or SDK layers above.

## Alternatives

### A single combined inbound + outbound surface

We could redefine RFC 0006's `host_payment_request` to take a richer destination type that covers both `AccountId` and `InboundRendezvous`. Rejected because the two destinations have meaningfully different semantics (offboard to a regular address vs. native product-to-product transfer) and overloading them complicates host implementation and product code. Keeping the lexical distinction makes intent explicit.

### Expose payment-system internals (denominations, key handles, ring memberships)

Lets products do their own splitting and routing. Rejected because it forces every product to learn the underlying private payment system and tracks its evolution. Wrong layer for TrUAPI.

### A higher-level "intent" or "session" surface

Bundle target creation, distribution, observation, receipt, and refund into a single host-managed object. Rejected because each product has different opinions about lifecycle, idempotency, status semantics, distribution channel, and metadata. Baking any one set of opinions into TrUAPI permanently couples it to that product. The primitives in this RFC support such a higher-level surface as a product or SDK library, without forcing the choice on every product.

### Host-driven rendezvous distribution

Have the host post the rendezvous to the statement store automatically. Rejected because distribution channel choice is product policy (some products want a QR, some want NFC, some want a deep link, some want a custom transport, some want all of them). The host should not silently consume statement-store allowance for distribution.

## Unresolved Questions

- **Rendezvous wire format and version negotiation.** The exact byte layout and the rules for cross-version interoperability between payer and receiver hosts.
- **Encryption of `attached`.** Whether the host should encrypt `attached` automatically using a deposit-bound key from the rendezvous, with an opt-out for plaintext memos.
- **Maximum simultaneous inbound targets per product.** A natural ceiling protects the host from runaway products. Suggested floor: 1024.
- **`needs_attention` semantics.** The exact threshold under which funds are flagged is left to host implementation. A future revision may standardize the hint.
- **Holdings disclosure granularity.** Whether scope-narrowed holdings disclosure carries the same consent weight as full disclosure.
- **Multi-asset support.** Tracked in RFC 0006; the same extension needs to apply here.

## Appendix A — Non-normative Host Implementation Notes for Coinage

This appendix is informational. It sketches the shape of the host-side bookkeeping required to make the methods in this RFC work over the current private payment system (Coinage). None of this surfaces to products and none of it constrains conforming hosts beyond what the normative sections require — it exists so that host implementors have a concrete starting point and so that reviewers can judge whether the proposed surface is in fact implementable.

The mechanics below assume familiarity with Coinage as specified in `paritytech/individuality::pallet-coinage`: fixed denominations `2^k × $0.01`, one coin per fresh Bandersnatch public key, per-coin age incremented by `transfer`/`split`, ring-based recycler with denomination-segregated rings, free vs paid unload tokens.

### A.1 Coin store

The host maintains a durable, locally-encrypted store. Suggested tables:

- `COINS` — one row per coin currently under host control: `(coin_pk, value_exponent, age, status, derivation_path, last_seen_block, owner_product, owner_scope, denomination_role)`. Status ranges over `pending_inbound | available | reserved | in_split | in_transfer_out | in_recycle_load | recycled_out | dust_destroyed | locked`.
- `INBOUND_TARGETS` — one row per outstanding `host_payment_inbound_create`: `(id, owner_product, owner_scope, nominal_amount, expires_at_ms, status, received_amount, attached_bytes, chain_anchor)`.
- `INBOUND_SLOTS` — one row per receiving key allocated under a target: `(id, slot, dest_pk, expected_value_exponent, state)`.
- `SPENDS` — one row per in-flight outbound: `(spend_id, owner_product, owner_scope, dest_rendezvous, amount, status, reserved_coins, finalized_block)`.
- `RECYCLER_RECORDS` — one row per coin currently inside a recycler ring on behalf of the host: `(coin_pk_pre, value_exponent, ring_index, member_pk, state, voucher_alias, fresh_coin_pk_post)`.
- `TOKEN_ALLOWANCE` — period-keyed counters for free-unload-token consumption (`people` / `lite-people`) and paid-token ring memberships.
- `ANCHORS` — most recent finalized block hash and number, used for restart reconciliation.

The store is encrypted at rest with a key derived from the user's seed. Field-level encryption MAY be applied to derivation paths.

### A.2 Receiving-key derivation

For each inbound target, the host derives fresh Bandersnatch keypairs under a scoped namespace such as:

```
seed → "coinage/recv" → productId → scope → target_id → slot
```

All hard derivation. Public keys are computed eagerly and stored in `INBOUND_SLOTS`; secret keys can be regenerated on demand from the path. The four-level namespace ensures keys never collide across products, scopes, or targets.

### A.3 Denomination planning (receive side)

Given a target `amount` in $0.01 minor units:

1. Greedy binary decomposition: pick the largest valid `2^k` denomination ≤ remaining amount, subtract, repeat. This always succeeds for amounts representable as sums of available denominations and within a per-target slot cap (suggested: 32 slots).
2. If the amount cannot be decomposed within the cap, return `AmountUnsupported`.
3. Allocate one slot per planned coin and write the rows.

Greedy decomposition minimizes coin count. A future variant could distribute denominations differently for additional payer-side privacy (Coinage Extension 3 touches on this), at the cost of more slots per receipt.

### A.4 Chain watching

A single light-client connection to the People chain underpins all observation. A "deposit watcher" subsystem maintains a watch set of every receiving public key in `INBOUND_SLOTS` and tracks two layers:

- **Liveness layer**: subscribe to `pallet-coinage` `Coin::Transferred` events; filter `to ∈ watch_set`; mark the matching slot as `funded(value, age, block)` immediately for product-visible "pending" balance.
- **Finality layer**: at each finalized block, read `CoinsByOwner` for slots seen funded; promote `funded` slots to `final` once their containing block is finalized.

Reorg handling falls out of the two-layer split: pre-final receipts only contribute to `PaymentHoldings.pending`, never to `available`, and never trigger `Received` events.

### A.5 Inbound target progression

Per target, the host transitions:

```
open
  → still open while sum(final slots) < nominal_amount and now ≤ expires_at_ms
  → Received       when sum(final slots) == nominal_amount and now ≤ expires_at_ms
  → LateReceived   when finalisation occurs after expires_at_ms (carry actual sum)
  → Expired        when expires_at_ms passes with no finalised slots
```

The host MAY tolerate small finalisation latency past `expires_at_ms` before declaring `Expired` (suggested: 30 s). Per the normative requirements, partial progress is not surfaced to the product.

### A.6 Holdings projection

`PaymentHoldings.available` is the sum of `COINS.value` where `status='available'` and `(owner_product, owner_scope)` matches the subscription. `pending` covers `status='pending_inbound'`. `reserved` covers `status='reserved'`. `needs_attention` is the host's heuristic estimate of funds approaching `MaximumAge` or sitting in dust (e.g. value < $0.04 across many keys); the exact policy is host-defined.

Emit on every commit that touches `COINS` for the relevant `(product, scope)`, debounced ~250 ms.

### A.7 Spending: coin selection and operation planning

When `host_payment_to_rendezvous_request` (user balance) or `host_payment_holdings_spend` (product scope) runs:

1. Decode the rendezvous → list of `(value_exponent_i, dest_pk_i)`.
2. From `COINS where status='available'` for the relevant ownership, plan a sequence of Coinage operations producing the required denominations at the destinations:
   - For each target denomination, prefer an exact-match coin.
   - Otherwise, plan a `split` of a larger coin into the needed sub-denominations.
   - If only smaller coins exist, plan a `recycle-and-consolidate` (load N coins into a recycler, unload one combined coin). This is slow (rings need to fill, ~10 minutes); return `InsufficientBalance` rather than blocking, unless the host has a pre-warmed combined coin available.
3. Prefer the **oldest acceptable coins** so that spending implicitly recycles. Exclude any coin at `MaximumAge` (must recycle before spending).
4. Reserve the chosen coins (`status='reserved'`); reflect in `PaymentHoldings.reserved`.
5. Construct the extrinsic batch (sequence of `split`, `transfer`).
6. Prompt the user as required by the API; sign each extrinsic; broadcast.
7. Watch for finality of each transfer. On success, drop the coins from `COINS`. On failure, revert `status` to `available`.
8. Emit `PaymentStatus::Completed` (RFC 0006) when all transfers finalise.

### A.8 Automatic recycling and consolidation

A background task runs periodically (every block or every few blocks). It owns the entire age/dust hygiene cycle without product involvement:

1. **Age scan**: enqueue any `available` coin with `age ≥ MaximumAge - 1`.
2. **Dust scan**: enqueue groups of small-value coins (e.g. ≥ 4 coins of `2^k`) for consolidation.
3. **Load**: per coin, generate a fresh Bandersnatch member key, sign and submit `load_recycler_with_coin(coin_pk, member_key)`, mark `RECYCLER_RECORDS.state='loaded'`.
4. **Wait for ring revision change** so that the recycler's anonymity set has grown since load. Suggested floor: wait until at least one recycler revision after load, or 10 minutes — whichever is shorter.
5. **Acquire an unload token**:
   - Free token first if the user has remaining allowance for the current period: produce a Ring VRF proof against the personhood ring with `context = "pop:polkadot.net/coinftk" || period || counter`, record the alias in `TOKEN_ALLOWANCE`.
   - Otherwise paid token: ensure the host has a member key in the current paid-token ring (joining if necessary; cost in DOT/stable/coin per host preference policy), then produce a ring proof against `context = "pop:polkadot.net/coinpaidtok" || period`.
6. **Unload**: derive a fresh `coin_pk_post`, submit `unload_recycler_into_coin(token, [voucher], value, ring_index, dest=coin_pk_post)`. Insert a fresh `COINS` row with `age=0`, drop the pre-key.

Consolidation uses the same flow but provides multiple vouchers to produce a single output of doubled value.

### A.9 Unload-token economics

- Free-token allowance per period is `min(allowance_in_asset / current_fee, MaxFreeUnloadTokensPerTimePeriod)`. Reconcile `TOKEN_ALLOWANCE.used` against `pallet-coinage::ConsumedFreeUnloadTokens` storage at period boundaries.
- Paid-token preference is a host-level user preference (e.g. "prefer free; fall back to paid in DOT; never use paid in coin"). Surface in host UI; products do not see it.
- The host SHOULD pre-warm paid tokens during low-activity periods so spend or recycle requests don't block on ring formation.

### A.10 Failure modes and reconciliation

**On boot:**

1. For each open `INBOUND_TARGETS` row, recompute state from chain: do any `INBOUND_SLOTS.dest_pk` already hold coins? Update accordingly.
2. For each in-flight `SPENDS` row, query inclusion of the planned extrinsics; either complete the spend or revert reservations.
3. For each `RECYCLER_RECORDS` row in `loaded` state, attempt the unload if the ring is now eligible and a token is available; otherwise leave for the next sweep.
4. Reconcile `COINS` against on-chain `CoinsByOwner` for all keys the host believes it owns. Drift should be logged and surfaced — it indicates either a bug or a security issue.

**During operation:**

- Network drop: pending observations queue locally; sweep replays once chain access is restored.
- Pre-finality reorg: `pending` adjusts; `available` is unaffected because it follows finality.
- User declines a spend prompt: revert reservation, return `Rejected`.
- Submission failure (insufficient fee, bad nonce, etc.): retry with backoff; never silently lose a coin.
- Coinage's per-coin lock periods (`2^retries × CoinFailureLockPeriod` after failed dispatch): mark `COINS.status='locked'` with a `lock_until` timestamp; exclude locked coins from selection until expiry.

### A.11 `attached` delivery transport

The transport is host-implementation-defined per the normative section. A workable default:

1. Just before broadcasting the on-chain transfer extrinsics, compute `delivery_topic = blake2_256("inbound-attached" || rendezvous_id)`.
2. Submit a statement to the statement store under `delivery_topic` whose payload is the `attached` bytes (ideally encrypted to a deposit-bound key carried in the rendezvous; see Unresolved Questions).
3. The receiver's host has a matching subscription on `delivery_topic` for every open inbound target. On receipt, store the bytes against the target and surface them in `InboundPaymentEvidence::attached`.

Statement-store allowance is consumed from the host's own consumer registration on the People chain (resources pallet), not the product's. If the side-channel post fails for any reason, the inbound target still completes — `attached` is best-effort by contract.
