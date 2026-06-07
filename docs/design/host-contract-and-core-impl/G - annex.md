# G - Annex: conventions & wiring reference

> Part of the [host-contract & core-impl spec](<index.md>). Shared mechanics referenced by
> [A](<A - host-primitives.md>) (host primitives) and [B](<B - core-impls.md>) (wire methods). Read the
> relevant spec first; come here for the recipe.

## Platform trait conventions

`truapi-platform` currently defines seven capability traits (`Storage`, `Navigation`, `Notifications`,
`Permissions`, `Features`, `ChainProvider`, `JsonRpcConnection`) plus a `Platform` supertrait
blanket-impl'd over the first six (`truapi-platform/src/lib.rs:31-130`). This plan adds more traits,
including host-global `SessionStore`. A new capability trait must:

- **Be RPITIT async, not `async-trait`:** `fn name(&self, ...) -> impl Future<Output = Result<T, E>> + Send;`.
- **Be bound `: Send + Sync`** and added to the `Platform` supertrait + its blanket impl (lib.rs:122-130).
- **Use concrete `truapi::v01` / `versioned` error types** (re-exported at lib.rs:25-27), never
  `CallError`; the runtime maps them to `CallError`.
- **Reuse existing `v01` payload types** where one fits, so the byte bridge needs no new SCALE type.

## Adding a host primitive: the five layers

Use this recipe only for capabilities the Rust core **cannot** perform itself: UI presentation, OS
services, persistence, and chain connections. Do **not** use it for account reads, signing, transaction
construction, alias, product statement-store, or SSO pairing internals; those are core methods in
[B](<B - core-impls.md>) and should remove the transitional raw JS callbacks that PR 104 shipped.

```
 (1) truapi-platform/src/lib.rs        trait method  + add to Platform supertrait (lib.rs:122-130)
 (2) truapi-server/src/runtime.rs      PlatformRuntimeHost calls it via UFCS alias, maps -> CallError
 (3a) truapi-server/src/wasm.rs        JsBridge field + from_js("camelName") + WasmPlatform impl + invoker
 (3b) truapi-server/src/native.rs      HostCallbacks method + CallbackPlatform impl   (run `make uniffi`)
 (4) js .../truapi-host-wasm           WasmRawCallbacks field + createUnavailableCallbacks stub
                                       + worker-protocol CallbackName + worker-runtime forward + main dispatch
 (5) host (dotli)                      supply the callback in the object passed to the core
```

Callback round-trip across the Web Worker boundary (trace of `localStorageRead`):

```
 WASM core (in worker)              worker-runtime.ts            main thread (create-worker-host-runtime)
   raw.localStorageRead(key) --> callbackRequest(name,args) --> { kind:"callbackRequest", id, name, args }
        ^                            stores resolver in                       |
        |                            pendingCallbacks[id]            state.rawCallbacks[name](...args)
   Promise resolves <-- pendingCallbacks[id].resolve <-- { kind:"callbackResponse", id, ok, value/error }
```

PR 104's JS package still exposes byte-oriented `WasmRawCallbacks`, including transitional
`accountGet`/`signPayload`/`statementStore*`/`preimageLookupSubscribe` callbacks. Treat those as
scaffolding for unsupported methods, not the destination. For any method moved into the Rust core, delete
the corresponding raw callback name from:

- `js/packages/truapi-host-wasm/src/runtime.ts` (`WasmRawCallbacks`, `createUnavailableCallbacks`);
- `js/packages/truapi-host-wasm/src/worker-protocol.ts` (`CallbackName` / `SubscriptionName`);
- `js/packages/truapi-host-wasm/src/worker-runtime.ts` forwarding;
- `js/packages/truapi-host-wasm/src/web/create-worker-host-runtime.ts` main-thread dispatch;
- dotli's callback object.

For genuine host primitives, request callbacks pass/return raw SCALE bytes (`Uint8Array`) or `boolean`;
the typed->raw SCALE adapter is intentionally **not** in the JS package (`runtime.ts:3-11`), so a host
currently supplies byte-oriented callbacks directly. Rust invokers today include `invoke_unit` (bytes in /
void), `invoke_bool` (bytes in / bool), `invoke_local_storage_read` (key in / `Option<Vec<u8>>`), and the
`chainConnect` bridge for bidirectional JSON-RPC. Add only the minimum invoker shape required by the new
primitive.

Per-primitive checklist:

- [ ] Trait in `truapi-platform/src/lib.rs`; add to `Platform` supertrait + blanket impl (lib.rs:122-130).
- [ ] `WasmPlatform` impl (wasm.rs:95-254) + `JsBridge` field (wasm.rs:36-51) + `from_js` key (wasm.rs:54-68) + invoker.
- [ ] `HostCallbacks` method (native.rs:100-132) + `CallbackPlatform` impl (native.rs:263-374); run `make uniffi`; re-implement on iOS/Android.
- [ ] `WasmRawCallbacks` field (runtime.ts:63-98) + `createUnavailableCallbacks` stub (runtime.ts:106-138).
- [ ] `CallbackName`/`SubscriptionName` (worker-protocol.ts:12-44) + `worker-runtime.ts` forward + main `handleCallbackRequest` (create-worker-host-runtime.ts:27-73).
- [ ] Update both test stubs (runtime.rs:600-679, core.rs:151-229) so they still satisfy `Platform`.
- [ ] Wire the `truapi::api::*` method to the new trait in `runtime.rs` (the [B](<B - core-impls.md>) side).

