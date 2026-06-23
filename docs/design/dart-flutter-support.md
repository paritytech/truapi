# Flutter / Dart support for TrUAPI

**Status:** Proposal / research — not yet implemented
**Author:** (research pass)
**Scope:** Add a first-class Dart client (and, later, host) target so Flutter/Dart
products can speak the TrUAPI wire protocol, while the Rust crates remain the single
source of truth.

---

## TL;DR

- TrUAPI's Rust→TypeScript pipeline is **pure build-time code generation**. There is
  **no WASM** anywhere in the repo (no `wasm-bindgen`, no `wasm-pack`, no `cdylib`, no
  `.wasm` artifacts). The Rust crates are never executed at runtime by JS; `truapi-codegen`
  reads **rustdoc JSON** and emits TypeScript source. (The "wasm host" mentioned in the
  task brief does not exist — and we do not need it.)
- The codegen already separates a **language-agnostic IR** (`rust/crates/truapi-codegen/src/rustdoc.rs`
  → `ApiDefinition`) from the **TypeScript emitter** (`ts.rs`). Adding Dart is therefore a
  **sibling emitter** (`dart.rs`) plus a hand-written **Dart runtime package** — exactly the
  shape of the existing `ts.rs` + `js/packages/truapi/src/{scale,transport,client}.ts` split.
- The wire contract is small and SCALE-based: `[requestId: SCALE str][u8 discriminant][payload]`,
  with versioned `Vn` envelopes at `#[codec(index = n-1)]`. The primitive surface is narrow
  (no floats, no maps, no big-int beyond u64/u128), which makes a faithful Dart SCALE codec
  tractable.
- **Recommended approach:** keep Rust as truth → reuse the existing IR → add a `--dart-output`
  emitter → ship a pure-Dart `truapi` pub package (hand-written SCALE codec + transport +
  providers + generated code) → guarantee parity with golden cross-language wire vectors.

---

## 1. How TrUAPI works today (research findings)

### 1.1 The pipeline is codegen, not WASM

```
rust/crates/truapi/         Rust traits define the protocol; each method tagged #[wire(id = N)]
        │
        │  cargo +nightly rustdoc -p truapi --output-format json
        ▼
target/doc/truapi.json       rustdoc JSON (types, traits, signatures, doc comments)
        │
        │  cargo run -p truapi-codegen  (rustdoc.rs parses JSON → ApiDefinition IR)
        ▼
ApiDefinition (IR)           language-agnostic: traits, methods, wire ids, type defs
        │
        │  ts.rs emitter
        ▼
js/packages/truapi/src/generated/{types,client,wire-table,index}.ts   (+ host, playground, explorer, examples)
```

Verified facts:

- `scripts/codegen.sh` runs `cargo +nightly rustdoc ... --output-format json` then
  `cargo run -p truapi-codegen` with `--output`, `--host-output`, `--playground-output`,
  `--explorer-output`, `--client-examples-output`, `--codec-version`.
- `truapi-codegen/src/main.rs` parses the JSON (`rustdoc::parse`), extracts the API
  (`rustdoc::extract_api`), then calls `ts::generate(...)` and the optional emitters.
- **No runtime Rust.** `grep -r "wasm-bindgen|wasm-pack|cdylib|wasm32"` over the repo
  returns nothing. The only crate that produces a binary is `truapi-codegen` (a CLI used at
  build time). The generated TS uses `scale-ts` (pure JS) for serialization.

**Implication for Dart:** we do **not** need to compile Rust to WASM or FFI. We add an
emitter that walks the same IR and prints Dart, and we hand-write the Dart runtime
(codec + transport) once. Rust stays the source of truth because every type, method, wire
id, and doc comment is *derived from* the rustdoc JSON.

### 1.2 The IR (`rustdoc.rs`) — the reuse point

`ApiDefinition` is already emitter-neutral:

- `TraitDef { name, module_path, methods, docs }`
- `MethodDef { name, kind, params, return_type, wire, docs }`
  - `MethodKind` ∈ `{ Request, Subscription, ResultSubscription }`
  - `WireAttrs { request_id, response_id, start_id, stop_id, interrupt_id, receive_id }`
  - `ReturnType` ∈ `{ Result{ok,err}, Subscription(item), ResultSubscription{item,err} }`
- `TypeDef { name, module_path, generic_params, kind, docs }`
  - `TypeDefKind` ∈ `{ Alias(TypeRef), Struct([FieldDef]), TupleStruct([TypeRef]), Enum([VariantDef]) }`
  - `VariantFields` ∈ `{ Unit, Unnamed([TypeRef]), Named([FieldDef]) }`
- `TypeRef` ∈ `{ Primitive(str), Named{name,args}, Vec, Option, Tuple, Array(inner,len), Generic, Unit }`
- `public_trait_order` — source order of the `TrUApi` super-trait bounds; drives stable emission.

