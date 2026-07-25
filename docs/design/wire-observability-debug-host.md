---
title: "Wire Observability and Debug Host"
owner: "@decrypto21"
---

# Wire Observability and Debug Host

|                 |                                                                                 |
| --------------- | ------------------------------------------------------------------------------- |
| **Start Date**  | 2026-07-25 |
| **Description** | A payload-blind observe seam on the TrUAPI transport, a wire debugger, a mock/forward debug host, and a WebSocket relay - correlated by the wire `requestId`, and enabled in a product by a single URL flag. |
| **Authors**     | @decrypto21 |

## Summary

Every TrUAPI operation is a sequence of SCALE-encoded frames crossing the product↔host
transport, already keyed by one id the transport mints: the `requestId`. This design adds a single
**payload-blind observe seam** on that id, a **wire debugger** that groups frames into
per-`requestId` traces, a **debug host** that can answer frames with scripted mocks or forward
them verbatim to a real host, and a **relay** (a `WireProvider` over WebSocket) that lets a
product point its transport at a debug host running in another process. It also adds the
integration surface that makes this usable by a real product with no application-code change: the
`@parity/truapi/sandbox` bootstrap installs the wire debugger when the embedding URL carries
`?debug=wire`, and the TrUAPI playground renders a live per-`requestId` trace panel off it.
Everything is in `@parity/truapi` and carries frame shape and timing only - never decoded payloads
or key material - so the recorder is safe to leave on anywhere, including production. On top of that
payload-blind default, a dev-gated `?debug=wire-decode` mode decodes frames to typed request/response
values through a codegen-emitted `WIRE_DECODE_TABLE`; it is off by default and never runs in the
payload-blind path.

## Motivation

Every TrUAPI operation crosses the wire as an opaque frame `{ requestId, payload: { id, value }
}`. When an operation misbehaves - a wrong response, a subscription that never delivers, a frame
the host drops silently - the everyday question is *"what did this operation put on the wire, and
where did it stall?"* Today, answering it means hand-decoding SCALE bytes or bolting an ad-hoc
logger onto a private copy of the transport. Each team re-invents this; there is no shared way to
see the wire, and no way to line up a product's own logs with what the host received.

Requirements:

1. **Observe every frame without decoding it.** Shape and timing must be visible; payloads and
   key material must never be exposed at this layer, so the same recorder is safe in production.
2. **One correlation id.** A single operation must be followable end to end - the wire trace and
   the host dispatch under one id - with no second correlation scheme introduced.
3. **Host-agnostic.** The seam must sit on the protocol's own transport, not on any one host's
   internals, so it works against any host that speaks TrUAPI.
4. **Active as well as passive.** Beyond watching, a developer must be able to answer a frame with
   a fake response (to develop against host behaviour that does not exist yet, or to reproduce an
   error path) or forward it unchanged to a real host - without touching that host.

## Detailed Design

The model in one line: **one payload-blind seam emits an `ObservedFrame` per wire frame, keyed on
the transport-minted `requestId`; a wire debugger groups those into traces, and a debug host
answers or forwards frames on the same id.**

### The correlation id

The transport mints `requestId` (`p:1`, `p:2`, …) when a product starts an operation, and every
frame of that operation carries it. The same id appears on the wire envelope and in the host's
`CallContext.requestId` at dispatch, so one operation is followable from the product, across the
wire, into the host, and back - under a single id. No second correlation id is introduced.

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

### The observe seam

`createTransport` gains an optional emit-only observer. It is the whole surface of the seam:

```ts
function createTransport(provider: WireProvider, options?: CreateTransportOptions): TrUApiTransport;
interface CreateTransportOptions { codecVersion?: number; observe?: TransportObserver; }
type TransportObserver = (frame: ObservedFrame) => void;

interface ObservedFrame {
  direction: "out" | "in";   // out = sent by this transport, in = received
  requestId: string;         // the one correlation id, e.g. "p:1"
  frameId: number;           // wire-table discriminant, e.g. 22
  role: FrameRole;           // request|response|start|receive|interrupt|stop|handshake|malformed|unknown
  byteLength: number;        // encoded SCALE length - shape only, never contents
  timestamp: number;         // epoch ms
}
```

Enforced properties (all in `client.ts`):

- **Payload-blind.** `ObservedFrame` carries shape and timing only. `byteLength` is read off the
  encoded bytes without decoding them. The key-set is frozen and tested.
- **Causally ordered.** The outbound frame is observed *before* `provider.postMessage`, so a
  request precedes the responses even over a synchronous in-memory provider.
