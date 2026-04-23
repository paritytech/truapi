# RFC-0013: Favourites API

|                 |                                                                 |
| --------------- | --------------------------------------------------------------- |
| **Start Date**  | 2026-04-21                                                      |
| **Description** | Let products read and manage the user's bookmarked apps         |
| **Authors**     | Filippo Vecchiato                                               |

## Summary

Products can query, add and remove bookmarked apps from the host's local product catalogue. The host exposes a subscription for the installed-product list and two mutations for adding/removing entries. Browse (the on-chain discovery product) receives privileged access without an explicit permission prompt.

## Motivation

The host maintains a local catalogue of products the user has bookmarked (starred). Today this data lives in the host's IndexedDB and is inaccessible to products. Browse — the primary discovery surface — cannot show which apps are already installed or let the user bookmark new ones without direct database access.

Exposing this catalogue:

1. **Enables discovery UIs** — Browse can render install/uninstall affordances inline.
2. **Keeps the host authoritative** — mutations go through the host, which owns the storage schema and can enforce invariants.
3. **Supports other products** — any product with permission can read the installed list (e.g. a dashboard, launcher, or analytics tool).

## Detailed Design

### Data Model

```rust
struct FavouriteProduct {
  product_id: DotNsIdentifier,
  installed: bool,
  source: ProductSource,
  created_at: Timestamp,
  updated_at: Timestamp
}

enum ProductSource {
  Remote,   // discovered via on-chain registry
  Local     // sideloaded or manually added
}
```

This mirrors the existing `ProductRecord` in the host's `products` table, exposing only the fields relevant to products.

### API

```rust
enum FavouritesErr {
  NotConnected,
  Rejected,
  Unknown(GenericErr)
}

fn host_favourites_subscribe(
  callback: fn(Vec<FavouriteProduct>)
) -> Result<Subscriber, FavouritesErr>;

fn host_favourites_add(
  product_id: DotNsIdentifier
) -> Result<FavouriteProduct, FavouritesErr>;

fn host_favourites_forget(
  product_id: DotNsIdentifier
) -> Result<void, FavouritesErr>;
```

- `host_favourites_subscribe` delivers the full list on each callback; hosts MAY debounce.
- `host_favourites_add` upserts a `FavouriteProduct` with `source: Remote`, setting `created_at` on first install and `updated_at` on every call. Returns the resulting record.
- `host_favourites_forget` removes the product from the catalogue entirely.

All methods require authentication (RFC-0009).

### Permission Model

Extends `DevicePermission` from RFC-0002:

```rust
enum DevicePermission {
  // ... existing variants ...
  Favourites
}
```

| Permission | Grants |
|-----------|--------|
| `Favourites` | Read, add, and forget bookmarked products |

**Browse privilege:** The host MAY grant implicit `Favourites` to Browse (the built-in discovery product) without prompting. This is analogous to how Browse currently writes directly to the products table.

### Host Behaviour

On `host_favourites_add`:
1. Upsert row in the products table with `installed: true`, `source: 'remote'`.
2. Set `created_at` if new, `updated_at` on every call.
3. Notify all active subscribers.

On `host_favourites_forget`:
1. Delete the row from the products table.
2. Notify all active subscribers.

The host SHOULD display a brief confirmation toast on install/forget for products other than Browse.

Favourites are local to the host instance. Cross-host sync is out of scope for this RFC.

## Drawbacks

- **Full-list delivery.** No pagination or filtered subscriptions. Acceptable for typical catalogue sizes (tens to low hundreds).
- **Browse coupling.** Implicit privilege for Browse assumes a well-known product identity. If Browse's DotNS identifier changes, the host must update its allowlist.

## Alternatives

-

## Unresolved Questions

1. **Batch operations.** Should `host_favourites_add` accept multiple product IDs?
