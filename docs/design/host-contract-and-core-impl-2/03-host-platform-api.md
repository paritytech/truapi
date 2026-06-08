# 03 - Host Platform API

> Parent: [dotli shared Rust core migration](<index.md>).

The host API should expose only capabilities the Rust core cannot perform
inside WASM/UniFFI. Product protocol behavior belongs in `truapi-server`.

## Keep From PR 104

PR 104's `truapi-platform` surface remains the base:

```rust
pub trait Platform:
    Navigation
    + Notifications
    + Permissions
    + Features
    + Storage
    + ChainProvider
{
}
```

The migration should keep these as host capabilities:

- `Storage`: product-scoped key/value storage.
- `Navigation`: open a URL after core policy/normalization.
- `Permissions`: render permission prompts and return user decisions.
- `Features`: report host-supported chain features.
- `ChainProvider`: open JSON-RPC connections for chain access.

The current `Notifications` shape needs to change because dotli parity requires
scheduled notifications, a returned id, and cancellation.

## Add Runtime Configuration

Runtime configuration is constructor state, not a callback:

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

Every product runtime needs this before handling account, signing, entropy,
statement, or permission methods.

## Add SSO Presentation

The core owns pairing, but the host owns UI.

```rust
pub trait PairingPresenter: Send + Sync {
    fn present_pairing(
        &self,
        deeplink: String,
    ) -> impl Future<Output = PairingPresentationResult> + Send;
}

pub enum PairingPresentationResult {
    CancelledByUser,
}
```

The core closes the presentation by dropping/cancelling the in-flight pairing
task after success, timeout, or abort. The host must not parse or verify the QR
payload.

## Add Session Store

The SSO session is host-global, not product-local.

```rust
pub trait SessionStore: Send + Sync {
    fn read(&self) -> impl Future<Output = Result<Option<Vec<u8>>, GenericError>> + Send;
    fn write(&self, value: Vec<u8>) -> impl Future<Output = Result<(), GenericError>> + Send;
    fn clear(&self) -> impl Future<Output = Result<(), GenericError>> + Send;
    fn subscribe(&self) -> BoxStream<'static, Result<(), GenericError>>;
}
```

Requirements:

- store one opaque core-encoded session blob;
- emit once immediately, then on local and cross-runtime changes;
- rely on host storage security, not a Rust-added encryption layer;
- do not reuse product `Storage`;
- do not require host-papp `SsoSessions` compatibility.

## Extend Notifications

dotli parity needs ids and cancellation:

```rust
pub trait Notifications: Send + Sync {
    fn schedule(
        &self,
        request: NotificationRequest,
    ) -> impl Future<Output = Result<NotificationId, NotificationError>> + Send;

    fn cancel(
        &self,
        id: NotificationId,
    ) -> impl Future<Output = Result<(), GenericError>> + Send;
}
```

`scheduled_at = None` or a past time fires immediately and still returns an id.
The web adapter can keep its IndexedDB scheduler and cross-tab wakeups.

## Add UI Confirmation Hooks

Signing and allocation are core protocol operations, but dotli requires user
confirmation before the core sends them to the SSO peer.

```rust
pub trait UserConfirmation: Send + Sync {
    fn confirm_sign_payload(&self, request: SignPayloadReview)
        -> impl Future<Output = Confirmation> + Send;

    fn confirm_sign_raw(&self, request: SignRawReview)
        -> impl Future<Output = Confirmation> + Send;

    fn confirm_create_transaction(&self, request: CreateTransactionReview)
        -> impl Future<Output = Confirmation> + Send;

    fn confirm_resource_allocation(&self, request: AllocationReview)
        -> impl Future<Output = AllocationConfirmation> + Send;
}
```

These are not wallet transports. They only collect local user consent and
display review information.

## Add Theme and Preimage Backends

```rust
pub trait ThemeHost: Send + Sync {
    fn subscribe_theme(&self) -> BoxStream<'static, Result<ThemeState, GenericError>>;
}

pub trait PreimageHost: Send + Sync {
    fn confirm_submit(&self, size: u64)
        -> impl Future<Output = Result<(), PreimageSubmitError>> + Send;

    fn submit(&self, value: Vec<u8>)
        -> impl Future<Output = Result<PreimageKey, PreimageSubmitError>> + Send;

    fn lookup(&self, key: PreimageKey)
        -> BoxStream<'static, Result<Option<Vec<u8>>, GenericError>>;
}
```

Theme subscriptions must emit the current value immediately. Preimage lookup
must emit the current cache/miss immediately, then updates until unsubscribe.

## Host Adapter Rule

Any API that reaches wallet keys, derives product accounts, constructs SSO
messages, signs statement proofs, or interprets TrUAPI wire semantics belongs
in Rust core, not in the host API.
