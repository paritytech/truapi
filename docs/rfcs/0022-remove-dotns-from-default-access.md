# RFC-0022: Optional `dotNsIdentifier` and permissioned external-account access

|                 |                                                                       |
| --------------- | --------------------------------------------------------------------- |
| **RFC Number**  | 22                                                                    |
| **Start Date**  | 2026-06-30                                                            |
| **Description** | Account, signing, and statement-store calls address the caller's own accounts by default, and a foreign dotNS identifier opts into permissioned cross-product access. |
| **Authors**     | Valentin Fernandez                                                    |

## Summary

The account, signing, and statement-store methods that operate on a product account take a
`ProductAccountId { dotNsIdentifier, derivationIndex }`. `dotNsIdentifier` is now **optional**. Omit it
and the call resolves against the caller's own dotNS domain, so the common case only needs a
`derivationIndex`.

```typescript
// Own account, the common case
await truapi.account.getAccount({ productAccountId: { derivationIndex: 0 } });

// Another product's account, requires the ExternalAccount permission
await truapi.account.getAccount({
  productAccountId: { dotNsIdentifier: "another-product.dot", derivationIndex: 0 },
});
```

Supplying a `dotNsIdentifier` that names a different product is cross-product access, gated behind a
new `ExternalAccount` remote permission. When the field is omitted the host resolves the account's
domain from the authenticated caller. When it names a foreign domain the host accepts or rejects the
call based on whether the user granted `ExternalAccount`.

This RFC covers seven call sites:

