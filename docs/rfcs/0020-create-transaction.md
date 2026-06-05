# RFC-0020: Remove `context` from `create_transaction` and mirror in Accounts Protocol

|                 |                                                             |
| --------------- | ----------------------------------------------------------- |
| **RFC Number**  | 20                                                          |
| **Start Date**  | 2026-05-11                                                  |
| **Description** | Formalize and fill the gaps for `create_transaction` calls. |
| **Authors**     | Valentin Sergeev                                            |

## Summary

Tighten the contract of `host_create_transaction` and `host_create_transaction_with_legacy_account`:

1. Drop the `context` field from `TxPayload`.
2. Parametrize `TxPayload` by signer type, folding the standalone `account_id` parameter into the payload.
3. Add an explicit `genesis_hash` field at the payload root so chain identification does not piggy-back on `CheckGenesis` extension decoding.
4. Define the Accounts Protocol message pairs the Host uses to delegate signing to the Account Holder.

## Motivation

**`context` does not belong on the wire.** `TxPayloadContext` (`metadata`, `token_symbol`, `token_decimals`, `best_block_height`) came from the Polkadot-API offline-signer proposal ([polkadot-js/api#6213](https://github.com/polkadot-js/api/issues/6213)), where the dApp is the only online participant and must hand the signer everything it needs. Our signer (Host or Account Holder) is always online: both hold a live chain connection and can derive every one of these fields themselves from the payload's `genesis_hash`. Keeping `context` is harmful on three counts:

- **Security.** A product shipping its own `metadata` blob can influence how the signer interprets the call. The signer is the security boundary; context must come from the chain, not the caller.
- **Bytes.** Runtime metadata is hundreds of kilobytes, paid on every signing request and every AP round-trip when AutoSigning is not granted.
- **Redundancy.** Token symbol/decimals come from chain spec; best block comes from the signer's own follow. Nothing in `context` is unique to the product.

**`signer: Option<str>` is overloaded.** Today the field means a different thing per call: `host_create_transaction` ignores it and uses a separate `account_id: ProductAccountId` parameter; `host_create_transaction_with_legacy_account` populates it as a hex-encoded `AccountId`. Stringification of an already-typed identifier, a dead field on one variant, and no compile-time guarantee the right kind of signer was supplied. Parametrizing the payload type lets each call site state its signer type precisely and removes the duplicated `account_id` parameter.

**No version envelope on the host API.** Action enums (`host_create_transaction_request(Versioned<...>)`) already carry a version at the message layer, so a second `VersionedTxPayload` envelope inside the payload is redundant. The version envelope only remains on the Accounts Protocol side, where the flat `SsoMessageContent` enum gives the payload no other place to carry its version.

**Explicit `genesis_hash` at the payload root.** Without it the signer has to locate the `CheckGenesis` extension and read its `additional_signed` to learn which chain the transaction targets — but decoding any extension at all already requires the chain's runtime metadata, which means you need the chain *before* you can decode. The current workaround relies on `CheckGenesis`'s SCALE encoding happening to be a raw 32-byte H256 the signer can lift out without a real decode pass; that is brittle and conflates two responsibilities. Carrying `genesis_hash` as a top-level field breaks the cycle: chain identification happens first, extension decoding happens against the correctly-selected runtime, and no extension is privileged over the others.

## Stakeholders

- **Product developers** — construct slimmer payloads; no metadata bundling.
- **Host developers** — derive any needed runtime context from the chain instead of the payload.
- **Account Holder developers (Mobile App)** — implement the new AP message pairs end to end, including deriving signing context.

## Explanation

### TrUAPI

`TxPayloadContext` is removed. `TxPayload` becomes generic over `Signer`; `signer` becomes required and typed. The `V1` suffix is dropped — versioning lives on the action enum.

Before:

```rust
struct TxPayloadContext {
  metadata: Vec<u8>,
  token_symbol: str,
  token_decimals: u32,
  best_block_height: u32
}

struct TxPayloadV1 {
  signer: Option<str>,
  call_data: Vec<u8>,
  extensions: Vec<TxPayloadExtensionV1>,
  tx_ext_version: u8,
  context: TxPayloadContext
}

fn host_create_transaction(
  account_id: ProductAccountId,
  payload: VersionedTxPayload
) -> Result<Vec<u8>, CreateTransactionErr>;

fn host_create_transaction_with_legacy_account(
  payload: VersionedTxPayload
) -> Result<Vec<u8>, CreateTransactionErr>;
```

After:

```rust
struct TxPayload<Signer> {
  signer: Signer,
  genesis_hash: GenesisHash,
  call_data: Vec<u8>,
  extensions: Vec<TxPayloadExtension>,
  tx_ext_version: u8
}

struct TxPayloadExtension {
  id: str,
  extra: Vec<u8>,
  additional_signed: Vec<u8>
}

fn host_create_transaction(
  payload: TxPayload<ProductAccountId>
) -> Result<Vec<u8>, CreateTransactionErr>;

fn host_create_transaction_with_legacy_account(
  payload: TxPayload<AccountId>
) -> Result<Vec<u8>, CreateTransactionErr>;
```

`host_create_transaction` has no production consumers, so the shape changes in place.

The codec on the wire (JAM codec) has no native generics — `TxPayload<Signer>` is type-level shorthand for one concrete encoding per call site, not a generic struct on the wire.

The `CheckGenesis` extension is still expected to appear in `extensions` since the chain's `InheritedImplication` includes it; the root `genesis_hash` and `CheckGenesis.additional_signed` will carry the same value. Signers MAY cross-check the two and reject a payload whose root `genesis_hash` disagrees with the `CheckGenesis` extension, but the chain-identification path uses the root field only.

### Accounts Protocol

Today `SsoMessageContent` has no `create_transaction` mirror. Add one pair, covering only the product-account variant — `host_create_transaction_with_legacy_account` is handled entirely by the Host (which already knows the user's imported legacy accounts) and is never forwarded to the Account Holder, so no AP mirror is needed for it.

```rust
enum SsoMessageContent {
  // ... existing variants unchanged ...

  /// Mirrors host_create_transaction.
  CreateTransactionRequest {
    payload: VersionedTxPayload<ProductAccountId>,
  },
  CreateTransactionResponse {
    responding_to: SsoSessionRequestId,
    signed_transaction: BSResult<SignedTransaction, String>,
  }
}

enum VersionedTxPayload<Signer> {
  V1(TxPayload<Signer>)
}

type SignedTransaction = Vec<u8>;
```

The Accounts Protocol retains `VersionedTxPayload` because, unlike host API actions, AP messages are not individually versioned per call — `SsoMessageContent` is one flat enum, so the version envelope has to live on the payload itself for the AP to evolve `TxPayload` independently of the rest of the message set.

On receipt, the Account Holder reads `payload.signer: ProductAccountId`, picks the chain identified by `payload.genesis_hash`, derives signing context against that runtime, presents the transaction, and on approval returns the encoded signed transaction. The Host maps the response back to `Result<Vec<u8>, CreateTransactionErr>`.

## Drawbacks

- **AH must fetch metadata for any chain a product transacts on.** Already true in practice — the AH needs metadata for its own native flows.
- **No product-supplied `best_block_height`.** Products that want to pin mortality to a specific observed block must encode it inside `extensions` (the supported path anyway).

## Alternatives

- **Make `context` optional / drop only `metadata`.** Doesn't address the security concern (products can still ship a bogus blob the signer might trust).
- **Introduce a `V2` payload.** Unnecessary — `TxPayload` has no production consumers; the shape changes in place.
- **Keep `signer: Option<str>` + separate `account_id`.** Dead field on one variant, stringification on the other, no type guarantee at the call site.
- **`enum Signer { Product, Legacy }` instead of a generic.** Collapses the two variants back into one and forces a runtime tag check on a statically-dispatched flow.
- **Recover the chain from `CheckGenesis.additional_signed` instead of an explicit field.** Works only because that extension's SCALE encoding happens to be identity; couples chain identification to a single extension's wire shape and forces signers to lift one extension's payload before the rest can even be decoded.

## Prior art

- Polkadot-API offline signer proposal — [polkadot-js/api#6213](https://github.com/polkadot-js/api/issues/6213) — origin of `TxPayloadContext`; assumes an offline signer that does not match this topology.
- RFC-0010 (W3S Allowance) — established the pattern of co-documenting a TrUAPI call with its Accounts Protocol companion.
