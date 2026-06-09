# TrUAPI host-contract additions & core-implementation spec

**Status:** draft for review · **Goal:** run dotli on the Rust core with current dotli feature parity and
no `@novasamatech` dependency, by defining the host primitives, core implementations, and package changes
that close the gap. "Feature parity" here means the handlers implemented in current dotli main
(`hosts/dotli` `origin/main` audited at `4611008` on 2026-06-09, with the older `~/github/dotli`
checkout at `85c9733` used only as historical implementation evidence); handlers dotli currently leaves
unimplemented stay out of scope until other hosts need them.

This is the high-level overview. Each detail doc is written so a senior Rust engineer can pick it up as
a ticket: exact signatures, file anchors, an implementation sketch, and acceptance criteria.

## Relationship to PR 104

PR 104 lands the shared-core **runtime substrate**: generated product clients, Rust dispatcher,
`truapi-platform` traits, WASM/UniFFI/native bridges, and the first dotli architecture notes. It
intentionally leaves account management, signing, statement-store, and preimage business logic
unsupported or bridged through transitional byte callbacks.

This spec is the follow-up implementation plan that turns that runtime substrate into the final shared
Rust core used by dotli:

- remove account-only session injection (`setActiveSession` / `clearActiveSession`) as a real lifecycle
  path;
- remove JS-owned account/signing/statement-store/raw preimage callback shims from the final dotli bridge;
- add the missing host primitives the core genuinely cannot perform itself (`PairingPresenter`,
  `SessionStore`, theme, notification id/cancel, preimage host content backend, allocation confirmation);
- implement current-dotli parity inside `truapi-server` so `@novasamatech/host-container`,
  `host-papp`, `statement-store`, `sdk-statement`, `host-api`, and `storage-adapter` can all disappear.

The older PR docs (`docs/design/dotli-rust-core-proposal.md` and
`docs/design/dotli-architecture-change.md`) describe the broad shared-core topology. Where they still
show `(D1*)` account/signing/statement callbacks or session handoff as transitional steps, this directory
is the more specific successor: those callbacks are removed once the core implementations in [B](<B - core-impls.md>)
land.

| Doc | Covers |
|---|---|
| [H - SSO pairing protocol](<H - sso-pairing-protocol.md>) | **Foundational.** The one transport (People-chain statement store) that pairing + signing + transaction construction + resource allocation + ring-VRF + statements all ride |
| [A - Host primitives](<A - host-primitives.md>) | The new SSO callback (QR presenter), runtime product config, and host-container parity surfaces |
| [B - Core implementations](<B - core-impls.md>) | The migrated wire methods, current deferred set, per-method tickets + diagrams |
| [C - Session contract](<C - session-contract.md>) | `SessionState` extension, persistence/restore, the JS package gap |
| [D - Crypto foundation](<D - crypto-foundation.md>) | wasm32 deps, the `get_account` derivation, the proof scheme, golden vectors |
| [E - Decision log / deferred questions](<E - open-questions.md>) | Resolved audit decisions plus non-parity deferred items |
| [F - `@novasamatech` removal](<F - novasamatech-removal.md>) | dotli-side cleanup, exact file:line list |
| [G - Annex](<G - annex.md>) | Shared mechanics: trait conventions, the 5-layer wiring recipe, the override template |
| [I - Nested dApps note](<I - nested-dapps.md>) | Current nested bridge behavior, usefulness, and why it is non-blocking for v1 |
| [J - Implementation plan](<J - implementation-plan.md>) | Ordered work packages, review slicing, and acceptance gates |

---

## Architecture

```
  Product (sandboxed page / iframe / worker)
     |  SCALE wire frames  (versioned v0.1 / v0.2)
     v
  +-------------------------------------------------------------+
  | truapi-server  (the core; ships as WASM and via UniFFI)     |
  |   generated dispatcher  ->  api::{Account,Signing,            |   <- protocol authority:
  |                               StatementStore,Preimage,...}    |      pairing handshake + verify,
  |   host_logic/  session . sso-pairing . message-exchange .     |      channel encryption, proof
  |                proofs . key-derivation                        |      signing, session state
  +----------------------+--------------------------------------+
       platform traits   |  (truapi-platform, RPITIT async)
                         v
  +-------------------------------------------------------------+
  | host shell  (dotli: web worker / electron / iOS / Android)  |
  |   Storage Navigation Notifications Permissions Features      |   <- OS/UI primitives
  |   ChainProvider Theme Preimage SessionStore                   |
  |   PairingPresenter (QR UI)                                    |
  |   ResourceAllocationConfirm                                   |
  |   RuntimeConfig { calling_product_id, pairing inputs }        |
  +----------------------+--------------------------------------+
                         |  ChainProvider connection
                         v
            People-chain statement store  (statement_submit / _subscribe)
                         ^  encrypted SCALE statements on derived topics (P2P gossip)
                         v
                 SSO peer / phone wallet  (holds root signing key + ring secret; signs on request)
```

The core is the protocol authority; the host is reduced to OS primitives. The load-bearing
consequences:

