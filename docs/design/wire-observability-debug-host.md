---
title: "Wire Observability and Debug Host"
owner: "@decrypto21"
---

# Wire Observability and Debug Host

|                    |                                                                                 |
| ------------------ | ------------------------------------------------------------------------------- |
| **Start Date**     | 2026-07-25 |
| **Authors**        | @decrypto21 |
| **Implementation** | truapi#295 |
| **Description**    | A payload-blind observe seam on the TrUAPI transport, a wire debugger, a mock/forward debug host, and a WebSocket relay - all correlated by the wire `requestId`. |

This is a **specification of the contract**, not a walkthrough of the implementation: it fixes the
interfaces, the observable behavior, and the invariants each layer must uphold. Implementation
detail, tests, and file layout live in truapi#295; where the two disagree, this document is the
intent and the code is the bug.

## Requirements

Every TrUAPI operation crosses the wire as an opaque frame `{ requestId, payload: { id, value } }`.
When one misbehaves - a wrong response, a subscription that never delivers, a silently dropped
frame - the question is *"what did this put on the wire, and where did it stall?"* Answering it
today means hand-decoding SCALE or forking the transport to add logging. This design must:

1. **Observe every frame without decoding it.** Shape and timing are visible by default; payloads
   and key material are not, so the same recorder is safe to run in production.
2. **Use one correlation id.** An operation is followable end to end - wire trace and host dispatch
   under a single id - with no second correlation scheme introduced.
3. **Be host-agnostic.** The seam sits on the protocol's own transport, not on any host's
   internals, so it works against any host that speaks TrUAPI.
