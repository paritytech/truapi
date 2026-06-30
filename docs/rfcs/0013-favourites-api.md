---
title: "Favourites API"
owner: "@filippovecchiato"
---

# RFC 0013 — Favourites API

## Summary

Products can query, add, and remove bookmarked apps from the host's local product catalogue. The host exposes a subscription for the bookmarked-product list and two mutations for adding/removing entries.

## Motivation

Hosts maintain a local catalogue of products the user has bookmarked, but this data is inaccessible to products. Discovery surfaces like Browse cannot show which apps are already bookmarked or let the user add new ones without this API.

## Detailed Design

### Data Model

```rust
struct FavouriteProduct {
    product_id: String,
    source: FavouriteProductSource,
    created_at: u64,
    updated_at: u64,
}

enum FavouriteProductSource {
    Remote,
    Local,
}
```

`product_id` is a DotNS identifier. `source` distinguishes on-chain registry discoveries (`Remote`) from sideloaded entries (`Local`). Timestamps are Unix seconds.

### API

Three methods on a `Favourites` trait:

**`subscribe`** — streams the full bookmarked-product list on each change. Hosts MAY debounce.

**`add`** — upserts a product with `source: Remote`, setting `created_at` on first add and `updated_at` on every call. Returns the resulting record.

**`forget`** — removes the product from the catalogue.

### Error Handling

Subscription errors and mutation errors use `CallError` with a domain enum:

```rust
enum HostFavouritesSubscribeError {
    Unknown { reason: String },
}

enum HostFavouritesAddError {
    Unknown { reason: String },
}

enum HostFavouritesForgetError {
    NotFound,
    Unknown { reason: String },
}
```

Permission denial and unsupported-host cases are handled by `CallError::Denied` and `CallError::Unsupported`.

### Permission Model

The host SHOULD prompt the user before granting a product access to the favourites catalogue. The host MAY grant implicit access to Browse (the built-in discovery product) without prompting.

### Host Behaviour

Favourites are local to the host instance. Cross-host sync is out of scope.

## Drawbacks

- **Full-list delivery.** No pagination or filtered subscriptions. Acceptable for typical catalogue sizes (tens to low hundreds).
- **Browse coupling.** Implicit privilege for Browse assumes a well-known product identity.
