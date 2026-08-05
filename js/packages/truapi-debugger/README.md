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
  generated `WIRE_DECODE_TABLE`. A dev-only tool that decodes every frame it can,
  with no sensitive special-casing.
- **`startDebugServer(...)`** (`server.ts`) — the runnable app: a Bun WS+HTTP
  server. A host dials the WS and sends one text message per frame,
  `{ channelId, dir, frame }` with `frame` base64-encoded; `GET /traces` returns
  the grouped traces (payload-blind), `GET /frame?id=&i=` is the per-frame
  drill-down (see below), `GET /` serves the view.

## Value decode (level 2 — dev-only, on by default)

This is a **dev-only tool that decodes everything**. The list views stay
payload-blind — they group frames and show byte lengths, never their contents —
but the **level-2** drill-down decodes a single frame's payload to a plain JS
value, for every frame, with no "sensitive" special-casing. Its contract:

- **On by default.** The server decodes unless
  `TRUAPI_DEBUGGER_DECODE_VALUES` is set to a falsy value (`0`/`false`/`no`/`off`),
  or `startDebugServer({ decodeValues: false })` in code — useful for a demo.
  With decode off, every frame reports byte length only, and no bytes are even
  retained.
- **Reuses the generated table.** Decoding is `WIRE_DECODE_TABLE[frameId]?.(bytes)`
  from `@parity/truapi/wire-decode` — the same dev-only codecs the client uses.
  The debugger writes none of its own.
- **No redaction, no reveal toggle.** Every frame the table can decode is
  decoded, including signing, login, and payment. A developer inspecting their
  own session's traffic sees the real values; there is no denylist, no reveal
  escape hatch, and no `redacted` state. A frame renders either its decoded value
  or, when it has no codec / no retained bytes / fails to decode, its byte length.
- **Never over the wire, never in `/traces`.** The host still emits opaque bytes
  only; nothing about decode changes what it sends. `/traces` never serializes
  raw bytes or decoded values. Decode happens only in the debugger, only in the
  `/frame` drill-down.

## Run

```bash
npm install   # links @parity/truapi via the workspace
npm run build # tsc -b
npm run serve # bun run src/server.ts — listens on :9231, decodes by default

# turn value decode off for a demo
TRUAPI_DEBUGGER_DECODE_VALUES=0 npm run serve
```

Point a host's debugger URL at `ws://<dev-machine>:9231` (the host dials out),
open `http://localhost:9231/` for the trace view; click a frame for its
drill-down detail. The exact host↔debugger framing is provisional (envelope
spec, track T3); base64-in-JSON is what the server accepts today.
