# Wire Observability and Debug Host

|                    |                                                                                 |
| ------------------ | ------------------------------------------------------------------------------- |
| **Start Date**     | 2026-07-25 |
| **Authors**        | Nidish Ramakrishnan |
| **Implementation** | truapi#295 (Rust tap in `truapi-server`; `@parity/truapi-debugger` engine + surfaces) |
| **Description**    | A payload-blind tap in the Rust host core streams every product↔host TrUAPI frame, under one correlation id, to a debugger that groups and decodes it. Two surfaces consume the same engine: a standalone loopback inspector and an in-host embed. |

The short version: product↔host TrUAPI traffic is opaque SCALE frames with no
Network tab. A tap in the Rust host core emits every frame as opaque
`{channelId, dir, bytes}`; the debugger — never the core — correlates them into
per-operation traces and decodes them. This is a **strictly dev-only tool**: the
tap and the decoder are compiled out of production builds, so there is nothing to
turn on in a shipped host.

## The problem

A product and its host exchange everything — account derivation, signing, chain
reads, scoped storage, subscriptions — as serialized SCALE `ProtocolMessage`
frames across a process boundary (`MessagePort`, iframe `postMessage`, or a native
webview channel). On the wire those frames are just bytes. There is no per-call
view, no way to see which operation a frame belongs to, no request/response
pairing, and no decoded payload. When a call misbehaves, the developer is staring
at a byte channel.

This design gives that byte channel a Network tab: capture every frame, group the
frames of one call together, and — because it is a dev tool looking at the
developer's own session — decode them.

## Where the tap lives

The tap is in the **Rust host core (`truapi-server`)**, behind a sink trait — not
in the TypeScript transport, and not turned on from the product side.
`truapi-server` has exactly two frame choke points, and every host funnels through
them:

| Direction | Choke point | Location |
|---|---|---|
| inbound (product → core) | `ProductRuntime::receive_frame()` — taps **before** dispatch | `host_core.rs` |
| outbound (core → product) | `SinkTransport::send()` — delivers via `FrameSink::emit_frame`, then taps | `host_core.rs` |

One tap covers every platform, and **nothing in `@parity/truapi` (the product
package) changes** — it has no debug seam at all. That the product package is
genuinely untouched is the test for the tap being in the right place: a seam in
the product transport would fail it. Decoding and correlation live in the
debugger, never in the core; the core only ever hands out opaque bytes.

The tap is **fire-and-forget**. `DebugSink::emit` must not block the frame path
and must not fail the operation that produced the event, so a slow, absent, or
crashed debugger only loses a trace, never a session. The two in-path call sites
wrap `emit` in `catch_unwind` (`emit_debug`), so even a panicking out-of-repo sink
is caught, logged, and swallowed rather than unwinding into a live dispatch.

```rust
/// Dev-only sink for host debug events. Unset ⇒ the tap is inert.
pub trait DebugSink: Send + Sync {
    /// Fire-and-forget: must not block the frame path or fail the operation.
    fn emit(&self, event: DebugEvent);
}

pub struct ChannelId(pub String);
pub enum FrameDirection { In, Out }

#[non_exhaustive] // room for non-frame events (e.g. SSO); adding one is not breaking
pub enum DebugEvent {
    /// A SCALE wire frame crossing a product channel. `bytes` are the untouched
    /// `ProtocolMessage`; the debugger decodes them, the core never does.
    Frame { channel_id: ChannelId, dir: FrameDirection, bytes: Vec<u8> },
}
```

A sink is installed per product channel via
`ProductRuntime::set_debug_sink(channel_id, sink)`; unset by default, so a host
that never installs one pays nothing. The concrete sink is provided by the host
adapter, not the core:

- the **web** host bridges `emit` to a JS `debugEmit(channelId, dir, frame)`
  callback — the host worker owns the actual socket and its dev-only URL gate;
- the **native** host's `WsDebugSink` (`native_debug.rs`) implements the trait
  directly over a loopback WebSocket. It only serializes and pushes onto a
  bounded queue (count- and byte-capped); a background task owns the socket,
  reconnects with capped backoff, and counts dropped frames when the queue is
  full. It compiles only under the `ws-bridge` feature, out of the wasm graph. It
  is a ready seam for later native hosts, not one of the two surfaces below.

## The envelope and wire identity

Each tapped frame becomes one envelope:

