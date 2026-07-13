//! JSON-RPC error synthesis for dropped requests.

use serde_json::{Value, json};

/// JSON-RPC internal-error code (per the spec's reserved range).
const JSON_RPC_INTERNAL_ERROR: i32 = -32603;

/// Build a JSON-RPC error response echoing `request`'s id, or `None` when the
/// request carries no id (a notification, which expects no response) or is not
/// valid JSON.
///
/// A backend calls this when it has to drop a request it cannot deliver (e.g. a
/// full send queue): the consumer correlates responses by id, so without a
/// synthesized error for that id it would wait forever.
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

#[cfg(test)]
mod tests {
    use super::synthetic_error_frame;

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
