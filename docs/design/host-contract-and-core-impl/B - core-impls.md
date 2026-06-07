# B - Core implementations (`truapi-server` wire methods)

> Part of the [host-contract & core-impl spec](<index.md>).

Implement the stubbed wire methods on `PlatformRuntimeHost<P>` (`runtime.rs`). `Signing`,
`StatementStore`, `Preimage`, `ResourceAllocation`, `Entropy`, `Theme`, and `Payment` are empty `impl`
blocks today (`runtime.rs:341-553`); `Notifications` is explicitly unavailable; `Account` is partially
overridden (`runtime.rs:321-339`). Every un-overridden method falls back to the trait default
`Err(CallError::unavailable())` / `Subscription::empty()`.

Use the [override template + error-mapping rules in the annex](<G - annex.md>) for the per-method
implementation shape (destructure `::V1`, UFCS-call the platform, map domain errors to
`CallError::Domain`), where new in-core state and `host_logic/` modules go, and the
`PermissionsService` pattern for multi-step logic.

Strategy tags below: **pure-core** (no host help) · **core+session** (reads `SessionState`) · **core+chan**
(a request/response over the SSO session channel, [H §5](<H - sso-pairing-protocol.md>)) · **core protocol**
(the pairing handshake or the statement-store client).

| Wire method (id) | Strategy | Tier |
|---|---|---|
| `get_legacy_accounts`(28), `get_user_id`(110) | core+session + runtime product config | 1 |
| `get_account`(22) | pure-core + crypto | 1 |
| `request_login`(112) | core protocol (SSO handshake, [H](<H - sso-pairing-protocol.md>)) | 2 |
| `sign_payload`(116), `sign_raw`(114), `sign_*_with_legacy_account`(34/36) | core+chan (signingRequest) | 3 |
| `create_transaction`(30), `create_transaction_with_legacy_account`(32) | core+chan (createTransactionRequest) | 3 |
| `create_proof`(60), `create_proof_authorized`(132) | core+session + crypto | 3 |
| `get_account_alias`(24) | core+chan (aliasRequest) | 3 |
| `subscribe`(56), `submit`(62) | core-native (statement client) | 3 |
| `ResourceAllocation::request`(130) | host-confirm + core+chan (resourceAllocationRequest) | 3 |
| `Entropy::derive`(108) | core+session + crypto | 3.5 |
| `Theme::subscribe`(104) | host-side primitive/subscription | 3.5 |
| `Notifications::send_push_notification`(4), `cancel_push_notification`(134) | host-side primitive, extend current trait | 3.5 |
| `lookup_subscribe`(64), `submit`(68) [preimage] | host-side primitive/callback | 3.5 |
| `create_account_proof`(26) | core+chan (full ring-VRF; needs new message) | deferred (unimplemented by dotli) |
| `Payment::*` | intentionally unavailable | deferred (dotli returns typed "not implemented" errors) |

---

## Tier 1: account reads + derivation

### `get_legacy_accounts` (#28) · `get_user_id` (#110): core+session **(S)**

Read `self.session_state.current()` (returns `Option<SessionInfo>`) and the runtime
`calling_product_id` / label config. No session => `HostAccountGetError::NotConnected` /
`HostGetUserIdError::NotConnected`.

- `get_legacy_accounts` → `HostGetLegacyAccountsResponse { accounts: vec![LegacyAccount { public_key:
  product_public_key.to_vec(), name: info.lite_username }] }`. Current dotli returns the single
  product-derived `(calling_product_id, 0)` account, not the wallet root
  (`~/github/dotli/packages/ui/src/container.ts:222-233`).
- `get_user_id` → `HostGetUserIdResponse { primary_username: info.full_username.or(info.lite_username)
  .ok_or(NotConnected)? }`, gated by the product-scoped `UserId` permission like current dotli
  (`~/github/dotli/packages/ui/src/container.ts:242-261`).

Both have **unit** versioned requests (no payload). **Acceptance:** with a session set,
`get_legacy_accounts` returns the product-derived account and `get_user_id` returns the permitted
username; without one, they match current dotli's not-connected/empty behavior.

### `get_account` (#22): pure-core + crypto **(S–M)**

```rust
async fn get_account(&self, _cx, request: HostAccountGetRequest)
    -> Result<HostAccountGetResponse, CallError<HostAccountGetError>>
// req:  v01::HostAccountGetRequest  { product_account_id: ProductAccountId }
//       ProductAccountId { dot_ns_identifier: String, derivation_index: u32 }
// resp: v01::HostAccountGetResponse { account: ProductAccount { public_key: Vec<u8> } }
// err:  NotConnected | Rejected | DomainNotValid | Unknown { reason }
```