1. **One transport for the SSO session protocol.** Pairing, transaction/raw signing, transaction
   construction, resource allocation, ring-VRF alias, and the product statement store are all SCALE
   statements on the People-chain statement store, reached through the existing `ChainProvider` trait. The
   core runs the SSO + message-exchange protocol itself ([H](<H - sso-pairing-protocol.md>)); there is no
   relay server and no separate wallet socket.
2. **Signing authority stays in the wallet, but signing is core protocol, not a host primitive.** The
   wallet keeps the root key and signs on request; the core sends `signingRequest`/`aliasRequest`
   messages over the encrypted SSO session channel and the wallet replies through the same protocol. This
   resolves the old "who holds the wallet channel" question: there is no separate wallet channel; the core
   owns the SSO protocol over the statement store ([E2](<E - open-questions.md>)).
3. **Pairing is core-verified.** The core mints the session keys, builds the QR, verifies the wallet's
   signed handshake statement, and derives the session. The host only renders the QR
   ([A1](<A - host-primitives.md>)); it cannot assert a false identity.
4. **The core owns `SessionState`, including the session `ssSecret`.** The `ssSecret` is the core's own
   session statement-store key (the wallet never sends a secret); the core signs statement proofs and
   derives product-account public keys in-core, no host round-trip.
5. **All wire types already exist** (`truapi/src/v01` + `versioned`). The gap is implementations, never
   type definitions.

6. **Nested dApps share the Rust core in v1.** Do not introduce separate nested runtime/session/product
   identities as part of this migration. The current dotli nested bridge behavior is documented as a
   future-design note in [I](<I - nested-dapps.md>).

The only new SSO protocol primitive is the QR presenter ([A1](<A - host-primitives.md>)). Separately,
removing dotli's current `host-container` bridge and restoring core-owned sessions requires platform
surfaces for notifications, theme, preimage, host-global session persistence, resource-allocation
confirmation, and product runtime config ([A3](<A - host-primitives.md>)). The pairing/signing crypto
internals are read from current dotli's pinned `@novasamatech` package sources plus the iOS peer, then
vector-gated; see [H](<H - sso-pairing-protocol.md>) and [D](<D - crypto-foundation.md>).

---

## Host portability contract

dotli is the first migration target, not the architecture boundary. The reusable boundary is:

- `truapi-server` owns protocol and product-visible behavior: SSO pairing, channel encryption,
  request/response routing, account derivation, statement-store proof/submit/subscribe, error mapping,
  timeout behavior, session restore/logout, and wire method dispatch.
- `truapi-platform` plus `RuntimeConfig` owns host-specific capabilities: storage, chain connection,
  navigation, notifications, theme, preimage backend, resource allocation confirmation UI, QR/deeplink
  presentation, and opaque session persistence.
- The same core APIs and crypto vectors must pass on WASM hosts (dotli web/Electron worker) and UniFFI
  hosts (iOS/Android). Host bindings may differ, but they cannot fork SSO, signing, statement-store,
  derivation, or session semantics.
- dotli-specific details such as `localStorage` route names, public metadata URL rewriting, modal
  components, and product-label lookup live in the dotli adapter/runtime config. They are not allowed to
  leak into `truapi-server`.

Adding another host should mean implementing the platform traits, providing `RuntimeConfig`, and wiring the
embedding transport. It must not mean reimplementing pairing, wallet signing proxy messages, statement
proof signing, or session persistence semantics outside Rust.

---

## The gap at a glance

| Domain | Deferred/unsupported in current dotli parity | Implemented in Rust core today |
|---|---|---|
| `Account` | `create_account_proof`(26) | `connection_status_subscribe`(18), `get_account`(22), `get_account_alias`(24), `get_legacy_accounts`(28), `get_user_id`(110), `request_login`(112), logout/disconnect |
| `Signing` | none | `create_transaction`(30), `create_transaction_with_legacy_account`(32), `sign_payload`(116), `sign_payload_with_legacy_account`(36), `sign_raw`(114), `sign_raw_with_legacy_account`(34) |
| `StatementStore` | none | `subscribe`(56), `submit`(62), `create_proof`(60), `create_proof_authorized`(132) |
| `ResourceAllocation` | none | `request`(130) |
| `Entropy` | none | `derive`(108) |
| `Theme` | none | `subscribe`(104) |
| `Notifications` | none | `send_push_notification`(4), `cancel_push_notification`(134) |
| `Preimage` | none | `lookup_subscribe`(64), `submit`(68) |
| `Payment` | `balance_subscribe`(118), `top_up`(122), `request`(124), `status_subscribe`(126) | none, and dotli intentionally returns typed "not implemented" errors |

`Chat` and `CoinPayment` remain outside this milestone and keep generated trait defaults until another
host/product requires them. `Payment` and full `create_account_proof` deliberately return
`Unsupported`; handlers that are unimplemented in dotli can stay that way until the other hosts need them.
The current Rust core owns the dotli parity paths: SSO-only pairing/restore/logout, product-account
derivation, signing and transaction SSO proxying, statement-store submit/subscribe/proofs, allocation
requests, entropy, theme, notifications, and preimage callbacks.

