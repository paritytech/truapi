//! People-chain statement-store JSON-RPC helpers.
//!
//! The core talks to the statement-store pallet through the host-provided
//! `ChainProvider` JSON-RPC connection. These helpers keep the dotli/
//! `@novasamatech/sdk-statement` request shapes in one place.

use serde_json::Value;
use serde_json::json;
use thiserror::Error;

pub const SUBSCRIBE_STATEMENT_METHOD: &str = "statement_subscribeStatement";
pub const UNSUBSCRIBE_STATEMENT_METHOD: &str = "statement_unsubscribeStatement";
pub const SUBMIT_STATEMENT_METHOD: &str = "statement_submit";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewStatements {
    pub remote_subscription_id: String,
    pub statements: Vec<Vec<u8>>,
    pub remaining: Option<u64>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StatementStoreParseError {
    #[error("invalid json-rpc frame: {0}")]
    InvalidJson(String),
    #[error("invalid statement hex: {0}")]
    InvalidStatementHex(String),
    #[error("malformed statement-store frame: {0}")]
    Malformed(String),
}

pub fn subscribe_match_all_request(id: &str, topics: &[[u8; 32]]) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": SUBSCRIBE_STATEMENT_METHOD,
        "params": [{
            "matchAll": topics.iter().map(hex_topic).collect::<Vec<_>>(),
        }],
    })
    .to_string()
}

pub fn unsubscribe_request(id: &str, remote_subscription_id: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": UNSUBSCRIBE_STATEMENT_METHOD,
        "params": [remote_subscription_id],
    })
    .to_string()
}

pub fn submit_statement_request(id: &str, statement: &[u8]) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": SUBMIT_STATEMENT_METHOD,
        "params": [format!("0x{}", hex::encode(statement))],
    })
    .to_string()
}

pub fn parse_subscribe_ack(
    frame: &str,
    expected_id: &str,
) -> Result<Option<String>, StatementStoreParseError> {
    let value = parse_frame(frame)?;
    if value.get("id").and_then(Value::as_str) != Some(expected_id) {
        return Ok(None);
    }
    if let Some(error) = value.get("error") {
        return Err(StatementStoreParseError::Malformed(
            error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("statement-store subscribe failed")
                .to_string(),
        ));
    }
    let Some(result) = value.get("result").and_then(Value::as_str) else {
        return Err(StatementStoreParseError::Malformed(
            "missing subscribe result".to_string(),
        ));
    };
    Ok(Some(result.to_string()))
}

