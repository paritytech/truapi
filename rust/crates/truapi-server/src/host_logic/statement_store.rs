//! People-chain statement-store helpers.
//!
//! The core talks to the statement-store pallet through the host-provided
//! `ChainProvider` JSON-RPC connection. Transport mechanics live in
//! `HostRpcClient`; this module owns statement-store payload encoding,
//! proof verification, and subscription-result parsing.

use thiserror::Error;

mod rpc;
mod statement;

pub use rpc::{
    MAX_MATCH_ALL_TOPICS, MAX_MATCH_ANY_TOPICS, NewStatements, SUBMIT_STATEMENT_METHOD,
    SUBSCRIBE_STATEMENT_METHOD, TopicFilterKind, UNSUBSCRIBE_STATEMENT_METHOD,
    parse_new_statements_result,
};
pub(crate) use statement::current_unix_secs;
pub use statement::{
    StatementField, StatementProof, VerifiedStatementData, build_signed_session_request_statement,
    build_signed_statement, decode_signed_statement, decode_statement_data,
    decode_verified_statement_data, hex_topic, sign_statement_fields, signed_statement_to_scale,
    statement_expiry_elapsed, statement_fields_from_v01, statement_proof_to_v01,
    statement_signing_payload,
};

/// Error while parsing statement-store JSON-RPC or SCALE statement payloads.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum StatementStoreParseError {
    #[error("invalid statement hex: {0}")]
    InvalidStatementHex(String),
    #[error("invalid statement scale: {0}")]
    InvalidStatementScale(String),
    #[error("malformed statement-store frame: {0}")]
    Malformed(String),
    #[error("invalid statement proof: {0}")]
    InvalidStatementProof(String),
}
