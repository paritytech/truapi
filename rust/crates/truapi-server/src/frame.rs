//! Wire protocol frame types.
//!
//! Every message on the wire is a `ProtocolMessage` containing a `requestId`
//! and a `payload`. On the wire the envelope is:
//!
//! ```text
//!   [requestId: SCALE str][discriminant: u8][payload bytes...]
//! ```
//!
//! The discriminant maps to a method/kind slot via the auto-generated
//! [`crate::generated::wire_table::WIRE_TABLE`]. Method ordering is part of
//! the wire protocol; only ever append to the table. The payload bytes are
//! the SCALE-encoded inner value, inlined without a length prefix.
//!
//! In-memory we keep the numeric id directly so dispatch does not need to
//! reconstruct string action tags on every frame.

use parity_scale_codec::{Decode, Encode, Error as CodecError, Input, Output};

use crate::generated::wire_table::{RequestFrameIds, SubscriptionFrameIds, WIRE_TABLE, WireKind};

/// Top-level wire message. Encoded as `[requestId][discriminant][bytes]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolMessage {
    /// Per-message identifier carried by both halves of a request/response.
    pub request_id: String,
    /// Tagged payload describing the frame kind and SCALE bytes.
    pub payload: Payload,
}

/// Encode `Versioned<Result<Ok, _>>` from a versioned success wrapper.
///
/// TODO(shared-core-wire): once all hosts use the shared Rust core/generated
/// client stack, remove this dispatcher compatibility rewrite and encode the
/// trait return shape directly: `Result<VersionedOk, CallError<VersionedErr>>`.
pub fn encode_versioned_ok_payload<T: Encode>(value: T) -> Vec<u8> {
    encode_versioned_result_payload(value, 0)
}

/// Encode `Versioned<Result<(), _>>` for methods whose success type is unit.
pub fn encode_versioned_unit_ok_payload(version: u8) -> Vec<u8> {
    vec![version_index(version), 0]
}

/// Encode `Versioned<Result<_, Err>>` from an ordinary error value.
pub fn encode_versioned_err_payload<T: Encode>(value: T, version: u8) -> Vec<u8> {
    let encoded = value.encode();
    let mut out = Vec::with_capacity(encoded.len() + 2);
    out.push(version_index(version));
    out.push(1);
    out.extend_from_slice(&encoded);
    out
}

/// Encode `Result<(), _>` for unversioned methods whose success type is unit.
pub fn encode_raw_unit_ok_payload() -> Vec<u8> {
    Ok::<(), ()>(()).encode()
}

/// Encode `Result<(), Err>` for unversioned methods from an ordinary error value.
pub fn encode_raw_err_payload<T: Encode>(value: T) -> Vec<u8> {
    Err::<(), T>(value).encode()
}

/// Encode a versioned subscription interrupt payload from an ordinary error.
pub fn encode_versioned_interrupt_payload<T: Encode>(value: T, version: u8) -> Vec<u8> {
    let encoded = value.encode();
    let mut out = Vec::with_capacity(encoded.len() + 1);
    out.push(version_index(version));
    out.extend_from_slice(&encoded);
    out
}

impl Encode for ProtocolMessage {
    fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
        self.request_id.encode_to(dest);
        self.payload.id.encode_to(dest);
        // Payload bytes are inlined; the receiver reads "until end of frame"
        // because each transport frame is one ProtocolMessage. This matches
        // the public versioned enum transport shape (variant payload encoded
        // inline, no length prefix), and constrains us to slice-shaped
        // `Input`s on the decode side.
        dest.write(&self.payload.value);
    }
}

// Callers must hand `Decode` a slice-shaped `Input`; streaming inputs cannot
// decode this envelope because the payload has no length prefix.
impl Decode for ProtocolMessage {
    fn decode<I: Input>(input: &mut I) -> Result<Self, CodecError> {
        let request_id = String::decode(input)?;
        let id = u8::decode(input)?;
        // Unknown ids are accepted here; routing is deferred to dispatch,
        // which drops frames with no registered handler.
        let remaining = input
            .remaining_len()?
            .ok_or_else(|| CodecError::from("frame input must report remaining length"))?;
        let mut value = vec![0u8; remaining];
        input.read(&mut value)?;
        Ok(ProtocolMessage {
            request_id,
            payload: Payload { id, value },
        })
    }
}

