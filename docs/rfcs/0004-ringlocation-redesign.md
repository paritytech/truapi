---
title: "Redesign RingLocation in host_account_create_proof"
type: rfc
status: draft
owner: "@valentin-parity"
pr:
---

# RFC 0004 — Redesign RingLocation in `host_account_create_proof`

|                 |                                                                                                           |
| --------------- |-----------------------------------------------------------------------------------------------------------|
| **Start Date**  | 2026-03-16                                                                                                |
| **Description** | Replace RingLocation with a junction-based path to fix request invalidation and multi-ring pallet support |
| **Authors**     | Valentin Sergeev                                                                                          |

## Summary

This RFC proposes replacing the current `RingLocation` struct in `host_account_create_proof` with a junction-based addressing scheme. The current design is fragile when rings change frequently (e.g. new members are added) because it relies on `ring_root_hash`, which becomes stale mid-request. It also cannot address rings within pallets that host multiple ring collections. The proposed design uses a `Vec<RingLocationJunction>` path that references only stable, immutable identifiers and returns the ring index and revision alongside the proof, so products can pass them directly to downstream precompiles without additional on-chain queries.

## Motivation

### Request invalidation

The current `RingLocation` includes a `ring_root_hash` — a blake2b32 hash of the ring root. When a ring changes frequently (e.g. new members joining), this hash changes, invalidating any in-flight proof request that was constructed against the previous root.

This is particularly problematic for coinage, where the recycler coin transaction extension requires both the ring-vrf proof **and** the revision at which the proof is valid. The current API has no way to communicate this revision back to the caller.

### Hints are insufficient for multi-ring pallets

With the introduction of the membership pallet, a single pallet instance can host rings from multiple ring collections. Each ring is identified by a `(collection_id, ring_index)` pair. The current `RingLocationHint` only supports an optional `pallet_instance`, which is not enough to disambiguate rings within the same pallet. This forces the host into guesswork or requires out-of-band coordination that the API should handle directly.

## Detailed Design

### Status Quo

The current API:

```rust
struct RingLocationHint {
    pallet_instance: Option<u32>
}

struct RingLocation {
    genesis_hash: GenesisHash,
    ring_root_hash: Vec<u8>,
    hints: Option<RingLocationHint>
}

type RingVrfProof = Vec<u8>;

fn host_account_create_proof(
    domain: ProductAccountId,
    ring: RingLocation,
    message: Vec<u8>
) -> Result<RingVrfProof, CreateProofErr>;
```

**Problems:**

1. `ring_root_hash` changes whenever ring membership changes, causing request invalidation if the ring is updated while the host is processing the proof request.
2. `RingLocationHint` cannot address a specific ring within a multi-collection pallet — it only knows `pallet_instance`.
3. The return type (`Vec<u8>`) has no way to communicate the ring revision, which downstream consumers (e.g. coinage transaction extensions) may need.

### Proposed Changes

