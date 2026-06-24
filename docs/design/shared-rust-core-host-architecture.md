# Shared Rust Core Host Architecture

_Design doc for the Rust core host architecture. The initial focus is building a shared TrUAPI core: one Rust implementation of the protocol that every Triangle Host embeds._

## Goals

The shared Rust core host architecture aims to:

- **Define the protocol once.** TrUAPI semantics, wire framing, dispatch, subscriptions, permission gating, the auth/SSO state machine, session interpretation, and signing orchestration live in a single shared Rust core ([`truapi-server`](https://github.com/paritytech/truapi/tree/main/rust/crates/truapi-server)), not re-implemented per host.
- **Keep hosts thin.** A host implements only a small set of platform primitives (storage, navigation, permissions, confirmation UI, chain transport, theme) behind one capability trait set ([`truapi-platform`](https://github.com/paritytech/truapi/tree/main/rust/crates/truapi-platform)), and no protocol logic.
- **Run the same core on every platform.** The identical core embeds as WASM on the web and over UniFFI on native, so dotli, Desktop, iOS, and Android behave the same way.
- **Generate bindings from one source.** The product client and the dispatcher are generated from the one protocol definition, so wire formats cannot drift between hosts.

## 1. Background

A **Triangle Host** is a Polkadot application ([Web/dot.li](https://github.com/paritytech/dotli), [Desktop](https://github.com/paritytech/polkadot-desktop), [iOS](https://github.com/paritytech/polkadot-app-ios-v2), [Android](https://github.com/paritytech/polkadot-app-android-v2)) that embeds and runs **products**. A product is a single-page web application that the host loads in a sandbox. The product shares no memory with the host and speaks to it only over **TrUAPI**, the host-to-product protocol (specified in [host-spec](https://github.com/paritytech/host-spec)).

TrUAPI is defined canonically in the [`truapi` Rust crate](https://github.com/paritytech/truapi/tree/main/rust/crates/truapi): its methods, payload and error types, and append-only wire ids, carried on the wire as SCALE-encoded byte frames.

### Platforms

A host's **platform** is how it embeds the core:

- **Web** hosts (dot.li, Desktop) load and run the core as WASM.
- **Native** hosts (iOS, Android) link the core in as a compiled library over UniFFI.

The platform decides how the core is loaded and how products reach it, not what the core does: the same core runs on both.

## 2. The architecture (high level)

A product emits wire frames, a thin host adapter shuttles those frames to the shared core.
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
| Host adapter / transport bridge          (host-owned, thin)  |
| moves frames in and out, holds no protocol logic             |
+--------------------------------------------------------------+
       |  in:  receive_frame(bytes)
       |  out: emit_frame(bytes)  via FrameSink
       v
+--------------------------------------------------------------+
| Shared Rust core  (truapi-server)            (core-owned)    |
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
  PRODUCT            ADAPTER            CORE               HOST
  (iframe /          (transport         (truapi-           (storage
   WebView)           bridge)            server)            capability)
     |                  |                  |                  |
     | request frame    |                  |                  |
     | (SCALE)          |                  |                  |
     |----------------->|                  |                  |
     |                  | receive_frame    |                  |
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
     |                  | emit_frame(bytes)|                  |
     |                  |<-----------------|                  |
     | response frame   |                  |                  |
     |<-----------------|                  |                  |
     |                  |                  |                  |
     v                  v                  v                  v
```

The only sideways step is the `capability call` out to the host and its `value` reply.
Subscriptions follow the same path, expanding into start, receive, interrupt, and stop frames.

### The core's entry point (`HostCore`)

`HostCore` is the core's single entry point: the host pushes the product's frames in and gets the core's frames back out.
This entry point is identical on every platform, so only the transport that carries the frames differs: a web host bridges the product's iframe over a `MessageChannel`, a native host bridges its WebView over a loopback WebSocket.

Both reach the same entry point:

```
  iframe in a web host      --[ MessageChannel ]----.
                                                     \
  WebView in a native host  --[ loopback WebSocket ]--+
                       two transports, one interface  |
                                                      v
            +---------------------------------------------+
            | HostCore                                    |
            |   frames in:  receive_frame(bytes)          |
            |   frames out: emit_frame(bytes)             |
            +---------------------------------------------+
                                  v
            +---------------------------------------------+
            | shared core                                 |
            +---------------------------------------------+
```

Beyond carrying frames, the host raises a couple of lifecycle signals the core reacts to:

- **session/auth lifecycle:** the host can cancel an in-progress login, log out, or signal that the stored session changed (including from another tab or process). In every case the core re-derives auth state and the host re-renders it.
- **dispose:** tear the core down when the product or tab goes away.

The core is created from the host's `Platform` implementation plus a small runtime config (the product identity and the host's pairing metadata).

### What the host provides (platform capabilities)

A host implements one capability surface ([`truapi-platform`](https://github.com/paritytech/truapi/tree/main/rust/crates/truapi-platform)), a syscall layer for OS primitives only. Account management, signing orchestration, and statement-store flows are not here; they live in the core. The capabilities a host supplies fall into a few groups:

- **OS primitives:** scoped key-value storage, opening URLs, notifications, the current theme, and feature probes.
- **Data backends:** a chain connection (JSON-RPC, with the core running chainHead on top) and a content-addressed preimage store (off-chain blobs addressed by hash). The host supplies the transport and picks the backend; the core owns the wire mapping and subscription lifecycle.
- **Auth and session:** rendering the core-owned auth state, and persisting an opaque session blob the core interprets.
- **Consent UI:** permission prompts, and local accept/reject confirmation before the core signs or asks the SSO peer to sign.

That is the whole host job.

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
| Storage, navigation, notifications, theme                  | No                     | Yes                                                                                       |
| Task spawning, frame transport                             | No                     | Yes (spawner, frame sink)                                                                 |

## 3. Non-signing host: dotli (web)

[dotli](https://github.com/paritytech/dotli) holds no keys. It runs the WASM core in a Web Worker, connects each product to it, renders a QR/deeplink for an external Polkadot Mobile wallet to scan, and lets that wallet sign over SSO. The host implements only browser-facing platform callbacks.

**Contrast with today:** dotli currently hand-maintains the full host-side protocol in TypeScript (its own `container.ts` on top of the Novasama `@novasamatech/host-*` packages). In this architecture that whole layer is a set of thin browser callbacks, and the protocol logic lives in the WASM core. Section 5 details the contrast.

### Topology

```
            per product (one core provider per product iframe)
+------------------------------------------------------------------+
| Product iframe (sandboxed)                                       |
| @parity/truapi client, emits SCALE frames                        |
+------------------------------------------------------------------+
       |  SCALE frames over a MessageChannel (port to port)
       v
+------------------------------------------------------------------+
| Host page main thread  (DOM-bound, host-owned)                   |
| MessagePort provider -> host dispatcher -> Web Worker provider   |
| typed callbacks: localStorage, modals, topbar, chain RPC         |
+------------------------------------------------------------------+
       |  postMessage across the worker boundary:
       |  frames in/out, callback request/response, sub items
       v
+------------------------------------------------------------------+
| Web Worker thread  (core-owned)                                  |
| worker runtime -> WasmHostCore -> HostCore -> the shared core    |
| WASM core; core-initiated callbacks forwarded to the main thread |
+------------------------------------------------------------------+
       core callbacks resolve, on the main thread, to browser and
       dotli primitives: shared auth storage, localStorage, DOM
       modals, smoldot light client or a curated RPC gateway
```

The WASM core runs in a dedicated Web Worker, so heavy work stays off the page's main thread; frames and callbacks cross the worker boundary by `postMessage`.
dotli's `Platform` implementation is plain JavaScript that maps each core callback onto a browser primitive: `localStorage`, DOM modals, shared auth storage, and an RPC gateway or light client for chain access.

### Signing and auth are core-owned

Because dotli holds no keys, the core runs the SSO pairing with the user's external wallet and then relays every signing request to it; dotli only shows a local accept/reject prompt and never signs. The auth and session state machine is core-owned as well: the core emits an ordered auth-state stream that the topbar renders, and it persists the session only as an opaque blob that dotli stores and never decodes. The host renders and stores; the core decides.

## 4. Signing host: iOS and Android (native)

A mobile host embeds the same core over UniFFI and exposes the same `HostCore` entry point to Swift/Kotlin callbacks.
The difference from the web host is the signing role.
The device holds the user's keys and signs locally, so there is no external wallet to pair with.
The native confirmation UI is the consent step, and a host signer produces the signature in-process.

### Topology

```
                              per product
+------------------------------------------------------------------+
| Product JS in a native WebView (WKWebView / Android WebView)     |
| @parity/truapi client, emits SCALE frames                        |
+------------------------------------------------------------------+
       |  ws://127.0.0.1:<port>/?t=<token>  (SCALE frames)
       v
+------------------------------------------------------------------+
| Loopback WebSocket bridge  (host-owned, inside the core lib)     |
| token-gated; the WebView shares no JS realm with the host        |
+------------------------------------------------------------------+
       |  in:  receive_frame(bytes)
       |  out: emit_frame(bytes)
       v
+------------------------------------------------------------------+
| Native core binding (UniFFI)  ->  HostCore  (the same core)      |
+------------------------------------------------------------------+
       |  down: capability call (UniFFI callback interface)
       |  up:   result or stream item
       v
+------------------------------------------------------------------+
| Swift / Kotlin host bridge  (native platform primitives)         |
| storage, navigation, permissions, confirmation UI, chain RPC,    |
| theme, session store, on-device signer and key custody           |
+------------------------------------------------------------------+
```

A product in a native WebView has no shared JavaScript realm with the host, so there is no `MessageChannel`.
The host runs a loopback WebSocket server on `127.0.0.1`, and the WebView dials in with a one-time token in the query string as the auth gate.
From there the path mirrors the web host: a native callback interface implements every `truapi-platform` trait, and the core is created the same way as on web.
Web and native differ in the transport and the callback language, not in the core.

### Signing flow

Because the device holds the keys, the signing path resolves locally instead of pairing with a remote wallet:

```
  PRODUCT               CORE                  HOST
  (WebView)             (truapi-server)       (consent UI +
                                               on-device signer)
     |                     |                     |
     | sign request frame  |                     |
     |-------------------->|                     |
     |            validate, gate permission,     |
     |            prepare the review             |
     |                     |                     |
     |                     | show consent UI     |
     |                     |-------------------->|
     |                     |            accept / reject
     |                     |                     |
     |                     |          on accept: on-device
     |                     |          signer + key custody
     |                     |          produce the signature
     |                     |                     |
     |                     | signature           |
     |                     |<--------------------|
     |            assemble the signed response   |
     |                     |                     |
     | response frame      |                     |
     |<--------------------|                     |
     |                     |                     |
     v                     v                     v
```

This is the contrast with the web host.
A non-signing host relays the prepared request to a remote wallet over SSO and returns the wallet's signature.
A signing host resolves consent and signs in-process behind its own signer capability.
Both use the same core, the same dispatch, and the same consent step, and differ only in where the signature is produced.

## 5. What changed vs the current architecture

Today each host re-implements TrUAPI protocol semantics in hand-maintained, per-language glue.
On the new core those semantics live once in the shared core, and each host is reduced to platform-primitive callbacks.

### The current "glue" layer (triangle-js-sdks)

The glue layer, [`triangle-js-sdks`](https://github.com/paritytech/triangle-js-sdks), is the `@novasamatech/host-*` package family.

- **Shared protocol and transport:** `@novasamatech/host-api`. It hand-writes the SCALE codecs and the method table (around 57 `versionedRequest`/`versionedSubscription` entries) plus the transport and provider interface.
- **Host-side implementation:** `@novasamatech/host-container` (message routing, iframe/webview providers, permission gating around each handler) and `@novasamatech/host-papp` (Polkadot Mobile pairing, SSO, sessions, secrets).

On the new model the product-facing surface swaps to the generated [`@parity/truapi`](https://github.com/paritytech/truapi/tree/main/js/packages/truapi) client (products still emit the same SCALE frames), and the host-side glue (`host-api`, `host-container`, `host-papp`, and each host's own handlers) collapses into the shared Rust core. The codecs and method table are generated from the `truapi` crate by [`truapi-codegen`](https://github.com/paritytech/truapi/tree/main/rust/crates/truapi-codegen) rather than transcribed by hand.

### Current dotli (web)

dotli hosts products with a hand-written TypeScript host on top of the Novasama packages above. The shape today:

- Every protocol method is a bespoke handler in dotli's own `container.ts`, hand-wiring permission gating, modals, account derivation, error mapping, rate limiting, and statement-store/preimage behavior.
- The codecs and the method table are hand-written TypeScript in `@novasamatech/host-api`.
- Signing and SSO are delegated to the paired Polkadot Mobile wallet, but the orchestration around them is JavaScript that dotli maintains and version-pins against the Novasama packages.

The new model deletes that per-host implementation layer in favor of core callbacks.

### Current iOS (native)

[iOS](https://github.com/paritytech/polkadot-app-ios-v2) runs a parallel, hand-written Swift host implementing the same method surface. A `ContainerBridge` actor parses JSON messages from the WebView and routes roughly forty named methods, each hand-mapping JSON to Swift DTOs and SCALE, with a bundled in-page JavaScript shim.

iOS today is a true device-as-wallet signer: keys are HD-derived from a local root entropy, and `signPayload`/`signRaw`/`createTransaction` build a signing model, present a native confirmation UI, and sign in-process. There is no shared core; the entire dispatch, codec, subscription, and signing layer is Swift.

### Before / after

| Aspect                     | Current (per host)                                                 | New (shared core)                                                                |
| -------------------------- | ------------------------------------------------------------------ | -------------------------------------------------------------------------------- |
| Protocol dispatch          | Hand-written per host (`container.ts` TS, `ContainerBridge` Swift) | One Rust dispatcher in `truapi-server`                                           |
| Wire codecs / method table | Hand-written TS (`@novasamatech/host-api`) and Swift DTOs          | Generated from the `truapi` crate by `truapi-codegen`                            |
| Permission gating          | Re-coded per host                                                  | Core permission state machine                                                    |
| Auth/SSO orchestration     | `host-papp` JS, or the Swift signing stack                         | Core auth state machine and SSO protocol                                         |
| Session interpretation     | Per host                                                           | Core decode/projection; host stores an opaque blob                               |
| Host responsibility        | Full protocol implementation plus platform glue                    | Thin `truapi-platform` callbacks only                                            |
| Web signing                | External wallet over SSO (JS-orchestrated)                         | External wallet over SSO (core-orchestrated)                                     |
| iOS signing                | Device holds keys, signs locally (bespoke Swift stack)             | Device holds keys, signs locally behind the core's consent and signer capability |

The takeaway: protocol semantics are re-implemented per host in hand-maintained glue, in two different languages, and drift from each other. On the new core they live once, generated from a single canonical definition, and each host becomes a thin adapter over its own platform primitives.

## 6. Why this matters

One versioned, tested implementation of the protocol stands in for many hand-maintained ones. Behavior is consistent across Web, iOS, Android, and Desktop because they execute the same bytes through the same logic, rather than each host's interpretation of a spec. The codecs and the method table have a single source (the `truapi` crate), so there is no wire drift between hosts to reconcile.

Owning the semantics once also makes capabilities practical that are not worth maintaining per host: shared structured logs and correlation ids that follow a request through the core on any host, a headless host (the core with no UI, for automation and CI), and a simulation host for deterministic testing of product behavior at scale. These build on the single implementation instead of being patched into each host.

This is the direction the shared core enables: it becomes the default TrUAPI implementation across dotli (web), iOS, Android, and Desktop, plus the headless and simulation hosts, with one protocol definition behind all of them.
