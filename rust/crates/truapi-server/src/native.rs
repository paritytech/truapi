//! UniFFI-facing native bridge. Exposes [`NativeTrUApiCore`] and the
//! [`HostCallbacks`] callback interface that iOS and Android call into.
//!
//! The native side builds a [`CallbackPlatform`] that adapts every
//! [`truapi_platform::Platform`] trait to a corresponding callback. The
//! resulting platform is fed into [`TrUApiCore::from_platform`] so the rest
//! of the dispatcher pipeline behaves identically to the WS-bridge and wasm
//! flavors.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures::channel::{mpsc, oneshot};
use futures::executor::ThreadPool;
use futures::future::BoxFuture;
use futures::stream::{self, BoxStream, StreamExt};
use futures::task::SpawnExt;
use parity_scale_codec::Encode;
use truapi::v01;
use truapi::versioned::system::{HostFeatureSupportedRequest, HostFeatureSupportedResponse};
use truapi_platform::PairingDeeplinkScheme as PlatformPairingDeeplinkScheme;
use truapi_platform::{
    ChainProvider, Features, JsonRpcConnection, Navigation, Notifications, PairingPresenter,
    Permissions, PreimageHost, RuntimeConfig, RuntimeConfigValidationError, SessionStore, Storage,
    ThemeHost, UserConfirmation,
};

use crate::TrUApiCore;
use crate::subscription::Spawner;
#[cfg(feature = "ws-bridge")]
use crate::ws_bridge::{BridgeLogger, WsBridge, WsBridgeEndpoint, WsBridgeStartError};

/// Native-friendly storage error. Mirrors the v0.1 wire shape so the
/// callback surface stays SCALE-free.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum HostStorageError {
    /// Quota exhausted.
    #[error("storage quota exhausted")]
    Full,
    /// Catch-all.
    #[error("{reason}")]
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

impl From<HostStorageError> for v01::HostLocalStorageReadError {
    fn from(err: HostStorageError) -> Self {
        match err {
            HostStorageError::Full => v01::HostLocalStorageReadError::Full,
            HostStorageError::Unknown { reason } => {
                v01::HostLocalStorageReadError::Unknown { reason }
            }
        }
    }
}

/// Native-friendly rejection error returned by callback methods that map
/// onto [`truapi::v01::GenericError`].
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

/// Native-friendly navigation error.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum HostNavigateRejection {
    /// User declined the navigation.
    #[error("navigation denied by user")]
    PermissionDenied,
    /// Catch-all.
    #[error("{reason}")]
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}

/// Native-friendly theme enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum HostTheme {
    /// Light host theme.
    Light,
    /// Dark host theme.
    Dark,
}

impl From<HostTheme> for v01::Theme {
    fn from(theme: HostTheme) -> Self {
        match theme {
            HostTheme::Light => v01::Theme::Light,
            HostTheme::Dark => v01::Theme::Dark,
        }
    }
}

/// Native-friendly SSO deeplink scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NativePairingDeeplinkScheme {
    /// Production Polkadot app.
    PolkadotApp,
    /// Development Polkadot app.
    PolkadotAppDev,
}

impl From<NativePairingDeeplinkScheme> for PlatformPairingDeeplinkScheme {
    fn from(scheme: NativePairingDeeplinkScheme) -> Self {
        match scheme {
            NativePairingDeeplinkScheme::PolkadotApp => PlatformPairingDeeplinkScheme::PolkadotApp,
            NativePairingDeeplinkScheme::PolkadotAppDev => {
                PlatformPairingDeeplinkScheme::PolkadotAppDev
            }
        }
    }
}