```
 { channelId, dir, frame }
```

`frame` is the untouched SCALE `ProtocolMessage` bytes; `dir` is **product-vantage**
(`out` = the frame left the product, `in` = it arrived at it). The Rust tap names
directions host-vantage internally and flips them to this convention on the way
out (`FrameDirection::wire_str`), so both ends always agree on which way a frame
went. Where the envelope crosses a text transport (the WS surface below),
`frame` is base64 so the whole thing fits one JSON line.

A producer also stamps a **wire-contract fingerprint** alongside the envelope: an
envelope version (`v`), the coarse codec version (`codec` = `TRUAPI_CODEC_VERSION`),
and — the load-bearing one — `TRUAPI_WIRE_SCHEMA_HASH` (`schema`), a hash of every
frame id and its method leg (e.g. `"c18def0e997626eb"`). Frame ids are `u8`
discriminants that get reassigned as the API evolves, so a frame from a host built
against a different contract would silently decode to the *wrong* method and the
wrong value. The hash catches that: the debugger allows the decode path only for a
channel whose `schema` affirmatively matches its own. An absent or mismatched
schema still groups (grouping is payload-blind) but is never trusted to decode —
which also closes the omit-the-identity-to-bypass hole.

## The debugger engine

The engine is one shared core (`@parity/truapi-debugger`) that both surfaces
drive, so the surface a developer picks never changes what is shown.

**Ingest.** Each envelope is decoded once (`decodeWireMessage`) to recover the
correlation `requestId` and the wire discriminant (`frameId`) — everything
grouping needs. An undecodable frame is surfaced as a `malformed` sentinel, not
dropped, so a trace records the failure instead of going dark. Each frame's
lifecycle `role` (request / response / start / receive / …) is resolved **at
ingest** from the frame id's wire-table kind, so every downstream consumer sees
the real role rather than `unknown`. Retained id strings are length-clamped
(default 256 chars): anything that can reach the tap could otherwise send
200k-char ids, one copy per frame, while real ids are short (`myapp.dot`, `p:1`).

**Grouping by `(channelId, requestId)`.** Frames are grouped into per-operation
traces keyed on the channel *and* the request id — not `requestId` alone, since
each host mints its own `p:1`, `p:2`, … and two hosts reuse the same values. The
channel keeps their operations apart. This `requestId` is also the id product-sdk
telemetry spans correlate on, so a frame trace and a product span line up under
one id with no extra plumbing.

**Generation segmentation.** A product may recycle a `requestId` for a later,
unrelated call. When a fresh opener (`request` / `start`) arrives for an id whose
current operation already opened, the engine rotates to a new *generation* rather
than merging two unrelated calls under one id.

**Retry-storm detection.** A burst of like operations — one host hammering
`signing.createTransaction` five times in 400ms — is a cross-operation signal no
single-trace view can see. The engine groups traces by `(channelId, opener
frameId)` and slides a window (default: 3 ops within 1000ms) over their start
times, tagging every trace in a burst with a `retry-storm` badge.

**Bounded retention, with visible eviction.** Memory is bounded three ways, and
each bound surfaces rather than lying about the data:

- an LRU cap on retained traces (default 256); whole-op evictions are counted
  (`evictedTraces()`) so a session that overflowed does not silently under-report
  its op count;
- a per-trace frame cap (default 1024), ring-buffered from index 1 so a
  long-lived subscription never drops its opener (pairing and retry-storm signals
  key on `frames[0]`);
- a per-trace byte cap (default 1 MiB, a true bound including the opener), which
  only bites when bytes are retained for decode.

A trace that lost frames or bytes to either cap carries a `truncated` badge, and a
producer that dropped frames (link backlog full) reports the count, surfaced in
stats — so "kept N of M" is never mistaken for "only N happened".

## The two surfaces

One engine, two ways to look at it. Both decode every frame; neither can surface a
value the engine would not, because both run the same code.

### (A) Standalone inspector — loopback `:9231`

A runnable Bun server (WS + HTTP) that a host dials **outward** to. Only the inside
can initiate the connection (nothing on a dev machine dials into a device or a
Worker), so the host is always the client. Over the socket the host sends one
`{channelId, dir, frame}` text message per frame; the server groups them and
serves a full-screen, Network-tab-style **web inspector** (aggregate summary
strip, filterable/sortable operation list, per-frame drill-down with a decoded
payload column) plus a **CLI / REPL** for headless or SSH work — one-shot
`ls` / `stats` / `show` / `tail` for scripting, and an interactive query prompt
(`filter`, `sort`, `use <channel>`, `show`). The CLI is a thin client over the same
HTTP endpoints (`/traces`, `/stats`, `/frame`) rebuilding the same view model, so
both frontends agree on operations, badges, and payloads.

