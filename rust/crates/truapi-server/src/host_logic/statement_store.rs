//! People-chain statement-store JSON-RPC helpers.
//!
//! The core talks to the statement-store pallet through the host-provided
//! `ChainProvider` JSON-RPC connection. These helpers keep dotli-compatible
//! request shapes in one place.

use parity_scale_codec::{Compact, Decode, Encode};
use schnorrkel::{PublicKey, SecretKey, Signature};
use serde_json::Value;
use serde_json::json;
use thiserror::Error;
use truapi::v01;

use crate::host_logic::session::SsoSessionInfo;

pub const SUBSCRIBE_STATEMENT_METHOD: &str = "statement_subscribeStatement";
pub const STATEMENT_NOTIFICATION_METHOD: &str = "statement_statement";
pub const UNSUBSCRIBE_STATEMENT_METHOD: &str = "statement_unsubscribeStatement";
pub const SUBMIT_STATEMENT_METHOD: &str = "statement_submit";
pub const MAX_MATCH_ALL_TOPICS: usize = 4;
pub const MAX_MATCH_ANY_TOPICS: usize = 128;
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
    #[error("invalid statement proof: {0}")]
    InvalidStatementProof(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopicFilterKind {
    MatchAll,
    MatchAny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedStatementData {
    pub data: Vec<u8>,
    pub signer: [u8; 32],
    /// Raw `Expiry` field, if present: unix seconds in the upper 32 bits.
    pub expiry: Option<u64>,
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
    subscribe_request(id, TopicFilterKind::MatchAll, topics)
}

pub fn subscribe_match_any_request(id: &str, topics: &[[u8; 32]]) -> String {
    subscribe_request(id, TopicFilterKind::MatchAny, topics)
}

pub fn subscribe_request(id: &str, kind: TopicFilterKind, topics: &[[u8; 32]]) -> String {
    let topics = topics.iter().map(hex_topic).collect::<Vec<_>>();
    match kind {
        TopicFilterKind::MatchAll => json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": SUBSCRIBE_STATEMENT_METHOD,
            "params": [{ "matchAll": topics }],
        }),
        TopicFilterKind::MatchAny => json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": SUBSCRIBE_STATEMENT_METHOD,
            "params": [{ "matchAny": topics }],
        }),
    }
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
    if is_json_rpc_request_echo(&value) {
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

pub fn parse_submit_ack(
    frame: &str,
    expected_id: &str,
) -> Result<Option<()>, StatementStoreParseError> {
    let value = parse_frame(frame)?;
    if value.get("id").and_then(Value::as_str) != Some(expected_id) {
        return Ok(None);
    }
    if is_json_rpc_request_echo(&value) {
        return Ok(None);
    }
    if let Some(error) = value.get("error") {
        return Err(StatementStoreParseError::Malformed(
            error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("statement-store submit failed")
                .to_string(),
        ));
    }
    if value.get("result").is_none() {
        return Err(StatementStoreParseError::Malformed(
            "missing submit result".to_string(),
        ));
    }
    Ok(Some(()))
}

pub fn parse_new_statements(
    frame: &str,
) -> Result<Option<NewStatements>, StatementStoreParseError> {
    let value = parse_frame(frame)?;
    let method = value.get("method").and_then(Value::as_str);
    if method != Some(SUBSCRIBE_STATEMENT_METHOD) && method != Some(STATEMENT_NOTIFICATION_METHOD) {
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
    statement_data_from_fields(decode_statement_fields(statement)?)
}

pub fn decode_verified_statement_data(
    statement: &[u8],
    expected_signer: Option<[u8; 32]>,
) -> Result<VerifiedStatementData, StatementStoreParseError> {
    let fields = decode_statement_fields(statement)?;
    let signer = verify_statement_proof(&fields, expected_signer)?;
    let expiry = fields.iter().find_map(|field| match field {
        StatementField::Expiry(value) => Some(*value),
        _ => None,
    });
    let data = statement_data_from_fields(fields)?;
    Ok(VerifiedStatementData {
        data,
        signer,
        expiry,
    })
}

/// Whether a statement `Expiry` field (unix seconds in the upper 32 bits) is
/// in the past relative to `now_unix_secs`.
pub fn statement_expiry_elapsed(expiry: u64, now_unix_secs: u64) -> bool {
    (expiry >> 32) < now_unix_secs
}

/// Current unix time in seconds, used to stamp outgoing statement expiries
/// and to gate inbound statement freshness. Trusts the local clock on both
/// native and wasm targets.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn current_unix_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Current unix time in seconds on wasm32, sourced from the JS clock.
#[cfg(target_arch = "wasm32")]
pub(crate) fn current_unix_secs() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}

pub fn decode_signed_statement(
    statement: &[u8],
) -> Result<v01::SignedStatement, StatementStoreParseError> {
    signed_statement_from_fields(decode_statement_fields(statement)?)
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

fn decode_statement_fields(
    statement: &[u8],
) -> Result<Vec<StatementField>, StatementStoreParseError> {
    let mut input = statement;
    let fields: Vec<StatementField> = Decode::decode(&mut input)
        .map_err(|err| StatementStoreParseError::InvalidStatementScale(err.to_string()))?;
    if !input.is_empty() {
        return Err(StatementStoreParseError::Malformed(
            "statement has trailing bytes".to_string(),
        ));
    }
    Ok(fields)
}

fn statement_data_from_fields(
    fields: Vec<StatementField>,
) -> Result<Vec<u8>, StatementStoreParseError> {
    fields
        .into_iter()
        .find_map(|field| match field {
            StatementField::Data(value) => Some(value),
            _ => None,
        })
        .ok_or_else(|| StatementStoreParseError::Malformed("statement has no data".to_string()))
}

fn verify_statement_proof(
    fields: &[StatementField],
    expected_signer: Option<[u8; 32]>,
) -> Result<[u8; 32], StatementStoreParseError> {
    let mut proof = None;
    let mut unsigned_fields = Vec::with_capacity(fields.len().saturating_sub(1));
    for field in fields {
        match field {
            StatementField::Proof(StatementProof::Sr25519 { signature, signer }) => {
                if proof.replace((*signature, *signer)).is_some() {
                    return Err(StatementStoreParseError::InvalidStatementProof(
                        "statement has duplicate proof".to_string(),
                    ));
                }
            }
            StatementField::Proof(_) => {
                return Err(StatementStoreParseError::InvalidStatementProof(
                    "statement proof is not sr25519".to_string(),
                ));
            }
            field => unsigned_fields.push(field.clone()),
        }
    }
    let (signature, signer) = proof.ok_or_else(|| {
        StatementStoreParseError::InvalidStatementProof("statement has no proof".to_string())
    })?;
    if let Some(expected) = expected_signer
        && signer != expected
    {
        return Err(StatementStoreParseError::InvalidStatementProof(
            "statement proof signer does not match expected peer".to_string(),
        ));
    }

    unsigned_fields.sort_by_key(statement_field_sort_index);
    let payload =
        statement_signing_payload(&unsigned_fields).map_err(StatementStoreParseError::Malformed)?;
    let public = PublicKey::from_bytes(&signer).map_err(|err| {
        StatementStoreParseError::InvalidStatementProof(format!("invalid sr25519 signer: {err}"))
    })?;
    let signature = Signature::from_bytes(&signature).map_err(|err| {
        StatementStoreParseError::InvalidStatementProof(format!("invalid sr25519 signature: {err}"))
    })?;
    public
        .verify_simple(SR25519_SIGNING_CONTEXT, &payload, &signature)
        .map_err(|err| {
            StatementStoreParseError::InvalidStatementProof(format!(
                "sr25519 signature verification failed: {err}"
            ))
        })?;
    Ok(signer)
}

pub fn statement_fields_from_v01(statement: v01::Statement) -> Result<Vec<StatementField>, String> {
    let mut fields = Vec::new();
    if let Some(proof) = statement.proof {
        fields.push(StatementField::Proof(statement_proof_from_v01(proof)));
    }
    if let Some(decryption_key) = statement.decryption_key {
        fields.push(StatementField::DecryptionKey(decryption_key));
    }
    if let Some(expiry) = statement.expiry {
        fields.push(StatementField::Expiry(expiry));
    }
    if let Some(channel) = statement.channel {
        fields.push(StatementField::Channel(channel));
    }
    push_statement_topics(&mut fields, statement.topics)?;
    if let Some(data) = statement.data {
        fields.push(StatementField::Data(data));
    }
    Ok(fields)
}

pub fn signed_statement_to_scale(statement: v01::SignedStatement) -> Result<Vec<u8>, String> {
    Ok(signed_statement_fields(statement)?.encode())
}

fn signed_statement_fields(statement: v01::SignedStatement) -> Result<Vec<StatementField>, String> {
    let mut fields = vec![StatementField::Proof(statement_proof_from_v01(
        statement.proof,
    ))];
    if let Some(decryption_key) = statement.decryption_key {
        fields.push(StatementField::DecryptionKey(decryption_key));
    }
    if let Some(expiry) = statement.expiry {
        fields.push(StatementField::Expiry(expiry));
    }
    if let Some(channel) = statement.channel {
        fields.push(StatementField::Channel(channel));
    }
    push_statement_topics(&mut fields, statement.topics)?;
    if let Some(data) = statement.data {
        fields.push(StatementField::Data(data));
    }
    fields.sort_by_key(statement_field_sort_index);
    Ok(fields)
}

fn signed_statement_from_fields(
    fields: Vec<StatementField>,
) -> Result<v01::SignedStatement, StatementStoreParseError> {
    let mut proof = None;
    let mut decryption_key = None;
    let mut expiry = None;
    let mut channel = None;
    let mut topics = Vec::new();
    let mut data = None;

    for field in fields {
        match field {
            StatementField::Proof(value) => {
                if proof.replace(statement_proof_to_v01(value)).is_some() {
                    return Err(StatementStoreParseError::Malformed(
                        "statement has duplicate proof".to_string(),
                    ));
                }
            }
            StatementField::DecryptionKey(value) => {
                if decryption_key.replace(value).is_some() {
                    return Err(StatementStoreParseError::Malformed(
                        "statement has duplicate decryption key".to_string(),
                    ));
                }
            }
            StatementField::Expiry(value) => {
                if expiry.replace(value).is_some() {
                    return Err(StatementStoreParseError::Malformed(
                        "statement has duplicate expiry".to_string(),
                    ));
                }
            }
            StatementField::Channel(value) => {
                if channel.replace(value).is_some() {
                    return Err(StatementStoreParseError::Malformed(
                        "statement has duplicate channel".to_string(),
                    ));
                }
            }
            StatementField::Topic1(value)
            | StatementField::Topic2(value)
            | StatementField::Topic3(value)
            | StatementField::Topic4(value) => topics.push(value),
            StatementField::Data(value) => {
                if data.replace(value).is_some() {
                    return Err(StatementStoreParseError::Malformed(
                        "statement has duplicate data".to_string(),
                    ));
                }
            }
        }
    }

    let proof = proof
        .ok_or_else(|| StatementStoreParseError::Malformed("statement has no proof".to_string()))?;
    Ok(v01::SignedStatement {
        proof,
        decryption_key,
        expiry,
        channel,
        topics,
        data,
    })
}

pub fn statement_proof_to_v01(proof: StatementProof) -> v01::StatementProof {
    match proof {
        StatementProof::Sr25519 { signature, signer } => {
            v01::StatementProof::Sr25519 { signature, signer }
        }
        StatementProof::Ed25519 { signature, signer } => {
            v01::StatementProof::Ed25519 { signature, signer }
        }
        StatementProof::Ecdsa { signature, signer } => {
            v01::StatementProof::Ecdsa { signature, signer }
        }
        StatementProof::OnChain {
            who,
            block_hash,
            event,
        } => v01::StatementProof::OnChain {
            who,
            block_hash,
            event,
        },
    }
}

fn statement_proof_from_v01(proof: v01::StatementProof) -> StatementProof {
    match proof {
        v01::StatementProof::Sr25519 { signature, signer } => {
            StatementProof::Sr25519 { signature, signer }
        }
        v01::StatementProof::Ed25519 { signature, signer } => {
            StatementProof::Ed25519 { signature, signer }
        }
        v01::StatementProof::Ecdsa { signature, signer } => {
            StatementProof::Ecdsa { signature, signer }
        }
        v01::StatementProof::OnChain {
            who,
            block_hash,
            event,
        } => StatementProof::OnChain {
            who,
            block_hash,
            event,
        },
    }
}

fn push_statement_topics(
    fields: &mut Vec<StatementField>,
    topics: Vec<[u8; 32]>,
) -> Result<(), String> {
    if topics.len() > 4 {
        return Err(format!(
            "statement has {} topics, maximum is 4",
            topics.len()
        ));
    }
    for (index, topic) in topics.into_iter().enumerate() {
        fields.push(match index {
            0 => StatementField::Topic1(topic),
            1 => StatementField::Topic2(topic),
            2 => StatementField::Topic3(topic),
            3 => StatementField::Topic4(topic),
            _ => unreachable!("topic count checked above"),
        });
    }
    Ok(())
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

fn is_json_rpc_request_echo(value: &Value) -> bool {
    value.get("method").and_then(Value::as_str).is_some()
        && value.get("params").is_some()
        && value.get("result").is_none()
        && value.get("error").is_none()
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
    fn builds_match_any_subscribe_request_like_dotli_sdk() {
        let topic = [8u8; 32];
        let request = subscribe_match_any_request("truapi:ss:any", &[topic]);
        let value: Value = serde_json::from_str(&request).unwrap();

        assert_eq!(value["method"], SUBSCRIBE_STATEMENT_METHOD);
        assert_eq!(
            value["params"][0]["matchAny"][0],
            "0x0808080808080808080808080808080808080808080808080808080808080808"
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
    fn ignores_echoed_subscribe_request_for_expected_request_id() {
        let frame = r#"{"jsonrpc":"2.0","id":"truapi:ss:1","method":"statement_subscribeStatement","params":[{"matchAll":["0x0707070707070707070707070707070707070707070707070707070707070707"]}]}"#;

        assert_eq!(parse_subscribe_ack(frame, "truapi:ss:1").unwrap(), None);
    }

    #[test]
    fn parses_submit_ack_for_expected_request_id() {
        let frame = r#"{"jsonrpc":"2.0","id":"truapi:ss:submit","result":"0xabc"}"#;

        assert_eq!(
            parse_submit_ack(frame, "truapi:ss:submit").unwrap(),
            Some(())
        );
        assert_eq!(parse_submit_ack(frame, "other").unwrap(), None);
    }

    #[test]
    fn ignores_echoed_submit_request_for_expected_request_id() {
        let frame = r#"{"jsonrpc":"2.0","id":"truapi:ss:submit","method":"statement_submit","params":["0xdeadbeef"]}"#;

        assert_eq!(parse_submit_ack(frame, "truapi:ss:submit").unwrap(), None);
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
        let frame = r#"{"jsonrpc":"2.0","method":"statement_statement","params":{"subscription":"remote-sub","result":{"event":"newStatements","data":{"statements":["0xdeadbeef","0xcafe"],"remaining":0}}}}"#;

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
    fn signed_statement_scale_round_trips_public_shape() {
        let signed = v01::SignedStatement {
            proof: v01::StatementProof::Sr25519 {
                signature: [9; 64],
                signer: [8; 32],
            },
            decryption_key: Some([7; 32]),
            expiry: Some(99),
            channel: Some([6; 32]),
            topics: vec![[1; 32], [2; 32]],
            data: Some(vec![3, 4, 5]),
        };

        let encoded = signed_statement_to_scale(signed.clone()).unwrap();

        assert_eq!(decode_signed_statement(&encoded).unwrap(), signed);
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
    fn verified_statement_data_accepts_valid_sr25519_proof() {
        let session = test_session();
        let statement =
            build_signed_session_request_statement(&session, vec![0xde, 0xad], 42).unwrap();

        let verified =
            decode_verified_statement_data(&statement, Some(session.ss_public_key)).unwrap();

        assert_eq!(
            verified,
            VerifiedStatementData {
                data: vec![0xde, 0xad],
                signer: session.ss_public_key,
                expiry: Some(42),
            }
        );
    }

    #[test]
    fn verified_statement_data_rejects_tampered_signature() {
        let session = test_session();
        let statement =
            build_signed_session_request_statement(&session, vec![0xde, 0xad], 42).unwrap();
        let mut fields = Vec::<StatementField>::decode(&mut statement.as_slice()).unwrap();
        let StatementField::Proof(StatementProof::Sr25519 { signature, .. }) = &mut fields[0]
        else {
            panic!("expected sr25519 proof");
        };
        signature[0] ^= 0xff;

        let err = decode_verified_statement_data(&fields.encode(), Some(session.ss_public_key))
            .unwrap_err();

        assert!(
            matches!(err, StatementStoreParseError::InvalidStatementProof(reason) if reason.contains("signature verification failed"))
        );
    }

    #[test]
    fn verified_statement_data_rejects_wrong_expected_signer() {
        let session = test_session();
        let statement =
            build_signed_session_request_statement(&session, vec![0xde, 0xad], 42).unwrap();

        assert_eq!(
            decode_verified_statement_data(&statement, Some([0xaa; 32])).unwrap_err(),
            StatementStoreParseError::InvalidStatementProof(
                "statement proof signer does not match expected peer".to_string()
            )
        );
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