`ts.rs` consumes exactly this. `dart.rs` will consume exactly this. **No changes to
`rustdoc.rs` are required** for a first Dart client (only additive helpers if we later find
gaps).

### 1.3 The wire protocol & runtime contract (what the Dart runtime must reproduce)

Frame (`transport.ts::encodeWireMessage`):

```
[ requestId : SCALE str ][ discriminant : u8 ][ payload : SCALE bytes ]
```

- **Request/response:** client sends `request_id` frame with `requestId = "p:<n>"`; host replies
  with the matching `response_id` frame and the same `requestId`. Payload is a versioned
  `Result<Ok, Err>` envelope (`{ tag:"V1", value: Result }`).
- **Subscriptions:** client sends `start_id`; host streams `receive_id` frames and ends with
  `interrupt_id` (typed reason → error; empty → complete). Client sends `stop_id` to cancel.
- **Versioned envelopes:** each `Vn` arm encodes as SCALE enum index `n-1`. The client picks the
  **highest** wrapper variant ≤ the target protocol version (`method_wire_version` in `ts.rs`).
- **Handshake special case:** `System::handshake` auto-responds to inbound
  `host_handshake_request` frames; the inner request carries the codec version. Legacy dotli
  hosts ping every 50ms until they see a response (see `client.ts`). A Dart transport must
  reproduce this auto-handshake to be a drop-in.

The TypeScript runtime that has no generated counterpart (i.e. the parts we must hand-port):

| TS file | Responsibility | Dart equivalent (hand-written) |
|---|---|---|
| `scale.ts` | SCALE primitives/combinators (wraps `scale-ts`) + `Hex`, `lazy`, `indexedTaggedUnion`, `OptionBool`, `Status`, `TaggedUnion` | `scale.dart` |
| `transport.ts` | `Provider` interface, frame encode/decode, iframe/MessagePort providers | `transport.dart` + `providers/*.dart` |
| `client.ts` | `createTransport`: request correlation, subscription lifecycle, auto-handshake | `transport_impl.dart` |
| `neverthrow` (dep) | `Result` / `ResultAsync` | `result.dart` (Dart 3 sealed) |

### 1.4 Scope of the generated surface

From the inventory pass:

- **14 service traits** (super-trait `TrUApi`): Account, Chain, Chat, CoinPayment, Entropy,
  LocalStorage, Notifications, Payment, Permissions, Preimage, ResourceAllocation, Signing,
  StatementStore, System, Theme.
- **~64 wire methods** (~49 request/response, ~15 subscriptions).
- **~186 `v01` concrete types** + **~179 `versioned_type!` envelopes** (currently all V1).
- **Primitive surface is narrow and codec-friendly:** `u8/u16/u32/u64/u128`, `i*`, `bool`,
  `String`, `Vec<u8>` (dominant), `Vec<T>`, `Option<T>`, `[u8; N]`, tuples, `Compact<_>`,
  `OptionBool`. **No** `f32/f64`, **no** `HashMap/BTreeMap`, **no** explicit enum discriminants,
  **no** U256. (`u64/u128` need `BigInt` in Dart — see §3.)
- **Framework types the codegen skips** (defined in `truapi/src/lib.rs`): `CallContext`,
  `CallError<D>` (`Domain/Denied/Unsupported/MalformedFrame/HostFailure`), `Subscription<T>`,
  `CancellationToken`, `RequestId`, `RuntimeFailure`. The Dart runtime supplies hand-written
  equivalents (`CallError`, `Subscription`/`Stream`), same as the TS runtime does.

---

## 2. Strategy

**Mirror the TS architecture, one layer at a time, with the wire format as the contract.**

1. **Reuse the IR.** No new parser. `dart.rs` is a peer of `ts.rs` under `truapi-codegen`.
2. **Hand-write the Dart runtime once** (codec, transport, providers, Result) — the analogue of
   the hand-written `scale.ts`/`transport.ts`/`client.ts`.
3. **Generate** Dart types, codecs, wire table, and service clients into a git-ignored
   `generated/` directory inside the Dart package (same convention as `js/packages/truapi/src/generated`).
4. **Prove parity** with golden wire vectors produced from the Rust/TS side and asserted byte-for-byte
   in Dart (`test/`). This is what makes "Rust is the source of truth" enforceable, not aspirational.
5. **Wire it into the build** (`scripts/codegen.sh`, `Makefile`, CI) so a Rust trait change
   regenerates Dart in the same run as TS, and CI fails if generated Dart drifts.

Both client and **host** dispatcher are implemented (Phase 6). The host reuses the client's generated
`types.dart`, so it lives in the same `dart/truapi` package, exposed via `package:truapi/host.dart`.

---

## 3. Type & codec mapping (Rust → TS → Dart)

The Dart emitter mirrors `codec_expr_mode` / `ts_type_with_named` in `ts.rs`.

