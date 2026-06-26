---
title: "JIT Account-Access Permission for ProductAccountId Methods"
owner: "@filvecchiato"
---

# RFC 0012 — JIT Account-Access Permission for ProductAccountId Methods

## Summary

This RFC introduces a per-account just-in-time (JIT) permission check for all Host API methods that accept a `ProductAccountId`. Today, any product can call `host_account_get`, `host_account_get_alias`, `host_account_create_proof`, signing methods, `host_create_transaction`, `remote_statement_store_create_proof`, and `host_payment_top_up` (via the `ProductAccount` source variant) with an arbitrary `ProductAccountId` — including identifiers belonging to other products — without user awareness or consent. This RFC requires the host to obtain explicit user approval before granting access to a specific `ProductAccountId`, preventing cross-product identity leakage.

## Motivation

`ProductAccountId` is a `(DotNsIdentifier, DerivationIndex)` tuple. The `DotNsIdentifier` component is a product's registered DotNS name, and the `DerivationIndex` selects a specific derived account under that product's namespace. The host derives cryptographic keys, aliases, and proofs from this identifier — all of which are sensitive, user-specific material scoped to a product domain.

Currently, none of the methods that accept `ProductAccountId` enforce that the calling product is authorized to access the requested domain. A malicious product can:

1. Call `host_account_get(["legitimate-product.dot", 0])` to obtain the user's derived public key for another product's domain.
2. Call `host_account_get_alias` to learn the user's contextual alias in another product's ring VRF context.
3. Call `host_account_create_proof` to generate valid ring VRF proofs under another product's identity.
4. Call `host_sign_raw` or `host_sign_payload` with another product's account to produce signatures.
5. Call `host_create_transaction` to create signed transactions using another product's derived key.
6. Call `remote_statement_store_create_proof` to create statement proofs under another product's account.

This enables cross-product user tracking and identity correlation without the user's knowledge — the same class of privacy concern that motivated the JIT permission model for `host_account_get_root` (RFC-0010).

## Detailed Design

### Permission model

The host MUST maintain a per-product, per-`ProductAccountId` permission grant. When a product calls any method that includes a `ProductAccountId` (either as a direct parameter or embedded in a request struct), the host MUST check whether the calling product has been granted access to that specific `ProductAccountId`.

The permission key is the full `ProductAccountId` tuple `(DotNsIdentifier, DerivationIndex)`. A grant for `("example.dot", 0)` does NOT extend to `("example.dot", 1)`.

### Permission lifecycle

1. **First call** — When a product calls a `ProductAccountId`-bearing method for the first time with a given `ProductAccountId`, the host presents an approval dialog to the user. The dialog MUST identify the requesting product and the target `ProductAccountId` (at minimum the `DotNsIdentifier`).
2. **Approved** — The host caches the grant and proceeds with the method. Subsequent calls from the same product with the same `ProductAccountId` resolve immediately without prompting.
3. **Denied** — The method returns its domain-specific rejection error:
   - `host_account_get`, `host_account_get_alias`: `HostAccountGetError::Rejected`
   - `host_account_create_proof`: `HostAccountCreateProofError::Rejected`
   - `host_sign_raw`, `host_sign_payload`: `HostSignPayloadError::PermissionDenied`
   - `host_create_transaction`: `HostCreateTransactionError::PermissionDenied`
   - `remote_statement_store_create_proof`: `RemoteStatementStoreCreateProofError::Rejected`
   - `host_payment_top_up` (with `PaymentTopUpSource::ProductAccount`): `HostPaymentTopUpError::Rejected`
4. **Grant persistence** — The host SHOULD persist grants across sessions for the same product identity. Session-scoped grants are acceptable as a minimum conforming implementation.

### Same-domain optimization

When the calling product's own `DotNsIdentifier` matches the `DotNsIdentifier` in the `ProductAccountId`, the host MAY skip the permission prompt and grant access implicitly. This is the common case: a product accessing its own derived accounts. The host MUST still prompt when the `DotNsIdentifier` differs from the calling product's identity.

Hosts that cannot reliably determine the calling product's identity (e.g. during development or in permissive sandbox modes) MUST fall back to prompting for all `ProductAccountId` requests.

### Affected methods