What the web inspector looks like — an aggregate strip, a filterable operation
list on the left, and a detail pane on the right where every frame of the opened
op is decoded inline (no decode toggle, no reveal step):

```
 ┌─────────────────────────────────────────────────────────────────────────────┐
 │ TrUAPI Wire Inspector   [ filter… ]  [ arrival ▾ ]    ( all ) ( dotli.app )   │
 ├─────────────────────────────────────────────────────────────────────────────┤
 │ 11 OPS  18 FRAMES  72 B  1 LIVE SUB  54ms AVG    5 ORPHANED  6 RETRY STORMS    │
 │                             account.getAccount 6   signing.signRaw 2   …       │
 ├───────────────────────────────────┬───────────────────────────────────────────┤
 │ ▶ system.handshake      2f · 30ms  │ p:2  signing.signRaw   2 frames · 164ms    │
 │ ▶ account.getAccount    2f · 71ms  │                                            │
 │     [RETRY STORM]                  │ ▶ REQUEST  signing.signRaw   22B  +0        │
 │ ▶ signing.signRaw   2f · 164ms  ◀──┤    { "tag": "V1", "value": { "account": {  │
 │ ↻ account.connectionStatus   live  │        "dotNsIdentifier": "alice.dot",     │
 │ ▶ payment.topUp         2f · 92ms  │        "derivationIndex": { … } }, … } }    │
 │ ▶ signing.signRaw   [ORPHANED]     │ ◀ RESPONSE signing.signRaw   4B  ⟳ 164ms   │
 └───────────────────────────────────┴───────────────────────────────────────────┘
   operation list: filter / sort,        detail pane: each frame decoded to its
   health badges, live subscription       SCALE value inline, refused only on
                                          a wire-schema (codec) mismatch
```

And the topology it runs in — the host always dials outward to the loopback server:

```
 dev machine (loopback only)
 ┌──────────────────────────────────────────────┐
 │  web host (browser)          debugger :9231    │
 │  ┌────────────────────┐      ┌──────────────┐  │
 │  │ product (iframe)   │      │ Bun WS + HTTP │  │
 │  │      │ frames       │      │   ├ engine    │  │
 │  │      ▼              │      │   ├ web UI    │  │
 │  │ host worker ─ tap ──┼──ws──▶   └ CLI/REPL  │  │
 │  │        rust wasm    │  ▲   └──────────────┘  │
 │  └────────────────────┘  │                      │
 │        WasmDebugSink → debugEmit → host worker   │
 │        owns the socket + dev-only URL gate       │
 └──────────────────────────────────────────────┘
      host dials outward; loopback bind, Origin +
      Host-header gated (see Confinement)
```

### (B) In-host embed — dotli ribbon

The same inspector, mounted inside the host, with no server and no dial-out. dotli
runs the host in one realm (the host iframe/worker) and the inspector in the top
frame. The tap in the host realm **tees** each `{channelId, dir, frame}` to the top
frame via `window.top` `postMessage`; the top frame feeds them into
`createInAppDebugger().handleFrame(...)`. A right-edge **ribbon** mounts the full
inspector on demand. The realm-tee is payload-blind **transport** — the raw
`ProtocolMessage` bytes cross the postMessage boundary untouched — and the panel
decodes at the point of display (decode-in-panel). Because the frames never leave
the app, each browser tab is its own tenant; there is nothing to host or scope.

```
 dotli (one browser app)
 ┌──────────────────────────────────────────────┐
 │  TOP FRAME (dotli-owned)                        │
 │   createInAppDebugger  ── engine + inspector    │
 │        ▲  handleFrame(channelId, dir, frame)    │
 │        │  window.top.postMessage (raw bytes)    │
 │   ─────┼──────────────  realm boundary  ────    │
 │  HOST REALM (iframe / worker)                    │
 │   host core ── tap ──▶ tee to verified top       │
 │        rust wasm                                 │
 └──────────────────────────────────────────────┘
   right-edge ribbon mounts the inspector on demand
   tee target pinned to a verified dotli-owned top
```

