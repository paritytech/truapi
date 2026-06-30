# RFC-0022: Remove `dotNsIdentifier` from default account access

|                 |                                                                       |
| --------------- | --------------------------------------------------------------------- |
| **RFC Number**  | 22                                                                    |
| **Start Date**  | 2026-06-30                                                            |
| **Description** | Default account, signing, and statement-store calls address only the caller's own accounts, by `derivationIndex`. |
| **Authors**     | Valentin Fernandez                                                    |

## Summary

The account, signing, and statement-store methods that operate on a product account today take a
`ProductAccountId { dotNsIdentifier, derivationIndex }`. The `dotNsIdentifier` is dropped from these
calls: each resolves against the caller's own dotNS domain, and products pass only `derivationIndex`.

```typescript
// Before
await truapi.account.getAccount({
  productAccountId: { dotNsIdentifier: "truapi-playground.dot", derivationIndex: 0 },
});

// After
await truapi.account.getAccount({ derivationIndex: 0 });
```

This RFC covers seven call sites:

- [`getAccount()`](https://paritytech.github.io/truapi/v/main/method/Account/get_account)
- [`getAccountAlias()`](https://paritytech.github.io/truapi/v/main/method/Account/get_account_alias)
- [`createAccountProof()`](https://paritytech.github.io/truapi/v/main/method/Account/create_account_proof)
- [`createTransaction()`](https://paritytech.github.io/truapi/v/main/method/Signing/create_transaction)
- [`signRaw()`](https://paritytech.github.io/truapi/v/main/method/Signing/sign_raw)
- [`signPayload()`](https://paritytech.github.io/truapi/v/main/method/Signing/sign_payload)
- [`statementStore.createProof()`](https://paritytech.github.io/truapi/v/main/method/StatementStore/create_proof)

`ProductAccountId` itself is retained: a follow-up RFC reuses it as the explicit target identifier for
opt-in cross-product access (see [Future Directions](#future-directions-and-related-material)).

## Motivation

The host already knows the calling product's dotNS domain, so making the product pass `dotNsIdentifier`
is redundant — the host can resolve it on its own from the authenticated caller.

The parameter also has no useful value other than the caller's own domain. The host only ever acts on
the domain the product is currently running under: if a product passes any other dotNS address, the
host rejects the call as invalid. For example, a product running on `truapi-playground.dot` that calls
`signRaw()` with a different dotNS address (say `another-product.dot`) gets back an *invalid* response
— it cannot reach the other product's account this way. So `dotNsIdentifier` can only ever echo the
caller's own domain (redundant) or name a domain the host refuses (a dead value).

Either way the parameter does not belong on the default call surface. Products should address only
their own accounts, by `derivationIndex`, and the host — the security boundary — should be the sole
authority on which domain a call resolves against, rather than taking it from the caller at all.

Split out from [#222](https://github.com/paritytech/truapi/issues/222); tracked in
[#243](https://github.com/paritytech/truapi/issues/243).

## Stakeholders

- **Product developers** — construct slimmer calls with no domain field to supply or get wrong; calls
  address the product's own accounts.
- **Host developers** — resolve the account's domain from the authenticated caller instead of trusting
  a parameter on the request.

## Explanation

`ProductAccountId` is **unchanged** — it stays available for the explicit external-access surface a
follow-up RFC introduces:

```rust
struct ProductAccountId {
  dot_ns_identifier: String,
  derivation_index: u32,
}
```

Each of the seven request types drops its `ProductAccountId`-typed field and carries a bare
`derivation_index: u32` instead. The host fills in the caller's own domain.

Before:

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

After:

```rust
struct HostAccountGetRequest      { derivation_index: u32 }
struct HostAccountGetAliasRequest { derivation_index: u32 }
struct HostAccountCreateProofRequest {
  derivation_index: u32,
  ring_location: RingLocation,
  context: Vec<u8>,
}
struct ProductAccountTxPayload {
  derivation_index: u32,
  genesis_hash: GenesisHash,
  call_data: Vec<u8>,
  extensions: Vec<TxPayloadExtension>,
  tx_ext_version: u8,
}
struct HostSignRawRequest     { derivation_index: u32, payload: RawPayload }
struct HostSignPayloadRequest { derivation_index: u32, payload: HostSignPayloadData }
struct RemoteStatementStoreCreateProofRequest {
  derivation_index: u32,
  statement: Statement,
}
```

In TypeScript the nested identifier object collapses to a single field:

```typescript
await truapi.account.getAccount({ derivationIndex: 0 });
await truapi.account.getAccountAlias({ derivationIndex: 0 });
await truapi.account.createAccountProof({ derivationIndex: 0, ringLocation, context: "0x" });
await truapi.signing.createTransaction({ derivationIndex: 0, genesisHash, callData, extensions: [], txExtVersion: 0 });
await truapi.signing.signRaw({ derivationIndex: 0, payload });
await truapi.signing.signPayload({ derivationIndex: 0, payload });
await truapi.statementStore.createProof({ derivationIndex: 0, statement });
```

The change is made in place on `v01`; no `v02` of these messages is introduced. Their wire IDs are
unchanged — only the request body shape changes. The `*WithLegacyAccount` signing variants are
unaffected: they identify a legacy account by raw `AccountId`, never by `dotNsIdentifier`.

`statementStore.createProofAuthorized()` already takes only the statement (it uses a pre-allocated
allowance account) and so needs no change. The deprecated `statementStore.createProof()` is the
statement-store call still carrying `ProductAccountId`, and is covered here for consistency.

## Drawbacks

These default methods only ever address the caller's own accounts. Some products may have a legitimate
reason to read, and possibly sign with, *another* product's account — a capability the host does not
expose today, since it rejects any domain other than the caller's. Rather than overloading
`dotNsIdentifier` to provide it, that capability is introduced explicitly, behind a permission, in the
follow-up RFC (see [Future Directions](#future-directions-and-related-material)).

## Testing, Security, and Privacy

Removing `dotNsIdentifier` makes the host the sole authority on which domain a call resolves against.
The host already rejects any domain other than the caller's, so a product cannot reach another
product's account today; dropping the field removes that redundant, rejected input entirely rather than
relying on the host to validate a caller-supplied domain on every call. The trust boundary becomes
structural — there is no domain field to validate or get wrong.

## Performance, Ergonomics, and Compatibility

### Ergonomics

Calls are smaller and harder to misuse — there is no domain string to get wrong, and the common case
(a product's own account) needs only an index.

### Compatibility

This is a breaking wire change to seven `v01` request bodies, and the protocol gives it **no version
signal**. Each message is wrapped in a `versioned_type!` envelope whose SCALE discriminant byte is the
version (`V1` = codec index `0`), but every one of the protocol's envelopes is currently single-variant
`V1` — there is no live multi-version support and no cross-version conversion anywhere in the tree. The
handshake negotiates only the SCALE *codec* version, not the API version. So editing the body of a
`V1` message keeps the version byte at `0` while changing the bytes that follow it; because SCALE is
positional and not self-describing, an un-upgraded host reads "V1" and silently misdecodes the new
layout (the leading `u32` is consumed as a string-length prefix). There is no clean failure.

The change is therefore safe **only while the host and product SDK ship together** — i.e. while the
v0.1 wire is not frozen against independently-deployed hosts. That is the repository's current posture:
`v01` is the single, still-evolving wire, breaking changes land in it in place (see
[RFC-0020](0020-create-transaction.md)), and the `versioned_type!` machinery — though fully scaffolded
— has never been exercised with a second variant. This RFC follows that posture. Some of these methods
do have a live consumer (the playground calls `signRaw` and `createProof`); those call sites are
updated in the same change.

Once the v0.1 wire is frozen and hosts deploy independently of the SDK, a breaking change like this one
must instead add a `V2` variant (`{ V1 => v01::X, V2 => v02::X }`) with hand-written
`FromLatest`/`IntoLatest`, so the version byte distinguishes the shapes (an old host rejects `V2`
cleanly instead of corrupting) and a dual-version host can bridge both. Adopting that for these seven
messages alone — while the rest of the protocol stays single-variant `V1` — would buy no real safety;
it is a protocol-wide discipline to take on at the freeze, tracked separately from this RFC.

## Prior Art and References

- [RFC-0020](0020-create-transaction.md) — removed a field from a signing request body in place on
  `v01`; establishes the in-place-change precedent followed here.
- [#222](https://github.com/paritytech/truapi/issues/222) — parent issue this is split out from.
- [#243](https://github.com/paritytech/truapi/issues/243) — tracking issue for this RFC and its
  follow-up.

## Unresolved Questions

- **In-place vs versioned migration.** This RFC changes the seven `v01` bodies in place, which is sound
  only while hosts and the SDK ship together (see [Compatibility](#compatibility)). If v0.1 is frozen
  against independently-deployed hosts before this lands, it must move to a `V2` envelope instead — and
  that decision is really protocol-wide (today all 165 envelopes are single-variant `V1`), not specific
  to these seven messages.
- **Replacement field name.** The replacement is `derivation_index: u32` uniformly. An alternative is
  to keep the role-indicating names each call uses today (`signer` on `createTransaction`, `account` on
  the signing calls) but retype them to `u32`. Uniform `derivation_index` is proposed for consistency.

## Future Directions and Related Material

A follow-up RFC introduces cross-product account access explicitly, rather than exposing it by
overloading `dotNsIdentifier`:

- A new `ExternalAccount` variant on `RemotePermission`, so cross-product access is grantable and
  deniable by the user like the other remote permissions.
- Explicit external methods that take a full `ProductAccountId` (the type retained by this RFC) and
  require the `ExternalAccount` grant: `getExternalAccount()`, `getExternalAccountAlias()`,
  `createExternalAccountProof()`.
- Whether to allow signing with an external account at all, and if so under what permission and
  prompting, is left to that RFC.
