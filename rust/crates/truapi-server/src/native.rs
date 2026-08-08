//! UniFFI-facing native bridge. Exposes [`NativeTrUApiCore`] and the
//! [`HostCallbacks`] callback interface that iOS and Android call into.
//!
//! The native side builds a `CallbackPlatform` that adapts every
//! [`truapi_platform::Platform`] trait to a corresponding callback. The
//! resulting platform is fed into [`SigningHostRuntime`] so the rest of the
//! dispatcher pipeline behaves identically to the WS-bridge and wasm flavors.
//! A native host owns the signer and can also serve responder sessions for
//! paired product hosts.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures::FutureExt;
use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::future::{AbortHandle, Abortable, BoxFuture};
use futures::stream::{self, BoxStream, StreamExt};
use futures::task::SpawnExt;
use parity_scale_codec::Encode;
use truapi::v01;
use truapi_platform::{
    AuthPresenter, AuthState, ChainProvider, CoreStorage, CoreStorageKey, Features, HostInfo,
    JsonRpcConnection, Navigation, Notifications, PermissionAuthorizationRequest,
    PermissionAuthorizationStatus, Permissions, PlatformInfo, PreimageHost, ProductContext,
    ProductStorage, RuntimeConfigValidationError, SigningHostConfig, ThemeHost, UserConfirmation,
    UserConfirmationReview, async_trait,
};

pub use crate::host_logic::dotns::NavigateDecision;
use crate::host_logic::{dotns, session::SsoSessionInfo};
use crate::runtime::ResponderPeer;
use crate::subscription::Spawner;
#[cfg(feature = "ws-bridge")]
use crate::ws_bridge::{BridgeLogger, WsBridge, WsBridgeEndpoint, WsBridgeStartError};
use crate::{ResponderExit, SigningHostRuntime};

/// Host-thrown storage failure wrapping the canonical error payload, so its
/// variants remain defined once in `truapi`.
///
/// [UniFFI 0.32 exposes `Result` failures as error enums or `Arc`-backed error
/// objects](https://mozilla.github.io/uniffi-rs/0.32/types/errors.html). Although
/// the canonical enum can be exposed as an external error, Kotlin foreign-trait
/// callbacks must lower thrown errors into this namespace's `RustBuffer`; the
/// external converter returns the canonical namespace's distinct `RustBuffer`
/// type. There is no derive-based bridge between them. `uniffi::remote(Error)`
/// would instead duplicate every canonical variant and field, so this local
/// one-variant wrapper preserves the canonical definition.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum HostStorageError {
    /// Canonical storage failure payload.
    #[error("{0}")]
    Storage(v01::HostLocalStorageReadError),
}

impl From<HostStorageError> for v01::HostLocalStorageReadError {
    fn from(err: HostStorageError) -> Self {
        let HostStorageError::Storage(err) = err;
        err
    }
}

/// Native-friendly rejection error returned by callback methods that map onto
/// [`truapi::v01::GenericError`].
///
/// [`uniffi::Error` is the value-style error mapping and only supports enums;
/// UniFFI's struct alternative is an `Arc`-backed object
/// error](https://mozilla.github.io/uniffi-rs/0.32/types/errors.html). Making the
/// canonical SCALE value an object would require Rust-owned handles and foreign
/// construction solely to carry one string. This local enum keeps the native
/// exception value-like without changing the canonical wire representation.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum HostRejection {
    /// Caller rejected the operation.
    #[error("{reason}")]
    Rejected {
        /// Human-readable rejection reason.
        reason: String,
    },
}

impl From<HostRejection> for v01::GenericError {
    fn from(err: HostRejection) -> Self {
        let HostRejection::Rejected { reason } = err;
        v01::GenericError { reason }
    }
}

impl From<v01::GenericError> for HostRejection {
    fn from(err: v01::GenericError) -> Self {
        HostRejection::Rejected { reason: err.reason }
    }
}

/// Host-thrown navigation failure wrapping the canonical error payload.
///
/// As described for [`HostStorageError`], [UniFFI's supported error
/// representations](https://mozilla.github.io/uniffi-rs/0.32/types/errors.html)
/// do not provide a derive-based way to bridge namespace-specific Kotlin
/// `RustBuffer` types when an external error is thrown by a foreign-trait
/// callback. The one-variant wrapper avoids mirroring the canonical navigation
/// error enum locally.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum HostNavigateRejection {
    /// Canonical navigation failure payload.
    #[error("{0}")]
    Navigate(v01::HostNavigateToError),
}

/// Native-friendly SSO deeplink scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativePairingDeeplinkScheme {
    /// Production Polkadot app.
    PolkadotApp,
    /// Development Polkadot app.
    PolkadotAppDev,
}

/// Pairing-host identity persisted by a native signing host so its responder
/// subscription can be restored after an app restart.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NativePairingPeer {
    /// Pairing host's 32-byte sr25519 Statement Store account id.
    pub statement_account_id: Vec<u8>,
    /// Pairing host's 32-byte raw X25519 public key.
    pub encryption_public_key: Vec<u8>,
}

impl From<ResponderPeer> for NativePairingPeer {
    fn from(peer: ResponderPeer) -> Self {
        Self {
            statement_account_id: peer.statement_account_id.to_vec(),
            encryption_public_key: peer.encryption_public_key.to_vec(),
        }
    }
}

/// Invalid persisted peer data or an SSO responder failure.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum NativePairingError {
    /// Statement Store account id was not exactly 32 bytes.
    #[error("statement_account_id must be exactly 32 bytes, got {actual}")]
    InvalidStatementAccountId {
        /// Supplied byte length.
        actual: u64,
    },
    /// X25519 public key was not exactly 32 bytes.
    #[error("encryption_public_key must be exactly 32 bytes, got {actual}")]
    InvalidEncryptionPublicKey {
        /// Supplied byte length.
        actual: u64,
    },
    /// Pairing or responder startup failed.
    #[error("{reason}")]
    Failed {
        /// Human-readable failure reason.
        reason: String,
    },
}

impl TryFrom<NativePairingPeer> for ResponderPeer {
    type Error = NativePairingError;

    fn try_from(peer: NativePairingPeer) -> Result<Self, Self::Error> {
        let statement_account_id =
            peer.statement_account_id
                .try_into()
                .map_err(
                    |value: Vec<u8>| NativePairingError::InvalidStatementAccountId {
                        actual: value.len() as u64,
                    },
                )?;
        let encryption_public_key =
            peer.encryption_public_key
                .try_into()
                .map_err(
                    |value: Vec<u8>| NativePairingError::InvalidEncryptionPublicKey {
                        actual: value.len() as u64,
                    },
                )?;
        Ok(Self {
            statement_account_id,
            encryption_public_key,
        })
    }
}

/// Native runtime configuration supplied before product calls are handled.
#[derive(Debug, Clone, uniffi::Record)]
pub struct NativeRuntimeConfig {
    /// Canonical product identifier used for account derivation.
    pub product_id: String,
    /// Host name shown by the wallet during SSO pairing.
    pub host_name: String,
    /// Optional host icon URL shown by the wallet during SSO pairing.
    pub host_icon: Option<String>,
    /// Optional host version shown by the wallet during SSO pairing.
    pub host_version: Option<String>,
    /// Optional platform/browser name shown by the wallet during SSO pairing.
    pub platform_type: Option<String>,
    /// Optional platform/browser version shown by the wallet during SSO pairing.
    pub platform_version: Option<String>,
    /// People-chain genesis hash. Must be exactly 32 bytes.
    pub people_chain_genesis_hash: Vec<u8>,
    /// Bulletin-chain genesis hash. Must be exactly 32 bytes.
    pub bulletin_chain_genesis_hash: Vec<u8>,
    /// Optional local signing-host secret material (raw BIP-39 entropy).
    pub local_session_secret: Option<Vec<u8>>,
    /// Optional lite username attached to the local signing-host session.
    pub local_session_lite_username: Option<String>,
    /// Deeplink scheme used in pairing QR payloads.
    pub pairing_deeplink_scheme: NativePairingDeeplinkScheme,
}

#[derive(Debug)]
struct NativeResolvedRuntimeConfig {
    signing: SigningHostConfig,
    product: ProductContext,
    local_session_secret: Option<Vec<u8>>,
    local_session_lite_username: Option<String>,
}