| Rust / `TypeRef` | TS type | TS codec | **Dart type** | **Dart codec** | Notes |
|---|---|---|---|---|---|
| `bool` | `boolean` | `S.bool` | `bool` | `S.bool` | |
| `u8 u16 u32` | `number` | `S.u8/u16/u32` | `int` | `S.u8/u16/u32` | fits 64-bit `int` |
| `i8 i16 i32` | `number` | `S.i8/i16/i32` | `int` | `S.i8/i16/i32` | |
| `u64 u128 i64 i128` | `bigint` | `S.u64/...` | `BigInt` | `S.u64/...` | Dart `int` is 64-bit **signed**; u64/u128 must use `BigInt` |
| `Compact<_>` | `number \| bigint` | `S.compact` | `BigInt` | `S.compact` | normalize to `BigInt` for one path |
| `OptionBool` | `boolean \| undefined` | `S.OptionBool` | `bool?` | `S.optionBool` | 1 byte: 0/1/2 |
| `String` | `string` | `S.str` | `String` | `S.str` | |
| `Vec<u8>` | `HexString` | `S.Hex()` | `Uint8List` | `S.bytes()` | see decision D4 (bytes vs hex) |
| `[u8; N]` | `HexString` | `S.Hex(N)` | `Uint8List` | `S.bytesFixed(N)` | |
| `Vec<T>` | `Array<T>` | `S.Vector(c)` | `List<T>` | `S.vector(c)` | |
| `Option<T>` | `T \| undefined` | `S.Option(c)` | `T?` | `S.option(c)` | nullable, idiomatic |
| `(A, B, …)` tuple | `[A, B]` | `S.Tuple(...)` | `(A, B)` record | `S.tuple2(a,b)` | Dart 3 records |
| `()` unit | `undefined` | `S._void` | `void`/`null` | `S.unit` | |
| struct | `interface` | `S.Struct({...})` | immutable class | `S.struct(...)` | const ctor, `==`/`hashCode` |
| unit-only enum | string union | `S.Status(...)` | Dart `enum` | `S.status(values)` | exhaustive `switch` |
| mixed enum | tagged union | `S.TaggedUnion` | **sealed class** | `S.taggedUnion(...)` | Dart 3 sealed + patterns |
| `Vn` envelope | `{tag,value}` union | `S.indexedTaggedUnion` | sealed `Versioned<T>` | `S.indexedTaggedUnion` | index `n-1` |
| `Result<Ok,Err>` | `ResultAsync` | `S.Result` | `Result<Ok,Err>` | `S.result(ok,err)` | sealed `Ok`/`Err` |
| `Subscription<T>` | `ObservableLike` | — | `Stream<T>` | — | idiomatic Dart streams |

**Naming** (mirror `ts.rs`): trait `Foo` → `FooClient`; method `host_account_get` → `accountGet`
(`strip_prefix` + lowerCamelCase); struct/enum names stay PascalCase; client facade `TrUApiClient`
with camelCase service getters. Reserved-word fields (`is`, `in`, `default`, …) get a trailing
underscore or `@JsonKey`-style remap — handled in the emitter.

---

## 4. Dart / Flutter best practices to apply

- **Pure-Dart core, no Flutter SDK dependency.** The `truapi` package depends only on `dart:typed_data`/
  `dart:async`. It then works in Flutter (all platforms), Dart CLI, and server. A Flutter-specific
  provider (if ever needed) lives behind a thin optional import, not in the core.
