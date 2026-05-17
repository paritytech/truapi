//! UniFFI-facing native bridge. Exposes [`NativeTrUApiCore`] and the
//! [`HostCallbacks`] callback interface that iOS and Android call into.
//!
//! The native side builds a [`CallbackPlatform`] that adapts every
//! [`truapi_platform::Platform`] trait to a corresponding callback. The
//! resulting platform is fed into [`TrUApiCore::from_platform`] so the rest
//! of the dispatcher pipeline behaves identically to the WS-bridge and wasm
//! flavors.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::executor::ThreadPool;
use futures::future::BoxFuture;
use futures::task::SpawnExt;
use parity_scale_codec::{Decode, Encode};
use truapi::v01;
use truapi::versioned::system::{HostFeatureSupportedRequest, HostFeatureSupportedResponse};
use truapi_platform::{
    ChainProvider, Features, JsonRpcConnection, Navigation, Notifications, Permissions, Storage,
};

use crate::subscription::Spawner;
#[cfg(feature = "ws-bridge")]
use crate::ws_bridge::{BridgeLogger, WsBridge, WsBridgeEndpoint, WsBridgeStartError};
use crate::{Payload, ProtocolMessage, TrUApiCore, Transport};

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
        v01::GenericError::GenericError(v01::GenericErr { reason })
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

    /// Forward an outbound protocol frame (already SCALE-encoded) to the
    /// product. The native shell pumps these into the in-app messaging
    /// channel.
    fn on_core_response(&self, frame: Vec<u8>);

    /// Open a URL in the system browser.
    fn navigate_to(&self, url: String) -> Result<(), HostNavigateRejection>;

    /// Deliver a push notification. The payload is the SCALE-encoded
    /// [`v01::HostPushNotificationRequest`].
    fn push_notification(&self, payload: Vec<u8>) -> Result<(), HostRejection>;

    /// Prompt the user for a device-level permission (camera, mic, ...).
    /// `request` is the SCALE-encoded
    /// [`v01::HostDevicePermissionRequest`]; the host returns whether the
    /// permission was granted.
    fn device_permission(&self, request: Vec<u8>) -> Result<bool, HostRejection>;

    /// Prompt the user for a remote (product-scoped) permission bundle.
    /// `request` is the SCALE-encoded [`v01::RemotePermissionRequest`].
    fn remote_permission(&self, request: Vec<u8>) -> Result<bool, HostRejection>;

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

    /// Push an inbound SCALE-encoded protocol frame from the product into
    /// the dispatcher. Responses are emitted back through the
    /// [`HostCallbacks::on_core_response`] callback.
    pub fn receive_from_product(&self, frame: Vec<u8>) -> bool {
        self.callbacks.on_core_log(
            "truapi.native.core.inbound".to_string(),
            format!("frame_bytes={}", frame.len()),
        );

        let callbacks = self.callbacks.clone();
        let core = self.core.clone();
        match catch_unwind(AssertUnwindSafe(|| {
            let message = ProtocolMessage::decode(&mut &*frame).ok();
            message.map(|message| {
                let transport = Arc::new(NativeCallbackTransport::new(callbacks.clone()));
                let transport_dyn: Arc<dyn Transport> = transport.clone();
                futures::executor::block_on(core.dispatch(message, transport_dyn));
                transport
            })
        })) {
            Ok(Some(transport)) => {
                if transport.sent_count() > 0 {
                    self.callbacks.on_core_log(
                        "truapi.native.core.request.ok".to_string(),
                        format!("response_frames={}", transport.sent_count()),
                    );
                } else {
                    self.callbacks.on_core_log(
                        "truapi.native.core.request.no_response".to_string(),
                        "dispatcher produced no frame".to_string(),
                    );
                }
                true
            }
            Ok(None) => {
                self.callbacks.on_core_log(
                    "truapi.native.core.request.decode_failed".to_string(),
                    "failed to decode inbound frame".to_string(),
                );
                false
            }
            Err(_) => {
                self.callbacks.on_core_log(
                    "truapi.native.core.request.panic".to_string(),
                    "request handling panicked".to_string(),
                );
                false
            }
        }
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
                lite_username,
                full_username,
            });
        true
    }

    /// Drop the currently-paired session. Mirrors the JS
    /// `clearActiveSession`.
    pub fn clear_active_session(&self) {
        self.core.session_state().clear_session();
    }

    /// Smoke-test helper: return a SCALE-encoded `feature_supported`
    /// request frame so the iOS/Android shells can verify the wire path
    /// without owning request construction logic.
    pub fn debug_smoke_feature_request_frame(&self) -> Vec<u8> {
        ProtocolMessage {
            request_id: "native-smoke:1".to_string(),
            payload: Payload {
                tag: "system_feature_supported_request".to_string(),
                value: HostFeatureSupportedRequest::V1(v01::HostFeatureSupportedRequest::Chain {
                    genesis_hash: vec![1u8; 32],
                })
                .encode(),
            },
        }
        .encode()
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
    ) -> Result<(), v01::GenericError> {
        self.callbacks.on_core_log(
            "truapi.native.callback.push_notification".to_string(),
            notification.text.clone(),
        );
        self.callbacks
            .push_notification(notification.encode())
            .map_err(Into::into)
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

// Chain capability is not wired through the callback surface. The platform
// trait requires an impl, so we stub it as an "unavailable" response.
// Account/signing/statement-store/preimage flows live in the Rust core
// itself; their `truapi::api::*` trait defaults return `Unsupported`.

impl ChainProvider for CallbackPlatform {
    async fn connect(
        &self,
        _genesis_hash: Vec<u8>,
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        Err(v01::GenericError::GenericError(v01::GenericErr {
            reason: "chain provider not wired through native callbacks".into(),
        }))
    }
}

struct NativeCallbackTransport {
    callbacks: Arc<dyn HostCallbacks>,
    sent: AtomicUsize,
}

impl NativeCallbackTransport {
    fn new(callbacks: Arc<dyn HostCallbacks>) -> Self {
        Self {
            callbacks,
            sent: AtomicUsize::new(0),
        }
    }

    fn sent_count(&self) -> usize {
        self.sent.load(Ordering::Relaxed)
    }
}

impl Transport for NativeCallbackTransport {
    fn send(&self, message: ProtocolMessage) {
        self.sent.fetch_add(1, Ordering::Relaxed);
        self.callbacks.on_core_response(message.encode());
    }

    fn on_message(
        &self,
        _handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
    ) -> Box<dyn FnOnce()> {
        Box::new(|| {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Capturing callback object: records every outbound response frame
    /// and returns deterministic answers on every prompt. Used to verify
    /// that the trait is object-safe and that an inbound
    /// `feature_supported` request actually round-trips through the
    /// dispatcher.
    struct CapturingCallbacks {
        responses: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    impl HostCallbacks for CapturingCallbacks {
        fn on_core_log(&self, _marker: String, _detail: String) {}
        fn on_core_response(&self, frame: Vec<u8>) {
            self.responses.lock().unwrap().push(frame);
        }
        fn navigate_to(&self, _url: String) -> Result<(), HostNavigateRejection> {
            Ok(())
        }
        fn push_notification(&self, _payload: Vec<u8>) -> Result<(), HostRejection> {
            Ok(())
        }
        fn device_permission(&self, _request: Vec<u8>) -> Result<bool, HostRejection> {
            Ok(true)
        }
        fn remote_permission(&self, _request: Vec<u8>) -> Result<bool, HostRejection> {
            Ok(true)
        }
        fn feature_supported(&self, _request: Vec<u8>) -> Result<bool, HostRejection> {
            Ok(true)
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

    #[test]
    fn native_core_round_trips_feature_supported_through_callbacks() {
        let responses: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
        let core = NativeTrUApiCore::new(Box::new(CapturingCallbacks {
            responses: responses.clone(),
        }));

        let frame = core.debug_smoke_feature_request_frame();
        assert!(core.receive_from_product(frame));

        let frames = responses.lock().unwrap();
        assert_eq!(frames.len(), 1, "expected one response frame");
        let response = ProtocolMessage::decode(&mut &frames[0][..]).expect("decode response frame");
        assert_eq!(response.request_id, "native-smoke:1");
        // Wire payload: `Result<Ok, Err>`-shaped:
        // [Ok disc=0x00][V1 variant 0x00][supported=1]
        assert_eq!(response.payload.value, vec![0x00, 0x00, 0x01]);
    }

    #[test]
    fn set_active_session_rejects_wrong_size_pubkey() {
        struct Noop;
        impl HostCallbacks for Noop {
            fn on_core_log(&self, _marker: String, _detail: String) {}
            fn on_core_response(&self, _frame: Vec<u8>) {}
            fn navigate_to(&self, _url: String) -> Result<(), HostNavigateRejection> {
                Ok(())
            }
            fn push_notification(&self, _payload: Vec<u8>) -> Result<(), HostRejection> {
                Ok(())
            }
            fn device_permission(&self, _request: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn remote_permission(&self, _request: Vec<u8>) -> Result<bool, HostRejection> {
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
            fn on_core_response(&self, _frame: Vec<u8>) {}
            fn navigate_to(&self, _url: String) -> Result<(), HostNavigateRejection> {
                Ok(())
            }
            fn push_notification(&self, _payload: Vec<u8>) -> Result<(), HostRejection> {
                Ok(())
            }
            fn device_permission(&self, _request: Vec<u8>) -> Result<bool, HostRejection> {
                Ok(false)
            }
            fn remote_permission(&self, _request: Vec<u8>) -> Result<bool, HostRejection> {
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
