# Typed host-callbacks across every platform

Status: proposed
Scope: `truapi-platform`, `truapi-codegen`, `truapi-server` (wasm + native), `@parity/truapi-host-wasm`, dotli, the iOS/Android host packages.

## Problem

A host (dotli web shell, iOS, Android) implements a *typed* capability surface, but the
core delivers callback payloads as **SCALE-encoded bytes** at every binding boundary, so a
bytes→typed adapter is wedged into each host:

- **Web:** `WasmPlatform` (in `truapi-server/src/wasm.rs`) holds the typed Rust request,
  `.encode()`s it to a `Uint8Array`, and calls the JS callback. `createWasmRawCallbacks`
  (`js/packages/truapi-host-wasm/src/typed-callbacks.ts`, ~257 lines, hand-written) then
  SCALE-*decodes* it straight back into the typed object the host wanted. dotli wraps its
  typed handlers in `createWasmRawCallbacks(...)` at `bridge.ts:610`.
- **Native:** the `#[uniffi::export(callback_interface)] trait HostCallbacks`
  (`truapi-server/src/native.rs`) takes `Vec<u8>` for every rich payload; `CallbackPlatform`
  re-encodes the typed value to bytes. Swift/Kotlin receive opaque `Data`/`ByteArray` and
  forward them to a WebView running `@parity/truapi` just to decode them.

So a value the core already holds as a typed Rust struct is encoded, shipped as bytes, and
decoded again on the far side — once per platform, in hand-written glue. The decode is the
only reason a native host needs a SCALE codec at all.

This is pure overhead: the wire protocol that *must* be SCALE is **product ↔ core**, not
**core ↔ host**. The callback boundary is internal and free to carry native types.

## Goal

Each binding layer presents the `truapi-platform` traits as **native typed callbacks**, and
the host implements them directly. No `createWasmRawCallbacks`, no dotli bridge wrap, no
`Data`/`ByteArray` forwarding, no host-side SCALE codec. `truapi-codegen` owns the typed
surface and the per-platform marshaling so none of it is hand-maintained.

Non-goal: changing the **product ↔ core wire**. That stays SCALE, versioned, and
language-agnostic (it is the sandboxed-dapp contract). Only the **core ↔ host callback**
representation changes.

## Principle: one typed source, projected per platform

`truapi-platform`'s traits are the single typed source of truth. Each platform's binding
generator projects them into native types from the same Rust definitions:

```
                       truapi-platform traits  (typed Rust: HostDevicePermissionRequest, ...)
                                  │
        ┌─────────────────────────┼─────────────────────────┐
        ▼                         ▼                          ▼
   wasm-bindgen              UniFFI                     truapi-codegen
   typed JsValue        uniffi::Record/Enum           generated TS interface
   (web host)           (Swift / Kotlin host)         (host author types)
```

The product↔core wire stays SCALE on both transports (MessagePort/WebSocket frames).

## Callback inventory and depth of change

Three classes, from the platform-trait audit (`truapi-platform/src/lib.rs`):

| Class | Callbacks | Platform trait today | Change needed |
|-------|-----------|----------------------|---------------|
| **Already typed in Rust** | `device_permission`, `remote_permission`, `push_notification`, `feature_supported`, `subscribe_theme`, `auth_state_changed`, `cancel_notification` | typed (`HostDevicePermissionRequest`, …) | **binding layer only** — stop re-encoding to bytes; project the typed value |
| **Opaque even in Rust** | `confirm_*` family (sign payload, sign raw, create tx, account alias, resource allocation) | `review: Vec<u8>` | **deeper** — thread the typed review struct (`HostSignPayloadData`, `ProductAccountTxPayload`, `AllocatableResource`, …) through the platform trait + core runtime |
| **Genuinely opaque** | storage read/write/clear, session read/write/clear, preimage submit/lookup, chain genesis hash | `Vec<u8>` / `String` | **none** — raw bytes the host stores or echoes, never renders. Stay bytes. |

