//! Binary golden-frame regression test.
//!
//! Loads `tests/snapshots/golden-account-get.bin` (the captured raw bytes
//! of an `account_get_account_request` frame) and asserts that
//! `ProtocolMessage::decode` produces the expected in-memory shape.
//!
//! The frame encodes:
//!   requestId = "p:1"
//!   payload   = account_get_account_request,
//!               inner = HostAccountGetRequest::V1(("foo", 0u32))
//!
//! On the wire (14 bytes):
//!   [0c 70 3a 31]                      requestId = compact-len(3) + "p:1"
//!   [16]                               discriminant 22 = account_get_account_request
//!   [00]                               versioned wrapper variant V1
//!   [0c 66 6f 6f]                      "foo"
//!   [00 00 00 00]                      u32 = 0
//!
//! If this test fails after a wire-protocol change, regenerate the file
//! deliberately and re-check the change against the wire table.

use parity_scale_codec::{Decode, Encode};
use truapi_server::{Payload, ProtocolMessage};

const GOLDEN: &[u8] = include_bytes!("snapshots/golden-account-get.bin");

#[test]
fn golden_account_get_frame_decodes_to_expected_message() {
    let decoded = ProtocolMessage::decode(&mut &GOLDEN[..])
        .expect("golden frame must decode with the current wire codec");

    let mut expected_inner = Vec::new();
    expected_inner.push(0x00u8); // V1 variant
    "foo".to_string().encode_to(&mut expected_inner);
    0u32.encode_to(&mut expected_inner);

    let expected = ProtocolMessage {
        request_id: "p:1".to_string(),
        payload: Payload {
            tag: "account_get_account_request".to_string(),
            value: expected_inner,
        },
    };
    assert_eq!(decoded, expected);
}

#[test]
fn golden_account_get_frame_round_trips() {
    // Encoding the in-memory shape must reproduce the on-disk bytes exactly.
    let decoded = ProtocolMessage::decode(&mut &GOLDEN[..]).expect("decode");
    assert_eq!(decoded.encode(), GOLDEN);
}
