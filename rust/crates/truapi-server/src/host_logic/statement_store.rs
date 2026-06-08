//! People-chain statement-store JSON-RPC helpers.
//!
//! The core talks to the statement-store pallet through the host-provided
//! `ChainProvider` JSON-RPC connection. These helpers keep the dotli/
//! `@novasamatech/sdk-statement` request shapes in one place.

use parity_scale_codec::{Compact, Decode, Encode};
use schnorrkel::SecretKey;
use serde_json::Value;
use serde_json::json;
use thiserror::Error;

use crate::host_logic::session::SsoSessionInfo;

pub const SUBSCRIBE_STATEMENT_METHOD: &str = "statement_subscribeStatement";
pub const UNSUBSCRIBE_STATEMENT_METHOD: &str = "statement_unsubscribeStatement";
pub const SUBMIT_STATEMENT_METHOD: &str = "statement_submit";
const SR25519_SIGNING_CONTEXT: &[u8] = b"substrate";

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
    #[error("invalid statement scale: {0}")]
    InvalidStatementScale(String),
    #[error("malformed statement-store frame: {0}")]
    Malformed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum StatementProof {
    #[codec(index = 0)]
    Sr25519 {
        signature: [u8; 64],
        signer: [u8; 32],
    },
    #[codec(index = 1)]
    Ed25519 {
        signature: [u8; 64],
        signer: [u8; 32],
    },
    #[codec(index = 2)]
    Ecdsa {
        signature: [u8; 65],
        signer: [u8; 33],
    },
    #[codec(index = 3)]
    OnChain {
        who: [u8; 32],
        block_hash: [u8; 32],
        event: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum StatementField {
    #[codec(index = 0)]
    Proof(StatementProof),
    #[codec(index = 1)]
    DecryptionKey([u8; 32]),
    #[codec(index = 2)]
    Expiry(u64),
    #[codec(index = 3)]
    Channel([u8; 32]),
    #[codec(index = 4)]
    Topic1([u8; 32]),
    #[codec(index = 5)]
    Topic2([u8; 32]),
    #[codec(index = 6)]
    Topic3([u8; 32]),
    #[codec(index = 7)]
    Topic4([u8; 32]),
    #[codec(index = 8)]
    Data(Vec<u8>),
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

pub fn decode_statement_data(statement: &[u8]) -> Result<Vec<u8>, StatementStoreParseError> {
    let mut input = statement;
    let fields: Vec<StatementField> = Decode::decode(&mut input)
        .map_err(|err| StatementStoreParseError::InvalidStatementScale(err.to_string()))?;
    if !input.is_empty() {
        return Err(StatementStoreParseError::Malformed(
            "statement has trailing bytes".to_string(),
        ));
    }
    fields
        .into_iter()
        .find_map(|field| match field {
            StatementField::Data(value) => Some(value),
            _ => None,
        })
        .ok_or_else(|| StatementStoreParseError::Malformed("statement has no data".to_string()))
}

pub fn build_signed_session_request_statement(
    session: &SsoSessionInfo,
    encrypted_data: Vec<u8>,
    expiry: u64,
) -> Result<Vec<u8>, String> {
    build_signed_statement(
        session,
        session.request_channel,
        session.session_id_own,
        encrypted_data,
        expiry,
    )
}

pub fn build_signed_statement(
    session: &SsoSessionInfo,
    channel: [u8; 32],
    topic1: [u8; 32],
    data: Vec<u8>,
    expiry: u64,
) -> Result<Vec<u8>, String> {
    let fields = vec![
        StatementField::Expiry(expiry),
        StatementField::Channel(channel),
        StatementField::Topic1(topic1),
        StatementField::Data(data),
    ];
    sign_statement_fields(session.ss_secret, session.ss_public_key, fields)
        .map(|fields| fields.encode())
}

pub fn sign_statement_fields(
    ss_secret: [u8; 64],
    expected_public_key: [u8; 32],
    mut fields: Vec<StatementField>,
) -> Result<Vec<StatementField>, String> {
    if fields
        .iter()
        .any(|field| matches!(field, StatementField::Proof(_)))
    {
        return Err("statement is already signed".to_string());
    }
    fields.sort_by_key(statement_field_sort_index);

    let secret =
        SecretKey::from_bytes(&ss_secret).map_err(|err| format!("invalid ss_secret: {err}"))?;
    let public = secret.to_public();
    if public.to_bytes() != expected_public_key {
        return Err("ss_secret does not match session statement public key".to_string());
    }

    let signing_payload = statement_signing_payload(&fields)?;
    let signature = secret
        .sign_simple(SR25519_SIGNING_CONTEXT, &signing_payload, &public)
        .to_bytes();

    let mut signed = Vec::with_capacity(fields.len() + 1);
    signed.push(StatementField::Proof(StatementProof::Sr25519 {
        signature,
        signer: expected_public_key,
    }));
    signed.extend(fields);
    Ok(signed)
}

pub fn statement_signing_payload(fields: &[StatementField]) -> Result<Vec<u8>, String> {
    let encoded = fields.to_vec().encode();
    let mut input = encoded.as_slice();
    let _: Compact<u32> =
        Decode::decode(&mut input).map_err(|err| format!("invalid statement vector: {err}"))?;
    let compact_len = encoded.len() - input.len();
    Ok(encoded[compact_len..].to_vec())
}

fn statement_field_sort_index(field: &StatementField) -> u8 {
    match field {
        StatementField::Proof(_) => 0,
        StatementField::DecryptionKey(_) => 1,
        StatementField::Expiry(_) => 2,
        StatementField::Channel(_) => 3,
        StatementField::Topic1(_) => 4,
        StatementField::Topic2(_) => 5,
        StatementField::Topic3(_) => 6,
        StatementField::Topic4(_) => 7,
        StatementField::Data(_) => 8,
    }
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
    use crate::host_logic::session::SsoSessionInfo;
    use schnorrkel::{ExpansionMode, MiniSecretKey, PublicKey, Signature};
    use serde_json::Value;

    fn test_session() -> SsoSessionInfo {
        let mini_secret = MiniSecretKey::from_bytes(&[7; 32]).unwrap();
        let keypair = mini_secret.expand_to_keypair(ExpansionMode::Ed25519);
        SsoSessionInfo {
            ss_secret: keypair.secret.to_bytes(),
            ss_public_key: keypair.public.to_bytes(),
            enc_secret: [1; 32],
            peer_enc_pubkey: [2; 65],
            identity_account_id: [3; 32],
            session_id_own: [4; 32],
            session_id_peer: [5; 32],
            request_channel: [6; 32],
            response_channel: [7; 32],
            peer_request_channel: [8; 32],
        }
    }

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

    #[test]
    fn decodes_statement_data_field() {
        let statement = vec![
            StatementField::Proof(StatementProof::Sr25519 {
                signature: [1; 64],
                signer: [2; 32],
            }),
            StatementField::Expiry(42),
            StatementField::Channel([3; 32]),
            StatementField::Topic1([4; 32]),
            StatementField::Data(vec![0xde, 0xad, 0xbe, 0xef]),
        ]
        .encode();

        assert_eq!(
            decode_statement_data(&statement).unwrap(),
            vec![0xde, 0xad, 0xbe, 0xef]
        );
    }

    #[test]
    fn signing_payload_strips_scale_vec_compact_len() {
        let fields = vec![
            StatementField::Expiry(42),
            StatementField::Channel([3; 32]),
            StatementField::Topic1([4; 32]),
            StatementField::Data(vec![0xde, 0xad, 0xbe, 0xef]),
        ];
        let encoded = fields.encode();

        assert_eq!(encoded[0], 16);
        assert_eq!(statement_signing_payload(&fields).unwrap(), encoded[1..]);
    }

    #[test]
    fn builds_signed_session_request_statement() {
        let session = test_session();

        let statement =
            build_signed_session_request_statement(&session, vec![0xde, 0xad], 42).unwrap();
        let mut input = statement.as_slice();
        let fields = Vec::<StatementField>::decode(&mut input).unwrap();

        assert!(input.is_empty());
        assert_eq!(fields.len(), 5);
        let StatementField::Proof(StatementProof::Sr25519 { signature, signer }) = fields[0] else {
            panic!("expected sr25519 proof");
        };
        assert_eq!(signer, session.ss_public_key);
        assert_eq!(fields[1], StatementField::Expiry(42));
        assert_eq!(fields[2], StatementField::Channel(session.request_channel));
        assert_eq!(fields[3], StatementField::Topic1(session.session_id_own));
        assert_eq!(fields[4], StatementField::Data(vec![0xde, 0xad]));

        let payload = statement_signing_payload(&fields[1..]).unwrap();
        let public = PublicKey::from_bytes(&signer).unwrap();
        let signature = Signature::from_bytes(&signature).unwrap();
        public
            .verify_simple(SR25519_SIGNING_CONTEXT, &payload, &signature)
            .unwrap();
    }

    #[test]
    fn signing_rejects_mismatched_session_key_material() {
        let mut session = test_session();
        session.ss_public_key = [0xff; 32];

        assert_eq!(
            build_signed_session_request_statement(&session, vec![0xde], 42).unwrap_err(),
            "ss_secret does not match session statement public key"
        );
    }

    #[test]
    fn signing_rejects_already_signed_statements() {
        let session = test_session();
        let fields = vec![StatementField::Proof(StatementProof::Sr25519 {
            signature: [1; 64],
            signer: session.ss_public_key,
        })];

        assert_eq!(
            sign_statement_fields(session.ss_secret, session.ss_public_key, fields).unwrap_err(),
            "statement is already signed"
        );
    }

    #[test]
    fn rejects_statement_without_data_field() {
        let statement = vec![StatementField::Expiry(42)].encode();

        assert_eq!(
            decode_statement_data(&statement).unwrap_err(),
            StatementStoreParseError::Malformed("statement has no data".to_string())
        );
    }
}
