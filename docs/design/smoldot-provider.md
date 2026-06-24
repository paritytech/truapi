# `smoldot_provider` — backing TrUAPI's chain services with a smoldot light client

**Status:** Proposal / research — not yet implemented
**Scope:** A new Dart package that lets a Flutter/Dart TrUAPI **host** serve the
chain-facing services from an in-process [smoldot](https://github.com/smol-dot/smoldot)
light client, with **zero changes to the `truapi` package interfaces**.
**Depends on:** `package:truapi` (this repo, `dart/truapi`) and **`package:smoldot`
v1.2.0 from the `snowpinelabs/polkadart` fork** (`/home/ubuntu/claude/polkadart-snowpinelabs`,
branch `chore/upgrade-smoldot-1.2.0`) — this wraps **smoldot-light 1.2.0** (modern
JSON-RPC v2 API) and adds native **statement-store** support, unlike the
`justkawal/polkadart` `packages/smoldot` (v0.1.0, smoldot-light 0.18). Use the
snowpinelabs fork.

---

## TL;DR

- Target **`package:smoldot` v1.2.0 (snowpinelabs fork)**, which wraps
  smoldot-light 1.2.0 — the full modern Polkadot JSON-RPC v2 API.
- The TrUAPI **Chain** service is, almost line-for-line, a wrapper over the modern
  JSON-RPC v2 families — `chainHead_v1_*`, `chainSpec_v1_*`, `transaction_v1_*`.
  **smoldot 1.2.0 speaks exactly these.** So the Chain service maps onto smoldot
  cleanly; that's the bulk of the chain surface (and `chainHead_v1_storage` now
  reads child tries natively, covering `get_head_storage`'s `childTrie`).
- **StatementStore `submit` + `subscribe` are now in scope** — smoldot-light 1.0+
  ships Substrate's statement-store protocol (`statement_submit` /
  `statement_subscribeStatement`), enabled per chain via
  `AddChainConfig.statementStore`. Only StatementStore's `create_proof*` (signing)
  stays out (wallet, not a light client).
- **smoldot is not a TrUAPI `Provider` by itself.** A TrUAPI `Provider` is the
  byte-frame transport between a product and its host; smoldot is a *chain
  JSON-RPC* backend. `smoldot_provider` bridges the two: it implements the
  generated **`ChainHostHandlers`** (and the smoldot-backable parts of
  **`StatementStoreHostHandlers`**) against smoldot, and (optionally) wraps an
  embedded host server as a turnkey TrUAPI `Provider` — so from the client's side
  it behaves "just like the js/ts providers," with **no `truapi` interface change**.
- **Signing and the rest of Preimage stay out** — signing is the wallet's; Preimage
  is a content store that smoldot 1.2.0 doesn't serve cleanly yet (see §2).
- Two real dependencies shape the work: (1) **native smoldot library** must be
  built and bundled per platform (FFI), and (2) even at v1.2.0 the **`package:smoldot`
  JSON-RPC handler is unchanged** — it doesn't surface subscription ids or do
  chainHead/statement-style unsubscribe — so `smoldot_provider` must work around or
  upstream-fix that.

---

## 0. Status & layering (current)

The work splits into **two packages / two layers**:

| Layer | Package | Repo | Status |
|---|---|---|---|
| **A — chain connection** | `smoldot_provider` | **polkadart** (`packages/smoldot_provider`) | **DONE** (branch `feat/smoldot-provider`) |
| **B — TrUAPI mapping** | `truapi_smoldot` | **truapi** (`dart/truapi_smoldot`) | **Chain DONE** (validated vs live Westend); **StatementStore** implemented, pending live validation; turnkey provider descoped |

