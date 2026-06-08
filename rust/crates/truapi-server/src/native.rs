//! UniFFI-facing native bridge. Exposes [`NativeTrUApiCore`] and the
//! [`HostCallbacks`] callback interface that iOS and Android call into.
//!
//! The native side builds a [`CallbackPlatform`] that adapts every
//! [`truapi_platform::Platform`] trait to a corresponding callback. The
//! resulting platform is fed into [`TrUApiCore::from_platform`] so the rest
//! of the dispatcher pipeline behaves identically to the WS-bridge and wasm
//! flavors.

use std::sync::Arc;

use futures::executor::ThreadPool;
use futures::future::BoxFuture;
use futures::stream::{self, BoxStream, StreamExt};
use futures::task::SpawnExt;
use parity_scale_codec::Encode;
use truapi::v01;
use truapi::versioned::system::{HostFeatureSupportedRequest, HostFeatureSupportedResponse};
use truapi_platform::{
    ChainProvider, Features, JsonRpcConnection, Navigation, Notifications, PairingPresenter,
    Permissions, PreimageHost, SessionStore, Storage, ThemeHost, UserConfirmation,
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
        let callbacks: Arc<dyn HostCallbacks> = callbacks.into();
        callbacks.on_core_log(
            "truapi.native.core.boot".to_string(),
            "core ready".to_string(),
        );

        let platform = Arc::new(CallbackPlatform {
            callbacks: callbacks.clone(),
        });
        let spawner = native_thread_pool_spawner(&callbacks);
        Arc::new(Self {
            core: Arc::new(TrUApiCore::from_platform(platform, spawner)),
            callbacks,
            #[cfg(feature = "ws-bridge")]
            bridge: std::sync::Mutex::new(None),
        })
    }

    /// Push the currently-paired session into the core. Mirrors the JS
    /// `setActiveSession`. `pubkey` must be exactly 32 bytes (sr25519 root
    /// public key).
    pub fn set_active_session(
        &self,
        pubkey: Vec<u8>,
        lite_username: Option<String>,
        full_username: Option<String>,
    ) -> bool {
        let Ok(public_key) = <[u8; 32]>::try_from(pubkey.as_slice()) else {
            self.callbacks.on_core_log(
                "truapi.native.core.session.invalid_pubkey".to_string(),
                format!("expected 32 bytes, got {}", pubkey.len()),
            );
            return false;
        };
        self.core
            .session_state()
            .set_session(crate::host_logic::session::SessionInfo {
                public_key,
                entropy_secret: None,
                lite_username,
                full_username,
            });
        true
    }

    /// Attach the host-papp session `ssSecret` used by current dotli entropy
    /// derivation. Returns false when no active session has been pushed yet.
    pub fn set_active_session_entropy_secret(&self, secret: Vec<u8>) -> bool {
        self.core.session_state().set_entropy_secret(secret)
    }

    /// Drop the currently-paired session. Mirrors the JS
    /// `clearActiveSession`.
    pub fn clear_active_session(&self) {
        self.core.session_state().clear_session();
    }
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

// `ChainProvider` is not exposed through the callback surface. The trait
// is required for `Platform`, so the impl stubs `connect` as
// `Unavailable`. Hosts that need chain access wire it directly into the
// Rust core (e.g. via `RuntimeChainProvider`) rather than through the
// JNI/Swift callback boundary.

impl ChainProvider for CallbackPlatform {
    async fn connect(
        &self,
        _genesis_hash: Vec<u8>,
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        Err(v01::GenericError {
            reason: "chain provider not wired through native callbacks".into(),
        })
    }
}

impl PairingPresenter for CallbackPlatform {
    async fn present_pairing(&self, deeplink: String) -> Result<(), v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.present_pairing.unavailable".to_string(),
            deeplink,
        );
        Err(v01::GenericError {
            reason: "pairing presenter callback not wired through native callbacks".to_string(),
        })
    }
}

impl SessionStore for CallbackPlatform {
    async fn read_session(&self) -> Result<Option<Vec<u8>>, v01::GenericError> {
        Ok(None)
    }

    async fn write_session(&self, _value: Vec<u8>) -> Result<(), v01::GenericError> {
        Ok(())
    }

    async fn clear_session(&self) -> Result<(), v01::GenericError> {
        Ok(())
    }

    fn subscribe_session_store(&self) -> BoxStream<'static, Result<(), v01::GenericError>> {
        stream::once(async { Ok(()) }).boxed()
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
        stream::empty().boxed()
    }
}

impl PreimageHost for CallbackPlatform {
    async fn confirm_preimage_submit(&self, _size: u64) -> Result<(), v01::PreimageSubmitError> {
        Err(v01::PreimageSubmitError::Unknown {
            reason: "preimage confirmation callback not wired through native callbacks".to_string(),
        })
    }

    async fn submit_preimage(&self, _value: Vec<u8>) -> Result<Vec<u8>, v01::PreimageSubmitError> {
        Err(v01::PreimageSubmitError::Unknown {
            reason: "preimage submit callback not wired through native callbacks".to_string(),
        })
    }

    fn lookup_preimage(
        &self,
        _key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        stream::empty().boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_active_session_rejects_wrong_size_pubkey() {
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
        assert!(!core.set_active_session(vec![0u8; 16], None, None));
        assert!(core.set_active_session(vec![0u8; 32], None, None));
        core.clear_active_session();
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
