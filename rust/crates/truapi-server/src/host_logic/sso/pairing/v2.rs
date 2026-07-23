//! Host-papp v2 handshake wire types.
//!
//! Host-spec B.1 defines the pairing handshake shape that this deployed v2
//! codec implements as a wire-compatible host-papp dialect:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L24-L103>

use parity_scale_codec::{Decode, Encode};

/// Handshake proposal sent by the host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Proposal {
    /// Keys the pairing host advertises for the handshake.
    pub device: Device,
    /// Display metadata describing the proposing host.
    pub metadata: Vec<MetadataEntry>,
}

/// Device keys advertised in the handshake proposal.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Device {
    /// Pairing host's sr25519 statement-store public key; keys the answer topic.
    pub statement_account_id: [u8; 32],
    /// Pairing host's SEC1 uncompressed P-256 key the wallet encrypts the answer to.
    pub encryption_public_key: [u8; 65],
}

/// Metadata key/value entry attached to a handshake proposal.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct MetadataEntry(pub MetadataKey, pub String);

/// Metadata keys understood by the mobile SSO pairing flow.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum MetadataKey {
    /// Free-form key outside the well-known set.
    Custom(String),
    /// Human-readable host name shown on the pairing prompt.
    HostName,
    /// Host software version.
    HostVersion,
    /// Host icon URL.
    HostIcon,
    /// Platform kind, such as a browser or OS name.
    PlatformType,
    /// Platform version.
    PlatformVersion,
}

/// Plaintext wallet response after decrypting the pairing statement.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum EncryptedResponse {
    /// Wallet accepted the proposal but is still preparing the session.
    Pending(Status),
    /// Wallet approved pairing and shares the session establishment material.
    Success(Box<Success>),
    /// Wallet rejected or aborted pairing, with a human-readable reason.
    Failed(String),
}

/// Intermediate handshake status emitted before success/failure.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum Status {
    /// Wallet is allocating statement-store allowance before answering.
    AllowanceAllocation,
}

/// Successful handshake payload used to establish the SSO session.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Success {
    /// User identity sr25519 account id, used for username lookup and chat addressing.
    pub identity_account_id: [u8; 32],
    /// User root sr25519 public key; parent for soft-derived product accounts.
    pub root_account_id: [u8; 32],
    /// User identity chat P-256 private scalar; lets this device decrypt identity chat.
    pub identity_chat_private_key: [u8; 32],
    /// Wallet's persistent P-256 public key; keys the SSO session channels.
    pub sso_enc_pub_key: [u8; 65],
    /// P-256 public key of the answering wallet device, for chat envelopes addressed back to it.
    pub device_enc_pub_key: [u8; 65],
    /// Wallet-derived source for deterministic product entropy, never the raw root secret.
    pub root_entropy_source: [u8; 32],
}
