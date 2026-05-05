use super::*;
use parity_scale_codec::{Decode, Encode};

#[cfg(feature = "sp-compat")]
mod sp_compat;

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

/// Filter for statement subscriptions, allowing richer topic matching than plain
/// topic vectors. Each position in the filter can be `Some(topic)` to require an
/// exact match or `None` to act as a wildcard.
///
/// Mirrors the `TopicFilter` type from `polkadot-sdk` statement store.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct TopicFilter {
    /// Positional topic matchers. `None` entries act as wildcards.
    pub topics: Vec<Option<Topic>>,
}
