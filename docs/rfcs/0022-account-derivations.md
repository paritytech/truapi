---
title: "Account key derivations"
owner: "@valentunn"
---

# RFC 0022 — Account key derivations

|                 |                                                                                          |
| --------------- | ---------------------------------------------------------------------------------------- |
| **RFC Number**  | 22                                                                                       |
| **Start Date**  | 2026-07-20                                                                               |
| **Description** | Specify how hosts derive product accounts, ring-VRF keys, and ECDH keys from the user's root account. |
| **Authors**     | Valentin Sergeev                                                                         |

## Summary

This RFC defines the derivation scheme for every key rooted in the user's main
account:

- **Product accounts** — sr25519 keys at `//product//{productId}/{index}`: a
  hard junction at the product boundary, plain soft derivation below it, no
  secret path components. `{index}` is a 32-byte derivation index.
- **Ring-VRF keys** — a hard-only keyed-hash chain rooted at
  `hash(root_entropy, "ring-vrf")`, with paths `//{domain}//{index}` mirroring
  the product account paths (a product's domain is its `productId`).
- **ECDH keys** — P-256 keys whose key material comes from the same
  keyed-hash chain, rooted at `hash(root_entropy, "ecdh")`, with paths
  `//{domain}`.

It amends `ProductAccountId` (`derivation_index: u32` →
`derivation_index: Either<u32, [u8; 32]>`) and `ProductProofContext.suffix`
(arbitrary bytes → the same `Either`), adds one Accounts Protocol request
for fetching a product's subtree public key, collapses RFC-0010's
`AutoSigning` payload to a single product-root secret key, and assigns product
identities to built-in app features. No truAPI methods are added or removed.

## Definitions

- **The Account Entropy** (`root_entropy`) — the BIP-39 entropy of the main
  user account, created when the user installs the app.
- **The Account Seed** (`root_seed`) — the 32-byte substrate-compatible
  mini-secret derived from the Account Entropy (per `substrate-bip39`).
- **Root keypair** — the sr25519 keypair obtained from the Account Seed. All
  account derivations in this RFC start here.
- **Host** / **Account Holder** — as in RFC-0010: the runtime executing
  products, and the device holding the user's root secret, respectively.
- **MDS** — the multi-device spec describing key management and encryption in
  a multi-device environment.
- **SSO** — single sign-on; a synonym for the Accounts Protocol.
- `Either<L, R>` — a two-variant sum type: `Left(L)` or `Right(R)`.
- `Sr25519PublicKey` — a 32-byte sr25519 public key.
- `hash(data, key)` — 32-byte BLAKE2b-256 in keyed mode.

## Motivation

Today the app's derivations are unspecced and non-unified: each feature
hard-derives its own accounts ad hoc. This RFC replaces that with a single
scheme covering product accounts, built-in features, ring-VRF keys, and ECDH
keys.

The scheme must respect a cryptographic constraint. In sr25519, soft
derivation is invertible from the child side: a **child
private key** plus the **parent public key** and **derivation path** recovers
the **parent private key**. Every key in a purely soft-derived subtree is
therefore equivalent to the subtree root's key.

This rules out deriving product accounts from the root keypair with soft
junctions, even when segments are salted with secret components as in
RFC-0010's current "Product account" definition:

```
/{productId ++ productDerivationSecret}/{index ++ indexDerivationSecret}
```

`AutoSigning` hands the Host the product-subtree private key *together with*
`productDerivationSecret`: the Host can invert the soft junction and recover
the **root private key**. Secret components also force a round trip to the
Account Holder for every account — even for public keys — on Hosts that don't
hold them, contradicting the goal of cheap, prompt-free account operations.

## Detailed Design

Account keys use **sr25519**.

### Product account derivations

```
//product//{productId}/{index}
```

derived from the root keypair with standard substrate sr25519 HDKD — no
secret components, no intermediate hashing layer.

- `//product` — **hard** namespace junction separating product accounts from
  the root keypair's other derivations.
- `//{productId}` — **hard** junction; `productId` is the product's dotNS
  identifier (e.g. `browse.dot`).
- `/{index}` — **soft** junction carrying the 32-byte derivation index.

The hard junction is the security firewall: leaking the `//product//{productId}`
secret key (via `AutoSigning` or compromise) exposes exactly that product's
subtree. Below it, soft derivation adds no exposure — any party holding a
child secret key already holds the product-root secret key.

#### The 32-byte derivation index

Internally — between the Account Holder and Hosts, and in derivation paths —
an account within a product is always identified by a 32-byte index:

```rust
Index32 = [u8; 32]

INDEX_MAGIC: [u8; 28] = blake2b256("product-account-index")[..28]

fn index_bytes(index: u32) -> Index32 {
    u32_le_bytes(index) ++ INDEX_MAGIC
}
```

Plain `u32` indices are the primary form: they keep a product's accounts
enumerable, and products are expected to use them for all ordinary accounts.
Raw 32-byte values are the escape hatch for cases where bytes are genuinely
necessary. `INDEX_MAGIC` keeps the two spaces separate for all practical use
cases: a raw value only collides with an index if it ends in the magic.

In derivation paths the 32-byte index is used directly as the soft-junction
chain code — it is already exactly 32 bytes, so no substrate path-segment
parsing or normalization is involved. The string segments (`product`,
`productId`) use the standard substrate junction normalization.

A product's default account is index `0`, i.e. `index_bytes(0)`.

#### `ProductAccountId`

The wire-level `ProductAccountId` lets products choose between the two index
forms:

```rust
ProductAccountId {
    /// A dotNS domain name identifier (e.g., `"my-product.dot"`).
    dot_ns_identifier: String,
    /// Account selector within the product subtree:
    /// Left — a plain index (primary form); Right — a raw 32-byte index.
    derivation_index: Either<u32, [u8; 32]>,
}
```

Hosts map `Left(n)` to `index_bytes(n)` and pass `Right(bytes)` through
unchanged; past the host API boundary only the 32-byte form exists.

#### `ProductProofContext` (RFC-0004)

The contextual-alias suffix is the same selector. `ProductProofContext` is
amended to:

```rust
ProductProofContext {
    /// dotNS product identifier (e.g. `"my-product.dot"`) scoping the context.
    product_id: String,
    /// Selector distinguishing contexts within the product; expands to the
    /// same 32-byte derivation index as `ProductAccountId.derivation_index`.
    suffix: Either<u32, [u8; 32]>,
}
```

The suffix expands to the same 32-byte value as an account's derivation
index, so the alias ↔ account mapping is the identity on it. This obsoletes
RFC-0004's `product_account_id_for_proof_context` convention (4-byte suffixes
packed into a `u32`).