/// Native runtime config validation error.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum NativeRuntimeConfigError {
    /// Required string field was empty or whitespace-only.
    #[error("{field} must not be empty")]
    EmptyField {
        /// Field name.
        field: String,
    },
    /// People-chain genesis hash was not exactly 32 bytes.
    #[error("people_chain_genesis_hash must be exactly 32 bytes, got {actual}")]
    InvalidPeopleChainGenesisHash {
        /// Supplied byte length.
        actual: u64,
    },
    /// Bulletin-chain genesis hash was not exactly 32 bytes.
    #[error("bulletin_chain_genesis_hash must be exactly 32 bytes, got {actual}")]
    InvalidBulletinChainGenesisHash {
        /// Supplied byte length.
        actual: u64,
    },
    /// Host icon URL could not be parsed.
    #[error("host_icon must be an absolute HTTPS URL: {reason}")]
    InvalidHostIcon {
        /// Parse failure reason.
        reason: String,
    },
    /// Host icon URL used a non-HTTPS scheme.
    #[error("host_icon must use https scheme, got {scheme:?}")]
    InsecureHostIcon {
        /// Actual URL scheme.
        scheme: String,
    },
    /// Pairing deeplink scheme included a URL separator.
    #[error("pairing_deeplink_scheme must not include ://, got {scheme:?}")]
    InvalidDeeplinkScheme {
        /// Actual deeplink scheme value.
        scheme: String,
    },
    /// Product id was not a valid host-spec product identifier.
    #[error("invalid product_id: {product_id}")]
    InvalidProductId {
        /// Actual product id value.
        product_id: String,
    },
    /// Local signing-host session activation failed.
    #[error("failed to activate local signing session: {reason}")]
    LocalSessionActivation {
        /// Activation failure reason.
        reason: String,
    },
}

impl TryFrom<NativeRuntimeConfig> for NativeResolvedRuntimeConfig {
    type Error = NativeRuntimeConfigError;

    fn try_from(config: NativeRuntimeConfig) -> Result<Self, Self::Error> {
        let people_chain_genesis_hash =
            <[u8; 32]>::try_from(config.people_chain_genesis_hash.as_slice()).map_err(|_| {
                NativeRuntimeConfigError::InvalidPeopleChainGenesisHash {
                    actual: config.people_chain_genesis_hash.len() as u64,
                }
            })?;
        let bulletin_chain_genesis_hash =
            <[u8; 32]>::try_from(config.bulletin_chain_genesis_hash.as_slice()).map_err(|_| {
                NativeRuntimeConfigError::InvalidBulletinChainGenesisHash {
                    actual: config.bulletin_chain_genesis_hash.len() as u64,
                }
            })?;
        let product =
            ProductContext::new(config.product_id).map_err(NativeRuntimeConfigError::from)?;
        let signing = SigningHostConfig::new(
            HostInfo {
                name: config.host_name,
                icon: config.host_icon,
                version: config.host_version,
            },
            PlatformInfo {
                kind: config.platform_type,
                version: config.platform_version,
            },
            people_chain_genesis_hash,
            bulletin_chain_genesis_hash,
        )?;
        Ok(Self {
            signing,
            product,
            local_session_secret: config.local_session_secret,
            local_session_lite_username: config.local_session_lite_username,
        })
    }
}

impl From<RuntimeConfigValidationError> for NativeRuntimeConfigError {
    fn from(err: RuntimeConfigValidationError) -> Self {
        match err {
            RuntimeConfigValidationError::EmptyField { field } => Self::EmptyField {
                field: field.to_string(),
            },
            // `url::ParseError` cannot cross the UniFFI boundary, so the native
            // error keeps a rendered string.
            RuntimeConfigValidationError::InvalidHostIcon { source } => Self::InvalidHostIcon {
                reason: source.to_string(),
            },
            RuntimeConfigValidationError::InsecureHostIcon { scheme } => {
                Self::InsecureHostIcon { scheme }
            }
            RuntimeConfigValidationError::InvalidDeeplinkScheme { scheme } => {
                Self::InvalidDeeplinkScheme { scheme }
            }
            RuntimeConfigValidationError::InvalidProductId { product_id } => {
                Self::InvalidProductId { product_id }
            }
        }
    }
}

impl From<HostNavigateRejection> for v01::HostNavigateToError {
    fn from(err: HostNavigateRejection) -> Self {
        let HostNavigateRejection::Navigate(err) = err;
        err
    }
}

/// Classify a navigation input exactly like the core's internal navigate host
/// call: `.dot` first, then `localhost`, then normalized external, with
/// everything else rejected. Pure and stateless; hosts call it on every
/// webview-internal navigation.
#[uniffi::export]
pub fn parse_navigate(input: String) -> NavigateDecision {
    dotns::parse_navigate(&input)
}

/// Callback surface that iOS and Android implement.
///
/// Threading contract: every callback executes on the shared bridge
/// executor's worker threads, and blocking one of those threads can stall
/// the entire bridge — not just the request being served. Async callbacks
/// (`navigate_to`, `push_notification`, `device_permission`,
/// `remote_permission`, `feature_supported`, `confirm_user_action`,
/// `lookup_preimage`) are awaited by the core — implementations hop to the
/// main thread for any UI and may keep the future pending arbitrarily long,
/// but must suspend rather than block the polling thread (foreign
/// implementations bridged through UniFFI suspend naturally; the rule
/// chiefly binds Rust implementations). Dropping the returned future
/// cancels the foreign task. The remaining sync callbacks run inline on the
/// dispatcher thread and must return promptly without blocking; in
/// particular `auth_state_changed` should only hand the state to the host
/// UI thread, never wait for the user.
#[uniffi::export(rust, foreign)]
#[async_trait::async_trait]
pub trait HostCallbacks: Send + Sync {
    /// Lifecycle logger. Marker is a stable slug, detail is free-form.
    fn on_core_log(&self, marker: String, detail: String);

    /// Open a URL in the system browser.
    async fn navigate_to(&self, url: String) -> Result<(), HostNavigateRejection>;

    /// Deliver a push notification.
    async fn push_notification(
        &self,
        request: v01::HostPushNotificationRequest,
    ) -> Result<u32, HostRejection>;

    /// Cancel a notification by id.
    fn cancel_notification(&self, id: u32) -> Result<(), HostRejection>;

