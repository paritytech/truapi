//! `TrUApiCore`: the entrypoint a host wraps around a `truapi::api::TrUApi`
//! implementation (direct path) or a `truapi_platform::Platform`
//! implementation (platform path).
//!
//! Direct path: `TrUApiCore::new(host)` accepts anything implementing
//! the unified [`truapi::api::TrUApi`] super-trait. Useful for unit tests
//! and bespoke hosts.
//!
//! Platform path: [`TrUApiCore::from_platform_with_config`] takes a
//! [`truapi_platform::Platform`] and wires it through
//! [`crate::runtime::PlatformRuntimeHost`] before registering with the
//! generated dispatcher. This is the path real platform shims (UniFFI,
//! wasm-bindgen, ws-bridge, ...) take.

use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;
use parity_scale_codec::{Decode, Encode};
use tracing::instrument;
use truapi::api::TrUApi;
use truapi_platform::{Platform, RuntimeConfig};

use crate::generated::dispatcher;
use crate::host_logic::session::SessionState;
use crate::runtime::PlatformRuntimeHost;
use crate::subscription::Spawner;
use crate::{Dispatcher, ProtocolMessage, Transport};

type DisconnectFn = Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>;

/// Top-level core. Owns the dispatcher and, on the platform path, the shared
/// session-state holder.
pub struct TrUApiCore {
    dispatcher: Dispatcher,
    /// Always present; empty for [`Self::new`] (direct host path), connected
    /// to a [`PlatformRuntimeHost`] for [`Self::from_platform_with_config`].
    session_state: Arc<SessionState>,
    disconnect: DisconnectFn,
}

impl TrUApiCore {
    /// Build a core around a direct `TrUApi` implementation. The session
    /// state holder is unused on this path (no platform pushes updates),
    /// but is created anyway so the public API surface stays consistent.
    /// Subscription work runs on `spawner`.
    #[instrument(skip_all, fields(runtime.method = "core.new"))]
    pub fn new<P>(host: Arc<P>, spawner: Spawner) -> Self
    where
        P: TrUApi + 'static,
    {
        let mut dispatcher = Dispatcher::new(spawner);
        dispatcher::register(&mut dispatcher, host);
        let session_state = SessionState::new();
        let disconnect_state = session_state.clone();
        Self {
            dispatcher,
            session_state,
            disconnect: Arc::new(move || {
                let state = disconnect_state.clone();
                Box::pin(async move {
                    state.clear_session();
                })
            }),
        }
    }

    /// Build a core around a [`Platform`] implementation and explicit product
    /// runtime configuration.
    #[instrument(skip_all, fields(runtime.method = "core.from_platform_with_config"))]
    pub fn from_platform_with_config<P>(
        platform: Arc<P>,
        runtime_config: RuntimeConfig,
        spawner: Spawner,
    ) -> Self
    where
        P: Platform + 'static,
    {
        let runtime = Arc::new(PlatformRuntimeHost::new(
            platform,
            runtime_config,
            spawner.clone(),
        ));
        runtime.start_session_store_sync(spawner.clone());
        let session_state = runtime.session_state();
        let disconnect_runtime = runtime.clone();
        let mut dispatcher = Dispatcher::new(spawner);
        dispatcher::register(&mut dispatcher, runtime);
        Self {
            dispatcher,
            session_state,
            disconnect: Arc::new(move || {
                let runtime = disconnect_runtime.clone();
                Box::pin(async move {
                    runtime.disconnect().await;
                })
            }),
        }
    }

    /// Handle to the shared session-state holder used by subscriptions and
    /// tests. Real host lifecycle flows through `SessionStore` and
    /// `disconnect`.
    pub fn session_state(&self) -> Arc<SessionState> {
        self.session_state.clone()
    }

