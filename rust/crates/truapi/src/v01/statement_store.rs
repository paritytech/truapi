use parity_scale_codec::{Decode, Encode};

use super::{Bytes, ProductAccountId};

/// 32-byte topic identifier.
pub type Topic = [u8; 32];

/// 32-byte channel identifier.
pub type Channel = [u8; 32];

/// 32-byte decryption key.
pub type DecryptionKey = [u8; 32];

/// Cryptographic proof for a statement.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum StatementProof {
    /// Sr25519 signature proof.
    Sr25519 {
        #[serde(serialize_with = "crate::serde_helpers::hex_bytes")]
        signature: [u8; 64],
        signer: [u8; 32],
    },
    /// Ed25519 signature proof.
    Ed25519 {
        #[serde(serialize_with = "crate::serde_helpers::hex_bytes")]
        signature: [u8; 64],
        signer: [u8; 32],
    },
    /// ECDSA signature proof.
    Ecdsa {
        #[serde(serialize_with = "crate::serde_helpers::hex_bytes")]
        signature: [u8; 65],
        #[serde(serialize_with = "crate::serde_helpers::hex_bytes")]
        signer: [u8; 33],
    },
    /// On-chain event proof.
    OnChain {
        who: [u8; 32],
        block_hash: [u8; 32],
        event: u64,
    },
}

/// A statement with optional proof and metadata.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct Statement {
    /// Optional cryptographic proof.
    pub proof: Option<StatementProof>,
    /// Optional decryption key.
    pub decryption_key: Option<DecryptionKey>,
    /// Optional Unix timestamp expiry.
    pub expiry: Option<u64>,
    /// Optional channel.
    pub channel: Option<Channel>,
    /// Topic tags.
    pub topics: Vec<Topic>,
    /// Optional data payload.
    pub data: Option<Bytes>,
}

/// A statement with a required (not optional) proof.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct SignedStatement {
    /// Required cryptographic proof.
    pub proof: StatementProof,
    /// Optional decryption key.
    pub decryption_key: Option<DecryptionKey>,
    /// Optional Unix timestamp expiry.
    pub expiry: Option<u64>,
    /// Optional channel.
    pub channel: Option<Channel>,
    /// Topic tags.
    pub topics: Vec<Topic>,
    /// Optional data payload.
    pub data: Option<Bytes>,
}

/// Request to create a cryptographic proof for a statement.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct StatementStoreCreateProofRequest {
    /// Product account that should create the proof.
    pub product_account_id: ProductAccountId,
    /// Statement to prove.
    pub statement: Statement,
}

/// Statement proof creation error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum StatementProofError {
    /// Signing operation failed.
    UnableToSign,
    /// Account not recognized.
    UnknownAccount,
    /// Catch-all.
    Unknown { reason: String },
}

pub type RemoteStatementStoreSubscribeRequest = Vec<Topic>;
pub type RemoteStatementStoreSubscribeItem = Vec<SignedStatement>;
pub type RemoteStatementStoreCreateProofRequest = StatementStoreCreateProofRequest;
pub type RemoteStatementStoreCreateProofResponse = StatementProof;
pub type RemoteStatementStoreCreateProofError = StatementProofError;
pub type RemoteStatementStoreSubmitRequest = Bytes;
pub type RemoteStatementStoreSubmitResponse = String;
pub type RemoteStatementStoreSubmitError = super::GenericError;