/// Native runtime configuration supplied before product calls are handled.
#[derive(Debug, Clone, uniffi::Record)]
pub struct NativeRuntimeConfig {
    /// Human-readable dotli label, e.g. `my-app`.
    pub product_label: String,
    /// Canonical product identifier used for account derivation.
    pub product_id: String,
    /// Host deployment/site identifier.
    pub site_id: String,
    /// HTTPS metadata URL the SSO peer can fetch for display.
    pub host_metadata_url: String,
    /// People-chain genesis hash. Must be exactly 32 bytes.
    pub people_chain_genesis_hash: Vec<u8>,
    /// Deeplink scheme used in pairing QR payloads.
    pub pairing_deeplink_scheme: NativePairingDeeplinkScheme,
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
    /// Metadata URL could not be parsed.
    #[error("host_metadata_url must be an absolute HTTPS URL: {reason}")]
    InvalidHostMetadataUrl {
        /// Parse failure reason.
        reason: String,
    },
    /// Metadata URL used a non-HTTPS scheme.
    #[error("host_metadata_url must use https scheme, got {scheme:?}")]
    InsecureHostMetadataUrl {
        /// Actual URL scheme.
        scheme: String,
    },
}

impl TryFrom<NativeRuntimeConfig> for RuntimeConfig {
    type Error = NativeRuntimeConfigError;

    fn try_from(config: NativeRuntimeConfig) -> Result<Self, Self::Error> {
        let people_chain_genesis_hash =
            <[u8; 32]>::try_from(config.people_chain_genesis_hash.as_slice()).map_err(|_| {
                NativeRuntimeConfigError::InvalidPeopleChainGenesisHash {
                    actual: config.people_chain_genesis_hash.len() as u64,
                }
            })?;
        let runtime_config = Self {
            product_label: config.product_label,
            product_id: config.product_id,
            site_id: config.site_id,
            host_metadata_url: config.host_metadata_url,
            people_chain_genesis_hash,
            pairing_deeplink_scheme: config.pairing_deeplink_scheme.into(),
        };
        runtime_config.validate()?;
        Ok(runtime_config)
    }
}

impl From<RuntimeConfigValidationError> for NativeRuntimeConfigError {
    fn from(err: RuntimeConfigValidationError) -> Self {
        match err {
            RuntimeConfigValidationError::EmptyField { field } => Self::EmptyField {
                field: field.to_string(),
            },
            RuntimeConfigValidationError::InvalidHostMetadataUrl { reason } => {
                Self::InvalidHostMetadataUrl { reason }
            }
            RuntimeConfigValidationError::InsecureHostMetadataUrl { scheme } => {
                Self::InsecureHostMetadataUrl { scheme }
            }
        }
    }
}

impl From<HostNavigateRejection> for v01::HostNavigateToError {
    fn from(err: HostNavigateRejection) -> Self {
        match err {
            HostNavigateRejection::PermissionDenied => v01::HostNavigateToError::PermissionDenied,
            HostNavigateRejection::Unknown { reason } => {
                v01::HostNavigateToError::Unknown { reason }
            }
        }
    }
}

/// Callback surface that iOS and Android implement. The Rust core invokes
/// these synchronously from `async` trait methods, which is acceptable for
/// UniFFI because every callback hop is short-lived and reentrant.
#[uniffi::export(callback_interface)]
pub trait HostCallbacks: Send + Sync {
    /// Lifecycle logger. Marker is a stable slug, detail is free-form.
    fn on_core_log(&self, marker: String, detail: String);

    /// Open a URL in the system browser.
    fn navigate_to(&self, url: String) -> Result<(), HostNavigateRejection>;

    /// Deliver a push notification. The payload is the SCALE-encoded
    /// [`v01::HostPushNotificationRequest`].
    fn push_notification(&self, payload: Vec<u8>) -> Result<u32, HostRejection>;

    /// Cancel a notification by id.
    fn cancel_notification(&self, id: u32) -> Result<(), HostRejection>;

    /// Prompt the user for a device-level permission (camera, mic, ...).
    /// `request` is the SCALE-encoded
    /// [`v01::HostDevicePermissionRequest`]; the host returns whether the
    /// permission was granted.
    fn device_permission(&self, request: Vec<u8>) -> Result<bool, HostRejection>;

    /// Prompt the user for a remote (product-scoped) permission bundle.
    /// `request` is the SCALE-encoded [`v01::RemotePermissionRequest`].
    fn remote_permission(&self, request: Vec<u8>) -> Result<bool, HostRejection>;

