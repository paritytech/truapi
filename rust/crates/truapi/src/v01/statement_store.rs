use parity_scale_codec::{Decode, Encode};

use super::ProductAccountId;

/// Cryptographic proof for a statement.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum StatementProof {
    /// Sr25519 signature proof.
    Sr25519 {
        signature: [u8; 64],
        signer: [u8; 32],
    },
    /// Ed25519 signature proof.
    Ed25519 {
        signature: [u8; 64],
        signer: [u8; 32],
    },
    /// ECDSA signature proof.
    Ecdsa {
        signature: [u8; 65],
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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Statement {
    /// Optional cryptographic proof.
    pub proof: Option<StatementProof>,
    /// Optional decryption key.
    pub decryption_key: Option<[u8; 32]>,
    /// Optional Unix timestamp expiry.
    pub expiry: Option<u64>,
    /// Optional channel.
    pub channel: Option<[u8; 32]>,
    /// [u8; 32] tags.
    pub topics: Vec<[u8; 32]>,
    /// Optional data payload.
    pub data: Option<Vec<u8>>,
}

/// A statement with a required (not optional) proof.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SignedStatement {
    /// Required cryptographic proof.
    pub proof: StatementProof,
    /// Optional decryption key.
    pub decryption_key: Option<[u8; 32]>,
    /// Optional Unix timestamp expiry.
    pub expiry: Option<u64>,
    /// Optional channel.
    pub channel: Option<[u8; 32]>,
    /// [u8; 32] tags.
    pub topics: Vec<[u8; 32]>,
    /// Optional data payload.
    pub data: Option<Vec<u8>>,
}

/// Request to create a cryptographic proof for a statement.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteStatementStoreCreateProofRequest {
    /// Product account that should create the proof.
    pub product_account_id: ProductAccountId,
    /// Statement to prove.
    pub statement: Statement,
}

/// Statement proof creation error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemoteStatementStoreCreateProofError {
    /// Signing operation failed.
    UnableToSign,
    /// Account not recognized.
    UnknownAccount,
    /// Catch-all.
    Unknown { reason: String },
}

/// Request to subscribe to statements matching topics.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteStatementStoreSubscribeRequest {
    /// Required topics.
    pub topics: Vec<[u8; 32]>,
}

/// Item containing statements delivered by the statement store subscription.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteStatementStoreSubscribeItem {
    /// Signed statements matching the subscription.
    pub statements: Vec<SignedStatement>,
}

/// Response containing a statement proof.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteStatementStoreCreateProofResponse {
    /// Created statement proof.
    pub proof: StatementProof,
}
