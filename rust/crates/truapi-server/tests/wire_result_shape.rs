//! Result-wire-shape regression test.
//!
//! The TS host/client codec expects every request response to be a
//! `Versioned<Result<Ok, Err>>` envelope on the wire (one leading version byte,
//! then one result discriminant byte, then the SCALE-encoded value). This test stands up a
//! `TrUApiCore::from_platform_with_config` with a platform whose `Features`
//! impl returns `Ok(supported = true)` and asserts:
//!
//! - A `system_feature_supported_request` produces a response whose
//!   payload begins with `0x00` (V1), then `0x00` (Ok), followed by the encoded
//!   `HostFeatureSupportedResponse`.
//! - A `local_storage_read_request` whose stub returns
//!   `Err(HostLocalStorageReadError::Full)` produces a response whose
//!   payload begins with `0x00` (V1), then `0x01` (Err), followed by the encoded
//!   `HostLocalStorageReadError::Full`.
//!
//! Both halves prove the wire layout stays in lockstep with the TS
//! `S.indexedTaggedUnion({ V1: S.Result(ok, err) })` codec.

use std::sync::Arc;

use parity_scale_codec::{Decode, Encode};

#[cfg(debug_assertions)]
use truapi::v02;
use truapi::versioned::system::HostFeatureSupportedRequest;
#[cfg(debug_assertions)]
use truapi::versioned::testing;
use truapi::versioned::{Versioned, account, payment, statement_store};
use truapi::{CallError, v01};

use truapi_server::core::TrUApiCore;
use truapi_server::frame::{Payload, ProtocolMessage, request_ids, subscription_ids};

mod common;
use common::{RecordingTransport, WireShapePlatform, test_runtime_config, test_spawner};

const PAYMENTS_NOT_IMPLEMENTED: &str = "Payments are not supported in dot.li";

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

    // Wire payload: [V1 disc=0x00][Ok disc=0x00][encoded response body].
    let mut expected = vec![0x00u8, 0x00u8];
    v01::HostFeatureSupportedResponse { supported: true }.encode_to(&mut expected);
    assert_eq!(response.payload.value, expected);
    assert_eq!(response.payload.value.first(), Some(&0x00));
    assert_eq!(response.payload.value.get(1), Some(&0x00));
}

#[cfg(debug_assertions)]
#[test]
fn testing_version_probe_v1_request_gets_v1_response() {
    let core = make_core();
    let request = testing::TestingVersionProbeRequest::V1(v01::TestingVersionProbeRequest {
        message: "hello V1".to_string(),
    });
    let ids = request_ids("testing_version_probe").expect("known request method");
    let response = dispatch(
        &core,
        ProtocolMessage {
            request_id: "p:testing-v1".into(),
            payload: Payload {
                id: ids.request_id,
                value: request.encode(),
            },
        },
    );

    let mut expected = vec![0x00u8, 0x00u8];
    v01::TestingVersionProbeResponse {
        received_version: 1,
        message: "hello V1".to_string(),
    }
    .encode_to(&mut expected);
    assert_eq!(response.payload.value, expected);
}

#[cfg(debug_assertions)]
#[test]
fn testing_version_probe_v2_request_gets_v2_response() {
    let core = make_core();
    let request = testing::TestingVersionProbeRequest::V2(v02::TestingVersionProbeRequest {
        message: "hello V2".to_string(),
        marker: 42,
    });
    let ids = request_ids("testing_version_probe").expect("known request method");
    let response = dispatch(
        &core,
        ProtocolMessage {
            request_id: "p:testing-v2".into(),
            payload: Payload {
                id: ids.request_id,
                value: request.encode(),
            },
        },
    );

    let mut expected = vec![0x01u8, 0x00u8];
    v02::TestingVersionProbeResponse {
        received_version: 2,
        message: "hello V2".to_string(),
        marker: 42,
    }
    .encode_to(&mut expected);
    assert_eq!(response.payload.value, expected);
}

#[cfg(debug_assertions)]
#[test]
fn testing_echo_error_uses_raw_result_shape() {
    let core = make_core();
    let request = v01::EchoErrorRequest {
        error: CallError::HostFailure {
            reason: "forced by testing.echo_error".to_string(),
        },
    };
    let ids = request_ids("testing_echo_error").expect("known request method");
    let response = dispatch(
        &core,
        ProtocolMessage {
            request_id: "p:testing-framework".into(),
            payload: Payload {
                id: ids.request_id,
                value: request.encode(),
            },
        },
    );

    let mut expected = vec![0x01u8];
    CallError::<v01::TestingVersionProbeError>::HostFailure {
        reason: "forced by testing.echo_error".to_string(),
    }
    .encode_to(&mut expected);
    assert_eq!(response.payload.value, expected);
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

    // Wire payload:
    // [V1 disc=0x00][Err disc=0x01][CallError::Domain][V1 error][encoded error body].
    let mut expected = vec![0x00u8, 0x01u8];
    CallError::Domain(
        truapi::versioned::local_storage::HostLocalStorageReadError::V1(
            v01::HostLocalStorageReadError::Full,
        ),
    )
    .encode_to(&mut expected);
    assert_eq!(response.payload.value, expected);
    assert_eq!(response.payload.value.first(), Some(&0x00));
    assert_eq!(response.payload.value.get(1), Some(&0x01));
}

