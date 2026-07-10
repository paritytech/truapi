---
"@parity/truapi": patch
---

Add the `@parity/truapi/sandbox` entry point: host-environment detection (`isCorrectEnvironment`), a lazily-built cached client (`getClientSync`, `null` outside a host container), and a `subscribeConnectionStatus` connected/disconnected listener. Browser-embedded hosts can bootstrap a client without assembling the transport by hand.
