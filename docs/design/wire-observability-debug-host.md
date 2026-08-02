# Wire Observability and Debug Host

|                    |                                                                                 |
| ------------------ | ------------------------------------------------------------------------------- |
| **Start Date**     | 2026-07-25 |
| **Authors**        | Nidish Ramakrishnan |
| **Implementation** | truapi#295 (Rust tap in `truapi-server`) |
| **Description**    | A payload-blind tap in the Rust host that streams every product↔host frame, under one correlation id, to a debugger app the host dials out to. |

This is a **specification of the contract**, not a walkthrough of the implementation:
it fixes where the tap lives, the topology, the event shape, and the invariants each
layer upholds. Where this doc and the code disagree, this doc is the intent.

## Where the tap lives

The tap is in the **Rust host (`truapi-server`)**, behind a sink trait — **not** in the
TypeScript transport and **not** turned on from the product side. `truapi-server` has
exactly two frame choke points, and every host funnels through them — the web
build via `wasm.rs`, native builds via `native_debug.rs`:

| Direction | Choke point |
|---|---|
| inbound (product → core) | `ProductRuntime::receive_frame()` — taps before dispatch — `host_core.rs` |
| outbound (core → product) | `SinkTransport::send()` — delivers via `FrameSink::emit_frame`, then taps — `host_core.rs` |

One tap implementation covers both platforms, and **nothing in `@parity/truapi` changes**
— the product is genuinely untouched. That the product package does not change at all is the
test for the tap being in the right place: a seam in the product transport would fail it.

## Topology: the host dials the debugger

The **debugger app is a WS server** on the dev machine; **every host dials outward** to it.
This is forced, not a preference — only the inside can initiate a connection:

- native: nothing on a dev machine can dial into a host running inside a device;
- web: nothing outside the browser can dial into a Worker.

```
 WEB                                      NATIVE / DESKTOP
 ┌─ browser ──────────────────┐           ┌─ device ───────────────────┐
 │  product (iframe)          │           │  product (webview)         │
 │      │ MessagePort         │           │      │ ws loopback         │
 │      ▼                     │           │      ▼                     │
 │  host worker ── tap        │           │  ws_bridge ── tap          │
 │      │        rust wasm    │           │      │       rust core     │
 └──────┼─────────────────────┘           └──────┼─────────────────────┘
        │ ws, dialed outward                     │ ws, dialed outward
        └──────────────┬─────────────────────────┘
                       ▼
           ┌──────────────────────┐
           │    debugger app      │   ws server on the dev machine
           └──────────────────────┘
```

Because the host dials the debugger directly, the envelope needs no routing metadata — it is just:

```
 host → debugger    { channelId, dir: "out" | "in", frame: bytes }
```