    /// Core-owned logout/disconnect. Platform-backed cores best-effort notify
    /// the SSO peer and clear the host-global session store; direct cores only
    /// clear their in-memory session state.
    #[instrument(skip_all, fields(runtime.method = "core.disconnect"))]
    pub async fn disconnect_async(&self) {
        (self.disconnect)().await;
    }

    /// Blocking wrapper for embedders that do not drive async directly.
    #[instrument(skip_all, fields(runtime.method = "core.disconnect_blocking"))]
    pub fn disconnect(&self) {
        futures::executor::block_on(self.disconnect_async());
    }

    /// Asynchronous form of [`Self::receive_from_product`]. Decodes the
    /// incoming frame, runs it through the dispatcher, and returns the
    /// SCALE-encoded response (if any).
    #[instrument(skip_all, fields(runtime.method = "core.receive_from_product"))]
    pub async fn receive_from_product_async(&self, frame: &[u8]) -> Option<Vec<u8>> {
        let message = ProtocolMessage::decode(&mut &*frame).ok()?;
        let transport = Arc::new(ResponseTransport::default());
        self.dispatcher
            .dispatch(message, transport.clone() as Arc<dyn Transport>)
            .await;
        transport.take().map(|response| response.encode())
    }

    /// Synchronous wrapper that blocks the current thread until the inner
    /// future resolves. Convenient for embedding contexts (e.g. UniFFI) that
    /// don't already drive an async runtime.
    #[instrument(skip_all, fields(runtime.method = "core.receive_from_product_blocking"))]
    pub fn receive_from_product(&self, frame: &[u8]) -> Option<Vec<u8>> {
        futures::executor::block_on(self.receive_from_product_async(frame))
    }

    /// Dispatch an already-decoded protocol message through the underlying
    /// dispatcher. Bridges that own a long-lived transport (e.g. WebSocket,
    /// JS callback) call this directly so subscription items flow back
    /// through the bridge transport instead of the single-slot capture used
    /// by [`Self::receive_from_product`].
    #[instrument(skip_all, fields(runtime.method = "core.dispatch"))]
    pub async fn dispatch(&self, message: ProtocolMessage, transport: Arc<dyn Transport>) {
        self.dispatcher.dispatch(message, transport).await;
    }
}

/// Single-slot transport that captures the next response the dispatcher
/// emits. Used by [`TrUApiCore::receive_from_product`] to bridge between the
/// dispatcher's push model and the synchronous "decode in, encoded out"
/// shape exposed to embedders.
#[derive(Default)]
struct ResponseTransport {
    response: Mutex<Option<ProtocolMessage>>,
}

impl ResponseTransport {
    fn take(&self) -> Option<ProtocolMessage> {
        self.response.lock().unwrap().take()
    }
}

impl Transport for ResponseTransport {
    fn send(&self, message: ProtocolMessage) {
        *self.response.lock().unwrap() = Some(message);
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
    use futures::stream::{self, BoxStream};
    use parity_scale_codec::Encode;
    use truapi::v01;
    use truapi::versioned::local_storage::{
        HostLocalStorageClearRequest, HostLocalStorageReadRequest, HostLocalStorageWriteRequest,
    };
    use truapi::versioned::notifications::HostPushNotificationRequest;
    use truapi::versioned::permissions::RemotePermissionRequest;
    use truapi::versioned::system::{HostFeatureSupportedRequest, HostFeatureSupportedResponse};
    use truapi_platform::{
        ChainProvider, Features, JsonRpcConnection, Navigation, Notifications,
        PairingDeeplinkScheme, PairingPresenter, Permissions, PreimageHost, SessionStore, Storage,
        ThemeHost, UserConfirmation,
    };

    use crate::frame::{FrameKind, Payload, compose_action};

    fn test_spawner() -> crate::subscription::Spawner {
        #[cfg(not(target_arch = "wasm32"))]
        {
            crate::subscription::thread_per_subscription_spawner()
        }
        #[cfg(target_arch = "wasm32")]
        {
            Arc::new(futures::executor::block_on)
        }
    }

    fn test_runtime_config() -> RuntimeConfig {
        RuntimeConfig {
            product_label: "dotli".to_string(),
            product_id: "dotli.dot".to_string(),
            site_id: "dot.li".to_string(),
            host_name: "Polkadot Web".to_string(),
            host_icon: Some("https://dot.li/dotli.png".to_string()),
            host_version: None,
            platform_type: None,
            platform_version: None,
            people_chain_genesis_hash: [0xa2; 32],
            pairing_deeplink_scheme: PairingDeeplinkScheme::PolkadotApp,
        }
    }

    struct StubPlatform;

    impl Storage for StubPlatform {
        async fn read(
            &self,
            _key: String,
        ) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
            Ok(None)
        }
        async fn write(
            &self,
            _key: String,
            _value: Vec<u8>,
        ) -> Result<(), v01::HostLocalStorageReadError> {
            Ok(())
        }
        async fn clear(&self, _key: String) -> Result<(), v01::HostLocalStorageReadError> {
            Ok(())
        }
    }

