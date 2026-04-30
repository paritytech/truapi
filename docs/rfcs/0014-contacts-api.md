# RFC-0014: Contacts API

|                 |                                                                 |
| --------------- | --------------------------------------------------------------- |
| **Start Date**  | 2026-04-17                                                      |
| **Description** | Expose the user's contact list to products via TrUAPI           |
| **Authors**     | Filippo Vecchiato                                               |

## Summary

Products can read the user's host-managed address book. Each contact pairs local metadata with a context-scoped map keyed by `ProductAccountId` (`DotNsIdentifier` + `DerivationIndex`) — the same namespace used for Ring VRF alias derivation. By default a product only sees entries for its own context; cross-context access is a separate privilege.

## Motivation

Our privacy model gives each user a different alias and account in each product context, so no single handle identifies a person across products. A contact list matters because it is the most convenient way for a user to maintain a private notebook of mappings — local name ("Alice") to whichever identifier represents her in each product.

The host already manages an address book, but does not expose it to products. Without this API, products cannot leverage the user's social circle to provide useful features — users must paste raw keys or scan QR codes for every interaction. Think of it like Spotify connecting to your Facebook friends, or WhatsApp reading your phone's contact list: letting products see the user's contacts (with permission) unlocks a class of social features that are otherwise impossible.

Exposing the contact list:

1. **Unlocks social features** — products can use the host's contact list to show who among the user's contacts is relevant in their context (e.g. "friends who also use this app"), without the user re-entering information.
2. **Per-product views of shared contacts** — multiple products see the same contact through their own context lens, each resolving to the appropriate alias and account for that product.
3. **Lets users navigate contextual identities** — a contact has different aliases and accounts per DotNS context; the API lets users see and navigate these mappings while preserving unlinkability across products.

## Detailed Design

### Data Model

Each host already has its own contact schema (e.g. desktop uses `P2PPeer { type, accountId, name }`, mobile uses `Chat.Contact { accountId, username, ... }`). This RFC does not replace those internal schemas — it defines the product-facing API shape that hosts translate their internal data into.

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

`ContactContext` is a `ProductAccountId` (`DotNsIdentifier` + `DerivationIndex`). The `DerivationIndex` is needed since there can be multiple derivations for a given account. The host derives the `[u8; 32]` Ring VRF context by hashing this identifier internally — note that the Ring VRF context type (`[u8; 32]`) differs from `ProductAccountId` in format; the conversion is a host-internal concern.

`ContextContactInfo` fields are optional; either or both may be present.

### Access Tiers

#### Tier 1: Own-context (default)

The host filters `entries` to only the requesting product's `ProductAccountId`. `LocalContactInfo` is always included. The product sees only identifiers scoped to its own context — it cannot learn the user's aliases or accounts in other products.

#### Tier 2: Cross-context (privileged)

Returns the full `entries` map. Required for host-privileged products that aggregate identities across contexts (e.g. Browse, profile, honour). The host MAY grant implicit tier 2 access to built-in host products that need it for their core function (e.g. a contact management UI).

### API

```rust
enum ContactsErr {
  NotConnected,
  Rejected,
  Unknown(GenericErr)
}

fn host_contacts_get(
  context: Option<DotNsIdentifier>
) -> Result<Vec<Contact>, ContactsErr>;

fn host_contacts_subscribe(
  callback: fn(Vec<Contact>)
) -> Result<Subscriber, ContactsErr>;
```

Both require authentication (RFC-0009). The host prompts for permission before returning. `host_contacts_subscribe` delivers the full filtered list on each callback; hosts MAY debounce.

When `context` is `None`, the host uses the calling product's own `DotNsIdentifier` (tier 1). When `context` is `Some(identifier)` and matches the calling product, it is equivalent to `None` (tier 1). When `context` names a different product, the host requires `DevicePermission::ContactsCrossContext` (tier 2) and filters entries to that product's context.

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

### A: Freeform context keys instead of ProductAccountId

Using arbitrary strings as context keys would lose alignment with Ring VRF contexts and make scoping ambiguous — there would be no canonical key for "this product's view of a contact."

### B: Per-contact lookup by alias instead of full list

An API that takes an alias and returns the matching contact would require the product to already know the alias, which defeats the discovery use case. Products need to browse the contact list, not just resolve known identifiers.

### C: No context scoping — return all entries to all products

Simpler, but breaks unlinkability. Any product could correlate aliases across all contexts, learning which contacts the user interacts with in other products.

## Unresolved Questions

1. **How do contacts enter the address book?** This RFC is read-only. The mechanism by which contacts are added (peer discovery, QR scan, manual entry, chat history) is host-specific and not specified here. A follow-up RFC should define a product-facing write API.
2. **Honour.** Needs a protected path so UAs can display honour without exposing the alias to the product. Whether honour is per-product or universal (or both) needs design. Likely a separate RFC.
3. **Common triage contexts.** Should well-known contexts (profile, honour) have a lighter permission model?
4. **Contact mutation.** Write access deferred to a follow-up RFC.
5. **Filtered subscriptions.** Should tier 2 `host_contacts_subscribe` accept a context filter?
6. **Overlay specification.** The exact overlay mechanism needs its own spec.
7. **Pagination.** May be needed for large contact lists — full-list delivery could become a performance concern as address books grow.