#### Fetching the product subtree

`//product//{productId}` is hard, so the root public key alone no longer determines
product account public keys. One new Accounts Protocol request closes the
gap:

```rust
/// Host → Account Holder.
ApProductSubtreeRequest {
    /// dotNS identifier of the product whose subtree is requested.
    product_id: String,
}

/// Account Holder → Host.
ApProductSubtreeResponse {
    /// sr25519 public key of `//product//{product_id}`.
    product_public_key: Sr25519PublicKey,
}
```

The request is **consent-free** — the response contains no secret material,
and individual product accounts become public on-chain once used; only
`AutoSigning` (secret material) requires consent, per RFC-0010.

Host behavior:

- Fetch and cache the response on first use of a product's accounts — one
  round trip per product, ever — then derive account public keys locally via
  the soft index junction.
- Without `AutoSigning`, signing round-trips to the Account Holder, which
  derives `//product//{productId}/{index}` from the root keypair and signs.
- With `AutoSigning`, the Host soft-derives the child secret key and signs
  locally.

#### Amendment to RFC-0010 `AutoSigning`

The `AutoSigning` payload collapses to the product-root secret key alone.
`ApAllocatedResource` (and its implementation counterpart
`SsoAllocatedResource`) is amended to:

```rust
AutoSigning {
    /// Secret key of `//product//{productId}`.
    product_root_private_key: Sr25519SecretKey,
}
```

`Sr25519SecretKey = Sr25519PrivateKey ++ Sr25519Nonce` (64 bytes) — the full
expanded secret needed to sign and soft-derive. This supersedes RFC-0010's
"Product account" definition and drops `product_derivation_secret`. Allowance
accounts (`//allowance//{system}//{productId}`) are unchanged.

### Built-in app features

Built-in features derive accounts through the same product scheme, using
reserved product identities as their `productId`:

| Category                             | Feature                      | `productId`  | Protection                                                                                     |
| ------------------------------------ | ---------------------------- | ------------ |------------------------------------------------------------------------------------------------|
| Migrating to a product soon          | Game (DIM2)                  | `dim2.dot`   | Governance-reserved 3–5 char name                                                              |
| Migrating long-term / product-shaped | PoI (DIM1)                   | `poi.dot`    | Governance-reserved 3–5 char name                                                              |
| Migrating long-term / product-shaped | Funding                      | `fund.dot`   | Governance-reserved 3–5 char name                                                              |
| Migrating long-term / product-shaped | Public light person identity | `uid.dot`    | Governance-reserved 3–5 char name                                                              |
| Migrating long-term / product-shaped | Personhood                   | `peopl.dot`  | Governance-reserved 3–5 char name                                                              |
| Not coercible to a product           | Coinage                      | —            | Deferred to a separate RFC (own layout today: `//pps//coin/{index}`, `//pps//ring-vrf/{index}`) |

### Well-known alias accounts

The runtime defines well-known Account Contexts (`resources`, `score`,
`mob-rule`) that are not owned by any product and do not follow truAPI's
product-based context construction. Their handling, including derivation of
linked accounts, is deferred to a separate RFC.

### Ring-VRF derivations

Ring-VRF keys live in their own tree, rooted directly in the Account Entropy:

```
root_ringvrf_entropy = hash(root_entropy, "ring-vrf")
```

#### HDKD for ring-VRF keys

```rust
RingVrfEntropy = [u8; 32]
ChainCode = [u8; 32]

fn derive_ringvrf_hard(parent: RingVrfEntropy, chain_code: ChainCode) -> RingVrfEntropy {
    hash(parent, chain_code)
}
```

To derive a child `RingVrfEntropy` from `parent` for a path `path`:

1. Compute each segment's chain code: string segments use the standard
   32-byte substrate normalization; index segments are the 32-byte index used
   directly. Only **hard** junction separators (`//`) are allowed; a path
   containing a soft separator is invalid. Produces `codes: Vec<ChainCode>`.
2. Fold: `codes.fold(parent, |acc, code| derive_ringvrf_hard(acc, code))`.

#### General scheme and domains

The path shape mirrors the product account paths, derived from
`root_ringvrf_entropy`:

```
//{DerivationDomain}//{DerivationIndex}
```

A `DerivationDomain` is always a `productId` — for built-in features, the
reserved product identity from the table above. `DerivationIndex` is the same
32-byte index format as product accounts, so each domain gets its own index
space.

The personhood keys live under the `peopl.dot` domain:

```rust
// Full personhood ring-VRF key
full_personhood_key  = //peopl.dot//index_bytes(0)

// Light personhood ring-VRF key
light_personhood_key = //peopl.dot//index_bytes(1)
```

Existing keys migrate to these paths. Coinage's ring-VRF keys
(recyclers/vouchers) are deferred to the coinage RFC.

### ECDH key derivations

Keys used for ECDH-based E2E encryption are **P-256 (NIST)** keys. Their key
material comes from the same keyed-hash HDKD as ring-VRF keys — not from
schnorrkel derivations — in a tree rooted directly in the Account Entropy:

```
root_ecdh_entropy = hash(root_entropy, "ecdh")
```

The key material for a domain is the entropy derived from `root_ecdh_entropy`
for the path (via the ring-VRF HDKD fold):

```
//{DerivationDomain}
```

This RFC specifies only this derivation. The exact material-to-P-256 key
mapping, key agreement, KDF, and AEAD choices — and a potential migration
from P-256 to x25519 — are specified in a separate encryption RFC, along with
migration mechanics for currently deployed keys.

Domains for built-in app features:

```rust
// Previously used for ECDH between chat participants.
// Post-MDS this key is shared across all devices and is used for device
// authentication in chat requests; encryption keys are generated randomly
// per device.
chat_domain = "chat"

// E2E communication encryption in the SSO transport.
sso_domain = "sso"

// E2E encryption in the "Chat with Players" chat in DIM2.
// "Chat with Players" is not covered by MDS, so this key is used for E2E
// encryption directly.
game_domain = "game"
```

> **Note:** the `game` domain is expected to go away soon. The Game is
> migrating to the `dim2.dot` product, which will obtain its key material
> via `host_derive_entropy` (RFC-0007) instead.

### Compatibility

There are no production deployments of secret-component derivations or of the
`derivation_index`-based `ProductAccountId`; breaking changes are made freely,
with no migration path. Existing ring-VRF keys move to their `peopl.dot`
paths; deployed encryption keys are handled by the encryption RFC.

## Drawbacks

- **One new Accounts Protocol message**, amortized to one round trip per
  product per Host.
- **No path-string tooling round trip.** The 32-byte index junction cannot be
  typed as a path segment, so `//product//browse.dot/5` in stock tooling
  (`polkadot-js`, `subkey`) does not derive index `5` (`index_bytes(5)`)

## Alternatives

- **Secret-component soft paths**
  (`/{productId ++ secret}/{index ++ secret}`) — rejected for the root-key
  recovery and round-trip problems described in Motivation.
- **Secret components as chain codes** — the same idea carried inside the
  derivation standard itself rather than managed separately; only available
  for ed25519, which lacks the soft public-key derivation this design relies
  on. Rejected together with ed25519.
- **`u32`-only index (status quo wire type)** — keeps accounts enumerable but
  cannot express byte-valued selectors (e.g. alias-linked accounts). Rejected:
  raw bytes are sometimes necessary.
- **Arbitrary byte suffixes** — maximally general, but makes accounts
  non-enumerable by default and pulls substrate path-parsing quirks (numeric
  aliasing) into the scheme. Rejected in favor of the fixed 32-byte index
  with the `u32` form as the primary, encouraged selector.

## Prior Art and References

- **RFC-0004** — context-scoped proofs; its `product/<product_id>/` context
  prefix is the alias-side analogue of the hard product junction here.
- **RFC-0007** — `host_derive_entropy`; a separate per-product entropy
  namespace for products that need raw key material.
- **RFC-0010** — allowance and `AutoSigning`; amended here, allowance
  accounts untouched.
- **Earlier derivations spec draft** — internal discussion material exploring
  secret-component soft derivations and per-feature domains; never confirmed,
  superseded in full by this RFC.