    impl Navigation for StubPlatform {
        async fn navigate_to(&self, _url: String) -> Result<(), v01::HostNavigateToError> {
            Ok(())
        }
    }

    impl Notifications for StubPlatform {
        async fn push_notification(
            &self,
            _notification: v01::HostPushNotificationRequest,
        ) -> Result<v01::HostPushNotificationResponse, v01::GenericError> {
            Ok(v01::HostPushNotificationResponse { id: 0 })
        }

        async fn cancel_notification(&self, _id: u32) -> Result<(), v01::GenericError> {
            Ok(())
        }
    }

    impl Permissions for StubPlatform {
        async fn device_permission(
            &self,
            _request: v01::HostDevicePermissionRequest,
        ) -> Result<v01::HostDevicePermissionResponse, v01::GenericError> {
            Ok(v01::HostDevicePermissionResponse { granted: true })
        }
        async fn remote_permission(
            &self,
            _request: v01::RemotePermissionRequest,
        ) -> Result<v01::RemotePermissionResponse, v01::GenericError> {
            Ok(v01::RemotePermissionResponse { granted: true })
        }
    }

    impl Features for StubPlatform {
        async fn feature_supported(
            &self,
            request: HostFeatureSupportedRequest,
        ) -> Result<HostFeatureSupportedResponse, v01::GenericError> {
            let HostFeatureSupportedRequest::V1(_) = request;
            Ok(HostFeatureSupportedResponse::V1(
                v01::HostFeatureSupportedResponse { supported: true },
            ))
        }
    }

