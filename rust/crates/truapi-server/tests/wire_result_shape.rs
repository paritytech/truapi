//! Result-wire-shape regression test.
//!
//! The TS host/client codec expects every request response to be
//! `Result<Ok, Err>`-shaped on the wire (one leading discriminant byte
//! followed by the SCALE-encoded value). This test stands up a
//! `TrUApiCore::from_platform` with a `StubPlatform` whose `Features`
//! impl returns `Ok(supported = true)` and asserts:
//!
//! - A `system_feature_supported_request` produces a response whose
//!   payload begins with `0x00` (Ok), followed by the encoded
//!   `HostFeatureSupportedResponse::V1(true)`.
//! - A `local_storage_read_request` whose stub returns
//!   `Err(HostLocalStorageReadError::Full)` produces a response whose
//!   payload begins with `0x01` (Err), followed by the encoded
//!   `CallError::Domain(Full)`.
//!
//! Both halves prove the wire layout stays in lockstep with the TS
//! `S.Result(ok, err)` codec.

use std::sync::Arc;

use futures::stream::{self, BoxStream};
use parity_scale_codec::{Decode, Encode};

use truapi::v01;
use truapi::versioned::system::{HostFeatureSupportedRequest, HostFeatureSupportedResponse};
use truapi::versioned::{account, payment};

use truapi_platform::{
    ChainProvider, Features, JsonRpcConnection, Navigation, Notifications, PairingPresenter,
    Permissions, PreimageHost, SessionStore, Storage, ThemeHost, UserConfirmation,
};

use truapi_server::{FrameKind, Payload, ProtocolMessage, TrUApiCore, compose_action};

struct StubPlatform;

impl Storage for StubPlatform {
    async fn read(&self, _key: String) -> Result<Option<Vec<u8>>, v01::HostLocalStorageReadError> {
        // Drive the error-path test: return `Full` so we can assert the
        // wire-Err discriminant precedes the SCALE-encoded `CallError::Domain(Full)`.
        Err(v01::HostLocalStorageReadError::Full)
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
        _request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, v01::GenericError> {
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
    fn subscribe_theme(&self) -> BoxStream<'static, Result<v01::Theme, v01::GenericError>> {
        Box::pin(stream::empty())
    }
}

impl PreimageHost for StubPlatform {
    async fn confirm_preimage_submit(&self, _size: u64) -> Result<(), v01::PreimageSubmitError> {
        Ok(())
    }
    async fn submit_preimage(&self, value: Vec<u8>) -> Result<Vec<u8>, v01::PreimageSubmitError> {
        Ok(value)
    }
    fn lookup_preimage(
        &self,
        _key: Vec<u8>,
    ) -> BoxStream<'static, Result<Option<Vec<u8>>, v01::GenericError>> {
        Box::pin(stream::empty())
    }
}

fn dispatch(core: &TrUApiCore, frame: ProtocolMessage) -> ProtocolMessage {
    let encoded = frame.encode();
    let response_bytes = core
        .receive_from_product(&encoded)
        .expect("dispatcher emitted a response frame");
    ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response")
}

fn test_spawner() -> truapi_server::subscription::Spawner {
    #[cfg(not(target_arch = "wasm32"))]
    {
        truapi_server::subscription::thread_per_subscription_spawner()
    }
    #[cfg(target_arch = "wasm32")]
    {
        Arc::new(futures::executor::block_on)
    }
}

#[test]
fn feature_supported_ok_response_uses_ok_discriminant() {
    let core = TrUApiCore::from_platform(Arc::new(StubPlatform), test_spawner());
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
    let response = dispatch(&core, frame);
    assert_eq!(response.request_id, "p:1");
    assert_eq!(
        response.payload.tag,
        compose_action("system_feature_supported", FrameKind::Response),
    );

    // Wire payload: [Ok disc=0x00][encoded versioned response]
    let mut expected = vec![0x00u8];
    HostFeatureSupportedResponse::V1(v01::HostFeatureSupportedResponse { supported: true })
        .encode_to(&mut expected);
    assert_eq!(response.payload.value, expected);
    // The Result-disc byte is unambiguously 0x00 for Ok.
    assert_eq!(response.payload.value.first(), Some(&0x00));
}