- [`getAccount()`](https://paritytech.github.io/truapi/v/main/method/Account/get_account)
- [`getAccountAlias()`](https://paritytech.github.io/truapi/v/main/method/Account/get_account_alias)
- [`createAccountProof()`](https://paritytech.github.io/truapi/v/main/method/Account/create_account_proof)
- [`createTransaction()`](https://paritytech.github.io/truapi/v/main/method/Signing/create_transaction)
- [`signRaw()`](https://paritytech.github.io/truapi/v/main/method/Signing/sign_raw)
- [`signPayload()`](https://paritytech.github.io/truapi/v/main/method/Signing/sign_payload)
- [`statementStore.createProof()`](https://paritytech.github.io/truapi/v/main/method/StatementStore/create_proof)

## Motivation

For the common case, a product addressing its own accounts, `dotNsIdentifier` is redundant. The host
already knows the calling product's dotNS domain and can resolve it from the authenticated caller.
Making the field optional lets that case carry only a `derivationIndex`.

The field still serves a real purpose, though. Some products have a legitimate reason to read, and
possibly sign with, *another* product's account. That capability should be explicit and consented, not
an unguarded consequence of an always-present parameter. So instead of dropping `dotNsIdentifier`
entirely, which would remove the capability, or leaving it mandatory, which offers no isolation, it
becomes an optional opt-in: absent for own-product access, present and permissioned for cross-product
access.

Split out from [#222](https://github.com/paritytech/truapi/issues/222), tracked in
[#243](https://github.com/paritytech/truapi/issues/243).

## Stakeholders

- **Product developers** omit the domain for their own accounts. To reach another product's account
  they supply a `dotNsIdentifier` and hold the `ExternalAccount` grant.
- **Host developers** resolve the domain from the authenticated caller when it is omitted, and enforce
  the `ExternalAccount` permission when a foreign domain is supplied.

## Explanation

`ProductAccountId` carries an optional domain:

```rust
struct ProductAccountId {
  dot_ns_identifier: Option<String>,
  derivation_index: u32,
}
```

Resolution rules:

- `dot_ns_identifier == None` resolves to the caller's own product.
- `dot_ns_identifier == Some(domain)` where `domain` is the caller's own also resolves to the caller's
  own product.
- `dot_ns_identifier == Some(domain)` where `domain` is a different product is cross-product access. It
  is permitted only when the user has granted the caller the `ExternalAccount` permission, otherwise
  the host rejects the call.

Each of the seven request types carries a `ProductAccountId`:

```rust
struct HostAccountGetRequest      { product_account_id: ProductAccountId }
struct HostAccountGetAliasRequest { product_account_id: ProductAccountId }
struct HostAccountCreateProofRequest {
  product_account_id: ProductAccountId,
  ring_location: RingLocation,
  context: Vec<u8>,
}
struct ProductAccountTxPayload {
  signer: ProductAccountId,
  genesis_hash: GenesisHash,
  call_data: Vec<u8>,
  extensions: Vec<TxPayloadExtension>,
  tx_ext_version: u8,
}
struct HostSignRawRequest     { account: ProductAccountId, payload: RawPayload }
struct HostSignPayloadRequest { account: ProductAccountId, payload: HostSignPayloadData }
struct RemoteStatementStoreCreateProofRequest {
  product_account_id: ProductAccountId,
  statement: Statement,
}
```

In TypeScript `dotNsIdentifier` is optional on the nested identifier, so own-account calls pass only
the index:

```typescript
await truapi.account.getAccount({ productAccountId: { derivationIndex: 0 } });
await truapi.account.getAccountAlias({ productAccountId: { derivationIndex: 0 } });
await truapi.account.createAccountProof({ productAccountId: { derivationIndex: 0 }, ringLocation, context: "0x" });
await truapi.signing.createTransaction({ signer: { derivationIndex: 0 }, genesisHash, callData, extensions: [], txExtVersion: 0 });
await truapi.signing.signRaw({ account: { derivationIndex: 0 }, payload });
await truapi.signing.signPayload({ account: { derivationIndex: 0 }, payload });
await truapi.statementStore.createProof({ productAccountId: { derivationIndex: 0 }, statement });
```

### `ExternalAccount` permission

A new `ExternalAccount` variant on `RemotePermission` governs cross-product access. Like `ChainSubmit`
and `StatementSubmit`, it is triggered implicitly by the business call. The first time a product issues
one of these calls with a foreign `dotNsIdentifier`, the host prompts for the grant, and the user's
decision persists. A product that only ever addresses its own accounts never sees the prompt.

The change is made in place on `v01`, with no `v02` of these messages. Their wire IDs are unchanged.
Only the request body shape changes, and `dotNsIdentifier` moves from a required `String` to an
optional one. The `*WithLegacyAccount` signing variants are unaffected, since they identify a legacy
account by raw `AccountId`, never by `dotNsIdentifier`.

`statementStore.createProofAuthorized()` already takes only the statement (it uses a pre-allocated
allowance account) and so needs no change. The deprecated `statementStore.createProof()` is the
statement-store call still carrying `ProductAccountId`, and is covered here for consistency.

## Drawbacks

Cross-product access adds a permission surface the host must enforce and the user must reason about.
The capability is opt-in on both sides, though: a product reaches another product's account only by
supplying a foreign domain, and only after the user grants `ExternalAccount`. Products that never opt
in are unaffected, and the default call surface stays minimal.

## Testing, Security, and Privacy

Cross-product access is only possible through an explicit foreign `dotNsIdentifier` combined with a
user-granted `ExternalAccount` permission. Own-account calls omit the domain entirely, so the host
resolves it from the authenticated caller and there is no caller-supplied domain to validate in the
common case. The host stays the sole authority on which domain a call resolves against, and the
permission prompt keeps the user in the loop before any product reads or signs with another product's
account.

## Performance, Ergonomics, and Compatibility

### Ergonomics

The common case, a product's own account, needs only an index, and there is no domain string to supply
or get wrong. Cross-product access is expressed by the presence of a domain plus a permission, so the
capability is discoverable instead of hidden behind an always-present field.

### Compatibility

This is a breaking wire change to seven `v01` request bodies, and the protocol gives it **no version
signal**. Each message is wrapped in a `versioned_type!` envelope whose SCALE discriminant byte is the
version (`V1` is codec index `0`), but every one of the protocol's envelopes is currently
single-variant `V1`. There is no live multi-version support and no cross-version conversion anywhere in
the tree. The handshake negotiates only the SCALE *codec* version, not the API version. So editing the
body of a `V1` message keeps the version byte at `0` while changing the bytes that follow it. Because
SCALE is positional and not self-describing, an un-upgraded host reads "V1" and silently misdecodes the
new layout. There is no clean failure.

The change is therefore safe only while the host and product SDK ship together, that is, while the
v0.1 wire is not frozen against independently-deployed hosts. That is the repository's current posture.
`v01` is the single, still-evolving wire, and breaking changes land in it in place (see
[RFC-0020](0020-create-transaction.md)). The `versioned_type!` machinery, though fully scaffolded, has
never been exercised with a second variant. This RFC follows that posture. Some of these methods do
have a live consumer (the playground calls `signRaw` and `createProof`), and those call sites are
updated in the same change. The PR stays a draft until the protocol-wide versioning strategy is
settled.

Once the v0.1 wire is frozen and hosts deploy independently of the SDK, a breaking change like this one
must instead add a `V2` variant (`{ V1 => v01::X, V2 => v02::X }`) with hand-written
`FromLatest`/`IntoLatest`, so the version byte distinguishes the shapes (an old host rejects `V2`
cleanly instead of corrupting) and a dual-version host can bridge both. Adopting that for these seven
messages alone, while the rest of the protocol stays single-variant `V1`, would buy no real safety. It
is a protocol-wide discipline to take on at the freeze, tracked separately from this RFC.

## Prior Art and References

- [RFC-0002](0002-permission-model.md), the permission model that `ExternalAccount` extends.
- [RFC-0020](0020-create-transaction.md), which removed a field from a signing request body in place on
  `v01` and establishes the in-place-change precedent followed here.
- [#222](https://github.com/paritytech/truapi/issues/222), the parent issue this is split out from.
- [#243](https://github.com/paritytech/truapi/issues/243), the tracking issue for this RFC.

## Unresolved Questions

- **In-place vs versioned migration.** This RFC changes the seven `v01` bodies in place, which is sound
  only while hosts and the SDK ship together (see [Compatibility](#compatibility)). If v0.1 is frozen
  against independently-deployed hosts before this lands, it must move to a `V2` envelope instead, and
  that decision is really protocol-wide (today all envelopes are single-variant `V1`), not specific to
  these seven messages.
