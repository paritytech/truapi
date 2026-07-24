# Move Bulletin preimage submission into the Rust core

## TL;DR

Preimage submission to the Bulletin chain now happens **entirely inside the Rust
core** (`truapi-server`). The core builds, signs, and submits the
`TransactionStorage.store` extrinsic itself, routing the chain traffic through
the host's existing `chain.connect` byte-pipe — the same transport products
already use for any chain access.

Previously the core handed the host a _signing capability_ and the host built
and submitted the transaction with PAPI. That seam is gone. As a result:

- The wallet-delegated **allowance secret never leaves the core** (it no longer
  crosses the host / FFI boundary as a signer handle).
- The host's only remaining preimage job is **content lookup** (Helia / IPFS).
- All hosts (web, and later mobile over uniffi) get identical submission
  behaviour for free, because the logic lives in the shared core rather than in
  each host's JS/native code.

This issue explains the moving parts and, most importantly, **the messages
exchanged** between the product, the Rust core, the host, and the network.

---

## Why

The base branch already routes every chain call through one host callback,
`chain.connect(genesisHash)`, which returns a JSON-RPC byte pipe. Preimage
submission was the one chain write that did _not_ use it: instead the core
derived an allowance signer and passed a `sign(payload)` closure out to the
host, which reconstructed a PAPI transaction and submitted it. That meant:

- Every host reimplemented extrinsic construction/submission (and dotli pulled
  `@novasamatech` + PAPI signer deps back in to do it).
- The allowance secret's signing capability crossed the worker / FFI boundary.
- Submission behaviour (nonce, mortality, dispatch-error handling, retries)
  diverged per host.

Folding it into the core removes all three.

---

## Before vs after: who owns what

The topology is unchanged — the core still reaches the network only through the
host — what changes is **ownership**. Each box below lists what that side owns;
every arrow crossing the vertical boundary is labelled with exactly what
crosses it.

Before, the *secret bytes* were core-owned, but the *capability to use them* —
and everything that decides what gets signed — was host-owned:

```
BEFORE
         core-owned  (WASM worker)          host-owned  (JS main thread)
  +------------------------------+   ||   +------------------------------+
  | Rust core                    |   ||   | dotli (PAPI)                 |
  |                              |   ||   |                              |
  | owns:                        |   ||   | owns:                        |
  |  - allowance secret (64 B)   |   ||   |  - signer handle             |
  |  - sign() execution          |   ||   |     { publicKey, sign() }    |
  |                              |   ||   |  - tx builder (PAPI)         |
  |                              |   ||   |  - decides WHAT is signed    |
  |                              |   ||   |  - submission + watch        |
  +------------------------------+   ||   +------------------------------+

  crosses ||:  core --[ signer handle ]--> host   (a live signing capability)
               core <--[ sign(payload) ]-- host   (one request per tx)
               core --[ signature      ]--> host --> Bulletin
```

After, everything signing-related sits on the core side; the only thing that
crosses the boundary during submission is opaque JSON-RPC text:

```
AFTER
         core-owned  (WASM worker)          host-owned  (JS main thread)
  +------------------------------+   ||   +------------------------------+
  | Rust core                    |   ||   | dotli                        |
  |                              |   ||   |                              |
  | owns:                        |   ||   | owns:                        |
  |  - allowance secret (64 B)   |   ||   |  - permission/confirm modals |
  |  - signer (crate-private,    |   ||   |  - chain.connect byte pipe   |
  |     never exported)          |   ||   |     (forwards verbatim)      |
  |  - tx builder (subxt)        |   ||   |  - lookupPreimage backend    |
  |  - decides WHAT is signed    |   ||   |     (Helia / IPFS)           |
  |  - submission + watch        |   ||   |                              |
  +------------------------------+   ||   +------------------------------+

  crosses ||:  core --[ JSON-RPC request  ]--> host --> Bulletin
               core <--[ JSON-RPC response ]-- host
               opaque strings only: no key, no capability, no sign requests
```

The security consequence: before, host code held a live capability and chose
the payloads it signed; a compromised host could sign arbitrary Bulletin
transactions with the user's allowance. After, the host can at worst drop or
corrupt bytes — a lie in either direction produces a transaction the real
chain rejects, never a signature over attacker-chosen content.

---

## Actors and responsibilities

| Actor                            | Runs where                                      | Owns                                                                     |
| -------------------------------- | ----------------------------------------------- | ------------------------------------------------------------------------ |
| **Product**                      | sandboxed iframe / app                          | speaks TrUAPI; never sees chains or keys                                 |
| **Rust core** (`truapi-server`)  | WASM in a Web Worker (web); native lib (mobile) | wire protocol, tx build + sign, submit + watch, allowance key            |
| **Host** (dotli / mobile shell)  | main thread / OS shell                          | user modals, `chain.connect` transport, `lookupPreimage` content backend |
| **Wallet** (paired signing host) | phone                                           | allocates the Bulletin allowance key over SSO                            |
| **Network**                      | —                                               | Bulletin chain (writes) + People chain (allowance allocation / SSO)      |

