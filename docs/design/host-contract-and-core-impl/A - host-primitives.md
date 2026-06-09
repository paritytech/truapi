# A - Host primitives (`truapi-platform`)

> Part of the [host-contract & core-impl spec](index.md).

SSO pairing, transaction/raw signing, transaction construction, ring-VRF alias, and the product
statement store all ride **one** mechanism: the People-chain statement store, reached through the
**existing `ChainProvider`** trait, with the protocol run **in the core** ([H](<H - sso-pairing-protocol.md>)).
That leaves exactly **one** genuinely new host primitive for the SSO protocol: a QR presenter.

Replacing dotli's current `@novasamatech/host-container` bridge still requires a few host/platform
surfaces for behavior that is already implemented in the current dotli checkout or required by the
core-owned session model: scheduled notifications, theme subscription, preimage submit/lookup,
resource-allocation confirmation UI, host-global session persistence, and static product identity. Those
are not SSO protocol primitives, but they are part of current dotli feature parity or secure restore. For
the trait conventions and the five-layer wiring recipe, see the [annex](<G - annex.md>).

## A0. Delta from PR 104's platform surface

PR 104 currently ships `truapi-platform` with:

- `Storage`, `Navigation`, `Notifications`, `Permissions`, `Features`, `ChainProvider`, and
  `JsonRpcConnection`;
- no runtime product config object;
- no SSO QR presenter;
- no host-global `SessionStore`;
- no theme/preimage/resource-allocation-confirmation traits;
- a `Notifications` trait that can push but cannot return a notification id or cancel scheduled
  notifications.

This spec keeps `Storage`, `Navigation`, `Permissions`, `Features`, and `ChainProvider` as real host
capabilities. It extends or adds only the host surfaces listed below. Account, signing, transaction
construction, alias, statement-store submit/subscribe/proof, entropy, and logout are **not** host
callbacks in the final architecture.

## A0.1. Portability rules

No method in `truapi-server` may depend on dotli-only APIs, package names, storage keys, browser globals,
or UI components. dotli supplies those details through its adapter layer and `RuntimeConfig`; the Rust core
sees only platform traits and opaque configuration values.

The same host contract must be usable from:

- **dotli web/Electron:** WASM worker runtime; QR, modals, notifications, preimage, theme, and
  host-global `SessionStore` backed by web/Electron APIs.
- **iOS/Android:** UniFFI/native runtime; the same QR/deeplink, session-store, notification, chain, and UI
  confirmation capabilities implemented with native platform APIs.
- **future hosts:** one new platform implementation plus product config. No forked SSO protocol, no
  host-owned signing proxy, and no separate wallet connection.

## A1. `PairingPresenter`: show the QR/deeplink **(S–M)**

**What:** display the `polkadotapp://pair?handshake=<hex>` deeplink the core builds during
`request_login`, as a QR (or open-url), and report user-initiated cancel. No protocol knowledge: it
renders a string the core produced ([H §1](<H - sso-pairing-protocol.md>)).

**Proposed trait** (lib.rs):

```rust
pub trait PairingPresenter: Send + Sync {
    /// Show the pairing deeplink/QR. The future resolves when the user dismisses/cancels.
    /// The core drops it on success/abort, which the host treats as "close the modal".
    fn present(&self, deeplink: String) -> impl Future<Output = ()> + Send;
}
```

The drop-to-close, resolve-on-cancel shape lets `request_login` `select!` over {handshake statement
arrives, present-future resolves (cancel), `cx.cancel()`}.

**Bridge:** `present` takes a `String` (like `navigate_to`) but must resolve on cancel, so it needs a
bespoke invoker: a JS callback returning a `Promise<void>` that resolves only when the user closes the
QR. **dotli mapping:** `topbar.ts` renders it via `QRCode.toCanvas(payload)`; the modal's close button
resolves the promise. **Acceptance:** the core drives show/close; closing the modal aborts pairing.

## A2. `RuntimeConfig`: static product and pairing inputs **(S)**

Each product iframe/runtime needs values that are fixed before the core can answer product-scoped
methods or build the pairing QR:

```rust
pub struct RuntimeConfig {
    pub product_id: String,
    pub product_label: String,
    pub site_id: String,
    pub host_name: String,
    pub host_icon: Option<String>,
    pub host_version: Option<String>,
    pub platform_type: Option<String>,
    pub platform_version: Option<String>,
    pub people_chain_genesis_hash: [u8; 32],
    pub pairing_deeplink_scheme: PairingDeeplinkScheme, // polkadotapp:// or polkadotappdev://
}
```

This is constructor/configuration state on `PlatformRuntimeHost`, WASM, UniFFI, and the JS worker package,
not a host callback and not part of every request payload. Current dotli creates one container per
product label and derives the product id with `labelToProductIdentifier(label)` in
`~/github/dotli/packages/ui/src/container.ts`. The same value is used by `get_legacy_accounts`, alias
permission prompts, signing validation, resource allocation, and product entropy derivation, so the Rust
runtime should be instantiated per product with this identity already known. Nested dApps are not a
separate runtime/config boundary for v1; if dotli keeps nested message forwarding, nested traffic uses the
same Rust core and product identity as the containing product. The current JS nested bridge behavior and
its possible future value are tracked separately in [I](<I - nested-dapps.md>).

