# truapi-platform

Platform capability traits for TrUAPI host implementations.

Each platform (web/WASM, iOS/UniFFI, Android/UniFFI) implements these traits to
provide native capabilities. The TrUAPI dispatcher (in `truapi-server`) calls
into these when handling API requests from the product side.

## Traits

- `Storage`, scoped key-value storage (`read`, `write`, `clear`).
- `Navigation`, open URLs in the system browser.
- `Notifications`, deliver push notifications.
- `Permissions`, prompt for device and remote permissions (v0.1 split callbacks).
- `Features`, probe per-chain (or other) feature support.
- `ChainProvider` / `JsonRpcConnection`, open JSON-RPC connections to chains.
- `Accounts`, product account lookup, aliasing, proofs, identity, connection status.
- `Signing`, sign payloads and raw byte/string blobs.
- `StatementStore`, subscribe, submit, and prove statements.
- `Preimage`, lookup preimage data.

`Platform` is a blanket-implemented supertrait that combines all of the above.

## Versioning

Types come from `truapi::versioned::*` (V1 enum wrappers) where possible, and
fall back to `truapi::v01::*` for inner error and value types. The crate
re-exports both modules for downstream convenience.