---

## Message flow: preimage submission (happy path)

This is the core of the change. The product calls one wire method,
`preimage.submit(value)`; everything below it is new in-core work.

The columns are ownership lanes. A boundary glyph shows what crosses it on that
row: `>` left-to-right, `<` right-to-left, `:` nothing. Everything in the CORE
lane stays in the WASM worker unless a glyph carries it out.

```
 PRODUCT  | CORE (holds keys, builds+signs)    : HOST (modals+pipe) : NET
 ---------+------------------------------------+--------------------+---
 submit   > receive                            :                    :
          | -- gate (unchanged) --             :                    :
          | remotePermission                   > prompt user        :
          | confirmUserAction                  > prompt user        :
          | -- allowance key (cached) --       :                    :
          | requestResourceAllocation          > relay SSO          > wlt
          | slotAccountKey                     < passthrough        < key
          | [[ 64-B secret stays in core ]]    :                    :
          | -- build + submit via pipe --      :                    :
          | chain.connect(bulletin)            > open pipe          :
          | chainHead_v1_follow                > forward            > N
          | Metadata_metadata_at_version       > forward            > N
          | AccountNonceApi_account_nonce      > forward            > N
          | [[ build + SIGN store(value) ]]    :                    :
          | validate_transaction (dry-run)     > forward            > N
          | transaction_v1_broadcast           > forward            > N
          | chainHead_v1_body (watch)          > forward            > N
          | chainHead_v1_storage(Events)       > forward            > N
       key< return blake2_256(value)           :                    :
          | [[ prime lookup cache ]]           :                    :

 boundary glyphs:  >  message crosses left->right     <  crosses right->left
                   :  boundary, no message on this row
 CORE:HOST is the WASM-worker edge; [[..]] work never leaves the core; NET is
 the Bulletin chain, reached only through the host's pipe (wlt = paired wallet)
```

Key points on this flow:

- The **gate** (permission + confirmation) is unchanged from the base protocol;
  it still guards the write.
- The **dry-run** is load-bearing: `transaction_v1_broadcast` is
  spec-guaranteed to be _silent_ on invalid transactions, so without a
  `validate_transaction` call the core could never distinguish "rejected" from
  "still pending" and would only ever see a timeout.
- Inclusion is confirmed by **matching the extrinsic hash in a block body**,
  then the **dispatch outcome** is read from `System.Events` — matching the
  fidelity the old PAPI `signSubmitAndWatch` path had.

---

## Message flow: allowance key (the one secret)

The allowance key is a wallet-delegated, scoped signing capability. The wallet
owns *minting* it; the core owns *holding and using* it; the host only relays
the SSO messages and never sees the resulting secret used. It is allocated once
per (session, product), cached in the core, and used only to sign the `store`
call.

```
 CORE (holds + uses the key)            : HOST (relays SSO)  : WALLET (mints)
 ---------------------------------------+--------------------+---
 -- first submit for this product --    :                    :
 requestResourceAllocation              > relay (Ignore)     > allocate
 slotAccountKey                         < passthrough        < { key }
 [[ store in memory + persisted ]]      :                    :
 [[ used only to sign store() calls ]]  :                    :
                                        :                    :
 -- later: allowance is exhausted --    :                    :
 [[ evict the cached key ]]             :                    :
 requestResourceAllocation              > relay (Increase)   > mint fresh,
 slotAccountKey                         < passthrough        < larger one
 [[ retry the submit exactly once ]]    :                    :

 >  crosses core -> host -> wallet      <  the reply crossing back
 the key lands in CORE and is used there; the host never sees it signing
```

The retry is bounded: the core refreshes the allowance and retries **at most
once**, and only when the failure is a typed "allowance rejected" signal (from
the dry-run or from a `TransactionStorage` authorization error in the events) —
never on transport errors or nonce races.

---

## Message flow: lookup (unchanged owner, new integrity check)

The *content backend* is host-owned (the host chooses Helia P2P or an IPFS
gateway); the *cache* and the *integrity gate* are core-owned. So even though
the bytes originate on the host side, the core decides whether they reach the
product.

```
 CORE (cache + integrity gate)            : HOST (content backend)
 -----------------------------------------+-----------------------
 lookup_subscribe(key) from product       :
                                          :
 cache hit (primed by in-core submit):    :
   emit value once, keep open             :
                                          :
 cache miss:                              :
   lookupPreimage(key)                    > poll Helia / IPFS
   value | miss                           < host-owned bytes
   [[ gate: blake2_256(value)==key? ]]    :
   match: forward   mismatch: -> miss     :

 >  request crosses into the host      <  host's bytes cross back
 the backend is host-owned; the cache + integrity gate are core-owned,
 so the core decides what reaches the product (a mismatch is warn-logged)
```

