use crate::v01::transaction::GenesisHash;
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
pub struct ContextualAlias {
    /// 32-byte context identifier the alias is bound to.
    pub context: [u8; 32],
    /// Ring VRF alias (variable length).
    pub alias: Vec<u8>,
}

/// A single step in a [`RingLocation`] path, addressing a ring within a chain.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RingLocationJunction {
    /// Pallet instance hosting the ring collection.
    PalletInstance(u8),
    /// Ring collection identifier within the pallet.
    CollectionId(Vec<u8>),
}

/// Locates a ring for ring VRF operations using only identifiers that are
/// stable across membership changes.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RingLocation {
    /// Genesis hash of the chain hosting the ring.
    pub chain_id: GenesisHash,
    /// Path addressing the ring within the chain.
    pub junctions: Vec<RingLocationJunction>,
}

/// dotNS product identifier (e.g. `"my-product.dot"`).
pub type ProductId = String;

/// Arbitrary-byte suffix distinguishing contexts within a single product.
pub type ProductProofContextSuffix = Vec<u8>;

/// A product-scoped proof context: a product and a context within it.
///
/// Hashed (with a `product/<ProductId>/` prefix) into the 32-byte context bound
/// to a ring VRF proof, so contexts cannot collide across products and the same
/// member key under different contexts yields unlinkable aliases.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProductProofContext(pub ProductId, pub ProductProofContextSuffix);

/// Request to create a ring VRF proof.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountCreateProofRequest {
    /// Product-scoped context the derived alias is bound to.
    pub context: ProductProofContext,
    /// Ring to generate the proof against; the host selects the member key.
    pub ring_location: RingLocation,
    /// Opaque message bound into the proof.
    pub message: Vec<u8>,
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
    /// The selected member key is not a member of the requested ring.
    NotMember,
    /// User or host rejected.
    Rejected,
    /// Catch-all.
    Unknown { reason: String },
}

/// Error returned when contextual alias derivation fails.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostAccountGetAliasError {
    /// Ring not available at the specified location.
    RingNotFound,
    /// The selected member key is not a member of the requested ring.
    NotMember,
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

/// Request to retrieve the contextual alias for a context and ring.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountGetAliasRequest {
    /// Product-scoped context to derive the alias for.
    pub context: ProductProofContext,
    /// Ring whose member key the host should use; matches `create_proof`.
    pub ring_location: RingLocation,
}

/// Response containing a ring VRF proof and the values needed to verify it
/// against a downstream precompile.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountCreateProofResponse {
    /// Variable-length ring VRF proof bytes.
    pub proof: Vec<u8>,
    /// Alias derived for the request's context.
    pub contextual_alias: ContextualAlias,
    /// Index of the selected member key within the ring.
    pub ring_index: u32,
    /// Ring revision the proof was generated against.
    pub ring_revision: u32,
}

/// Response containing all legacy (user-imported) accounts owned by the user.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostGetLegacyAccountsResponse {
    /// Legacy accounts.
    pub accounts: Vec<LegacyAccount>,
}
