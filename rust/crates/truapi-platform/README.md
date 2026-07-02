# truapi-platform

Platform capability traits for TrUAPI host implementations.

Each host (web/WASM, desktop, iOS/UniFFI, Android/UniFFI) implements these
traits to provide the native capabilities the shared Rust runtime cannot reach
directly. The dispatcher in `truapi-server` calls this surface while the Rust
runtime owns product account management, SSO signing, statement-store protocol
flows, permission state, and auth state transitions.

## Type Imports

Host-facing wire types are imported from `truapi::latest` by this crate and are
exposed through the trait signatures below.

## Host Callback Traits

- `ProductStorage`: product-scoped key-value storage.
- `CoreStorage`: typed core-owned storage slots such as auth session, pairing
  identity, and permission authorization state.
- `Navigation`: open URLs in the system browser.
- `Notifications`: deliver and cancel push notifications.
- `Permissions`: prompt for device and remote authorizations.
- `Features`: report host feature support.
- `ChainProvider` / `JsonRpcConnection`: open JSON-RPC connections to chains.
- `AuthPresenter`: render core-owned auth state transitions.
- `UserConfirmation`: confirm signing, transaction, resource, alias, and
  preimage actions before the core asks the paired wallet.
- `ThemeHost`: stream the host theme into the runtime.
- `PreimageHost`: submit and look up preimages through the host-selected backend.

`Platform` is a blanket-implemented supertrait that combines the capability
traits above.

## Core-Owned Admin API

`CoreAdmin` is not part of the host-provided `Platform` callback surface. It is
the core-owned control API exposed to host UI for logout, pairing cancellation,
session-store refresh, and permission administration.

## Mock platform (`mock` feature)

The `mock` feature — a dev-dependency, excluded from the default and production
builds — provides `MockPlatform`, a config-driven, in-memory implementation of all
the capability traits above. It is the canonical seam mock: plug it into the real
`truapi-server` core (via `TrUApiCore::from_platform_with_config`) to exercise the
production dispatcher without a device or a paired wallet.

`MockConfig` drives its behavior — `PermissionPolicy` (`AllowAll`/`DenyAll`, device
and remote separately), `ChainBehavior` (`Silent` | `Scripted` | `Closed` |
`ConnectError`), `MockFaults` for fault injection, and confirmation control — and it
records what the core asked the device to do (navigations, notifications,
confirmations, auth-state transitions, sent RPC) for assertions. Storage is
namespaced in-memory (`product:` / `core:`); preimages round-trip via submit → lookup.
See the `from_mock_platform_*` through-core tests in `truapi-server` for end-to-end
usage.