    struct DeadConnection;
    impl JsonRpcConnection for DeadConnection {
        fn send(&self, _request: String) {}
        fn responses(&self) -> BoxStream<'static, String> {
            Box::pin(stream::empty())
        }
    }

    impl ChainProvider for StubPlatform {
        async fn connect(
            &self,
            _genesis_hash: Vec<u8>,
        ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
            Ok(Box::new(DeadConnection))
        }
    }

    impl PairingPresenter for StubPlatform {
        async fn present_pairing(&self, _deeplink: String) -> Result<(), v01::GenericError> {
            Err(v01::GenericError {
                reason: "pairing presenter callback not provided by host".to_string(),
            })
        }
    }

    impl SessionStore for StubPlatform {
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
            Box::pin(stream::once(async { Ok(()) }))
        }
    }

    impl UserConfirmation for StubPlatform {
        async fn confirm_sign_payload(&self, _review: Vec<u8>) -> Result<bool, v01::GenericError> {
            Ok(false)
        }
        async fn confirm_sign_raw(&self, _review: Vec<u8>) -> Result<bool, v01::GenericError> {
            Ok(false)
        }
        async fn confirm_create_transaction(
            &self,
            _review: Vec<u8>,
        ) -> Result<bool, v01::GenericError> {
            Ok(false)
        }
        async fn confirm_account_alias(&self, _review: Vec<u8>) -> Result<bool, v01::GenericError> {
            Ok(false)
        }
        async fn confirm_resource_allocation(
            &self,
            _review: Vec<u8>,
        ) -> Result<bool, v01::GenericError> {
            Ok(false)
        }
    }

    impl ThemeHost for StubPlatform {
        fn subscribe_theme(
            &self,
        ) -> BoxStream<'static, Result<v01::ThemeVariant, v01::GenericError>> {
            Box::pin(stream::empty())
        }
    }

    impl PreimageHost for StubPlatform {
        async fn confirm_preimage_submit(
            &self,
            _size: u64,
        ) -> Result<(), v01::PreimageSubmitError> {
            Ok(())
        }
        async fn submit_preimage(
            &self,
            value: Vec<u8>,
        ) -> Result<Vec<u8>, v01::PreimageSubmitError> {
            Ok(value)
        }
        fn lookup_preimage(
            &self,
            _key: Vec<u8>,
        ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
            Box::pin(stream::empty())
        }
    }

    #[test]
    fn from_platform_dispatches_feature_supported() {
        let core = TrUApiCore::from_platform_with_config(
            Arc::new(StubPlatform),
            test_runtime_config(),
            test_spawner(),
        );
        let request = HostFeatureSupportedRequest::V1(v01::HostFeatureSupportedRequest::Chain {
            genesis_hash: vec![0u8; 32],
        });
        let frame = ProtocolMessage {
            request_id: "p:1".into(),
            payload: Payload {
                tag: compose_action("system_feature_supported", FrameKind::Request),
                value: request.encode(),
            },
        };
        let encoded = frame.encode();
        let response_bytes = core
            .receive_from_product(&encoded)
            .expect("dispatcher should emit a response");
        let response = ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response");
        assert_eq!(response.request_id, "p:1");
        assert_eq!(
            response.payload.tag,
            compose_action("system_feature_supported", FrameKind::Response),
        );
        // Wire payload is `Result<Ok, Err>`-shaped:
        // [Ok disc=0x00][V1 variant 0x00][supported=1]
        assert_eq!(response.payload.value, vec![0x00, 0x00, 0x01]);
    }

    /// Drive a request frame through `TrUApiCore::receive_from_product`,
    /// decode the response envelope, and return its payload bytes (without
    /// the wrapping ProtocolMessage). Shared by the runtime-delegation
    /// tests below.
    fn run_request(core: &TrUApiCore, method: &str, request_bytes: Vec<u8>) -> Vec<u8> {
        let frame = ProtocolMessage {
            request_id: "p:1".into(),
            payload: Payload {
                tag: compose_action(method, FrameKind::Request),
                value: request_bytes,
            },
        };
        let response_bytes = core
            .receive_from_product(&frame.encode())
            .expect("dispatcher should emit a response");
        let response = ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response");
        assert_eq!(response.request_id, "p:1");
        assert_eq!(
            response.payload.tag,
            compose_action(method, FrameKind::Response),
        );
        response.payload.value
    }

    fn make_core() -> TrUApiCore {
        TrUApiCore::from_platform_with_config(
            Arc::new(StubPlatform),
            test_runtime_config(),
            test_spawner(),
        )
    }

    #[test]
    fn local_storage_read_round_trips_none() {
        let core = make_core();
        let request = HostLocalStorageReadRequest::V1(v01::HostLocalStorageReadRequest {
            key: "missing".into(),
        });
        let payload = run_request(&core, "local_storage_read", request.encode());
        // Ok disc 0x00, V1 variant 0x00, Option::None = 0x00.
        assert_eq!(payload, vec![0x00, 0x00, 0x00]);
    }

    #[test]
    fn local_storage_write_round_trips_unit_ok() {
        let core = make_core();
        let request = HostLocalStorageWriteRequest::V1(v01::HostLocalStorageWriteRequest {
            key: "k".into(),
            value: vec![1, 2, 3],
        });
        let payload = run_request(&core, "local_storage_write", request.encode());
        // Ok disc 0x00, V1 variant 0x00.
        assert_eq!(payload, vec![0x00, 0x00]);
    }

    #[test]
    fn local_storage_clear_round_trips_unit_ok() {
        let core = make_core();
        let request =
            HostLocalStorageClearRequest::V1(v01::HostLocalStorageClearRequest { key: "k".into() });
        let payload = run_request(&core, "local_storage_clear", request.encode());
        // Ok disc 0x00, V1 variant 0x00.
        assert_eq!(payload, vec![0x00, 0x00]);
    }

    #[test]
    fn send_push_notification_delegates_to_platform() {
        let core = make_core();
        let request = HostPushNotificationRequest::V1(v01::HostPushNotificationRequest {
            text: "hi".into(),
            deeplink: None,
            scheduled_at: None,
        });
        let payload = run_request(
            &core,
            "notifications_send_push_notification",
            request.encode(),
        );
        // Ok disc 0x00, V1 variant 0x00, notification id 0.
        let mut expected = vec![0x00u8];
        truapi::versioned::notifications::HostPushNotificationResponse::V1(
            v01::HostPushNotificationResponse { id: 0 },
        )
        .encode_to(&mut expected);
        assert_eq!(payload, expected);
    }

    #[test]
    fn request_remote_permission_round_trips_granted() {
        let core = make_core();
        let request = RemotePermissionRequest::V1(v01::RemotePermissionRequest {
            permission: v01::RemotePermission::ChainSubmit,
        });
        let payload = run_request(
            &core,
            "permissions_request_remote_permission",
            request.encode(),
        );
        // Stub permissions grants every request. Wire is Ok disc 0x00, V1
        // variant 0x00, granted=1.
        assert_eq!(payload, vec![0x00, 0x00, 0x01]);
    }

    /// `connection_status_subscribe` produces a stream whose first item is
    /// the current session state. Drive it through the dispatcher with a
    /// recording transport and assert exactly one `_receive` frame appears.
    #[test]
    fn connection_status_subscribe_yields_initial_disconnected() {
        use std::sync::Mutex;

        #[derive(Default)]
        struct RecordingTransport {
            sent: Mutex<Vec<ProtocolMessage>>,
        }
        impl Transport for RecordingTransport {
            fn send(&self, message: ProtocolMessage) {
                self.sent.lock().unwrap().push(message);
            }
            fn on_message(
                &self,
                _handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
            ) -> Box<dyn FnOnce()> {
                Box::new(|| {})
            }
        }

        let core = make_core();
        let transport = Arc::new(RecordingTransport::default());
        let dyn_transport: Arc<dyn Transport> = transport.clone();

        let frame = ProtocolMessage {
            request_id: "p:1".into(),
            payload: Payload {
                tag: compose_action("account_connection_status_subscribe", FrameKind::Start),
                value: Vec::new(),
            },
        };
        futures::executor::block_on(core.dispatch(frame, dyn_transport));

        // Wait briefly for the spawned thread to emit the initial item.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if !transport.sent.lock().unwrap().is_empty() {
                break;
            }
            if std::time::Instant::now() > deadline {
                panic!("subscription did not yield an item in time");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let sent = transport.sent.lock().unwrap().clone();
        assert!(!sent.is_empty(), "expected at least one _receive frame");
        let first = &sent[0];
        assert_eq!(
            first.payload.tag,
            compose_action("account_connection_status_subscribe", FrameKind::Receive,),
        );
        // V1(Disconnected): V1 variant 0x00, Disconnected discriminant 0x00.
        assert_eq!(first.payload.value, vec![0x00, 0x00]);
    }
}