    /// Present an SSO pairing deeplink or QR payload built by the Rust core.
    /// Implementations should show and return immediately; user cancellation
    /// is reported through `NativeTrUApiCore.notify_pairing_cancelled()`.
    fn present_pairing(&self, deeplink: String) -> Result<(), HostRejection>;

    /// Close any active SSO pairing presentation.
    fn dismiss_pairing(&self);

    /// Read the opaque core-owned SSO session blob from host-global storage.
    fn read_session(&self) -> Result<Option<Vec<u8>>, HostRejection>;

    /// Persist the opaque core-owned SSO session blob in host-global storage.
    fn write_session(&self, value: Vec<u8>) -> Result<(), HostRejection>;

    /// Clear the persisted core-owned SSO session blob.
    fn clear_session(&self) -> Result<(), HostRejection>;

    /// Open a JSON-RPC connection for a chain. Return a host-assigned
    /// connection id, or `None` when unsupported.
    fn chain_connect(&self, genesis_hash: Vec<u8>) -> Result<Option<u32>, HostRejection>;

    /// Send one JSON-RPC request over a previously opened chain connection.
    fn chain_send(&self, connection_id: u32, request: String) -> Result<(), HostRejection>;

    /// Close a previously opened chain connection.
    fn chain_close(&self, connection_id: u32) -> Result<(), HostRejection>;

    /// Confirm a sign-payload request. `review` is a SCALE-encoded review
    /// payload owned by the Rust core.
    fn confirm_sign_payload(&self, review: Vec<u8>) -> Result<bool, HostRejection>;

    /// Confirm a sign-raw request. `review` is a SCALE-encoded review payload
    /// owned by the Rust core.
    fn confirm_sign_raw(&self, review: Vec<u8>) -> Result<bool, HostRejection>;

    /// Confirm a create-transaction request. `review` is a SCALE-encoded
    /// review payload owned by the Rust core.
    fn confirm_create_transaction(&self, review: Vec<u8>) -> Result<bool, HostRejection>;

    /// Confirm a cross-domain account-alias request. `review` is a
    /// SCALE-encoded review payload owned by the Rust core.
    fn confirm_account_alias(&self, review: Vec<u8>) -> Result<bool, HostRejection>;

    /// Confirm a resource-allocation request. `review` is a SCALE-encoded
    /// review payload owned by the Rust core.
    fn confirm_resource_allocation(&self, review: Vec<u8>) -> Result<bool, HostRejection>;

    /// Confirm preimage submission before the host stores it.
    fn confirm_preimage_submit(&self, size: u64) -> Result<(), HostRejection>;

    /// Submit the preimage through the host backend and return its key.
    fn submit_preimage(&self, value: Vec<u8>) -> Result<Vec<u8>, HostRejection>;

    /// Look up one preimage value by key. The native shim emits this as the
    /// current item in its subscription stream.
    fn lookup_preimage(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection>;

    /// Current host theme. The native shim emits this as the current item in
    /// its subscription stream.
    fn current_theme(&self) -> Result<HostTheme, HostRejection>;

    /// Answer a feature-support query. `request` is the SCALE-encoded
    /// [`HostFeatureSupportedRequest`].
    fn feature_supported(&self, request: Vec<u8>) -> Result<bool, HostRejection>;

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
    core: Arc<TrUApiCore>,
    events: Arc<NativeEventBus>,
    #[cfg(feature = "ws-bridge")]
    callbacks: Arc<dyn HostCallbacks>,
    #[cfg(feature = "ws-bridge")]
    bridge: std::sync::Mutex<Option<WsBridge>>,
}

#[uniffi::export]
impl NativeTrUApiCore {
    /// Construct the core from a callback object. The native shell hands
    /// over its [`HostCallbacks`] trait object; the core wraps it in a
    /// [`CallbackPlatform`] and feeds the result into
    /// [`TrUApiCore::from_platform`].
    ///
    /// Subscriptions registered through this core run on a shared
    /// `futures::executor::ThreadPool`. The pool sticks around for the
    /// lifetime of the core; new subscriptions never spawn a fresh OS
    /// thread each.
    #[uniffi::constructor]
    pub fn new(callbacks: Box<dyn HostCallbacks>) -> Arc<Self> {
        native_core_from_platform_config(callbacks, RuntimeConfig::compatibility_default())
    }

