use super::*;

/// 32-byte account identifier (typically an SS58 public key).
pub type AccountId = [u8; 32];

/// Variable-length public key.
pub type PublicKey = Vec<u8>;

/// A dotNS domain name identifier (e.g., `"my-product.dot"`).
pub type DotNsIdentifier = String;

/// Key derivation index for generating product-specific accounts.
pub type DerivationIndex = u32;

/// Identifies a product-specific account by combining a dotNS domain name with a
/// derivation index.
pub type ProductAccountId = (DotNsIdentifier, DerivationIndex);

/// An account with its public key and optional display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The account public key (variable-length bytes).
    pub public_key: PublicKey,
    /// Optional human-readable display name.
    pub name: Option<String>,
}

/// A privacy-preserving alias derived via ring VRF, bound to a specific context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextualAlias {
    /// 32-byte context identifier.
    pub context: [u8; 32],
    /// Ring VRF alias (variable length).
    pub alias: Vec<u8>,
}

/// Hints for locating a ring on-chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RingLocationHint {
    /// Optional pallet instance index.
    pub pallet_instance: Option<u32>,
}

/// Locates a specific ring on a specific chain for ring VRF operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RingLocation {
    /// Chain genesis hash.
    pub genesis_hash: GenesisHash,
    /// Root hash of the ring.
    pub ring_root_hash: Hex,
    /// Optional location hints.
    pub hints: Option<RingLocationHint>,
}

/// Variable-length ring VRF proof bytes.
pub type RingVrfProof = Vec<u8>;

/// User's authentication state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountConnectionStatus {
    Disconnected,
    Connected,
}

/// The user's primary DotNS account identity.
///
/// V0.2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserIdentity {
    /// The user's primary DotNS identifier.
    pub dot_ns_identifier: DotNsIdentifier,
    /// The user's primary public key.
    pub public_key: PublicKey,
}

/// Error from [`AccountManagement::host_get_user_id`].
///
/// V0.2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserIdentityError {
    /// User denied the identity disclosure request.
    Rejected,
    /// User is not logged in.
    NotConnected,
    /// Catch-all.
    Unknown { reason: String },
}

/// Error returned when credential/account requests fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestCredentialsError {
    /// User is not logged in.
    NotConnected,
    /// User or host rejected the request.
    Rejected,
    /// Domain identifier is invalid.
    DomainNotValid,
    /// Catch-all error with reason.
    Unknown { reason: String },
}

/// Error returned when ring VRF proof creation fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateProofError {
    /// Ring not available at the specified location.
    RingNotFound,
    /// User or host rejected.
    Rejected,
    /// Catch-all.
    Unknown { reason: String },
}