/// Tagged payload. The `id` is the wire discriminant from
/// [`crate::generated::wire_table::WIRE_TABLE`], identifying the frame's method
/// and kind (request/response/start/stop/interrupt/receive).
///
/// Note: `Payload` does not derive `Encode`/`Decode` directly; the wire
/// representation lives on [`ProtocolMessage`]. `Payload` is kept as a plain
/// data type for in-memory dispatch (key on `id`, value bytes already
/// SCALE-encoded by the call site).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    /// Wire discriminant identifying the frame's method and kind.
    pub id: u8,
    /// SCALE-encoded inner value bytes.
    pub value: Vec<u8>,
}

/// Request discriminants for a request method, by name. Walks the generated
/// [`WIRE_TABLE`]; intended for tests and embedders that route by method
/// string rather than holding the generated const.
pub fn request_ids(method: &str) -> Option<RequestFrameIds> {
    WIRE_TABLE
        .iter()
        .find_map(|entry| match (&entry.kind, entry.method == method) {
            (WireKind::Request(ids), true) => Some(*ids),
            _ => None,
        })
}

/// Subscription discriminants for a subscription method, by name. Walks the
/// generated [`WIRE_TABLE`].
pub fn subscription_ids(method: &str) -> Option<SubscriptionFrameIds> {
    WIRE_TABLE
        .iter()
        .find_map(|entry| match (&entry.kind, entry.method == method) {
            (WireKind::Subscription(ids), true) => Some(*ids),
            _ => None,
        })
}

/// Unique ID generator with a prefix.
pub struct IdFactory {
    prefix: String,
    counter: u64,
}

impl IdFactory {
    /// Build a factory that mints IDs of the form `{prefix}{counter}`.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            counter: 0,
        }
    }

    /// Return the next ID, monotonically increasing from 1.
    pub fn next_id(&mut self) -> String {
        self.counter += 1;
        format!("{}{}", self.prefix, self.counter)
    }
}

fn encode_versioned_result_payload<T: Encode>(value: T, result_index: u8) -> Vec<u8> {
    let encoded = value.encode();
    let Some((&version_index, inner)) = encoded.split_first() else {
        return vec![result_index];
    };
    let mut out = Vec::with_capacity(encoded.len() + 1);
    out.push(version_index);
    out.push(result_index);
    out.extend_from_slice(inner);
    out
}

