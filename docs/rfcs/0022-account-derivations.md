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

- **Product accounts** — sr25519 keys at `//product//{productId}/{suffix}`: a hard
  junction at the product boundary, plain soft derivation below it, no secret
  path components.
- **Ring-VRF keys** — a hard-only keyed-hash chain rooted at
  `hash(root_entropy, "ring-vrf")`, with paths `//{domain}//{index}`.
- **ECDH keys** — sr25519 keys hard-derived at `//ecdh//{domain}`.

It amends `ProductAccountId` (`derivation_index: u32` →
`derivation_suffix: Bytes`), adds one Accounts Protocol request for fetching a
product's subtree public key, collapses RFC-0010's `AutoSigning` payload to a
single product-root secret key, and assigns product identities to built-in app
features. No truAPI methods are added or removed.

## Definitions

- **The Account Entropy** (`root_entropy`) — the BIP-39 entropy of the main
  user account, created when the user installs the app.
- **The Account Seed** (`root_seed`) — the 32-byte substrate-compatible
  mini-secret derived from the Account Entropy (per `substrate-bip39`).
- **Root keypair** — the sr25519 keypair obtained from the Account Seed. All
  account and ECDH derivations in this RFC start here.
- **Host** / **Account Holder** — as in RFC-0010: the runtime executing
  products, and the device holding the user's root secret, respectively.
- **MDS** — the multi-device spec describing key management and encryption in
  a multi-device environment.
- **SSO** — single sign-on; a synonym for the Accounts Protocol.
- `Bytes` — a variable-length byte array (SCALE `Vec<u8>` on the wire).
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
//product//{productId}/{suffix}
```

derived from the root keypair with standard substrate sr25519 HDKD — no
secret components, no intermediate hashing layer.

- `//product` — **hard** namespace junction separating product accounts from
  the root keypair's other derivations.
- `//{productId}` — **hard** junction; `productId` is the product's dotNS
  identifier (e.g. `browse.dot`).
- `/{suffix}` — **soft** junction.

The hard junction is the security firewall: leaking the `//product//{productId}`
secret key (via `AutoSigning` or compromise) exposes exactly that product's
subtree. Below it, soft derivation adds no exposure — any party holding a
child secret key already holds the product-root secret key.

#### `ProductAccountId`

The wire-level `ProductAccountId` is amended to carry the derivation suffix
directly:

```rust
ProductAccountId {
    /// A dotNS domain name identifier (e.g., `"my-product.dot"`).
    dot_ns_identifier: String,
    /// Derivation suffix selecting an account within the product subtree.
    derivation_suffix: Bytes,
}
```

The suffix equals the RFC-0004 proof-context suffix — the alias ↔ account
mapping is the identity on the suffix bytes. RFC-0004's
`product_account_id_for_proof_context` convention (4-byte suffixes packed into
the former `u32`) is obsolete.

#### Junction encoding

A suffix is interpreted exactly as the standard substrate junction encoder
interprets a path segment — no custom rules — keeping paths compatible with
existing tooling (`polkadot-js`, `subkey`). A suffix that is not valid UTF-8
(which the encoder cannot see as a path segment) is a raw-bytes junction.

One implication: the encoder normalizes numeric segments, so multiple byte
forms of the same number — `b"5"`, `b"05"`, `b"+5"` — derive the same key.
Products that need distinct accounts must use distinct numbers or
non-numeric suffixes.

The empty suffix is invalid; a product's default account uses suffix `b"0"`.

`//product//browse.dot/5` in stock tooling therefore derives the same key as
`derivation_suffix = b"5"`.

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
  the soft suffix junction.
- Without `AutoSigning`, signing round-trips to the Account Holder, which
  derives `//product//{productId}/{suffix}` from the root keypair and signs.
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

| Category                             | Feature                      | `productId`  | Protection                                                                                       |
| ------------------------------------ | ---------------------------- | ------------ | ------------------------------------------------------------------------------------------------ |
| Migrating to a product soon          | Game (DIM2)                  | `dimtwo.dot` | Convention only — the product ships imminently and claims the name                               |
| Migrating long-term / product-shaped | PoI (DIM1)                   | `poi.dot`    | Governance-reserved 3–5 char name                                                                |
| Migrating long-term / product-shaped | Funding                      | `fund.dot`   | Governance-reserved 3–5 char name                                                                |
| Migrating long-term / product-shaped | Public light person identity | `uid.dot`    | Governance-reserved 3–5 char name                                                                |
| Not coercible to a product           | Coinage                      | —            | Deferred to a separate RFC (own layout today: `//pps//coin/{index}`, `//pps//ring-vrf/{index}`)  |

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

1. Parse `path` per the substrate derivation scheme, with the standard 32-byte
   chain-code normalization of each segment. Only **hard** junction
   separators (`//`) are allowed; a path containing a soft separator is
   invalid. Produces `codes: Vec<ChainCode>`.
2. Fold: `codes.fold(parent, |acc, code| derive_ringvrf_hard(acc, code))`.

#### General scheme and domains

The general path shape, derived from `root_ringvrf_entropy`, is:

```
//{DerivationDomain}//{DerivationIndex}
```

The currently defined domains are single keys and carry **no index segment**:

```rust
// Ring-VRF key used in the lite-people ring
light_people_domain = "lite-people"   // key: //lite-people

// Ring-VRF key used in the people ring
people_domain = "people"              // key: //people
```

The indexed form is reserved for future domains that need multiple keys.
Coinage's ring-VRF keys (recyclers/vouchers) are deferred to the coinage RFC.

### ECDH key derivations

Keys used for ECDH-based E2E encryption are **sr25519** keys, hard-derived
from the root keypair with standard substrate HDKD:

```
//ecdh//{DerivationDomain}
```

This RFC specifies only the derivation of these keys; key agreement, KDF, and
AEAD choices are specified in a separate encryption RFC.

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
> migrating to the `dimtwo.dot` product, which will obtain its key material
> via `host_derive_entropy` (RFC-0007) instead.

### Compatibility

There are no production deployments of secret-component derivations or of the
`derivation_index`-based `ProductAccountId`; breaking changes are made freely,
with no migration path.

## Drawbacks

- **One new Accounts Protocol message**, amortized to one round trip per
  product per Host.
- **Convention-only protection for `dimtwo.dot`** until the Game product
  deployment claims the name.
- **Public enumerability**: a party holding a product's subtree public key
  can enumerate all of its account public keys. Secret path components would
  add no guarantee here: the `//product//{productId}` public key is published nowhere
  — not even exposed to the product, only Hosts see it via
  `ApProductSubtreeResponse` — and a secret component would have to be shared
  with Hosts through that same response, so whatever leaks the subtree public
  key leaks the secret with it. Individual product accounts are public
  on-chain anyway; privacy-sensitive identities use ring-VRF contextual
  aliases (RFC-0004).

## Alternatives

- **Secret-component soft paths**
  (`/{productId ++ secret}/{suffix ++ secret}`) — rejected for the root-key
  recovery and round-trip problems described in Motivation.
- **Secret components as chain codes** — the same idea carried inside the
  derivation standard itself rather than managed separately; only available
  for ed25519, which lacks the soft public-key derivation this design relies
  on. Rejected together with ed25519.
- **`u32` index suffix** — keeping the current wire type as the derivation
  primitive. Rejected in favor of arbitrary byte suffixes, which subsume the
  index and equal RFC-0004 suffixes without the 4-byte packing convention.

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
