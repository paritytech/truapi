# truapi-server

*Runtime core for TrUAPI: dispatcher, protocol frames, SCALE-coded wire envelope.*

## What this crate is for

`truapi-server` is the runtime that turns trait implementations of the
`truapi` API into a working host. It owns:

- the [`ProtocolMessage`] wire envelope and SCALE codec
- the [`Dispatcher`] that routes incoming frames to per-method handlers
- the subscription lifecycle (start/receive/stop/interrupt)
- the [`Transport`] trait that platform-specific IPC backends implement
- the auto-generated dispatcher/wire-table tables shipped under
  [`crate::generated`]

## Wire envelope

Every frame on the wire is encoded as:

```text
[requestId: SCALE str][discriminant: u8][payload bytes...]
```

The discriminant identifies a method + frame kind via the auto-generated
[`crate::generated::wire_table::WIRE_TABLE`]. Each method's ids are exposed
as a named const (`PREIMAGE_SUBMIT`, ...); both `WIRE_TABLE` and the generated
dispatcher reference those consts. Method ordering is part of the wire
protocol; only ever append.

The payload bytes are the SCALE-encoded inner value, inlined without a
length prefix. The discriminant is carried directly as `Payload::id`, and the
dispatcher routes on that numeric id via id-keyed tables.