4. **Be active, not only passive.** A developer can answer a frame with a scripted response (to
   develop against behavior that doesn't exist yet, or reproduce an error path) or forward it
   verbatim to a real host - without modifying that host.

## Model

**One payload-blind seam emits an `ObservedFrame` per wire frame, keyed on the transport-minted
`requestId`; a wire debugger groups those into traces; a debug host answers or forwards frames on
the same id.** The transport mints `requestId` (`p:1`, `p:2`, …) when a product starts an operation
and stamps it on every frame; the same id appears on the wire envelope and in the host's
`CallContext.requestId`. No second correlation id exists.

```
 product code        transport            wire debugger        host dispatch
      │  getAccount()    │                       │                    │
      │────────────────▶│ mint requestId = p:1   │                    │
      │                  │─ observe(out, p:1) ───▶│ push → WireTrace   │
      │                  │─ frame{ p:1 } ───────────────────────────▶  │ ctx.requestId=p:1
      │                  │◀ frame{ p:1 } ─────────────────────────────│
      │                  │─ observe(in, p:1) ────▶│ push → WireTrace   │
      │◀ Ok(response) ──│                        │                    │
```

## Interface

The signatures below are the normative surface, all in `@parity/truapi`.

### Transport observe seam

```ts
function createTransport(provider: WireProvider, options?: CreateTransportOptions): TrUApiTransport;
interface CreateTransportOptions {
  codecVersion?: number;
  observe?: TransportObserver;   // emit-only; the whole seam
  exposeFrameBytes?: boolean;    // dev-gated; see "Decoded view"
}
type TransportObserver = (frame: ObservedFrame) => void;

interface ObservedFrame {
  direction: "out" | "in";   // out = sent by this transport, in = received
  requestId: string;         // the one correlation id, e.g. "p:1"
  frameId: number;           // wire-table discriminant, e.g. 22
  role: FrameRole;           // request|response|start|receive|interrupt|stop|handshake|malformed|unknown
  byteLength: number;        // encoded SCALE length - shape only
  timestamp: number;         // epoch ms
  bytes?: Uint8Array;        // present only when exposeFrameBytes is set
}
```

Guarantees (enforced in `client.ts`):

- **Payload-blind by default.** `ObservedFrame` carries shape and timing only; `byteLength` is read
  off the encoded bytes without decoding them. The key-set is frozen (additive evolution only).
- **Causally ordered.** The outbound frame is observed *before* `provider.postMessage`, so a
  request precedes its responses even over a synchronous provider.
- **Failure-isolated.** A throwing observer is swallowed; it can never break the message loop.
- **Zero-cost when unset.** The notify path short-circuits before allocating anything.
- **Corrupt inbound frames are recorded.** An *inbound* frame that fails envelope decode surfaces as
  a `malformed` observed frame (sentinel `requestId`/`frameId`; under `exposeFrameBytes` it also
  carries the raw envelope `bytes`, which is what you want when diagnosing the corruption) before the
  transport closes, so the trace never goes dark. Malformed payload *values* are not seen here - the
  seam never decodes values. This covers the inbound path only: an *outbound* frame that fails to
  encode closes the transport before the observe hook runs, so it is not recorded.

### Wire debugger

```ts
function createWireDebugger(options?: WireDebuggerOptions): WireDebugger;
interface WireDebuggerOptions {
  sink?: (line: string, frame: ObservedFrame) => void; // formatted line + its frame; defaults to console.debug
  forward?: TransportObserver;        // a second observer (e.g. onward to a panel)
  maxTraces?: number;                 // LRU cap on trace count, default 256
  maxFramesPerTrace?: number;         // ring-buffer cap on frames within one trace, default 1024
  methodNames?: ReadonlyMap<number, WireMethodInfo>;
}
interface WireDebugger {
  readonly observe: TransportObserver; // hand to createTransport({ observe })
  traces(): WireTrace[];
  trace(requestId: string): WireTrace | undefined;
  clear(): void;
}
interface WireTrace { requestId: string; frames: ObservedFrame[]; startedAt: number; lastAt: number; }

function createMethodNameMap(table: Record<string, unknown>, services: readonly string[]): ReadonlyMap<number, WireMethodInfo>;
interface WireMethodInfo { method: string; kind: WireFrameKind; }
```

Behavior:

- Frames group into a `WireTrace` by `requestId`. Traces are held in an insertion-ordered map,
  **LRU-capped at `maxTraces`**; each trace's `frames` are **ring-buffered at `maxFramesPerTrace`**,
  so a long-lived subscription (whose frames all share one `requestId` that never LRU-evicts) cannot
  grow without bound. Memory is therefore bounded on both axes regardless of session length.
- `sink` and `forward` are each isolated in `try/catch`.
- `createMethodNameMap` inverts the generated wire-table into `frameId → { method, kind }`,
  resolving the longest service prefix first (`LOCAL_STORAGE_READ → localStorage.read`, not
  `local.storageRead`). It is the runtime source of readable names; a generated constant is a
  non-goal (below). A trace reads:

  ```text
  [wire p:1] → request  account.getAccount (id=22, 14B)
  [wire p:1] ← response account.getAccount (id=23, 35B)
  ```

### Decoded view (dev-gated)

The payload-blind seam is the safe foundation; the decoded view lets a developer see typed
request/response values. It is a second layer on top of the seam, and the core never decodes -
decoding is a consumer concern.

- **`exposeFrameBytes`** attaches each frame's raw SCALE `bytes` to its `ObservedFrame`. Without it,
  frames stay shape-and-timing only.
- **`WIRE_DECODE_TABLE`** (`@parity/truapi/wire-decode`) is a codegen-emitted
  `Record<number, (payload: Uint8Array) => unknown>` with one entry per request/response id plus
  subscription start/receive. Decoding is always against the generated client - the same source of
  truth as the wire codecs - so a decoded value cannot drift from the wire schema.
- **`?debug=wire:decode`** (see [the grammar](#the-debug-grammar)) requests the decoded view, but is
  **build-gated**: it enables `exposeFrameBytes` only when the bundle was built with
  `TRUAPI_WIRE_DECODE=1` (`NEXT_PUBLIC_TRUAPI_WIRE_DECODE=1` for the Next-built playground, so webpack
  inlines it), mirroring the relay's `TRUAPI_RELAY` gate. Without the build flag it degrades to a
  plain payload-blind `?debug=wire` trace. The production playground deploy leaves the gate unset.

**Bundle isolation.** The decode table is **not** statically imported. The playground trace panel
`import()`s `@parity/truapi/wire-decode` lazily, and only when a frame actually carries `bytes`. Under
`next dev` this code-splits into a separate chunk; the production static export forces a single chunk
(`splitChunks:false` + `maxChunks:1`), so there the module lands in the main bundle but its factory
never runs in payload-blind mode - the gate keeps it out of the *execution path*, not the bundle. The
table is non-sensitive generated codecs, and byte exposure is separately gated, so this is a
bundle-hygiene note, not a security boundary.

### Debug host

A `WireProvider`-shaped man-in-the-middle: it answers frames it is scripted to claim and forwards
the rest verbatim to a real host.

```ts
function createWireDebugHost(options: CreateWireDebugHostOptions): WireDebugHost;
interface CreateWireDebugHostOptions {
  provider: WireProvider;              // the product side
  entries?: readonly WireDebugHostEntry[]; // mock entries, each claiming its wire ids
  forward?: WireProvider;              // optional pipe to a real host
  observe?: TransportObserver;         // payload-blind, host vantage
  onDecision?: (d: WireDebugHostDecision) => void;
}
interface WireDebugHost { dispose(): void; }

interface DebugRequestEntry {
  readonly kind: "request";
  readonly ids: RequestFrameIds;                          // { request, response }
  handle(ctx: DebugCallContext, payload: Uint8Array): Uint8Array | Promise<Uint8Array>;
}
interface DebugSubscriptionEntry {
  readonly kind: "subscription";
  readonly ids: SubscriptionFrameIds;                     // { start, receive, interrupt, stop }
  start(ctx: DebugCallContext, payload: Uint8Array, port: DebugSubscriptionPort): DebugSubscriptionCleanup | Promise<DebugSubscriptionCleanup>;
}
type WireDebugHostEntry = DebugRequestEntry | DebugSubscriptionEntry;
interface WireDebugHostDecision { tier: "mock" | "forward" | "unhandled"; method?: string; frame: ObservedFrame; }
```

Behavior:

- **Entries are byte-level, keyed by wire id** - a flat list, not a nested handler tree, and there
  is no internal loopback dispatcher. `handle` takes and returns raw SCALE bytes. "A mock cannot
  emit a malformed frame" holds by the *convention* that the caller encodes answers with the
  generated codecs (`encodeWireMessage`/`decodeWireMessage`), not by the type of `handle`. Two
  entries claiming the same inbound (`request`/`start`) wire id is a construction error:
  `createWireDebugHost` throws rather than silently letting one shadow the other.
- **Routing.** A claiming request/subscription entry answers (`tier: "mock"`); a claimed `stop` is
  terminal. Otherwise, with a `forward` pipe set the frame travels it **byte-verbatim, `requestId`
  untouched** (`tier: "forward"`) and the answer relays back; with no forward pipe it surfaces
  loudly as `tier: "unhandled"` (via `onDecision`, or a `console.warn` when no listener is
  set - so the stall is loud rather than silent; the caller still hangs, as no answer is sent).
- **Every frame is marked** via `onDecision`, so a scripted answer is never silently mistaken for
  real host behavior.
- **Errors are loud, never a silent wrong answer.** A mock `handle`/`start` that throws, or a
  response that fails to encode, emits a `console.warn` and sends nothing - the caller hangs loudly,
  the same policy as `unhandled`. An undecodable *inbound* envelope is forwarded byte-transparent
  when a `forward` pipe is set, or `console.warn`ed when headless - the host-side analogue of the
  transport's `malformed` recording. (Repeated unhandled frames for one wire id warn once.)
- **Teardown.** `dispose()` sends `stop` upstream for any live forwarded subscription (so the real
  host stops streaming into a detached pipe) and runs cleanup for every live *mock* subscription, so
  a torn-down session leaks nothing on either side.

This is an **observability seam, not a test host.** With no forward pipe it answers scripted bytes
with no core behind it (no dispatch, permissions, or storage) - a debugging convenience, not a
fidelity tier. The deterministic testing tier is the mock host (truapi#294); the canonical headless
host is truapi#264. The debug host sits *in front of* those and forwards *to* them.

### Relay

```ts
function createRelayProvider(opts: { url: string; sessionId: string; productId: string; role: "product" | "host" | "debugger"; optIn?: boolean }): WireProvider;
interface RelayEnvelope { v: 1; role: "product" | "host" | "debugger"; sessionId: string; productId: string; frame: Uint8Array; }
class RelayRouter { join(sessionId, peer): void; leave(sessionId, peer): void; handleEnvelope(from, bytes): void; }
```

`createRelayProvider` carries frames over a WebSocket in a routing envelope; the relay routes by
`(sessionId, role)` and never parses a frame. Because it is a plain `WireProvider`, pointing a
product at a debug host in another process is a **provider swap** - transport and product code
untouched, and the same provider drops into a debug host's `provider`/`forward` slots. It is
**dev-gated**: `createRelayProvider` throws unless built with `TRUAPI_RELAY=1` or passed
`{ optIn: true }` (no silent fallback; a session that cannot reach its relay fails loudly).
`RelayRouter` is the transport-agnostic core (join-order-independent: frames arriving before the
counterpart joins are buffered - capped at 1024 per session, excess dropped with a one-time warning -
and flushed on join); `createLoopbackSocketFactory` runs a relay in-process, with no network hop, for
tests and single-tab use.

Only `v: 1` envelopes are accepted; any other version fails to decode and the frame is dropped (the
envelope is versioned so the wire can evolve without silently mis-routing an unknown shape). A
carrier must call `RelayRouter.leave()` on disconnect - it drops the peer and, once the session is
empty, deletes the session and its pending buffer; `createRelayProvider().dispose()` closes the
socket and clears subscribers.

## Enablement

No product changes its call sites. The `@parity/truapi/sandbox` bootstrap - the shared transport
builder for browser-embedded products, including the playground - reads the embedding URL once at
module load (snapshotted, so a product that rewrites its own URL can't drop the flag before the
first `getClientSync()`).

### The `?debug=` grammar

Debug surfaces are selected by a single, extensible query key rather than a new flag per feature. A
debugger accretes modes; the grammar is built to accrete with it:

```text
?debug=<channel>[:<modifier>[:<modifier>…]][,<channel>…]

  wire            payload-blind observe seam + wire debugger        (safe anywhere)
  wire:decode     + typed-value decode of each frame                (build-gated; see below)
```

- **Channels** name a debug surface (`wire` today; a `relay` channel and others slot in without a
  new query key). **Modifiers** are additive verbosity levels on a channel: `wire` is the safe
  baseline; `wire:decode` raises it to expose payload bytes.
- **Composes:** `?debug=wire:decode,relay` opts several channels in at once.
- **Forward-compatible:** unknown channels and modifiers are ignored, not errors, so a newer link
  opened against an older build degrades to whatever that build understands instead of failing.
- **Legacy alias:** `?debug=wire-decode` is accepted as `wire:decode`.

Effect: `wire` installs a `createWireDebugger` on the transport's `observe` hook and exposes it via
`getWireDebugger()` and `window.__truapiWireDebugger__` (the playground renders its trace panel off
this). `wire:decode` additionally sets `exposeFrameBytes` **iff the `TRUAPI_WIRE_DECODE` build gate
is on**, so the panel decodes each frame through `WIRE_DECODE_TABLE`; otherwise it degrades to a
payload-blind trace. So enabling the payload-blind debugger is a **URL flag**, not a code change;
byte-level decoding additionally takes a build-time opt-in.

## Privacy and security

- **Payload-blind default is the boundary.** The default surface carries no decoded payload and no
  key material, so it is safe in production. The relay carries frames as opaque bytes; mocked
  responses are always marked `tier: "mock"`.
- **Byte exposure is build-gated, deliberately.** A URL cannot turn on `exposeFrameBytes` by itself -
  which matters because the playground is a deployed site and dotli forwards unknown query params
  through to the product iframe, so the URL is attacker-influenceable. The raw wire can carry key
  material (the truapi#264 review found secret key material reachable on the SSO response path), so
  the build gate is the structural defense that keeps decoded payloads out of production. The
  decoded mode is a developer tool and is not claimed to be production-safe.
- **Residual metadata exposure (accepted).** `?debug=wire` publishes the debugger on
  `window.__truapiWireDebugger__`, so any script already on the page can read the traces - but they
  are shape-and-timing only, the same metadata already in devtools' network view, so this is
  defense-in-depth against a third-party script, not a payload leak. And frame shape plus timing is
  what traffic analysis works from: "safe in production" means it leaks no application *content*, not
  that the metadata is zero-knowledge.

## Compatibility

Purely additive. `observe`/`exposeFrameBytes` are new optional fields on `CreateTransportOptions`;
`debug.ts`, `debug-host.ts`, and `relay.ts` are new modules with new barrel exports. No existing
interface changes; no migration. The observe hook is zero-cost when unset and, when set, allocates
one small record per frame into the doubly-capped map.

## Non-goals and deferred work

- **No interactive UI beyond the playground trace panel.** A host-panel bridge and a mock-handler
  editor are later.
- **No host-side observe hook** in this design; the seam is client-transport-side only.
- **No generated method-name constant.** `createMethodNameMap` is the runtime source today.
- **`requestId` stays transport-local.** Each `createTransport` mints from zero, so a debugger/debug
  host is 1:1 with a transport; cross-transport aggregation (namespacing the trace key) is deferred
  until a use case appears.
- **Relay session pairing** (minting a short `sessionId`) and a dev-preview-server mount for a
  reference relay are deferred.

## Validation status

What is automated versus proven by hand, so no claim here reads as more validated than it is:

- **Automated in truapi#295** (the `js/packages/truapi/test/*.test.mjs` suite): correlation,
  mock/forward/unhandled routing, dispose-time upstream stop, the relay envelope round-trip and the
  loopback mock/forward flows, and the debugger driven over a real WebSocket carrier (a single
  process with a real loopback socket, not separate OS processes). The playground e2e
  (`playground/tests/e2e/wire-debug.spec.ts`) covers the `?debug=wire` and `?debug=wire:decode`
  panel paths.
- **Verified manually, not yet automated:** end-to-end decode against the genuine Rust core running
  headless as WASM (a `localStorage.read` observed under one `requestId` and forwarded verbatim
  through the debug host). This was reproduced by hand; there is no WASM-core test in the package.
- **Not yet validated at all:** auth-gated methods (signing needs a paired session) and a live
  in-browser playground run inside a host.

## Alternatives considered

- **Host-specific debugger (rejected).** Tapping one host's internal hook (prior art: a dotli-only
  read-only panel) ties the tool to that host's unstable internals and cannot mock or forward.
  Hooking the protocol's own transport is host-agnostic and active.
- **A second correlation id (rejected).** Every layer keys on the transport-minted `requestId`,
  which already appears on the wire and in host dispatch; a second id is pure plumbing.
- **Frame rewriting instead of codec-backed mock entries (deferred).** Entries answer through the
  generated codecs so mocked frames stay well-formed; raw rewriting remains possible later at the
  relay layer.
- **A standalone debugger app (deferred).** The playground panel plus the relay cover the near-term
  need without a new app to ship.

## References

- Implementation: truapi#295
  (`js/packages/truapi/src/{client,debug,debug-host,relay,sandbox}.ts`, the playground trace panel).
  Automated coverage: `js/packages/truapi/test/*.test.mjs` and
  `playground/tests/e2e/wire-debug.spec.ts` (scope as listed under Non-goals → Automated).
- Peers in the "headless / mock host" space: mock host (truapi#294), headless host (truapi#264).
- Requirement source: the debugger tracker (sdk-team#26).