Read the session root pubkey (`info.public_key`), derive the product public key per
[D](<D - crypto-foundation.md>) (sr25519 soft-derivation over junctions `["product", dot_ns_identifier,
String(derivation_index)]`), return it. No session => `NotConnected`; invalid dotNS => `DomainNotValid`.
**Acceptance:** byte-identical to dotli's `deriveProductPublicKey` for shared `(root, dotNS, index)`
golden vectors.

---

## Tier 2: pairing in the core

### `request_login` (#112): core protocol (SSO handshake) **(L)**

Replace the current override body (`runtime.rs:321-339`). `request_login` runs the SSO pairing handshake
over the People-chain statement store and establishes the session. The full protocol (QR payload, topic
derivation, channel encryption, handshake messages) is in [H](<H - sso-pairing-protocol.md>); below is
the wire contract + control flow.

```rust
async fn request_login(&self, cx, request: HostRequestLoginRequest)
    -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>>
// req:  v01::HostRequestLoginRequest { reason: Option<String> }
// resp: enum { Success, AlreadyConnected, Rejected }
// err:  HostRequestLoginError::Unknown { reason }
```

Control flow (see [H §1-§3](<H - sso-pairing-protocol.md>) for the byte-level steps):

```
  session already present? --> AlreadyConnected
  mint session sr25519 + P-256 keypairs ; read RuntimeConfig ; build HostHandshakeData -> deeplink
  A1.present(deeplink) ; subscribe(bootstrap topic) on the People-chain statement store
  select! {
     handshake statement -> verify sr25519 proof ; ECDH-decrypt the answer ;
                            derive SessionState{ public_key = root_user_account_id, ss_secret,
                                                 peer keys, session ids } ;
                            persist (C) ; set_session -> Connected -> Success
     A1 present resolves (user closed the QR) -> Rejected
     cx.cancel() (product cancelled) -> abort
  }
```

Depends on the Tier-2 statement-store client (submit/subscribe over `ChainProvider`) and the runtime
pairing config ([A2](<A - host-primitives.md>)). The crypto byte contract is specified in
[H](<H - sso-pairing-protocol.md>) + [D](<D - crypto-foundation.md>) and must be pinned by vectors before
implementation. **Acceptance:** a real wallet pairs and `connection_status` flips to Connected carrying
the wallet's `root_user_account_id`; a handshake statement with a bad proof or wrong signer is rejected;
closing the QR aborts cleanly.

---

## Tier 3: message-exchange ops + statement proofs

### `sign_payload` (#116) · `sign_raw` (#114) · legacy variants (#34/#36): core+chan (signingRequest) **(M)**

```rust
// sign_payload req: v01::HostSignPayloadRequest { account: ProductAccountId, payload: HostSignPayloadData }
// sign_raw     req: v01::HostSignRawRequest     { account: ProductAccountId, payload: RawPayload }
// resp (both):     v01::HostSignPayloadResponse { signature: Vec<u8>, signed_transaction: Option<Vec<u8>> }
// err  (both):     FailedToDecode | Rejected | PermissionDenied | Unknown { reason }
```

Enforce product-account validity and the `ChainSubmit` remote permission via the existing `Permissions`
trait first (denied => `CallError::Domain(PermissionDenied)`), then send a
`signingRequest(transaction | rawPayload)` over the SSO session channel ([H §5](<H - sso-pairing-protocol.md>))
and await the `signingResponse`. The wallet signs with its `/product/<dotNs>/<idx>` key and returns
`Signature { raw_signature, signed_transaction? }`. Current dotli's host modal maps local cancel before
dispatch to `Rejected`; `session.signPayload` / `session.signRaw` `Err(message)`, transport teardown, and
host-papp's 180s queue timeout map to `Unknown { reason }`. dotli's modal also wraps the call in a 300s
outer fallback, but the session-layer timeout should win for normal no-response cases. No active session
maps to `Rejected` in the current signing handlers, and invalid product account / denied `ChainSubmit`
maps to `PermissionDenied`. The legacy variants re-derive `(calling_product_id, 0)`, verify the provided
legacy signer matches that derived account, and then route through the same product-account request.
**Acceptance:** signature
matches today's dotli path; ungranted `ChainSubmit` => `PermissionDenied`; host-modal cancel =>
`Rejected`; SSO/session failure => `Unknown { reason }`; signer mismatch maps to `Unknown` like current
dotli.