`frame` is the base64 of the untouched SCALE `ProtocolMessage` bytes (JSON can't carry binary), so
the envelope is a single text WS message.

The debugger groups frames into per-operation traces keyed on **`(channelId, requestId)`** —
not `requestId` alone, since each host mints its own `requestId` sequence and two hosts can
reuse the same id. `channelId` keeps their operations apart and drives the app's per-host view.

### One call, end to end

```
 product                host edge               rust core           debugger app
(iframe / webview)        (tap)                                       (remote)
    │  getAccount()         │                       │                     │
    ├─ frame{ p:1, id=22 } ─▶                       │                     │
    │                       ├┈┈ { dir:out, frame } ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈▶│  log ▲ out p:1
    │                       ├ delivered immediately ▶  ctx.requestId="p:1" │
    │                       ◀─ frame{ p:1, id=23 } ─┤                     │
    ◀ delivered immediately ┤                       │                     │
    │  Ok(response)         ├┈┈ { dir:in, frame } ┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈▶│  log ▼ in  p:1
```

`emit` is fire-and-forget in both legs. **Outbound** the tap delivers to the product
first, then emits; **inbound** it emits before dispatch, so an undecodable frame is still
observed. The order differs; the off-the-critical-path guarantee does not (see Invariants).

## Invariants

1. **In the path, not in the critical path.** The tap carries every frame, so it sees them
   all, yet a slow, absent, or crashed debugger can never stall a session — only the trace is
   lost. The ordering is **asymmetric by design**: outbound (core → product) the tap forwards
   to the product first, then emits; inbound (product → core) it emits first, before
   decode/dispatch, so an undecodable frame is still observed. The guarantee holds either way
   because of invariant 2, not because of a single fixed ordering.
2. **Emit, never wait.** `emit` is fire-and-forget: no reply is read from the debugger socket,
   and it must never block or fail the frame path. That is what keeps the tap off the critical
   path wherever in each leg it fires.

**The second invariant is also the extension point.** Mocking/mutation later needs no
topology or envelope change — the tap starts *reading a reply* on the frames it cares about:
deliver unchanged is today's behaviour, deliver modified is mutation, respond is a mock.
This is why the one-way version ships first.

## Interface (`truapi-server`)

The tap is a **sink trait**, not a hardcoded socket — both to keep the WS transport out of
the core, and because wire frames aren't the only thing worth observing (host-internal
events like SSO have no frame to hang off). The enum leaves room for them.

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
    /// A SCALE wire frame crossing a product channel.
    Frame { channel_id: ChannelId, dir: FrameDirection, bytes: Vec<u8> },
}
```

- Installed per product channel via `ProductRuntime::set_debug_sink(channel_id, sink)`;
  `None` by default, so production pays nothing.
- The concrete sink (the outward WS dial) is provided by the **host adapter**, not the core:
  the web build's `WasmDebugSink` bridges `emit` to a JS `debugEmit` callback (the host worker
  owns the actual `WebSocket` and its dev-only URL gating); the native build's `WsDebugSink`
  implements the trait directly over a loopback `WebSocket`.
- `bytes` are the untouched `ProtocolMessage`; **the debugger app decodes, the core never does.**

## Privacy and security

- Frames stream as **opaque bytes**; the core never decodes them, so nothing at the tap
  reads application content or key material. Decoding happens only in the debugger app.
- **Dev-only**, off in production (the sink is unset). The debugger app runs on a trusted
  dev machine.
- **Residual exposure — metadata, not payload.** Even fully payload-blind, the stream still
  carries, per frame, the direction, the method id and message shape, the byte size, and the
  timing. That is traffic-analysis metadata: whoever holds the debugger socket can see *which*
  operations ran, *when*, and *how big*, with no payload at all. The loopback-only sink and the
  unset-in-production default are what bound who can hold that socket — this is a confinement
  property, not an anonymity guarantee.

## Level 2: value decode (in the debugger, gated, off by default)

Level 1 above is payload-blind end to end: the tap streams bytes, the debugger groups by
`requestId` and shows byte lengths. **Level 2** is a strictly-scoped extension *inside the
debugger only* — decode one frame's payload to a plain JS value in the drill-down detail
path. It changes nothing about the tap, the envelope, or the host: the host still emits
**bytes only**, unchanged. The contract:

- **Off by default, behind a real gate.** Value decode is an explicit dev-only opt-in, read
  once at the debugger's server entry point from `TRUAPI_DEBUGGER_DECODE_VALUES`. With it off,
  the debugger retains no payload bytes and every frame reports its byte length and nothing
  else. The gate is structural, not cosmetic: `@parity/truapi-debugger` is a private, dev-only
  package that is not part of any product or host production bundle, so the decode path cannot
  be reached in a shipped build — it is a build/env boundary, not a code comment and not a
  client-side URL flag.
- **Reuse, don't reinvent.** Decoding is `WIRE_DECODE_TABLE[frameId]?.(bytes)` from
  `@parity/truapi/wire-decode` — the same generated, dev-only codecs the client uses. The
  debugger writes no codecs of its own.
- **Sensitive denylist — the security core.** The generated table can decode *every* frame,
  including signing and login; it deliberately does **not** exclude them. The safety of the
  feature is a denylist the debugger consults *before* the table. Sensitivity is a property of
  the payload **type**, declared at the source: a method carrying key material is marked
  `#[wire(sensitive)]` on the Rust trait, and codegen emits its frame ids into a generated
  `SENSITIVE_FRAME_IDS` set (in `@parity/truapi/wire-table`) that the debugger consumes
  directly. A newly annotated method is denylisted the moment the client is regenerated — no
  name-matching, and a codegen rename cannot silently drop a family. The methods it covers:

  | Family (marked `#[wire(sensitive)]`) | Why redacted |
  |---|---|
  | `signing/*` (create-transaction, sign-raw, sign-payload, each +legacy) | payloads to be signed and the resulting signatures |
  | `account` / `statement-store` create-proof (incl. authorized) | cryptographic proofs bound to a key/identity |
  | `account/sign-vrf` | a VRF signature — key material (RFC 0023) |
  | `entropy/derive` | key-derivation material |
  | `account` request-login + `get-user-id` | login flow and the user id it resolves |
  | `local-storage/read` + `write` | product-controlled storage — can hold tokens, session state, PII (`clear` carries only a key name, so it is not sensitive) |
  | `payment/top-up` | can carry a raw sr25519 secret key (`PaymentTopUpSource`) |
  | coin-payment `create-cheque` / `deposit` / `listen-for-payment` | carry a `CoinPaymentCheque` with redeemable `encryptedSecrets` |
  | statement-store `subscribe` / `submit` | carry a `SignedStatement` with a `decryptionKey` |

  A sensitive frame is **never decoded**, even with the toggle on — it reports its byte
  length under a `redacted · sensitive method` label (the full form carries the withheld
  byte count; see *Redaction is visible* below). The default action for anything under
  security review is **exclude** (bytes only), not decode. Chain reads/broadcasts, chat,
  notifications, permissions, theme, resource-allocation, and preimage carry no key material
  and stay decodable.

  A **fail-closed content check** backs the generated denylist: any decoded value carrying a
  secret-named field (`sr25519SecretKey`, `encryptedSecrets`, `decryptionKey`, a mnemonic, a
  derivation secret, …) is redacted even if its method was somehow not marked — so a
  secret-bearing method added without the annotation is still caught. The `#[wire(sensitive)]`
  set is the authoritative guarantee; the content check is defence in depth for the payload
  fields whose names the type never advertises.