All rich payload types derive only `Encode`/`Decode` with no unbounded generics, so they map
cleanly to `uniffi::Record`/`uniffi::Enum` and to the existing TS shapes (verified against
`v01/permissions.rs`, `v01/notifications.rs`, `v01/signing.rs`, `v01/transaction.rs`,
`v01/resource_allocation.rs`).

## Web (wasm-bindgen)

**Decision — keep SCALE on the wasm callback boundary; codegen *emits* the JS decode
adapter.** The JS side already carries the SCALE codec: the product↔core wire needs it, and
`@parity/truapi` already ships codegen-emitted `.dec`/`.enc` for exactly these `v01` types.
The callback payloads *are* those wire types, so a thin generated decoder (`.dec` → typed
handler → encode result) reuses tested infrastructure, leaves `wasm.rs` shipping
`request.encode()` essentially unchanged, and introduces no second representation of any type.

This is "move today's hand-written `typed-callbacks.ts` into `truapi-codegen`." Rejected
alternative — Rust→`JsValue` converters (generalizing the hand-rolled `auth_state_to_js`):
that builds a parallel conversion layer in `wasm.rs` that must shape-match the TS types, more
code and more drift surface, for a purely cosmetic "no bytes on the boundary" win. Reusing the
codec the JS client already has is cleaner and lower-risk.

`createWasmProvider` applies the generated adapter *internally*, so the host passes its typed
`HostCallbacks` object directly — `createWasmRawCallbacks` and the dotli `bridge.ts:610` wrap
are both deleted; `WasmRawCallbacks` becomes an internal generated type. Subscriptions
(`subscribe_theme`, `subscribe_session_store`, `lookup_preimage`) keep the `sendItem` push
model — that plumbing (`driveResultStream`, `JsSubscriptionStream`) is genuine runtime
support, not an adapter, and stays as a small hand-written module the generated code calls.

The asymmetry with native is deliberate and principled: **web reuses SCALE because JS has the
codec; native uses typed mirrors because Swift/Kotlin do not.** Same goal (host implements
typed callbacks, no hand-rolled glue), realized with each platform's cheapest mechanism.

### Symmetric SCALE boundary (enables a uniform generated adapter)

Today the WASM raw boundary is *asymmetric*: the request crosses as SCALE bytes but the
response comes back as a raw scalar (`invoke_bool` → `bool`, `invoke_u32` → `number`), and the
core re-wraps it (`HostDevicePermissionResponse { granted }`). Reproducing that in codegen
would require the generator to know each response type's single field (`.granted`, `.id`) —
but those response types live in `@parity/truapi`, not in the platform crate the generator
parses, so it cannot see them.

Make the boundary **symmetric**: every raw callback exchanges the SCALE-encoded `ok` type in
both directions (`bool` and structs alike encode via SCALE; `()` carries nothing; streams
carry encoded items). `wasm.rs` decodes the response bytes into the typed `ok`. The generated
adapter then has one uniform shape, derivable purely from the trait signature:

```ts
devicePermission: async (payload) =>
  HostDevicePermissionResponse.enc(await host.devicePermission(HostDevicePermissionRequest.dec(payload))),
```

Cost: `wasm.rs`'s `invoke_*` helpers and the `WasmRawCallbacks` return types change to
bytes. That is the e2e-risk surface, so it lands and is verified before `confirm_*`/native.

### confirm_* union review types

`confirm_sign_payload` and `confirm_sign_raw` are each invoked from **two** runtime sites with
**different** v01 types (standard `HostSignPayloadRequest` and
`HostSignPayloadWithLegacyAccountRequest`), unified today only because both are erased to
`Vec<u8>`. To type them, introduce a review enum per confirm method, e.g.

```rust
pub enum SignPayloadReview { Standard(HostSignPayloadRequest), LegacyAccount(HostSignPayloadWithLegacyAccountRequest) }
```

The runtime wraps `inner` in the appropriate variant instead of `inner.encode()`; the
`UserConfirmation` trait takes the review type; `wasm.rs`/`native.rs` project it. The single-
site confirms (`confirm_create_transaction` → `ProductAccountTxPayload`,
`confirm_resource_allocation`, `confirm_account_alias`) take their v01 type directly.

## Native (UniFFI)

