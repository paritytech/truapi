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
| `PreimageSubmit` | Permission to submit preimages via `remote_preimage_submit`. |

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

`UserIdentity` (struct with `primary_username` and `public_key`), `UserIdentityError`.

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

- `remote_statement_store_subscribe`: The `topics: Vec<Topic>` parameter is replaced by `filter: TopicFilter`, an enum with `MatchAll(Vec<Topic>)` (AND) and `MatchAny(Vec<Topic>)` (OR) variants.
- `remote_statement_store_submit`: Now takes raw SCALE-encoded `Bytes` instead of a `SignedStatement` struct, and returns the statement hash (`String`) on success instead of `()`.

### New types

`TopicFilter` (enum with `MatchAll(Vec<Topic>)` and `MatchAny(Vec<Topic>)` variants).

### Rationale

The v0.1 statement store API had gaps and possibly bugs in its implementation. The v0.2 changes align with the `polkadot-sdk` statement store specification:

- `TopicFilter` mirrors the [`TopicFilter`](https://github.com/paritytech/polkadot-sdk/blob/89aa25d825603d0f34764ff02ae3ab6b8d8826c9/substrate/primitives/statement-store/src/store_api.rs#L45) type from `polkadot-sdk`, enabling MatchAll (AND) and MatchAny (OR) topic matching.
- Switching `statement_store_submit` to raw bytes gives products more control over encoding and aligns with the SSS node's RPC interface.

William noted in the working group that all SSS APIs must be available since use cases for Web3 Summit cannot be predicted upfront.


## 8. Preimage: `remote_preimage_submit` Removed

### What changed

The `remote_preimage_submit` method is removed from v0.2. The `remote_preimage_lookup_subscribe` method remains unchanged.

### Rationale

Basti proposed removing `remote_preimage_submit` because products should have more fine-grained control over where data is stored. Bulletin chain storage should go through regular transactions (via the PAPI provider and chain interaction methods), not through a dedicated preimage submission function. The preimage lookup function is kept as a generic retrieval mechanism where the host decides how to resolve a hash to bytes (could be bulletin chain, IPFS, smoldot, etc.).


## 9. Account Type Split

### What changed

- `Account` is replaced by two types: `ProductAccount` (no `name` field) and `LegacyAccount` (with optional `name`).
- `host_account_get` now returns `ProductAccount` instead of `Account`.
- `host_get_non_product_accounts` is renamed to `host_get_legacy_accounts` and returns `Vec<LegacyAccount>`.

### Rationale

Product-derived accounts are protocol-generated and have no user-chosen label, while legacy (imported) accounts may carry a display name. Splitting the types makes this distinction explicit.


## 10. Login Flow and Legacy Account Signing

### New methods

| Method | Pattern | Purpose |
|--------|---------|---------|
| `host_request_login` | Request/Response | Initiate a login flow; returns `LoginResult` (Success, AlreadyConnected, Rejected). |
| `host_sign_raw_with_legacy_account` | Request/Response | Sign raw data using a legacy account. |
| `host_sign_payload_with_legacy_account` | Request/Response | Sign a transaction payload using a legacy account. |


## 11. Private Chat Host API

RFC: [RFC 0013](docs/rfcs/0013-private-chat-host-api.md)

### New group: Private Chat (10 methods)

| Method | Pattern | Purpose |
|--------|---------|---------|
| `host_private_chat_identity_get` | Request/Response | Return the local peer-facing private chat identity. |
| `host_private_chat_username_resolve` | Request/Response | Resolve a username to an account ID. |
| `host_private_chat_peer_resolve` | Request/Response | Resolve a peer and validate that the host can attempt to route chat. |
| `host_private_chat_request_send` | Request/Response | Send a first-contact request. |
| `host_private_chat_accept_send` | Request/Response | Accept an incoming first-contact request. |
| `host_private_chat_message_send` | Request/Response | Send a text message to an active conversation. |
| `host_private_chat_conversation_state_get` | Request/Response | Return host-local conversation state for a peer. |
| `host_private_chat_message_subscribe` | Subscription | Subscribe to inbound private chat messages. |
| `host_private_chat_delivery_status_subscribe` | Subscription | Subscribe to outbound request/message delivery status. |
| `host_private_chat_request_subscribe` | Subscription | Subscribe to incoming first-contact requests. |

### New types

`PrivateChatConversationState`, `PrivateChatIdentity`, `PrivateChatPeer`,
`PrivateChatRequestReceipt`, `PrivateChatMessageReceipt`,
`PrivateChatContentType`, `PrivateChatMessageEvent`,
`PrivateChatDeliveryStatus`, `PrivateChatDeliveryStatusEvent`,
`PrivateChatRequestEvent`, `PrivateChatErr`.

### Rationale

Private wallet-to-wallet chat should keep key material, identity lookup,
encryption, signing, statement-store submission, and event delivery inside the
host. Product SPAs such as chat.dot get a narrow UI-oriented API for resolving
peers, sending requests/messages, and receiving events without direct access to
private keys or submit-capable statement payloads.

### Renamed methods

| Old name | New name |
|----------|----------|
| `host_create_transaction_with_non_product_account` | `host_create_transaction_with_legacy_account` |

### New types

`LoginResult`, `LoginError`, `SigningPayloadPayload`, `SigningRawPayloadWithoutAccount`, `SigningPayloadWithoutAccount`.


## 11. Theme Subscription

### New group: Theme (1 method)

| Method | Pattern | Purpose |
|--------|---------|---------|
| `host_theme_subscribe` | Subscription | Subscribes to the host's visual theme (Light/Dark). |

### New types

`Theme` (enum: Light, Dark).


## 12. SigningRawPayload Field Rename

### What changed

`SigningRawPayload.data` is renamed to `SigningRawPayload.payload`.


## 13. PaymentBalance Simplification

### What changed

`PaymentBalance.pending` field is removed. The type now only carries `available: Balance`.


## 14. UserIdentity Field Alignment

### What changed

- `UserIdentity.dot_ns_identifier` is renamed to `UserIdentity.primary_username`.
- `UserIdentityError::Rejected` is renamed to `UserIdentityError::PermissionDenied`.
- `PaymentRequestError::Denied` is renamed to `PaymentRequestError::Rejected`.


## Summary of All Changes

### New methods (11)

| Method | Group | RFC |
|--------|-------|-----|
| `host_get_user_id` | Account Management | |
| `host_request_login` | Account Management | |
| `host_sign_raw_with_legacy_account` | Signing | |
| `host_sign_payload_with_legacy_account` | Signing | |
| `host_chat_create_simple_group` | Chat | |
| `host_payment_balance_subscribe` | Payment (new) | [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94) |
| `host_payment_top_up` | Payment (new) | [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94) |
| `host_payment_request` | Payment (new) | [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94) |
| `host_payment_status_subscribe` | Payment (new) | [RFC 0006](https://github.com/paritytech/triangle-js-sdks/pull/94) |
| `host_derive_entropy` | Entropy (new) | [RFC 0007](https://github.com/paritytech/triangle-js-sdks/pull/95) |
| `host_theme_subscribe` | Theme (new) | |

### Changed methods (8)

| Method | What changed | RFC |
|--------|-------------|-----|
| `host_device_permission` | Request type: `DevicePermissionRequest` (4 variants) to `DevicePermission` (9 variants) | [RFC 0001](https://github.com/paritytech/triangle-js-sdks/pull/66) |
| `remote_permission` | Request type: single `RemotePermissionRequest` to batched `Vec<RemotePermission>` (5 variants incl. `PreimageSubmit`) | [RFC 0001](https://github.com/paritytech/triangle-js-sdks/pull/66) |
| `host_sign_payload` | `SigningPayload.address` replaced by `SigningPayload.account: ProductAccountId` | [RFC 0005](https://github.com/paritytech/triangle-js-sdks/pull/82) |
| `host_sign_raw` | `SigningRawPayload.address` replaced by `SigningRawPayload.account: ProductAccountId` | [RFC 0005](https://github.com/paritytech/triangle-js-sdks/pull/82) |
| `remote_statement_store_subscribe` | Parameter: `Vec<Topic>` to `TopicFilter` (MatchAll/MatchAny enum) | |
| `remote_statement_store_submit` | Parameter: `SignedStatement` to `Bytes`; return: `()` to `String` (hash) | |
| `host_sign_raw` | `SigningRawPayload.address` replaced by `SigningRawPayload.account: ProductAccountId`; `data` field renamed to `payload` | [RFC 0005](https://github.com/paritytech/triangle-js-sdks/pull/82) |
| `host_account_get` | Return type: `Account` to `ProductAccount` (no `name` field) | |
| `remote_statement_store_subscribe` | Parameter: `Vec<Topic>` to `TopicFilter` (MatchAll/MatchAny enum); callback: `Vec<SignedStatement>` to `SignedStatementsPage` | |
| `remote_statement_store_submit` | Parameter: `Bytes` to `SignedStatement`; return: `String` to `()` | |
| `host_get_user_id` | `UserIdentity.dot_ns_identifier` → `primary_username`; error variant `Rejected` → `PermissionDenied` | |

### Renamed methods (2)

| Old name | New name |
|----------|----------|
| `host_get_non_product_accounts` | `host_get_legacy_accounts` |
| `host_create_transaction_with_non_product_account` | `host_create_transaction_with_legacy_account` |

### Removed methods (1)

| Method | Reason |
|--------|--------|
| `remote_preimage_submit` | Products should use chain transactions for storage; preimage lookup remains for retrieval. |

### New groups (3)

| Group | Methods |
|-------|---------|
| Payment | `host_payment_balance_subscribe`, `host_payment_top_up`, `host_payment_request`, `host_payment_status_subscribe` |
| Entropy | `host_derive_entropy` |
| Theme | `host_theme_subscribe` |

### Features deferred to v0.3+

| Feature | RFC/Issue | Reason |
|---------|-----------|--------|
| Chat Extension v2 (full) | [#54](https://github.com/paritytech/triangle-js-sdks/issues/54) | Too large for W3S timeline; simple group chat covers the immediate need. |
| RingLocation redesign | [#56](https://github.com/paritytech/triangle-js-sdks/issues/56) | Not needed for W3S; no products currently require ring proofs. |
| Contacts API | | Deferred post-W3S per William. (Note from William - this was referring to the privacy preserving friends list; however it seems there is a different 'Contacts API' functionality that should be there |
| Honour API | | Not due for W3S; implementation still in progress. |
| HOP API | | Limited solution; bulletin chain is quicker for W3S use cases. |
