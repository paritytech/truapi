# @parity/truapi-debugger

The debugger-side consumer for TrUAPI wire frames. **Private, in-repo, not published.**

The host taps every product↔host wire frame in its Rust core (`truapi-server`'s
`DebugSink`) and streams each one outward as a `{ channelId, dir, frame: bytes }`
envelope. This package is the other end: it decodes the wire *envelope* (the
`requestId` and frame id, via `decodeWireMessage`) and groups frames into
per-operation traces. The trace view stays payload-blind — it never decodes the
frame payload. Envelope decoding lives here, in the debugger, never in the host
core, which treats frames as opaque bytes.

This keeps `@parity/truapi` (the product package) genuinely untouched: the tap is
in the Rust host, and the debugger's decode/trace logic lives here instead
of in the product transport.

> **Scope note.** This package holds both the debugger *library* (the
> trace + envelope-decode engines + the ingest that turns a wire envelope into a
> decoded frame) and a minimal *runnable app* (`server.ts`: the WS server a host
> dials into, plus a tiny trace view). It lives in-repo because the debugger is
> coupled to the protocol this repo owns — it decodes wire frames with
> `@parity/truapi`, tracking the generated wire surface. *Where the app
> ultimately lives* (stays a truapi tool /
> own repo / a desktop app) is still an open decision for the host-protocol
> owner; in-repo now is the low-regret default and moving it later is cheap. See
> `docs/design/wire-observability-debug-host.md`.

## What's here

- **`createDebugSession()`** — the trace engine wired to the ingest. Feed it
  envelopes with `handleEnvelope(...)`; read grouped traces from `traceEngine`.
- **`createDebugIngest(sink)`** — decodes a `DebugFrameEnvelope` into an
  `ObservedFrame` and forwards it. The layer that turns raw wire bytes into
  something the trace engine can group.
- **`createWireDebugger(...)`** — accumulates observed frames into per-`requestId`
  traces (correlates with product-sdk telemetry spans on the same id).
- **`createFrameDecoder(...)`** — the level-2 value decoder (see below): a gated,
  per-frame decode of a payload to a plain JS value, reusing `@parity/truapi`'s
  generated `WIRE_DECODE_TABLE` behind a dev-only opt-in and a sensitive-method
  denylist.
- **`startDebugServer(...)`** (`server.ts`) — the runnable app: a Bun WS+HTTP
  server. A host dials the WS and sends one text message per frame,
  `{ channelId, dir, frame }` with `frame` base64-encoded; `GET /traces` returns
  the grouped traces (payload-blind), `GET /frame?id=&i=` is the per-frame
  drill-down (see below), `GET /` serves the view.

## Value decode (level 2 — dev-only, off by default)

By default the debugger is **payload-blind**: it groups frames and shows byte
lengths, never their contents. A separate, opt-in **level-2** capability can
decode a single frame's payload to a plain JS value in the drill-down detail
path. Its contract:

- **Off by default.** The server enables it only when
  `TRUAPI_DEBUGGER_DECODE_VALUES` is truthy (`startDebugServer({ decodeValues })`
  in code). With it off, every frame reports byte length only, and no bytes are
  even retained.
- **Reuses the generated table.** Decoding is `WIRE_DECODE_TABLE[frameId]?.(bytes)`
  from `@parity/truapi/wire-decode` — the same dev-only codecs the client uses.
  The debugger writes none of its own.
- **Sensitive denylist.** The generated table decodes *every* frame, including
  signing and login. The security of this feature is the denylist layered on
  top: the generated `SENSITIVE_FRAME_IDS` set in `@parity/truapi/wire-table`,
  emitted from every method marked `#[wire(..., sensitive)]` on the Rust trait —
  so sensitivity is a property of the payload type, and a codegen rename cannot
  silently drop a family. It covers **signing/\*** (create-transaction, sign-raw,
  sign-payload, and their legacy variants), **\*create\*proof\*** (account +
  statement-store, incl. authorized), **entropy/derive**, **SSO/login +
  get-user-id**, **local-storage read/write** (`clear` carries only a key name,
  so it stays decodable), **payment/top-up**,
  **coin-payment create-cheque/deposit/listen-for-payment**, and
  **statement-store subscribe/submit**. A sensitive frame is never decoded — it
  reports its byte length labelled `redacted: sensitive method`, even with the
  toggle on. A fail-closed content check (any secret-named field in a decoded
  value) backs it up for any secret-bearing method that was never annotated.
- **Never over the wire, never in `/traces`.** The host still emits opaque bytes
  only; nothing about decode changes what it sends. `/traces` never serializes
  raw bytes or decoded values. Decode happens only in the debugger, only in the
  `/frame` drill-down.

## Run

```bash
npm install   # links @parity/truapi via the workspace
npm run build # tsc -b
npm run serve # bun run src/server.ts — listens on :9231

# opt into level-2 value decode (dev machines only)
TRUAPI_DEBUGGER_DECODE_VALUES=1 npm run serve
```

Point a host's debugger URL at `ws://<dev-machine>:9231` (the host dials out),
open `http://localhost:9231/` for the trace view; click a frame for its
drill-down detail. The exact host↔debugger framing is provisional (envelope
spec, track T3); base64-in-JSON is what the server accepts today.
