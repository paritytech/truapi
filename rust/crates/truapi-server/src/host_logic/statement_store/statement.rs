use parity_scale_codec::{Compact, Decode, Encode};
use schnorrkel::{PublicKey, SecretKey, Signature};
use truapi::v01;

use super::StatementStoreParseError;
use crate::host_logic::session::SsoSessionInfo;

const SR25519_SIGNING_CONTEXT: &[u8] = b"substrate";

/// Verified statement payload plus the sr25519 signer recovered from proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedStatementData {
    /// Raw statement data field.
    pub data: Vec<u8>,
    /// Sr25519 signer recovered from the proof.
    pub signer: [u8; 32],
    /// Raw `Expiry` field, if present: unix seconds in the upper 32 bits.
    pub expiry: Option<u64>,
}

/// SCALE statement proof variants mirrored from `sp_statement_store::Proof`.
///
/// See the current upstream `Proof` codec:
/// <https://github.com/paritytech/polkadot-sdk/blob/f2f3aa6a8fda8ea52282da9711b3c5da4ba82529/substrate/primitives/statement-store/src/lib.rs#L273-L299>
///
/// `OnChain` is retained for v01 wire compatibility with older
/// statement-store bytes:
/// <https://github.com/paritytech/polkadot-sdk/blob/7d525248d594c79dcc5e30217becbd56d2fcda40/substrate/primitives/statement-store/src/lib.rs#L260-L289>
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum StatementProof {
    Sr25519 {
        signature: [u8; 64],
        signer: [u8; 32],
    },
    Ed25519 {
        signature: [u8; 64],
        signer: [u8; 32],
    },
    Ecdsa {
        signature: [u8; 65],
        signer: [u8; 33],
    },
    OnChain {
        who: [u8; 32],
        block_hash: [u8; 32],
        event: u64,
    },
}

/// SCALE statement field variants mirrored from `sp_statement_store::Field`.
///
/// See the upstream statement field vector codec:
/// <https://github.com/paritytech/polkadot-sdk/blob/f2f3aa6a8fda8ea52282da9711b3c5da4ba82529/substrate/primitives/statement-store/src/lib.rs#L314-L337>
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum StatementField {
    Proof(StatementProof),
    DecryptionKey([u8; 32]),
    Expiry(u64),
    Channel([u8; 32]),
    Topic1([u8; 32]),
    Topic2([u8; 32]),
    Topic3([u8; 32]),
    Topic4([u8; 32]),
    Data(Vec<u8>),
}

/// Extract the raw `Data` field from a SCALE-encoded statement.
pub fn decode_statement_data(statement: &[u8]) -> Result<Vec<u8>, StatementStoreParseError> {
    statement_data_from_fields(decode_statement_fields(statement)?)
}

/// Verify statement proof and extract signer, expiry, and raw `Data` field.
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

/// Decode a SCALE signed statement into the public v01 statement shape.
pub fn decode_signed_statement(
    statement: &[u8],
) -> Result<v01::SignedStatement, StatementStoreParseError> {
    signed_statement_from_fields(decode_statement_fields(statement)?)
}

/// Build a signed statement on the active SSO request channel.
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

/// Build a signed statement for an arbitrary channel/topic pair.
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

/// Sort fields, insert an sr25519 proof, and return signed fields.
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

/// Build the statement signing payload from sorted fields.
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

/// Convert a public v01 statement into SCALE statement fields.
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

/// Convert a public v01 signed statement into SCALE bytes.
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

/// Convert an internal proof into the public v01 proof shape.
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
    // Keep in sync with upstream `sp_statement_store::Field` discriminants:
    // https://github.com/paritytech/polkadot-sdk/blob/f2f3aa6a8fda8ea52282da9711b3c5da4ba82529/substrate/primitives/statement-store/src/lib.rs#L314-L337
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

/// Format a 32-byte statement-store topic as `0x`-prefixed hex.
pub fn hex_topic(topic: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(topic))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_logic::session::SsoSessionInfo;
    use schnorrkel::{ExpansionMode, MiniSecretKey, PublicKey, Signature};

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
