# D - Crypto & dependency foundation

> Part of the [host-contract & core-impl spec](index.md).

`truapi-server` has no crypto deps today (grep for `schnorrkel`/`blake2` in the workspace `Cargo.toml`s
returns nothing). This doc specifies the deps, the one fully-portable algorithm (`get_account`
derivation), the proof scheme that must be extracted from source, and the golden-vector harness that
gates all of it.

Implementation shape: use `~/github/useragent-kit` as implementation precedent only, especially
`crates/host-wallet/src/crypto.rs`, `crates/host-encoding/src/statement_store.rs`, and
`crates/host-wallet/tests/pinned_vectors.rs`: keep pure protocol crypto in a narrow WASM-safe
module/crate, use existing Rust crypto crates for primitives, provide deterministic helpers for vectors
(for example fixed AES-GCM nonces), and test the same vectors on native and wasm. Do **not** hand-roll
crypto primitives. `useragent-kit` has broader scope and is not a protocol source of truth for TrUAPI
SSO bytes; consult it when we need examples of similar Novasama-codepath migrations to Rust.

## D0. Tier 1.5 crypto/vector gate

Before Tier 2 pairing I/O starts, add a focused crypto module/crate and pinned vectors for:

- `get_account` public HDKD derivation.
- Statement signing payload encoding and sr25519 proof sign/verify.
- QR handshake SCALE encoding and deeplink bytes.
- P-256 ECDH -> HKDF -> AES-GCM encrypt/decrypt with fixed inputs/nonces.
- Topic/channel/session-id derivation.

This gate produces tests, not the full `request_login` flow. Tier 2 starts only after those tests pass on
native and `wasm32-unknown-unknown`.

`useragent-kit` is a dependency/style reference, not a protocol source of truth. Its scope is broader and
may include a different pairing dialect. The vector capture must compare against the pinned
`@novasamatech/host-papp` package and the wallet peer.

## D1. Dependencies (must build for `wasm32-unknown-unknown`)

Two crypto layers, with constants read from the current dotli-installed `@novasamatech` packages and
locked by the Tier 1.5 vectors ([H](<H - sso-pairing-protocol.md>)):

- **Statements + accounts (sr25519):** `schnorrkel` for statement-proof sign/verify and `get_account`
  HDKD soft derivation. SCALE (`parity-scale-codec`, already a dep) for chain-code + statement encoding.
- **The SSO channel (P-256 ECIES):** NIST P-256 ECDH + HKDF-SHA256 + AES-GCM. RustCrypto stack:
  `p256` (key agreement), `hkdf` + `sha2`, `aes-gcm`. Current dotli uses HKDF-SHA256 with empty
  `salt`/`info`, `dkLen=32`, AES-GCM with a random 12-byte nonce prepended to ciphertext+tag.
- **Topic / channel / session-id derivation:** keyed **BLAKE2b-256** (`blake2`, keyed mode). Current
  dotli's `khash(secret, message)` is `blake2b(message, { dkLen: 32, key: secret })`; it is used for the
  handshake topic, session ids, and request/response channels.
- `bandersnatch_vrfs` / `ark-*`: only if `create_account_proof` / ring-VRF moves in-core
  ([E3](<E - open-questions.md>)); the alias itself is proxied to the wallet.

Add behind the existing wasm build and confirm `make wasm` + `cargo build -p truapi-server
--target wasm32-unknown-unknown` succeed (schnorrkel + p256 need `getrandom`'s `js` feature on wasm).

## D2. `get_account` product-account derivation: fully specified, portable now

This is pure (no secret) and is the cleanest in-core port. Current dotli's reference is
`~/github/dotli/packages/auth/src/account.ts` using `@scure/sr25519` `HDKD.publicSoft`. Reproduce
exactly:

```
 root_pk = SessionInfo.public_key      (= session.remoteAccount.accountId, 32 bytes)
 junctions = ["product", dot_ns_identifier, String(derivation_index)]   // always these 3, in order

   root_pk --publicSoft(cc("product"))--> pk1 --publicSoft(cc(dotNS))--> pk2 --publicSoft(cc(idxStr))--> product_pk
   (left fold: junctions.reduce((pk, j) => HDKD.publicSoft(pk, cc(j)), root_pk))
```

Chain code `cc(code)`: a **32-byte buffer, zero-initialised**, after encoding/compression:

```
  if code matches /^\d+$/: encoded = SCALE u64(BigInt(code))
  else:                    encoded = SCALE str(code)  (compact-length byte + UTF-8)

  if encoded.len > 32:     bytes = blake2b(encoded, dkLen=32)
  else:                    bytes[0..encoded.len] = encoded; remaining bytes stay zero
```

So `"product"` and the dotNS go through the `str` branch; `String(derivation_index)` (always numeric)
goes through the `u64` branch. In Rust: build the `[u8;32]`, use schnorrkel public soft derivation
(`PublicKey` soft-derive with that chain code), fold over the three junctions. This must be vector-tested
against current dotli because `u32` would produce the wrong product account for every numeric derivation
index.

**Note:** product accounts have no per-product secret today; the matching `deriveProductSecretKey`
(`HDKD.secretSoft`) is intentionally disabled in dotli (`account.ts:28-42`) because derived accounts have
no People-chain allowance yet. The core derives the product **public** key for `get_account` but signs
statement proofs with the **session statement-store** key (`ss_secret`, see D3), not a product key.

**Acceptance:** golden vectors: for fixed `(root_pk, dotNS, index)` tuples, the Rust output equals
`HDKD.publicSoft`-folded output byte-for-byte.

## D3. Statement-proof signing

Current dotli signs with `createSr25519Prover(ssSecret).generateMessageProof(statement)` in
`~/github/dotli/packages/ui/src/container.ts` (`handleStatementStoreCreateProof`). `ssSecret` is the
core's session statement-store sr25519 secret; the output is
`StatementProof::Sr25519 { signature:[u8;64], signer:[u8;32] }` with `signer` = the session statement
pubkey (allowance-granted by the wallet).

The **message framing is known** from current dotli packages: `@novasamatech/sdk-statement` encodes the
unsigned `Statement`, strips the leading compact length prefix from the encoded bytes, and signs the
remaining SCALE payload. `@novasamatech/statement-store` then sets
`proof = sr25519 { signature, signer }`. The secret passed to the signer is the 64-byte
Ed25519-expanded/scure HDKD secret returned by `createSr25519Secret`; the package's substrate sr25519
wrapper calls `@polkadot-labs/schnorrkel-wasm` `sr25519_sign(publicKey, secret, message)` with no
explicit context parameter.

**Acceptance:** for a fixed `(ssSecret, statement)`, the Rust `{signature, signer}` equals
`generateMessageProof`'s output (signatures are randomised, so verify with the public key + compare
`signer`, or seed the RNG when capturing vectors).