    /// Prompt the user for a device-level permission (camera, mic, ...);
    /// the host returns whether the permission was granted.
    async fn device_permission(
        &self,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<bool, HostRejection>;

    /// Prompt the user for a remote (product-scoped) permission.
    async fn remote_permission(
        &self,
        request: v01::RemotePermission,
    ) -> Result<bool, HostRejection>;

    /// Observe an auth state change. Emitted only when the state actually
    /// changes, in transition order: render `Pairing` as the pairing QR UI,
    /// `Connected`/`Disconnected` as the account badge, `LoginFailed` as a
    /// retryable error. User cancellation is reported through
    /// `NativeTrUApiCore.cancel_login()`.
    fn auth_state_changed(&self, state: AuthState);

    /// A paired host explicitly ended its SSO session. Native shells should
    /// remove the matching persisted host/device and update their UI. Ordinary
    /// transport interruptions are retried by the core and do not emit this.
    fn pairing_peer_disconnected(&self, peer: NativePairingPeer);

    /// Read a core-owned host-private storage slot. `key` is a SCALE-encoded
    /// [`CoreStorageKey`].
    fn core_storage_read(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection>;

    /// Persist a core-owned host-private storage slot. `key` is a
    /// SCALE-encoded [`CoreStorageKey`].
    fn core_storage_write(&self, key: Vec<u8>, value: Vec<u8>) -> Result<(), HostRejection>;

    /// Clear a core-owned host-private storage slot. `key` is a SCALE-encoded
    /// [`CoreStorageKey`].
    fn core_storage_clear(&self, key: Vec<u8>) -> Result<(), HostRejection>;

    /// Open a JSON-RPC connection for a chain. Return a host-assigned
    /// connection id, or `None` when unsupported.
    fn chain_connect(&self, genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection>;

    /// Send one JSON-RPC request over a previously opened chain connection.
    fn chain_send(&self, connection_id: u32, request: String) -> Result<(), HostRejection>;

    /// Close a previously opened chain connection.
    fn chain_close(&self, connection_id: u32) -> Result<(), HostRejection>;

    /// Confirm one user-reviewed core action.
    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, HostRejection>;

    /// Look up one preimage value by key. The native shim emits this as the
    /// current item in its subscription stream.
    async fn lookup_preimage(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection>;

    /// Current host theme. The native shim emits this as the current item in
    /// its subscription stream.
    fn current_theme(&self) -> Result<v01::ThemeVariant, HostRejection>;

    /// Answer a feature-support query.
    async fn feature_supported(
        &self,
        request: v01::HostFeatureSupportedRequest,
    ) -> Result<bool, HostRejection>;

    /// Read a value from the host's scoped key-value store.
    fn local_storage_read(&self, key: String) -> Result<Option<Vec<u8>>, HostStorageError>;
    /// Write a value to the host's scoped key-value store.
    fn local_storage_write(&self, key: String, value: Vec<u8>) -> Result<(), HostStorageError>;
    /// Clear a value from the host's scoped key-value store.
    fn local_storage_clear(&self, key: String) -> Result<(), HostStorageError>;
}

/// UniFFI object exposing the TrUAPI core to native hosts.
#[derive(uniffi::Object)]
pub struct NativeTrUApiCore {
    runtime: Arc<SigningHostRuntime>,
    product: ProductContext,
    events: Arc<NativeEventBus>,
    callbacks: Arc<dyn HostCallbacks>,
    spawner: Spawner,
    pairing_tasks: Arc<Mutex<HashMap<[u8; 32], NativePairingTask>>>,
    next_pairing_generation: AtomicU64,
    #[cfg(feature = "ws-bridge")]
    bridge: std::sync::Mutex<Option<WsBridge>>,
}

struct NativePairingTask {
    generation: u64,
    abort: AbortHandle,
    session: SsoSessionInfo,
}

#[uniffi::export]
impl NativeTrUApiCore {
    /// Construct the core with explicit product and pairing runtime config.
    ///
    /// When `runtime_config` carries `local_session_secret`, the session is
    /// activated before this returns, so construction blocks the calling thread
    /// on the same key derivation as [`Self::activate_local_session`]. Prefer
    /// constructing off the host's main/UI thread.
    #[uniffi::constructor]
    pub fn with_runtime_config(
        callbacks: Arc<dyn HostCallbacks>,
        runtime_config: NativeRuntimeConfig,
    ) -> Result<Arc<Self>, NativeRuntimeConfigError> {
        native_core_from_platform_config(callbacks, runtime_config.try_into()?)
    }

    /// Core-owned logout/disconnect. Best-effort notifies the SSO peer when
    /// the session has channel material, then clears in-memory and persisted
    /// session state.
    ///
    /// Blocks the calling thread until the disconnect completes, so call it off
    /// the host's main/UI thread.
    pub fn disconnect(&self) {
        futures::executor::block_on(self.runtime.disconnect_session());
    }

    /// Notify this core that host-global session storage changed outside a
    /// direct core write/clear.
    ///
    /// **Inert on a native host.** A signing host owns the active session in
    /// memory, so there is no session-store sync loop to wake. Retained so
    /// hosts written against the pairing-host surface still link.
    pub fn notify_session_store_changed(&self) {
        // Signing hosts own the active local session in memory. There is no
        // pairing-host session-store sync loop to notify.
    }

    /// Cancel an in-flight pairing login.
    ///
    /// **Inert on a native host.** The native bridge runs a signing host, which
    /// has no pairing flow to cancel: `request_login` resolves against the
    /// locally activated session instead. Calling this emits no auth state and
    /// changes nothing. Retained so hosts written against the pairing-host
    /// surface still link.
    pub fn cancel_login(&self) {
        // Signing hosts do not perform SSO pairing when products call
        // request_login; a locally activated session returns AlreadyConnected.
    }

    /// Read a stored permission authorization status without prompting.
    ///
    /// Blocks the calling thread on the storage read, so call it off the host's
    /// main/UI thread.
    pub fn permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
    ) -> Result<PermissionAuthorizationStatus, HostRejection> {
        let admin = self.runtime.product_admin(self.product.clone());
        let status = futures::executor::block_on(admin.permission_authorization_status(request))?;
        Ok(status)
    }

    /// Update a stored permission authorization status. Passing
    /// `.notDetermined` clears the stored value so the next product request
    /// prompts again.
    ///
    /// Blocks the calling thread on the storage write, so call it off the host's
    /// main/UI thread.
    pub fn set_permission_authorization_status(
        &self,
        request: PermissionAuthorizationRequest,
        status: PermissionAuthorizationStatus,
    ) -> Result<(), HostRejection> {
        let admin = self.runtime.product_admin(self.product.clone());
        futures::executor::block_on(admin.set_permission_authorization_status(request, status))?;
        Ok(())
    }

    /// Activate or replace the local signing-host session from host-held
    /// secret material (raw BIP-39 entropy).
    ///
    /// Blocks the calling thread while the session is derived (PBKDF2, 2048
    /// rounds), so call it off the host's main/UI thread.
    pub fn activate_local_session(
        &self,
        secret: Vec<u8>,
        lite_username: Option<String>,
    ) -> Result<(), HostRejection> {
        futures::executor::block_on(
            self.runtime
                .activate_local_session_with_identity(secret, lite_username),
        )
        .map_err(Into::into)
    }

    /// Answer a pairing deeplink and start serving the resulting SSO session
    /// in the core's background pool. Returns after the handshake statement is
    /// accepted, not when the long-lived session eventually ends.
    ///
    /// Blocks the calling thread on the handshake submission, so call it off
    /// the host's main/UI thread.
    pub fn respond_to_pairing(
        &self,
        deeplink: String,
    ) -> Result<NativePairingPeer, NativePairingError> {
        let (peer, session) = futures::executor::block_on(self.runtime.answer_pairing(&deeplink))
            .map_err(|err| NativePairingError::Failed { reason: err.reason })?;
        self.start_pairing_task(peer.clone(), session);
        Ok(peer.into())
    }

    /// Restore the background responder for a previously persisted pairing.
    /// Repeated calls replace the old subscription for the same peer.
    pub fn resume_pairing(&self, peer: NativePairingPeer) -> Result<(), NativePairingError> {
        let peer = ResponderPeer::try_from(peer)?;
        let session = self
            .runtime
            .responder_session_for_peer(&peer)
            .map_err(|err| NativePairingError::Failed { reason: err.reason })?;
        self.start_pairing_task(peer, session);
        Ok(())
    }

    /// Notify one paired host of a local disconnect and stop its responder.
    ///
    /// Blocks on the best-effort Statement Store submission, so call it off
    /// the host's main/UI thread.
    pub fn disconnect_pairing(&self, peer: NativePairingPeer) -> Result<(), NativePairingError> {
        let peer = ResponderPeer::try_from(peer)?;
        let session = self
            .stop_pairing_task(&peer)
            .map(|task| task.session)
            .map(Ok)
            .unwrap_or_else(|| self.runtime.responder_session_for_peer(&peer))
            .map_err(|err| NativePairingError::Failed { reason: err.reason })?;
        match futures::executor::block_on(self.runtime.disconnect_responder_session(&session)) {
            Ok(()) => Ok(()),
            Err(err) => {
                self.start_pairing_task(peer, session);
                Err(NativePairingError::Failed { reason: err.reason })
            }
        }
    }

    /// Stop one responder subscription without notifying the peer. Used when
    /// the native app suspends and will restore sessions later.
    pub fn suspend_pairing(&self, peer: NativePairingPeer) -> Result<(), NativePairingError> {
        let peer = ResponderPeer::try_from(peer)?;
        self.stop_pairing_task(&peer);
        Ok(())
    }

    /// Stop all responder subscriptions without disconnecting their peers.
    pub fn suspend_all_pairings(&self) {
        let tasks = std::mem::take(
            &mut *self
                .pairing_tasks
                .lock()
                .expect("native pairing tasks mutex poisoned"),
        );
        for (_, task) in tasks {
            task.abort.abort();
        }
    }

    /// Push a host theme update to active TrUAPI theme subscriptions.
    pub fn notify_theme_changed(&self, theme: v01::ThemeVariant) {
        self.events.notify_theme_changed(theme);
    }

    /// Push a preimage lookup update to active subscriptions for `key`.
    ///
    /// `value == None` represents a known miss; `Some(bytes)` represents the
    /// current preimage value.
    pub fn notify_preimage_changed(&self, key: Vec<u8>, value: Option<Vec<u8>>) {
        self.events.notify_preimage_changed(&key, value);
    }

    /// Push a JSON-RPC response from a native chain connection into the core.
    pub fn notify_chain_response(&self, connection_id: u32, json: String) {
        self.events.notify_chain_response(connection_id, json);
    }

    /// Notify the core that a native chain connection closed externally.
    pub fn notify_chain_closed(&self, connection_id: u32) {
        self.events.notify_chain_closed(connection_id);
    }
}

impl NativeTrUApiCore {
    fn start_pairing_task(&self, peer: ResponderPeer, session: SsoSessionInfo) {
        let generation = self.next_pairing_generation.fetch_add(1, Ordering::Relaxed);
        let (abort, registration) = AbortHandle::new_pair();
        let task = NativePairingTask {
            generation,
            abort,
            session: session.clone(),
        };
        if let Some(previous) = self
            .pairing_tasks
            .lock()
            .expect("native pairing tasks mutex poisoned")
            .insert(peer.statement_account_id, task)
        {
            previous.abort.abort();
        }

        let runtime = self.runtime.clone();
        let callbacks = self.callbacks.clone();
        let tasks = self.pairing_tasks.clone();
        let peer_for_callback: NativePairingPeer = peer.clone().into();
        let peer_key = peer.statement_account_id;
        let future = async move {
            loop {
                match runtime.serve_responder_session(session.clone()).await {
                    Ok(ResponderExit::PeerDisconnected) => {
                        let is_current = tasks
                            .lock()
                            .expect("native pairing tasks mutex poisoned")
                            .get(&peer_key)
                            .is_some_and(|task| task.generation == generation);
                        if !is_current {
                            break;
                        }
                        callbacks.pairing_peer_disconnected(peer_for_callback.clone());
                        break;
                    }
                    Ok(ResponderExit::SubscriptionEnded) => callbacks.on_core_log(
                        "truapi.native.sso.subscription_ended".to_string(),
                        format!(
                            "peer={}; retrying",
                            hex::encode(peer_for_callback.statement_account_id.as_slice())
                        ),
                    ),
                    Err(err) => callbacks.on_core_log(
                        "truapi.native.sso.subscription_failed".to_string(),
                        format!(
                            "peer={}; {}; retrying",
                            hex::encode(peer_for_callback.statement_account_id.as_slice()),
                            err.reason
                        ),
                    ),
                }
                futures_timer::Delay::new(std::time::Duration::from_secs(1)).await;
            }

            let mut active = tasks.lock().expect("native pairing tasks mutex poisoned");
            if active
                .get(&peer_key)
                .is_some_and(|task| task.generation == generation)
            {
                active.remove(&peer_key);
            }
        };
        (self.spawner)(Box::pin(Abortable::new(future, registration).map(|_| ())));
    }

    fn stop_pairing_task(&self, peer: &ResponderPeer) -> Option<NativePairingTask> {
        let task = self
            .pairing_tasks
            .lock()
            .expect("native pairing tasks mutex poisoned")
            .remove(&peer.statement_account_id);
        if let Some(task) = &task {
            task.abort.abort();
        }
        task
    }
}

impl Drop for NativeTrUApiCore {
    fn drop(&mut self) {
        let tasks = std::mem::take(
            &mut *self
                .pairing_tasks
                .lock()
                .expect("native pairing tasks mutex poisoned"),
        );
        for (_, task) in tasks {
            task.abort.abort();
        }
    }
}

/// Set the live log level (`off`/`error`/`warn`/`info`/`debug`/`trace`) for
/// the `tracing` output, which on native routes to stderr (system logs on
/// iOS/Android). Most native diagnostics flow through `on_core_log` instead;
/// this controls the cross-platform `tracing` events shared with wasm.
#[uniffi::export]
pub fn set_log_level(level: String) {
    crate::logging::set_level_from_str(&level);
}

fn native_core_from_platform_config(
    callbacks: Arc<dyn HostCallbacks>,
    runtime_config: NativeResolvedRuntimeConfig,
) -> Result<Arc<NativeTrUApiCore>, NativeRuntimeConfigError> {
    crate::logging::init();
    callbacks.on_core_log(
        "truapi.native.core.boot".to_string(),
        "core ready".to_string(),
    );

    let events = Arc::new(NativeEventBus::default());
    let platform = Arc::new(CallbackPlatform {
        callbacks: callbacks.clone(),
        events: events.clone(),
    });
    let spawner = native_thread_pool_spawner(&callbacks);
    let runtime = Arc::new(SigningHostRuntime::new(
        platform,
        runtime_config.signing,
        spawner.clone(),
    ));

    if let Some(secret) = runtime_config.local_session_secret {
        futures::executor::block_on(runtime.activate_local_session_with_identity(
            secret,
            runtime_config.local_session_lite_username,
        ))
        .map_err(|err| NativeRuntimeConfigError::LocalSessionActivation { reason: err.reason })?;
    }

    Ok(Arc::new(NativeTrUApiCore {
        runtime,
        product: runtime_config.product,
        events,
        callbacks,
        spawner,
        pairing_tasks: Arc::new(Mutex::new(HashMap::new())),
        next_pairing_generation: AtomicU64::new(1),
        #[cfg(feature = "ws-bridge")]
        bridge: std::sync::Mutex::new(None),
    }))
}

#[cfg(feature = "ws-bridge")]
#[uniffi::export]
impl NativeTrUApiCore {
    /// Start the localhost WebSocket bridge. Returns the descriptor the
    /// host hands to the product so it can dial back in.
    pub fn start_ws_bridge(&self, bind_port: u16) -> Result<WsBridgeEndpoint, WsBridgeStartError> {
        let mut guard = self.bridge.lock().unwrap();
        if guard.is_some() {
            return Err(WsBridgeStartError::AlreadyRunning);
        }
        let logger: BridgeLogger = {
            let callbacks = self.callbacks.clone();
            Arc::new(move |marker: &str, detail: &str| {
                callbacks.on_core_log(marker.to_string(), detail.to_string());
            })
        };
        let runtime = self.runtime.clone();
        let product = self.product.clone();
        let runtime_factory = Arc::new(move |sink| runtime.product_runtime(product.clone(), sink));
        let (bridge, endpoint) = WsBridge::start(bind_port, runtime_factory, logger)?;
        *guard = Some(bridge);
        Ok(endpoint)
    }

    /// Stop the localhost WebSocket bridge (if running).
    pub fn stop_ws_bridge(&self) {
        if let Some(mut bridge) = self.bridge.lock().unwrap().take() {
            bridge.stop();
        }
    }
}

/// Build a [`Spawner`] backed by a shared `futures::executor::ThreadPool`.
/// The pool is sized at the default (one worker per logical CPU). Falls
/// back to a thread-per-subscription spawner if the pool fails to build,
/// which only ever happens if the host has no available threads at all.
fn native_thread_pool_spawner(callbacks: &Arc<dyn HostCallbacks>) -> Spawner {
    match ThreadPool::new() {
        Ok(pool) => {
            let callbacks = callbacks.clone();
            Arc::new(move |fut: BoxFuture<'static, ()>| {
                if let Err(err) = pool.spawn(fut) {
                    callbacks.on_core_log(
                        "truapi.native.core.subscription.spawn_failed".to_string(),
                        format!("{err}"),
                    );
                }
            })
        }
        Err(err) => {
            callbacks.on_core_log(
                "truapi.native.core.subscription.pool_unavailable".to_string(),
                format!("{err}; falling back to thread-per-subscription"),
            );
            crate::subscription::thread_per_subscription_spawner()
        }
    }
}

struct CallbackPlatform {
    callbacks: Arc<dyn HostCallbacks>,
    events: Arc<NativeEventBus>,
}

#[derive(Default)]
struct NativeEventBus {
    theme_changes: Mutex<Vec<mpsc::UnboundedSender<Result<v01::ThemeVariant, v01::GenericError>>>>,
    preimage_changes: Mutex<Vec<PreimageSubscription>>,
    chain_responses: Mutex<HashMap<u32, mpsc::UnboundedSender<String>>>,
}

struct PreimageSubscription {
    key: Vec<u8>,
    tx: mpsc::UnboundedSender<Result<Option<Vec<u8>>, v01::GenericError>>,
}

impl NativeEventBus {
    fn subscribe_theme(
        &self,
        current: Result<v01::ThemeVariant, v01::GenericError>,
    ) -> BoxStream<'static, Result<v01::ThemeVariant, v01::GenericError>> {
        let (tx, rx) = mpsc::unbounded();
        self.theme_changes
            .lock()
            .expect("native theme subscribers mutex poisoned")
            .push(tx);
        stream::once(async move { current }).chain(rx).boxed()
    }

    fn notify_theme_changed(&self, theme: v01::ThemeVariant) {
        self.theme_changes
            .lock()
            .expect("native theme subscribers mutex poisoned")
            .retain(|tx| tx.unbounded_send(Ok(theme)).is_ok());
    }

    fn subscribe_preimage_changes(
        &self,
        key: Vec<u8>,
    ) -> mpsc::UnboundedReceiver<Result<Option<Vec<u8>>, v01::GenericError>> {
        let (tx, rx) = mpsc::unbounded();
        self.preimage_changes
            .lock()
            .expect("native preimage subscribers mutex poisoned")
            .push(PreimageSubscription { key, tx });
        rx
    }

    fn notify_preimage_changed(&self, key: &[u8], value: Option<Vec<u8>>) {
        self.preimage_changes
            .lock()
            .expect("native preimage subscribers mutex poisoned")
            .retain(|sub| {
                if sub.key != key {
                    return true;
                }
                sub.tx.unbounded_send(Ok(value.clone())).is_ok()
            });
    }

    fn register_chain(&self, connection_id: u32) -> mpsc::UnboundedReceiver<String> {
        let (tx, rx) = mpsc::unbounded();
        self.chain_responses
            .lock()
            .expect("native chain subscribers mutex poisoned")
            .insert(connection_id, tx);
        rx
    }

    fn notify_chain_response(&self, connection_id: u32, json: String) {
        let mut responses = self
            .chain_responses
            .lock()
            .expect("native chain subscribers mutex poisoned");
        let Some(tx) = responses.get(&connection_id) else {
            return;
        };
        if tx.unbounded_send(json).is_err() {
            responses.remove(&connection_id);
        }
    }

    fn notify_chain_closed(&self, connection_id: u32) {
        self.chain_responses
            .lock()
            .expect("native chain subscribers mutex poisoned")
            .remove(&connection_id);
    }
}

#[async_trait]
impl Navigation for CallbackPlatform {
    async fn navigate_to(&self, url: String) -> Result<(), v01::HostNavigateToError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.navigate_to".to_string(),
            url.clone(),
        );
        self.callbacks.navigate_to(url).await.map_err(Into::into)
    }
}

