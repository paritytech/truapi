# @parity/truapi-host

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
