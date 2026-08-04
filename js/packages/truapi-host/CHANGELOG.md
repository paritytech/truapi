# @parity/truapi-host

## 0.4.0

### Minor Changes

- Publish the RFC-0022 mobile host cutover and completed RFC-0023 account VRF
  signing runtime. Pairing hosts persist product-scoped AutoSigning keys, sign
  matching same-product requests locally, and require structured host and
  Account Holder confirmations before forwarding every other request.

### Patch Changes

- Updated dependencies
  - @parity/truapi@0.7.0

## 0.3.0

### Minor Changes

- Update the WASM host runtime and generated callbacks for tagged 32-byte
  product-account derivation indexes. Implement sr25519 VRF signing through both
  local AutoSigning authorization and account-holder confirmation flows.

### Patch Changes

- Updated dependencies
  - @parity/truapi@0.6.0

## 0.2.1

### Patch Changes

- Update the WASM host runtime so Bulletin preimage submission survives
  `chainHead_follow` interruptions without double-storing: an interrupted watch
  re-checks finalized blocks for the already-broadcast transaction before any
  retry, retries re-broadcast the identical signed bytes instead of re-signing
  with a fresh nonce, and a bounced re-broadcast surfaces as
  inclusion-unverified rather than a failure. Allowance propagation waits are
  now bounded by wall-clock time instead of a best-block count, keeping the
  budget stable across changes in Bulletin's block cadence.

## 0.2.0

### Minor Changes

- Update the WASM host runtime for junction-based ring locations and contextual
  alias/proof reviews. The runtime also exposes login progress after wallet
  approval, routes product and DotNS identity raw signing through their matching
  account-holder messages, and retries transient preimage inclusion lookups.

### Patch Changes

- Updated dependencies
  - @parity/truapi@0.5.0

## 0.1.0

### Minor Changes

- Initial public release of `@parity/truapi-host`: a WASM-backed TrUAPI host runtime that embeds the Rust core. Subpath entries expose the shared host types (`.`), the browser iframe + Web Worker runtime (`/web`), the Worker entry (`/worker-runtime`), and the packaged WASM bundle (`/wasm/web`).
