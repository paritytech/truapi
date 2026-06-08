//! People-chain statement-store JSON-RPC helpers.
//!
//! The core talks to the statement-store pallet through the host-provided
//! `ChainProvider` JSON-RPC connection. These helpers keep the dotli/
//! `@novasamatech/sdk-statement` request shapes in one place.

use serde_json::json;

pub const SUBSCRIBE_STATEMENT_METHOD: &str = "statement_subscribeStatement";
pub const UNSUBSCRIBE_STATEMENT_METHOD: &str = "statement_unsubscribeStatement";
pub const SUBMIT_STATEMENT_METHOD: &str = "statement_submit";

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

fn hex_topic(topic: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(topic))
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
}
