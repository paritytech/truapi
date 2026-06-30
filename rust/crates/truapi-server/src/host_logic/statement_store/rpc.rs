//! Statement-store JSON-RPC shapes mirrored from `sp_statement_store`.
//!
//! See the upstream RPC methods plus `TopicFilter` / `StatementEvent` types:
//! <https://github.com/paritytech/polkadot-sdk/blob/f2f3aa6a8fda8ea52282da9711b3c5da4ba82529/substrate/client/rpc-api/src/statement/mod.rs#L19-L117>
//! <https://github.com/paritytech/polkadot-sdk/blob/f2f3aa6a8fda8ea52282da9711b3c5da4ba82529/substrate/primitives/statement-store/src/store_api.rs#L41-L54>
//! <https://github.com/paritytech/polkadot-sdk/blob/f2f3aa6a8fda8ea52282da9711b3c5da4ba82529/substrate/primitives/statement-store/src/store_api.rs#L204-L221>

use serde_json::Value;

use super::StatementStoreParseError;

/// Statement-store RPC method used to open a topic subscription.
pub const SUBSCRIBE_STATEMENT_METHOD: &str = "statement_subscribeStatement";
/// Statement-store RPC method used to close a topic subscription.
pub const UNSUBSCRIBE_STATEMENT_METHOD: &str = "statement_unsubscribeStatement";
/// Statement-store RPC method used to submit a signed statement.
pub const SUBMIT_STATEMENT_METHOD: &str = "statement_submit";
/// Maximum `matchAll` topic count accepted by the statement-store RPC.
pub const MAX_MATCH_ALL_TOPICS: usize = 4;
/// Maximum `matchAny` topic count accepted by the statement-store RPC.
pub const MAX_MATCH_ANY_TOPICS: usize = 128;

/// Decoded `newStatements` subscription notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewStatements {
    /// Remote subscription id included in the notification.
    pub remote_subscription_id: String,
    /// SCALE-encoded signed statements carried by the notification.
    pub statements: Vec<Vec<u8>>,
    /// Optional server-side backlog count.
    pub remaining: Option<u64>,
}

/// Topic filter flavor used by statement-store subscribe requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopicFilterKind {
    /// Require every listed topic to match.
    MatchAll,
    /// Accept any listed topic match.
    MatchAny,
}

/// Parse a statement-store subscription result value.
pub fn parse_new_statements_result(
    remote_subscription_id: String,
    result: &Value,
) -> Result<NewStatements, StatementStoreParseError> {
    if result.get("event").and_then(Value::as_str) != Some("newStatements") {
        return Err(StatementStoreParseError::Malformed(
            "result is not a newStatements event".to_string(),
        ));
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
    let remaining = data
        .get("remaining")
        .map(|value| {
            value.as_u64().ok_or_else(|| {
                StatementStoreParseError::Malformed("remaining is not an integer".to_string())
            })
        })
        .transpose()?;

    Ok(NewStatements {
        remote_subscription_id,
        statements,
        remaining,
    })
}

fn decode_hex(value: &str) -> Result<Vec<u8>, StatementStoreParseError> {
    hex::decode(value.strip_prefix("0x").unwrap_or(value))
        .map_err(|error| StatementStoreParseError::InvalidStatementHex(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dotli_sdk_new_statements_result() {
        let result = serde_json::json!({
            "event": "newStatements",
            "data": {
                "statements": ["0xdeadbeef", "0xcafe"],
                "remaining": 0,
            },
        });

        assert_eq!(
            parse_new_statements_result("remote-sub".to_string(), &result).unwrap(),
            NewStatements {
                remote_subscription_id: "remote-sub".to_string(),
                statements: vec![vec![0xde, 0xad, 0xbe, 0xef], vec![0xca, 0xfe]],
                remaining: Some(0),
            }
        );
    }
}