    /// Construct the core with explicit product and pairing runtime config.
    #[uniffi::constructor]
    pub fn with_runtime_config(
        callbacks: Box<dyn HostCallbacks>,
        runtime_config: NativeRuntimeConfig,
    ) -> Result<Arc<Self>, NativeRuntimeConfigError> {
        Ok(native_core_from_platform_config(
            callbacks,
            runtime_config.try_into()?,
        ))
    }

    /// Core-owned logout/disconnect. Best-effort notifies the SSO peer when
    /// the session has channel material, then clears in-memory and persisted
    /// session state.
    pub fn disconnect(&self) {
        self.core.disconnect();
    }

    /// Notify this core that host-global session storage changed outside a
    /// direct core write/clear. Native hosts call this after cross-process or
    /// platform storage notifications so the core re-reads `SessionStore`.
    pub fn notify_session_store_changed(&self) {
        self.events.notify_session_store_changed();
    }

    /// Notify the core that the user dismissed the active SSO pairing UI.
    pub fn notify_pairing_cancelled(&self) {
        self.events.notify_pairing_cancelled();
    }

    /// Push a host theme update to active TrUAPI theme subscriptions.
    pub fn notify_theme_changed(&self, theme: HostTheme) {
        self.events.notify_theme_changed(theme.into());
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

fn native_core_from_platform_config(
    callbacks: Box<dyn HostCallbacks>,
    runtime_config: RuntimeConfig,
) -> Arc<NativeTrUApiCore> {
    let callbacks: Arc<dyn HostCallbacks> = callbacks.into();
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
    Arc::new(NativeTrUApiCore {
        core: Arc::new(TrUApiCore::from_platform_with_config(
            platform,
            runtime_config,
            spawner,
        )),
        events,
        #[cfg(feature = "ws-bridge")]
        callbacks,
        #[cfg(feature = "ws-bridge")]
        bridge: std::sync::Mutex::new(None),
    })
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
        let (bridge, endpoint) = WsBridge::start(bind_port, self.core.clone(), logger)?;
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
    pairing_cancels: Mutex<Vec<oneshot::Sender<()>>>,
    session_store_ticks: Mutex<Vec<mpsc::UnboundedSender<Result<(), v01::GenericError>>>>,
    theme_changes: Mutex<Vec<mpsc::UnboundedSender<Result<v01::Theme, v01::GenericError>>>>,
    preimage_changes: Mutex<Vec<PreimageSubscription>>,
    chain_responses: Mutex<HashMap<u32, mpsc::UnboundedSender<String>>>,
}

struct PreimageSubscription {
    key: Vec<u8>,
    tx: mpsc::UnboundedSender<Result<Option<Vec<u8>>, v01::GenericError>>,
}

impl NativeEventBus {
    fn register_pairing_cancel(&self) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        self.pairing_cancels
            .lock()
            .expect("native pairing cancel waiters mutex poisoned")
            .push(tx);
        rx
    }

    fn notify_pairing_cancelled(&self) {
        let waiters = std::mem::take(
            &mut *self
                .pairing_cancels
                .lock()
                .expect("native pairing cancel waiters mutex poisoned"),
        );
        for tx in waiters {
            let _ = tx.send(());
        }
    }

    fn subscribe_session_store(&self) -> BoxStream<'static, Result<(), v01::GenericError>> {
        let (tx, rx) = mpsc::unbounded();
        self.session_store_ticks
            .lock()
            .expect("native session store subscribers mutex poisoned")
            .push(tx);
        stream::once(async { Ok(()) }).chain(rx).boxed()
    }

    fn notify_session_store_changed(&self) {
        self.session_store_ticks
            .lock()
            .expect("native session store subscribers mutex poisoned")
            .retain(|tx| tx.unbounded_send(Ok(())).is_ok());
    }