- **Dart 3 language features as the natural fit for the Rust shapes:**
  - `sealed class` + exhaustive `switch`/pattern matching for enums, tagged unions, `Result`, `Versioned<T>`.
  - **records** `(A, B)` for Rust tuples.
  - nullable types `T?` for `Option<T>` (don't invent an `Option` box — be idiomatic).
- **Immutability:** generated data classes use `final` fields, `const` constructors where possible,
  value `==`/`hashCode`, and `copyWith`. Generate this boilerplate ourselves — **do not** pull in
  `freezed`/`build_runner` (we already own a generator; adding a second codegen toolchain is churn).
- **Bytes:** use `Uint8List` and `BytesBuilder` for buffers; never `List<int>` on hot paths. Provide
  `hex`/`fromHex` helpers for ergonomics, but keep wire ops on bytes.
- **`BigInt` discipline:** `u64/u128/i64/i128` are `BigInt` to avoid silent truncation; document it.
- **Subscriptions as `Stream<T>`:** back each subscription with a `StreamController` whose `onCancel`
  sends the `_stop` frame; map typed `interrupt` to a `SubscriptionInterrupted<Reason>` error and
  empty interrupt to normal stream completion. This is the idiomatic replacement for the TS
  ES-Observable interop.
- **Errors as values:** requests return `Future<Result<Ok, Err>>` (sealed `Result`), mirroring
  `neverthrow`. Reserve thrown exceptions for transport/decode faults.
- **Tooling:** `dart format`, `package:lints`/`flutter_lints` with `analysis_options.yaml`,
  `dart analyze --fatal-infos`, `dart test`. Generated files carry
  `// Auto-generated by truapi-codegen. Do not edit.` + an `// ignore_for_file:` header for lints
  that don't apply to generated code.
- **Docs:** forward Rust doc comments to Dart `///` doc comments (the IR already carries `docs`),
  stripping the ` ```ts ` playground example blocks (the TS emitter does this in
  `strip_playground_doc_blocks` — port the same filter).
- **Versioning:** `truapi` pub package version tracks the crate/`@parity/truapi` version
  (extend `scripts/sync-cargo-version.mjs` / changeset flow to bump `pubspec.yaml`).

---

## 5. Proposed repository layout

Mirror `js/packages/` with a `dart/` tree (single client package for v1):

```
dart/
  truapi/                          # pub package: `name: truapi`
    pubspec.yaml
    analysis_options.yaml
    lib/
      truapi.dart                  # barrel (exports runtime + generated)
      src/
        scale.dart                 # hand-written SCALE codec (port of scale.ts)
        result.dart                # sealed Result<Ok,Err>
        transport.dart             # Provider interface + frame encode/decode (port of transport.ts)
        transport_impl.dart        # createTransport: correlation, subs, auto-handshake (port of client.ts)
        providers/
          loopback_provider.dart   # in-memory pipe for tests
          message_port_provider.dart  # Flutter Web (dart:js_interop) — parity w/ TS
        generated/                 # GIT-IGNORED, emitted by truapi-codegen --dart-output
          types.dart
          codecs.dart              # (or fold codecs into types.dart, mirroring ts)
          client.dart
          wire_table.dart
          index.dart
    test/
      scale_test.dart
      wire_vectors_test.dart       # golden cross-language parity
      transport_test.dart
  # (future) truapi_host/          # Dart host dispatcher — Phase 6
```

Generated Dart is **git-ignored** like the TS `generated/`, and produced by `scripts/codegen.sh`.
(If consumers must `pub get` without a Rust toolchain, we can optionally commit a generated snapshot
later — decision D6.)

`truapi-codegen` changes:

```
rust/crates/truapi-codegen/src/
  main.rs        # add --dart-output (+ later --dart-host-output) flags
  dart.rs        # NEW: Dart emitter (peer of ts.rs)
  dart/          # NEW: submodules if it grows (mirrors ts/{examples,playground,explorer}.rs)
```

---

## 6. Key design decisions (recommendations + rationale)

| # | Decision | Recommendation | Why / alternatives |
|---|---|---|---|
| **D1** | **Transport target** — how does a Flutter product reach the host? | **RESOLVED: native bridge.** Native Flutter (iOS/Android/desktop) reaches the host over a **new host-side transport** (not the web `postMessage` path). The `Provider` interface is unchanged; a native `Provider` carries raw SCALE frame bytes over the chosen channel. Requires host-side coordination (a matching native endpoint). See §6.1 for channel options. | Everything above the `Provider` interface stays transport-agnostic; only the provider + the host's transport endpoint are new. |
| **D2** | **SCALE library** | **Hand-roll combinator codecs** in `scale.dart` (port of `scale.ts`, ~300 lines). | The primitive set is tiny and fixed; combinators (`struct/vector/option/enum/indexedTaggedUnion`) give exact wire control and zero heavy deps. Alt: `package:polkadart_scale_codec` is mature but registry/metadata-oriented — a different model than the combinator codecs we need to mirror. |
| **D3** | **`Result` type** | **Hand-rolled Dart 3 `sealed class Result<Ok,Err>`** with `Ok`/`Err`. | Idiomatic pattern matching, no dependency. Alt: `package:fpdart`/`result_dart` add API surface and a dep for a 30-line type. |
| **D4** | **`Vec<u8>` / `[u8; N]` representation** | **`Uint8List`** in public API + `hex`/`fromHex` helpers. | More idiomatic and efficient than the TS `HexString`. The wire bytes are identical; only the in-memory surface differs. (If strict TS API parity matters for some consumer, expose hex getters.) |
| **D5** | **Subscriptions** | **`Stream<T>`** with `StreamController(onCancel: sendStop)`. | First-class Dart; replaces ES-Observable interop. Typed interrupt → `SubscriptionInterrupted<Reason>`; empty interrupt → `onDone`. |
| **D6** | **Commit generated Dart?** | **Git-ignore** (match TS), regenerate in `codegen.sh`/CI. Optionally commit a snapshot later if we publish to pub.dev. | Consistency with the existing convention; avoids stale generated noise in diffs. |
| **D7** | **Immutable class generation** | **Emit plain classes** (final fields, const ctor, `==`/`hashCode`, `copyWith`) — no `freezed`. | We already own the generator; a second build_runner toolchain is unjustified churn. |
| **D8** | **Host (dispatcher) target** | **DONE (Phase 6).** Shipped in the same `dart/truapi` package via `package:truapi/host.dart`. | A Flutter host app needs the host side; it reuses the client's `types.dart`, so no separate package. Handlers take/return inner (selected-version) types. |

### 6.1 Native bridge transport (D1 = native)

The decision is **native bridge**: native Flutter products reach the host over a real
bidirectional byte channel, not the browser `postMessage` pipe. The `Provider` contract is
identical to the web case (`postMessage(Uint8List)`, `subscribe(cb)`, `subscribeClose(cb)`,
`dispose()`) — it simply transports raw SCALE wire frames over a native channel. **The host
must expose a matching endpoint**, so this requires coordination with the host/dotli team (a
new host capability beyond today's webview `postMessage` bridge).

Candidate channels (pick one in Phase 0, with the host team):

| Channel | When it fits | Dart side | Host side | Notes |
|---|---|---|---|---|
| **Flutter `BasicMessageChannel<ByteData>`** (BinaryCodec) | Host is the **native shell embedding the Flutter engine** | `BasicMessageChannel` provider (needs `package:flutter`) | platform-side channel handler | Cleanest if the host owns the Flutter embedding; the only option that pulls in the Flutter SDK, so keep it in a thin optional sub-package, not the pure-Dart core |
| **Local socket / WebSocket** (`dart:io` / `web_socket_channel`) | Host and product are **separate processes** on device/desktop | socket provider (pure Dart) | host listens on a local port/UDS | Process-isolated; works for desktop and sidecar models; needs a framing/auth handshake |
| **stdio / named pipe** | Host **spawns** the product (CLI/desktop) | `dart:io` stdin/stdout provider | host pipes | Simple for spawned children; length-prefix the frames |

Whichever is chosen, the wire **frame format is unchanged** (`[requestId: SCALE str][u8 id][payload]`),
so the codec, codegen, transport correlation, and parity vectors are all identical to the web path —
only the `Provider` implementation and the host's transport endpoint differ. Recommendation: prototype
against a `LoopbackProvider` first (Phases 1–4 need no real channel), then implement the chosen native
provider once the host endpoint exists.

---

## 7. Phased plan

Each phase is independently shippable and verifiable.

### Phase 0 — Spike & decisions (de-risk)
Confirm the rustdoc-JSON → Dart path end-to-end on a *single* method before building the full emitter,
and resolve the transport question (D1).
- Stand up an empty `dart/truapi` pub package (`pubspec.yaml`, `analysis_options.yaml`, CI lint).
- Hand-write `scale.dart` for the **primitive subset actually used** (bool, u8/16/32, u64/128 as BigInt,
  str, bytes, vector, option, struct, enum/taggedUnion, indexedTaggedUnion, compact, optionBool).
- Hand-write a `LoopbackProvider` + minimal `transport.dart`.
- Manually transcribe **one** request method (e.g. `System::handshake`) and **one** subscription, encode a
  payload, and **diff the bytes against the TS client / a Rust-emitted vector**. Lock the frame format.
- **Pick the native channel** (§6.1: `BasicMessageChannel` vs socket/WebSocket vs stdio) **with the host
  team**, and confirm the host can expose the matching endpoint. (D1 is already resolved to *native*.)

### Phase 1 — SCALE runtime (hand-written, fully tested)
Production `scale.dart` with parity tests.
- Implement every combinator the emitter will reference; match `scale-ts`/`parity_scale_codec` byte output.
- `compact` (4 modes incl. big-int), `OptionBool`, fixed/var byte arrays, `Result`, `Tuple`,
  `indexedTaggedUnion` (index = n−1), `lazy` (recursive codecs).
- Golden tests: a corpus of `(type, value, hex)` vectors generated from Rust/TS, asserted in Dart.

### Phase 2 — Dart emitter in `truapi-codegen`
`dart.rs` peer to `ts.rs`, driven by the existing IR.
- `--dart-output` flag in `main.rs`; `ts::generate`-shaped `dart::generate`.
- Emit `types.dart` (data classes, enums, sealed unions, `Versioned<T>`, codecs), `wire_table.dart`
  (request/subscription frame-id constants), `client.dart` (service `XxxClient` classes + `TrUApiClient`
  facade + `createClient`), `index.dart` barrel.
- Port the version-selection logic (`method_wire_version`, `selected_public_aliases`,
  `versioned_wrapper_emit_versions`) verbatim in behavior.
- Port doc-comment forwarding + ` ```ts ` block stripping.
- Reserved-word/identifier sanitization for Dart.
- Unit tests on emitter output (snapshot of generated Dart for a fixtured mini-API).

### Phase 3 — Transport & providers
Port `transport.ts` + `client.ts` semantics to Dart.
- `Provider` interface; `transport.dart` frame encode/decode (`scanStrEnd`, etc.).
- `createTransport`: `request` correlation (`Future<Result>`), `subscribeRaw` → `Stream`,
  **auto-handshake** to inbound `host_handshake_request`, idempotent `dispose`, close propagation.
- `LoopbackProvider` (tests) and the **native bridge provider** selected in §6.1 (`BasicMessageChannel` /
  socket / stdio). The native channel can land after the loopback path is green, in lockstep with the
  host-side endpoint.
- Transport unit tests (request/response, subscription receive/interrupt/stop, close races).

### Phase 4 — End-to-end parity & conformance
Make "Rust is the source of truth" enforceable.
- **Golden wire-vector corpus**: extend the Rust/TS test tooling to emit canonical SCALE bytes for
  representative request/response/subscription payloads across all 14 services; assert byte-identical
  encode **and** decode in Dart (`wire_vectors_test.dart`).
- Cross-client smoke: drive the Dart client against the existing `@parity/truapi-host` dispatcher (or a
  recorded transcript) over a loopback, exercising one method per service.
- Mirror the TS "wire-equality" + "wire-table-loop" smoke tests in Dart.

### Phase 5 — Build integration, CI, packaging, docs
- Add Dart generation to `scripts/codegen.sh` and a `make dart` / extend `make codegen`.
- CI job: regenerate Dart + `dart analyze --fatal-infos` + `dart test`; **fail if generated Dart drifts**
  from committed source intent (regen-and-diff check, same spirit as TS).
- `pubspec.yaml` version sync with the crate (extend `sync-cargo-version.mjs` / changeset flow).
- README for `dart/truapi` (install, `createClient`, request + subscription examples), and update
  top-level `README.md` + `CLAUDE.md` layout sections (required by repo convention).
- (If publishing) pub.dev metadata, example app, `CHANGELOG.md`.

### Phase 6 — (Optional) Dart host dispatcher
Only if a Dart/Flutter **host** is needed.
- Port the `truapi-host` generated surface (`generate_host` / `generate_host_server` in `ts.rs`):
  typed handler interfaces per service, dispatch table keyed by wire id, decode→handle→encode,
  subscription frame ports.
- `--dart-host-output` flag + `dart/truapi_host` package.

---

## 7.1 Implementation status

**Client AND host complete and verified.** Phases 0–6 done. Verified locally: `cargo build --workspace` + `cargo test --workspace` green, `cargo +nightly fmt --check` + `cargo +nightly clippy -- -D warnings` clean; Dart `dart analyze --fatal-infos` clean, `dart format --set-exit-if-changed` clean, `dart test` = **35 tests green** (incl. the generated client ↔ generated host round-trip).

**Host (Phase 6):** `package:truapi/host.dart` — implement the per-service `TruapiHostHandlers` and wire them with `createTruapiServer(provider, handlers)`. Handlers receive/return the same inner `types.dart` the client uses; the generated dispatch entries + `lib/src/host/host_server.dart` runtime handle versioned wrapping, SCALE codec, and the frame lifecycle. Generated by `truapi-codegen --dart-host-output`.

What exists now:

- **Runtime** (`dart/truapi/lib/src/`): `scale.dart` (SCALE codec incl. `Unit`, `vectorFixed`, `versioned`), `result.dart` (sealed `Result`/`Ok`/`Err`), `transport.dart` (`Provider`, frame codec, `createTransport`, `subscribeStream`, `SubscriptionInterrupted`, injectable `HandshakeResponder`), `providers/loopback_provider.dart`, barrel `lib/truapi.dart`.
- **Emitter** (`rust/crates/truapi-codegen/src/dart.rs`, wired via `--dart-output` in `main.rs`): generates `types.dart`, `wire_table.dart`, `client.dart`, `index.dart` (~7.9k lines: 14 service clients, ~64 methods, ~360 types). Handles structs, Dart enums (unit-only), sealed classes (mixed enums), typedef aliases, generic `Component<P>`, versioned-wrapper selection, and the auto-handshake responder.
- **Parity** (`dart/truapi/test/`): `wire_vectors_test.dart` asserts the generated Dart codecs are **byte-identical to `parity_scale_codec`** using golden vectors from `rust/crates/truapi/examples/wire_vectors.rs`; `generated_client_test.dart` round-trips Ok + typed Err through the generated `TruapiClient` over `LoopbackChannel`. Plus `scale_test.dart` (16) and `transport_test.dart` (5).
- **Build/CI** (Phase 5): `--dart-output` in `scripts/codegen.sh` (+ best-effort `dart format`), a `make dart` target, a `dart` CI job in `.github/workflows/ci.yml` (regen → `dart format --set-exit-if-changed` → `dart analyze --fatal-infos` → `dart test`), `pubspec.yaml` kept in lockstep by `scripts/sync-cargo-version.mjs`, `dart/truapi/README.md`, and updated top-level `README.md` + `CLAUDE.md`.

**Remaining (not blocking use):** pick the §6.1 native channel and stand up the host-side endpoint (the one external dependency — everything above `Provider` is done and proven on `LoopbackChannel`); broaden the golden-vector corpus toward all 14 services; optional Phase 6 Dart host dispatcher.

## 8. Implementation checklist

### Phase 0 — Spike & decisions
- [x] Create `dart/truapi` package skeleton (`pubspec.yaml`, `analysis_options.yaml`, `lib/`, `test/`)
- [x] CI/lint runs `dart format --set-exit-if-changed` + `dart analyze` (the `dart` CI job)
- [x] Minimal `scale.dart` (primitive subset + struct/vector/option/enum)
- [x] `LoopbackProvider` + `transport.dart`
- [x] Handshake + subscription proven via byte diff vs Rust (`wire_vectors`) + e2e (`generated_client_test`)
- [x] **D1 resolved: native bridge** (Flutter products reach the host over a native channel)
- [ ] **Native channel chosen** (§6.1) + host confirms it can expose the matching endpoint
- [x] D2–D7 confirmed (or amended) in this doc

### Phase 1 — SCALE runtime
- [x] `bool, u8, u16, u32, i8, i16, i32`
- [x] `u64, u128, i64, i128` as `BigInt`
- [x] `compact` (all length modes, incl. big-int mode)
- [x] `str`, `bytes()` (Vec<u8>), `bytesFixed(N)` ([u8; N])
- [x] `vector(c)`, `option(c)`, `tupleN(...)` / record codecs, `unit`
- [x] `result(ok, err)`, `optionBool`, `lazy`, `versioned` (index = n−1)
- [~] struct / unit-enum / mixed-enum codecs — emitted inline per-type by `dart.rs` (Phase 2) rather than as generic combinators
- [x] `scale_test.dart` green (16 tests, incl. canonical SCALE vectors: compact, str, options)
- [ ] Golden cross-language `(type, value, hex)` corpus (Phase 4)

### Phase 2 — Dart emitter (`dart.rs`)
- [x] `--dart-output` flag in `main.rs`; `dart::generate` entry
- [x] `types.dart`: immutable data classes (final fields, const ctor, value `==`/`hashCode`, `toString`). copyWith intentionally omitted — nullable-field unset ambiguity; DTOs are immutable and constructed directly.
- [x] `types.dart`: Dart `enum` for unit-only, `sealed class` (+ exhaustive `switch` codec) for mixed enums
- [x] Versioned wrappers handled via `S.versioned(n-1, innerCodec)` inline (public type = stripped inner), not a separate `Versioned<T>` type — matches the TS `.value`-stripping surface
- [x] `wire_table.dart`: `RequestFrameIds`/`SubscriptionFrameIds` consts, id inference ported from `ts.rs`
- [x] `client.dart`: `XxxClient` classes, `TruapiClient` facade, `createClient` (+ auto-handshake responder)
- [x] Port version selection (`method_wire_version`, max-≤-target inner selection)
- [x] Doc-comment forwarding + ` ```ts ` block stripping
- [x] Dart reserved-word / identifier sanitization + generic `Component<P>` codec functions
- [x] Validated by generated output compiling clean (`dart analyze`) + Phase 4 parity/e2e tests (in lieu of Rust snapshot unit tests)

### Phase 3 — Transport & providers
- [x] `Provider` interface + `transport.dart` frame encode/decode
- [x] `createTransport`: request correlation → `Future<Result<Ok,Err>>`
- [x] Subscription lifecycle → `Stream<T>` (`subscribeStream` helper: receive/interrupt/stop, `onCancel` → stop frame, typed `SubscriptionInterrupted`)
- [x] Auto-handshake mechanism — injectable `HandshakeResponder` (generated client wires it in Phase 2)
- [x] Idempotent `dispose` + provider-close propagation
- [x] `LoopbackProvider` (done) + native bridge provider (§6.1) — host endpoint coordinated (pending)
- [x] `transport_test.dart` — 5 tests green (request Ok/Err, subscription receive+interrupt, stop frame, auto-handshake)

### Phase 4 — Parity & conformance
- [x] Golden wire-vector corpus emitted from Rust (`cargo run -p truapi --example wire_vectors` → `dart/truapi/test/wire_vectors.json`, 11 representative vectors covering struct/Vec<u8>/Option/sealed+unit enum/compact/OptionBool/`[u8;32]`/versioned envelope)
- [x] `wire_vectors_test.dart`: Dart generated codecs **byte-identical to `parity_scale_codec`** (encode); decode covered by `scale_test.dart` round-trips
- [x] Generated `TruapiClient` ↔ fake host over `LoopbackChannel`, Ok + typed Err round-trips (`generated_client_test.dart`)
- [~] Broaden golden corpus toward all 14 services (current set is representative; add more vectors as needed)

### Phase 5 — Build, CI, packaging, docs
- [x] Dart generation added to `scripts/codegen.sh` (+ best-effort `dart format`) + `make dart` target
- [x] CI: `dart` job regenerates + `dart format --set-exit-if-changed` + `dart analyze --fatal-infos` + `dart test` (drift-safe: generated from source each run)
- [x] `pubspec.yaml` version sync via `scripts/sync-cargo-version.mjs`
- [x] `dart/truapi/README.md` with client + subscription examples
- [x] Update top-level `README.md` and `CLAUDE.md` layout (repo convention)

### Phase 6 — Dart host dispatcher
- [x] `--dart-host-output` flag (host lives in the same `dart/truapi` package, exported via `package:truapi/host.dart` — no separate package needed since it reuses the client's `types.dart`)
- [x] Hand-written dispatcher runtime `lib/src/host/host_server.dart` (`createHostServer`, `CallContext`, `SubscriptionFramePort`, request/subscription entries, pending/active subscription state machine — port of `truapi-host/src/index.ts`)
- [x] Generated `host.dart`: per-service typed handler interfaces, public `build<Service>Entries`, `TruapiHostHandlers` + `createTruapiServer`
- [x] Host dispatcher tests: generated client ↔ generated host over `LoopbackChannel` (request round-trip + subscription) — `host_server_test.dart`
- [x] One-shot host scaffold: `--dart-host-scaffold-output` (+ `make dart-scaffold`) emits `example/host_scaffold.dart` — a `ScaffoldHostHandlers` implementing every service method with `throw UnimplementedError(...)`, with heuristic backing notes (smoldart light-client vs host-local vs wallet)

---

## 9. Parity / conformance strategy (how Rust stays the source of truth)

1. **Single IR, two emitters.** Dart and TS are both derived from the same rustdoc JSON. A Rust trait
   change regenerates both in one `codegen.sh` run; neither can silently diverge in *shape*.
2. **Golden wire vectors** lock the *bytes*. The corpus is produced from the Rust/TS side and checked
   into the test suite; Dart must encode and decode each vector identically. A codec bug surfaces as a
   failing byte diff, not a runtime mystery.
3. **Drift check in CI.** Regenerate Dart and fail if it differs from intent (same guard the TS client
   uses). Stale generated code cannot merge.
4. **Append-only wire ids** (existing invariant) mean older Dart consumers stay compatible across
   protocol revisions, just like TS.

---

## 10. Open decisions needing input

- **D1 — RESOLVED: native bridge.** Native Flutter (iOS/Android/desktop) reaches the host over a new
  host-side transport. **Remaining sub-decision:** which native channel (§6.1 — `BasicMessageChannel`,
  local socket/WebSocket, or stdio) and confirming the **host team** can stand up the matching endpoint.
  This is the main external dependency: Phases 1–4 proceed on a `LoopbackProvider`, but a real
  end-to-end demo needs the host's native transport to exist.
- **Publishing:** internal package (path/git dependency) vs pub.dev publication? Affects D6 (commit
  generated code) and packaging tasks in Phase 5.
- **Client vs host scope:** confirm host (Phase 6) is out of scope for the first delivery.
- **Minimum Dart/Flutter SDK** target (Dart 3 sealed classes + records assumed; confirm ≥ Dart 3.0).

---

## 11. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Native bridge (D1) depends on the host exposing a new transport endpoint | Decouple: Phases 1–4 run on `LoopbackProvider`; coordinate the §6.1 channel + host endpoint early so the native provider lands without blocking codec/codegen work |
| SCALE byte drift (e.g. `compact`, `u128`, enum index) | Golden cross-language vectors in Phase 1/4; byte-diff in CI |
| rustdoc JSON format changes break the parser | Already a shared risk with TS; the IR is the same code path, so TS CI catches it |
| `int` truncation for `u64/u128` | Mandate `BigInt` for 64/128-bit; lint/test for it |
| Generated-code lint noise | `// ignore_for_file:` header + exclude `generated/` from `dart format` diff gate |
| Two version sources (crate vs pubspec) drift | Extend `sync-cargo-version.mjs`/changeset flow to bump `pubspec.yaml` |

---

## 12. Appendix — key source references

- IR / rustdoc parsing: `rust/crates/truapi-codegen/src/rustdoc.rs`
- TS emitter (template for `dart.rs`): `rust/crates/truapi-codegen/src/ts.rs`
  (`generate`, `generate_client`, `generate_types`, `generate_wire_table`,
  `codec_expr_mode`, `ts_type_with_named`, `method_wire_version`)
- CLI / flags: `rust/crates/truapi-codegen/src/main.rs`
- `#[wire]` / `versioned_type!` macros: `rust/crates/truapi-macros/src/lib.rs`
- Hand-written TS runtime to port: `js/packages/truapi/src/{scale,transport,client,index}.ts`
- Host dispatcher (Phase 6 reference): `js/packages/truapi-host/src/index.ts`,
  `ts.rs::generate_host` / `generate_host_server`
- Framework types skipped by codegen: `rust/crates/truapi/src/lib.rs`
- Service traits + super-trait: `rust/crates/truapi/src/api/*.rs` (`mod.rs` = `TrUApi`)
- Build pipeline: `scripts/codegen.sh`, top-level `Makefile`
```
