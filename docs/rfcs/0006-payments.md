---
title: "Payment Host API"
owner: "Valentin Sergeev"
---

# RFC 0006 — Payment Host API

## Summary

This RFC proposes a set of host API calls that allow products to perform payment-related operations through an abstract interface. Products can query a user's balance, top up the user's balance from product-controlled funds, request payments to arbitrary destinations, and track payment status asynchronously. The interface intentionally hides the underlying payment medium from products.

## Motivation

Products in the Polkadot ecosystem need a way to handle payments -- charging users for services, verifying sufficient funds, and funding user accounts. Today, there is no standardized host API surface for these operations.

The underlying payment system (coinage) introduces constraints that make synchronous payment completion impractical. Coinage uses a UTXO-like model where coin transfers happen off-chain via private key handoff, and withdrawals require "matured" recycler vouchers before funds can move to external accounts. This means payment settlement is inherently asynchronous.

Rather than exposing these implementation details to products, this RFC defines a minimal, abstract payment API that:

- Gives products **balance visibility** to drive UI and validation.
- Provides a **top-up mechanism** for products to fund user balances.
- Offers **asynchronous payment requests** with status tracking, accommodating the non-instant nature of the underlying system.

### Stakeholders

- **Product developers** -- consumers of the host API who wish to integrate payment features into their applications.
- **Host implementors** -- responsible for implementing the host-side logic including user consent flows, coin management, and settlement.
- **End users** -- whose privacy must be preserved while enabling product interactions with their balance.

## Detailed Design

### Design Principles

1. **Abstraction over implementation** -- Products interact with balances and payments without knowledge of coinage, coins, recycling, or any other settlement detail.
2. **Asynchronous settlement** -- Payment requests return immediately with an identifier; actual settlement is tracked via subscription.
3. **User consent where appropriate** -- Balance disclosure requires explicit user approval. Top-ups, being in the user's favour, do not.

### API Calls

#### 1. Balance Subscription

Subscribe to the user's payment balance. The host must explicitly ask the user whether they want to grant the product access to their balance before emitting data.

```rust
fn host_payment_balance_subscribe(
    callback: fn(PaymentBalance)
) -> Result<Subscriber, PaymentBalanceErr>

struct PaymentBalance {
    /// Balance that can be spent right now
    available: Balance
}

enum PaymentBalanceErr {
    /// User denied the balance disclosure request
    PermissionDenied,
    Unknown(GenericErr)
}
```

The host emits updated `PaymentBalance` values whenever the user's balance changes. The product can unsubscribe at any time via the returned `Subscriber`.

#### 2. Top Up

Top up the user's payment balance from a product-controlled funding source. This operation is always in the user's favour and does not require user consent.

```rust
fn host_payment_top_up(
    amount: Balance,
    source: PaymentTopUpSource
) -> Result<(), PaymentTopUpErr>

enum PaymentTopUpSource {
    /// Fund from one of the calling product's scoped accounts
    ProductAccount(DerivationIndex),
    /// Fund from a one-time account represented by its private key.
    /// This is a standard account holding public funds -- not a coin key.
    PrivateKey(Ed25519PrivateKey)
}

enum PaymentTopUpErr {
    /// The source account does not hold sufficient funds
    InsufficientFunds,
    /// The source account was not found or is invalid
    InvalidSource,
    Unknown(GenericErr)
}
```

`PaymentTopUpSource::PrivateKey` refers to a regular account (e.g. holding DOT or pUSD) whose private key the product possesses -- for instance, a one-time deposit account. This is not a coinage coin key.

#### 3. Request Payment

Request a payment from the user's available balance to a destination account. The host should prompt the user to authorize the payment. Returns a `PaymentId` for tracking.

```rust
fn host_payment_request(
    amount: Balance,
    destination: AccountId
) -> Result<PaymentReceipt, PaymentRequestErr>

type PaymentId = str;

struct PaymentReceipt {
    id: PaymentId
}

enum PaymentRequestErr {
    /// User denied the payment request
    Denied,
    /// User's available balance is not sufficient for the requested amount
    InsufficientBalance,
    Unknown(GenericErr)
}
```

