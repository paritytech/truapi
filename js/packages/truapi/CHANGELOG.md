# @parity/truapi

## 0.3.2

### Minor Changes

- 621a48c: Rename the exported `Provider` transport type to `WireProvider` to make its role explicit. It is the low-level SCALE-wire-frame pipe (a `MessagePort` or iframe `postMessage` channel) that `createTransport` runs on. The `createIframeProvider` / `createMessagePortProvider` factories are unchanged; only the type name moves. Consumers importing `Provider` should import `WireProvider` instead.
- 130789d: Add the `@parity/truapi/sandbox` entry point: host-environment detection (`isCorrectEnvironment`), a lazily-built cached client (`getClientSync`, `null` outside a host container), and a `subscribeConnectionStatus` connected/disconnected listener. Browser-embedded hosts can bootstrap a client without assembling the transport by hand.

## 0.3.1

### Patch Changes

- Fixed `HostPaymentTopUpError` SCALE variant ordering: `PartialPayment` (index 2) now precedes `Unknown` (index 3), matching the canonical wire layout.
- Fixed explorer 0.3.1 snapshot import paths.

## 0.1.0

### Minor Changes

- Initial public release of `@parity/truapi`: TrUAPI transport, SCALE codecs, and the generated TypeScript API client for protocol v1.0.