- **Failure-isolated.** A throwing observer is swallowed - an observer must never break the
  transport's message loop.
- **Zero-cost when unset.** The notify path short-circuits before constructing anything.
- **Corrupt frames are recorded.** A frame that fails envelope decode is surfaced as a
  `malformed` observed frame (byte length only, sentinel `requestId`/`frameId`) before the
  transport closes, so a decode failure is recorded rather than the trace going dark. Malformed
  payload *values* are not seen here (the seam never decodes values); those errors reach the
  caller.

### The decoded view

The payload-blind seam is the safe foundation; the decoded view is what lets a developer see the
typed request and response values, not just frame shapes. It is a second, dev-gated layer built on
top of the seam, and the core never decodes - decoding is a consumer concern.

- **`exposeFrameBytes`.** A dev-gated `createTransport` option that attaches each frame's raw SCALE
  `bytes` to its `ObservedFrame`. Without it, `ObservedFrame` stays shape-and-timing only; the byte
  attachment exists solely so a consumer can decode.
- **`WIRE_DECODE_TABLE`.** A codegen-emitted table exported at `@parity/truapi/wire-decode`, a
  `Record<number, (payload: Uint8Array) => unknown>` with one entry per request/response frame id
  plus subscription start/receive, produced by `truapi-codegen`. Decoding is therefore always
  against the generated client - the same source of truth as the wire codecs - so a decoded value
  cannot drift from the wire schema.
- **The `?debug=wire-decode` flag.** It enables `exposeFrameBytes` and turns on the consumer-side
  decode. A dev consumer - the playground trace panel - looks each frame's id up in
  `WIRE_DECODE_TABLE`, decodes the attached bytes, and renders the decoded request/response value
  inline under the payload-blind row.

Off by default: with no `?debug=wire-decode`, `exposeFrameBytes` is never set, no bytes are
attached, and `WIRE_DECODE_TABLE` is not imported, so the decode path is dead-code-eliminable in a
production build.

### The wire debugger

```ts
function createWireDebugger(options?: WireDebuggerOptions): WireDebugger;
interface WireDebuggerOptions {
  sink?: (line: string) => void;      // formatted lines; defaults to console.debug
  forward?: TransportObserver;        // a second observer (e.g. onward to a panel)
  maxTraces?: number;                 // LRU cap, default 256
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

Frames group into a `WireTrace` by `requestId`; traces are held in an insertion-ordered map capped
at `maxTraces` (LRU - each touch re-inserts, oldest evicted past the cap). `sink` and `forward`
are each isolated in `try/catch`. `createMethodNameMap` inverts the generated wire-table into
`frameId → { method, kind }`, resolving the longest service prefix first (`LOCAL_STORAGE_READ →
localStorage.read`, not `local.storageRead`); it is the runtime source of readable names. A trace
reads:

```text
[wire p:1] → request  account.getAccount (id=22, 14B)
[wire p:1] ← response account.getAccount (id=23, 35B)
```

### The debug host

A `WireProvider`-shaped man-in-the-middle. It answers frames it is scripted to claim, and forwards
the rest verbatim to a real host.

```ts
function createDebugHost(options: CreateDebugHostOptions): DebugHost;
interface CreateDebugHostOptions {
  provider: WireProvider;              // the product side
  entries?: readonly DebugHostEntry[]; // mock entries, each claiming its wire ids
  forward?: WireProvider;              // optional pipe to a real host
  observe?: TransportObserver;         // payload-blind, host vantage
  onDecision?: (d: DebugHostDecision) => void;
}
interface DebugHost { dispose(): void; }