A successful response means the user has authorized the payment and the host has accepted it for processing. It does **not** mean the payment has settled -- use `host_payment_status_subscribe` to track completion.

#### 4. Payment Status Subscription

Subscribe to status updates for a previously requested payment. The subscription emits status changes until the payment reaches a terminal state (`Completed` or `Failed`).

```rust
fn host_payment_status_subscribe(
    payment_id: PaymentId,
    callback: fn(PaymentStatus)
) -> Result<Subscriber, PaymentStatusErr>

enum PaymentStatus {
    /// Payment is being processed
    Processing,
    /// Payment has been settled successfully
    Completed,
    /// Payment has failed
    Failed(str)
}

enum PaymentStatusErr {
    /// PaymentId was not found or does not belong to the current product
    PaymentNotFound,
    Unknown(GenericErr)
}
```

### Behavioral Requirements

1. **Balance subscription consent**: On the first `host_payment_balance_subscribe` call, the host must prompt the user for permission. If denied, the host returns `PermissionDenied`. The consent granularity (per-session, per-product, persistent) is left to the host implementation.

2. **Payment authorization**: Each `host_payment_request` call must trigger a user-facing confirmation prompt showing the amount and destination. The host must not auto-approve payments.

3. **Payment ID scoping**: A `PaymentId` is scoped to the product that created it. A product cannot query or subscribe to payment status for another product's payments.

4. **Terminal status delivery**: Once a payment reaches `Completed` or `Failed`, the host must deliver that status to any active subscriber and may then close the subscription. The host should make a best effort to deliver terminal status even across session restarts.

5. **Top-up idempotency**: If the same top-up is submitted multiple times (e.g. due to a retry), the host should ensure funds are only transferred once where possible. However, this is a best-effort guarantee -- products should implement their own idempotency checks for critical flows.

### Asset Assumption

This proposal assumes a single, fixed payment asset (e.g. pUSD) known to both the host and the product. `Balance` values are interpreted according to that asset's decimals. Support for multiple assets is deferred to a future version.

## Drawbacks

1. **Asynchronous complexity** -- Products must handle payment lifecycle states rather than getting a synchronous success/failure. This adds implementation burden compared to a simple "pay and done" model.

2. **No direct smart-contract payments** -- The abstraction does not support paying directly into a smart contract. Products needing this must receive funds to a product-controlled account first, then interact with the contract separately.

3. **Single asset limitation** -- Only one payment asset is supported. Products dealing with multiple currencies or cross-chain payments will need additional host API extensions in the future.

4. **Top-up trust model** -- `PaymentTopUpSource::PrivateKey` requires the product to possess a private key for the funding account. The host must validate that the account actually holds sufficient funds before proceeding.

### Ergonomics

The API is intentionally low-level and aligned with the rest of the Host API. Higher-level abstractions (e.g. "pay-and-wait" helpers, balance formatting utilities) are expected to be provided by the Product SDK.

## Alternatives

### Compatibility

This proposal adds new host API calls without modifying existing ones. It introduces four new payload variant groups in the protocol. Existing products and hosts are unaffected.

### Prior Art and References

- [Coinage Design Document](https://docs.google.com/document/d/124mp6mnMhKFgSjmL6Y1NDzD0v-hRAhmryjqWEJ7yFDk/edit?usp=sharing)
- [Host API PRD](https://docs.google.com/document/d/1AxKjF15y7gmdl-a6twc5wd8R5xcxKxMO8Ahp2l20v0g) -- defines the overall protocol architecture, naming conventions, and transport layer
- Previous RFC "Coinage Host API for Private Payments" (2026-03-13) -- superseded by this RFC. The previous version coupled payment transfers to the chat infrastructure and exposed coinage internals (coin private keys, amount hints). This RFC replaces that approach with an abstract payment interface.

## Unresolved Questions

- **Multi-asset support** -- Extend `Balance` to include an asset identifier for payments in different currencies.
- **Batch payments** -- Support for multiple payments in a single request.
- **Smart-contract integration** -- Direct payment to contract addresses without intermediate steps.
