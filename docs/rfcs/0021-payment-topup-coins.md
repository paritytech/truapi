---
title: "Add Coins variant to PaymentTopUpSource"
owner: "@filippovecchiato"
---

# RFC 0021 — Add `Coins` variant to `PaymentTopUpSource`

|                 |                                                                  |
| --------------- | ---------------------------------------------------------------- |
| **Start Date**  | 2026-05-29                                                       |
| **Description** | Extend `PaymentTopUpSource` with a `Coins` variant for direct coin-key top-ups. |
| **Authors**     | Filippo Vecchiato                                                |

## Summary

Add a `Coins` variant to `PaymentTopUpSource` so products can top up a user's balance by supplying raw sr25519 coin secret keys directly, without an on-chain intermediary.

## Motivation

Issue: [polkadot-app-android-v2#673](https://github.com/paritytech/polkadot-app-android-v2/issues/673)

T3rminal's W3S payment flow receives coin secret keys via the statement store and needs to move them into the user's coin set. `PaymentTopUpSource` only supports on-chain funding sources today, forcing an unnecessary on-chain round trip.

## Detailed Design

Append a new variant to `PaymentTopUpSource`:

```rust
enum PaymentTopUpSource {
    ProductAccount { derivation_index: u32 },
    PrivateKey { ed25519_private_key: [u8; 32] },
    Coins { sr25519_secret_keys: Vec<[u8; 32]> },
}
```

`host_payment_top_up` is unchanged; the host validates each key, claims the coins, and credits the target purse. Spent or sniped coins are skipped. No user consent required (top-ups are always in the user's favour).

A `PartialPayment { credited: Balance }` variant is added to `HostPaymentTopUpError` so the caller knows how much was credited when some coins could not be claimed.

## Drawbacks

- The caller learns the credited total but not which individual keys failed.

## Alternatives

- **An encrypted-cheque deposit flow.** Wrapping keys into an encrypted cheque tied to a receivable is unnecessary ceremony when the product already holds keys in the clear.

## Unresolved Questions

1. Should the protocol impose a hard limit on the number of keys per call?