## D4. SSO channel crypto (host-papp + wallet-peer vectors)

Current dotli main uses host-papp 0.8.6 SSO V2. The vector gate must cover:

- `VersionedHandshakeProposal::V2` SCALE with `Device { statementAccountId, encryptionPublicKey }`.
- Metadata entries for `HostName`, `HostVersion`, `HostIcon`, `PlatformType`, and `PlatformVersion`.
- V2 pairing topic and channel:
  `blake2b256_keyed(encryptionPublicKey || "topic", key=statementAccountId)` and the same for
  `"channel"`.
- `VersionedHandshakeResponse` / encrypted V2 response variants (`Pending`, `Success`, `Failed`).
- `HandshakeSuccessV2` fields, especially `rootAccountId`, `identityAccountId`, `ssoEncPubKey`,
  `deviceEncPubKey`, and `rootEntropySource`.

The current dotli web path is explicit:

- `createEncrSecret(entropy)`: convert entropy to a mini-secret, zero-extend to a 48-byte seed, then
  `p256.keygen(seed).secretKey`.
- `getEncrPub(secret)`: uncompressed P-256 public key, 65 bytes.
- `createSharedSecret(secret, publicKey)`: `p256.getSharedSecret(secret, publicKey).slice(1, 33)`, i.e.
  the X coordinate from the uncompressed ECDH output.
- bootstrap decrypt: `createSharedSecret(core_encr_secret, wallet_tmp_key)`.
- steady-state shared secret: `createSharedSecret(core_encr_secret, shared_secret_derivation_key)`, where
  `shared_secret_derivation_key` is the SSO peer's persistent P-256 public key from the sensitive
  handshake response.
- encryption: `createEncryption(sharedSecret)` derives `aesKey = HKDF-SHA256(sharedSecret, salt=[], info=[], len=32)`;
  encrypt prepends a random 12-byte nonce to AES-GCM ciphertext+tag; decrypt splits the first 12 bytes as
  nonce.
- handshake topic: `khash(ss_public_key, encr_public_key || "topic")`.
- session id: `khash(sharedSecret, "session" || accountA.accountId || accountB.accountId || pinA || pinB)`,
  where each missing pin contributes the separator byte/string `"/"`.
- request/response channels: `khash(session_id, "request")` and `khash(session_id, "response")`.

The vector gate remains mandatory because it catches byte-order/SCALE/nonce mistakes, not because these
choices are still open.

## D5. Golden-vector harness

The `@novasamatech` packages are pinned in current dotli main at `0.8.6` for `host-api`, `host-papp`,
`statement-store`, and `storage-adapter` (`sdk-statement` remains `^0.6.0`). To capture vectors: use
`hosts/dotli` `origin/main` or a scratch workspace with the same package versions, then snapshot into a
Rust test fixture:

- `HDKD.publicSoft` fold outputs for several `(root, dotNS, index)` tuples (D2).
- `createSr25519Prover(secret).generateMessageProof(statement)` `{signature, signer}` for fixed inputs (D3).
- SSO V2 QR proposal SCALE/deeplink bytes for fixed public keys + host metadata entries.
- P-256 ECDH -> HKDF -> AES-GCM round-trip + topic/session-id derivation for fixed inputs (D4),
  cross-checked against the iOS `SsoTestData` fixtures and host-papp outputs.

Commit the vectors as test data and assert the Rust implementations reproduce them. This harness is the
acceptance gate for Tiers 1-3; do it before, not after, implementing the crypto.
