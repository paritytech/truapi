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

The discriminant maps to a method/kind tag via the auto-generated
[`crate::generated::wire_table::WIRE_TABLE`]. Method ordering is part of
the wire protocol; only ever append.

The payload bytes are the SCALE-encoded inner value, inlined without a
length prefix. In-memory we keep the tag as a `String` so the dispatcher
(which keys on method name) is independent of the wire numbering.
