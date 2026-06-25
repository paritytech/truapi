---
"@parity/truapi": minor
---

Rename the exported `Provider` transport type to `WireProvider` to make its role explicit — it is the low-level SCALE-wire-frame pipe (a `MessagePort` or iframe `postMessage` channel) that `createTransport` runs on. The `createIframeProvider` / `createMessagePortProvider` factories are unchanged; only the type name moves. Consumers importing `Provider` should import `WireProvider` instead.
