# @parity/truapi

## 0.5.0

### Minor Changes

- Redesign account alias and ring-VRF proof requests around stable, junction-based
  ring locations and product-scoped proof contexts. Proof responses now include
  the contextual alias, ring index, and ring revision, with distinct
  `RingNotFound` and `NotMember` errors.

## 0.4.1

### Patch Changes

- Treat Firefox's masked `"null"` `location.ancestorOrigins` entries as an unknown host origin in the sandbox bootstrap. The ready ping falls back to the source-checked wildcard instead of throwing `SyntaxError: An invalid or illegal string was specified`, which left iframe-hosted products permanently offline in Firefox.

## 0.4.0

### Minor Changes

- Add the `coinPayment` client namespace (RFC 0017 Coinage Payment): `createPurse`, `queryPurse`, `rebalancePurse`, `deletePurse`, `deposit`, `refund`, `createCheque`, `createReceivable`, and `listenForPayment`, with the `CoinPayment*` / `HostCoinPayment*` / `VersionedHostCoinPayment*` request/response/error types and their wire discriminants.

  **Breaking:** the `CallError<D>` SCALE codec now decodes to a tagged `CallErrorValue<D>` union (`Domain` / `Denied` / `Unsupported` / `MalformedFrame` / `HostFailure`) instead of projecting only the domain error and throwing on framework-level failures. The `Transport.truapiVersion` field is removed and `Transport.codecVersion` is deprecated; generated handshake calls read the codec version directly.

## 0.3.2

### Minor Changes

- Rename the exported `Provider` transport type to `WireProvider` to make its role explicit. It is the low-level SCALE-wire-frame pipe (a `MessagePort` or iframe `postMessage` channel) that `createTransport` runs on. The `createIframeProvider` / `createMessagePortProvider` factories are unchanged; only the type name moves. Consumers importing `Provider` should import `WireProvider` instead.
- Add the `@parity/truapi/sandbox` entry point: host-environment detection (`isCorrectEnvironment`), a lazily-built cached client (`getClientSync`, `null` outside a host container), and a `subscribeConnectionStatus` connected/disconnected listener. Browser-embedded hosts can bootstrap a client without assembling the transport by hand.

## 0.3.1

### Patch Changes

- Fixed `HostPaymentTopUpError` SCALE variant ordering: `PartialPayment` (index 2) now precedes `Unknown` (index 3), matching the canonical wire layout.
- Fixed explorer 0.3.1 snapshot import paths.

## 0.1.0

### Minor Changes

- Initial public release of `@parity/truapi`: TrUAPI transport, SCALE codecs, and the generated TypeScript API client for protocol v1.0.
