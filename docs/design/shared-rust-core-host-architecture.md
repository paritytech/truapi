# Shared Rust Core Host Architecture

_Design doc for the Rust core host architecture. The initial focus is building a shared TrUAPI core: one Rust implementation of the protocol that every Triangle Host embeds._

## Goals

The shared Rust core host architecture aims to:

- **Define the protocol once.** TrUAPI semantics, wire framing, dispatch, subscriptions, permission gating, the auth/SSO state machine, session interpretation, and signing orchestration live in a single shared Rust core ([`truapi-server`](https://github.com/paritytech/truapi/tree/main/rust/crates/truapi-server)), not re-implemented per host.
- **Keep hosts thin.** A host implements only a small set of platform primitives (storage, navigation, permissions, confirmation UI, chain transport, theme) behind one capability trait set ([`truapi-platform`](https://github.com/paritytech/truapi/tree/main/rust/crates/truapi-platform)), and no protocol logic.
- **Run the same core on every platform.** The identical core embeds as WASM on the web and over UniFFI on native, so dotli, Desktop, iOS, and Android behave the same way.
- **Generate bindings from one source.** The product client and the dispatcher are generated from the one protocol definition, so wire formats cannot drift between hosts.

The payoff is one versioned, tested implementation instead of one per host: behavior stays consistent across Web, Desktop, iOS, and Android, and shared tooling (structured logs, analytics, ...) can be built once on the core rather than per host.

## 1. Background

A **Triangle Host** is a Polkadot application ([Web/dot.li](https://github.com/paritytech/dotli), [Desktop](https://github.com/paritytech/polkadot-desktop), [iOS](https://github.com/paritytech/polkadot-app-ios-v2), [Android](https://github.com/paritytech/polkadot-app-android-v2)) that embeds and runs **products**. A product is a single-page web application that the host loads in a sandbox. The product shares no memory with the host and speaks to it only over **TrUAPI**.

TrUAPI is defined canonically in the [`truapi` Rust crate](https://github.com/paritytech/truapi/tree/main/rust/crates/truapi): its methods, payload and error types, and wire ids, carried on the wire as SCALE-encoded byte frames.

### Platforms

A host's **platform** is how it embeds the core:

- **Web** hosts (dot.li, Desktop) load and run the core as WASM.
- **Native** hosts (iOS, Android) link the core in as a compiled library over UniFFI.

The platform decides how the core is loaded and how products reach it, not what the core does: the same core runs on both.

## 2. The architecture (high level)

A product emits wire frames, a thin host transport bridge shuttles those frames to the shared core.
The core executes all protocol logic, and when the core needs an OS capability it calls back out through the platform trait set.
Everything protocol-shaped lives in the core, everything OS-shaped lives in the host.

### Layered model

```
                        per-product / per-tab
+--------------------------------------------------------------+
| Product  (sandboxed iframe on web, WebView on native)        |
| speaks TrUAPI, shares no memory with the host                |
+--------------------------------------------------------------+
       |  down: request and subscription-start frames (SCALE)
       |  up:   response and subscription-item frames (SCALE)
       v
+--------------------------------------------------------------+
| Host transport bridge                    (host-owned, thin)  |
| moves frames in and out, holds no protocol logic             |
+--------------------------------------------------------------+
       |  down: product frames
       |  up:   response and subscription frames
       v
+--------------------------------------------------------------+
| Embedded shared runtime  (truapi-server)     (core-owned)    |
| framing, dispatch, subscriptions, permission gating,         |
| auth/SSO state machine, session interpretation,              |
| signing orchestration, chainHead runtime                     |
+--------------------------------------------------------------+
       |  down: capability call (the core initiates)
       |  up:   result or stream item
       v
+--------------------------------------------------------------+
| Host platform primitives (truapi-platform traits)  (host)    |
| storage, navigation, permissions, confirmation UI,           |
| auth presenter, session store, chain RPC, theme, preimage    |
+--------------------------------------------------------------+
```

### One end-to-end request flow

A product calls a method, for example `storage_read`, and the request makes one round trip across the four participants:

```
  PRODUCT            BRIDGE             CORE               HOST
  (iframe /          (transport         (truapi-           (storage
   WebView)           bridge)            server)            capability)
     |                  |                  |                  |
     | request frame    |                  |                  |
     | (SCALE)          |                  |                  |
     |----------------->|                  |                  |
     |                  | runtime ingress  |                  |
     |                  | (bytes)          |                  |
     |                  |----------------->|                  |
     |                  |          decode frame, route by     |
     |                  |          wire id, run the handler   |
     |                  |                  |                  |
     |                  |                  | capability call  |
     |                  |                  | (read storage)   |
     |                  |                  |----------------->|
     |                  |                  |          value   |
     |                  |                  |<-----------------|
     |                  |          encode response            |
     |                  |          (success or typed error)   |
     |                  |                  |                  |
     |                  | response bytes   |                  |
     |                  |<-----------------|                  |
     | response frame   |                  |                  |
     |<-----------------|                  |                  |
     |                  |                  |                  |
     v                  v                  v                  v
```

The only sideways step is the `capability call` out to the host and its `value` reply.
Subscriptions follow the same path, expanding into start, receive, interrupt, and stop frames.

### Runtime ownership

Each host embeds the shared runtime implementation and creates one isolated
runtime instance per product context. Each runtime instance owns that product's
dispatch, permissions, subscriptions, and lifecycle, while shared host services
stay behind the platform layer.

The runtime implementation is shared across hosts. Each host defines its own
transport bridge and platform capability implementation around it, but the
protocol behavior inside the runtime remains the same.

- **Host-owned:** transport bridge, platform services, and runtime
  configuration such as product identity and pairing metadata.
- **Core-owned:** product protocol state, account/session state,
  subscriptions, lifecycle decisions, and all protocol behavior.
- **Target-specific:** transport and platform capabilities can differ by host;
  the runtime contract stays the same. For example, a web host may show a
  browser modal and relay signing to an external SSO wallet, while a native
  signing host may show native confirmation UI and sign on device.

### What the host provides (platform capabilities)

A host implements one capability surface ([`truapi-platform`](https://github.com/paritytech/truapi/tree/main/rust/crates/truapi-platform)), a syscall layer for OS primitives only. That surface is:

- `Storage`: product-scoped key-value storage.
- `Navigation`: opening URLs after core-side normalization.
- `Notifications`: scheduling and cancelling host notifications.
- `Permissions`: device and remote permission prompts.
- `Features`: feature-support probing.
- `ChainProvider` and `JsonRpcConnection`: JSON-RPC transport; the core runs chainHead on top.
- `AuthPresenter`: rendering core-owned auth state transitions.
- `SessionStore`: persisting the opaque core session blob.
- `UserConfirmation`: local accept/reject review before core-owned user actions continue.
- `ThemeHost`: current host theme and future theme changes.
- `PreimageHost`: host-selected preimage submit and lookup backend.

`RuntimeConfig` is supplied alongside those traits and carries product identity, host/pairing metadata, the People-chain genesis hash, and the pairing deeplink scheme. This is the semantic boundary for both web and native hosts.

This boundary can move over time. As a host responsibility becomes common
across targets and stops needing target-specific primitives, it should move
from host callbacks into the core so the reusable cross-platform surface grows
and host code stays thin.

### Who owns what

| Concern                                                    | Core (`truapi-server`) | Host (`truapi-platform` impl)                                                             |
| ---------------------------------------------------------- | ---------------------- | ----------------------------------------------------------------------------------------- |
| Wire framing, dispatch, error wrapping                     | Yes                    | No                                                                                        |
| Subscription lifecycle (start/stop/interrupt/receive)      | Yes                    | No                                                                                        |
| Permission state machine and gating                        | Yes (the decision)     | UI prompt only                                                                            |
| Auth/SSO state machine and transitions                     | Yes                    | Renders the state                                                                         |
| SSO pairing protocol (handshake, channel derivation)       | Yes                    | No                                                                                        |
| Session encode/decode, versioning, projection              | Yes                    | Persists an opaque blob                                                                   |
| Signing orchestration (validate, gate consent, round-trip) | Yes                    | Local confirm UI                                                                          |
| The cryptographic signature itself                         | No                     | Non-signing host: no (remote SSO-peer wallet signs). Signing host: yes (on-device signer) |
| Product-account public key derivation                      | Yes                    | No                                                                                        |
| chainHead v1 runtime                                       | Yes                    | Supplies JSON-RPC transport                                                               |

## 3. Web and native embeddings

The web and native hosts embed the same runtime, but cross different host/runtime boundaries. On web, the runtime is WASM and host callbacks stay in TypeScript. On native, the runtime is linked through UniFFI and host callbacks are Swift/Kotlin.

### Topology

```
                         per product runtime instance
+--------------------------------+  +--------------------------------+
| Web host                       |  | Native host                    |
| Product iframe                 |  | Product WebView                |
| @parity/truapi frames          |  | @parity/truapi frames          |
+--------------------------------+  +--------------------------------+
       | MessageChannel                    | ws://127.0.0.1:<port>
       v                                   v
+--------------------------------+  +--------------------------------+
| TS host page / bridge          |  | Loopback WebSocket bridge      |
| browser callbacks on main      |  | token-gated native transport   |
+--------------------------------+  +--------------------------------+
       | postMessage to worker             | UniFFI binding
       v                                   v
+--------------------------------+  +--------------------------------+
| Web Worker                     |  | Native core binding            |
| truapi-server WASM runtime     |  | truapi-server native runtime   |
+--------------------------------+  +--------------------------------+
       | callbacks to TS                   | callbacks to Swift/Kotlin
       v                                   v
+--------------------------------+  +--------------------------------+
| TS platform services           |  | Swift/Kotlin platform services |
| storage, RPC, auth UI          |  | storage, signer, RPC, UI       |
+--------------------------------+  +--------------------------------+
```

The shared runtime runs as WASM in a dedicated Web Worker, so heavy work stays off the page's main thread. dotli's `Platform` implementation stays on the main thread and resolves core-initiated callbacks to browser primitives.
A product in a native WebView has no shared JavaScript realm with the host, so the host exposes a token-gated loopback WebSocket for SCALE frames. From there the path mirrors the web host: native callbacks implement `truapi-platform`, and the same shared runtime owns protocol behavior.

## 4. What changed vs the current architecture

Today each host re-implements TrUAPI protocol semantics in hand-maintained, per-language glue.
On the new core those semantics live once in the shared core, and each host is reduced to platform-primitive callbacks.

At the package level, generated [`@parity/truapi`](https://github.com/paritytech/truapi/tree/main/js/packages/truapi) replaces `host-api-wrapper`, [`truapi-codegen`](https://github.com/paritytech/truapi/tree/main/rust/crates/truapi-codegen) replaces the hand-written method table and SCALE codecs, and the embedded runtime replaces the `host-container` and `host-papp` protocol orchestration.

### Transport bridge

The transport bridge becomes a byte pipe, not a protocol layer.

- **Web:** product frames cross the `MessageChannel` to the host page and are forwarded to the Web Worker. The page main thread does not decode or interpret TrUAPI frames; the WASM runtime does that in the worker.
- **Native:** product frames go over the token-gated loopback WebSocket to the native runtime. The WebView no longer needs `container.js` to decode SCALE and translate into an ad-hoc container-to-native protocol.
- **Versioning:** products use the latest `@parity/truapi` API cut available when the npm package is published. Each method is versioned independently; the Rust runtime can understand supported versions and answer in the requested API version. Breaking changes add a new request/response variant, while untouched APIs stay stable.

### Before / after

| Aspect                     | Current (per host)                                                 | New (shared core)                                                                |
| -------------------------- | ------------------------------------------------------------------ | -------------------------------------------------------------------------------- |
| Protocol dispatch          | Hand-written per host (`container.ts` TS, `ContainerBridge` Swift) | One Rust dispatcher in `truapi-server`                                           |
| Wire codecs / method table | Hand-written TS (`@novasamatech/host-api`) and Swift DTOs          | Generated from the `truapi` crate by `truapi-codegen`                            |
| Transport bridge           | Decodes or routes host protocol in JS/container layers             | Forwards SCALE frames to the runtime                                             |
| API versioning             | Host/package versions can drift by implementation                  | Method-level versioning in `@parity/truapi` and the Rust runtime                 |
| Permission gating          | Re-coded per host                                                  | Core permission state machine                                                    |
| Auth/SSO orchestration     | `host-papp` JS, or the Swift signing stack                         | Core auth state machine and SSO protocol                                         |
| Session interpretation     | Per host                                                           | Core decode/projection; host stores an opaque blob                               |
| Host responsibility        | Full protocol implementation plus platform glue                    | Thin `truapi-platform` callbacks only                                            |

The result is one generated, versioned protocol implementation, with host-specific code limited to transport and platform capabilities.

## References

- Shared core and dotli web port: https://github.com/paritytech/truapi/pull/104
- Native/mobile scaffolding: https://github.com/paritytech/truapi/pull/215