### `create_transaction` (#30) · `create_transaction_with_legacy_account` (#32): core+chan **(M)**

Current dotli implements both (`~/github/dotli/packages/ui/src/container.ts:413-461`,
`:594-664`). Product-account `create_transaction` validates `signer`, checks `ChainSubmit`, then sends
`createTransactionRequest` over the SSO channel. The legacy variant re-derives `(calling_product_id, 0)`,
checks the raw signer public key, then sends the same SSO request with synthetic product signer
`[calling_product_id, 0]`. **Acceptance:** returns the encoded signed transaction bytes from the wallet;
permission denial, no-session rejection, host-modal cancel, signer mismatch, SSO/session failure, and the
host-papp 180s queue timeout match current dotli behavior (`Rejected` for no session or local cancel,
`PermissionDenied` for product/permission failure, `Unknown { reason }` for signer mismatch,
session-channel failure, or timeout). The current UI wrapper also has a 300s outer fallback.

### `create_proof` (#60) · `create_proof_authorized` (#132): core+session + crypto **(L)**

```rust
// create_proof            req: v01::RemoteStatementStoreCreateProofRequest { product_account_id, statement: Statement }
// create_proof_authorized req: v01::Statement  (bare; uses a pre-allocated allowance account)
// resp (both):                 RemoteStatementStoreCreateProofResponse { proof: StatementProof }
// err  (both):                 UnableToSign | UnknownAccount | Unknown { reason }
```

Sign the `Statement` in-core with the session `ssSecret` (schnorrkel sr25519), producing
`StatementProof::Sr25519 { signature:[u8;64], signer:[u8;32] }` where `signer` is the **session
statement-store pubkey** (the core's own session sr25519 key, which the wallet granted statement-store
allowance to during pairing, **not** the wallet root). dotli reads this same key as `ssSecret` via
`secrets.read`. No session/secret => `UnableToSign`. The `Statement` field ordering fed to the proof is
known from current `@novasamatech/sdk-statement`: encode the unsigned statement, strip the leading
compact length prefix, and sign the remaining SCALE payload with the 64-byte expanded `ssSecret`
([D3](<D - crypto-foundation.md>)).
**Acceptance:** proof bytes match `createSr25519Prover(ssSecret).generateMessageProof(statement)` golden
vectors.

### `get_account_alias` (#24): core+chan (aliasRequest) **(M)**

Send an `aliasRequest { product_account_id, calling_product_id }` over the SSO channel; the wallet
derives the bandersnatch alias for `context = blake2b(utf8("/product/<dotNs>/<idx>"))` and replies
`ContextualAlias { context:[u8;32], alias }` ([H §5](<H - sso-pairing-protocol.md>)). Map to
`HostAccountGetAliasResponse { context, alias }`; on cross-domain alias requests the host shows the
alias-permission prompt (today via dotli; in-core this is a `Permissions` check). Current dotli does not
add a local timeout around `session.getRingVrfAlias`; no session maps to `NotConnected`, invalid product
identifier to `DomainNotValid`, permission-prompt cancel to `Rejected`, and session-channel errors to
`Unknown { reason }`. **Acceptance:** `get_account_alias` returns `{ context, alias }` matching today.

### `ResourceAllocation::request` (#130): host-confirm + core+chan **(M)**

Current dotli prompts, then calls `session.requestResourceAllocation`
(`~/github/dotli/packages/ui/src/container.ts:666-724`). The core should send
`resourceAllocationRequest { calling_product_id, resources, on_existing }` over the SSO channel only
after a host UI confirmation callback accepts the request. Dismissal/cancel maps to the typed unknown
error path dotli uses today. This is a dedicated confirmation trait/callback, not
`Permissions::remote_permission`, because current dotli's modal owns retry behavior around the SSO
round-trip: a failed allocation attempt leaves the modal open and lets the user retry. Dotli does not add
a separate UI-level timeout around `requestResourceAllocation`; it relies on session-channel request
machinery, which has the host-papp 180s queue timeout for this operation. No active session,
host dismissal/cancel, and SSO/session errors all map to
`ResourceAllocationErr.Unknown { reason }`. Map the host-api 0.8 `BulletinAllowance` tag to the SSO
peer's current `BulletInAllowance` dialect if still required by vectors. Strip secret material from
`Allocated` outcomes before returning to the product, as dotli does today. **Acceptance:** requesting
statement/bulletin allowances shows host confirmation, allows retry after SSO failure, succeeds with the
paired SSO peer after approval, and does not expose returned secrets to the product.

---