    fn subscribe_theme(
        &self,
        current: Result<v01::Theme, v01::GenericError>,
    ) -> BoxStream<'static, Result<v01::Theme, v01::GenericError>> {
        let (tx, rx) = mpsc::unbounded();
        self.theme_changes
            .lock()
            .expect("native theme subscribers mutex poisoned")
            .push(tx);
        stream::once(async move { current }).chain(rx).boxed()
    }

    fn notify_theme_changed(&self, theme: v01::Theme) {
        self.theme_changes
            .lock()
            .expect("native theme subscribers mutex poisoned")
            .retain(|tx| tx.unbounded_send(Ok(theme)).is_ok());
    }

    fn subscribe_preimage(
        &self,
        key: Vec<u8>,
        current: Result<Option<Vec<u8>>, v01::GenericError>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        let (tx, rx) = mpsc::unbounded();
        self.preimage_changes
            .lock()
            .expect("native preimage subscribers mutex poisoned")
            .push(PreimageSubscription { key, tx });
        stream::once(async move { current }).chain(rx).boxed()
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

impl Navigation for CallbackPlatform {
    async fn navigate_to(&self, url: String) -> Result<(), v01::HostNavigateToError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.navigate_to".to_string(),
            url.clone(),
        );
        self.callbacks.navigate_to(url).map_err(Into::into)
    }
}

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
            .push_notification(notification.encode())
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
            .device_permission(request.encode())
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
            .remote_permission(request.encode())
            .map_err(v01::GenericError::from)?;
        Ok(v01::RemotePermissionResponse { granted })
    }
}

impl Features for CallbackPlatform {
    async fn feature_supported(
        &self,
        request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, v01::GenericError> {
        let supported = self
            .callbacks
            .feature_supported(request.encode())
            .map_err(v01::GenericError::from)?;
        Ok(HostFeatureSupportedResponse::V1(
            v01::HostFeatureSupportedResponse { supported },
        ))
    }
}

impl Storage for CallbackPlatform {
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
}

impl Drop for NativeJsonRpcConnection {
    fn drop(&mut self) {
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

impl ChainProvider for CallbackPlatform {
    async fn connect(
        &self,
        genesis_hash: Vec<u8>,
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        let Some(connection_id) = self
            .callbacks
            .chain_connect(genesis_hash)
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

impl PairingPresenter for CallbackPlatform {
    async fn present_pairing(&self, deeplink: String) -> Result<(), v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.present_pairing".to_string(),
            String::new(),
        );
        let cancel = self.events.register_pairing_cancel();
        self.callbacks
            .present_pairing(deeplink)
            .map_err(v01::GenericError::from)?;
        let _dismiss = NativePairingDismiss {
            callbacks: self.callbacks.clone(),
        };
        cancel.await.map_err(|_| v01::GenericError {
            reason: "pairing presenter cancelled by core".to_string(),
        })
    }
}

struct NativePairingDismiss {
    callbacks: Arc<dyn HostCallbacks>,
}

impl Drop for NativePairingDismiss {
    fn drop(&mut self) {
        self.callbacks.dismiss_pairing();
    }
}

impl SessionStore for CallbackPlatform {
    async fn read_session(&self) -> Result<Option<Vec<u8>>, v01::GenericError> {
        self.callbacks
            .read_session()
            .map_err(v01::GenericError::from)
    }

    async fn write_session(&self, value: Vec<u8>) -> Result<(), v01::GenericError> {
        self.callbacks
            .write_session(value)
            .map_err(v01::GenericError::from)?;
        self.events.notify_session_store_changed();
        Ok(())
    }

    async fn clear_session(&self) -> Result<(), v01::GenericError> {
        self.callbacks
            .clear_session()
            .map_err(v01::GenericError::from)?;
        self.events.notify_session_store_changed();
        Ok(())
    }

    fn subscribe_session_store(&self) -> BoxStream<'static, Result<(), v01::GenericError>> {
        self.events.subscribe_session_store()
    }
}

impl UserConfirmation for CallbackPlatform {
    async fn confirm_sign_payload(&self, review: Vec<u8>) -> Result<bool, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.confirm_sign_payload".to_string(),
            String::new(),
        );
        self.callbacks
            .confirm_sign_payload(review)
            .map_err(v01::GenericError::from)
    }