**Layer B progress (`dart/truapi_smoldot`):**
- [x] Package skeleton + cross-repo dep wiring (path deps on `truapi`, `smoldot_provider`; `smoldot` override). `dart pub get` resolves.
- [x] `JsonRpcClient` over a `smoldot_provider` `JsonRpcProvider` (request/response by id, subscriptions by `params.subscription`, per-method unsubscribe, orphan-notification buffering). 5 unit tests green (fake provider, no FFI).
- [x] `SmoldotChainBackend` (client + chain-per-genesis + `JsonRpcClient`) + hex codec. Lazily `addChain`s per genesis hash, caches a `JsonRpcClient` future per chain, optional relay-chain + statement-store.
- [x] `SmoldotChainHandlers`: `chainSpec_v1_*` methods (genesis hash, chain name, properties) + error→`Err(GenericError)` guard. 4 unit tests green (scripted provider, no FFI).
- [x] `SmoldotChainHandlers`: the `chainHead_v1_*` follow/operation engine + `getHeadHeader`/`getHeadBody`/`getHeadStorage`/`callHead`/unpin/continue/stop. A follow-session registry keyed by `followSubscriptionId` (= the follow call's `CallContext.requestId`, which the product echoes on every operation) maps operations onto the smoldot follow subscription; `followHeadSubscribe` streams every `chainHead_v1_follow` event (lifecycle + operation results) decoded into typed `RemoteChainHeadFollowItem`s. Uses an explicit `StreamController` (not `async*`/`await for`, which deadlocks `cancel()`) to forward cancellation → `chainHead_v1_unfollow`. 7 unit tests green (fake chain, no FFI).
- [x] `SmoldotChainHandlers`: `transaction_v1_*` (`broadcast`/`stop`). 2 unit tests green.
- [x] `SmoldotStatementStoreHandlers`: `submit` (↔ `statement_submit`) + `subscribe` (↔ `statement_subscribeStatement`, host-side topic filter for `MatchAll`/`MatchAny`); `createProof`/`createProofAuthorized` return `Err(Unknown)` (signing is the wallet's, not a light client's). Includes a self-contained Substrate `sp_statement_store::Statement` SCALE codec (`statement_codec.dart`) transcoding the typed `SignedStatement` ⇄ statement bytes. 13 unit tests green (codec round-trip per proof variant + handler behaviour over a fake chain). **Open validation point:** the `expiry: u64` ⇄ Substrate `Priority(u32)` mapping (`priority = expiry >> 32`) and the `statement_subscribeStatement` notification shape / dump-vs-live framing are best-effort from spec and must be confirmed against a live statement-store-enabled chain (no public network runs it yet, so this stays a documented follow-up).
- [ ] Turnkey provider — **descoped for now.** Products are web apps on the TS route, so the Dart side only needs the host handlers, which a Flutter host wires into its own `TruapiHostHandlers` + transport (native-bridge decision D1) via the generated `buildChainEntries`/`buildStatementStoreEntries`. Revisit only if an in-process Dart product↔host transport is needed.
- [x] Live integration test (FFI + network): `SmoldotChainHandlers` end-to-end over a real smoldot light client against **Westend** (`test/chain_integration_test.dart`). Validates `getSpecChainName` → "Westend", `getSpecGenesisHash` → the known genesis, and `followHeadSubscribe` → `getHeadHeader` (the operation correlates to the live follow by id and returns a real finalized header). Opt-in via `@Tags(['network'])` + `dart_test.yaml` skip; run with `dart test --run-skipped -t network` and `LD_LIBRARY_PATH` pointing at `packages/smoldot/native/linux`. The default `dart test` run stays offline (33 unit tests, 0 network).
- [ ] Live integration test for **StatementStore** submit/subscribe — pending a statement-store-enabled chain reachable by a light client (none public yet). This is the validation gap noted above for the `expiry`⇄`priority` mapping and the subscribe notification framing.

- **Layer A — `smoldot_provider` (DONE).** The Dart equivalent of polkadot-api's
  `@polkadot-api/sm-provider`: `getSmProvider(Chain | Future<Chain>)` turns a smoldot
  chain into a standard string-based `JsonRpcProvider`
  (`(onMessage) => { send, disconnect }`). Plus `getRawProvider` over a minimal
  `RawJsonRpcChain`. This is "the provider," with the same interface as the JS/TS path,
  so consumers run their own JSON-RPC client over it. Light-client only, by design.
  Prereq also done: the snowpinelabs `package:smoldot` `Chain` now exposes the raw JS
  interface (`sendJsonRpc`/`nextJsonRpcResponse`/`jsonRpcResponses`; branch
  `feat/smoldot-raw-jsonrpc-chain-api`). Both verified against live Westend.
- **Layer B — `truapi_smoldot` (remaining).** Consumes a `JsonRpcProvider` from
  `smoldot_provider`, runs a JSON-RPC client over it, and maps TrUAPI **Chain** (↔
  `chainHead_v1_*`/`chainSpec_v1_*`/`transaction_v1_*`) and **StatementStore**
  `submit`/`subscribe` (↔ `statement_*`) onto the generated `ChainHostHandlers` /
  `StatementStoreHostHandlers`, plus a turnkey TrUAPI `Provider`. Lives in the truapi
  repo (it depends on both `package:truapi` and `package:smoldot_provider`). This is the
  bulk of §§2–4 below, now built on the Layer-A provider rather than raw FFI.

> Naming note: §§5–8 below were written before the split and use "`smoldot_provider`"
> for the whole thing. Read those as **Layer B (`truapi_smoldot`)**, which now `import`s
> the Layer-A `smoldot_provider` `JsonRpcProvider` instead of re-deriving a raw client.

---

## 1. Architecture & terminology (read this first)

There are **two different things called "provider"** in play. Keeping them
separate is the whole key to this design.

| | **TrUAPI `Provider`** | **Chain JSON-RPC provider** |
|---|---|---|
| Defined in | `package:truapi` (`lib/src/transport.dart`) | `package:smoldot` (`Chain.request/subscribe`); also polkadart's `Provider` |
| Carries | TrUAPI wire frames `[requestId][u8 id][payload]` | Polkadot JSON-RPC requests/notifications |
| Connects | product (client) ↔ host (dispatcher) | host (or any app) ↔ a chain |
| Examples | `LoopbackChannel`, MessagePort/iframe (TS), a native bridge | `WsProvider`, smoldot `Chain` |

**Where smoldot fits.** A TrUAPI host implements every service. For the **Chain**
service, the host needs a chain connection — in the desktop (novasama) host that's
its node/light client; in Dart that's **smoldot**. So smoldot backs the host's
**Chain handlers**. It does *not* replace the product↔host transport.

```
 product (TrUAPI client)
        │  createClient(provider)
        │  TrUAPI wire frames
        ▼
 TrUAPI host dispatcher  (createTruapiServer / createHostServer)
        │  ChainHostHandlers  ← implemented by smoldot_provider
        ▼
 SmoldotChainHandlers  ──JSON-RPC (chainHead_v1_*, chainSpec_v1_*, transaction_v1_*)──▶  smoldot light client ──▶ chain
```

**Two ways to consume `smoldot_provider`** (it ships both; same core underneath):

1. **Host handlers (primary, for a Flutter host).** You build a TrUAPI host and
   plug `SmoldotChainHandlers` into your `TruapiHostHandlers.chain`. Your product
   (TS, or Dart) talks to your host over its real transport; the host answers Chain
   calls from smoldot. This is the literal answer to "back the host's chain logic
   with smoldot."
2. **Turnkey TrUAPI `Provider` (convenience, all-in-one-process).** For a Dart app
   that wants the typed TrUAPI client to read chain data directly with no separate
   host process, `createSmoldotProvider(...)` returns a TrUAPI `Provider` that
   embeds a host server (Chain via smoldot + your handlers for other services).
   Then `createClient(provider)` "just works" — this is the "smoldot as the
   provider, like js/ts providers" experience, with **no `truapi` interface change**.

Both are implemented purely against the **existing** `ChainHostHandlers` /
`HostDispatchEntry` / `Provider` interfaces. Nothing in `package:truapi` changes.

---

## 2. Scope — what smoldot can and cannot back

From the per-method mapping (§3), the chain-facing TrUAPI services split as follows
**for smoldot-light 1.2.0**:

| Service / method | smoldot-backable? | Why |
|---|---|---|
| **Chain** (all 13) | ✅ **Yes — core deliverable** | 1:1 with `chainHead_v1_*` / `chainSpec_v1_*` / `transaction_v1_*`; read-only chain access + tx broadcast |
| **StatementStore::subscribe** | ✅ **Yes** | `statement_subscribeStatement` (topic filter), with `AddChainConfig.statementStore` enabled |
| **StatementStore::submit** | ✅ **Yes** | `statement_submit` (a signed statement) |
| **StatementStore::create_proof / create_proof_authorized** | ❌ No | **Signing** — the host's wallet, not a light client |
| **Preimage** (`lookup_subscribe`, `submit`) | ⚠️ Not yet | Content store: `submit` writes (needs signing/extrinsic); `lookup` could be `pallet_preimage` storage reads via `chainHead_v1_storage` (awkward, polling) or `bitswap_v1_get` (smoldot ≥ 3.1, not in 1.2.0). Treat as out for now; revisit |

**In scope for `smoldot_provider`:** the **Chain** service (complete) and the
**StatementStore** `submit` + `subscribe`. **Out of scope:** StatementStore
`create_proof*` (signing), Preimage, and every other service (Account, Signing,
Theme, …) — the host supplies those itself (e.g. `create_proof*` via its wallet).

> Note: `Chain::broadcast_transaction` *submits* a signed extrinsic
> (`transaction_v1_broadcast`). The transaction must already be signed (by the
> host's wallet/Signing service) — smoldot only broadcasts bytes. Everything else
> in Chain is read-only and ideal for a light client.

---

## 3. Chain → JSON-RPC mapping

Every Chain method carries a `genesisHash` (identifies the target chain) and uses
SCALE `Vec<u8>` / hex on the wire. The mapping to smoldot's JSON-RPC:

| Chain method | wire | JSON-RPC | Notes |
|---|---|---|---|
| `follow_head_subscribe` | sub 76 | `chainHead_v1_follow` | subscription; notifications → `RemoteChainHeadFollowItem` variants (`Initialized`/`NewBlock`/`BestBlockChanged`/`Finalized`/`Operation*`/`Stop`) |
| `get_head_header` | req 80 | `chainHead_v1_header` | returns the SCALE header inline (`{ header: Option<bytes> }`) |
| `get_head_body` | req 82 | `chainHead_v1_body` | returns `{ operation: Started{operationId} \| LimitReached }`; body arrives as `OperationBodyDone` on the follow stream |
| `get_head_storage` | req 84 | `chainHead_v1_storage` | operation; results stream as `OperationStorageItems`/`OperationStorageDone` |
| `call_head` | req 86 | `chainHead_v1_call` | operation; result as `OperationCallDone`; needs `withRuntime:true` on follow |
| `unpin_head` | req 88 | `chainHead_v1_unpin` | GC of pinned blocks |
| `continue_head` | req 90 | `chainHead_v1_continue` | resume a `WaitingForContinue` operation |
| `stop_head_operation` | req 92 | `chainHead_v1_stopOperation` | cancel an operation |
| `get_spec_genesis_hash` | req 94 | `chainSpec_v1_genesisHash` | |
| `get_spec_chain_name` | req 96 | `chainSpec_v1_chainName` | |
| `get_spec_properties` | req 98 | `chainSpec_v1_properties` | JSON properties string |
| `broadcast_transaction` | req 100 | `transaction_v1_broadcast` | returns an `operationId` (Option); **submits** signed tx bytes |
| `stop_transaction` | req 102 | `transaction_v1_stop` | cancels a broadcast |

**The follow/operation correlation is the heart of the adapter.** In `chainHead_v1`,
`body`/`storage`/`call` don't return their data directly — they return an
`operationId`, and the data arrives later as a `chainHead_v1_followEvent`
notification (`operationBodyDone`, etc.) on the *follow* subscription. TrUAPI models
this exactly (operation-started response + `Operation*Done` follow items). So the
adapter must, per active follow subscription, route operation events back through
the follow item stream — it does **not** invent a request/response correlation;
it mirrors the JSON-RPC one.

The TrUAPI `followSubscriptionId` carried by `get_head_*` requests equals the host
dispatcher's `requestId` for the follow `start` frame (= `CallContext.requestId` of
`follow_head_subscribe`). So `SmoldotChainHandlers` keys its per-follow state by
that id and looks it up on each operation call. (See §5.3.)

### 3.1 StatementStore → JSON-RPC (smoldot-light ≥ 1.0)

Enabled per chain by passing `AddChainConfig.statementStore` (a `StatementStoreConfig`).

| StatementStore method | wire | JSON-RPC | Notes |
|---|---|---|---|
| `subscribe` | sub 56 | `statement_subscribeStatement` | `MatchAll`/`MatchAny(Vec<Topic>)` → topic filter params; notifications → `RemoteStatementStoreSubscribeItem { statements, is_complete }` |
| `submit` | req 62 | `statement_submit` | a `SignedStatement` (SCALE-encoded statement bytes) → `()` |
| `create_proof` | req 60 | — (host wallet) | **signing**; out of scope |
| `create_proof_authorized` | req 132 | — (host wallet) | **signing**; out of scope |

The statement wire format (`SignedStatement`, `StatementProof`, topics) must map to
smoldot's statement encoding for `statement_submit` and from the
`statement_subscribeStatement` notifications — a focused codec task, fixtured like
the chain codecs. `create_proof*` are left to the host's signer (the provider can
delegate to an app-supplied callback or return `UnableToSign`).

