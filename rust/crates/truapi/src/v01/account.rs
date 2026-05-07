use parity_scale_codec::{Decode, Encode};

use super::{Bytes, GenesisHash, Hex};

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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct Account {
    /// The account public key (variable-length bytes).
    pub public_key: PublicKey,
    /// Optional human-readable display name.
    pub name: Option<String>,
}

/// A privacy-preserving alias derived via ring VRF, bound to a specific context.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct ContextualAlias {
    /// 32-byte context identifier.
    pub context: [u8; 32],
    /// Ring VRF alias (variable length).
    pub alias: Vec<u8>,
}

/// Hints for locating a ring on-chain.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct RingLocationHint {
    /// Optional pallet instance index.
    pub pallet_instance: Option<u32>,
}

/// Locates a specific ring on a specific chain for ring VRF operations.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
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

/// Request to create a ring VRF proof for a product account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub struct AccountCreateProofRequest {
    /// Product account that should create the proof.
    pub product_account_id: ProductAccountId,
    /// Ring location to use for proof generation.
    pub ring_location: RingLocation,
    /// Context bytes bound to the proof.
    pub context: Bytes,
}

/// User's authentication state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum AccountConnectionStatus {
    Disconnected,
    Connected,
}

/// Error returned when credential/account requests fail.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum CreateProofError {
    /// Ring not available at the specified location.
    RingNotFound,
    /// User or host rejected.
    Rejected,
    /// Catch-all.
    Unknown { reason: String },
}

pub type HostAccountGetRequest = ProductAccountId;
pub type HostAccountGetResponse = Account;
pub type HostAccountGetError = RequestCredentialsError;
pub type HostAccountGetAliasRequest = ProductAccountId;
pub type HostAccountGetAliasResponse = ContextualAlias;
pub type HostAccountGetAliasError = RequestCredentialsError;
pub type HostAccountCreateProofRequest = AccountCreateProofRequest;
pub type HostAccountCreateProofResponse = RingVrfProof;
pub type HostAccountCreateProofError = CreateProofError;
pub type HostGetNonProductAccountsResponse = Vec<Account>;
pub type HostGetNonProductAccountsError = RequestCredentialsError;
pub type HostAccountConnectionStatusSubscribeItem = AccountConnectionStatus;
