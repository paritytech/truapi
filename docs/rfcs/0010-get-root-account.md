---
title: "Host API root account access"
owner: "@johnthecat"
---

# RFC 0010 — Host API root account access

## Summary

This RFC introduces `host_account_get_root`, a new Host API method that returns the user's root account. Because revealing this identifier allows a product to track the user across sessions and correlate their identity with other products, the host must obtain explicit user approval before returning the account. The call follows a just-in-time (JIT) approval model: the host shows a permission prompt on the first request and, if granted, may cache the decision for subsequent calls within the same session.

## Motivation

Products that need to establish a stable, user-owned identity — for example, to set up an encrypted communication channel, attribute content to a user, or display a personalised greeting — currently have no way to discover which account the user considers their primary identity. `host_account_get` exists but requires the product to already know the `ProductAccountId` (DotNS identifier + derivation index) it wants to look up. There is no way to ask "who is the user?".

`host_account_get_root` fills this gap: it returns the account the host considers the user's primary identity without requiring the product to supply an identifier first.

This is intentionally a privileged operation. Knowing the root account makes it possible for a product to:

- Identify the user deterministically across sessions and across products that have the same grant.
- Correlate the user's activity with their on-chain identity.

These properties are desirable for some products and undesirable for others. The permission prompt gives the user explicit control over which products receive this information.

## Detailed Design

### API changes

```rust
fn host_account_get_root() -> Result<Account, RequestCredentialsErr>;
```

The returned `Account` is the same struct used by `host_account_get`:

```rust
struct Account {
  public_key: PublicKey,
  name: Option<str>
}
```

`name` is the user's primary DotNS identifier when present. More sophisticated hosts may let the user select a different DotNS name they control; the simplest conforming implementation returns the most recently registered free username (lite or full-pop).

The method is added to the **Accounts** section of the host API protocol, alongside `host_account_get` and `host_account_get_alias`. It takes no request parameters.

### Permission lifecycle

1. **First call** — The host shows a permission prompt identifying the requesting product and explaining that the product will learn the user's primary account. The user may approve or deny.
2. **Approved** — The host returns the `Account` and may cache the grant for subsequent calls within the same session. Hosts MAY persist the grant across sessions; this is an implementation choice.
3. **Denied** — The host returns `RequestCredentialsErr::Rejected`. The product SHOULD treat this as a permanent signal for the current session and not re-prompt immediately.
4. **No root account** — If the user has no primary DotNS account, the host returns `RequestCredentialsErr::NotConnected` without prompting.

## Drawbacks

**Linkability.** Any product that receives approval can deterministically link the user's activity to their root account. This is a deliberate design trade-off — the permission prompt is the user's only control point. Hosts that want stronger privacy guarantees could choose to always deny or to prompt on every call, but the protocol does not require this.

**No revocation signal.** Like other permission-based methods in this API, there is no push notification to the product if the user later revokes the grant in host settings. Products should handle `RequestCredentialsErr::Rejected` at any point as a signal that access was withdrawn.

**`NotFound` edge case.** Users without a DotNS username receive `NotFound`, which may be confusing for products that assume every user has a root account. Products should handle this gracefully (e.g. by falling back to a product-scoped identity).

## Unresolved Questions

- **Grant persistence.** Should the host be required to persist the grant across sessions, or is session-scoped caching sufficient? Persistent grants reduce friction for returning users; session-scoped grants give users more frequent control.
- **Re-prompt policy.** If the user denied the request in a previous session and the host persisted that denial, should the product ever be able to trigger a new prompt? The protocol is silent on this; it is left to host implementations.
