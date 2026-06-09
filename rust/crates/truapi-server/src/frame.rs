//! Wire protocol frame types.
//!
//! Every message on the wire is a `ProtocolMessage` containing a `requestId`
//! and a `payload`. On the wire the envelope is:
//!
//! ```text
//!   [requestId: SCALE str][discriminant: u8][payload bytes...]
//! ```
//!
//! The discriminant maps to a method/kind tag via the auto-generated
//! [`crate::generated::wire_table::WIRE_TABLE`]. Method ordering is part of
//! the wire protocol; only ever append to the table. The payload bytes are
//! the SCALE-encoded inner value, inlined without a length prefix.
//!
//! In-memory we keep the tag as a `String` so the dispatcher (which keys on
//! method name) is unchanged. Only the codec impls below cross between
//! string-tag and discriminant.

use parity_scale_codec::{Decode, Encode, Error as CodecError, Input, Output};

use truapi::CallError;

use crate::generated::wire_table::{WIRE_TABLE, WireKind};

/// Top-level wire message. Encoded as `[requestId][discriminant][bytes]`; the
/// in-memory `payload.tag` carries the resolved method tag string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolMessage {
    /// Per-message identifier carried by both halves of a request/response.
    pub request_id: String,
    /// Tagged payload describing the frame kind and SCALE bytes.
    pub payload: Payload,
}

/// Encode `CallError::MalformedFrame { reason }` as the SCALE-encoded payload
/// bytes a handler returns on a decode failure. The dispatcher wraps these
/// bytes into a response frame with the matching `request_id` and response
/// tag.
pub fn encode_decode_error(reason: String) -> Vec<u8> {
    let err: CallError<()> = CallError::MalformedFrame { reason };
    encode_call_error(&err)
}

/// Encode a `CallError<E>` as the SCALE-encoded payload bytes a handler
/// returns on the error path. The dispatcher wraps these bytes into a
/// response frame with the matching `request_id` and response tag.
pub fn encode_call_error_payload<E: Encode>(err: CallError<E>) -> Vec<u8> {
    encode_call_error(&err)
}

impl Encode for ProtocolMessage {
    fn encode_to<T: Output + ?Sized>(&self, dest: &mut T) {
        self.request_id.encode_to(dest);
        // Encode the discriminant. An unknown tag is a build-time bug (the
        // dispatcher only emits tags it registered handlers for) but we
        // poison the frame with 0xFF instead of panicking so a misconfigured
        // impl can't take down the host process. The poisoned discriminant
        // decodes as "unknown wire discriminant" on the peer.
        let id = id_for_tag(&self.payload.tag).unwrap_or(0xFF);
        id.encode_to(dest);
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
        let tag = tag_for_id(id)
            .ok_or_else(|| CodecError::from("unknown wire discriminant"))?
            .to_string();
        let remaining = input
            .remaining_len()?
            .ok_or_else(|| CodecError::from("frame input must report remaining length"))?;
        let mut value = vec![0u8; remaining];
        input.read(&mut value)?;
        Ok(ProtocolMessage {
            request_id,
            payload: Payload { tag, value },
        })
    }
}

/// Tagged payload. The `tag` encodes `{method}_{suffix}` where suffix is one of:
/// `request`, `response`, `start`, `receive`, `stop`, `interrupt`.
///
/// Note: `Payload` does not derive `Encode`/`Decode` directly; the wire
/// representation lives on [`ProtocolMessage`]. `Payload` is kept as a plain
/// data type for in-memory dispatch (key on `tag`, value bytes already
/// SCALE-encoded by the call site).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    /// Method tag with frame-kind suffix.
    pub tag: String,
    /// SCALE-encoded inner value bytes.
    pub value: Vec<u8>,
}

/// The suffix part of an action tag, identifying the frame type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    /// Client-initiated request frame.
    Request,
    /// Host response to a request.
    Response,
    /// Client-initiated subscription start.
    Start,
    /// Host-emitted item on a subscription.
    Receive,
    /// Client-initiated subscription stop.
    Stop,
    /// Host-emitted subscription termination.
    Interrupt,
}