| Method | `ProductAccountId` location | Rejection error |
|--------|---------------------------|-----------------|
| `host_account_get` | `HostAccountGetRequest.product_account_id` | `HostAccountGetError::Rejected` |
| `host_account_get_alias` | `HostAccountGetAliasRequest.product_account_id` | `HostAccountGetError::Rejected` |
| `host_account_create_proof` | `HostAccountCreateProofRequest.product_account_id` | `HostAccountCreateProofError::Rejected` |
| `host_sign_raw` | `HostSignRawRequest.account` | `HostSignPayloadError::PermissionDenied` |
| `host_sign_payload` | `HostSignPayloadRequest.account` | `HostSignPayloadError::PermissionDenied` |
| `host_create_transaction` | `ProductAccountTxPayload.signer` | `HostCreateTransactionError::PermissionDenied` |
| `remote_statement_store_create_proof` | `RemoteStatementStoreCreateProofRequest.product_account_id` | `RemoteStatementStoreCreateProofError::Rejected` |
| `host_payment_top_up` | `PaymentTopUpSource::ProductAccount` variant | `HostPaymentTopUpError::Rejected` |

### API changes

No new methods are introduced. Two error enums gain a `Rejected` variant to
cover the denial case:

- `RemoteStatementStoreCreateProofError::Rejected`
- `HostPaymentTopUpError::Rejected`

All other affected methods already have a suitable rejection variant
(`Rejected` or `PermissionDenied`).

`CallContext` gains `caller_product_id: Option<String>`, set by the host to
the calling product's DotNS identifier. Handlers use it for the same-domain
optimization (skip the prompt when the caller's domain matches the requested
`ProductAccountId`). Hosts that cannot determine the caller identity leave it
`None`, which forces the prompt for all requests.

### Interaction with existing permission systems

- **Remote permissions (RFC-0002)**: Account-access permission is orthogonal to remote permissions. A product that has `RemotePermission::ChainSubmit` still needs account-access permission before calling `host_create_transaction` with a cross-domain `ProductAccountId`.
- **Signing confirmation flow**: The per-operation signing confirmation (the dialog that shows "sign this payload?") remains unchanged. Account-access permission is checked first; if granted, the signing confirmation flow proceeds as before.
- **`host_account_get_root` (RFC-0010)**: Root account access has its own independent JIT permission. This RFC does not affect it.

### Implementation guidance for `host-container`

In the `host-container` package, the affected slots should be changed from `makeNotImplementedSlot` to a new pattern (e.g. `makeAccountGatedRequestSlot`) that:

1. Extracts the `ProductAccountId` from the incoming request payload.
2. Checks the permission cache.
3. If not cached, delegates to a host-provided permission callback to prompt the user.
4. On approval, caches the grant and calls the handler.
5. On denial, returns the appropriate error without calling the handler.

## Drawbacks

**Prompt fatigue.** Products that legitimately need cross-domain account access will trigger a permission prompt on each new `ProductAccountId`. For products that access many accounts across different domains, this could be disruptive. The same-domain optimization mitigates the common case.

**Per-derivation-index granularity may be too fine.** Requiring separate grants for `("example.dot", 0)` and `("example.dot", 1)` provides maximum privacy but could annoy users when a product uses multiple derivation indices under the same domain. An alternative would be to grant per-`DotNsIdentifier` (see Alternatives).

**No revocation signal.** Like other permission-based methods (RFC-0002, RFC-0010), there is no push notification to the product if the user later revokes the grant. Products should handle rejection errors at any point.

## Alternatives

### Per-DotNsIdentifier grants (instead of per-ProductAccountId)

A simpler model would grant access to all derivation indices under a `DotNsIdentifier` with a single prompt. This reduces prompt frequency but allows a product to enumerate all derivation indices under a domain once access is granted. This may be acceptable if the privacy concern is primarily about cross-domain leakage rather than intra-domain enumeration.

### Domain-scoping enforcement (no prompts)

Instead of a JIT prompt, the host could silently reject any request where the `DotNsIdentifier` does not match the calling product's registered identity. This is simpler but prevents legitimate cross-domain use cases (e.g. a product that aggregates accounts across multiple DotNS domains with user consent).

### Combine with `host_account_get_root` permission

If a product has already received `host_account_get_root` approval (RFC-0010), the host could implicitly grant access to the root account's `ProductAccountId` without an additional prompt. This RFC does not mandate this optimization but hosts MAY implement it.

## Unresolved Questions

1. **Grant granularity.** Should grants be per-`ProductAccountId` (as proposed) or per-`DotNsIdentifier`? The former is more private; the latter is more ergonomic. Feedback from host implementors is needed.

2. **`host_payment_top_up` with `PrivateKey` source.** The `PaymentTopUpSource` enum also has a `PrivateKey` variant that does not use `ProductAccountId`. Should payment top-ups from private keys have their own permission gate? This is out of scope for this RFC but worth considering.

3. **Batch consent.** Should there be a mechanism for a product to declare all the `ProductAccountId`s it intends to use upfront, so the user gets a single prompt? This would parallel `remote_permission`'s batching model.

4. **Re-prompt policy.** If the user denied access in a previous session, should the product be able to trigger a new prompt in a new session? The protocol is silent on this; it is left to host implementations.
