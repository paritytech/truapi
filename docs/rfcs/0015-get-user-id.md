# RFC-0015: Get User Primary DotNS Name

|                 |                                                                                       |
| --------------- | ------------------------------------------------------------------------------------- |
| **Start Date**  | 2026-04-27                                                                            |
| **Description** | Host API call returning the user's primary DotNS username, plus account-type cleanup  |
| **Authors**     | Valentin Sergeev                                                                      |

## Summary

A new host call, `host_get_user_id`, returns the user's primary DotNS username scoped to the calling product. The existing `Account` type is split into `ProductAccount` (no name) and `LegacyAccount` (with name) so that the presence of a `name` field always means "user-chosen label."

Supersedes RFC-0010, which was merged without review.

## Motivation

Products need a way to refer to the user by a human-readable name. Today the only username-shaped field is `Account.name`, returned by both `host_account_get` (product accounts) and `host_get_legacy_accounts` (imported accounts). This is ambiguous: product accounts are protocol-derived and have no user-chosen label, so whatever a host puts in `Account.name` for them is host-defined. Legacy accounts, on the other hand, *do* carry a meaningful user-chosen label and need to keep one.

RFC-0010 tried to solve the username need by returning a full root account `{ public_key, name }`. That leaks more than the original requirement ("return the primary username") and re-couples username retrieval to account retrieval. This RFC realigns to the original requirement.

## Stakeholders

Product developers (consumers), host / Account Holder implementors (own the user-to-username mapping and consent UX), end users (control disclosure).

## Explanation

### `host_get_user_id`

```rust
fn host_get_user_id() -> Result<GetUserIdResponse, GetUserIdErr>

struct GetUserIdResponse {
    /// The user's primary DotNS username scoped to the calling product.
    primary_username: DotNsIdentifier
}

enum GetUserIdErr {
    /// User denied the disclosure request.
    PermissionDenied,
    /// User is not logged in.
    NotConnected,
    Unknown(GenericErr)
}
```

`DotNsIdentifier` is the existing API type — no new identifier shape.

Behavior:

- **Connection precedence.** No connected account → `NotConnected` without prompting. `NotConnected` strictly precedes `PermissionDenied`.
- **Consent.** If connected and not previously granted, the host prompts using the existing permission model (one-time vs persistent). On denial → `PermissionDenied`.
- **Source-agnostic and host-chosen.** The host picks what counts as primary for this product (lite username, full username, custom — products MUST NOT assume). When the user is connected, the host is guaranteed to be able to pick one.
- **Per-product scope.** Whether two products see the same identifier is a host implementation choice. Simple hosts will return the same to all; sophisticated hosts MAY let users pick distinct primaries per product.
- **Per-call freshness, no revocation.** Each call reflects current host state; if the user changes their primary, subsequent calls return the new value. Once disclosed, a value cannot be retracted from the product.
- **Sync semantics.** The signature is synchronous in the language-agnostic protocol; concrete bindings may expose it as `Promise`/`Future`/etc.

### Account type split

```rust
/// Protocol-derived, product-scoped. No user-chosen label.
pub struct ProductAccount {
    pub public_key: PublicKey,
}

/// User-imported into the Account Holder. May carry a user-chosen label.
pub struct LegacyAccount {
    pub public_key: PublicKey,
    pub name: Option<String>,
}

fn host_account_get(
    product_account_id: ProductAccountId
) -> Result<ProductAccount, RequestCredentialsError>

fn host_get_legacy_accounts() -> Result<Vec<LegacyAccount>, RequestCredentialsError>
```

The rename `host_get_non_product_accounts` → `host_get_legacy_accounts` already shipped in v0.6→v0.7; only the return type changes here.

After this RFC, the three identity concerns separate: `host_get_user_id` for "who is the user", `host_account_get` for product-scoped signing key, `host_get_legacy_accounts` for user-imported accounts (with their labels).

## Drawbacks

- **Privacy surface.** A primary username is identifying and persistent. Once disclosed, a product may cache it indefinitely; the consent prompt is the only protocol-level mitigation. Products needing stronger guarantees should use contextual aliases instead.
- **"No primary" eliminated by fiat.** If a user is connected, the host MUST be able to pick a primary; otherwise it must treat the state as `NotConnected`. This pushes complexity into the connection-status model in exchange for a smaller error surface.

## Compatibility

Breaking change:

1. `host_account_get` no longer returns `name`. Products reading it today must migrate to `host_get_user_id` (or stop reading it).
2. `host_get_legacy_accounts` element type changes from `Account` to `LegacyAccount` — mechanical rename in typed bindings.

**Alternative considered (rejected): keep `Account`, return `name: None` from `host_account_get`.** Wire-compatible, but preserves the exact semantic confusion this RFC removes — `Account.name` remains reachable from product-account paths and a future host could re-populate it with host-defined values. Splitting the types makes the misuse unrepresentable.

## Prior Art

- **RFC-0010** (Host API root account access) — superseded. Returned `{ public_key, name }`, coupling username retrieval to account retrieval.
- **v0.6 → v0.7 migration** — established the "legacy account" vocabulary.
- **`host_account_get_alias`** — privacy-preserving alternative when a user-readable handle is not needed.

## Future Directions

- **`host_user_id_subscribe`** — push updates when the user changes their primary username, avoiding re-polling.