pub fn parse_new_statements(
    frame: &str,
) -> Result<Option<NewStatements>, StatementStoreParseError> {
    let value = parse_frame(frame)?;
    if value.get("method").and_then(Value::as_str) != Some(SUBSCRIBE_STATEMENT_METHOD) {
        return Ok(None);
    }
    let params = value
        .get("params")
        .ok_or_else(|| StatementStoreParseError::Malformed("missing params".to_string()))?;
    let remote_subscription_id = params
        .get("subscription")
        .and_then(Value::as_str)
        .ok_or_else(|| StatementStoreParseError::Malformed("missing subscription id".to_string()))?
        .to_string();
    let result = params
        .get("result")
        .ok_or_else(|| StatementStoreParseError::Malformed("missing result".to_string()))?;
    if result.get("event").and_then(Value::as_str) != Some("newStatements") {
        return Ok(None);
    }
    let data = result
        .get("data")
        .ok_or_else(|| StatementStoreParseError::Malformed("missing data".to_string()))?;
    let statement_values = data
        .get("statements")
        .and_then(Value::as_array)
        .ok_or_else(|| StatementStoreParseError::Malformed("missing statements".to_string()))?;
    let statements = statement_values
        .iter()
        .map(|value| {
            let Some(hex) = value.as_str() else {
                return Err(StatementStoreParseError::Malformed(
                    "statement is not a hex string".to_string(),
                ));
            };
            decode_hex(hex)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let remaining = match data.get("remaining") {
        Some(value) => Some(value.as_u64().ok_or_else(|| {
            StatementStoreParseError::Malformed("remaining is not an integer".to_string())
        })?),
        None => None,
    };

    Ok(Some(NewStatements {
        remote_subscription_id,
        statements,
        remaining,
    }))
}

fn hex_topic(topic: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(topic))
}

fn parse_frame(frame: &str) -> Result<Value, StatementStoreParseError> {
    serde_json::from_str(frame)
        .map_err(|error| StatementStoreParseError::InvalidJson(error.to_string()))
}

fn decode_hex(value: &str) -> Result<Vec<u8>, StatementStoreParseError> {
    hex::decode(value.strip_prefix("0x").unwrap_or(value))
        .map_err(|error| StatementStoreParseError::InvalidStatementHex(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn builds_match_all_subscribe_request_like_dotli_sdk() {
        let topic = [7u8; 32];
        let request = subscribe_match_all_request("truapi:ss:1", &[topic]);
        let value: Value = serde_json::from_str(&request).unwrap();

        assert_eq!(value["jsonrpc"], "2.0");
        assert_eq!(value["id"], "truapi:ss:1");
        assert_eq!(value["method"], SUBSCRIBE_STATEMENT_METHOD);
        assert_eq!(
            value["params"][0]["matchAll"][0],
            "0x0707070707070707070707070707070707070707070707070707070707070707"
        );
    }

    #[test]
    fn builds_unsubscribe_request_with_remote_id() {
        let request = unsubscribe_request("truapi:ss:2", "remote-sub");
        let value: Value = serde_json::from_str(&request).unwrap();

        assert_eq!(value["method"], UNSUBSCRIBE_STATEMENT_METHOD);
        assert_eq!(value["params"][0], "remote-sub");
    }

    #[test]
    fn builds_submit_request_like_dotli_sdk() {
        let request = submit_statement_request("truapi:ss:3", &[0xde, 0xad, 0xbe, 0xef]);
        let value: Value = serde_json::from_str(&request).unwrap();

        assert_eq!(value["method"], SUBMIT_STATEMENT_METHOD);
        assert_eq!(value["params"][0], "0xdeadbeef");
    }

    #[test]
    fn parses_subscribe_ack_for_expected_request_id() {
        let frame = r#"{"jsonrpc":"2.0","id":"truapi:ss:1","result":"remote-sub"}"#;

        assert_eq!(
            parse_subscribe_ack(frame, "truapi:ss:1").unwrap(),
            Some("remote-sub".to_string())
        );
        assert_eq!(parse_subscribe_ack(frame, "other").unwrap(), None);
    }

    #[test]
    fn maps_subscribe_error_to_malformed_frame() {
        let frame =
            r#"{"jsonrpc":"2.0","id":"truapi:ss:1","error":{"code":-32000,"message":"no peers"}}"#;

        assert_eq!(
            parse_subscribe_ack(frame, "truapi:ss:1").unwrap_err(),
            StatementStoreParseError::Malformed("no peers".to_string())
        );
    }

    #[test]
    fn parses_dotli_sdk_new_statements_notification() {
        let frame = r#"{"jsonrpc":"2.0","method":"statement_subscribeStatement","params":{"subscription":"remote-sub","result":{"event":"newStatements","data":{"statements":["0xdeadbeef","0xcafe"],"remaining":0}}}}"#;

        assert_eq!(
            parse_new_statements(frame).unwrap(),
            Some(NewStatements {
                remote_subscription_id: "remote-sub".to_string(),
                statements: vec![vec![0xde, 0xad, 0xbe, 0xef], vec![0xca, 0xfe]],
                remaining: Some(0),
            })
        );
    }

    #[test]
    fn ignores_non_statement_notifications_and_non_new_statement_events() {
        assert_eq!(
            parse_new_statements(
                r#"{"jsonrpc":"2.0","method":"chainHead_v1_followEvent","params":{}}"#
            )
            .unwrap(),
            None
        );
        assert_eq!(
            parse_new_statements(
                r#"{"jsonrpc":"2.0","method":"statement_subscribeStatement","params":{"subscription":"remote-sub","result":{"event":"other","data":{}}}}"#
            )
            .unwrap(),
            None
        );
    }
}