Replace `RingLocation` with a junction-based path (inspired by XCM's `MultiLocation` junctions) and extend the return type to include the ring revision:

```rust
enum RingLocationJunction {
    Chain(GenesisHash),
    PalletInstance(u8),
    CollectionId(Vec<u8>),
}

type RingLocation = Vec<RingLocationJunction>;

struct RingVrfProof {
    proof: Vec<u8>,
    ring_index: u32,
    ring_revision: u32,
}

fn host_account_create_proof(
    domain: ProductAccountId,
    ring: RingLocation,
    message: Vec<u8>
) -> Result<RingVrfProof, CreateProofErr>;
```

### Design Rationale

**Only stable identifiers in the request.** The product supplies a path of junctions that are constant for the lifetime of the ring — chain genesis hash, pallet instance, and collection id. None of these change when ring membership is updated. The host resolves the current ring root and the caller's ring index internally, eliminating the race condition.

**Ring index and revision in the response.** The host knows which ring index the caller occupies and which revision of the ring it used to generate the proof. By returning both `ring_index` and `ring_revision`, the product can pass them directly to downstream consumers (e.g. the `personhoodInfoByProof` precompile from [individuality#891](https://github.com/paritytech/individuality/pull/891), or coinage's recycler transaction extension) without a separate lookup. Moving `ring_index` to the output also means the product never needs to discover or track its own position in the ring — the host resolves it from the product's derived account.

**Extensible junction set.** New junction variants can be added in the future without breaking existing consumers, since `RingLocation` is simply a vector of junctions. For example, if a new addressing dimension is introduced (e.g. a sub-collection), a new `RingLocationJunction` variant can be appended.

**Abstraction-friendly.** Products should not need to know low-level ring addressing details. The junction-based path lets the host abstract away pallet instances and collection ids behind a well-known alias or default path in the future, while the structured response (`ring_index`, `ring_revision`, `proof`) gives products everything they need to call downstream precompiles without any additional on-chain queries.

### Migration

The `ring_root_hash` field is removed entirely — products no longer need to fetch or compute ring roots. The `hints` field is also removed, as pallet instance addressing is now a first-class junction rather than an optional hint.

Existing products using `host_account_create_proof` will need to update their `RingLocation` construction from:

```rust
// Before
RingLocation {
    genesis_hash: chain_genesis,
    ring_root_hash: computed_hash,
    hints: Some(RingLocationHint { pallet_instance: Some(42) })
}

// After
let location = vec![
    RingLocationJunction::Chain(chain_genesis),
    RingLocationJunction::PalletInstance(42),
    RingLocationJunction::CollectionId(collection),
];
let result = host_account_create_proof(domain, location, message)?;
// result.proof      — the ring-vrf proof bytes
// result.ring_index — the caller's index in the ring (for precompile calls)
// result.ring_revision — the ring revision the proof was generated against
```

### Stakeholders

- **Product developers** — consumers of `host_account_create_proof` who need reliable proof generation without worrying about ring root staleness.
- **Mobile app / host implementors** — responsible for resolving ring locations and generating proofs; benefit from unambiguous ring addressing.

### Testing, Security, and Privacy

- **Testing**: Implementations should verify that proof generation succeeds even when ring membership changes between request construction and proof generation. The returned `ring_index` and `ring_revision` must match the actual ring state used for the proof.
- **Security**: The host must validate that the junction path resolves to a real ring. Invalid or malicious paths should return `CreateProofErr` rather than panicking or producing invalid proofs.
- **Privacy**: No change from the current model — the same information is exchanged, just addressed differently.

### Performance, Ergonomics, and Compatibility

#### Performance

Proof generation performance is unchanged. The host may need an additional lookup to resolve the ring root from the junction path, but this is expected to be negligible compared to the proof computation itself.

#### Ergonomics

Products no longer need to fetch and hash ring roots before requesting a proof — they only need to know the stable addressing coordinates of the ring. This simplifies the product-side implementation and eliminates a common source of errors.

#### Compatibility

This is a breaking change to the `host_account_create_proof` method signature. Both the request type (`RingLocation`) and response type (`RingVrfProof`) are modified. A protocol version bump is required.

## Drawbacks

1. **Breaking change** — All existing consumers of `host_account_create_proof` must update their `RingLocation` construction and handle the new `RingVrfProof` struct instead of raw bytes.
2. **Host complexity** — The host must now resolve the ring root from the junction path internally, which may require additional chain queries. Previously the product supplied the root hash directly.
3. **Junction ordering** — The path is a flat vector with no enforced ordering or validation at the type level. Malformed paths (e.g. missing `Chain` junction) must be handled at runtime.

## Alternatives

- Keep `ring_root_hash` but add a retry mechanism on the product side — rejected because it doesn't solve the revision visibility problem and adds complexity to every product.
- Use a structured struct instead of junction vec — rejected in favor of extensibility; adding new addressing dimensions would require struct changes.
- XCM `MultiLocation` directly — rejected as overly general for this use case, but the junction pattern is borrowed as inspiration.

## Unresolved Questions

1. Should the junction ordering be enforced (e.g. `Chain` must come first), or is any order acceptable as long as the host can resolve it?
2. Should `CollectionId` use a fixed-size type (e.g. `[u8; 32]`) instead of `Vec<u8>` for consistency with other on-chain identifiers?
3. Are there ring identification schemes beyond `(collection_id)` that we should anticipate in the junction design?
4. Should `CreateProofErr` gain a new variant (e.g. `RingLocationInvalid`) for unresolvable junction paths, or is the existing `RingNotFound` sufficient?
5. Should the host provide a well-known default `RingLocation` (e.g. for the canonical personhood ring on the relay chain) so products can request proofs without knowing any chain/pallet/collection details? This would address the ergonomics concern raised in [triangle-js-sdks#81](https://github.com/paritytech/triangle-js-sdks/pull/81).

## References

- [Host API Design Document v0.5](https://docs.google.com/document/d/1AxKjF15y7gmdl-a6twc5wd8R5xcxKxMO8Ahp2l20v0g/edit?usp=sharing)
- XCM `MultiLocation` junction model — inspiration for the junction-based addressing pattern
- [Polkadot People Registry / Ring VRF](https://forum.polkadot.network/t/the-people-registry/12749)
- [individuality#878](https://github.com/paritytech/individuality/pull/878) — alias-account assignment for derived product addresses
- [individuality#891](https://github.com/paritytech/individuality/pull/891) — `personhoodInfoByProof` precompile (motivates `ring_index` in the response)
- [triangle-js-sdks#81 comment](https://github.com/paritytech/triangle-js-sdks/pull/81) — @Zebedeusz's feedback on moving `ring_index` to output and abstraction concerns
