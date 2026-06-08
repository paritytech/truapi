# 05 - Migration Plan

> Parent: [dotli shared Rust core migration](<index.md>).

The work should be split by ownership boundary, not by TrUAPI method. Account,
signing, statements, allocation, and entropy all share session state and SSO
protocol machinery, so slicing them per API domain creates avoidable protocol
drift.

## Workstreams

| Workstream | Driver | Scope |
|---|---|---|
| Platform contract | Platform/bindings owner | `RuntimeConfig`, `PairingPresenter`, `SessionStore`, notification id/cancel, theme, preimage, confirmation hooks, WASM/UniFFI parity. |
| Core protocol | Core Rust owner | statement-store client, SSO pairing, session lifecycle, account derivation, signing, alias, allocation, statement proofs, entropy. |
| dotli adapter | dotli owner | web worker wiring, QR/modal adapters, storage adapters, notification/theme/preimage adapters, feature flag rollout, Nova package deletion. |
| Validation | Test/parity owner | vectors, mock SSO peer, dotli parity matrix, wasm/native checks, no-Nova-deps gate. |

## Review Slices

1. **Contract slice**
   - Add runtime config and host extension traits.
   - Generate/update WASM, TS, Kotlin, and Swift surfaces.
   - No SSO protocol yet.

2. **Vector and fixture slice**
   - Capture product-account, entropy, statement-proof, and SSO fixtures from
     dotli main / Nova package behavior.
   - Add native and wasm tests.

3. **Session and pairing slice**
   - Implement `SessionStore` restore.
   - Implement SSO pairing through People-chain statement store.
   - Implement logout/disconnect cleanup.

4. **Product method slice**
   - Account, user id, alias.
   - Signing/raw signing/create transaction, including legacy variants.
   - Resource allocation.
   - Statement-store submit/subscribe/proof.
   - Entropy.

5. **Host-backed parity slice**
   - Notifications schedule/cancel.
   - Theme subscription.
   - Preimage submit/lookup.
   - Navigation and permission parity adjustments.

6. **dotli cutover slice**
   - Route products through `@parity/truapi`.
   - Keep old Nova path behind a temporary switch until parity flows pass.
   - Delete Nova bridge code and runtime dependencies.

## Dependency Diagram

```
PR 104 runtime substrate
        |
        v
Platform contract + RuntimeConfig
        |
        +----> dotli adapter skeleton
        |
        v
Vectors and mock peer
        |
        v
Statement-store client
        |
        v
SSO pairing + SessionStore
        |
        v
Session-channel methods
        |
        +----> signing / transaction / alias / allocation
        +----> statement-store / proof / entropy
        |
        v
host-backed parity facades
        |
        v
dotli Nova dependency removal
```

## Acceptance Gates

- `make check` on the TrUAPI workspace.
- `cargo test --workspace --features ws-bridge`.
- `cargo check -p truapi-server --target wasm32-unknown-unknown`.
- Fresh codegen produces no diff.
- Native and wasm vector tests pass.
- dotli can:
  - pair through SSO;
  - restore across reload;
  - logout and react to peer disconnect;
  - derive product accounts;
  - expose connection status;
  - reveal user id after permission;
  - sign payload/raw and create transactions;
  - run legacy signing variants;
  - derive alias;
  - request resource allocation;
  - submit/subscribe statements and create proofs;
  - derive entropy from the current dotli `ssSecret` input;
  - read/write/clear local storage;
  - navigate;
  - handle permissions;
  - schedule/cancel notifications;
  - submit/lookup preimages;
  - subscribe to theme changes.
- dotli keeps explicit typed unavailable behavior for payments.
- `rg "@novasamatech/(host-api|host-container|host-papp|statement-store|sdk-statement|storage-adapter)"`
  finds no runtime dependency/imports in the migrated dotli packages.

## Rollout

Use a temporary runtime switch:

```
Nova host-container path  <---- feature flag ---->  Rust core path
```

The flag is removed only after the acceptance gates pass. The migrated session
store is intentionally new; users may need to pair once after cutover.
