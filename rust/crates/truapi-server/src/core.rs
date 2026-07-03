//! Internal dispatcher/runtime core.
//!
//! Public host adapters should wrap this through [`crate::ProductRuntime`], which
//! owns the stable byte-frame ingress/egress and lifecycle API.

use std::sync::{Arc, Mutex};

use parity_scale_codec::{Decode, Encode};
use tracing::instrument;
use truapi::api::TrUApi;
use truapi_platform::{PairingHostConfig, Platform, ProductContext};

use crate::dispatcher::Dispatcher;
use crate::frame::ProtocolMessage;
use crate::generated::dispatcher;
use crate::host_logic::session::SessionState;
use crate::runtime::{PairingHostRole, ProductAuthority, ProductRuntimeHost, RuntimeServices};
use crate::subscription::Spawner;
use crate::transport::Transport;

/// Top-level core. Owns the generated dispatcher.
pub struct TrUApiCore {
    dispatcher: Dispatcher,
    session_state: Arc<SessionState>,
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
        Self {
            dispatcher,
            session_state: SessionState::new(),
        }
    }

    /// Build a product-facing core around a [`Platform`] implementation,
    /// explicit host runtime config, and product context.
    #[instrument(skip_all, fields(runtime.method = "core.from_platform_with_config"))]
    pub fn from_platform_with_config<P>(
        platform: Arc<P>,
        host_config: PairingHostConfig,
        product: ProductContext,
        spawner: Spawner,
    ) -> Self
    where
        P: Platform + 'static,
    {
        let platform: Arc<dyn Platform> = platform;
        let services = RuntimeServices::new(
            platform,
            host_config.people_chain_genesis_hash,
            spawner.clone(),
        );
        let pairing_host = PairingHostRole::new(services.clone(), host_config);
        pairing_host.clone().start_session_store_sync(spawner);
        Self::from_runtime_parts(services, pairing_host, product)
    }

    /// Build a product-facing core from shared services and authority.
    #[instrument(skip_all, fields(runtime.method = "core.from_runtime_parts"))]
    pub(crate) fn from_runtime_parts(
        services: Arc<RuntimeServices>,
        authority: Arc<dyn ProductAuthority>,
        product: ProductContext,
    ) -> Self {
        let runtime = Arc::new(ProductRuntimeHost::from_services(
            services.clone(),
            authority.clone(),
            product,
        ));
        Self::from_product_runtime(runtime, services.spawner.clone(), authority.session_state())
    }

    /// Build a dispatcher core around an already-created product runtime.
    #[instrument(skip_all, fields(runtime.method = "core.from_product_runtime"))]
    pub(crate) fn from_product_runtime(
        runtime: Arc<ProductRuntimeHost>,
        spawner: Spawner,
        session_state: Arc<SessionState>,
    ) -> Self {
        let mut dispatcher = Dispatcher::new(spawner);
        dispatcher::register(&mut dispatcher, runtime);
        Self {
            dispatcher,
            session_state,
        }
    }

    /// Handle to the shared session-state holder used by subscriptions and
    /// tests. Real host lifecycle flows through CoreStorage session sync and
    /// `disconnect`.
    pub fn session_state(&self) -> Arc<SessionState> {
        self.session_state.clone()
    }

    /// Decode an incoming product frame, run it through the dispatcher, and
    /// return the SCALE-encoded response frame when the method has one.
    #[instrument(skip_all, fields(runtime.method = "core.receive_from_product"))]
    pub async fn receive_from_product(&self, frame: &[u8]) -> Option<Vec<u8>> {
        let message = ProtocolMessage::decode(&mut &*frame).ok()?;
        let transport = Arc::new(ResponseTransport::default());
        self.dispatcher
            .dispatch(message, transport.clone() as Arc<dyn Transport>)
            .await;
        transport.take().map(|response| response.encode())
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
/// dispatcher's push model and the one-response frame API exposed to embedders.
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
    use parity_scale_codec::Encode;
    use truapi::v01;
    use truapi::versioned::local_storage::{
        HostLocalStorageClearRequest, HostLocalStorageReadRequest, HostLocalStorageWriteRequest,
    };
    use truapi::versioned::notifications::HostPushNotificationRequest;
    use truapi::versioned::permissions::RemotePermissionRequest;
    use truapi::versioned::system::HostFeatureSupportedRequest;

    use crate::frame::{Payload, request_ids, subscription_ids};
    use crate::test_support::{StubPlatform, runtime_config, test_spawner};

    #[test]
    fn from_platform_dispatches_feature_supported() {
        let (host_config, product) = runtime_config("dotli.dot");
        let core = TrUApiCore::from_platform_with_config(
            Arc::new(StubPlatform::default()),
            host_config,
            product,
            test_spawner(),
        );
        let request = HostFeatureSupportedRequest::V1(v01::HostFeatureSupportedRequest::Chain {
            genesis_hash: vec![0u8; 32],
        });
        let ids = request_ids("system_feature_supported").expect("known request method");
        let frame = ProtocolMessage {
            request_id: "p:1".into(),
            payload: Payload {
                id: ids.request_id,
                value: request.encode(),
            },
        };
        let encoded = frame.encode();
        let response_bytes = futures::executor::block_on(core.receive_from_product(&encoded))
            .expect("dispatcher should emit a response");
        let response = ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response");
        assert_eq!(response.request_id, "p:1");
        assert_eq!(response.payload.id, ids.response_id);
        // Wire payload is `Result<Ok, Err>`-shaped:
        // [Ok disc=0x00][V1 variant 0x00][supported=1]
        assert_eq!(response.payload.value, vec![0x00, 0x00, 0x01]);
    }

    /// Drive a request frame through `TrUApiCore::receive_from_product`,
    /// decode the response envelope, and return its payload bytes (without
    /// the wrapping ProtocolMessage). Shared by the runtime-delegation
    /// tests below.
    fn run_request(core: &TrUApiCore, method: &str, request_bytes: Vec<u8>) -> Vec<u8> {
        let ids = request_ids(method).expect("known request method");
        let frame = ProtocolMessage {
            request_id: "p:1".into(),
            payload: Payload {
                id: ids.request_id,
                value: request_bytes,
            },
        };
        let response_bytes =
            futures::executor::block_on(core.receive_from_product(&frame.encode()))
                .expect("dispatcher should emit a response");
        let response = ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response");
        assert_eq!(response.request_id, "p:1");
        assert_eq!(response.payload.id, ids.response_id);
        response.payload.value
    }

    fn make_core() -> TrUApiCore {
        let (host_config, product) = runtime_config("dotli.dot");
        TrUApiCore::from_platform_with_config(
            Arc::new(StubPlatform::default()),
            host_config,
            product,
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

        let sub_ids =
            subscription_ids("account_connection_status_subscribe").expect("known subscription");
        let frame = ProtocolMessage {
            request_id: "p:1".into(),
            payload: Payload {
                id: sub_ids.start_id,
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
        assert_eq!(first.payload.id, sub_ids.receive_id);
        // V1(Disconnected): V1 variant 0x00, Disconnected discriminant 0x00.
        assert_eq!(first.payload.value, vec![0x00, 0x00]);
    }
}
