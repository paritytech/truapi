# RFC-0014: Contacts API

|                 |                                                                 |
| --------------- | --------------------------------------------------------------- |
| **Start Date**  | 2026-04-17                                                      |
| **Description** | Expose the user's contact list to products via Host API         |
| **Authors**     | Filippo Vecchiato                                               |

## Summary

Products can read the user's host-managed address book. Each contact pairs local metadata with a context-scoped map keyed by `ProductAccountId` (`DotNsIdentifier` + `DerivationIndex`) — the same namespace used for Ring VRF alias derivation. By default a product only sees entries for its own context; cross-context access is a separate privilege.

## Motivation

Products need to resolve human-readable identities to accounts. The host manages an address book but does not expose it. Without this API, users must paste raw keys or scan QR codes for every interaction.

Exposing the contact list:

1. **Removes friction** — products show names instead of raw addresses.
2. **Enables cross-product identity** — multiple products resolve the same contact within their respective contexts.
3. **Preserves user control** — the host gates access and filters responses to the requesting product's scope.
4. **Supports contextual accounts** — a contact has different aliases and accounts per DotNS context, preserving unlinkability.

## Detailed Design

### Data Model

```rust
type ContactContext = ProductAccountId; // (DotNsIdentifier, DerivationIndex)

struct ContextContactInfo {
  alias: Option<Vec<u8>>,
  account_id: Option<AccountId>
}

struct LocalContactInfo {
  display_name: Option<str>
}

struct Contact {
  local: LocalContactInfo,
  entries: Map<ContactContext, ContextContactInfo>
}
```

`ContactContext` is a `ProductAccountId` (`DotNsIdentifier` + `DerivationIndex`) — the same tuple used for Ring VRF alias derivation. The host derives the `[u8; 32]` Ring VRF context by hashing this identifier internally.

`ContextContactInfo` fields are optional; either or both may be present.

### Access Tiers

#### Tier 1: Own-context (default)

The host filters `entries` to only the requesting product's `ProductAccountId`. `LocalContactInfo` is always included. This is safe because the product could already derive this information through its own alias system.

#### Tier 2: Cross-context (privileged)

Returns the full `entries` map. Required for host-privileged products that aggregate identities across contexts (e.g. Browse, profile, honour). The host MAY grant implicit tier 2 access to built-in host products that need it for their core function (e.g. a contact management UI).

### API

```rust
enum ContactsErr {
  NotConnected,
  Rejected,
  Unknown(GenericErr)
}

fn host_contacts_get() -> Result<Vec<Contact>, ContactsErr>;

fn host_contacts_subscribe(
  callback: fn(Vec<Contact>)
) -> Result<Subscriber, ContactsErr>;
```

Both require authentication (RFC-0009). The host prompts for permission before returning. `host_contacts_subscribe` delivers the full filtered list on each callback; hosts MAY debounce.

This API returns only contacts the user has explicitly saved in their address book. It is not a global name resolution service — resolving arbitrary accounts to DotNS names is a separate concern (on-chain DotNS lookup).

### Permission Model

Extends `DevicePermission` from RFC-0002 with two new variants:

```rust
enum DevicePermission {
  // ... existing variants ...
  Contacts,
  ContactsCrossContext
}
```

| Permission | Tier | Grants |
|-----------|------|--------|
| `Contacts` | 1 | Own-context entries + local info |
| `ContactsCrossContext` | 2 | Full entries across all contexts |

The tier 2 prompt SHOULD warn that the product can correlate contacts across contexts. `ContactsCrossContext` implies `Contacts`.

### Example

```
Product ("voting.dot", 0) calls host_contacts_get():

→ Host checks DevicePermission::Contacts grant
→ Host filters each contact's entries to key ("voting.dot", 0)
→ Returns:
  [
    Contact {
      local: { display_name: "Alice" },
      entries: { ("voting.dot", 0): { alias: 0xab.., account_id: 0x12.. } }
    },
    Contact {
      local: { display_name: "Bob" },
      entries: {}  // Bob has no entry in ("voting.dot", 0) context
    }
  ]
```

### Privacy-Preserving Display

The host can render a contact picker in a privileged overlay using full contact data, returning only the selected contact's own-context entry to the product. This lets users see rich details without the product receiving cross-context data. The overlay mechanism is host-specific and out of scope.

## Drawbacks

- **Privacy surface.** Even tier 1 reveals the user's social graph. The permission prompt mitigates but does not eliminate this.
- **Full-list delivery.** No per-contact queries. The overlay pattern partially addresses this for picker UIs.
- **Read-only.** Products cannot add contacts. Deferred intentionally.

## Alternatives

### A: Freeform context keys

Loses alignment with Ring VRF contexts and makes scoping ambiguous.

### B: Per-contact lookup by alias

Requires knowing the alias upfront; does not support browsing.

### C: No context scoping

Breaks unlinkability — any product could correlate aliases across all contexts.

## Unresolved Questions

1. **Honour.** Needs a protected path so UAs can display honour without exposing the alias to the product. Whether honour is per-product or universal (or both) needs design. Likely a separate RFC.
2. **Common triage contexts.** Should well-known contexts (profile, honour) have a lighter permission model?
3. **Contact mutation.** Write access deferred to a follow-up RFC.
4. **Filtered subscriptions.** Should tier 2 `host_contacts_subscribe` accept a context filter?
5. **Overlay specification.** The exact overlay mechanism needs its own spec.
6. **Pagination.** May be needed for large contact lists.
