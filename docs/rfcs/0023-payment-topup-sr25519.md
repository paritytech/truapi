---
title: "Use Sr25519 Secret Keys in PaymentTopUpSource"
owner: "@filippovecchiato"
---

# RFC 0023 — Use Sr25519 Secret Keys in PaymentTopUpSource

## Summary

Change all secret key fields in `PaymentTopUpSource` (RFC 0006) from 32-byte keys to 64-byte Sr25519 secret keys. The `PrivateKey` variant switches from `Ed25519PrivateKey` to `Sr25519SecretKey`, and the `Coins` variant widens its keys to match.

## Motivation

RFC 0006 specified Ed25519 for the `PrivateKey` variant, but the host implementation in triangle-js-sdks uses Sr25519 ([triangle-js-sdks#198](https://github.com/paritytech/triangle-js-sdks/pull/198)). The original RFC was simply wrong at that place — the accounts backing top-ups are Sr25519, not Ed25519. The `Coins` variant already used Sr25519, but with a 32-byte mini-secret; the upstream implementation uses the full 64-byte secret key for both.

## Detailed Design

`PaymentTopUpSource` changes from:

```rust
enum PaymentTopUpSource {
    ProductAccount { derivation_index: u32 },
    PrivateKey { ed25519_private_key: [u8; 32] },
    Coins { sr25519_secret_keys: Vec<[u8; 32]> },
}
```

to:

```rust
enum PaymentTopUpSource {
    ProductAccount { derivation_index: u32 },
    PrivateKey { sr25519_secret_key: [u8; 64] },
    Coins { sr25519_secret_keys: Vec<[u8; 64]> },
}
```

Both `PrivateKey` and `Coins` now carry 64-byte Sr25519 secret keys. The `Ed25519PrivateKey` type is removed.

This is a **wire-breaking** change: the SCALE encoding of both variants changes length. All hosts and products must upgrade together within the same protocol version.