interface DebugRequestEntry {
  readonly kind: "request";
  readonly ids: RequestFrameIds;                                   // { request, response }
  handle(ctx: DebugCallContext, payload: Uint8Array): Uint8Array | Promise<Uint8Array>;
}
interface DebugSubscriptionEntry {
  readonly kind: "subscription";
  readonly ids: SubscriptionFrameIds;                              // { start, receive, interrupt, stop }
  start(ctx: DebugCallContext, payload: Uint8Array, port: DebugSubscriptionPort): DebugSubscriptionCleanup | Promise<DebugSubscriptionCleanup>;
}
type DebugHostEntry = DebugRequestEntry | DebugSubscriptionEntry;
interface DebugHostDecision { tier: "mock" | "forward" | "unhandled"; method?: string; frame: ObservedFrame; }
```

Mechanics (`debug-host.ts`):

- **Entries are byte-level and keyed by wire id** - a flat list, not a nested `{ service: { method
  } }` handler tree, and there is no internal loopback dispatcher. A `handle` takes and returns raw
  SCALE bytes. The debug host imports the generated codecs (`encodeWireMessage` /
  `decodeWireMessage`), and the property "a mock cannot emit a malformed frame" holds by the
  convention that the caller encodes answers with those codecs - it is not enforced by the type of
  `handle`.
- **Router.** Each inbound frame is split by wire id: a claiming request/subscription entry answers
  (`tier: "mock"`); a claimed `stop` is terminal (marked `mock`, stops the slot if live);
  otherwise, if a `forward` pipe is set, the frame travels it **byte-verbatim** with `requestId`
  untouched (`tier: "forward"`) and the answer relays back; with no forward pipe, the frame
  surfaces loudly as `tier: "unhandled"` (a `console.warn` - the caller would otherwise hang).
- **Every frame is marked** via `onDecision` with its `tier`, so a scripted answer can never be
  silently mistaken for real host behaviour.
- **Teardown.** `dispose()` sends `stop` frames upstream for any live forwarded subscriptions, so a
  torn-down debug session does not leak subscriptions on the real host.

The debug host is an **observability seam, not a test host.** With no forward pipe it answers
scripted bytes with no core behind it - no dispatch, no permissions, no storage - so it is a
debugging convenience, not a testing tier. The deterministic testing tier is the mock host
(truapi#294, real core + mocked platform/wallet seams); the canonical headless host is truapi#264.
This debug host sits *in front of* those and forwards *to* them; it does not replace them.

### The relay

`createRelayProvider` is a `WireProvider` that carries frames over a WebSocket wrapped in a routing
envelope `{ v, role, sessionId, productId, frame }`; the relay routes by `(sessionId, role)` and
never parses a frame. Because it is just a `WireProvider`, pointing a product at a debug host in
another process is a **provider swap** - the transport and product code are untouched, and the same
provider drops into a debug host's `provider` / `forward` slots.

```ts
function createRelayProvider(opts: { url: string; sessionId: string; productId: string; role: "product" | "host" | "debugger"; optIn?: boolean }): WireProvider;
interface RelayEnvelope { v: 1; role: "product" | "host" | "debugger"; sessionId: string; productId: string; frame: Uint8Array; }
class RelayRouter { join(sessionId, peer): void; leave(sessionId, peer): void; handleEnvelope(from, bytes): void; }
```

The client is **dev-gated**: `createRelayProvider` throws unless built with `TRUAPI_RELAY=1` or
passed `{ optIn: true }` - no silent fallback, a session that cannot reach its relay fails loudly.
`RelayRouter` is the transport-agnostic routing core (join-order-independent: frames that arrive
before the counterpart joins are buffered and flushed on join); `createLoopbackSocketFactory` runs
a relay in-process for tests and single-tab use. A reference Bun WebSocket relay ships as
`examples/relay-server.mjs`.

### How a product turns it on

No product changes its call sites. The `@parity/truapi/sandbox` bootstrap - the shared path that
builds the transport for browser-embedded products (including the TrUAPI playground) - reads the
embedding URL: with `?debug=wire`, `getClientSync()` installs a `createWireDebugger` on the
transport's `observe` hook and exposes it via `getWireDebugger()` (and
`window.__truapiWireDebugger__`). The playground renders a live per-`requestId` trace panel off
that. Adding `?debug=wire-decode` does everything `?debug=wire` does and also sets
`exposeFrameBytes`, so the playground panel decodes each frame through `WIRE_DECODE_TABLE` and
renders the typed request/response value inline. So enabling the debugger for a sandbox-based
product is a **URL flag**, not a code change.

### Testing and verification

`bun test` covers request/response and subscription lifecycle under one id, method-name mapping
across the full wire-table, payload-blindness key-set assertions, observer/sink/forward failure
isolation, LRU eviction at the cap, the debug host's mock/forward/unhandled routing and marking,
entry-precedence over the forward pipe, the dispose-time upstream stop (with a cross-service drift
guard), and the start-pending-vs-stop race; the relay's envelope round-trip, router routing +
join-order buffering + dev gate, and two end-to-end flows over a loopback relay; the sandbox
`?debug=wire` opt-in; and a codegen test asserting `WIRE_DECODE_TABLE` is emitted with one entry
per request/response frame id plus subscription start/receive. A Playwright e2e asserts both the
payload-blind panel under `?debug=wire` and the decoded typed-value view under `?debug=wire-decode`.
The suite is green (203 pass); `tsc -b` is clean, and the TrUAPI playground (which mounts the trace
panel) builds and lints clean.

Validated end to end against the genuine Rust core run headless as WASM: a real `localStorage.read`
round trip observed under one `requestId` and forwarded verbatim through the debug host (the host
callback received the core's own namespaced key and the core's own response encoding, confirming it
is the real dispatcher, not a shim). The relay is also proven over a real cross-process socket: a
relay server, a debug host, and a product running as three separate OS processes, communicating
only over WebSockets, carried a call answered under one `requestId` end to end. Not yet exercised:
auth-gated methods (signing, needs a paired session) and a live in-browser playground run inside a
host.

### Privacy

Load-bearing and by design: the **default** observe surface carries no decoded payload and no key
material, so the seam is safe to run in production. The relay carries frames as opaque bytes, and
mocked responses are always marked (`tier: "mock"`).

Decoded payloads exist **only** under the dev-gated `?debug=wire-decode` mode: `exposeFrameBytes`
attaches raw bytes and the consumer decodes them through `WIRE_DECODE_TABLE`. That mode is off by
default and dead-code-eliminable in a production build. This dev-gating is a hard requirement, not a
convenience: the raw wire can carry key material - the truapi#264 review found secret key material
reachable on the SSO response path - so the dev gate is the structural defense that keeps decoded
payloads out of production. The decoded mode is a developer tool and is not claimed to be safe to
run in production.

### Compatibility and performance

Purely additive. `observe` is a new optional field on `CreateTransportOptions`; `debug.ts`,
`debug-host.ts`, and `relay.ts` are new modules with new barrel exports. No existing interface
changes; no migration required. The `ObservedFrame` key-set is frozen (additive evolution only).
The observe hook is zero-cost when unset (a single short-circuit before any allocation); when set,
it allocates one small record per frame into an LRU-capped map, so memory is bounded regardless of
session length.

### Out of scope

No interactive UI beyond the playground trace panel (a host-panel bridge and a handler editor are
later); no host-side observe hook; no generated method-name constant (the runtime
`createMethodNameMap` is the source today). Decoded payloads are in scope, but only as the dev-gated
`?debug=wire-decode` mode - the default observe surface stays payload-blind and the core never
decodes. Relay session pairing (minting a short `sessionId`) and a dev-preview-server mount for the
reference relay are also deferred.

## Drawbacks

- **`requestId` is transport-local.** Each `createTransport` mints `p:1, p:2, …` from zero, so a
  wire debugger / debug host instance is 1:1 with a transport; sharing one across transports would
  collapse unrelated operations under the same id. Namespacing the trace key to aggregate multiple
  transports is deferred until a shared-aggregation use case appears.
- **A second thing named "host."** The debug host adds a third artifact that reads as a
  "headless/mock host" next to truapi#264 and truapi#294; the naming space is crowded. The
  export keeps the name `createDebugHost`, and the distinction from those hosts is documented
  above.
- **Byte-level mock entries are lower-level than a typed handler tree** - authoring a mock means
  encoding bytes with the generated codecs, which is less ergonomic than a typed method handler
  would be (a future surface concern, not a blocker).

## Alternatives

- **Host-specific debugger (rejected).** Tapping one host's internal hook (prior art: a dotli-only,
  read-only panel) ties the tool to one host's unstable internals and cannot mock or forward. This
  design hooks the protocol's own transport instead, so it is host-agnostic and active.
- **Frame rewriting instead of a dispatcher-level mock (deferred).** A mock could hand-rewrite SCALE
  frames; instead, entries answer through the generated codecs, which keeps mocked frames
  well-formed. Raw frame rewriting remains possible later at the relay layer.
- **A standalone debugger application (deferred).** A separate desktop app was considered; the
  playground trace panel plus the relay cover the near-term need without a new app to ship.
- **Introducing a second correlation id (rejected).** Every layer keys on the transport-minted
  `requestId`, which already appears on the wire and in the host dispatch context; a second id would
  add plumbing with no benefit.

## References

- Implementation: truapi#295 (`js/packages/truapi/src/{client,debug,debug-host,relay,sandbox}.ts`)
  and the TrUAPI playground trace panel.
- Peers in the "headless / mock host" space: the mock host (truapi#294) and the headless host
  (truapi#264).
- Requirement source: the debugger tracker (sdk-team#26), which asks to see what a product sends
  and what the host returns, "decoded to typed values" - the requirement the dev-gated
  `?debug=wire-decode` view fulfills on top of the payload-blind seam.