#[test]
fn local_storage_read_err_response_uses_err_discriminant() {
    let core = TrUApiCore::from_platform(Arc::new(StubPlatform), test_spawner());
    let request = truapi::versioned::local_storage::HostLocalStorageReadRequest::V1(
        v01::HostLocalStorageReadRequest {
            key: "missing".to_string(),
        },
    );
    let frame = ProtocolMessage {
        request_id: "p:2".into(),
        payload: Payload {
            tag: compose_action("local_storage_read", FrameKind::Request),
            value: request.encode(),
        },
    };
    let response = dispatch(&core, frame);
    assert_eq!(response.request_id, "p:2");
    assert_eq!(
        response.payload.tag,
        compose_action("local_storage_read", FrameKind::Response),
    );

    // Wire payload: `[Err disc=0x01][CallError::Domain variant=0x00][encoded
    // domain error]`. Build the expected bytes from the typed value the runtime
    // wraps (the stub returns `Full`) rather than a hand-written literal, so a
    // reorder of any domain enum variant is caught instead of silently passing.
    let domain = truapi::versioned::local_storage::HostLocalStorageReadError::V1(
        v01::HostLocalStorageReadError::Full,
    );
    let mut expected = vec![0x01u8, 0x00u8];
    domain.encode_to(&mut expected);
    assert_eq!(response.payload.value, expected);
    assert_eq!(response.payload.value.first(), Some(&0x01));
}

fn assert_request_returns_unsupported(
    core: &TrUApiCore,
    request_id: &str,
    method: &str,
    value: Vec<u8>,
) {
    let response = dispatch(
        core,
        ProtocolMessage {
            request_id: request_id.into(),
            payload: Payload {
                tag: compose_action(method, FrameKind::Request),
                value,
            },
        },
    );
    assert_eq!(response.request_id, request_id);
    assert_eq!(
        response.payload.tag,
        compose_action(method, FrameKind::Response),
    );
    assert_eq!(
        response.payload.value,
        vec![0x01, 0x02],
        "{method} must remain explicitly unavailable for current dotli parity"
    );
}

fn assert_subscription_start_interrupts_unsupported(
    core: &TrUApiCore,
    request_id: &str,
    method: &str,
    value: Vec<u8>,
) {
    use std::sync::Mutex;
    use truapi_server::Transport;

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

    let transport = Arc::new(RecordingTransport::default());
    futures::executor::block_on(core.dispatch(
        ProtocolMessage {
            request_id: request_id.into(),
            payload: Payload {
                tag: compose_action(method, FrameKind::Start),
                value,
            },
        },
        transport.clone(),
    ));

    let sent = transport.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].request_id, request_id);
    assert_eq!(
        sent[0].payload.tag,
        compose_action(method, FrameKind::Interrupt),
    );
    assert_eq!(
        sent[0].payload.value,
        vec![0x02],
        "{method} subscription must remain explicitly unavailable for current dotli parity"
    );
}

#[test]
fn deferred_account_proof_returns_unsupported() {
    let core = make_core();
    let request = account::HostAccountCreateProofRequest::V1(v01::HostAccountCreateProofRequest {
        product_account_id: v01::ProductAccountId {
            dot_ns_identifier: "myapp.dot".to_string(),
            derivation_index: 0,
        },
        ring_location: v01::RingLocation {
            genesis_hash: vec![0u8; 32],
            ring_root_hash: vec![1u8; 32],
            hints: None,
        },
        context: Vec::new(),
    });

    assert_request_returns_unsupported(
        &core,
        "p:account-proof",
        "account_create_account_proof",
        request.encode(),
    );
}

#[test]
fn deferred_payment_requests_return_unsupported() {
    let core = make_core();
    let request = payment::HostPaymentRequest::V1(v01::HostPaymentRequest {
        from: None,
        amount: 1,
        destination: [0u8; 32],
    });

    assert_request_returns_unsupported(&core, "p:payment", "payment_request", request.encode());

    let top_up = payment::HostPaymentTopUpRequest::V1(v01::HostPaymentTopUpRequest {
        into: None,
        amount: 1,
        source: v01::PaymentTopUpSource::ProductAccount {
            derivation_index: 0,
        },
    });
    assert_request_returns_unsupported(&core, "p:top-up", "payment_top_up", top_up.encode());
}