---

## 4. The `package:smoldot` surface (v1.2.0) and its gaps

`package:smoldot` v1.2.0 (snowpinelabs fork, `packages/smoldot`) is FFI bindings to
**smoldot-light 1.2.0** (Rust `smoldot-light = "1.2"`). It exposes the full modern
JSON-RPC v2 API plus the statement-store protocol, and ships convenience method
constants (`chainHead_v1_*`, `transaction_v1_*`, `statement_*`, `bitswap_v1_get`).

```dart
final client = SmoldotClient(config: SmoldotConfig(maxChains: 8, maxLogLevel: 3));
await client.initialize();
final chain = await client.addChain(AddChainConfig(
  chainSpec: assetHubSpecJson,
  potentialRelayChains: [relay.chainId],          // parachain → its relay
  statementStore: StatementStoreConfig(),          // enable statement_* on this chain
));
final resp = await chain.request('chainSpec_v1_genesisHash', []);     // Future<JsonRpcResponse>
final stream = chain.subscribe('chainHead_v1_follow', [false]);        // Stream<JsonRpcResponse>
// resp.result is the decoded JSON value; stream items carry params.result of each notification
await client.dispose();
```

- `Chain.request(method, params) → Future<JsonRpcResponse>` (`.result` = decoded JSON).
- `Chain.subscribe(method, params) → Stream<JsonRpcResponse>` (each item = a
  notification's `params.result`).
- Multi-chain: `addChain` with `potentialRelayChains` for parachains.
- FFI → native lib per platform (Android/iOS/macOS/Linux/Windows).

**The "gap" is in the Dart convenience layer, not in smoldot — and the fix is the
same one the JS/TS world already uses.** smoldot itself (both the Rust FFI and the
official `smoldot` **JS** bindings) exposes only a *raw* JSON-RPC interface:
`sendJsonRpc(request: string)` + `nextJsonRpcResponse(): Promise<string>` /
`jsonRpcResponses` — **no** `subscribe()` helper and **no** subscription-id
management. The caller owns request ids, subscription-id correlation, and framing.
The JS/TS consumers (substrate-connect, polkadot-api, and any TS TrUAPI host) run
their **own JSON-RPC client** over that raw interface — which is precisely why they
have the subscription id (it's in the subscribe *response*) and do correct
per-method unsubscribe. There is no gap there to "fix"; the correlation is just part
of the consumer.

The Dart `package:smoldot` had instead added a *convenience* layer
(`JsonRpcHandler.request/subscribe/unsubscribe`) that hid the raw interface and
dropped the subscription id. **DONE:** the snowpinelabs fork's `Chain` now exposes
the raw JS interface directly — `sendJsonRpc(String)`,
`nextJsonRpcResponse() -> Future<String>`, `jsonRpcResponses -> Stream<String>`
(branch `feat/smoldot-raw-jsonrpc-chain-api`; the incomplete convenience layer was
removed, `JsonRpcHandler` → `RawJsonRpc`). So `smoldot_provider` builds its own
JSON-RPC client over `Chain.sendJsonRpc` / `Chain.jsonRpcResponses` — exactly the
JS architecture, no reaching into FFI internals.

Remaining real constraints (not "gaps"):
- **Polling/pull model**: `nextJsonRpcResponse` yields one message at a time (same
  as JS); our client runs a read-loop. Pin the `chore/upgrade-smoldot-1.2.0` commit.
- **Native library bundling** (BUILD.md; `rust/rust-toolchain.toml` pins rustc ≥ 1.85
  for smoldot-light 1.2.0's edition-2024) — a real packaging task.

So Phase 1 builds `smoldot_provider`'s **own JSON-RPC client** over the raw
interface (the JS architecture), giving us subscription ids and correct
per-method unsubscribe for free — no dependence on the package's convenience layer.

---

## 5. Package design: `smoldot_provider`

### 5.1 Layout

```
dart/smoldot_provider/                 # new package in this repo (sibling of dart/truapi)
  pubspec.yaml                         # deps: truapi (path), smoldot (path/git), test
  lib/
    smoldot_provider.dart              # barrel
    src/
      backend.dart                     # SmoldotChainBackend: client + chains keyed by genesis hash
      json_rpc.dart                    # OUR JSON-RPC client over smoldot's raw sendJsonRpc/nextJsonRpcResponse (request + subscribe-with-id + per-method unsubscribe), mirroring substrate-connect/polkadot-api
      chain_handlers.dart             # SmoldotChainHandlers implements ChainHostHandlers (the engine)
      statement_handlers.dart         # SmoldotStatementStoreHandlers implements StatementStoreHostHandlers
      follow.dart                      # per-follow state: smoldot sub id + operation→follow routing
      codec/
        params.dart                    # TrUAPI Chain request types  → JSON-RPC params (bytes↔hex)
        events.dart                    # JSON-RPC follow events       → RemoteChainHeadFollowItem
        responses.dart                 # JSON-RPC results            → TrUAPI Chain responses
        statements.dart                # SignedStatement / topics ⇄ statement_submit / _subscribeStatement
      provider.dart                    # createSmoldotProvider(...): turnkey TrUAPI Provider
  test/
    codec_test.dart                    # mapping unit tests (recorded JSON-RPC fixtures)
    chain_handlers_test.dart           # chain handlers against a fake JSON-RPC chain
    statement_handlers_test.dart       # statement handlers against a fake JSON-RPC chain
    integration_test.dart              # real smoldot + Paseo/Westend (tagged, opt-in)
  example/
    host_with_smoldot.dart             # compose Smoldot{Chain,StatementStore}Handlers into a host
```

### 5.2 Public API (sketch — implements existing interfaces only)

```dart
// Manages the smoldot client and one chain per genesis hash, from app-supplied specs.
class SmoldotChainBackend {
  static Future<SmoldotChainBackend> create({
    // per genesis hash: chain spec, optional relay spec, and whether to enable statement-store
    required Map<String /*genesisHashHex*/, ChainSpecConfig> chains,
    SmoldotConfig config,
  });
  Future<void> dispose();
}

// The core deliverable for a Flutter host: drop into TruapiHostHandlers.chain.
class SmoldotChainHandlers implements ChainHostHandlers {
  SmoldotChainHandlers(SmoldotChainBackend backend);
  // ...all 13 Chain methods, backed by smoldot...
}

// Statement store via smoldot (subscribe + submit). create_proof* delegate to the
// host's signer (app-supplied callback) or return UnableToSign.
class SmoldotStatementStoreHandlers implements StatementStoreHostHandlers {
  SmoldotStatementStoreHandlers(SmoldotChainBackend backend, {StatementSigner? signer});
}

// Convenience: a turnkey TrUAPI Provider (embedded host: smoldot Chain [+ statements]
// + your other services).
Future<Provider> createSmoldotProvider({
  required SmoldotChainBackend backend,
  TruapiHostHandlers? otherServices,      // optional: your Account/Signing/… handlers
  StatementSigner? statementSigner,        // optional: enable create_proof* via your wallet
  HostServerHooks? hooks,
});
```

`SmoldotChainHandlers` / `SmoldotStatementStoreHandlers` implement the **generated**
`ChainHostHandlers` / `StatementStoreHostHandlers` (no changes to them).
`createSmoldotProvider` returns the **existing** `Provider` type. Statement-store
chains must be created with `AddChainConfig.statementStore` enabled (D6).

### 5.3 The follow/operation engine (`follow.dart` + `chain_handlers.dart`)

Per active TrUAPI follow subscription, keyed by `followSubscriptionId`
(= the follow start's `CallContext.requestId`):

- `followHeadSubscribe(ctx, req)` →
  - resolve the smoldot `Chain` for `req.genesisHash` (via backend),
  - `chain.subscribeWithId('chainHead_v1_follow', [req.withRuntime])` → `(smoldotSubId, jsonStream)`,
  - register `_FollowState(followId: ctx.requestId, smoldotSubId, chain)`,
  - return a `Stream<RemoteChainHeadFollowItem>` mapping each json event via `codec/events.dart`;
    on stream cancel → `chain.request('chainHead_v1_unfollow', [smoldotSubId])` + drop state.
- `getHeadHeader/Body/Storage/Call/unpin/continue/stopOperation(ctx, req)` →
  - look up `_FollowState` by `req.followSubscriptionId`,
  - call the matching `chainHead_v1_*` request with `state.smoldotSubId` + mapped params,
  - map the JSON result → TrUAPI response (operation-started or inline value).
  - Operation **data** is delivered through the existing follow stream (the
    `Operation*Done` events) — no extra correlation needed.
- `getSpec*` → `chainSpec_v1_*` on the resolved chain.
- `broadcastTransaction` → `transaction_v1_broadcast`; `stopTransaction` → `transaction_v1_stop`.

Genesis-hash routing: the backend lazily `addChain`s from the app-supplied spec
and verifies `chainSpec_v1_genesisHash` matches the requested hash.

### 5.4 Type mapping (`codec/`)

The adapter converts between TrUAPI Chain types (the generated `types.dart`:
`Uint8List`, structs, the `RemoteChainHeadFollowItem` sealed class) and JSON-RPC
JSON (hex strings, objects). Examples:
- `Vec<u8>` / `[u8;N]` ↔ `0x…` hex.
- `RemoteChainHeadFollowItem.Initialized{finalizedBlockHashes, finalizedBlockRuntime}`
  ↔ `{ event: "initialized", finalizedBlockHashes:[...], finalizedBlockRuntime:{...} }`.
- `StorageQueryItem{key, queryType}` ↔ `{ key:"0x…", type:"value|hash|…" }`.
- `OperationStartedResult` ↔ `{ result:"started", operationId } | { result:"limitReached" }`.

This layer is pure, deterministic, and the most test-worthy part (record real
smoldot JSON-RPC notifications as fixtures and assert both directions).

---

## 6. Key design decisions (recommendations)

| # | Decision | Recommendation | Why / alternatives |
|---|---|---|---|
| **D1** | What is `smoldot_provider`? | **Both:** core `SmoldotChainHandlers` (host handlers) **+** convenience `createSmoldotProvider` (turnkey TrUAPI `Provider`). | Covers the Flutter-host case (primary) and the all-in-one-process product case, with one engine. No `truapi` change either way. |
| **D2** | Where does the package live? | **`dart/smoldot_provider/` in this (truapi) repo**, sibling to `dart/truapi`. | Reuses the generated `ChainHostHandlers`/types; versioned with the protocol. Alt: standalone repo or in the host app — fine, but loses co-location. |
| **D3** | `package:smoldot` dependency | **RESOLVED: snowpinelabs fork, v1.2.0** (`chore/upgrade-smoldot-1.2.0`), pinned by commit; isolate behind `src/json_rpc.dart`. | Wraps smoldot-light 1.2.0 (modern JSON-RPC v2 + statement store); the justkawal v0.1.0 wraps 0.18 and lacks statement store. |
| **D4** | Subscription-id / unsubscribe | **DONE.** The fork's `Chain` now exposes the raw `sendJsonRpc`/`nextJsonRpcResponse`/`jsonRpcResponses` (branch `feat/smoldot-raw-jsonrpc-chain-api`). `smoldot_provider` runs its own JSON-RPC client over it (the JS/TS approach). | smoldot is raw-JSON-RPC by design; JS consumers own correlation — so do we. Gives subscription ids + correct unsubscribe; no convenience-layer fork. |
| **D5** | Scope | **Chain (all) + StatementStore `submit`/`subscribe`.** `create_proof*` (signing), Preimage, and other services are the host's own. | smoldot 1.2.0 serves chain + statement-store protocol, but not signing or content-store preimages. |
| **D6** | Chain specs + statement store | **App supplies `{genesisHash: (chainSpec, relaySpec?, enableStatementStore)}`.** | smoldot needs a chain spec to add a chain; statement-store services require `AddChainConfig.statementStore` on that chain. Ship Paseo Asset Hub (+ relay) and Westend specs as examples. |
| **D7** | Native library | **Treat as a first-class packaging phase**; for Flutter, a plugin that bundles per-platform binaries. | FFI light client; can't ship Dart-only. |

---

## 7. Phased plan

### Phase 0 — Spike & decisions (de-risk the dependency) — DONE
- ✅ snowpinelabs `package:smoldot` v1.2.0 running locally; native lib loads (rustc ≥ 1.85).
- ✅ Raw interface exposed on `Chain` (`sendJsonRpc`/`nextJsonRpcResponse`/`jsonRpcResponses`)
  and driven **end-to-end against live Westend** (legacy `system_chain` + modern
  `chainSpec_v1_genesisHash`), through the `smoldot_provider` `JsonRpcProvider`.
- ✅ `smoldot_provider` (Layer A) built + tested: `getSmProvider` over a smoldot chain.
- Remaining spike bits (carry into Layer B): exercise `chainHead_v1_follow` (capture follow
  sub id from the subscribe **response**) + `chainHead_v1_header`, and `statement_subscribeStatement`;
  capture their notification shapes as codec fixtures. Decide D6 (which chains enable statements).

### Phase 1 — JSON-RPC client + chain backend (Layer B) — over the `smoldot_provider` provider
- Run a JSON-RPC client over a `JsonRpcProvider` (from `smoldot_provider.getSmProvider`):
  a read-loop routing responses by request id and notifications by `params.subscription`;
  `request(method,params)`, `subscribe(...) → (id, Stream)`, per-method unsubscribe
  (`chainHead_v1_unfollow`, `statement_unsubscribeStatement`). (The provider already owns
  the raw send/receive; this is the correlation layer, mirroring substrate-connect/papi.)
- A small chain registry: one `JsonRpcProvider` per genesis hash from app-supplied specs
  (with `AddChainConfig.statementStore` where enabled), verify genesis via
  `chainSpec_v1_genesisHash`, parachain (relay) wiring, lifecycle/dispose.
- Unit-test the client against a fake `JsonRpcProvider` (no FFI).

### Phase 1 — smoldot backend + our JSON-RPC client
- `SmoldotChainBackend`: init `SmoldotClient`, lazily `addChain` per genesis hash from
  app-supplied specs (with `AddChainConfig.statementStore` where enabled), verify genesis
  via `chainSpec_v1_genesisHash`, parachain (relay) wiring, lifecycle/dispose.
- `src/json_rpc.dart`: **our own JSON-RPC client** over smoldot's raw
  `sendJsonRpc`/`nextJsonRpcResponse` — a read-loop that routes responses by request id
  and notifications by `params.subscription`; `request(method,params)`,
  `subscribe(method,params) → (String id, Stream<dynamic>)`, and per-method unsubscribe
  (`chainHead_v1_unfollow`, `statement_unsubscribeStatement`). Mirrors substrate-connect.
- (Optional, recommended) upstream a clean `Chain.sendJsonRpc`/`jsonRpcResponses`
  passthrough to the snowpinelabs fork; until then use the public `bindings`/`chainId`.
- Unit-test the client against a fake/in-memory raw JSON-RPC chain.

### Phase 2 — Codec layer (the mapping)
- `codec/params.dart`, `codec/events.dart`, `codec/responses.dart`: bidirectional
  TrUAPI Chain types ↔ JSON-RPC JSON for every method + every follow event variant.
- `codec/statements.dart`: `SignedStatement` / topics ⇄ `statement_submit` params and
  `statement_subscribeStatement` notifications (+ the `RemoteStatementStoreSubscribeItem`
  `{statements, is_complete}` shape).
- `codec_test.dart`: drive with recorded real-smoldot fixtures from Phase 0; assert
  both directions (params, results, events, statements) — this is the correctness core.

### Phase 3 — Service handlers
- **`SmoldotChainHandlers`** (`implements ChainHostHandlers`): the follow/operation engine
  (§5.3) — per-follow state keyed by `followSubscriptionId`, operation routing through the
  follow stream, genesis-hash routing; all 13 Chain methods; error mapping; lifecycle
  (TrUAPI follow cancel → `chainHead_v1_unfollow` + cleanup).
- **`SmoldotStatementStoreHandlers`** (`implements StatementStoreHostHandlers`): `subscribe`
  → `statement_subscribeStatement` (topic filter; cancel → `statement_unsubscribeStatement`),
  `submit` → `statement_submit`; `create_proof*` delegate to an app-supplied `StatementSigner`
  or return `UnableToSign`.
- `chain_handlers_test.dart` + `statement_handlers_test.dart`: handlers driven against a
  fake JSON-RPC chain (no native lib).

### Phase 4 — Turnkey TrUAPI `Provider`
- `createSmoldotProvider(...)`: build an embedded host (`createHostServer` with
  `buildChainEntries(SmoldotChainHandlers)` + `buildStatementStoreEntries(...)` + optional
  app handlers) over an in-process channel; expose the client side as a `Provider`.
- e2e over `LoopbackChannel`: generated TrUAPI **client** → smoldot provider → fake chain,
  exercising `getSpecChainName`, `followHeadSubscribe`, `getHeadHeader`, and
  `statementStore.subscribe`/`submit`.
- (Note the related client-side gap in §10 for *Dart products* using chain follow.)

### Phase 5 — Native library & platform packaging
- Reproducible build of smoldot-light native libs (per BUILD.md) for target platforms.
- Flutter plugin packaging that bundles the binaries (Android `.so`, iOS/macOS
  `.dylib`/xcframework, Linux `.so`, Windows `.dll`); document desktop dev setup.
- Example Flutter host app loading the lib and running a real chain.

### Phase 6 — Testing & conformance
- Unit (codec) green on fixtures; handler tests on the fake chain.
- Integration (`integration_test.dart`, opt-in/tagged): real smoldot + Westend/Paseo —
  follow → header/body/storage/call → unpin/unfollow; spec methods; broadcast a tx
  (signed test extrinsic) end to end.
- e2e: TrUAPI client → turnkey provider → live chain.

### Phase 7 — Upstream / dependency hardening — DONE
- **Done:** the snowpinelabs `package:smoldot` `Chain` now exposes the raw JS interface
  (`sendJsonRpc(String)` + `nextJsonRpcResponse()` + `jsonRpcResponses`); the incomplete
  convenience layer was removed (`JsonRpcHandler` → `RawJsonRpc`). Branch
  `feat/smoldot-raw-jsonrpc-chain-api` (analyze/format clean; verified against live Westend).
- Remaining: merge that branch; pin the resulting commit.

### Phase 8 — Docs, CI, packaging
- `dart/smoldot_provider/README.md` (host-handlers usage + turnkey provider).
- CI: analyze + codec/handler tests on every push; integration tests on a manual/nightly
  job (native lib + network).
- Version alignment with `package:truapi`; publish decision.

---

## 8. Implementation checklist

### Phase 0 — Spike — DONE
- [x] smoldot v1.2.0 native lib loads on the dev platform (rustc ≥ 1.85)
- [x] `SmoldotClient` + `addChain` (Westend) running locally
- [x] Raw `Chain` interface + `smoldot_provider.getSmProvider` driven end-to-end on live
      Westend (`system_chain` + `chainSpec_v1_genesisHash`)
- [x] `smoldot_provider` (Layer A): `getSmProvider`/`getRawProvider` + unit + integration tests
- [ ] `chainHead_v1_follow`/`_header` + `statement_subscribeStatement` exercised; fixtures captured (→ Layer B)
- [ ] Decision D6 fixed (which chains enable statements)

### Phase 1 — JSON-RPC client + chain registry (Layer B, over `smoldot_provider`)
- [ ] JSON-RPC client over a `JsonRpcProvider` (`request` + `subscribe`-with-id + per-method unsubscribe)
- [ ] Chain registry (one provider per genesis hash + `statementStore`, verify genesis, relay, dispose)
- [ ] Client unit tests (fake `JsonRpcProvider`)

### Phase 2 — Codec
- [ ] `params.dart` — all Chain request types → JSON-RPC params
- [ ] `events.dart` — follow events → `RemoteChainHeadFollowItem` (all variants)
- [ ] `responses.dart` — results → header/body/storage/call/spec/tx responses
- [ ] `statements.dart` — `SignedStatement`/topics ⇄ `statement_submit`/`_subscribeStatement`
- [ ] `codec_test.dart` green on recorded fixtures (both directions)

### Phase 3 — Service handlers
- [ ] `SmoldotChainHandlers`: `_FollowState` + per-follow routing; all 13 methods; error mapping; cancel → `chainHead_v1_unfollow`
- [ ] `SmoldotStatementStoreHandlers`: `subscribe`/`submit` via `statement_*`; `create_proof*` → signer/UnableToSign
- [ ] `chain_handlers_test.dart` + `statement_handlers_test.dart` green (fake chain)

### Phase 4 — Turnkey provider
- [ ] `createSmoldotProvider(...)` (embedded host: Chain + StatementStore + composition slot)
- [ ] e2e: TrUAPI client → provider → fake chain (chain request + follow + statement sub/submit)

### Phase 5 — Native & packaging
- [ ] Reproducible native build (target platforms)
- [ ] Flutter plugin bundling per-platform binaries
- [ ] Example Flutter host app on a real chain

### Phase 6 — Testing
- [ ] Codec + handler unit tests green
- [ ] Integration (real smoldot + Westend/Paseo), opt-in
- [ ] Full e2e against a live chain

### Phase 7 — Upstream
- [ ] Raw `Chain.sendJsonRpc`/`jsonRpcResponses` passthrough upstreamed to snowpinelabs fork (or driven via public `bindings`/`chainId`)
- [ ] Fork commit pinned

### Phase 8 — Docs/CI
- [ ] README (both usages)
- [ ] CI: analyze + unit; nightly/manual integration
- [ ] Version alignment + publish decision

---

## 9. Testing strategy

- **Codec unit tests (no native lib):** record real smoldot `chainHead_v1` JSON-RPC
  notifications/results in Phase 0; assert TrUAPI⇄JSON both directions. This is where
  correctness lives and runs in plain CI.
- **Handler tests (no native lib):** drive `SmoldotChainHandlers` against a fake
  JSON-RPC chain (scripted responses/notifications) — covers the follow/operation
  routing and lifecycle without smoldot or network.
- **Integration (opt-in):** real smoldot + Westend/Paseo; slow, network + native lib;
  manual/nightly CI job.
- **e2e:** generated TrUAPI client → turnkey provider → chain (fake in CI, live in the
  nightly job).

The fake-chain seam means the bulk of `smoldot_provider` is testable in ordinary CI
with no native dependency.

---

## 10. Open decisions / questions

- **Native build & platforms (D7):** which platforms first (desktop Linux/macOS for
  dev; Android/iOS for the app)? Who owns the reproducible smoldot native build —
  reuse the snowpinelabs fork's `rust/` + `tool/`, or build here?
- **smoldot fork/version (D3) — RESOLVED:** snowpinelabs `package:smoldot` v1.2.0
  (smoldot-light 1.2.0), pinned by commit. (justkawal v0.1.0 wraps 0.18 and lacks the
  statement store.)
- **Raw-interface passthrough (D4):** ship now by reaching the public `bindings`/`chainId`,
  or upstream a `Chain.sendJsonRpc`/`jsonRpcResponses` first? (Either works; upstream is tidier.)
- **Chain specs + statement store (D6):** which specs (Paseo Asset Hub + relay, Westend),
  and which chains enable `statementStore`? App supplies the rest. Confirm target chains.
- **Client-side subscription id (related, optional, *not* required for the host path):**
  the Dart TrUAPI **client**'s subscription methods return `Stream<Item>` and don't
  surface the subscription id, which a *Dart product* needs to call chain operations
  (`followSubscriptionId`). For the **host** use (your case) this is irrelevant — the
  remote product supplies the id. For the turnkey-provider-with-a-Dart-product case,
  we'd add an opt-in client API to expose it (mirrors TS `Subscription.subscriptionId`)
  — a small, backward-compatible addition, tracked separately, **not** a protocol change.
- **Transaction status:** `broadcast_transaction` (`transaction_v1_broadcast`) is
  fire-and-broadcast (no status stream in the Chain trait). Confirm the host doesn't
  need `transactionWatch_v1_*` status events surfaced elsewhere.

---

## 11. Risks & mitigations

| Risk | Mitigation |
|---|---|
| `package:smoldot`'s convenience layer is incomplete (subscription id, unsubscribe) | Bypass it — run our own JSON-RPC client over the **raw** `sendJsonRpc`/`nextJsonRpcResponse` (the JS/TS approach); pin the fork; optional upstream passthrough (Phase 7) |
| Statement wire format ⇄ smoldot statement encoding mismatch | Fixture-driven `codec/statements.dart` tests against real `statement_*` notifications |
| Native lib build/bundling friction across platforms | Dedicated Phase 5; start with desktop dev; reuse polkadart's build tooling |
| chainHead_v1 follow/operation correlation bugs | Mirror the JSON-RPC model exactly; fixture-driven codec tests + fake-chain handler tests |
| smoldot warm-up/sync latency in tests | `waitUntilSynced`/finalized-head gating; mark integration tests slow/opt-in |
| Genesis-hash → chain-spec mismatch | Verify `chainSpec_v1_genesisHash` on add; fail loudly |
| Scope creep into Preimage/StatementStore/signing | Explicitly out of scope (§2); host owns them |

---

## 12. Appendix — key references

- TrUAPI Chain trait + doc examples: `rust/crates/truapi/src/api/chain.rs`; types:
  `rust/crates/truapi/src/v01/chain.rs`, `v01/common.rs`.
- Generated Dart host handler interface: `ChainHostHandlers` in
  `dart/truapi/lib/src/generated/host.dart`; types in `…/generated/types.dart`.
- TrUAPI host runtime: `dart/truapi/lib/src/host/host_server.dart`
  (`createHostServer`, `buildChainEntries`, `Provider`).
- smoldot Dart bindings (target): `polkadart-snowpinelabs/packages/smoldot/lib/`
  (`smoldot.dart`, `src/client.dart`, `src/chain.dart`, `src/json_rpc.dart`,
  `src/bindings.dart` — raw `sendJsonRpcRequest`/`nextJsonRpcResponse`); build:
  `packages/smoldot/BUILD.md`, `rust/rust-toolchain.toml`, `rust/Cargo.toml` (`smoldot-light = "1.2"`).
- smoldot **JS** raw interface (the model we mirror): `smol-dot/smoldot`
  `wasm-node/javascript/src/public-types.ts` — `Chain.sendJsonRpc(string)`,
  `nextJsonRpcResponse()`, `jsonRpcResponses` (no `subscribe` helper; caller owns correlation).
- polkadart JSON-RPC provider reference: `polkadart/packages/polkadart/lib/apis/provider.dart`.
- Modern Polkadot JSON-RPC spec: `chainHead_v1_*`, `chainSpec_v1_*`, `transaction_v1_*`,
  `statement_*` (paritytech/json-rpc-interface-spec; smoldot is the reference implementation).