    async fn confirm_sign_raw(&self, review: Vec<u8>) -> Result<bool, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.confirm_sign_raw".to_string(),
            String::new(),
        );
        self.callbacks
            .confirm_sign_raw(review)
            .map_err(v01::GenericError::from)
    }

    async fn confirm_create_transaction(&self, review: Vec<u8>) -> Result<bool, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.confirm_create_transaction".to_string(),
            String::new(),
        );
        self.callbacks
            .confirm_create_transaction(review)
            .map_err(v01::GenericError::from)
    }

    async fn confirm_account_alias(&self, review: Vec<u8>) -> Result<bool, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.confirm_account_alias".to_string(),
            String::new(),
        );
        self.callbacks
            .confirm_account_alias(review)
            .map_err(v01::GenericError::from)
    }

    async fn confirm_resource_allocation(
        &self,
        review: Vec<u8>,
    ) -> Result<bool, v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.confirm_resource_allocation".to_string(),
            String::new(),
        );
        self.callbacks
            .confirm_resource_allocation(review)
            .map_err(v01::GenericError::from)
    }
}

impl ThemeHost for CallbackPlatform {
    fn subscribe_theme(&self) -> BoxStream<'static, Result<v01::Theme, v01::GenericError>> {
        let current = self
            .callbacks
            .current_theme()
            .map(v01::Theme::from)
            .map_err(v01::GenericError::from);
        self.events.subscribe_theme(current)
    }
}

impl PreimageHost for CallbackPlatform {
    async fn confirm_preimage_submit(&self, size: u64) -> Result<(), v01::PreimageSubmitError> {
        self.callbacks.confirm_preimage_submit(size).map_err(|err| {
            v01::PreimageSubmitError::Unknown {
                reason: err.to_string(),
            }
        })
    }

    async fn submit_preimage(&self, value: Vec<u8>) -> Result<Vec<u8>, v01::PreimageSubmitError> {
        self.callbacks
            .submit_preimage(value)
            .map_err(|err| v01::PreimageSubmitError::Unknown {
                reason: err.to_string(),
            })
    }

