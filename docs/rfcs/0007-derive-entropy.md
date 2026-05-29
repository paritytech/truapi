# RFC-0007: Deterministic Entropy Derivation for Products

| Field  | Value            |
| ------ | ---------------- |
| RFC    | 0007             |
| Status | Draft            |
| Author | Valentin Sergeev |
| Date   | 2026-03-30       |

## Summary

This RFC introduces a new Host API method, `host_derive_entropy`, that allows products to derive deterministic 32-byte entropy scoped to a specific product and caller-chosen key. The derivation is based on the root account's BIP-39 entropy and uses BLAKE2b-256 in a three-layer keyed hashing scheme. The same inputs always produce the same output on any conforming host, enabling products to derive stable cryptographic keys (e.g., X25519 key pairs) without managing their own key storage.

## Motivation

Products running on a Polkadot Host currently have no standardized way to derive deterministic cryptographic keys tied to a user's account. This is a blocker for features that require stable key material -- for example, generating X25519 key pairs for encrypted peer-to-peer communication (see [polkadot-desktop#117](https://github.com/paritytech/polkadot-desktop/issues/117)).

Without a host-provided derivation primitive, each product would need to implement its own key management, leading to inconsistent security properties, duplicated effort, and no guarantee of determinism across hosts.

By providing a single, well-specified derivation function at the Host API level, we ensure:

- **Determinism**: The same root account + product + key always yields the same entropy, regardless of which conforming host executes the product.
- **Isolation**: Different products derive independent entropy from the same root account. One product cannot compute another product's derived values.
- **Simplicity**: Products call a single function and receive 32 bytes. Higher-level abstractions (e.g., deriving an X25519 keypair) can be built in SDK layers above.

## Specification

### Host API Method

```rust
type Entropy = [u8; 32];

enum DeriveEntropyError {
    /// An unexpected error occurred in the host (e.g., an internal bug).
    Unknown(GenericErr),
}

/// Derives 32 bytes of deterministic entropy scoped to the calling product
/// and the provided key.
///
/// - `key`: An arbitrary value up to 32 bytes chosen by the caller. The host
///   does not assign any semantic meaning to this parameter; it is opaque
///   input to the derivation. Higher-level SDKs may impose their own structure.
///
/// Returns the same `Entropy` for the same root account, product, and key
/// on every conforming host.
fn host_derive_entropy(key: Vec<u8>) -> Result<Entropy, DeriveEntropyError>
```

### Derivation Algorithm

The derivation proceeds in three layers. Each layer uses BLAKE2b with a 256-bit (32-byte) output, referred to below as `blake2b256`.

Two modes of BLAKE2b are used and must be clearly distinguished:

- **Keyed mode**: BLAKE2b initialized with a key via its built-in keyed hashing support. The key MUST be at most 32 bytes. Denoted as `blake2b256_keyed(message, key)`.
- **Unkeyed mode**: Standard BLAKE2b with no key. Denoted as `blake2b256(message)`.

#### Inputs

| Name                | Description                                                                                                                    |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `productId`         | The identifier of the product that invoked `host_derive_entropy`. Arbitrary-length string; hashed before use as a BLAKE2b key. |
| `rootAccountSecret` | The raw BIP-39 entropy bytes of the root account (NOT the 64-byte PBKDF2-derived seed).                                        |
| `key`               | The `key` argument passed to `host_derive_entropy`. Up to 32 bytes; passed directly as the BLAKE2b key without hashing.        |

#### Steps

```rust
let domainSeparator: &[u8] = b"product-entropy-derivation";

let rootEntropySource: [u8; 32] = blake2b256_keyed(
    message: rootAccountSecret,
    key: domainSeparator,
);
let perProductEntropy: [u8; 32] = blake2b256_keyed(
    message: rootEntropySource,
    key: blake2b256(productId),
);
let requestedEntropy: [u8; 32] = blake2b256_keyed(
    message: perProductEntropy,
    key: key,
);
```

The function returns `requestedEntropy`.

### Error Handling

`host_derive_entropy` returns `DeriveEntropyError::Unknown` only under abnormal circumstances, such as an internal bug in the host implementation. Under normal operation the function always succeeds. Callers should treat `Unknown` as an unrecoverable error.

### Core Invariant

For any conforming host implementation:

> Given the same `rootAccountSecret`, the same `productId`, and the same `key`, `host_derive_entropy(key)` MUST return identical `Entropy`.

This is the fundamental determinism guarantee of this specification.

## Security Considerations

### Threat Model

The Polkadot Host security model operates on a triangle of trust: **Account Holder**, **Host**, and **Product**. The user (Account Holder) explicitly trusts the Host they choose to run. A product executes within the Host's sandbox.

### Why Host-Local Computation (Option 1)

Four options were evaluated for where the derivation is performed:

| Option | What is shared                     | When                   | Host knowledge                                                                                                                                     |
| ------ | ---------------------------------- | ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1      | `rootEntropySource`                | During SSO handshake   | Host can compute all possible entropies for any product and any key. No round trips at runtime.                                                    |
| 2      | `perProductEntropy`                | On-demand, per product | Host learns all possible entropies for a given product. Round-trip to Account Holder on first request per product.                                 |
| 3      | `requestedEntropy`                 | On-demand, per request | Host learns only the specific requested entropy. Round-trip to Account Holder for every request.                                                   |
| 3.1    | `requestedEntropy` (e2e encrypted) | On-demand, per request | Same as option 3, but the entropy is end-to-end encrypted between Account Holder and Product -- Host cannot read it. Round-trip for every request. |

**Option 1 is selected** for the following reasons:

- **No runtime round trips**: After the initial SSO handshake, `host_derive_entropy` is a pure local computation with no IPC or network calls. This makes it fast and predictable.
- **Options 2--4 provide no real additional security**: A malicious host controls the product's execution environment. It can skip calling the Account Holder entirely and return fabricated keys, or it can run products side-by-side and access all derived secrets. Since the user already trusts the host (per the triangle security model), adding Account Holder involvement at runtime does not raise the security bar -- it only adds latency and complexity.

### Secret Material

`rootEntropySource` is derived from the user's raw BIP-39 entropy and must be treated as secret material by the host. It should be stored with the same protections as any other account credential.

## Out of Scope

The following topics are intentionally excluded from this RFC:

- **SSO handshake integration**: The mechanism by which the Account Holder shares `rootEntropySource` with the Host during SSO is defined by the SSO protocol and is outside the scope of this specification.
- **Higher-level SDK abstractions**: Product SDKs may provide convenience methods on top of `host_derive_entropy` (e.g., `deriveX25519KeyPair(purpose: string)`). Such abstractions are outside the scope of this specification.
