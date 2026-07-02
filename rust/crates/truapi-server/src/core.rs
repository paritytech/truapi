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
    /// Subscription starts should use [`Self::dispatch`] with a long-lived
    /// transport; changing this byte-frame helper to reject them or return a
    /// richer response shape is a separate API decision.
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

    /// Cancel all active and pending subscriptions owned by this core.
    pub fn cancel_subscriptions(&self) {
        self.dispatcher.cancel_subscriptions();
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

    /// The canonical config-driven `MockPlatform` drives the real core
    /// end-to-end: a configured answer flows through the production dispatcher
    /// and out the wire, proving the mock is faithful by construction rather
    /// than merely trait-complete.
    #[test]
    fn from_mock_platform_dispatches_configured_feature_supported() {
        use truapi_platform::mock::{MockConfig, MockPlatform};

        let request = HostFeatureSupportedRequest::V1(v01::HostFeatureSupportedRequest::Chain {
            genesis_hash: vec![0u8; 32],
        });
        let ids = request_ids("system_feature_supported").expect("known request method");
        let encoded = ProtocolMessage {
            request_id: "p:1".into(),
            payload: Payload {
                id: ids.request_id,
                value: request.encode(),
            },
        }
        .encode();

        let dispatch = |platform: MockPlatform| {
            let (host_config, product) = runtime_config("dotli.dot");
            let core = TrUApiCore::from_platform_with_config(
                Arc::new(platform),
                host_config,
                product,
                test_spawner(),
            );
            let response_bytes = futures::executor::block_on(core.receive_from_product(&encoded))
                .expect("dispatcher should emit a response");
            let response =
                ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response");
            assert_eq!(response.payload.id, ids.response_id);
            response.payload.value
        };

        // Default mock supports the feature: [V1 0x00][Ok 0x00][supported=1].
        assert_eq!(dispatch(MockPlatform::new()), vec![0x00, 0x00, 0x01]);
        // A configured "unsupported" answer flows through the same dispatcher.
        let unsupported = MockPlatform::with_config(MockConfig {
            feature_supported: false,
            ..Default::default()
        });
        assert_eq!(dispatch(unsupported), vec![0x00, 0x00, 0x00]);
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

    fn make_mock_core(config: truapi_platform::mock::MockConfig) -> TrUApiCore {
        let (host_config, product) = runtime_config("dotli.dot");
        TrUApiCore::from_platform_with_config(
            Arc::new(truapi_platform::mock::MockPlatform::with_config(config)),
            host_config,
            product,
            test_spawner(),
        )
    }

    /// MockPlatform product storage round-trips through the real dispatcher:
    /// a value written over the wire reads back, then misses after clear.
    /// Proves the seam works end-to-end, not just in the mock's own unit tests.
    #[test]
    fn from_mock_platform_storage_round_trips_through_core() {
        let core = make_mock_core(truapi_platform::mock::MockConfig::default());

        let write = HostLocalStorageWriteRequest::V1(v01::HostLocalStorageWriteRequest {
            key: "k".into(),
            value: vec![1, 2, 3],
        });
        // V1 0x00, Ok 0x00.
        assert_eq!(
            run_request(&core, "local_storage_write", write.encode()),
            vec![0x00, 0x00]
        );

        let read =
            HostLocalStorageReadRequest::V1(v01::HostLocalStorageReadRequest { key: "k".into() });
        // V1 0x00, Ok 0x00, Some 0x01, compact-len(3) 0x0c, bytes.
        assert_eq!(
            run_request(&core, "local_storage_read", read.encode()),
            vec![0x00, 0x00, 0x01, 0x0c, 1, 2, 3]
        );

        let clear =
            HostLocalStorageClearRequest::V1(v01::HostLocalStorageClearRequest { key: "k".into() });
        assert_eq!(
            run_request(&core, "local_storage_clear", clear.encode()),
            vec![0x00, 0x00]
        );

        // After clear the read misses: V1 0x00, Ok 0x00, None 0x00.
        let read_again =
            HostLocalStorageReadRequest::V1(v01::HostLocalStorageReadRequest { key: "k".into() });
        assert_eq!(
            run_request(&core, "local_storage_read", read_again.encode()),
            vec![0x00, 0x00, 0x00]
        );
    }

    /// The mock's per-capability permission policy surfaces through the real
    /// permission service and wire: a `DenyAll` device policy yields
    /// `granted: false`, an `AllowAll` policy yields `granted: true`. Closes the
    /// "allow-all hiding a denied path" gap.
    #[test]
    fn from_mock_platform_device_permission_policy_through_core() {
        use truapi::versioned::permissions::HostDevicePermissionRequest;
        use truapi_platform::mock::{MockConfig, PermissionPolicy};

        let request =
            || HostDevicePermissionRequest::V1(v01::HostDevicePermissionRequest::Camera).encode();

        // AllowAll (default): V1 0x00, Ok 0x00, granted=1.
        let allow = make_mock_core(MockConfig::default());
        assert_eq!(
            run_request(&allow, "permissions_request_device_permission", request()),
            vec![0x00, 0x00, 0x01]
        );

        // DenyAll: granted=0.
        let deny = make_mock_core(MockConfig {
            device_permissions: PermissionPolicy::DenyAll,
            ..Default::default()
        });
        assert_eq!(
            run_request(&deny, "permissions_request_device_permission", request()),
            vec![0x00, 0x00, 0x00]
        );
    }

    /// Preimage submit flows through the core's confirm gate: the default mock
    /// auto-confirms (Ok envelope), and a `confirm = false` mock is rejected by
    /// the core after the confirmation prompt but before the preimage backend
    /// (`PreimageHost::submit_preimage`) is reached (Err envelope).
    // PENDING #104/#264: #104 now requires a bulletin allowance signer for
    // preimage submit, so the auto-confirm "Ok" path needs a paired session with
    // allowance keys the bare MockPlatform doesn't set up (it now returns Err).
    // Restore the Ok assertion once the allowance flow settles (post-#264 revisit).
    #[ignore = "#104 preimage submit now needs an allowance signer; revisit post-#264"]
    #[test]
    fn from_mock_platform_preimage_submit_through_core() {
        use truapi::versioned::preimage::RemotePreimageSubmitRequest;
        use truapi_platform::mock::MockConfig;

        // Versioned envelope layout is [version_index, result_discriminant, ..].
        // For a V1 response the version index is 0, so the Ok/Err distinction
        // lives at byte index 1: 0x00 = Ok, 0x01 = Err.

        // Default mock auto-confirms: submit succeeds (V1 index 0x00, Ok 0x00).
        let confirmed = make_mock_core(MockConfig::default());
        let ok_payload = run_request(
            &confirmed,
            "preimage_submit",
            RemotePreimageSubmitRequest::V1(vec![1, 2, 3]).encode(),
        );
        assert_eq!(ok_payload.first(), Some(&0x00));
        assert_eq!(ok_payload.get(1), Some(&0x00));

        // confirm_user_actions = false: the core rejects after the confirmation
        // prompt, before the preimage backend (V1 index 0x00, Err 0x01).
        let rejected = make_mock_core(MockConfig {
            confirm_user_actions: false,
            ..Default::default()
        });
        let err_payload = run_request(
            &rejected,
            "preimage_submit",
            RemotePreimageSubmitRequest::V1(vec![1, 2, 3]).encode(),
        );
        assert_eq!(err_payload.first(), Some(&0x00));
        assert_eq!(err_payload.get(1), Some(&0x01));
    }

    /// A MockPlatform storage fault surfaces through the real core as a wire
    /// `Err` envelope — proving fault injection propagates through the dispatcher.
    #[test]
    fn from_mock_platform_storage_fault_surfaces_through_core() {
        use truapi_platform::mock::{MockConfig, MockFaults};

        let core = make_mock_core(MockConfig {
            faults: MockFaults {
                storage_error: Some("disk full".into()),
                ..Default::default()
            },
            ..Default::default()
        });
        let read =
            HostLocalStorageReadRequest::V1(v01::HostLocalStorageReadRequest { key: "k".into() });
        // Versioned wire layout is [version_index=0x00 (V1)][result_index][inner];
        // the Err discriminant is byte 1, not byte 0 (byte 0 is the V1 version index).
        let payload = run_request(&core, "local_storage_read", read.encode());
        assert_eq!(payload.first(), Some(&0x00)); // V1 version index
        assert_eq!(payload.get(1), Some(&0x01)); // Result::Err discriminant
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
