# RFC 0004 — Redesign RingLocation in `host_account_create_proof`

|                 |                                                                                                           |
| --------------- |-----------------------------------------------------------------------------------------------------------|
| **Start Date**  | 2026-03-16                                                                                                |
| **Description** | Junction-based RingLocation, context-scoped proofs, and a specified host member-key selection contract |
| **Authors**     | Valentin Sergeev                                                                                          |

## Summary

Redesign `host_account_create_proof` and `host_account_get_alias`:

1. **Junction-based ring addressing** — replace the `ring_root_hash`-based `RingLocation` with a struct carrying a required `chain_id` and a `Vec<RingLocationJunction>` path of stable, immutable identifiers.
2. **Member-key-based, context-scoped proofs** — replace `domain: ProductAccountId` with `ProductProofContext = (ProductId, ProductProofContextSuffix)`. The proof is created with a member key the host holds (selected for the requested ring); the context scopes the derived alias for unlinkability.
3. **Richer output and errors** — return `contextual_alias`, `ring_index`, and `ring_revision`; specify host member-key selection; add a `NotMember` error.

No protocol version bump is required: the current shape of these methods is unusable (the `ring_root_hash` race makes it broken by construction) and is not implemented or consumed anywhere yet, so it can be replaced in place.

## Motivation

