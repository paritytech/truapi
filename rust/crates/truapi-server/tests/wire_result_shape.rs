//! Result-wire-shape regression test.
//!
//! The TS host/client codec expects every request response to be
//! `Result<Ok, Err>`-shaped on the wire (one leading discriminant byte
//! followed by the SCALE-encoded value). This test stands up a
//! `TrUApiCore::from_platform_with_config` with a platform whose `Features`
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

use parity_scale_codec::{Decode, Encode};

use truapi::v01;
use truapi::versioned::system::{HostFeatureSupportedRequest, HostFeatureSupportedResponse};
use truapi::versioned::{account, statement_store};

use truapi_server::{
    Payload, ProtocolMessage, TrUApiCore, encode_call_error_payload, request_ids, subscription_ids,
};

mod common;
use common::{WireShapePlatform, test_runtime_config, test_spawner};

fn dispatch(core: &TrUApiCore, frame: ProtocolMessage) -> ProtocolMessage {
    let encoded = frame.encode();
    let response_bytes = core
        .receive_from_product(&encoded)
        .expect("dispatcher emitted a response frame");
    ProtocolMessage::decode(&mut &response_bytes[..]).expect("decode response")
}

#[test]
fn feature_supported_ok_response_uses_ok_discriminant() {
    let core = make_core();
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
    let response = dispatch(&core, frame);
    assert_eq!(response.request_id, "p:1");
    assert_eq!(response.payload.id, ids.response_id);

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
    let core = make_core();
    let request = truapi::versioned::local_storage::HostLocalStorageReadRequest::V1(
        v01::HostLocalStorageReadRequest {
            key: "missing".to_string(),
        },
    );
    let ids = request_ids("local_storage_read").expect("known request method");
    let frame = ProtocolMessage {
        request_id: "p:2".into(),
        payload: Payload {
            id: ids.request_id,
            value: request.encode(),
        },
    };
    let response = dispatch(&core, frame);
    assert_eq!(response.request_id, "p:2");
    assert_eq!(response.payload.id, ids.response_id);

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
    let ids = request_ids(method).expect("known request method");
    let response = dispatch(
        core,
        ProtocolMessage {
            request_id: request_id.into(),
            payload: Payload {
                id: ids.request_id,
                value,
            },
        },
    );
    assert_eq!(response.request_id, request_id);
    assert_eq!(response.payload.id, ids.response_id);
    assert_eq!(
        response.payload.value,
        vec![0x01, 0x02],
        "{method} must remain explicitly unavailable for current dotli parity"
    );
}

fn assert_subscription_start_interrupts_error<E: Encode>(
    core: &TrUApiCore,
    request_id: &str,
    method: &str,
    value: Vec<u8>,
    error: truapi::CallError<E>,
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

    let ids = subscription_ids(method).expect("known subscription method");
    let transport = Arc::new(RecordingTransport::default());
    futures::executor::block_on(core.dispatch(
        ProtocolMessage {
            request_id: request_id.into(),
            payload: Payload {
                id: ids.start_id,
                value,
            },
        },
        transport.clone(),
    ));

    let sent = transport.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].request_id, request_id);
    assert_eq!(sent[0].payload.id, ids.interrupt_id);
    assert_eq!(sent[0].payload.value, encode_call_error_payload(error));
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
fn statement_store_subscribe_topic_limit_interrupts_with_typed_error() {
    let core = make_core();
    let request = statement_store::RemoteStatementStoreSubscribeRequest::V1(
        v01::RemoteStatementStoreSubscribeRequest::MatchAny(vec![[7u8; 32]; 129]),
    );

    assert_subscription_start_interrupts_error(
        &core,
        "p:ss-too-many",
        "statement_store_subscribe",
        request.encode(),
        truapi::CallError::Domain(statement_store::RemoteStatementStoreSubscribeError::V1(
            v01::GenericError {
                reason: "MatchAny has 129 topics, maximum is 128".to_string(),
            },
        )),
    );
}

fn make_core() -> TrUApiCore {
    TrUApiCore::from_platform_with_config(
        Arc::new(WireShapePlatform),
        test_runtime_config(),
        test_spawner(),
    )
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
    let ids = subscription_ids(method).expect("known subscription method");
    let start = ProtocolMessage {
        request_id: "p:1".into(),
        payload: Payload {
            id: ids.start_id,
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
    assert_eq!(transport.sent.lock().unwrap()[0].payload.id, ids.receive_id);

    // Stop the subscription, then push a session change. A live subscription
    // would emit a Connected `_receive`; a stopped one must stay silent.
    let stop = ProtocolMessage {
        request_id: "p:1".into(),
        payload: Payload {
            id: ids.stop_id,
            value: Vec::new(),
        },
    };
    futures::executor::block_on(core.dispatch(stop, dyn_transport));
    std::thread::sleep(Duration::from_millis(50));

    core.session_state()
        .set_session(truapi_server::host_logic::session::SessionInfo {
            public_key: [7u8; 32],
            sso: None,
            root_entropy_source: None,
            identity_account_id: None,
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
