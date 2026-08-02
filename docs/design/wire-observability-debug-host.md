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
exactly two frame choke points, and every host (web via `wasm.rs`, native via a
`ws_bridge` `FrameSink`) funnels through them:

| Direction | Choke point |
|---|---|
| inbound (product → core) | `ProductRuntime::receive_frame()` — `host_core.rs` |
| outbound (core → product) | `FrameSink::emit_frame()` / `SinkTransport::send()` — `host_core.rs` |

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
  the web host bridges `emit` to a JS `WebSocket`; native hosts do the equivalent on their platform.
- `bytes` are the untouched `ProtocolMessage`; **the debugger app decodes, the core never does.**

## Privacy and security

- Frames stream as **opaque bytes**; the core never decodes them, so nothing at the tap
  reads application content or key material. Decoding happens only in the debugger app.
- **Dev-only**, off in production (the sink is unset). The debugger app runs on a trusted
  dev machine.

## Hosts and scope

Every host taps at its `truapi-server` choke points and dials the debugger app: **dotli**
(web), **Polkadot Desktop**, **iOS**, **Android**. Today those hosts run the
`@novasamatech/host-*` stack, so each adopts this once it runs `truapi-server`; the tap and
envelope are identical across all of them. Each host supplies its own outward-dial sink for
its platform — the web sink (bridging `emit` to a JS `WebSocket`) is built; the native sinks
are phase-2.

## Non-goals

- No tap in `@parity/truapi` / the TS transport (the whole point of putting it in the core).
- No mocking/mutation in v1 — but the envelope and topology already accommodate it.

## Validation status

- **Automated (Rust tap):** both choke points tapped, `channel_id` per event, bytes untouched and
  in order — proven by `cargo test` (`debug_sink_taps_frames_in_both_directions`); the product-vantage
  direction convention (`out` = left the product) by `frame_direction_wire_str_is_product_vantage`.
  Inert-when-unset is structural (the `has_debug` fast path skips the tap when no sink is installed);
  off-the-critical-path follows from invariant 2 — `emit`'s result is discarded and never awaited, and
  the sink contract forbids blocking or panicking.
- **Automated (debugger app):** the WS server end (`@parity/truapi-debugger`, in-repo) — a host
  dials in, streams a frame, the server decodes + groups it and serves it on `/traces` — proven by
  its `bun test` against a live server. The exact host→debugger framing (base64-in-JSON today) is
  still provisional pending the envelope spec.
- **Built (web outward dial):** the web host's sink — `WasmDebugSink` bridging `emit` to a JS
  `WebSocket`, dev-gated via a `localStorage` debugger URL that the host worker reads — is wired end
  to end, and a product running under the `@parity/truapi-host` WASM host streams every frame to the
  debugger, verified against a local host harness.
- **Not yet built:** the native outward-dial sink. This is the native-integration track (phase-2).

## References
- Implementation: truapi#295 (`rust/crates/truapi-server/src/host_core.rs` — `DebugSink`/`DebugEvent`/tap).
