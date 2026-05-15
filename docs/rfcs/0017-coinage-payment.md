---
title: "Coinage Payment User Agent API"
owner: "@replghost"
---

# RFC 0017 - Coinage Payment User Agent API

## Summary

This RFC proposes a small User Agent API for using Coinage inside products without
exposing Coinage secrets to those products. Products can create firewalled
Coinage purses, inspect purse metadata and balance status when authorized,
transfer balances between local purses, create receivables, construct and
deposit cheques, and present standardized invoices to user agents.

This RFC defines both the product-facing API shape and the semantics expected
from a Coinage-backed implementation. A host that exposes this API must satisfy
the CoinPayment contract rather than advertising partial or simulated settlement
as RFC 0017 support.

The product never receives Coinage private keys, derivation paths, source coin
IDs, coin secrets, voucher secrets, recycler internals, statement-store
internals, raw ring-VRF proofs, or raw private payment evidence. The user agent
owns Coinage key material, coin inventory, cheque decryption, proof
construction, transfer submission, finality tracking, durable purse storage,
and recovery.

This RFC is deliberately not merchant-specific. Merchant checkout, terminal
attribution, POS references, receipts, refund policy, end-of-day accounting,
settlement policy, and operator permissions belong in a product SDK layer or a
future merchant-specific RFC above this RFC.