## Decode posture

Stated plainly: this tool **decodes every frame by default**. There is no sensitive
redaction, no denylist, and no reveal toggle. A developer inspecting their own
session's traffic sees the real values, in both surfaces. Decoding reuses
`WIRE_DECODE_TABLE[frameId]` from `@parity/truapi/wire-decode` — the same generated,
dev-only codecs the client uses; the debugger writes none of its own — and is
confined to the per-frame drill-down. The list view (`/traces`) is byte- and
value-free in every configuration: it never serializes raw bytes or decoded
values.

The safety here is not "decode carefully". It is that **the tap and the decoder do
not exist in a production build**:

- The web host reads its debugger URL behind `import.meta.env.DEV`. Vite replaces
  that with a boolean literal, so a production bundle returns `null`
  unconditionally and the tap stays inert — a stray `localStorage` key cannot turn
  it on in prod. Absent a URL, the host never installs `debugEmit`, so the Rust
  core never installs its sink.
- The dotli ribbon is gated on a **build-time** `DEBUG` flag, not the runtime
  `?debug` toggle — a production dotli build has no ribbon to mount.
- `@parity/truapi-debugger` is a private, dev-only package that is not part of any
  product or host production bundle.

**Why "redact the sensitive ones" was dropped.** An earlier draft kept a denylist:
decode everything *except* a hand-marked set of key-bearing methods, backed by a
content check and a gated reveal. That was a false guarantee. A denylist is only as
good as its marking — a secret-bearing method added without the annotation leaks,
and a content check is a heuristic. Worse, the whole edifice implies the tool is
safe-ish to point at real traffic, which invites exactly the use it must never
have. Replacing it with "not in prod, ever" is both simpler and more honest: there
is no production surface to redact, so the guarantee is structural rather than a
list someone has to keep correct.

## Confinement and hardening

Even as a dev tool, each surface is confined to the machine it runs on:

- **Loopback bind.** The standalone server binds `127.0.0.1`, so off-box peers
  cannot reach it.
- **CSWSH Origin gate.** The WebSocket upgrade is refused unless the request's
  `Origin` is a loopback host. A non-browser client (CLI, curl) sends no Origin and
  is allowed; a cross-origin browser page trying to dial the debugger to inject
  frames or drive the decoder is rejected — something binding to loopback alone
  does not prevent.
- **Host-header allowlist (DNS-rebinding guard).** A page served from `evil.com`
  whose DNS has been rebound to `127.0.0.1` can issue same-origin `fetch`es to the
  debugger; those still carry `Host: evil.com`. Requests are 403'd unless the
  `Host` header is an exact loopback name (never a fuzzy match that would read
  `127.0.0.1.evil.com` as loopback).
- **Bounded payloads and retention.** The WS transport caps `maxPayloadLength`
  (1 MiB), and the engine's byte-bounded retention keeps a hostile or runaway
  stream from growing memory without bound.
- **Panic-safe tap.** The `catch_unwind` at the two tap sites keeps a misbehaving
  sink from unwinding into a live dispatch.
- **Loopback-only producer.** The native `WsDebugSink` accepts only a `ws://`
  loopback URL, resolves it, and dials the resolved loopback address directly —
  closing the "validate one string, dial another" gap. No `wss`, no LAN.
- **Pinned realm-tee.** The dotli embed's `postMessage` tee is pinned to a verified
  dotli-owned top frame, so the host realm neither forwards frames to nor accepts a
  mount from an untrusted window.

## Non-goals

- No tap in `@parity/truapi` or the TS transport — the whole point of putting it in
  the core.
- No sensitive-frame redaction, denylist, or reveal path — a dev-only tool decodes
  everything, and production has no tap to redact.
- No mocking or mutation — the tap is one-way today. The `DebugSink` contract
  (`emit`, never wait) is also the extension point: a future sink that reads a
  reply could deliver-modified (mutation) or respond (mock) with no envelope or
  topology change.

## References

- Implementation: truapi#295 — `rust/crates/truapi-server/src/{host_core.rs,
  native_debug.rs}` (tap + sinks), `rust/crates/truapi-codegen` (schema hash),
  `js/packages/truapi-debugger/src` (engine + surfaces).
- Tracking: sdk-team#26 — validation status, per-host sequencing, and remaining
  work live there, not in this doc.