### `subscribe` (#56) · `submit` (#62) [statement-store]: core-native **(M, was XL)**

These are part of current dotli feature parity because dotli wires `handleStatementStoreSubscribe` and
`handleStatementStoreSubmit` through `@novasamatech/statement-store` today
(`~/github/dotli/packages/ui/src/container.ts:981-1041`). They therefore block removing
`statement-store` / `sdk-statement`, even though the heavy transport work is already done by the Tier-2
statement-store client.

```rust
// subscribe req: RemoteStatementStoreSubscribeRequest = MatchAll(Vec<[u8;32]>) | MatchAny(Vec<[u8;32]>)
//           item: RemoteStatementStoreSubscribeItem { statements: Vec<SignedStatement>, is_complete: bool }
// submit    req: v01::SignedStatement (bare) ; resp: () ; err: v01::GenericError
```

These reuse the **same statement-store client** built for pairing (Tier 2): the core already submits and
subscribes to statements on the People-chain connection. `submit` posts the `SignedStatement`; `subscribe`
filters by topic and streams pages. **Paging contract** (RFC-0008) the implementation must preserve:

```
  is_complete:  false   false   false   |   true     true     true   ...
                \___ historical dump ___/  ^first true = "synced"   \_ live, new statements only _/
                (more pages follow)        (all subsequent pages are true)
  topic limits: MatchAll <= 4, MatchAny <= 128 ; subscribe filters to statements with a proof
```

The on-chain `Statement` SCALE shape + the `statement_submit`/`statement_subscribeStatement` RPC are
known from the iOS peer ([H](<H - sso-pairing-protocol.md>)). Because the pairing client already exists,
this is no longer an XL pallet port; the remaining work is the product-facing topic-filtered subscription
+ the dump-then-live paging. [E1](<E - open-questions.md>) resolves this in-core.

---

## Tier 3.5: implemented host-side dotli behavior

### `Entropy::derive` (#108): core+session + crypto **(S-M)**

Current dotli reads `ssSecret` and calls `deriveProductEntropy(secret, "${label}.dot", key)`
(`~/github/dotli/packages/ui/src/container.ts:769-794`). Implement in-core using the same algorithm from
`@novasamatech/host-container` vectors and the runtime `product_label` parity input. No session/secret =>
`Unknown { reason:"Not connected" }` / `"Session secret missing"` parity unless the Rust error type is
more specific.

### `Theme::subscribe` (#104): host-side **(S)**

Current dotli emits `{ name: Default, variant: Light|Dark }` and listens for
`dotli:theme-changed` (`~/github/dotli/packages/ui/src/container.ts:1194-1214`). Add a small platform
theme subscription or a runtime host callback; this is not Nova-specific, but it is implemented dotli
behavior once `host-container` is removed.

### Notifications (#4/#134): extend platform primitive **(M)**

Current dotli schedules/cancels local notifications and returns a host-assigned id
(`~/github/dotli/packages/ui/src/container.ts:929-975`). `truapi-platform::Notifications` currently only
has `push_notification(...) -> Result<(), GenericError>`, while the TrUAPI wire requires
`HostPushNotificationResponse { id }` and `cancel_push_notification`. Extend the platform trait/bridge so
the host can return ids and cancel scheduled notifications.

### `lookup_subscribe` (#64) · `submit` (#68) [preimage]: host-side **(M)**

Current dotli implements preimage submit/lookup with the selected content backend and local cache
(`~/github/dotli/packages/ui/src/container.ts:1081-1192`). It does not use Nova packages, but it must still
be exposed through the Rust host once `host-container` is removed. Keep the implementation host-side for
v1 ([E4](<E - open-questions.md>)); add platform callbacks/traits rather than moving IPFS/Bulletin logic
into `truapi-server`.

---

## Tier 4: remaining

### `create_account_proof` (#26): core+chan (ring-VRF): deferred

Req `RingLocation { genesis_hash, ring_root_hash, hints }` + `context`, resp `{ proof: Vec<u8> }`. This
is a **full ring-VRF proof**, but the SSO channel exposes `deriveAlias` only today (the wallet's
`BandersnatchKeyManaging.createProof` is not wired to a message). Carrying it needs a new message type or
an in-core bandersnatch path ([E3](<E - open-questions.md>)).

### `Payment::*`: intentionally unavailable

Current dotli registers typed "not implemented" payment responses
(`~/github/dotli/packages/ui/src/container.ts:1216-1243`). Leave Payment unavailable for this milestone;
it only becomes relevant when another host/product needs real payment rails.