- **Never over the wire, never in `/traces`.** Decode is confined to the per-frame drill-down
  (`GET /frame` and its server-rendered `GET /frame-html`). `/traces` stays byte- and value-free
  in every configuration; a decoded
  value exists only transiently in the drill-down response and is never persisted or relayed.
  Even a sensitive frame's payload transits only as opaque bytes, lives transiently in
  debugger memory, and is never decoded, displayed, or serialized.

- **Dev-only sensitive-reveal escape hatch.** A sensitive frame stays redacted even with decode
  on. A second, independent gate — `TRUAPI_DEBUGGER_REVEAL_SENSITIVE` — can *arm* a reveal, but
  it is meaningful only when decode is also on (the server folds it into the decode gate, so a
  stray reveal env var alone can never arm it), and arming changes no default: the session still
  redacts every sensitive frame until the operator explicitly reveals one. A reveal is a
  per-frame action gated by an explicit confirmation, offered through a distinct danger-styled
  control (never the ordinary decode button), and the value it returns is flagged `sensitive` so
  the UI renders it as the danger it is. Like decode, this gate is read only at the dev server's
  entry point, so a reveal is structurally impossible in any shipped build.
- **Redaction is visible, never silent.** An operation carrying a redacted-by-default method is
  marked with a 🔒 on its op-row (and carries a `data-sensitive` attribute the "🔒 only" top-bar
  filter keys on); each sensitive frame shows a 🔒 lock in the frame list; and a redacted detail
  renders a clear `redacted · sensitive method · <N>B withheld` label with the byte length, never the value. These
  markers are payload-blind — they reveal nothing the method name doesn't.

## Frontends

One trace/decode/denylist engine drives two frontends, so a reviewer's choice of tool never
changes what is shown or what is decodable:

- A **web inspector** — a full-screen, Network-tab-style app the host dials into: an aggregate
  summary strip, a filterable/sortable operation list, and a drill-down with the blur-to-reveal
  payload column, Decode-all/Encode-all, and the gated reveal.
- A **terminal frontend** — for headless / SSH / CI work where a browser isn't reachable:
  one-shot `ls` / `stats` / `show` / `tail` for scripting, plus an interactive query REPL you
  keep querying (filter, sort, `use <channel>`, `show`, `reveal`).

The terminal frontend is a thin client over the running debugger's HTTP endpoints
(`/traces`, `/stats`, and the gated `/frame`) that rebuilds the **same** shared view model — it
forks neither the engine nor the sensitive denylist. Both frontends therefore agree on
operations, badges, sensitivity, redaction, and what may be revealed; the reveal gate is enforced
server-side, so neither frontend can surface a value the server would withhold.

## Hosts and scope

The tap and envelope apply uniformly to any `truapi-server` host — web, desktop, or mobile —
each of which dials the debugger with its own platform sink. Two outward-dial sinks exist today:
the web `WasmDebugSink` (bridging `emit` to a JS `debugEmit` the host worker owns) and the
native `WsDebugSink` — a loopback-only, fire-and-forget, WASM-safe WebSocket client, compiled
only under the `ws-bridge` feature (the same feature a native host's transport uses), so it is
out of the wasm graph and out of default builds. The native sink ships here as a tested,
ready-to-install seam: the CLI host (truapi#264) is on `main` and is its intended first
consumer, but does not yet install it — wiring `WsDebugSink` into a host's runtime and
surfacing its traces is that host's own integration surface, done next.

## Non-goals

- No tap in `@parity/truapi` / the TS transport (the whole point of putting it in the core).
- No mocking/mutation in v1 — but the envelope and topology already accommodate it.

## References
- Implementation: truapi#295 (`rust/crates/truapi-server/src/host_core.rs` — `DebugSink`/`DebugEvent`/tap).
- Tracking: sdk-team#26 — validation status, per-host sequencing, and remaining work live there, not in this doc.
