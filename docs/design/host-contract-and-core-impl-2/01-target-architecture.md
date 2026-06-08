# 01 - Target Architecture

> Parent: [dotli shared Rust core migration](<index.md>).

## Before

Today dotli main runs product protocol behavior in JS:

```
Product iframe
  @novasamatech product client
        |
        | postMessage / host-container wire
        v
dotli UI main thread
  container.ts
  auth.ts + host-papp
  statement-store + sdk-statement
  permissions, notifications, preimage, theme, storage
        |
        +--> People-chain statement store
        +--> dotli protocol iframe / chain bridge
        +--> browser UI and storage APIs
```

This works for web, but the protocol and parity rules live in dotli-specific
TypeScript. Other hosts would need to re-create the same behavior.

## After

The Rust core owns product protocol behavior. The host owns OS/UI primitives.

```
Product iframe / WebView
  @parity/truapi generated client
        |
        | TrUAPI wire bytes
        v
+------------------------------------------------------+
| truapi-server shared Rust core                       |
|                                                      |
| - generated dispatcher                               |
| - session restore and login state                    |
| - SSO pairing and encrypted message exchange         |
| - account derivation and user identity responses     |
| - signing / create-transaction request routing       |
| - statement-store submit / subscribe / proof         |
| - resource allocation request routing                |
| - entropy derivation                                 |
+-----------------------------+------------------------+
                              |
                              | truapi-platform + host extensions
                              v
Host shell
  WASM worker on web/Electron
  UniFFI bridge on iOS/Android
  storage, modals, QR/deeplinks, notifications, theme,
  preimage backend, chain connections
        |
        +--> People-chain statement store
        +--> browser/native UI and storage APIs
```

The core is the portable boundary. Host adapters may differ by platform, but
they must not fork SSO, signing, statement-store proof logic, product account
derivation, session semantics, or TrUAPI wire behavior.

## Runtime Placement

PR 104 introduces the runtime substrate:

- product clients generated from Rust protocol traits;
- `truapi-server` dispatcher and runtime;
- `truapi-platform` host capabilities;
- WASM packages for web/Electron;
- UniFFI/Kotlin/Swift bindings for native hosts.

The dotli migration should start with the PR 104 placement model:

```
dotli host page
    |
    +-- product iframe
    |     @parity/truapi client
    |
    +-- protocol/runtime host
          WASM core in worker
          host callbacks routed to dotli UI where needed
```

A future SharedWorker or protocol-iframe optimization can reduce duplicate
cores across tabs, but it is not required for feature parity. The migration
should keep that optimization possible by keeping all core state behind
`SessionStore`, `Storage`, and `ChainProvider` instead of relying on JS globals.

## SSO Ownership

There is no separate wallet socket or wallet host primitive.

```
Core
  builds QR/deeplink
  submits and subscribes to People-chain statements
  verifies pairing response
  derives encrypted session channels
  sends sign / alias / transaction / allocation requests

Phone wallet / SSO peer
  scans QR
  approves pairing
  holds root keys and ring secret
  signs or derives on request
  replies through the same statement-store protocol
```

The host can present UI and open chain connections, but it cannot claim a
session or sign for the wallet.

## Product Identity

Each runtime instance is configured with product identity before it handles
requests:

```rust
pub struct RuntimeConfig {
    pub product_label: String,
    pub product_id: String,
    pub site_id: String,
    pub host_metadata_url: String,
    pub people_chain_genesis_hash: [u8; 32],
    pub pairing_deeplink_scheme: PairingDeeplinkScheme,
}
```

dotli derives `product_id` from the label using current main behavior:

- development preview labels keep the bare label;
- deployed dot products use `<label>.dot`.

The core receives the resulting value. It should not know dotli DNS rules.