#[async_trait]
impl Notifications for CallbackPlatform {
    async fn push_notification(
        &self,
        notification: v01::HostPushNotificationRequest,
    ) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.push_notification".to_string(),
            notification.text.clone(),
        );

        let id = self
            .callbacks
            .push_notification(notification)
            .await
            .map_err(v01::GenericError::from)?;
        Ok(v01::HostPushNotificationResponse { id })
    }

    async fn cancel_notification(&self, id: u32) -> Result<(), v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.cancel_notification".to_string(),
            id.to_string(),
        );
        self.callbacks
            .cancel_notification(id)
            .map_err(v01::GenericError::from)
    }
}

#[async_trait]
impl Permissions for CallbackPlatform {
    async fn device_permission(
        &self,
        request: v01::HostDevicePermissionRequest,
    ) -> Result<v01::HostDevicePermissionResponse, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.device_permission".to_string(),
            format!("{request}"),
        );

        let granted = self
            .callbacks
            .device_permission(request)
            .await
            .map_err(v01::GenericError::from)?;
        Ok(v01::HostDevicePermissionResponse { granted })
    }

    async fn remote_permission(
        &self,
        request: v01::RemotePermissionRequest,
    ) -> Result<v01::RemotePermissionResponse, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.remote_permission".to_string(),
            format!("{request}"),
        );

        let granted = self
            .callbacks
            .remote_permission(request.permission)
            .await
            .map_err(v01::GenericError::from)?;
        Ok(v01::RemotePermissionResponse { granted })
    }
}