## Implementing a core-owned wire method

For account/signing/statement-store methods, the work is different from adding a host primitive:

1. Add core state or helpers under `truapi-server/src/host_logic/` (`sso`, `statement_store`,
   `message_exchange`, `proofs`, `key_derivation`, etc.).
2. Add fields to `PlatformRuntimeHost<P>` for the shared state/service handles, not JS callbacks.
3. Override the relevant `truapi::api::*` trait method in `runtime.rs`, preserving versioned request/response
   wrappers and domain error mapping.
4. If the method needs host UI or storage, call a small platform capability from [A](<A - host-primitives.md>)
   (`PairingPresenter`, `SessionStore`, resource allocation confirmation), then continue protocol logic
   in Rust.
5. Remove the matching transitional `WasmRawCallbacks`/worker/dotli callback route.

This is the path for `request_login`, `get_account`, signing, create transaction, alias,
statement-store submit/subscribe/proof, entropy, and core-owned logout.

## Adding runtime configuration

Static values that are known when the host runtime is constructed are not platform traits. Thread them as
constructor/config state instead:

- `truapi-server/src/runtime.rs`: add a config field to `PlatformRuntimeHost<P>` and pass it through
  `new` / `TrUApiCore::from_platform`.
- `truapi-server/src/wasm.rs`: accept the config from JS when constructing the WASM core; validate byte
  lengths and URL invariants at the boundary.
- `truapi-server/src/native.rs`: expose equivalent UniFFI constructor/config fields.
- `js/packages/truapi-host-wasm`: add the config to in-process, Node, and worker runtime creation options;
  forward it in the worker init frame.
- dotli: create one runtime per top-level product iframe/container. Pass `calling_product_id`
  (`labelToProductIdentifier(label)` parity), `product_label`, `pairing_metadata_url` computed from the
  current origin using the existing host-papp rule, the People-chain genesis hash, and the production/dev
  deeplink scheme. Nested dApps are not separate Rust runtime instances for v1; if the host keeps nested
  message forwarding, route those messages through the same core/product context. Track the usefulness and
  any future independent nested-product model in [I](<I - nested-dapps.md>).

Current-parity host-container surfaces ([A3](<A - host-primitives.md>)) use the five-layer primitive
recipe above rather than runtime config:

- SessionStore is a host-global secret persistence capability for opaque core-owned bytes encoding the
  full `SessionInfo`; keep it separate from product-scoped `Storage` and do not expose typed session
  fields across JS/UniFFI. It includes a current-then-changes coarse notification stream for same-runtime
  writes/clears and cross-tab/process logout/re-pair propagation; each tick causes the core to call
  `read()`, with core-side dedupe for equivalent blobs/session ids. Wire it like other host-to-core
  streams rather than as a one-shot callback.
- Notifications need a response id plus cancel support, so the existing `Notifications` trait shape must
  change.
- Theme needs a subscription callback/channel.
- Preimage submit/lookup should stay host-side but be routed through Rust once the JS container is gone.
- Resource allocation needs a dedicated host confirmation UI callback before the core sends the
  SSO session request. Keep it separate from `Permissions::remote_permission`; it is approval UI around a
  retry-capable SSO operation, not a cached permission decision.

## Implementing a wire method: the override template

Wire methods are implemented on `PlatformRuntimeHost<P>` (`runtime.rs`). Copy this shape (from
`LocalStorage::read`, `runtime.rs:269-281`): destructure the versioned request, call the platform via the
UFCS alias, wrap the response `V1`, map the error.

```rust
async fn read(
    &self, _cx: &CallContext, request: HostLocalStorageReadRequest,
) -> Result<HostLocalStorageReadResponse, CallError<HostLocalStorageReadError>> {
    let HostLocalStorageReadRequest::V1(v01::HostLocalStorageReadRequest { key }) = request;
    PlatformStorage::read(self.platform.as_ref(), key)
        .await
        .map(|value| HostLocalStorageReadResponse::V1(v01::HostLocalStorageReadResponse { value }))
        .map_err(|err| CallError::Domain(HostLocalStorageReadError::V1(err)))
}
```

Rules:

- Platform traits are imported under aliases (`Storage as PlatformStorage`, etc., runtime.rs:68-71) to
  avoid clashing with `truapi::api::*`.
- Domain errors map to `CallError::Domain(Err::V1(inner))`; infra failures to
  `CallError::HostFailure { reason }`; not-yet-wired to `CallError::unavailable()`.
- For multi-step host logic, follow `PermissionsService` (a struct over `&` platform borrows,
  permissions.rs:36) and the `request_remote_permission` override (runtime.rs:241-258).
- New in-core state goes on `PlatformRuntimeHost<P>` (runtime.rs:75-84) + `new` (runtime.rs:90-100); new
  host-logic modules under `host_logic/` declared in `host_logic/mod.rs`.
- The `Foo::V1(inner)` destructure is irrefutable while a domain stays single-version. Multi-version
  envelopes (post-merge `versioned_type!`) use `Versioned`/`IntoLatest`/`FromLatest`; out of scope for
  the v0.1 tickets here.