impl FrameKind {
    /// Return the wire suffix string for this frame kind.
    pub fn suffix(&self) -> &'static str {
        match self {
            FrameKind::Request => "request",
            FrameKind::Response => "response",
            FrameKind::Start => "start",
            FrameKind::Receive => "receive",
            FrameKind::Stop => "stop",
            FrameKind::Interrupt => "interrupt",
        }
    }

    /// Parse the suffix from an action tag string, returning `(method, kind)`.
    pub fn from_tag(tag: &str) -> Option<(String, FrameKind)> {
        for kind in [
            FrameKind::Request,
            FrameKind::Response,
            FrameKind::Start,
            FrameKind::Receive,
            FrameKind::Stop,
            FrameKind::Interrupt,
        ] {
            let suffix = format!("_{}", kind.suffix());
            if let Some(method) = tag.strip_suffix(&suffix) {
                return Some((method.to_string(), kind));
            }
        }
        None
    }
}

/// Build an action tag from method name and frame kind.
pub fn compose_action(method: &str, kind: FrameKind) -> String {
    format!("{}_{}", method, kind.suffix())
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

/// Look up a discriminant by tag. Walks the generated [`WIRE_TABLE`].
pub fn id_for_tag(tag: &str) -> Option<u8> {
    let (method, kind) = FrameKind::from_tag(tag)?;
    for entry in WIRE_TABLE {
        if entry.method != method {
            continue;
        }
        return match (&entry.kind, kind) {
            (WireKind::Request { request_id, .. }, FrameKind::Request) => Some(*request_id),
            (WireKind::Request { response_id, .. }, FrameKind::Response) => Some(*response_id),
            (WireKind::Subscription { start_id, .. }, FrameKind::Start) => Some(*start_id),
            (WireKind::Subscription { stop_id, .. }, FrameKind::Stop) => Some(*stop_id),
            (WireKind::Subscription { interrupt_id, .. }, FrameKind::Interrupt) => {
                Some(*interrupt_id)
            }
            (WireKind::Subscription { receive_id, .. }, FrameKind::Receive) => Some(*receive_id),
            _ => None,
        };
    }
    None
}

/// Look up a tag string by discriminant. Walks the generated [`WIRE_TABLE`].
pub fn tag_for_id(id: u8) -> Option<&'static str> {
    static CACHE: std::sync::OnceLock<Vec<Option<(&'static str, FrameKind)>>> =
        std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(|| {
        let max = WIRE_TABLE
            .iter()
            .map(|e| match &e.kind {
                WireKind::Request {
                    request_id,
                    response_id,
                } => (*request_id).max(*response_id),
                WireKind::Subscription {
                    start_id,
                    stop_id,
                    interrupt_id,
                    receive_id,
                } => (*start_id)
                    .max(*stop_id)
                    .max(*interrupt_id)
                    .max(*receive_id),
            })
            .max()
            .unwrap_or(0);
        let mut table: Vec<Option<(&'static str, FrameKind)>> = vec![None; usize::from(max) + 1];
        for entry in WIRE_TABLE {
            match &entry.kind {
                WireKind::Request {
                    request_id,
                    response_id,
                } => {
                    table[usize::from(*request_id)] = Some((entry.method, FrameKind::Request));
                    table[usize::from(*response_id)] = Some((entry.method, FrameKind::Response));
                }
                WireKind::Subscription {
                    start_id,
                    stop_id,
                    interrupt_id,
                    receive_id,
                } => {
                    table[usize::from(*start_id)] = Some((entry.method, FrameKind::Start));
                    table[usize::from(*stop_id)] = Some((entry.method, FrameKind::Stop));
                    table[usize::from(*interrupt_id)] = Some((entry.method, FrameKind::Interrupt));
                    table[usize::from(*receive_id)] = Some((entry.method, FrameKind::Receive));
                }
            }
        }
        table
    });
    let (method, kind) = (*cache.get(usize::from(id))?)?;
    // Leak the composed tag once per id so we can hand out `&'static str`.
    static TAGS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<u8, &'static str>>,
    > = std::sync::OnceLock::new();
    let mut map = TAGS
        .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
        .lock()
        .ok()?;
    if let Some(s) = map.get(&id) {
        return Some(*s);
    }
    let leaked: &'static str = Box::leak(compose_action(method, kind).into_boxed_str());
    map.insert(id, leaked);
    Some(leaked)
}

/// Encode a `CallError<E>` as SCALE bytes. `CallError` does not derive
/// `Encode` directly so the variants are emitted manually.
fn encode_call_error<E: Encode>(err: &CallError<E>) -> Vec<u8> {
    let mut out = Vec::new();
    match err {
        CallError::Domain(value) => {
            0u8.encode_to(&mut out);
            value.encode_to(&mut out);
        }
        CallError::Denied => {
            1u8.encode_to(&mut out);
        }
        CallError::Unsupported => {
            2u8.encode_to(&mut out);
        }
        CallError::MalformedFrame { reason } => {
            3u8.encode_to(&mut out);
            reason.encode_to(&mut out);
        }
        CallError::HostFailure { reason } => {
            4u8.encode_to(&mut out);
            reason.encode_to(&mut out);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(tag: &str, value: Vec<u8>) -> ProtocolMessage {
        ProtocolMessage {
            request_id: "p:1".to_string(),
            payload: Payload {
                tag: tag.to_string(),
                value,
            },
        }
    }

    fn expected_wire(tag_id: u8, value: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        "p:1".to_string().encode_to(&mut out);
        out.push(tag_id);
        out.extend_from_slice(value);
        out
    }

    #[test]
    fn handshake_request_encodes_with_discriminant_zero() {
        // SCALE-encoded HostHandshakeRequest::V1(1u8) = [0u8 variant][1u8 codec_version]
        let inner: Vec<u8> = vec![0x00, 0x01];
        let msg = build("system_handshake_request", inner.clone());
        assert_eq!(msg.encode(), expected_wire(0, &inner));
    }

    #[test]
    fn get_account_request_encodes_with_discriminant_22() {
        let mut inner = vec![0x00]; // V1 variant
        "foo".to_string().encode_to(&mut inner);
        0u32.encode_to(&mut inner);
        let msg = build("account_get_account_request", inner.clone());
        assert_eq!(msg.encode(), expected_wire(22, &inner));
    }

    #[test]
    fn round_trip_preserves_tag_and_value() {
        let inner: Vec<u8> = vec![0x00, 0x42, 0xab, 0xcd];
        let msg = build("local_storage_read_request", inner.clone());
        let decoded = ProtocolMessage::decode(&mut &msg.encode()[..]).expect("decode");
        assert_eq!(decoded, msg);
    }

    #[test]
    fn unknown_discriminant_fails_to_decode() {
        let mut bytes = Vec::new();
        "p:1".to_string().encode_to(&mut bytes);
        bytes.push(250); // far outside the populated range
        bytes.extend_from_slice(&[0u8; 4]);
        assert!(ProtocolMessage::decode(&mut &bytes[..]).is_err());
    }

    #[test]
    fn subscription_phases_share_consecutive_ids() {
        assert_eq!(
            id_for_tag("account_connection_status_subscribe_start"),
            Some(18)
        );
        assert_eq!(
            id_for_tag("account_connection_status_subscribe_stop"),
            Some(19)
        );
        assert_eq!(
            id_for_tag("account_connection_status_subscribe_interrupt"),
            Some(20)
        );
        assert_eq!(
            id_for_tag("account_connection_status_subscribe_receive"),
            Some(21)
        );
    }

    /// All four subscription phases round-trip through the codec, not just
    /// the lookup table. Catches a regression where `Decode` mishandles a
    /// frame whose payload is empty for `_stop` / `_interrupt` (no inner
    /// data) but non-empty for `_start` / `_receive`.
    #[test]
    fn subscription_phases_round_trip_through_codec() {
        let cases: &[(&str, u8, Vec<u8>)] = &[
            (
                "account_connection_status_subscribe_start",
                18,
                vec![0x00, 0xaa],
            ),
            ("account_connection_status_subscribe_stop", 19, Vec::new()),
            (
                "account_connection_status_subscribe_interrupt",
                20,
                Vec::new(),
            ),
            (
                "account_connection_status_subscribe_receive",
                21,
                vec![0x01, 0x02, 0x03, 0x04],
            ),
        ];
        for (tag, id, value) in cases {
            let msg = build(tag, value.clone());
            let bytes = msg.encode();
            assert_eq!(
                bytes,
                expected_wire(*id, value),
                "encode mismatch for {tag}"
            );
            let decoded = ProtocolMessage::decode(&mut &bytes[..]).expect("decode");
            assert_eq!(decoded, msg, "round-trip mismatch for {tag}");
        }
    }

    /// Genuine zero-byte payload (e.g. unit-typed response). `Decode` must
    /// handle `remaining_len == 0` without erroring or reading past EOF.
    #[test]
    fn empty_payload_round_trips() {
        let msg = build("local_storage_clear_response", Vec::new());
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
                tag: "account_get_account_request".to_string(),
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
                tag: "account_get_account_request".to_string(),
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
                tag: "account_get_account_request".to_string(),
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
        let msg = build("account_get_account_request", big);
        let decoded = ProtocolMessage::decode(&mut &msg.encode()[..]).expect("decode");
        assert_eq!(decoded, msg);
    }

    /// Discriminant 0xFF is the documented poison/escape-hatch slot. It is
    /// not registered as a real tag, so encoding a frame with an unknown
    /// tag falls back to 0xFF, and the decoder must reject 0xFF.
    #[test]
    fn poison_slot_0xff_is_unmapped_and_decode_rejects() {
        assert!(tag_for_id(0xFF).is_none());
        let mut bytes = Vec::new();
        "p:1".to_string().encode_to(&mut bytes);
        bytes.push(0xFF);
        bytes.extend_from_slice(&[0u8; 4]);
        assert!(
            ProtocolMessage::decode(&mut &bytes[..]).is_err(),
            "decoding the 0xFF poison slot must fail",
        );
    }

    #[test]
    fn encode_decode_error_matches_malformed_frame_variant() {
        let bytes = encode_decode_error("bad input".to_string());
        let mut expected = Vec::new();
        let err: CallError<()> = CallError::MalformedFrame {
            reason: "bad input".to_string(),
        };
        match &err {
            CallError::MalformedFrame { reason } => {
                3u8.encode_to(&mut expected);
                reason.encode_to(&mut expected);
            }
            _ => unreachable!(),
        }
        assert_eq!(bytes, expected);
    }

    #[test]
    fn encode_call_error_payload_matches_call_error_variants() {
        let denied: CallError<()> = CallError::Denied;
        assert_eq!(encode_call_error_payload(denied), vec![1u8]);

        let unsupported: CallError<()> = CallError::Unsupported;
        assert_eq!(encode_call_error_payload(unsupported), vec![2u8]);

        let host: CallError<()> = CallError::HostFailure {
            reason: "x".to_string(),
        };
        let mut expected = vec![4u8];
        "x".to_string().encode_to(&mut expected);
        assert_eq!(encode_call_error_payload(host), expected);
    }

    /// `id_for_tag` resolves a known tag (`system_feature_supported_request`,
    /// id 2 per the generated table) without going through round-trip code.
    #[test]
    fn id_for_tag_known_method_returns_id() {
        assert_eq!(id_for_tag("system_handshake_request"), Some(0));
        assert_eq!(id_for_tag("system_handshake_response"), Some(1));
        assert_eq!(id_for_tag("system_feature_supported_request"), Some(2));
        assert_eq!(id_for_tag("system_feature_supported_response"), Some(3));
        assert_eq!(id_for_tag("account_get_account_request"), Some(22));
    }

    /// `tag_for_id` maps a known id back to its tag, and the result is a
    /// `&'static str` that compares equal to the same value the codec
    /// composes.
    #[test]
    fn tag_for_id_known_id_returns_static_str() {
        assert_eq!(tag_for_id(0), Some("system_handshake_request"));
        assert_eq!(tag_for_id(2), Some("system_feature_supported_request"));
        assert_eq!(tag_for_id(3), Some("system_feature_supported_response"));
        // The leaked-tag cache hands out the same `&'static str` on
        // repeated lookups for the same id.
        assert!(std::ptr::eq(tag_for_id(2).unwrap(), tag_for_id(2).unwrap()));
    }

    /// Unmapped slots return None; 0xFF is the documented poison slot and
    /// every id past the populated range is also unmapped.
    #[test]
    fn tag_for_id_unmapped_id_returns_none() {
        assert!(tag_for_id(0xFF).is_none());
        assert!(tag_for_id(250).is_none());
    }

    /// `compose_action` and `FrameKind::from_tag` must be exact inverses
    /// for every `FrameKind` variant. This pins the suffix table.
    #[test]
    fn compose_action_round_trips_each_framekind() {
        for kind in [
            FrameKind::Request,
            FrameKind::Response,
            FrameKind::Start,
            FrameKind::Receive,
            FrameKind::Stop,
            FrameKind::Interrupt,
        ] {
            let tag = compose_action("system_feature_supported", kind);
            let (method, parsed_kind) =
                FrameKind::from_tag(&tag).expect("from_tag must parse the composed tag");
            assert_eq!(method, "system_feature_supported");
            assert_eq!(parsed_kind, kind, "round-trip mismatch for {kind:?}");
            assert_eq!(tag, format!("system_feature_supported_{}", kind.suffix()));
        }
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
