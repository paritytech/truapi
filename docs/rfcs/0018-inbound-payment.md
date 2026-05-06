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
    /// Cancelled by the product.
    Cancelled,
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

#### 3. Cancel a pending inbound target

Stops watching an inbound target. The subscription emits `Cancelled` and closes. Funds that arrive after cancellation are still retained by the host and will surface in `host_payment_holdings_subscribe` aggregate balances; they no longer trigger per-target events.

```rust
fn host_payment_inbound_cancel(
    id: InboundPaymentId
) -> Result<(), InboundPaymentActionErr>

enum InboundPaymentActionErr {
    NotFound,
    /// Target is already in a terminal state.
    AlreadyClosed,
    Unknown(GenericErr)
}
```

#### 4. Pay a rendezvous

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

#### 5. Spend product-held funds

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

#### 6. Subscribe to product holdings

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

8. **Cancellation does not retract on-chain receipts.** Funds arriving after `Cancelled` remain under host control and surface only in `PaymentHoldings`. Products that need to actively reject funds must implement that policy themselves via `host_payment_holdings_spend`.

9. **Inbound target scoping.** An `InboundPaymentId` is scoped to the product that created it. A product MUST NOT be able to query, subscribe to, or cancel another product's inbound targets.

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