- **Request invalidation.** `ring_root_hash` changes whenever ring membership changes, invalidating any in-flight proof request built against the previous root.
- **No revision in the response.** Downstream consumers (coinage's recycler transaction extension, the `personhoodInfoByProof` precompile) need the ring revision and index, which the current `Vec<u8>` return cannot carry.
- **Hints can't address multi-ring pallets.** With the membership pallet, one pallet instance hosts rings from multiple collections, each identified by `(collection_id, ring_index)`. `RingLocationHint`'s optional `pallet_instance` cannot disambiguate them.
- **`domain: ProductAccountId` is the wrong input.** Proof generation depends only on which member key proves membership in the requested ring — the host holds one or more member keys (possibly different keys for different rings) and selects the right one. A derived product account and its derivation index have nothing to do with that. The old signature conflated product-account derivation with proof generation; unlinkability instead comes from the `context` (the same member key under different contexts yields different, unlinkable aliases), so the request needs an explicit, product-scoped context rather than a derivation index.
- **Member-key selection is unspecified.** A host may hold several member keys but the API hides them (exposing them leaks identity). Without a defined selection contract, two hosts can derive different aliases for the same request.
- **No "not a member" error.** A user who has not reached full personhood is not in the ring. `CreateProofErr` cannot distinguish this from "ring does not exist", so products can't route the user to onboarding.

## Status Quo

```rust
struct RingLocationHint { pallet_instance: Option<u32> }
struct RingLocation { genesis_hash: GenesisHash, ring_root_hash: Vec<u8>, hints: Option<RingLocationHint> }
type RingVrfProof = Vec<u8>;

fn host_account_create_proof(domain: ProductAccountId, ring: RingLocation, message: Vec<u8>)
    -> Result<RingVrfProof, CreateProofErr>;
fn host_account_get_alias(domain: ProductAccountId)
    -> Result<ContextualAlias, RequestCredentialsErr>;
```

## Design

### Ring addressing

`chain_id` is a required field (not a junction) so a location can never omit its chain; the junctions address the ring within it. All identifiers are stable for the ring's lifetime, so the host can resolve the current root and the caller's index internally without a membership-change race. New `RingLocationJunction` variants can be added without breaking consumers. (The junction pattern is borrowed from XCM's `MultiLocation`.)

```rust
enum RingLocationJunction {
    PalletInstance(u8),
    CollectionId(Vec<u8>),
}

struct RingLocation {
    chain_id: GenesisHash,
    junctions: Vec<RingLocationJunction>,
}
```

### Product-scoped proof context

`ProductId` is the existing dotNS product identifier (named here as a reminder of what scopes the context). `domain: ProductAccountId` is replaced by:

```rust
type ProductProofContextSuffix = Vec<u8>;            // arbitrary bytes
type ProductProofContext = (ProductId, ProductProofContextSuffix);

// 32-byte context bound into the proof.
fn product_context_bytes(context: ProductProofContext) -> [u8; 32] {
    blake2b256(utf8("product/") ++ utf8(context.0) ++ utf8("/") ++ context.1)
}
```

- **Product-scoped.** The `product/<product_id>/` prefix stops a malicious product from choosing a suffix that collides with another product's context and thereby links its aliases. This is a privacy boundary.
- **Arbitrary-byte suffix.** Some contexts need more than one index — e.g. a pgas claim derives its context from two `u32`s (period and sequence). A single-index suffix would make them unrepresentable.

### `create_proof` and `get_alias`

The proof is created with a member key the host holds; the host selects which key based on the requested ring (see below). Both methods take the same `(context, ring)` so they derive the same alias.

```rust
struct RingVrfProof {
    proof: Vec<u8>,
    contextual_alias: ContextualAlias,
    ring_index: u32,
    ring_revision: u32,
}

fn host_account_create_proof(context: ProductProofContext, ring: RingLocation, message: Vec<u8>)
    -> Result<RingVrfProof, CreateProofErr>;

fn host_account_get_alias(context: ProductProofContext, ring: RingLocation)
    -> Result<ContextualAlias, RequestCredentialsErr>;
```

`ring_index` / `ring_revision` let products call downstream precompiles without a separate lookup. `contextual_alias` is an ergonomics optimization — the same value `get_alias` returns for the same `(context, ring)` — saving a round trip when a caller needs both proof and alias (e.g. a voting contract keying votes by alias). The host MUST select the member key identically in both methods so the two aliases match.

### Host member-key selection

The host may hold multiple member keys; the API exposes neither the keys nor their ids. The host MUST:

1. Define the **"PoP" ring collection** as the collection corresponding to full-personhood rings.
2. Choose a member key that is present in / logically corresponds to the requested `RingLocation`.
3. If correspondence is not determinable, fall back to a key corresponding to the "PoP" ring.
4. If multiple keys correspond to the same ring, consistently pick any one — the choice MUST be stable across calls for the same inputs so the alias is stable.

**Out of scope:** explicit member-key management (letting the caller reference a specific key rather than having the host infer one) is left to a future RFC — exposing keys or their ids is a separate, larger design with its own privacy considerations.

### Errors

```rust
enum CreateProofErr { RingNotFound, NotMember, Rejected, Unknown }
```

`NotMember` is returned when the selected member key is not a member of the requested ring — most importantly when the user has not yet reached full personhood — letting products distinguish it from `RingNotFound` and route to onboarding.

### Usage

`ring_root_hash`, `hints`, and the `domain` parameter are gone — products never fetch or hash ring roots or manage derivation indices.

```rust
let location = RingLocation {
    chain_id: chain_genesis,
    junctions: vec![
        RingLocationJunction::PalletInstance(42),
        RingLocationJunction::CollectionId(collection),
    ],
};
let result = host_account_create_proof(
    (product_id, suffix),
    location,
    message,
)?;
// result.proof / contextual_alias / ring_index / ring_revision
```

## Out of Scope: Product-SDK Helpers (Non-Normative)

These live at the product-sdk level, not in truAPI; the host implements none of them. Documented only because they shape how products build a `ProductProofContext`.

**Default context.** For contexts that need no suffix, the sdk can use a canonical default:

```rust
const SINGLETON_PROOF_SUFFIX: [u8; 1] = [0];
fn singleton_proof_context(product_id: ProductId) -> ProductProofContext {
    (product_id, SINGLETON_PROOF_SUFFIX.to_vec())
}
```

**Context ↔ accountId linkability.** To set an account as the alias for a context, the sdk needs a canonical suffix → `DerivationIndex` mapping (`host_account_get_account` takes `ProductAccountId = (ProductId, DerivationIndex)`):

```rust
fn product_account_id_for_proof_context(product_id: ProductId, suffix: [u8; 4]) -> ProductAccountId {
    ProductAccountId { product_id, derivation_index: u32_from_be_bytes(suffix) }
}
fn u32_from_be_bytes(bytes: [u8; 4]) -> u32;   // big-endian
```

Defined only for 4-byte suffixes to keep a bijection with `u32`. Hashing arbitrary bytes down to 4 was rejected — the space is too small (high collision risk). This is a helper-level limit only: truAPI still accepts arbitrary-byte suffixes, so products not needing a 1:1 context→account mapping are unaffected.

## Drawbacks

- **Host complexity** — the host must resolve the root from the junction path and implement member-key selection (PoP fallback + stable tiebreak).
- **No type-level junction validation** — `chain_id` is mandatory, but the `junctions` vector has no enforced ordering; malformed paths are handled at runtime.

## Alternatives

- Keep `ring_root_hash` with product-side retry — doesn't solve revision visibility; adds complexity to every product.
- Keep `domain: ProductAccountId` plus a separate context — keeps proof generation tied to a derived product account instead of the host's member key for the ring.
- Single-`u32` suffix — too narrow; real contexts (pgas claims) need more.
- XCM `MultiLocation` directly — overly general; only the junction pattern is borrowed.

## References

- [Host API Design Document v0.5](https://docs.google.com/document/d/1AxKjF15y7gmdl-a6twc5wd8R5xcxKxMO8Ahp2l20v0g/edit?usp=sharing)
- Technical Design: Sybil-Resistant Voting with Personhood — driving product for the member-key-based proof model, the `contextual_alias` response, and `NotMember`.
- [Polkadot People Registry / Ring VRF](https://forum.polkadot.network/t/the-people-registry/12749)
- [individuality#878](https://github.com/paritytech/individuality/pull/878) — alias-account assignment for derived product addresses
- [individuality#891](https://github.com/paritytech/individuality/pull/891) — `personhoodInfoByProof` precompile (motivates the richer response)
- [triangle-js-sdks#81 comment](https://github.com/paritytech/triangle-js-sdks/pull/81) — feedback on moving `ring_index` to output and abstraction concerns
