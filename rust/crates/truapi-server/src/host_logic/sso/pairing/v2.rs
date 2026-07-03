//! Host-papp v2 handshake wire types.

use parity_scale_codec::{Decode, Encode};

/// Handshake proposal sent by the host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Proposal {
    pub device: Device,
    pub metadata: Vec<MetadataEntry>,
}

/// Device keys advertised in the handshake proposal.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Device {
    pub statement_account_id: [u8; 32],
    pub encryption_public_key: [u8; 65],
}

/// Metadata key/value entry attached to a handshake proposal.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct MetadataEntry(pub MetadataKey, pub String);

/// Metadata keys understood by the mobile SSO pairing flow.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum MetadataKey {
    Custom(String),
    HostName,
    HostVersion,
    HostIcon,
    PlatformType,
    PlatformVersion,
}

/// Plaintext wallet response after decrypting the pairing statement.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum EncryptedResponse {
    Pending(Status),
    Success(Box<Success>),
    Failed(String),
}

/// Intermediate handshake status emitted before success/failure.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum Status {
    AllowanceAllocation,
}

/// Successful handshake payload used to establish the SSO session.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Success {
    pub identity_account_id: [u8; 32],
    pub root_account_id: [u8; 32],
    pub identity_chat_private_key: [u8; 32],
    pub sso_enc_pub_key: [u8; 65],
    pub device_enc_pub_key: [u8; 65],
    pub root_entropy_source: [u8; 32],
}
