---
"@parity/truapi": minor
---

Add the `@parity/truapi/sandbox` entry point: host-environment detection (`isCorrectEnvironment`) and a cached client via `getClientSync` (no handshake) / `getClient` (runs `system.handshake` once) / `isReady`, plus a `subscribeConnectionStatus` status listener. Browser-embedded hosts can bootstrap a client without assembling the transport by hand.
