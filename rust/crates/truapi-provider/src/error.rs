//! Typed backend errors and JSON-RPC error synthesis for dropped requests.

#[cfg(feature = "smoldot")]
use serde_json::{Value, json};
use truapi::latest::GenericError;

/// Failure modes surfaced by the provider's backends.
///
/// Converts to the trait's [`GenericError`] at the
/// [`ChainProvider`](truapi_platform::ChainProvider) boundary while letting
/// in-crate callers match on the cause (e.g. for retry or telemetry).
///
/// Which variants are constructed depends on the enabled backends and target
/// (e.g. `MissingRuntime` is native-WebSocket only), so the enum as a whole
/// allows dead variants rather than cfg-gating each one.
#[allow(dead_code)]
#[derive(Debug, derive_more::Display)]
pub(crate) enum ProviderError {
    /// No backend is registered — and no bundled network defines — this
    /// genesis hash.
    #[display("no chain registered for genesis 0x{}", hex::encode(genesis))]
    UnknownGenesis {
        /// The queried genesis hash.
        genesis: [u8; 32],
    },
    /// A parachain named a relay that is not a registered light-client chain.
    #[display(
        "relay 0x{} is not a registered light-client chain",
        hex::encode(relay)
    )]
    UnknownRelay {
        /// The relay genesis hash the parachain referenced.
        relay: [u8; 32],
    },
    /// The WebSocket handshake with a remote node failed.
    #[display("WebSocket handshake with {url} failed: {reason}")]
    Handshake {
        /// The node URL.
        url: String,
        /// The underlying failure.
        reason: String,
    },
    /// smoldot rejected the chain spec when adding the chain.
    #[display("failed to add a chain to the light client: {reason}")]
    AddChain {
        /// The underlying failure.
        reason: String,
    },
    /// The native WebSocket backend was called without an ambient tokio
    /// runtime to drive its transport.
    #[display("the WebSocket backend requires an ambient tokio runtime")]
    MissingRuntime,
    /// A transport-level failure (e.g. browser WebSocket creation).
    #[display("{reason}")]
    Transport {
        /// The underlying failure.
        reason: String,
    },
}

impl std::error::Error for ProviderError {}

impl From<ProviderError> for GenericError {
    fn from(error: ProviderError) -> Self {
        GenericError {
            reason: error.to_string(),
        }
    }
}

/// JSON-RPC internal-error code (per the spec's reserved range).
#[cfg(feature = "smoldot")]
const JSON_RPC_INTERNAL_ERROR: i32 = -32603;

/// Build a JSON-RPC error response echoing `request`'s id, or `None` when the
/// request carries no id (a notification, which expects no response) or is not
/// valid JSON.
///
/// The light backend calls this when smoldot refuses a request (a full queue):
/// the connection stays alive, and the consumer correlates responses by id, so
/// without a synthesized error for that id it would wait forever. (The
/// WebSocket backends instead end the whole response stream, since a send
/// failure there means the socket is dead.)
#[cfg(feature = "smoldot")]
pub(crate) fn synthetic_error_frame(request: &str, message: &str) -> Option<String> {
    let value: Value = serde_json::from_str(request).ok()?;
    let id = value.get("id").filter(|id| !id.is_null())?;
    Some(
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": JSON_RPC_INTERNAL_ERROR, "message": message },
        })
        .to_string(),
    )
}

/// Outcome of matching a JSON-RPC response frame against an awaited request id.
#[cfg(feature = "smoldot")]
pub(crate) enum FrameForId {
    /// The frame answers `id` with a string `result`.
    Result(String),
    /// The frame answers `id` with an `error`, or with a `result` that is not a
    /// string.
    Failure(String),
}

/// Match `frame` against the awaited `id`, returning how it answers.
///
/// `None` means the frame belongs to some other request (or is not a response),
/// so the caller keeps waiting. Both answer cases must terminate the wait: a
/// node that rejects the method answers with an `error` and keeps the socket
/// open, so treating an error frame as unmatched would wait forever.
#[cfg(feature = "smoldot")]
pub(crate) fn frame_for_id(frame: &str, id: &str) -> Option<FrameForId> {
    let value: Value = serde_json::from_str(frame).ok()?;
    if value.get("id")?.as_str()? != id {
        return None;
    }
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        return Some(FrameForId::Failure(message.to_owned()));
    }
    match value.get("result").and_then(Value::as_str) {
        Some(result) => Some(FrameForId::Result(result.to_owned())),
        None => Some(FrameForId::Failure(
            "response carried no string result".to_owned(),
        )),
    }
}

#[cfg(all(test, feature = "smoldot"))]
mod tests {
    use super::{FrameForId, frame_for_id, synthetic_error_frame};

    #[test]
    fn a_matching_result_frame_yields_its_string() {
        let frame = frame_for_id(r#"{"id":"db","result":"blob"}"#, "db").expect("the id matches");
        let FrameForId::Result(result) = frame else {
            panic!("expected a result");
        };
        assert_eq!(result, "blob");
    }

    #[test]
    fn a_matching_error_frame_fails_instead_of_waiting() {
        // A node that does not implement the method answers with an error and
        // keeps the socket open, so this must terminate the caller's wait.
        let frame = frame_for_id(
            r#"{"id":"db","error":{"code":-32601,"message":"Method not found"}}"#,
            "db",
        )
        .expect("the id matches");
        let FrameForId::Failure(reason) = frame else {
            panic!("expected a failure");
        };
        assert_eq!(reason, "Method not found");
    }

    #[test]
    fn a_matching_non_string_result_fails() {
        let frame = frame_for_id(r#"{"id":"db","result":null}"#, "db").expect("the id matches");
        assert!(matches!(frame, FrameForId::Failure(_)));
    }

    #[test]
    fn other_ids_and_notifications_are_not_matched() {
        assert!(frame_for_id(r#"{"id":"other","result":"blob"}"#, "db").is_none());
        assert!(frame_for_id(r#"{"method":"chainHead_v1_followEvent"}"#, "db").is_none());
        assert!(frame_for_id("not json", "db").is_none());
    }

    #[test]
    fn echoes_the_request_id() {
        let frame = synthetic_error_frame(
            r#"{"jsonrpc":"2.0","id":7,"method":"x","params":[]}"#,
            "queue full",
        )
        .expect("a request with an id yields an error frame");
        let value: serde_json::Value = serde_json::from_str(&frame).expect("valid JSON");
        assert_eq!(value["id"], 7);
        assert_eq!(value["error"]["code"], -32603);
        assert_eq!(value["error"]["message"], "queue full");
    }

    #[test]
    fn string_ids_are_preserved() {
        let frame =
            synthetic_error_frame(r#"{"id":"abc","method":"x"}"#, "boom").expect("has an id");
        let value: serde_json::Value = serde_json::from_str(&frame).expect("valid JSON");
        assert_eq!(value["id"], "abc");
    }

    #[test]
    fn notifications_and_garbage_yield_nothing() {
        assert!(synthetic_error_frame(r#"{"method":"x","params":[]}"#, "m").is_none());
        assert!(synthetic_error_frame(r#"{"id":null,"method":"x"}"#, "m").is_none());
        assert!(synthetic_error_frame("not json", "m").is_none());
    }
}
