---
"@parity/truapi": minor
---

Add the `@parity/truapi/sandbox` entry point: host-environment detection (`isCorrectEnvironment`) and a cached client exposing both a promise lifecycle (`getClient` / `getClientSync` / `getClientOrThrow` / `isReady`) and a status-listener lifecycle (`subscribeConnectionStatus`). Browser-embedded hosts can bootstrap a client without assembling the transport by hand.
