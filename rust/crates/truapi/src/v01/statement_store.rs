use super::ProductAccountId;
use parity_scale_codec::{Decode, Encode};

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
    /// Account that should create the proof.
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

/// 32-byte statement topic.
pub type Topic = [u8; 32];

/// Request to subscribe to statements via a topic filter (RFC 0008).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RemoteStatementStoreSubscribeRequest {
    /// AND: statement must contain every listed topic.
    MatchAll(Vec<Topic>),
    /// OR: statement must contain at least one listed topic.
    MatchAny(Vec<Topic>),
}

/// Page of signed statements delivered by the statement store subscription
/// (RFC 0008). The `is_complete` flag distinguishes the historical-dump phase
/// (`false`) from the live-update phase (`true`).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteStatementStoreSubscribeItem {
    /// Signed statements matching the subscription.
    pub statements: Vec<SignedStatement>,
    /// `false` while the host is still streaming the historical dump (more
    /// pages to follow). `true` once the dump is complete; all subsequent
    /// pages are also `true` and carry only newly-arrived statements.
    pub is_complete: bool,
}

/// Response containing a statement proof.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemoteStatementStoreCreateProofResponse {
    /// Created statement proof.
    pub proof: StatementProof,
}