Generalize the existing `AuthState`/`SessionUiInfo`/`HostTheme` mirror pattern in `native.rs`
to every rich payload: a `uniffi::Record`/`uniffi::Enum` mirror of the `v01` type plus an
`impl From`, used directly in the `HostCallbacks` trait signature in place of `Vec<u8>`.
`CallbackPlatform` converts the typed platform value into the mirror instead of `.encode()`.
UniFFI then generates idiomatic Swift enums/structs and Kotlin data classes; the
`HostBridge` protocol/interface becomes typed and the `Data`/`ByteArray` forwarding is
removed. The Swift/Kotlin `HostCallbackAdapter` pass-throughs become typed pass-throughs.

**Decision — generate the mirrors:** `truapi-codegen` emits the `uniffi::Record`/`Enum`
mirror types + `From` impls into `truapi-server`, so `native.rs` no longer hand-maintains
them. (If emitting Rust into another crate proves awkward in step 1, the fallback is to keep
the mirrors hand-written in `native.rs` following the proven existing pattern — this does not
block the web path or the definition of done.)

## Codegen changes (the heart of "codegen exposes rich callbacks per platform")

`truapi-codegen` already parses `truapi-platform` rustdoc JSON and emits the typed TS
`HostCallbacks` interface (`ts/host_callbacks.rs`, `platform.rs`). Extend it to also emit:

1. **TS interface**: the typed `HostCallbacks` interface (already emitted); unchanged.
2. **TS adapter (web)**: the SCALE-decode adapter (today's hand-written `typed-callbacks.ts`),
   emitted from the trait surface — `.dec` the payload, call the typed handler, encode the
   result. Consumed internally by `createWasmProvider`.
3. **native mirrors**: `uniffi::Record`/`Enum` + `From` for each rich callback type, consumed
   by `native.rs`.

Swift/Kotlin types themselves remain UniFFI-generated (not codegen) — UniFFI already projects
the mirror types into both languages.

## What stays SCALE

- product ↔ core wire frames (MessagePort/WebSocket) — unchanged.
- the genuinely-opaque callback payloads (storage/session/preimage bytes, genesis hash).

## PR distribution (force-push authorized)

| Change | Lands in |
|--------|----------|
| platform trait: typed `confirm_*` reviews | PR1 `01-core-runtime` |
| codegen: emit wasm converters + native mirrors | PR1 |
| `truapi-server` wasm.rs + native.rs typed marshaling | PR1 |
| `@parity/truapi-host-wasm`: delete `createWasmRawCallbacks`, collapse `WasmRawCallbacks` | PR1 |
| regenerated TS + generated Rust | PR1 |
| dotli: drop the adapter wrap, typed handlers (submodule commit + pointer bump) | PR1 |
| iOS/Android `HostBridge`: typed protocol/interface | PR2 `02-mobile-bindings` |
| this design note | PR3 `03-docs` |

## Verification / definition of done

`make e2e-dotli` exercises only the **web** path (dotli web shell + playground in a browser,
core as WASM). So:

- **Web path must work end-to-end** — `make e2e-dotli` green (diagnosis flow: auth,
  permissions, session, notifications, theme, storage).
- **Native path must compile and bind** — `cargo build --workspace`, `make uniffi`, and the
  host-packages Android/iOS assembles in CI. Its runtime isn't covered by `make e2e-dotli`.
- Full Rust suite (fmt/clippy/test), TS package tests, playground build, and the codegen
  golden tests all green.

## Risks

- **Shape match (web):** the generated Rust→JS converter must produce exactly what the
  generated TS type expects. Mitigated by emitting both from codegen and covering them with
  the existing golden tests.
- **`confirm_*` depth:** typing the reviews touches the core runtime that currently passes
  encoded review bytes. If this balloons, the reviews can ship typed in a follow-up while the
  already-typed callbacks land first; the web definition of done does not require the
  `confirm_*` reviews to be typed (dotli's confirm UI keeps working on the existing payload).
- **dotli is a second repo:** the host-side change is a dotli commit + submodule bump. The
  truapi-side typed surface must land first (or together) so dotli builds against it.