#[test]
fn deferred_payment_subscriptions_interrupt_unsupported() {
    let core = make_core();
    let balance =
        payment::HostPaymentBalanceSubscribeRequest::V1(v01::HostPaymentBalanceSubscribeRequest {
            purse: None,
        });
    assert_subscription_start_interrupts_unsupported(
        &core,
        "p:balance",
        "payment_balance_subscribe",
        balance.encode(),
    );

    let status =
        payment::HostPaymentStatusSubscribeRequest::V1(v01::HostPaymentStatusSubscribeRequest {
            payment_id: "payment-id".to_string(),
        });
    assert_subscription_start_interrupts_unsupported(
        &core,
        "p:status",
        "payment_status_subscribe",
        status.encode(),
    );
}

fn make_core() -> TrUApiCore {
    TrUApiCore::from_platform(Arc::new(StubPlatform), test_spawner())
}

/// Untrusted product input that is not a decodable frame must be dropped
/// (return `None`), never panic. Exercises the decode-failure boundary in
/// `receive_from_product` that the happy-path tests above bypass.
#[test]
fn malformed_frames_are_dropped_without_panic() {
    let core = make_core();

    // Empty input and arbitrary garbage.
    assert!(core.receive_from_product(&[]).is_none());
    assert!(
        core.receive_from_product(&[0xff, 0xff, 0xff, 0xff])
            .is_none()
    );

    // A truncated SCALE string header (claims length but no body).
    assert!(
        core.receive_from_product(&[200u8 << 2, 0x61, 0x62])
            .is_none()
    );

    // A well-formed requestId envelope carrying an unknown wire discriminant.
    let mut unknown_disc = Vec::new();
    "p:1".to_string().encode_to(&mut unknown_disc);
    unknown_disc.push(0xFA);
    unknown_disc.extend_from_slice(&[0u8; 4]);
    assert!(core.receive_from_product(&unknown_disc).is_none());
}

/// Drive a subscription through the encoded-frame boundary: `_start` yields
/// the initial `_receive`, then `_stop` tears it down so a later session
/// change produces no further frames. Covers the wire layer the in-crate
/// `subscription.rs` unit tests bypass.
#[test]
fn subscription_start_receive_stop_through_wire_boundary() {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    use truapi_server::Transport;

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

    let method = "account_connection_status_subscribe";
    let start = ProtocolMessage {
        request_id: "p:1".into(),
        payload: Payload {
            tag: compose_action(method, FrameKind::Start),
            value: Vec::new(),
        },
    };
    futures::executor::block_on(core.dispatch(start, dyn_transport.clone()));

    // Wait for the initial `_receive` item (Disconnected).
    let deadline = Instant::now() + Duration::from_secs(2);
    while transport.sent.lock().unwrap().is_empty() {
        assert!(Instant::now() < deadline, "no initial _receive frame");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        transport.sent.lock().unwrap()[0].payload.tag,
        compose_action(method, FrameKind::Receive),
    );

    // Stop the subscription, then push a session change. A live subscription
    // would emit a Connected `_receive`; a stopped one must stay silent.
    let stop = ProtocolMessage {
        request_id: "p:1".into(),
        payload: Payload {
            tag: compose_action(method, FrameKind::Stop),
            value: Vec::new(),
        },
    };
    futures::executor::block_on(core.dispatch(stop, dyn_transport));
    std::thread::sleep(Duration::from_millis(50));

    core.session_state()
        .set_session(truapi_server::host_logic::session::SessionInfo {
            public_key: [7u8; 32],
            sso: None,
            entropy_secret: None,
            lite_username: None,
            full_username: None,
        });
    std::thread::sleep(Duration::from_millis(50));

    assert_eq!(
        transport.sent.lock().unwrap().len(),
        1,
        "stopped subscription must emit no further frames"
    );
}