For current dotli main, pairing metadata is embedded directly in the SSO V2 proposal. dotli should pass
`host_name = "Polkadot Web"`, `host_icon = "https://dot.li/dotli.png"`,
`host_version = __DOTLI_VERSION__`, and browser-derived `platform_type` / `platform_version` when available.

## A3. Current host-container parity surfaces **(S-M)**

These are platform traits or host callbacks needed only because the current dotli bridge already exposes
them to products through `@novasamatech/host-container`.

- **Notifications:** `push_notification` must return the host-api notification id and `cancel` must be
  exposed. Current `truapi-platform::Notifications` only returns `Result<(), GenericError>`, so it needs
  the wire-visible id/cancel shape before dotli can drop `handlePushNotification` /
  `handlePushNotificationCancel`.
- **Theme:** expose a subscription that emits the current `{ name, variant }` and future changes. Current
  dotli maps browser light/dark state into the host-api `Default` theme.
- **Preimage:** keep preimage host-side for v1, but expose submit and lookup subscription through Rust so
  the JS container handler can be removed.
- **Resource allocation confirmation:** current dotli shows an allowance modal before sending the SSO
  `resourceAllocationRequest` over SSO. The core owns the encrypted SSO session request, but calls a
  dedicated host UI confirmation hook first and maps dismissal to the same typed error path. Do not overload
  `Permissions::remote_permission`: allocation confirmation wraps an SSO round-trip and retry-capable
  modal, while remote permission is just a permission decision.
- **SessionStore:** persist one optional opaque core-encoded blob for the full `SessionInfo` in
  host-global secret storage. Do not reuse product-scoped `Storage`: the SSO session is shared across
  product runtimes and contains `ss_secret` plus ECDH/session-channel material. The host does not see
  typed session fields, and v1 does not preserve host-papp's multi-session list shape. Rust does not add
  blob-level encryption/MAC; confidentiality and tamper resistance are the host storage layer's
  responsibility. It must also support current-then-changes change notifications for both same-runtime
  writes/clears and cross-tab/process changes so logout or re-pair updates all live runtimes without a
  read/subscribe race. Notifications are coarse: the core receives a tick and calls `read()` to fetch and
  validate the current blob.

**Proposed trait**:

```rust
pub trait SessionStore: Send + Sync {
    fn read(&self) -> impl Future<Output = Result<Option<Vec<u8>>, GenericError>> + Send;
    fn write(&self, value: Vec<u8>) -> impl Future<Output = Result<(), GenericError>> + Send;
    fn clear(&self) -> impl Future<Output = Result<(), GenericError>> + Send;
    /// Emits once immediately, then once per future local or cross-runtime change.
    /// The core calls `read()` after each tick.
    fn subscribe(&self) -> BoxStream<'static, Result<(), GenericError>>;
}
```

Signing, transaction construction, alias, pairing, and statement-store operations are not in this list
because they are core protocol over the statement store.

## Not host primitives (core protocol over the statement store)

Earlier drafts modeled a pairing transport, a wallet-signing proxy, and a ring-VRF proxy as host
primitives. The real protocol ([H](<H - sso-pairing-protocol.md>)) makes them **core logic**, not
platform traits:

- **Transport** is the People-chain statement store, reached via the **existing `ChainProvider`** trait
  (`connect(genesis_hash) -> JsonRpcConnection`, then `statement_submit` /
  `statement_subscribeStatement`). No new transport primitive.
- **`sign_payload` / `sign_raw` / legacy variants / `create_transaction`** are `signingRequest` messages
  the core sends over the encrypted session channel; the wallet signs and replies ([B](<B - core-impls.md>)).
- **`request_resource_allocation`** is likewise an SSO session-channel request in current dotli, with
  host-side confirmation before send and secret stripping around the result.
- **`get_account_alias`** is an `aliasRequest` message over the same channel. **`create_account_proof`**
  needs a full ring-VRF proof and is a separate Tier-4 decision ([E3](<E - open-questions.md>)); current
  dotli code only calls the alias path.
- **Statement-store `create_proof` / `subscribe` / `submit`** are core-native: the core already speaks
  the statement store for pairing, and signs proofs with its own session key.

## Identity: no new primitive

`get_user_id` / `get_legacy_accounts` answer from `SessionState` (the wallet account ids learned at
pairing, [C](<C - session-contract.md>)) plus product runtime config. Current dotli's
`get_legacy_accounts` returns the product-derived `(calling_product_id, 0)` account, not the root account.
`get_user_id` is additionally gated by the product-scoped `UserId` permission. Usernames, if surfaced,
come from an on-chain People-chain identity lookup over the same `ChainProvider` connection and are
**not** part of the pairing handshake.