fn versioned_result_err_payload<E>(error: E) -> Vec<u8>
where
    E: Clone + Encode + Versioned,
{
    let mut expected = vec![version_index(error.version()), 0x01u8];
    CallError::Domain(error).encode_to(&mut expected);
    expected
}

fn versioned_interrupt_err_payload<E>(error: E) -> Vec<u8>
where
    E: Clone + Encode + Versioned,
{
    let mut expected = vec![version_index(error.version())];
    CallError::Domain(error).encode_to(&mut expected);
    expected
}

fn assert_request_returns_domain_error<E>(
    core: &TrUApiCore,
    request_id: &str,
    method: &str,
    value: Vec<u8>,
    error: E,
) where
    E: Clone + Encode + Versioned,
{
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
    assert_eq!(response.payload.value, versioned_result_err_payload(error));
}

fn assert_subscription_start_interrupts_error<E>(
    core: &TrUApiCore,
    request_id: &str,
    method: &str,
    value: Vec<u8>,
    error: E,
) where
    E: Clone + Encode + Versioned,
{
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
    assert_eq!(
        sent[0].payload.value,
        versioned_interrupt_err_payload(error)
    );
}

fn version_index(version: u8) -> u8 {
    version.saturating_sub(1)
}

#[test]
fn deferred_account_proof_returns_framework_unsupported() {
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

    let ids = request_ids("account_create_account_proof").expect("known request method");
    let response = dispatch(
        &core,
        ProtocolMessage {
            request_id: "p:account-proof".into(),
            payload: Payload {
                id: ids.request_id,
                value: request.encode(),
            },
        },
    );
    assert_eq!(response.request_id, "p:account-proof");
    assert_eq!(response.payload.id, ids.response_id);
    assert_eq!(response.payload.value, vec![0x00u8, 0x01u8, 0x02u8]);
}

#[test]
fn deferred_payment_requests_return_dotli_not_implemented_errors() {
    let core = make_core();
    let request = payment::HostPaymentRequest::V1(v01::HostPaymentRequest {
        from: None,
        amount: 1,
        destination: [0u8; 32],
    });

    assert_request_returns_domain_error(
        &core,
        "p:payment",
        "payment_request",
        request.encode(),
        payment::HostPaymentError::V1(v01::HostPaymentError::Unknown {
            reason: PAYMENTS_NOT_IMPLEMENTED.to_string(),
        }),
    );

    let top_up = payment::HostPaymentTopUpRequest::V1(v01::HostPaymentTopUpRequest {
        into: None,
        amount: 1,
        source: v01::PaymentTopUpSource::ProductAccount {
            derivation_index: 0,
        },
    });
    assert_request_returns_domain_error(
        &core,
        "p:top-up",
        "payment_top_up",
        top_up.encode(),
        payment::HostPaymentTopUpError::V1(v01::HostPaymentTopUpError::Unknown {
            reason: PAYMENTS_NOT_IMPLEMENTED.to_string(),
        }),
    );
}

#[test]
fn deferred_payment_subscriptions_interrupt_dotli_not_implemented_errors() {
    let core = make_core();
    let balance =
        payment::HostPaymentBalanceSubscribeRequest::V1(v01::HostPaymentBalanceSubscribeRequest {
            purse: None,
        });
    assert_subscription_start_interrupts_error(
        &core,
        "p:balance",
        "payment_balance_subscribe",
        balance.encode(),
        payment::HostPaymentBalanceSubscribeError::V1(
            v01::HostPaymentBalanceSubscribeError::PermissionDenied,
        ),
    );

    let status =
        payment::HostPaymentStatusSubscribeRequest::V1(v01::HostPaymentStatusSubscribeRequest {
            payment_id: "payment-id".to_string(),
        });
    assert_subscription_start_interrupts_error(
        &core,
        "p:status",
        "payment_status_subscribe",
        status.encode(),
        payment::HostPaymentStatusSubscribeError::V1(
            v01::HostPaymentStatusSubscribeError::Unknown {
                reason: PAYMENTS_NOT_IMPLEMENTED.to_string(),
            },
        ),
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
        statement_store::RemoteStatementStoreSubscribeError::V1(v01::GenericError {
            reason: "MatchAny has 129 topics, maximum is 128".to_string(),
        }),
    );
}

#[test]
fn malformed_result_subscription_start_interrupts_with_malformed_frame() {
    let core = make_core();
    let method = "payment_balance_subscribe";
    let ids = subscription_ids(method).expect("known subscription method");
    let transport = Arc::new(RecordingTransport::default());

    futures::executor::block_on(core.dispatch(
        ProtocolMessage {
            request_id: "p:malformed-sub".into(),
            payload: Payload {
                id: ids.start_id,
                value: vec![0xff],
            },
        },
        transport.clone(),
    ));

    let sent = transport.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].request_id, "p:malformed-sub");
    assert_eq!(sent[0].payload.id, ids.interrupt_id);
    assert_eq!(sent[0].payload.value.first(), Some(&0x00));

    let mut payload = &sent[0].payload.value[1..];
    let error = CallError::<payment::HostPaymentBalanceSubscribeError>::decode(&mut payload)
        .expect("decode malformed interrupt error");
    assert!(payload.is_empty());
    match error {
        CallError::MalformedFrame { reason } => assert!(!reason.is_empty()),
        other => panic!("expected MalformedFrame interrupt, got {other:?}"),
    }
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
    use std::time::{Duration, Instant};
    use truapi_server::transport::Transport;

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