#[async_trait]
impl Features for CallbackPlatform {
    async fn feature_supported(
        &self,
        request: v01::HostFeatureSupportedRequest,
    ) -> Result<v01::HostFeatureSupportedResponse, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.feature_supported".to_string(),
            format!("{request:?}"),
        );

        let supported = self
            .callbacks
            .feature_supported(request)
            .await
            .map_err(v01::GenericError::from)?;
        Ok(v01::HostFeatureSupportedResponse { supported })
    }
}

#[async_trait]
impl ProductStorage for CallbackPlatform {
    async fn read(&self, key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
        self.callbacks.local_storage_read(key).map_err(Into::into)
    }

    async fn write(
        &self,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), v01::HostLocalStorageReadError> {
        self.callbacks
            .local_storage_write(key, value)
            .map_err(Into::into)
    }

    async fn clear(&self, key: String) -> Result<(), v01::HostLocalStorageReadError> {
        self.callbacks.local_storage_clear(key).map_err(Into::into)
    }
}

#[async_trait]
impl CoreStorage for CallbackPlatform {
    async fn read_core_storage(
        &self,
        key: CoreStorageKey,
    ) -> Result<Option<Vec<u8>>, v01::GenericError> {
        self.callbacks
            .core_storage_read(key.encode())
            .map_err(v01::GenericError::from)
    }

    async fn write_core_storage(
        &self,
        key: CoreStorageKey,
        value: Vec<u8>,
    ) -> Result<(), v01::GenericError> {
        self.callbacks
            .core_storage_write(key.encode(), value)
            .map_err(v01::GenericError::from)
    }

    async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), v01::GenericError> {
        self.callbacks
            .core_storage_clear(key.encode())
            .map_err(v01::GenericError::from)
    }
}

struct NativeJsonRpcConnection {
    id: u32,
    callbacks: Arc<dyn HostCallbacks>,
    events: Arc<NativeEventBus>,
    response_rx: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
    closed: AtomicBool,
}

impl JsonRpcConnection for NativeJsonRpcConnection {
    fn send(&self, request: String) {
        if self.closed.load(Ordering::Relaxed) {
            return;
        }
        if let Err(err) = self.callbacks.chain_send(self.id, request) {
            self.callbacks.on_core_log(
                "truapi.native.callback.chain_send_failed".to_string(),
                err.to_string(),
            );
        }
    }

    fn responses(&self) -> BoxStream<'static, String> {
        let mut guard = self.response_rx.lock().unwrap();
        match guard.take() {
            Some(rx) => rx.boxed(),
            None => {
                self.callbacks.on_core_log(
                    "truapi.native.chain.responses_reused".to_string(),
                    "responses() called more than once".to_string(),
                );
                stream::empty().boxed()
            }
        }
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::Relaxed) {
            return;
        }
        self.events.notify_chain_closed(self.id);
        if let Err(err) = self.callbacks.chain_close(self.id) {
            self.callbacks.on_core_log(
                "truapi.native.callback.chain_close_failed".to_string(),
                err.to_string(),
            );
        }
    }
}

impl Drop for NativeJsonRpcConnection {
    fn drop(&mut self) {
        self.close();
    }
}

#[async_trait]
impl ChainProvider for CallbackPlatform {
    async fn connect(
        &self,
        genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        let Some(connection_id) = self
            .callbacks
            .chain_connect(genesis_hash.to_vec())
            .map_err(v01::GenericError::from)?
        else {
            return Err(v01::GenericError {
                reason: "chain provider unavailable".to_string(),
            });
        };
        let response_rx = self.events.register_chain(connection_id);
        Ok(Box::new(NativeJsonRpcConnection {
            id: connection_id,
            callbacks: self.callbacks.clone(),
            events: self.events.clone(),
            response_rx: Mutex::new(Some(response_rx)),
            closed: AtomicBool::new(false),
        }))
    }
}

impl AuthPresenter for CallbackPlatform {
    fn auth_state_changed(&self, state: truapi_platform::AuthState) {
        self.callbacks.on_core_log(
            "truapi.native.callback.auth_state_changed".to_string(),
            String::new(),
        );
        self.callbacks.auth_state_changed(state);
    }
}

#[async_trait]
impl UserConfirmation for CallbackPlatform {
    async fn confirm_user_action(
        &self,
        review: UserConfirmationReview,
    ) -> Result<bool, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.confirm_user_action".to_string(),
            String::new(),
        );
        self.callbacks
            .confirm_user_action(review)
            .await
            .map_err(v01::GenericError::from)
    }
}

impl ThemeHost for CallbackPlatform {
    fn subscribe_theme(&self) -> BoxStream<'static, Result<v01::ThemeVariant, v01::GenericError>> {
        let current = self
            .callbacks
            .current_theme()
            .map_err(v01::GenericError::from);
        self.events.subscribe_theme(current)
    }
}

