# TrUAPI v0.2: Changes from v0.1

This document describes all changes between Protocol v0.1 and v0.2, the reasoning behind each change, and links to the RFCs and issues that provide further context.

This document is based on the following meeting minutes and documents:
- [Host API v0.2: Feature Input Summary](https://docs.google.com/document/d/19WAfrjBAFeoz76c5mBxp-QUqNFbaPZhivC0qhvaXfXo/edit?tab=t.0#heading=h.xrf9ovdhv9qx)
- [TrUAPI V0.2 Features](https://docs.google.com/document/d/1rdNbH2rbNwWJfutGQVCCgFvtOsbDB0Xk3KVJ8uLvu7M/edit?tab=t.0)
- [TruAPI Working Group](https://docs.google.com/document/d/1_23zlst_kzYhb176rlzonFS1VDhS2AkbV8FWvyd-OoI/edit?tab=t.0#heading=h.f90u7hm8fbsv)



## 1. JIT Permission Model

RFC: [RFC 0001](https://github.com/paritytech/triangle-js-sdks/pull/66) | Issue: [#64](https://github.com/paritytech/triangle-js-sdks/issues/64)

### What changed

- `DevicePermissionRequest` (4 variants) is replaced by `DevicePermission` (9 variants).
- `RemotePermissionRequest` (single request) is replaced by `Vec<RemotePermission>` (batched requests).
- The `remote_permission` function signature changes accordingly.

### New `DevicePermission` variants

| Variant | Why |
|---------|-----|
| `Notifications` | Products need to send native push notifications to users. |
| `Nfc` | Tap-to-pay and NFC tag interactions at physical events (e.g. Web3 Summit). |
| `Clipboard` | Read/write access to the system clipboard for copy-paste workflows. |
| `OpenUrl` | Permission to open URLs in the system browser (external navigation out of the host). |
| `Biometrics` | Trigger biometric authentication (fingerprint, Face ID) for sensitive operations. |

### Batched `RemotePermission`

The v0.1 `RemotePermissionRequest` had two variants (`ExternalRequest`, `TransactionSubmit`). The v0.2 `RemotePermission` replaces this with a richer set that supports batching:

| Variant | Purpose |
|---------|---------|
| `Remote(Vec<String>)` | HTTP/HTTPS/WS/WSS access with domain-pattern matching (`"api.example.com"`, `"*.example.com"`, `"*"`). |
| `WebRtc` | WebRTC access, which can expose the user's IP address. |
| `ChainSubmit` | Permission to broadcast signed transactions via `remote_chain_transaction_broadcast`. |
| `StatementSubmit` | Permission to submit statements via `remote_statement_store_submit`. |

### Rationale

The v0.1 permission model required pre-authorisations in the product manifest. Gav's direction was to move to *just-in-time* (JIT) permission requests instead: the host prompts the user the first time a capability is needed, with options for "Allow always", "This time only", or "Never". Batching remote permissions into a single call lets the host present one consolidated prompt instead of several sequential dialogs.

Business methods like `remote_chain_transaction_broadcast` and `remote_statement_store_submit` implicitly trigger permission prompts if not yet resolved, so simple products work correctly without explicit permission preambles.


## 2. Payment API (Coinage)

RFC: [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94) | Issue: [#41](https://github.com/paritytech/triangle-js-sdks/issues/41)

### New group: Payment (4 methods)

| Method | Pattern | Purpose |
|--------|---------|---------|
| `host_payment_balance_subscribe` | Subscription | Subscribe to the user's payment balance. Requires user consent on first call. |
| `host_payment_top_up` | Request/Response | Top up the user's balance from a product-controlled source. No user consent needed (always in the user's favour). |
| `host_payment_request` | Request/Response | Request a payment from the user to a destination. Prompts user for authorization. Returns a `PaymentId` for tracking. |
| `host_payment_status_subscribe` | Subscription | Track the lifecycle of a payment (`Processing` then `Completed` or `Failed`). |

### New types

`Balance` (u128), `PaymentId`, `Ed25519PrivateKey`, `PaymentBalance`, `PaymentTopUpSource`, `PaymentReceipt`, `PaymentStatus`, `PaymentBalanceError`, `PaymentTopUpError`, `PaymentRequestError`, `PaymentStatusError`.

### Rationale

The primary use case driving the Payment API is Web3 Summit: products like T3rminal need to accept payments (e.g. buying a coffee with private funds). The underlying payment system (coinage) uses a UTXO-like model where transfers happen off-chain via private key handoff, and withdrawals require matured recycler vouchers. This makes settlement inherently asynchronous.

Rather than exposing coinage internals to products, RFC 0006 defines an *abstract payment interface* that hides the settlement mechanism. Products interact with balances and payment lifecycles; the host handles coins, recycling, and settlement. The API assumes a single fixed payment asset (e.g. pUSD); multi-asset support is deferred.

`PaymentTopUpSource::PrivateKey` enables a product to fund a user's balance from a one-time deposit account whose private key the product holds. This is a regular account holding public funds, not a coinage coin key.


## 3. Deterministic Entropy Derivation

RFC: [RFC 0007](https://github.com/paritytech/triangle-js-sdks/pull/95) | Issue: [polkadot-desktop#117](https://github.com/paritytech/polkadot-desktop/issues/117)

### New group: Entropy (1 method)

| Method | Pattern | Purpose |
|--------|---------|---------|
| `host_derive_entropy` | Request/Response | Derives 32 bytes of deterministic entropy scoped to the calling product and a caller-chosen key. |

### New types

`Entropy` ([u8; 32]), `DeriveEntropyError`.

### Derivation scheme

A three-layer BLAKE2b-256 keyed hashing scheme:

```
rootEntropySource    = blake2b256_keyed(rootAccountSecret, b"product-entropy-derivation")
perProductEntropy    = blake2b256_keyed(rootEntropySource, blake2b256(productId))
requestedEntropy     = blake2b256_keyed(perProductEntropy, key)
```

### Rationale

Products need a standardized way to derive deterministic cryptographic keys tied to a user's account. The primary motivating use case is X25519 key derivation for encrypted peer-to-peer communication (needed by HackM3 and Mark3t for encryption).

Without a host-provided derivation primitive, each product would implement its own key management with inconsistent security properties. The three-layer scheme ensures:

- *Determinism*: The same root account + product + key always yields the same 32 bytes on any conforming host.
- *Isolation*: Different products derive independent entropy from the same root account. One product cannot compute another product's values.
- *No runtime round trips*: After the initial SSO handshake shares the `rootEntropySource`, all derivation is a pure local computation.

The working group decided to keep the output at 32 bytes for v0.2, with higher-level abstractions (e.g. `deriveX25519KeyPair`) provided by SDK layers above.


## 4. `ProductAccountId` in Signing Methods

RFC: [RFC 0005](https://github.com/paritytech/triangle-js-sdks/pull/82) | Issue: [#40](https://github.com/paritytech/triangle-js-sdks/issues/40)

### What changed

In `SigningPayload` and `SigningRawPayload`, the `address: String` field is replaced by `account: ProductAccountId`.

### Rationale

The v0.1 signing methods (`host_sign_payload`, `host_sign_raw`) identified the signer via an `address` string, while every other account-related method in the API uses `ProductAccountId = (DotNsIdentifier, DerivationIndex)`. This inconsistency added complexity on the host side, since `Address -> ProductAccountId` is an irreversible mapping without additional caching/lookup, making implementations more error-prone. On the product side, developers had to manage two different identifier schemes for the same account.

The working group agreed (Mar 17 meeting) to ship all breaking changes together in v0.2 to minimize disruption.


## 5. Primary User ID

### New method

| Method | Pattern | Purpose |
|--------|---------|---------|
| `host_get_user_id` | Request/Response | Returns the user's primary DotNS account identifier. |

### New types

`UserIdentity` (struct with `dot_ns_identifier` and `public_key`), `UserIdentityError`.

### Rationale

Requested by Gav: products sometimes need to know *who* the user is (their primary DotNS identity), not just interact with derived accounts. This is a privilege, since it allows a product to identify the user across contexts, so it requires JIT user approval on first call. More sophisticated user agents may let users select a different name they control.

This was classified as "must have" in the Mar 24 working group meeting.


## 6. Simple Group Chat

### New method

| Method | Pattern | Purpose |
|--------|---------|---------|
| `host_chat_create_simple_group` | Request/Response | Creates a lightweight group chat room. Returns a join link for participants. |

### New types

`SimpleGroupChatRequest`, `SimpleGroupChatResult` (includes `join_link` and `status`).

### Rationale

The full Chat Extension v2 ([#54](https://github.com/paritytech/triangle-js-sdks/issues/54)) was deferred to v0.3 as too large for the Web3 Summit timeline. However, the primary W3S use case (users creating a group chat from within a SPA, being admin, and sharing a join link) can be achieved with a lightweight host call that covers 90-95% of the mockup without the full v2 complexity.

The host handles the group chat UI with default rendering (no custom elements). Custom chat features (welcome messages, per-message actions, custom footer) remain in scope for Chat v2 / v0.3.


## 7. Statement Store API Changes

### What changed

- `remote_statement_store_subscribe`: The `topics: Vec<Topic>` parameter is replaced by `filter: TopicFilter` which supports wildcard positions (`None` entries match any topic).
- `remote_statement_store_submit`: Now takes raw SCALE-encoded `Bytes` instead of a `SignedStatement` struct, and returns the statement hash (`String`) on success instead of `()`.

### New types

`TopicFilter` (struct with `topics: Vec<Option<Topic>>`).

### Rationale

The v0.1 statement store API had gaps and possibly bugs in its implementation. The v0.2 changes align with the `polkadot-sdk` statement store specification:

- `TopicFilter` mirrors the [`TopicFilter`](https://github.com/paritytech/polkadot-sdk/blob/89aa25d825603d0f34764ff02ae3ab6b8d8826c9/substrate/primitives/statement-store/src/store_api.rs#L45) type from `polkadot-sdk`, enabling richer topic matching with wildcard positions.
- Switching `statement_store_submit` to raw bytes gives products more control over encoding and aligns with the SSS node's RPC interface.

William noted in the working group that all SSS APIs must be available since use cases for Web3 Summit cannot be predicted upfront.


## 8. Preimage: `remote_preimage_submit` Removed

### What changed

The `remote_preimage_submit` method is removed from v0.2. The `remote_preimage_lookup_subscribe` method remains unchanged.

### Rationale

Basti proposed removing `remote_preimage_submit` because products should have more fine-grained control over where data is stored. Bulletin chain storage should go through regular transactions (via the PAPI provider and chain interaction methods), not through a dedicated preimage submission function. The preimage lookup function is kept as a generic retrieval mechanism where the host decides how to resolve a hash to bytes (could be bulletin chain, IPFS, smoldot, etc.).


## Summary of All Changes

### New methods (7)

| Method | Group | RFC |
|--------|-------|-----|
| `host_get_user_id` | Account Management | |
| `host_chat_create_simple_group` | Chat | |
| `host_payment_balance_subscribe` | Payment (new) | [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94) |
| `host_payment_top_up` | Payment (new) | [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94) |
| `host_payment_request` | Payment (new) | [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94) |
| `host_payment_status_subscribe` | Payment (new) | [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94) |
| `host_derive_entropy` | Entropy (new) | [RFC 0007](https://github.com/paritytech/triangle-js-sdks/pull/95) |

### Changed methods (6)

| Method | What changed | RFC |
|--------|-------------|-----|
| `host_device_permission` | Request type: `DevicePermissionRequest` (4 variants) to `DevicePermission` (9 variants) | [RFC 0001](https://github.com/paritytech/triangle-js-sdks/pull/66) |
| `remote_permission` | Request type: single `RemotePermissionRequest` to batched `Vec<RemotePermission>` | [RFC 0001](https://github.com/paritytech/triangle-js-sdks/pull/66) |
| `host_sign_payload` | `SigningPayload.address` replaced by `SigningPayload.account: ProductAccountId` | [RFC 0005](https://github.com/paritytech/triangle-js-sdks/pull/82) |
| `host_sign_raw` | `SigningRawPayload.address` replaced by `SigningRawPayload.account: ProductAccountId` | [RFC 0005](https://github.com/paritytech/triangle-js-sdks/pull/82) |
| `remote_statement_store_subscribe` | Parameter: `Vec<Topic>` to `TopicFilter` (supports wildcards) | |
| `remote_statement_store_submit` | Parameter: `SignedStatement` to `Bytes`; return: `()` to `String` (hash) | |

### Removed methods (1)

| Method | Reason |
|--------|--------|
| `remote_preimage_submit` | Products should use chain transactions for storage; preimage lookup remains for retrieval. |

### New groups (2)

| Group | Methods |
|-------|---------|
| Payment | `host_payment_balance_subscribe`, `host_payment_top_up`, `host_payment_request`, `host_payment_status_subscribe` |
| Entropy | `host_derive_entropy` |

### Features deferred to v0.3+

| Feature | RFC/Issue | Reason |
|---------|-----------|--------|
| Chat Extension v2 (full) | [#54](https://github.com/paritytech/triangle-js-sdks/issues/54) | Too large for W3S timeline; simple group chat covers the immediate need. |
| RingLocation redesign | [#56](https://github.com/paritytech/triangle-js-sdks/issues/56) | Not needed for W3S; no products currently require ring proofs. |
| Contacts API | | Deferred post-W3S per William. (Note from William - this was referring to the privacy preserving friends list; however it seems there is a different 'Contacts API' functionality that should be there |
| Honour API | | Not due for W3S; implementation still in progress. |
| HOP API | | Limited solution; bulletin chain is quicker for W3S use cases. |
| Legacy account support | | Pending decision from Gav. |