fn version_index(version: u8) -> u8 {
    version.saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Encode)]
    enum TestVersioned<T> {
        V1(T),
    }

    fn build(id: u8, value: Vec<u8>) -> ProtocolMessage {
        ProtocolMessage {
            request_id: "p:1".to_string(),
            payload: Payload { id, value },
        }
    }

    fn expected_wire(id: u8, value: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        "p:1".to_string().encode_to(&mut out);
        out.push(id);
        out.extend_from_slice(value);
        out
    }

    #[test]
    fn handshake_request_encodes_with_discriminant_zero() {
        // SCALE-encoded HostHandshakeRequest::V1(1u8) = [0u8 variant][1u8 codec_version]
        let inner: Vec<u8> = vec![0x00, 0x01];
        let msg = build(0, inner.clone());
        assert_eq!(msg.encode(), expected_wire(0, &inner));
    }

    #[test]
    fn get_account_request_encodes_with_discriminant_22() {
        let mut inner = vec![0x00]; // V1 variant
        "foo".to_string().encode_to(&mut inner);
        0u32.encode_to(&mut inner);
        let msg = build(22, inner.clone());
        assert_eq!(msg.encode(), expected_wire(22, &inner));
    }

    #[test]
    fn round_trip_preserves_id_and_value() {
        let inner: Vec<u8> = vec![0x00, 0x42, 0xab, 0xcd];
        let msg = build(12, inner.clone());
        let decoded = ProtocolMessage::decode(&mut &msg.encode()[..]).expect("decode");
        assert_eq!(decoded, msg);
    }

    /// An unknown discriminant is no longer rejected at decode; routing is
    /// deferred to dispatch (which drops frames with no registered handler).
    #[test]
    fn unknown_discriminant_decodes_ok() {
        let mut bytes = Vec::new();
        "p:1".to_string().encode_to(&mut bytes);
        bytes.push(250); // far outside the populated range
        bytes.extend_from_slice(&[0xaa, 0xbb]);
        let decoded = ProtocolMessage::decode(&mut &bytes[..]).expect("unknown id must decode");
        assert_eq!(decoded.payload.id, 250);
        assert_eq!(decoded.payload.value, vec![0xaa, 0xbb]);
    }

    /// All four subscription phases round-trip through the codec. Catches a
    /// regression where `Decode` mishandles a frame whose payload is empty for
    /// `_stop` / `_interrupt` (no inner data) but non-empty for `_start` /
    /// `_receive`. The ids are the `account_connection_status_subscribe`
    /// quartet (18..=21).
    #[test]
    fn subscription_phases_round_trip_through_codec() {
        let cases: &[(u8, Vec<u8>)] = &[
            (18, vec![0x00, 0xaa]),             // start
            (19, Vec::new()),                   // stop
            (20, Vec::new()),                   // interrupt
            (21, vec![0x01, 0x02, 0x03, 0x04]), // receive
        ];
        for (id, value) in cases {
            let msg = build(*id, value.clone());
            let bytes = msg.encode();
            assert_eq!(
                bytes,
                expected_wire(*id, value),
                "encode mismatch for id {id}"
            );
            let decoded = ProtocolMessage::decode(&mut &bytes[..]).expect("decode");
            assert_eq!(decoded, msg, "round-trip mismatch for id {id}");
        }
    }

    /// `request_ids` / `subscription_ids` resolve a method name to its
    /// generated discriminants without going through the codec.
    #[test]
    fn id_helpers_resolve_known_methods() {
        let handshake = request_ids("system_handshake").expect("known request method");
        assert_eq!(handshake.request_id, 0);
        assert_eq!(handshake.response_id, 1);

        let get_account = request_ids("account_get_account").expect("known request method");
        assert_eq!(get_account.request_id, 22);

        let sub =
            subscription_ids("account_connection_status_subscribe").expect("known subscription");
        assert_eq!(sub.start_id, 18);
        assert_eq!(sub.stop_id, 19);
        assert_eq!(sub.interrupt_id, 20);
        assert_eq!(sub.receive_id, 21);

        // A request method is not a subscription and vice versa.
        assert!(subscription_ids("system_handshake").is_none());
        assert!(request_ids("account_connection_status_subscribe").is_none());
        assert!(request_ids("not_a_method").is_none());
    }

    /// Genuine zero-byte payload (e.g. unit-typed response). `Decode` must
    /// handle `remaining_len == 0` without erroring or reading past EOF.
    #[test]
    fn empty_payload_round_trips() {
        // local_storage_clear_response = 17.
        let msg = build(17, Vec::new());
        let bytes = msg.encode();
        // [SCALE compact-len 0x0c][p][:][1][u8 17] = 4 + 1 = 5 bytes total
        assert_eq!(bytes.len(), 5);
        let decoded = ProtocolMessage::decode(&mut &bytes[..]).expect("decode");
        assert_eq!(decoded, msg);
    }

    /// Compact-len mode 1 kicks in for strings with length 64..=16383. Make
    /// sure the codec handles a long requestId without truncation.
    #[test]
    fn long_request_id_round_trips() {
        let long_id: String = "x".repeat(200);
        let msg = ProtocolMessage {
            request_id: long_id,
            payload: Payload {
                id: 22,
                value: vec![0x00, 0xab, 0xcd],
            },
        };
        let decoded = ProtocolMessage::decode(&mut &msg.encode()[..]).expect("decode");
        assert_eq!(decoded, msg);
    }

    /// Truncated frames must surface a `CodecError`, not panic.
    #[test]
    fn truncated_frames_error_cleanly() {
        // Empty buffer.
        assert!(ProtocolMessage::decode(&mut &[][..]).is_err());
        // Just the requestId, no discriminant byte.
        let mut only_request_id = Vec::new();
        "p:1".to_string().encode_to(&mut only_request_id);
        assert!(ProtocolMessage::decode(&mut &only_request_id[..]).is_err());
        // RequestId header claims length=200 but the buffer is far shorter.
        let truncated_str_header = [200u8 << 2, 0x61, 0x62, 0x63];
        assert!(ProtocolMessage::decode(&mut &truncated_str_header[..]).is_err());
    }

    /// Empty requestId (zero-length string) is a valid SCALE-encoded `str`
    /// (compact-len 0, no body). The codec must round-trip it without
    /// confusing length-0 with EOF.
    #[test]
    fn empty_request_id_round_trips() {
        let msg = ProtocolMessage {
            request_id: String::new(),
            payload: Payload {
                id: 22,
                value: vec![0x00, 0x01, 0x02],
            },
        };
        let bytes = msg.encode();
        // [SCALE compact-len 0 = 0x00][discriminant][payload]
        assert_eq!(bytes[0], 0x00);
        let decoded = ProtocolMessage::decode(&mut &bytes[..]).expect("decode");
        assert_eq!(decoded, msg);
    }

    /// Unicode characters round-trip through SCALE string encoding.
    #[test]
    fn unicode_request_id_round_trips() {
        let msg = ProtocolMessage {
            request_id: "héllo-世界-🦀".to_string(),
            payload: Payload {
                id: 22,
                value: vec![0x00, 0x01],
            },
        };
        let decoded = ProtocolMessage::decode(&mut &msg.encode()[..]).expect("decode");
        assert_eq!(decoded, msg);
    }

    /// Large payload (>64KiB) round-trips. Catches buffer-size assumptions
    /// in the inline-payload read path.
    #[test]
    fn large_payload_round_trips() {
        let big = vec![0xa5u8; 100 * 1024];
        let msg = build(22, big);
        let decoded = ProtocolMessage::decode(&mut &msg.encode()[..]).expect("decode");
        assert_eq!(decoded, msg);
    }

    #[test]
    fn encode_versioned_unit_ok_payload_wraps_unit_success() {
        assert_eq!(encode_versioned_unit_ok_payload(1), vec![0u8, 0u8]);
        assert_eq!(encode_versioned_unit_ok_payload(0), vec![0u8, 0u8]);
    }

    #[test]
    fn encode_versioned_ok_payload_wraps_success_values() {
        let mut expected = vec![0u8, 0u8];
        7u32.encode_to(&mut expected);
        assert_eq!(
            encode_versioned_ok_payload(TestVersioned::V1(7u32)),
            expected
        );
    }

    #[test]
    fn encode_versioned_err_payload_wraps_error_values() {
        let mut expected = vec![0u8, 1u8];
        9u32.encode_to(&mut expected);
        assert_eq!(encode_versioned_err_payload(9u32, 1), expected);
    }

    #[test]
    fn encode_versioned_interrupt_payload_wraps_error_values() {
        let mut expected = vec![1u8];
        9u32.encode_to(&mut expected);
        assert_eq!(encode_versioned_interrupt_payload(9u32, 2), expected);
    }

    /// IdFactory mints monotonically increasing ids prefixed with the
    /// configured string.
    #[test]
    fn id_factory_minted_ids_are_unique_and_monotonic() {
        let mut factory = IdFactory::new("p:");
        assert_eq!(factory.next_id(), "p:1");
        assert_eq!(factory.next_id(), "p:2");
        assert_eq!(factory.next_id(), "p:3");
    }

    /// Two distinct factories each maintain their own counter; minting from
    /// one does not advance the other.
    #[test]
    fn two_factories_dont_share_state() {
        let mut a = IdFactory::new("a:");
        let mut b = IdFactory::new("b:");
        assert_eq!(a.next_id(), "a:1");
        assert_eq!(b.next_id(), "b:1");
        assert_eq!(a.next_id(), "a:2");
        assert_eq!(b.next_id(), "b:2");
    }
}
