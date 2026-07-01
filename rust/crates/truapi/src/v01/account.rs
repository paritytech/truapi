use parity_scale_codec::{Decode, Encode};

/// Identifies a product-specific account by an optional dotNS domain name and a
/// derivation index.
///
/// When `dot_ns_identifier` is `None`, the account resolves against the caller's
/// own product. When it is `Some(domain)` naming a different product, the call is
/// cross-product access and requires the
/// [`ExternalAccount`](crate::v01::RemotePermission::ExternalAccount) permission.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProductAccountId {
    /// Optional dotNS domain name identifier (e.g., `"my-product.dot"`). `None`
    /// targets the caller's own product.
    pub dot_ns_identifier: Option<String>,
    /// Key derivation index for generating product-specific accounts.
    pub derivation_index: u32,
}

/// A user-imported (legacy) account: public key plus an optional user-chosen
/// display name.
///
/// Returned by [`HostGetLegacyAccountsResponse`]. Distinct from
/// [`ProductAccount`], which is protocol-derived and never carries a label.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct LegacyAccount {
    /// The account public key (variable-length bytes).
    pub public_key: Vec<u8>,
    /// Optional user-chosen display name.
    pub name: Option<String>,
}

/// A product account: public key only, no display name.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProductAccount {
    /// The account public key (variable-length bytes).
    pub public_key: Vec<u8>,
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
    /// Account that should create the proof.
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

/// Result of a login request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum HostRequestLoginResponse {
    /// User successfully authenticated.
    Success,
    /// User is already authenticated — no action was taken.
    AlreadyConnected,
    /// User dismissed/rejected the login UI.
    Rejected,
}

/// Request to present the host login flow.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRequestLoginRequest {
    /// Optional human-readable reason shown in the login UI.
    pub reason: Option<String>,
}

/// Login request error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostRequestLoginError {
    /// Catch-all.
    Unknown { reason: String },
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
    /// Account to retrieve.
    pub product_account_id: ProductAccountId,
}

/// Response containing a product-scoped account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountGetResponse {
    /// Retrieved product account.
    pub account: ProductAccount,
}

/// The user's primary DotNS account identity.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostGetUserIdResponse {
    /// The user's primary DotNS username.
    pub primary_username: String,
}

/// Error from [`crate::api::Account::get_user_id`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostGetUserIdError {
    /// User denied the identity disclosure request.
    PermissionDenied,
    /// User is not logged in.
    NotConnected,
    /// Catch-all.
    Unknown { reason: String },
}

/// Request to retrieve a contextual alias for a product account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountGetAliasRequest {
    /// Account to derive the alias for.
    pub product_account_id: ProductAccountId,
}

/// Response containing a ring VRF proof.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountCreateProofResponse {
    /// Variable-length ring VRF proof bytes.
    pub proof: Vec<u8>,
}

/// Response containing all legacy (user-imported) accounts owned by the user.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostGetLegacyAccountsResponse {
    /// Legacy accounts.
    pub accounts: Vec<LegacyAccount>,
}