This draft is based on the finalized
[Sample Coinage API](https://hackmd.io/@polkadot/coinage-api), adapting its
Purse, Receivable, Cheque, Invoice, deposit, refund, and clearing-status model
to the TrUAPI RFC format and existing `host_*` method naming convention.

## Motivation

Coinage today is primarily a user-agent balance. Products also need scoped
Coinage balances and payment flows, but product access must remain firewalled:
products should be able to ask the user agent to receive, deposit, refund, and
move Coinage value without ever learning the coin secrets that control that
value.

The goal is to define a minimal Coinage substrate:

- firewalled purses under ultimate user-agent control;
- local purse-to-purse balance movement;
- a `CoinPaymentReceivable` public key that identifies a payment and protects transmitted
  coin secrets;
- a standard `CoinPaymentCheque` structure for encrypted coin-secret transmission;
- a standard `CoinPaymentInvoice` structure that regular user agents can understand from a
  QR code or deep link;
- clearing status and evidence references without exposing raw private payment
  data.

Earlier drafts described the product boundary as a "namespace". This draft
uses "purse" instead. A purse is the unit that owns and receives coins.
Products that need merchant namespaces, store hierarchies, terminal groups, or
accounting ledgers can model those concepts above one or more purses.

## Basic Concepts

### Purse

A purse is a separate, firewalled Coinage depository opaquely managed by the
user agent and ultimately owned by the user.

There is always a single main user-owned purse, identified by `MAIN_PURSE`.
This is the purse that the user agent itself uses and presents as the user's
ordinary Coinage balance.

Other purses are identified by numeric `PurseId` values within a user agent.
Non-main identifiers are randomly assigned by the user agent. A purse retains
metadata including:

- creation product ID;
- creation timestamp;
- internal human-readable name supplied by the creating product;
- balance and funding status.

The user can get an overview of all purses and transfer balances between them.
Products may query purses by ID to determine metadata and funding/balance
status, but will generally be blocked without user consent. The product which
created a purse has default permission to query that purse.

Similarly, a product may propose an instruction for spending or transferring
purse funds. This will generally be blocked without user consent, except that
the creating product has default permission for its own purse. User consent for
the transfer itself may still be required. As with other PKI-controlled
resources, the user agent decides when explicit user consent is required.

Like Coinage itself, purses are generally controlled by a single user agent,
though other user agents controlled by the same user may have read access to
the relevant keys.

### Coin

A coin is an NFT representing a particular denomination of dotUSD. Coins are
always associated with a public-key owner.

### Coin Secret

A coin secret is the secret key which allows transfer of ownership, or
"spending", of a coin.

Control of coins is transmitted asynchronously by transmitting coin secrets. A
payment concatenates, encrypts, and asynchronously transmits several coin
secrets in a standardized structure called a `CoinPaymentCheque`.

### Receivable

A receivable is a short serializable token used to identify a payment and
protect its constituent coin secrets. It is a public key created by the user
agent's Coinage subsystem and passed to a product when that product needs to
request payment into a purse.

Without the receivable, payers cannot encrypt coin secrets to the intended
receiving party. The controller of the remote transmission endpoint would
otherwise be able to claim the coins, which may not be the intended recipient
when the product sets up the channel.

Receivables are not inherently associated with an amount. In principle, any
amount may be deposited into one. An invoice is the higher-level concept that
combines a receivable with an amount and a data channel.

This RFC uses `CoinPaymentReceivable` for the concept called `ReceiveKey` in the Sample
Coinage API. Both names refer to the same 32-byte public-key token; products
should treat it as opaque.

### Receiving Party

For a receivable, the receiving party is the user agent which derived it and
knows the corresponding secret. It is therefore the party that can decrypt
content encrypted to the receivable public key.

### Channel

A channel is a means of passing coin secrets between devices and therefore
making a payment. This RFC includes one initial standardized channel shape, but
future RFCs may define additional channels.

### Cheque

A cheque is a standardized data structure containing coin secrets encrypted
under a particular receivable public key. Knowledge of this data by the
corresponding receiving party allows that party to claim ownership of the
coins.

### Invoice

An invoice is a simple datagram combining a data-channel identifier, a
receivable, and a payment amount.

Invoices are expected to be placed in deep links or QR codes and scanned by a
payer's device. The payer's user agent can then describe the payment request,
seek user confirmation, create a cheque, and transmit it without product-level
interaction on the payer side.

The invoice URI is host-profiled rather than globally tied to one branded app
scheme:

```text
<ua-invoice-scheme>://coinpayment/invoice?payload=<base64url-no-padding(CoinPaymentInvoiceV0)>
```

The payload is the canonical SCALE encoding of the V0 `CoinPaymentInvoice`
using the TrUAPI schema defined by this RFC. A host profile chooses the
concrete URI scheme that it registers or understands. For the Polkadot app host
profile, the URI is:

```text
polkadotapp://coinpayment/invoice?payload=<base64url-no-padding(CoinPaymentInvoiceV0)>
```

Native user agents can register their concrete scheme with the operating
system. Web or PWA user agents can parse the same URI from their in-app QR
scanner without requiring OS-level scheme registration. Hosts may accept
additional aliases for compatibility, but products should emit the concrete
scheme named by the target host profile.

## Detailed Design

### API Calls

```rust
fn host_coin_payment_create_purse(
  name: String
) -> Result<PurseId, CoinPaymentError>;

fn host_coin_payment_query_purse(
  purse: PurseId
) -> Result<CoinPaymentPurseInfo, CoinPaymentError>;

fn host_coin_payment_rebalance_purse(
  from: PurseId,
  to: PurseId,
  amount: Balance
) -> Result<Resolvable<CoinPaymentStatus>, CoinPaymentError>;

fn host_coin_payment_delete_purse(
  target: PurseId,
  drain_into: PurseId
) -> Result<Resolvable<CoinPaymentStatus>, CoinPaymentError>;

fn host_coin_payment_create_receivable(
  into: PurseId
) -> Result<CoinPaymentReceivable, CoinPaymentError>;

fn host_coin_payment_create_cheque(
  from: PurseId,
  to: CoinPaymentReceivable,
  amount: Balance
) -> Result<CoinPaymentCheque, CoinPaymentError>;

fn host_coin_payment_deposit(
  cheque: CoinPaymentCheque
) -> Result<Resolvable<CoinPaymentStatus>, CoinPaymentError>;

fn host_coin_payment_refund(
  receivable: CoinPaymentReceivable
) -> Result<Resolvable<CoinPaymentStatus>, CoinPaymentError>;

fn host_coin_payment_listen_for(
  receivable: CoinPaymentReceivable
) -> Result<Subscription<CoinPaymentListenForItem>, CoinPaymentError>;
// CoinPaymentListenForItem =
//   Channel(CoinPaymentTransmissionChannel) | Cheque(CoinPaymentCheque)
// The subscription emits Channel first, then Cheque when one arrives.
```

### Core Types

```rust
type PurseId = u32;
const MAIN_PURSE: PurseId = u32::MAX;

type Balance = u32;
type Timestamp = u64;
type CoinPaymentProductId = String;
type Resolvable<T> = Stream<T>;
type Subscription<T> = Stream<T>;
type CoinPaymentReceivable = [u8; 32]; // public key
type CoinPaymentMerkleRoot = [u8; 32];
type CoinPaymentTransactionHash = [u8; 32];
type CoinPaymentCoinagePubKey = [u8; 32];

struct CoinPaymentPurseInfo {
  name: String,
  created: Timestamp,
  creator: CoinPaymentProductId,
  balance: Balance
}
```

`Resolvable<T>` is RFC shorthand for a long-running operation that emits
ordered updates. In TrUAPI this is represented as a subscription/stream whose
items are `T`, not as a single `async T` value. A stream must deliver updates in
order and must emit at most one terminal item for operations whose status type
has terminal variants such as `Done` or `Failed`.

`MAIN_PURSE` is the ordinary user-owned Coinage purse. Products should not
assume access to it. Product-created purses are separate, firewalled balances
whose identifiers are assigned by the user agent.

`Balance` is an integer count of the RFC17 V1 CoinPayment denomination. V1
denominates balances in dotUSD cents, with exponent `2`; for example,
`1250` means `12.50 dotUSD`. `Balance` is not a chain-native planck amount or
an arbitrary payment-asset unit. Product layers that need fiat quotes, EUR sale
amounts, display currency, or payment-asset negotiation model those concepts
above this RFC.

```rust
struct CoinPaymentCheque {
  version: u8, // 0
  id: CoinPaymentReceivable,
  amount: Balance,
  encrypted_secrets: Vec<u8>
}
```

The `id` is the receivable public key that protects the cheque. The
`encrypted_secrets` field contains concatenated coin secrets encrypted so that
only the receiving party for that receivable can decrypt them. Products must
treat `encrypted_secrets` as opaque bytes. The implementation must encrypt
actual Coinage coin secrets sufficient to claim ownership of the selected coins.

```rust
enum CoinPaymentError {
  BalanceLow,
  Denied,
  BadCoins,
  SnipedCoins,
  PurseNotFound,
  ReceivableNotFound,
  UnsupportedChannel,
  UserAgentCapabilityUnavailable,
  Internal
}
```

`BalanceLow` means the source purse has too little balance. `Denied` means the
user agent denied the spend, transfer, or access request. `BadCoins` means the
coin secrets do not control valid coins. `SnipedCoins` means the coin secrets
were claimed elsewhere.

```rust
struct CoinPaymentClearingReference {
  root: CoinPaymentMerkleRoot,
  leaves: Vec<(CoinPaymentCoinagePubKey, CoinPaymentTransactionHash)>
}

enum CoinPaymentStatus {
  /// More coins have cleared.
  Clearing {
    clearing: Balance,
    cleared: Balance
  },
  /// Some or all coins failed to transfer.
  Failed {
    error: CoinPaymentError,
    cleared: Balance,
    reference: CoinPaymentClearingReference
  },
  /// All coins cleared.
  Done {
    cleared: Balance,
    reference: CoinPaymentClearingReference
  }
}
```

`Clearing` can be emitted multiple times because deposits and refunds may claim
coins over several blocks. `Done` is the terminal successful state and returns
the cleared amount plus a clearing reference that products may store for
reconciliation or receipts. `Failed` can still include a non-zero `cleared`
amount and clearing reference when some coins cleared before the failure.

`CoinPaymentClearingReference` is host-owned product-visible clearing evidence.
It is suitable for reconciliation, audit correlation, receipts, and refund
linkage, but it is not itself a merchant receipt, payer identity, source coin
inventory, raw private proof material, or a stable public transaction API. The
`root` identifies the clearing batch or root known to the host. `leaves`
identify affected Coinage clearing items only to the extent the host may safely
reveal them to the caller without exposing payer identity, source coin IDs, coin
secrets, voucher secrets, derivation paths, or raw private evidence. Exact
cryptographic proof semantics are Coinage implementation details unless a
future RFC standardizes public clearing proofs.

```rust
enum CoinPaymentTransmissionChannel {
  Standard {
    sss_topic: [u8; 32]
  },
  // Future RFCs may define additional channel variants.
}

struct CoinPaymentInvoice {
  version: u8, // 0
  handoff: CoinPaymentTransmissionChannel,
  receiver: CoinPaymentReceivable,
  amount: Balance
}
```

`CoinPaymentTransmissionChannel::Standard` is the V1 handoff descriptor for the
default host-managed cheque transport. It carries an explicit topic so a payer
user agent can transmit the encrypted cheque to the receiver user agent after
scanning or opening an invoice. Current hosts may implement that handoff with
statement store and fall back to HOP when a cheque is too large. Products must
treat the channel as an opaque delivery descriptor and must not infer payer
identity, purse identity, POS references, or settlement state from it.

### Call Semantics

#### `host_coin_payment_create_purse`

Creates a new firewalled Coinage purse.

The user agent must:

- assign a random non-main `PurseId`;
- record the creating product ID from authenticated product context;
- record the creation timestamp and product-supplied name;
- keep the purse under user-agent control;
- grant the creating product default permission to query and propose operations
  against this purse.

The product must not receive Coinage keys, derivation paths, source coin IDs, or
raw private payment material.

#### `host_coin_payment_query_purse`

Returns purse metadata and funding/balance status when authorized.

The user agent may require user consent unless the calling product created the
purse or has otherwise been granted access. The returned information must not
expose source coins, coin secrets, derivation paths, recycler internals, or raw
proofs.

#### `host_coin_payment_rebalance_purse`

Transfers balance between local purses.

The user agent must authorize access to both purses and decide whether explicit
user consent is required. Products use this for local aggregation, for example
moving funds from a terminal purse into a store purse.

#### `host_coin_payment_delete_purse`

Deletes a purse after draining its balance into another local purse.

The user agent must authorize the operation, preserve any durable audit state it
needs for recovery or user overview, and must not delete `MAIN_PURSE`.

#### `host_coin_payment_create_receivable`

Creates a receivable public key for depositing into a purse.

The user agent must:

- authorize receive access to the target purse;
- derive or create a fresh receivable and retain the corresponding secret;
- bind the receivable to the target purse;
- return only the public `CoinPaymentReceivable` token to the product.

The product can place this receivable into an invoice. The product cannot use it
to decrypt cheques or claim coins.

#### `host_coin_payment_create_cheque`

Creates a cheque paying from a local purse to a receivable.

The user agent must authorize access to the source purse, decide whether user
confirmation is required, select suitable coins, encrypt their secrets to the
receivable public key, and return the resulting cheque.

#### `host_coin_payment_listen_for`

Creates or selects a transmission channel for a receivable and returns a
subscription that emits channel and cheque items. Products use this when they
need an invoice that can receive a cheque asynchronously.

The subscription emits a `Channel` item first, suitable for inclusion in an
invoice. It then emits a `Cheque` item when a cheque for the receivable arrives
through that channel. Receipt of a cheque is not payment finality; the receiver
still needs to call `deposit`.

#### `host_coin_payment_deposit`

Attempts to claim coins from a cheque into the purse associated with the
cheque's receivable.

The user agent must decrypt the cheque, validate that the coin secrets control
valid coins, claim ownership, and emit clearing progress. It returns `Done` only
after the deposit has completed.

`Done` means the host has verified the Coinage claim evidence represented by the
returned `CoinPaymentClearingReference`.

#### `host_coin_payment_refund`

Attempts to return coins associated with a receivable back to the sender when
possible.

Refunds are best-effort. They may fail when the receivable has no usable return
path, when coins have already been claimed elsewhere, or when the user agent no
longer has enough information to return the coins safely. Product-specific
refund policy decides how to handle failure. Refund progress uses the same
`CoinPaymentStatus` stream as `deposit`: `Clearing` reports in-flight return progress,
`Done { cleared, reference }` reports a completed refund, and
`Failed { error, cleared, reference }` reports a failed or partially failed
refund.

Refund execution must be authorized by the user agent and must return status and
clearing references with the same honesty requirements as deposits. The
implementation may reverse the original receivable flow, create a new Coinage
payment, or use another host-native Coinage refund primitive, but the product
must not select source coins or construct raw spends.

### Example Usage

Non-normative example for a product issuing an invoice:

```rust
let my_purse: PurseId = match read_storage("purse") {
  Some(p) => p,
  None => {
    let p = host_coin_payment_create_purse("Product purse")?;
    write_storage("purse", p);
    p
  }
};

let amount = 1000; // Ten dollars to invoice.
let old_balance = host_coin_payment_query_purse(my_purse)?.balance;

let receiver = host_coin_payment_create_receivable(my_purse)?;
let mut listener = host_coin_payment_listen_for(receiver)?;

let handoff = listener.next().await; // Channel item.
let invoice = CoinPaymentInvoice { version: 0, handoff, receiver, amount };
display_as_qr_or_link(invoice);

let cheque = listener.next().await; // Cheque item.
let deposit_status = host_coin_payment_deposit(cheque)?;

let payment_reference = loop {
  match deposit_status.await_changed() {
    CoinPaymentStatus::Failed { error, cleared, reference } => {
      if cleared > 0 {
        host_coin_payment_refund(receiver)?;
      }
      return Err(error);
    }
    CoinPaymentStatus::Clearing { clearing, cleared } => {
      update_progress(clearing, cleared);
    }
    CoinPaymentStatus::Done { cleared, reference } => {
      assert_eq!(cleared, amount);
      break reference;
    }
  }
};

let new_balance = host_coin_payment_query_purse(my_purse)?.balance;
assert_eq!(new_balance, old_balance + amount);

if refund_is_needed {
  let refund_status = host_coin_payment_refund(receiver)?;
  let refund_reference = loop {
    match refund_status.await_changed() {
      CoinPaymentStatus::Done { reference, .. } |
      CoinPaymentStatus::Failed { reference, .. } => break reference,
      CoinPaymentStatus::Clearing { .. } => {}
    }
  };
  record_receipt(refund_reference);
}
```

### Privacy Requirements

- Coin secrets are never exposed to products except as encrypted cheque data
  intended for a receivable.
- Source coin IDs are never exposed to products.
- Derivation paths are never exposed to products.
- Products receive clearing state and clearing references, not raw proofs.
- Channels and invoice identifiers must avoid derivation from public POS
  references or other correlatable product identifiers unless the product
  intentionally chooses that trade-off.
- Clearing references must avoid payer identity unless a future permission
  explicitly allows it.

### Permissions

Add a Coinage payment user-agent permission:

```rust
enum UserAgentPermission {
  // existing variants...
  CoinPayment
}
```

The user agent denies CoinPayment access by default. A product can use this API
only after approval for the product, purse, and action, except that the product
which created a purse has default permission to query and propose operations
against that purse. Transfer execution may still require explicit user consent.

This permission does not grant raw signing, raw Coinage access, arbitrary
statement-store submission, root-account access, chat, merchant checkout
semantics, settlement destination management, or product-visible access to other
products' purses.

### Behavioral Requirements

1. Purses are user-agent-owned Coinage depositories, not public accounts.
2. `MAIN_PURSE` always exists and represents the ordinary user Coinage balance.
3. Products must not construct raw Coinage transfers directly.
4. Products must not receive private keys, source coin IDs, derivation paths,
   coin secrets, voucher secrets, or raw proofs.
5. A receivable is a public key used to protect cheque contents.
6. A cheque must encrypt coin secrets to the receivable.
7. An invoice must combine a transmission channel, receivable, and amount.
8. CoinPaymentCheque receipt is not payment finality; the user agent must deposit and
   clear the cheque before a product treats payment as complete.
9. `Clearing` may emit multiple times because claims may finalize over several
   blocks.
10. `Done { cleared, reference }` is the terminal successful state.
11. Deposit/refund failures must distinguish low balance, denied operations,
    bad coins, and sniped coins.
12. Clearing references may be exposed; raw private evidence must not be
    exposed.
13. Refunding a receivable must not require the product to select source coins
    or construct a spend.
14. Purses survive product reload and user-agent restart.
15. Hosts must back balances, cheques, deposits, refunds, and
    clearing references with real Coinage inventory and finality evidence.
16. Purses, receivables, pending cheques, deposit/refund operations, and
    clearing references must be durable across user-agent restart. Hosts must
    have a recovery path from user-agent-controlled secret
    material.
17. RFC 0006 payment purse selectors must obey the same authorization, consent,
    durability, and privacy requirements as RFC 0017 purse operations.

### Relationship to RFC 0006 Payment

RFC 0006 defines the generic payment surface for balance display, top-up,
outbound payment requests, and payment status. RFC 0017 does not replace that
surface. Instead, it extends the relevant RFC 0006 request types with optional
CoinPayment purse selectors so generic payment operations can address either
the ordinary user-owned purse or an authorized product-created purse.

This keeps CoinPayment focused on Gav's Sample Coinage API primitives:
purses, receivables, cheques, invoices, deposits, refunds, and local purse
movement. It also avoids defining duplicate CoinPayment-specific balance,
top-up, outbound-payment, and status APIs.

The RFC 0006 request types become purse-aware as follows:

```rust
struct HostPaymentBalanceSubscribeRequest {
  purse: Option<CoinPaymentPurseId>
}

struct HostPaymentTopUpRequest {
  into: Option<CoinPaymentPurseId>,
  amount: Balance,
  source: PaymentTopUpSource
}

struct HostPaymentRequest {
  from: Option<CoinPaymentPurseId>,
  amount: Balance,
  destination: [u8; 32]
}
```

`None` selects `MAIN_PURSE`. `Some(purse)` selects the corresponding RFC 0017
CoinPayment purse when the calling product is authorized to access that purse.
Hosts must apply the same authorization and consent policy used by
`query_purse`, `rebalance_purse`, `create_cheque`, and other purse operations:
the creating product may query and propose operations against its own purse by
default, while access to other purses or execution of sensitive operations may
require explicit user consent.

`balance_subscribe({ purse: Some(purse) })` is the live subscription form of
`query_purse(purse).balance`. It reports spendable balance for UI and
validation without exposing coin inventory, source coin IDs, derivation paths,
recycler state, voucher state, or raw proofs. `query_purse` remains the
metadata snapshot API; RFC 0006 remains the generic live balance/status API.

`top_up({ into: Some(purse), ... })` funds the selected purse from the
product-controlled source described by RFC 0006. The user agent converts that
top-up into Coinage inventory controlled by the target purse. Products do not
choose coin keys, denominations, recycler vouchers, unload tokens, or
settlement transactions.

`request({ from: Some(purse), ... })` spends from the selected purse to the RFC
0006 destination account. This gives product layers a generic way to move funds
out of a CoinPayment purse after their own settlement or offload policy decides
to do so. The request is asynchronous and is tracked through RFC 0006
`status_subscribe`. A successful request response means the host accepted the
operation for processing; final completion is reported through payment status.

For all RFC 0006 purse-aware operations, the host remains responsible for
denomination selection, Ring VRF proof construction, free or paid unload-token
use, recycler/voucher cycles, chain submission, retry, and finality tracking.
Those mechanics are host-private. Products receive only RFC 0006 balances,
payment receipts and payment status, plus RFC 0017 purse metadata and clearing
references where those are explicitly returned.

### Relationship to Product Payment Layers

Merchant checkout, POS, ecommerce, and reconciliation flows can be implemented
as product-specific layers on top of this API.

Mapping:

- merchant/store/terminal operating unit -> one or more CoinPayment purses;
- checkout QR/deep link -> product-created `CoinPaymentInvoice`;
- customer payment -> payer user agent creates and transmits a `CoinPaymentCheque`;
- merchant acceptance -> receiver user agent deposits the cheque and observes
  `CoinPaymentStatus`;
- paid receipt -> merchant artifact over sale metadata and `CoinPaymentClearingReference`;
- refund -> merchant policy plus `refund(receivable)`;
- end-of-day accounting -> product ledger aggregation across separate purses.

Those layers should get their own RFC if they standardize POS references,
merchant receipts, fiscal disclaimers, quote semantics, refund linkage, user
experience, operator permissions, and merchant-specific reconciliation.

### Settlement / Offload

This RFC does not define merchant treasury policy. Products that need merchant
settlement or offload policy should define scheduling, thresholds, destinations,
retained refund balances, authorization, and reconciliation above this layer.

For V1, a product can keep terminal/store purses separate and aggregate activity
by reading its own merchant ledger and rebalancing purses locally. When product
policy decides that funds should leave a purse, it uses the RFC 0006
purse-aware `request({ from: Some(purse), ... })` path described above. That
operation is implemented by the user agent using Coinage unload/recycler
mechanics as needed, but those mechanics are not exposed as product API.

## Appendix A - Coinage Key Derivation

Production purse keys are created like ordinary Coinage keys, except that the
purse index is inserted immediately after the initial `//coinage` path
component. For ordinary Coinage keys:

```text
//coinage//<PAGE><DERIV_SEC>/<ITEM>
```

For purse-scoped keys:

```text
//coinage//<PURSE>//<PAGE><DERIV_SEC>/<ITEM>
```

For key index `5` on page index `4` in purse index `3`, the derived path is:

```text
//coinage//3//4<DERIV_SEC>/5
```

This allows all Coinage keys to be discovered with knowledge of the root secret
while keeping product-visible purse IDs and product state free of derivation
paths.

Future RFCs may define an alternative production recovery scheme, but V1
production implementations use this purse-scoped derivation shape. Loss of
product storage must not make Coinage funds unrecoverable when the
user-agent-controlled root secret is available.

At a high level, a user agent can implement this RFC over Coinage by
maintaining a durable, locally encrypted coin store with rows for owned coins,
purses, receivables, in-flight cheques, deposits, refunds, recycler records,
allowance counters, and recent finalized chain anchors. A purse maps to a
user-agent-private Coinage ownership boundary. The purse is not an off-chain IOU
ledger: its balances are projections over actual Coinage coins whose secret
material is controlled by the user agent.

The user agent remains responsible for age and dust hygiene. It may recycle or
consolidate coins in the background using Coinage recycler operations, free or
paid unload tokens, and user preferences. These mechanics must not leak coin
identities, derivation paths, recycler secrets, raw proofs, or statement-store
internals to products.

## Drawbacks

**Less complete for products.** This RFC intentionally omits merchant records,
operator policy, settlement, receipts, and full channel APIs. Product layers
must define those semantics.

**Channel convention remains small.** The standard invoice/channel shape is
only enough for V1. Additional channel work may need future RFCs.

**Clearing references remain unsettled.** Coinage payments may span many
transactions and blocks, so the exact clearing reference format may need
iteration.

## Alternatives

### Use RFC 0006 Directly

Rejected for products that need firewalled Coinage purses, receivables,
cheques, invoices, and product-controlled receive flows.

### Product-Owned Coinage

Rejected. Products must not receive raw Coinage keys, source coins, derivation
paths, voucher secrets, or proof material.

### Keep the Namespace/Reserve/Record API

Rejected for V1. That surface mixes Coinage ownership with merchant accounting,
operator permissions, reserves, settlement, and product audit logs. Those
concepts are useful, but they belong in product layers or future RFCs once the
Coinage substrate is stable.

## Unresolved Questions

- Whether `CoinPaymentTransmissionChannel::Standard` should derive its topic from the
  receivable rather than carrying an explicit topic.
- Exact clearing reference shape for multi-transaction Coinage payments.
- Whether additional channel APIs should be standardized separately from this
  RFC.
- How purse metadata and balances sync across multiple user agents/devices.
- How purse recovery/backup is surfaced to users.
