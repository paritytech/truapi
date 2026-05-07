use parity_scale_codec::{Decode, Encode};

/// Identifies a product-specific account by combining a dotNS domain name with a
/// derivation index.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProductAccountId {
    /// A dotNS domain name identifier (e.g., `"my-product.dot"`).
    pub dot_ns_identifier: String,
    /// Key derivation index for generating product-specific accounts.
    pub derivation_index: u32,
}

/// An account with its public key and optional display name.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Account {
    /// The account public key (variable-length bytes).
    pub public_key: Vec<u8>,
    /// Optional human-readable display name.
    pub name: Option<String>,
}

/// A privacy-preserving alias derived via ring VRF, bound to a specific context.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountGetAliasResponse {
    /// 32-byte context identifier.
    pub context: [u8; 32],
    /// Ring VRF alias (variable length).
    pub alias: Vec<u8>,
}

/// Hints for locating a ring on-chain.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RingLocationHint {
    /// Optional pallet instance index.
    pub pallet_instance: Option<u32>,
}

/// Locates a specific ring on a specific chain for ring VRF operations.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RingLocation {
    /// Chain genesis hash.
    pub genesis_hash: Vec<u8>,
    /// Root hash of the ring.
    pub ring_root_hash: Vec<u8>,
    /// Optional location hints.
    pub hints: Option<RingLocationHint>,
}

/// Request to create a ring VRF proof for a product account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountCreateProofRequest {
    /// Product account that should create the proof.
    pub product_account_id: ProductAccountId,
    /// Ring location to use for proof generation.
    pub ring_location: RingLocation,
    /// Context bytes bound to the proof.
    pub context: Vec<u8>,
}

/// User's authentication state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum HostAccountConnectionStatusSubscribeItem {
    Disconnected,
    Connected,
}

/// Error returned when credential/account requests fail.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostAccountGetError {
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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostAccountCreateProofError {
    /// Ring not available at the specified location.
    RingNotFound,
    /// User or host rejected.
    Rejected,
    /// Catch-all.
    Unknown { reason: String },
}

/// Request to retrieve a product-scoped account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountGetRequest {
    /// Product account to retrieve.
    pub product_account_id: ProductAccountId,
}

/// Response containing a product-scoped account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountGetResponse {
    /// Retrieved account.
    pub account: Account,
}

/// Request to retrieve a contextual alias for a product account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountGetAliasRequest {
    /// Product account to derive the alias for.
    pub product_account_id: ProductAccountId,
}

/// Response containing a ring VRF proof.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountCreateProofResponse {
    /// Variable-length ring VRF proof bytes.
    pub proof: Vec<u8>,
}

/// Response containing all non-product accounts owned by the user.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostGetNonProductAccountsResponse {
    /// Non-product accounts.
    pub accounts: Vec<Account>,
}