impl PreimageHost for CallbackPlatform {
    fn lookup_preimage(
        &self,
        key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        // Register the change receiver first so no event between the lookup
        // and the subscription is lost, then await the current value lazily.
        let rx = self.events.subscribe_preimage_changes(key.clone());
        let callbacks = self.callbacks.clone();
        let current = async move {
            callbacks
                .lookup_preimage(key)
                .await
                .map_err(v01::GenericError::from)
        };
        stream::once(current).chain(rx).boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use truapi::Bytes32;
    use truapi::v01::LegacyAccountTxPayload;
    use truapi_platform::CreateTransactionReview;

    type PreimageFixtureEntries = Vec<(Vec<u8>, Option<Vec<u8>>)>;

    struct EventCallbacks {
        theme: Mutex<v01::ThemeVariant>,
        preimages: Mutex<PreimageFixtureEntries>,
        auth_states: Mutex<Vec<AuthState>>,
        chain_id: Mutex<Option<u32>>,
        chain_connects: Mutex<Vec<Vec<u8>>>,
        chain_sends: Mutex<Vec<(u32, String)>>,
        chain_closes: Mutex<Vec<u32>>,
    }

    impl EventCallbacks {
        fn new() -> Self {
            Self {
                theme: Mutex::new(v01::ThemeVariant::Light),
                preimages: Mutex::new(Vec::new()),
                auth_states: Mutex::new(Vec::new()),
                chain_id: Mutex::new(None),
                chain_connects: Mutex::new(Vec::new()),
                chain_sends: Mutex::new(Vec::new()),
                chain_closes: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl HostCallbacks for EventCallbacks {
        fn on_core_log(&self, _marker: String, _detail: String) {}
        async fn navigate_to(&self, _url: String) -> Result<(), HostNavigateRejection> {
            Ok(())
        }
        async fn push_notification(
            &self,
            _request: v01::HostPushNotificationRequest,
        ) -> Result<u32, HostRejection> {
            Ok(0)
        }
        fn cancel_notification(&self, _id: u32) -> Result<(), HostRejection> {
            Ok(())
        }
        async fn device_permission(
            &self,
            _request: v01::HostDevicePermissionRequest,
        ) -> Result<bool, HostRejection> {
            Ok(false)
        }
        async fn remote_permission(
            &self,
            _request: v01::RemotePermission,
        ) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn auth_state_changed(&self, state: AuthState) {
            self.auth_states
                .lock()
                .expect("auth state mutex poisoned")
                .push(state);
        }
        fn pairing_peer_disconnected(&self, _peer: NativePairingPeer) {}
        fn core_storage_read(&self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
            Ok(None)
        }
        fn core_storage_write(&self, _key: Vec<u8>, _value: Vec<u8>) -> Result<(), HostRejection> {
            Ok(())
        }
        fn core_storage_clear(&self, _key: Vec<u8>) -> Result<(), HostRejection> {
            Ok(())
        }
        fn chain_connect(&self, genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection> {
            self.chain_connects
                .lock()
                .expect("chain connects mutex poisoned")
                .push(genesis_hash);
            Ok(*self.chain_id.lock().expect("chain id mutex poisoned"))
        }
        fn chain_send(&self, connection_id: u32, request: String) -> Result<(), HostRejection> {
            self.chain_sends
                .lock()
                .expect("chain sends mutex poisoned")
                .push((connection_id, request));
            Ok(())
        }
        fn chain_close(&self, connection_id: u32) -> Result<(), HostRejection> {
            self.chain_closes
                .lock()
                .expect("chain closes mutex poisoned")
                .push(connection_id);
            Ok(())
        }
        async fn confirm_user_action(
            &self,
            _review: UserConfirmationReview,
        ) -> Result<bool, HostRejection> {
            Ok(false)
        }
        async fn lookup_preimage(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
            Ok(self
                .preimages
                .lock()
                .expect("preimage map mutex poisoned")
                .iter()
                .find(|(stored_key, _)| stored_key == &key)
                .and_then(|(_, value)| value.clone()))
        }
        fn current_theme(&self) -> Result<v01::ThemeVariant, HostRejection> {
            Ok(*self.theme.lock().expect("theme mutex poisoned"))
        }
        async fn feature_supported(
            &self,
            _request: v01::HostFeatureSupportedRequest,
        ) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn local_storage_read(&self, _key: String) -> Result<Option<Vec<u8>>, HostStorageError> {
            Ok(None)
        }
        fn local_storage_write(
            &self,
            _key: String,
            _value: Vec<u8>,
        ) -> Result<(), HostStorageError> {
            Ok(())
        }
        fn local_storage_clear(&self, _key: String) -> Result<(), HostStorageError> {
            Ok(())
        }
    }

    fn event_platform() -> (Arc<EventCallbacks>, Arc<NativeEventBus>, CallbackPlatform) {
        let callbacks = Arc::new(EventCallbacks::new());
        let events = Arc::new(NativeEventBus::default());
        let platform = CallbackPlatform {
            callbacks: callbacks.clone(),
            events: events.clone(),
        };
        (callbacks, events, platform)
    }

    fn native_runtime_config(product_id: &str) -> NativeRuntimeConfig {
        NativeRuntimeConfig {
            product_id: product_id.to_string(),
            host_name: "Polkadot Web".to_string(),
            host_icon: Some("https://example.invalid/dotli.png".to_string()),
            host_version: None,
            platform_type: None,
            platform_version: None,
            people_chain_genesis_hash: vec![0xa2; 32],
            bulletin_chain_genesis_hash: vec![0xbb; 32],
            local_session_secret: None,
            local_session_lite_username: None,
            pairing_deeplink_scheme: NativePairingDeeplinkScheme::PolkadotApp,
        }
    }

    #[test]
    fn permission_authorization_request_mirror_round_trips() {
        let device_cases = [
            v01::HostDevicePermissionRequest::Notifications,
            v01::HostDevicePermissionRequest::Camera,
            v01::HostDevicePermissionRequest::Microphone,
            v01::HostDevicePermissionRequest::Bluetooth,
            v01::HostDevicePermissionRequest::NFC,
            v01::HostDevicePermissionRequest::Location,
            v01::HostDevicePermissionRequest::Clipboard,
            v01::HostDevicePermissionRequest::OpenUrl,
            v01::HostDevicePermissionRequest::Biometrics,
        ];
        let remote_cases = [
            v01::RemotePermission::Remote {
                domains: vec!["a.dot".to_string(), "b.dot".to_string()],
            },
            v01::RemotePermission::WebRtc,
            v01::RemotePermission::ChainSubmit,
            v01::RemotePermission::PreimageSubmit,
            v01::RemotePermission::StatementSubmit,
        ];

        let mut cases: Vec<PermissionAuthorizationRequest> = Vec::new();
        cases.extend(
            device_cases
                .into_iter()
                .map(PermissionAuthorizationRequest::Device),
        );
        cases.extend(remote_cases.into_iter().map(|permission| {
            PermissionAuthorizationRequest::Remote(v01::RemotePermissionRequest { permission })
        }));
        cases.push(PermissionAuthorizationRequest::IdentityDisclosure);
        cases.push(PermissionAuthorizationRequest::AccountAccess {
            target_product_id: "other.dot".to_string(),
        });

        for case in cases {
            let native = case.clone();
            assert_eq!(native, case);
        }
    }

    #[test]
    fn native_auth_presenter_forwards_states_across_the_ffi_mirror() {
        let (callbacks, _events, platform) = event_platform();

        platform.auth_state_changed(truapi_platform::AuthState::Pairing {
            deeplink: "polkadotapp://pair?handshake=00".to_string(),
        });
        platform.auth_state_changed(truapi_platform::AuthState::Connected(
            truapi_platform::SessionUiInfo {
                public_key: [7; 32],
                identity_account_id: None,
                lite_username: Some("alice".to_string()),
                full_username: None,
            },
        ));
        platform.auth_state_changed(truapi_platform::AuthState::Disconnected);

        assert_eq!(
            callbacks
                .auth_states
                .lock()
                .expect("auth state mutex poisoned")
                .as_slice(),
            &[
                AuthState::Pairing {
                    deeplink: "polkadotapp://pair?handshake=00".to_string(),
                },
                AuthState::Connected(truapi_platform::SessionUiInfo {
                    public_key: [7; 32],
                    identity_account_id: None,
                    lite_username: Some("alice".to_string()),
                    full_username: None,
                }),
                AuthState::Disconnected,
            ]
        );
    }

    #[test]
    fn native_theme_subscription_emits_current_then_notified_changes() {
        let (callbacks, events, platform) = event_platform();
        let mut stream = platform.subscribe_theme();

        let first = futures::executor::block_on(stream.next()).unwrap();
        *callbacks.theme.lock().expect("theme mutex poisoned") = v01::ThemeVariant::Dark;
        events.notify_theme_changed(v01::ThemeVariant::Dark);
        let second = futures::executor::block_on(stream.next()).unwrap();

        assert_eq!(first.unwrap(), v01::ThemeVariant::Light);
        assert_eq!(second.unwrap(), v01::ThemeVariant::Dark);
    }

    #[test]
    fn native_preimage_subscription_emits_current_then_notified_value() {
        let (callbacks, events, platform) = event_platform();
        let key = vec![7; 32];
        callbacks
            .preimages
            .lock()
            .expect("preimage map mutex poisoned")
            .push((key.clone(), Some(vec![1, 2, 3])));
        let mut stream = platform.lookup_preimage(key.clone());

        let first = futures::executor::block_on(stream.next()).unwrap();
        events.notify_preimage_changed(&key, Some(vec![4, 5, 6]));
        let second = futures::executor::block_on(stream.next()).unwrap();

        assert_eq!(first.unwrap(), Some(vec![1, 2, 3]));
        assert_eq!(second.unwrap(), Some(vec![4, 5, 6]));
    }

    #[test]
    fn native_chain_provider_forwards_send_response_and_close() {
        let (callbacks, events, platform) = event_platform();
        *callbacks.chain_id.lock().expect("chain id mutex poisoned") = Some(42);
        let genesis = [9; 32];

        let connection = futures::executor::block_on(ChainProvider::connect(&platform, genesis))
            .expect("chain connection should open");
        connection.send(r#"{"jsonrpc":"2.0","id":1}"#.to_string());
        let mut responses = connection.responses();
        events.notify_chain_response(42, r#"{"jsonrpc":"2.0","id":1,"result":true}"#.to_string());
        let response = futures::executor::block_on(responses.next()).unwrap();
        drop(responses);
        drop(connection);

        assert_eq!(
            callbacks
                .chain_connects
                .lock()
                .expect("chain connects mutex poisoned")
                .as_slice(),
            &[genesis.to_vec()]
        );
        assert_eq!(
            callbacks
                .chain_sends
                .lock()
                .expect("chain sends mutex poisoned")
                .as_slice(),
            &[(42, r#"{"jsonrpc":"2.0","id":1}"#.to_string())]
        );
        assert_eq!(response, r#"{"jsonrpc":"2.0","id":1,"result":true}"#);
        assert_eq!(
            callbacks
                .chain_closes
                .lock()
                .expect("chain closes mutex poisoned")
                .as_slice(),
            &[42]
        );
    }

    #[test]
    fn runtime_config_rejects_wrong_size_genesis_hash() {
        let err = NativeResolvedRuntimeConfig::try_from(NativeRuntimeConfig {
            people_chain_genesis_hash: vec![0; 31],
            ..native_runtime_config("app.dot")
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::InvalidPeopleChainGenesisHash { actual: 31 }
        ));
    }

    #[test]
    fn runtime_config_rejects_empty_required_fields() {
        let err = NativeResolvedRuntimeConfig::try_from(NativeRuntimeConfig {
            product_id: " ".to_string(),
            ..native_runtime_config("app.dot")
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::EmptyField { field } if field == "product_id"
        ));
    }

    #[test]
    fn runtime_config_rejects_relative_host_icon() {
        let err = NativeResolvedRuntimeConfig::try_from(NativeRuntimeConfig {
            host_icon: Some("/dotli.png".to_string()),
            ..native_runtime_config("app.dot")
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::InvalidHostIcon { .. }
        ));
    }

    #[test]
    fn runtime_config_rejects_non_https_host_icon() {
        let err = NativeResolvedRuntimeConfig::try_from(NativeRuntimeConfig {
            host_icon: Some("http://localhost:3000/dotli.png".to_string()),
            ..native_runtime_config("app.dot")
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::InsecureHostIcon { scheme } if scheme == "http"
        ));
    }

    #[test]
    fn native_pairing_peer_validates_persisted_key_lengths() {
        let err = ResponderPeer::try_from(NativePairingPeer {
            statement_account_id: vec![0; 31],
            encryption_public_key: vec![0; 32],
        })
        .unwrap_err();
        assert!(matches!(
            err,
            NativePairingError::InvalidStatementAccountId { actual: 31 }
        ));

        let err = ResponderPeer::try_from(NativePairingPeer {
            statement_account_id: vec![0; 32],
            encryption_public_key: vec![0; 31],
        })
        .unwrap_err();
        assert!(matches!(
            err,
            NativePairingError::InvalidEncryptionPublicKey { actual: 31 }
        ));
    }

    /// Calling `start_ws_bridge` twice on the same `NativeTrUApiCore`
    /// without an intervening `stop_ws_bridge` is a hard error. The bridge
    /// is single-instance per core, so the second start must surface
    /// `AlreadyRunning` rather than silently leaking a worker thread.
    #[cfg(feature = "ws-bridge")]
    #[test]
    fn start_ws_bridge_twice_returns_already_running() {
        struct Noop;
        #[async_trait::async_trait]
        impl HostCallbacks for Noop {
            fn on_core_log(&self, _marker: String, _detail: String) {}
            async fn navigate_to(&self, _url: String) -> Result<(), HostNavigateRejection> {
                Ok(())
            }
            async fn push_notification(
                &self,
                _request: v01::HostPushNotificationRequest,
            ) -> Result<u32, HostRejection> {
                Ok(0)
            }
            fn cancel_notification(&self, _id: u32) -> Result<(), HostRejection> {
                Ok(())
            }
            async fn device_permission(
                &self,
                _request: v01::HostDevicePermissionRequest,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            async fn remote_permission(
                &self,
                _request: v01::RemotePermission,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn auth_state_changed(&self, _state: AuthState) {}
            fn pairing_peer_disconnected(&self, _peer: NativePairingPeer) {}
            fn core_storage_read(&self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn core_storage_write(
                &self,
                _key: Vec<u8>,
                _value: Vec<u8>,
            ) -> Result<(), HostRejection> {
                Ok(())
            }
            fn core_storage_clear(&self, _key: Vec<u8>) -> Result<(), HostRejection> {
                Ok(())
            }
            fn chain_connect(&self, _genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection> {
                Ok(None)
            }
            fn chain_send(
                &self,
                _connection_id: u32,
                _request: String,
            ) -> Result<(), HostRejection> {
                Ok(())
            }
            fn chain_close(&self, _connection_id: u32) -> Result<(), HostRejection> {
                Ok(())
            }
            async fn confirm_user_action(
                &self,
                _review: UserConfirmationReview,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            async fn lookup_preimage(
                &self,
                _key: Vec<u8>,
            ) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn current_theme(&self) -> Result<v01::ThemeVariant, HostRejection> {
                Ok(v01::ThemeVariant::Light)
            }
            async fn feature_supported(
                &self,
                _request: v01::HostFeatureSupportedRequest,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn local_storage_read(
                &self,
                _key: String,
            ) -> Result<Option<Vec<u8>>, HostStorageError> {
                Ok(None)
            }
            fn local_storage_write(
                &self,
                _key: String,
                _value: Vec<u8>,
            ) -> Result<(), HostStorageError> {
                Ok(())
            }
            fn local_storage_clear(&self, _key: String) -> Result<(), HostStorageError> {
                Ok(())
            }
        }

        let core = NativeTrUApiCore::with_runtime_config(
            Arc::new(Noop),
            NativeRuntimeConfig {
                host_icon: Some("https://dot.li/dotli.png".to_string()),
                ..native_runtime_config("dotli.dot")
            },
        )
        .expect("runtime config should be valid");
        let _first = core.start_ws_bridge(0).expect("first start must succeed");
        let err = core
            .start_ws_bridge(0)
            .expect_err("second start must error");
        assert!(matches!(err, WsBridgeStartError::AlreadyRunning));
        core.stop_ws_bridge();
    }

    /// A permission callback suspends while awaiting the user's decision and
    /// holds no executor worker, so an unrelated request on the same
    /// connection still round-trips while the decision is pending.
    #[cfg(feature = "ws-bridge")]
    #[test]
    fn pending_permission_decision_does_not_stall_bridge() {
        use std::sync::atomic::{AtomicBool, Ordering};

        use futures::SinkExt;
        use parity_scale_codec::Decode;
        use tokio_tungstenite::tungstenite::Message as WsMessage;
        use truapi::versioned::permissions::HostDevicePermissionRequest;
        use truapi::versioned::system::HostFeatureSupportedRequest;

        use crate::frame::{Payload, ProtocolMessage, request_ids};

        /// `device_permission` stays pending until the test sends on
        /// `release`; every other callback is a trivial success.
        struct GatedPermissionCallbacks {
            permission_entered: Arc<AtomicBool>,
            release: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<()>>,
        }

        #[async_trait::async_trait]
        impl HostCallbacks for GatedPermissionCallbacks {
            fn on_core_log(&self, _marker: String, _detail: String) {}
            async fn navigate_to(&self, _url: String) -> Result<(), HostNavigateRejection> {
                Ok(())
            }
            async fn push_notification(
                &self,
                _request: v01::HostPushNotificationRequest,
            ) -> Result<u32, HostRejection> {
                Ok(0)
            }
            fn cancel_notification(&self, _id: u32) -> Result<(), HostRejection> {
                Ok(())
            }
            async fn device_permission(
                &self,
                _request: v01::HostDevicePermissionRequest,
            ) -> Result<bool, HostRejection> {
                self.permission_entered.store(true, Ordering::SeqCst);
                self.release
                    .lock()
                    .await
                    .recv()
                    .await
                    .expect("release signal");
                Ok(true)
            }
            async fn remote_permission(
                &self,
                _request: v01::RemotePermission,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn auth_state_changed(&self, _state: AuthState) {}
            fn pairing_peer_disconnected(&self, _peer: NativePairingPeer) {}
            fn core_storage_read(&self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn core_storage_write(
                &self,
                _key: Vec<u8>,
                _value: Vec<u8>,
            ) -> Result<(), HostRejection> {
                Ok(())
            }
            fn core_storage_clear(&self, _key: Vec<u8>) -> Result<(), HostRejection> {
                Ok(())
            }
            fn chain_connect(&self, _genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection> {
                Ok(None)
            }
            fn chain_send(
                &self,
                _connection_id: u32,
                _request: String,
            ) -> Result<(), HostRejection> {
                Ok(())
            }
            fn chain_close(&self, _connection_id: u32) -> Result<(), HostRejection> {
                Ok(())
            }
            async fn confirm_user_action(
                &self,
                _review: UserConfirmationReview,
            ) -> Result<bool, HostRejection> {
                Ok(false)
            }
            async fn lookup_preimage(
                &self,
                _key: Vec<u8>,
            ) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn current_theme(&self) -> Result<v01::ThemeVariant, HostRejection> {
                Ok(v01::ThemeVariant::Light)
            }
            async fn feature_supported(
                &self,
                _request: v01::HostFeatureSupportedRequest,
            ) -> Result<bool, HostRejection> {
                Ok(true)
            }
            fn local_storage_read(
                &self,
                _key: String,
            ) -> Result<Option<Vec<u8>>, HostStorageError> {
                Ok(None)
            }
            fn local_storage_write(
                &self,
                _key: String,
                _value: Vec<u8>,
            ) -> Result<(), HostStorageError> {
                Ok(())
            }
            fn local_storage_clear(&self, _key: String) -> Result<(), HostStorageError> {
                Ok(())
            }
        }

        let (release_tx, release_rx) = tokio::sync::mpsc::channel::<()>(1);
        let permission_entered = Arc::new(AtomicBool::new(false));
        let core = NativeTrUApiCore::with_runtime_config(
            Arc::new(GatedPermissionCallbacks {
                permission_entered: permission_entered.clone(),
                release: tokio::sync::Mutex::new(release_rx),
            }),
            NativeRuntimeConfig {
                host_icon: Some("https://dot.li/dotli.png".to_string()),
                ..native_runtime_config("dotli.dot")
            },
        )
        .expect("runtime config should be valid");
        let endpoint = core.start_ws_bridge(0).expect("start bridge");
        let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");

        let permission_ids =
            request_ids("permissions_request_device_permission").expect("known request method");
        let feature_ids = request_ids("system_feature_supported").expect("known request method");
        let (feature_response, permission_response) = rt.block_on(async {
            let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.expect("dial");

            let permission_frame = ProtocolMessage {
                request_id: "p:permission".into(),
                payload: Payload {
                    id: permission_ids.request_id,
                    value: HostDevicePermissionRequest::V1(
                        v01::HostDevicePermissionRequest::Camera,
                    )
                    .encode(),
                },
            };
            ws.send(WsMessage::Binary(permission_frame.encode()))
                .await
                .expect("send device permission");

            // Wait until the permission callback is blocked on the decision.
            for _ in 0..1000 {
                if permission_entered.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            assert!(
                permission_entered.load(Ordering::SeqCst),
                "permission callback was not invoked"
            );

            let feature_frame = ProtocolMessage {
                request_id: "p:feature".into(),
                payload: Payload {
                    id: feature_ids.request_id,
                    value: HostFeatureSupportedRequest::V1(
                        v01::HostFeatureSupportedRequest::Chain {
                            genesis_hash: vec![0u8; 32],
                        },
                    )
                    .encode(),
                },
            };
            ws.send(WsMessage::Binary(feature_frame.encode()))
                .await
                .expect("send feature_supported");

            let feature_response =
                tokio::time::timeout(std::time::Duration::from_secs(10), async {
                    loop {
                        match ws.next().await {
                            Some(Ok(WsMessage::Binary(bytes))) => {
                                break ProtocolMessage::decode(&mut &bytes[..])
                                    .expect("decode response");
                            }
                            Some(Ok(_)) => continue,
                            Some(Err(err)) => panic!("ws error: {err}"),
                            None => panic!("connection closed before response"),
                        }
                    }
                })
                .await
                .expect("feature_supported must answer while the permission decision is pending");

            release_tx
                .send(())
                .await
                .expect("release permission callback");
            let permission_response =
                tokio::time::timeout(std::time::Duration::from_secs(10), async {
                    loop {
                        match ws.next().await {
                            Some(Ok(WsMessage::Binary(bytes))) => {
                                break ProtocolMessage::decode(&mut &bytes[..])
                                    .expect("decode response");
                            }
                            Some(Ok(_)) => continue,
                            Some(Err(err)) => panic!("ws error: {err}"),
                            None => panic!("connection closed before response"),
                        }
                    }
                })
                .await
                .expect("released permission must answer");

            (feature_response, permission_response)
        });

        assert_eq!(feature_response.request_id, "p:feature");
        assert_eq!(feature_response.payload.id, feature_ids.response_id);

        assert_eq!(permission_response.request_id, "p:permission");
        assert_eq!(permission_response.payload.id, permission_ids.response_id);
        // [Ok 0x00][V1 0x00][granted=1]
        assert_eq!(permission_response.payload.value, vec![0x00, 0x00, 0x01]);

        core.stop_ws_bridge();
    }

    #[test]
    fn bytes32_widens_to_plain_bytes_on_the_wire() {
        let mut buf = Vec::new();
        <Bytes32 as uniffi::Lower<truapi::UniFfiTag>>::write([7; 32], &mut buf);
        assert_eq!(buf[..4], 32i32.to_be_bytes());
        assert_eq!(buf[4..], [7; 32]);
    }

    #[test]
    fn bytes32_lift_rejects_wrong_length() {
        let mut buf = Vec::new();
        <Vec<u8> as uniffi::Lower<crate::UniFfiTag>>::write(vec![7; 31], &mut buf);
        assert!(
            <Bytes32 as uniffi::Lift<truapi::UniFfiTag>>::try_read(&mut buf.as_slice()).is_err()
        );
    }

    #[test]
    fn bytes32_fields_survive_the_ffi_roundtrip() {
        let review = UserConfirmationReview::CreateTransaction(
            CreateTransactionReview::LegacyAccount(LegacyAccountTxPayload {
                signer: [13; 32],
                genesis_hash: [14; 32],
                call_data: vec![15],
                extensions: vec![],
                tx_ext_version: 0,
            }),
        );

        let mut buf = Vec::new();
        <UserConfirmationReview as uniffi::Lower<crate::UniFfiTag>>::write(
            review.clone(),
            &mut buf,
        );
        let lifted = <UserConfirmationReview as uniffi::Lift<crate::UniFfiTag>>::try_read(
            &mut buf.as_slice(),
        )
        .expect("review must lift back");
        assert_eq!(lifted, review);
    }
}