A host that returns bytes not matching the requested key can no longer feed a
product forged content: the core's integrity gate downgrades the mismatch to a
miss before it reaches the product.

---

## Failure handling and the inclusion watch

The submit flow returns a typed error that maps to a stable reason string on the
wire (`PreimageSubmitError::Unknown { reason }` — the product wire is
unchanged). The decision the core makes at each failure:

```
 dry-run: validate_transaction
    |
    +-- Valid ................................. broadcast + watch  (see below)
    +-- Invalid::Payment / Custom / BadSigner . allowance rejected -> refresh + retry once
    +-- Invalid::Future / Stale ............... nonce race         (no retry)
    +-- other Invalid ......................... invalid            (no retry)

 broadcast + watch: included?
    |
    +-- ExtrinsicSuccess ...................... return the preimage key
    +-- TransactionStorage auth error ......... allowance rejected -> refresh + retry once
    +-- other dispatch error .................. dispatch failed
    +-- 120s elapsed / follow stopped ......... broadcast, inclusion unverified
```

"Allowance rejected" is the only outcome that refreshes the key
(`onExisting=Increase`) and retries — at most once, in both the dry-run and the
dispatch branches.

The inclusion watch is a single event loop over one ephemeral chainHead follow.
It fetches a block body only after the allowance account's nonce is seen to
advance (so it does not download every block), matches by extrinsic hash, and
releases pins as it goes. Because the follow is dropped when the 120 s budget or
a cancellation fires, its pins and connection lease are released automatically.

---

## Security invariants

Because the chain provider is an untrusted byte pipe (especially in RPC-gateway
mode), the core enforces four invariants so a hostile or buggy provider cannot
subvert the allowance key:

1. **Genesis is config-pinned.** The signed payload's genesis hash always comes
   from host _configuration_, never from provider-echoed chain data — so a
   provider cannot redirect the allowance-key signature to a different chain.
   Every other provider-supplied input (spec/tx version, nonce, mortality
   anchor, metadata) is then fail-closed: lying about it yields a signature the
   real chain rejects, never one valid elsewhere.
2. **Call is pinned.** Name resolution of `TransactionStorage.store` is
   hard-asserted against audited pallet/call indices, and the metadata-built
   call bytes are checked byte-for-byte against a canonically built copy — so
   provider metadata cannot make the key sign a different call.
3. **Signer is confined.** The only allowance-key -> signer conversion is
   crate-private to the bulletin module; the signer is transient, never stored,
   never logged, and the key type is zeroized on drop.
4. **Lookup is content-addressed.** Host-returned bytes are verified against the
   requested key before reaching the product.

A crafted `NewBlock` parent link (self-referential or cyclic) is also guarded in
the inclusion watch so the provider cannot spin the worker.

---

## What changed (high-level map)

- `truapi-server/src/host_logic/extrinsic.rs` — offline subxt assembler:
  config-pinned `SubstrateConfig`, sr25519 signer, metadata / validity / events
  / header decoders.
- `truapi-server/src/host_logic/bulletin.rs` — `store{data}` build + sign, call
  pinning, byte-level call-data encoder.
- `truapi-server/src/runtime/bulletin_rpc.rs` — the submit flow (follow ->
  metadata -> nonce -> dry-run -> broadcast -> watch -> events) and its typed
  errors.
- `truapi-server/src/runtime.rs` — `Preimage::submit` ordering + refresh/retry;
  `lookup_subscribe` cache + integrity check.
- `truapi-platform` — `PreimageHost` keeps only `lookupPreimage`;
  `BulletinAllowanceKey` is zeroized on drop; configs gain an optional Bulletin
  genesis hash.
- Host TS (`@parity/truapi-host`) + dotli — the signer bridge is removed; dotli
  keeps only lookup, threads the Bulletin genesis into its runtime config, and
  routes the Bulletin genesis through `chain.connect` on both backends.

The **product-facing wire is unchanged** — `preimage.submit` /
`preimage.lookup_subscribe` keep their wire ids and shapes, so products need no
changes.

---

## Verification

- Rust: build / fmt / clippy (`-D warnings`) / tests all green; wasm32 compiles;
  `cargo deny` licenses ok. New unit tests cover extrinsic construction against
  real Bulletin metadata, genesis-binding, call pinning, the extension-encoding
  rules, and the inclusion-watch cycle guard.
- Host TS: `tsc` + `bun` tests green. WASM bundle grows ~0.5 MB (subxt offline).
- **Manual / e2e gates** (network + signing-host CLI, not in CI): the live-chain
  dry-run encoding proof and `make e2e-dotli` preimage flow.

## Follow-ups

- Signing-host (mobile local-key) role: derive the Bulletin allowance key from
  root entropy so submission works there too — everything downstream of the
  allowance-key fetch is already shared.
- Reuse `host_logic/extrinsic.rs` for the local `create_transaction` path.