Latest-dotli audit note: dotli main has moved from host-papp's V1 metadata-URL QR to the host-papp 0.8.6
SSO V2 proposal. The Rust pairing implementation must therefore migrate from `HostHandshakeData::V1` to
`VersionedHandshakeProposal::V2`, carry host metadata entries (`HostName`, `HostVersion`, `HostIcon`,
`PlatformType`, `PlatformVersion`), parse the V2 response envelope, and persist the returned
`rootEntropySource`. Until that lands, the current Rust V1 pairing path is not complete current-dotli
pairing parity even though non-pairing dotli bridge gates pass.

Current dotli backs its host bridge with `@novasamatech/host-api` + `host-container`, and auth/session
flows with `host-papp`, `statement-store`, `sdk-statement`, and `storage-adapter`. The packages are **not
part of the Rust repo**, but current `~/github/dotli/node_modules/.bun` contains the pinned JS sources.
Pairing, message-exchange, proof, and derivation internals are therefore extract-from-source/vector tasks;
see [D](<D - crypto-foundation.md>).

---

## Roadmap

| Tier | Scope | Detail |
|---|---|---|
| 0 | Runtime construction plumbing (S): pass `RuntimeConfig` through Rust, WASM, UniFFI, and JS worker creation; no account-only session injection APIs. Zero crypto. | [A](<A - host-primitives.md>), [G](<G - annex.md>) |
| 1 | Pure-core accounts (S–M): add the crypto baseline + golden vectors; `get_account` (in-core sr25519 derivation); `get_legacy_accounts`, `get_user_id` (read `SessionState`). | [B](<B - core-impls.md>), [D](<D - crypto-foundation.md>) |
| 1.5 | Protocol crypto/vector gate (M): build the narrow WASM-safe crypto module/crate, leaning on existing Rust crypto crates and using `useragent-kit` only as implementation precedent for similar migrations; capture vectors for HDKD, statement proof, handshake/channel encryption, and topic/session derivation. No pairing I/O yet. | [D](<D - crypto-foundation.md>) |
| 2 | Statement-store client + pairing (L): a minimal People-chain statement submit/subscribe client over `ChainProvider`, then the `request_login` SSO handshake ([H](<H - sso-pairing-protocol.md>)) + the `PairingPresenter` (A1); extend `SessionInfo`; persist/restore through `SessionStore`; expose public logout/disconnect. The keystone. | [H](<H - sso-pairing-protocol.md>), [A](<A - host-primitives.md>), [B](<B - core-impls.md>), [C](<C - session-contract.md>) |
| 3 | Message-exchange ops + statement-store parity: `sign_payload`/`sign_raw`, `create_transaction`, and legacy signing/create-transaction over the SSO session channel; `create_proof`/`_authorized` (in-core, session `ssSecret`); product statement `subscribe`/`submit` (same client as Tier 2); `get_account_alias` (aliasRequest); resource allocation host confirmation + SSO request. | [H](<H - sso-pairing-protocol.md>), [B](<B - core-impls.md>) |
| 3.5 | Non-Nova but implemented dotli host behavior needed once host-container is gone: preimage host callbacks, scheduled/cancellable notifications, theme subscription, and entropy derivation from the SSO V2 `rootEntropySource`. | [B](<B - core-impls.md>), [A](<A - host-primitives.md>) |
| 4 | Remaining TrUAPI surface not required for current dotli parity: full `create_account_proof`, Payment, Chat, CoinPayment, and any future move of preimage fully in-core. | [B](<B - core-impls.md>), [E](<E - open-questions.md>) |

The protocol crypto/vector gate (Tier 1.5) keeps byte-level compatibility failures out of the pairing
I/O path. The statement-store client (Tier 2) is the keystone: it unlocks pairing, signing-proxy,
ring-VRF, and the product statement store, all over one People-chain connection. After Tier 3.5, the dotli
bridge no longer depends on any Nova package: `host-api`, `host-container`, `host-papp`,
`statement-store`, `sdk-statement`, and `storage-adapter` all drop. Full checklist in
[F](<F - novasamatech-removal.md>); concrete work packages and review slices are in
[J](<J - implementation-plan.md>).

## Landing checklist

Use this checklist to review whether the spec is complete enough to implement:

- The architecture has one protocol owner: `truapi-server`; hosts provide only OS/UI/storage/chain
  capabilities.
- Current dotli parity is defined from `~/github/dotli`, not from the stale submodule.
- Every currently implemented dotli handler is either assigned to an in-core implementation, assigned to a
  true host primitive, or explicitly deferred because dotli does not implement it today.
- PR 104 transitional surfaces are named and have a removal path: raw account/signing/statement callbacks
  and account-only session injection cannot become permanent architecture.
- SSO pairing, signing, transaction construction, alias, resource allocation, statement proofs, and
  product statement-store all use the same People-chain statement-store protocol.
- Session persistence is core-owned, single-session, host-global, and not host-papp compatible; cutover
  requires one-time re-pair.
- Crypto constants are sourced from current dotli package code and gated by native + wasm vectors before
  pairing I/O lands.
- The dotli removal checklist ends with no runtime dependency on any `@novasamatech/*` package.