    fn lookup_preimage(
        &self,
        key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        let current = self
            .callbacks
            .lookup_preimage(key.clone())
            .map_err(v01::GenericError::from);
        self.events.subscribe_preimage(key, current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type PreimageFixtureEntries = Vec<(Vec<u8>, Option<Vec<u8>>)>;

    struct EventCallbacks {
        theme: Mutex<HostTheme>,
        preimages: Mutex<PreimageFixtureEntries>,
        presented_pairings: Mutex<Vec<String>>,
        dismissed_pairings: Mutex<u32>,
        chain_id: Mutex<Option<u32>>,
        chain_connects: Mutex<Vec<Vec<u8>>>,
        chain_sends: Mutex<Vec<(u32, String)>>,
        chain_closes: Mutex<Vec<u32>>,
    }

    impl EventCallbacks {
        fn new() -> Self {
            Self {
                theme: Mutex::new(HostTheme::Light),
                preimages: Mutex::new(Vec::new()),
                presented_pairings: Mutex::new(Vec::new()),
                dismissed_pairings: Mutex::new(0),
                chain_id: Mutex::new(None),
                chain_connects: Mutex::new(Vec::new()),
                chain_sends: Mutex::new(Vec::new()),
                chain_closes: Mutex::new(Vec::new()),
            }
        }
    }

    impl HostCallbacks for EventCallbacks {
        fn on_core_log(&self, _marker: String, _detail: String) {}
        fn navigate_to(&self, _url: String) -> Result<(), HostNavigateRejection> {
            Ok(())
        }
        fn push_notification(&self, _payload: Vec<u8>) -> Result<u32, HostRejection> {
            Ok(0)
        }
        fn cancel_notification(&self, _id: u32) -> Result<(), HostRejection> {
            Ok(())
        }
        fn device_permission(&self, _request: Vec<u8>) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn remote_permission(&self, _request: Vec<u8>) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn present_pairing(&self, deeplink: String) -> Result<(), HostRejection> {
            self.presented_pairings
                .lock()
                .expect("presented pairings mutex poisoned")
                .push(deeplink);
            Ok(())
        }
        fn dismiss_pairing(&self) {
            *self
                .dismissed_pairings
                .lock()
                .expect("dismissed pairings mutex poisoned") += 1;
        }
        fn read_session(&self) -> Result<Option<Vec<u8>>, HostRejection> {
            Ok(None)
        }
        fn write_session(&self, _value: Vec<u8>) -> Result<(), HostRejection> {
            Ok(())
        }
        fn clear_session(&self) -> Result<(), HostRejection> {
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
        fn confirm_sign_payload(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn confirm_sign_raw(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn confirm_create_transaction(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn confirm_account_alias(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn confirm_resource_allocation(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
            Ok(false)
        }
        fn confirm_preimage_submit(&self, _size: u64) -> Result<(), HostRejection> {
            Ok(())
        }
        fn submit_preimage(&self, value: Vec<u8>) -> Result<Vec<u8>, HostRejection> {
            Ok(value)
        }
        fn lookup_preimage(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
            Ok(self
                .preimages
                .lock()
                .expect("preimage map mutex poisoned")
                .iter()
                .find(|(stored_key, _)| stored_key == &key)
                .and_then(|(_, value)| value.clone()))
        }
        fn current_theme(&self) -> Result<HostTheme, HostRejection> {
            Ok(*self.theme.lock().expect("theme mutex poisoned"))
        }
        fn feature_supported(&self, _request: Vec<u8>) -> Result<bool, HostRejection> {
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

    #[test]
    fn native_pairing_presenter_waits_for_cancel_notification() {
        let (callbacks, events, platform) = event_platform();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            tx.send(futures::executor::block_on(
                platform.present_pairing("polkadotapp://pair?handshake=00".to_string()),
            ))
            .expect("send pairing result");
        });

        for _ in 0..100 {
            if !callbacks
                .presented_pairings
                .lock()
                .expect("presented pairings mutex poisoned")
                .is_empty()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        assert_eq!(
            callbacks
                .presented_pairings
                .lock()
                .expect("presented pairings mutex poisoned")
                .as_slice(),
            &["polkadotapp://pair?handshake=00".to_string()]
        );
        assert!(
            rx.try_recv().is_err(),
            "pairing presenter must stay pending until native cancel"
        );

        events.notify_pairing_cancelled();
        assert!(rx.recv().expect("pairing result").is_ok());
        handle.join().expect("pairing presenter thread joins");
        assert_eq!(
            *callbacks
                .dismissed_pairings
                .lock()
                .expect("dismissed pairings mutex poisoned"),
            1
        );
    }

    #[test]
    fn native_session_store_subscription_emits_current_then_notified_ticks() {
        let (_callbacks, events, platform) = event_platform();
        let mut stream = platform.subscribe_session_store();

        let first = futures::executor::block_on(stream.next()).unwrap();
        events.notify_session_store_changed();
        let second = futures::executor::block_on(stream.next()).unwrap();

        assert!(first.is_ok());
        assert!(second.is_ok());
    }

    #[test]
    fn native_theme_subscription_emits_current_then_notified_changes() {
        let (callbacks, events, platform) = event_platform();
        let mut stream = platform.subscribe_theme();

        let first = futures::executor::block_on(stream.next()).unwrap();
        *callbacks.theme.lock().expect("theme mutex poisoned") = HostTheme::Dark;
        events.notify_theme_changed(v01::Theme::Dark);
        let second = futures::executor::block_on(stream.next()).unwrap();

        assert_eq!(first.unwrap(), v01::Theme::Light);
        assert_eq!(second.unwrap(), v01::Theme::Dark);
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
        let genesis = vec![9; 32];

        let connection =
            futures::executor::block_on(ChainProvider::connect(&platform, genesis.clone()))
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
            &[genesis]
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
        let err = RuntimeConfig::try_from(NativeRuntimeConfig {
            product_label: "app".to_string(),
            product_id: "app.dot".to_string(),
            site_id: "dot.li".to_string(),
            host_metadata_url: "https://example.invalid/metadata.json".to_string(),
            people_chain_genesis_hash: vec![0; 31],
            pairing_deeplink_scheme: NativePairingDeeplinkScheme::PolkadotApp,
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::InvalidPeopleChainGenesisHash { actual: 31 }
        ));
    }

    #[test]
    fn runtime_config_rejects_empty_required_fields() {
        let err = RuntimeConfig::try_from(NativeRuntimeConfig {
            product_label: "app".to_string(),
            product_id: " ".to_string(),
            site_id: "dot.li".to_string(),
            host_metadata_url: "https://example.invalid/metadata.json".to_string(),
            people_chain_genesis_hash: vec![0; 32],
            pairing_deeplink_scheme: NativePairingDeeplinkScheme::PolkadotApp,
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::EmptyField { field } if field == "product_id"
        ));
    }

    #[test]
    fn runtime_config_rejects_relative_metadata_url() {
        let err = RuntimeConfig::try_from(NativeRuntimeConfig {
            product_label: "app".to_string(),
            product_id: "app.dot".to_string(),
            site_id: "dot.li".to_string(),
            host_metadata_url: "/metadata.json".to_string(),
            people_chain_genesis_hash: vec![0; 32],
            pairing_deeplink_scheme: NativePairingDeeplinkScheme::PolkadotApp,
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::InvalidHostMetadataUrl { .. }
        ));
    }

    #[test]
    fn runtime_config_rejects_non_https_metadata_url() {
        let err = RuntimeConfig::try_from(NativeRuntimeConfig {
            product_label: "app".to_string(),
            product_id: "app.dot".to_string(),
            site_id: "dot.li".to_string(),
            host_metadata_url: "http://localhost:3000/metadata.json".to_string(),
            people_chain_genesis_hash: vec![0; 32],
            pairing_deeplink_scheme: NativePairingDeeplinkScheme::PolkadotApp,
        })
        .unwrap_err();

        assert!(matches!(
            err,
            NativeRuntimeConfigError::InsecureHostMetadataUrl { scheme } if scheme == "http"
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
        impl HostCallbacks for Noop {
            fn on_core_log(&self, _marker: String, _detail: String) {}
            fn navigate_to(&self, _url: String) -> Result<(), HostNavigateRejection> {
                Ok(())
            }
            fn push_notification(&self, _payload: Vec<u8>) -> Result<u32, HostRejection> {
                Ok(0)
            }
            fn cancel_notification(&self, _id: u32) -> Result<(), HostRejection> {
                Ok(())
            }
            fn device_permission(&self, _request: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn remote_permission(&self, _request: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn present_pairing(&self, _deeplink: String) -> Result<(), HostRejection> {
                Ok(())
            }
            fn dismiss_pairing(&self) {}
            fn read_session(&self) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn write_session(&self, _value: Vec<u8>) -> Result<(), HostRejection> {
                Ok(())
            }
            fn clear_session(&self) -> Result<(), HostRejection> {
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
            fn confirm_sign_payload(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn confirm_sign_raw(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn confirm_create_transaction(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn confirm_account_alias(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn confirm_resource_allocation(&self, _review: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn confirm_preimage_submit(&self, _size: u64) -> Result<(), HostRejection> {
                Ok(())
            }
            fn submit_preimage(&self, value: Vec<u8>) -> Result<Vec<u8>, HostRejection> {
                Ok(value)
            }
            fn lookup_preimage(&self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, HostRejection> {
                Ok(None)
            }
            fn current_theme(&self) -> Result<HostTheme, HostRejection> {
                Ok(HostTheme::Light)
            }
            fn feature_supported(&self, _request: Vec<u8>) -> Result<bool, HostRejection> {
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

        let core = NativeTrUApiCore::new(Box::new(Noop));
        let _first = core.start_ws_bridge(0).expect("first start must succeed");
        let err = core
            .start_ws_bridge(0)
            .expect_err("second start must error");
        assert!(matches!(err, WsBridgeStartError::AlreadyRunning));
        core.stop_ws_bridge();
    }
}
