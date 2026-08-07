use crate::v01::transaction::GenesisHash;
use parity_scale_codec::{Decode, Encode};

/// Account selector within a product subtree. Encodes as
/// `Either<u32, [u8; 32]>` on the wire (`Index` = left, `Raw` = right).
///
/// `Index` is the primary form — plain indices keep a product's accounts
/// enumerable. `Raw` carries a raw 32-byte derivation index for cases where
/// bytes are genuinely necessary. Hosts expand `Index(n)` to the internal
/// 32-byte index (`u32` little-endian plus the index magic).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum DerivationIndex {
    /// Plain account index.
    Index(u32),
    /// Raw 32-byte derivation index.
    Raw([u8; 32]),
}

/// Identifies a product-specific account by combining a dotNS domain name with a
/// derivation index.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ProductAccountId {
    /// A dotNS domain name identifier (e.g., `"my-product.dot"`).
    pub dot_ns_identifier: String,
    /// Account selector within the product subtree.
    pub derivation_index: DerivationIndex,
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum RingLocationJunction {
    /// Pallet instance hosting the ring collection.
    PalletInstance(u8),
    /// Ring collection identifier within the pallet.
    CollectionId(Vec<u8>),
}

/// Locates a ring for ring VRF operations using only identifiers that are
/// stable across membership changes.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct RingLocation {
    /// Genesis hash of the chain hosting the ring.
    pub chain_id: GenesisHash,
    /// Path addressing the ring within the chain.
    pub junctions: Vec<RingLocationJunction>,
}

/// A product-scoped proof context: a product and a context within it.
///
/// Hashed (with a `product/<product_id>/` prefix) into the 32-byte context bound
/// to a ring VRF proof, so contexts cannot collide across products and the same
/// member key under different contexts yields unlinkable aliases.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ProductProofContext {
    /// dotNS product identifier (e.g. `"my-product.dot"`) scoping the context.
    pub product_id: String,
    /// Selector distinguishing contexts within the product; expands to the
    /// same 32-byte derivation index as [`ProductAccountId::derivation_index`].
    pub suffix: DerivationIndex,
}

/// Request to create a ring VRF proof.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountCreateProofRequest {
    /// Ring-VRF key handle naming the member key to use.
    pub key_handle: ProductAccountId,
    /// Product-scoped context the derived alias is bound to.
    pub context: ProductProofContext,
    /// Ring to generate the proof against.
    pub ring_location: RingLocation,
    /// Opaque message bound into the proof.
    pub message: Vec<u8>,
}

/// Ring-VRF member public key.
pub type RingVrfPublicKey = [u8; 32];

/// A registered ring-VRF key entry.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RegisteredRingVrfKey {
    /// Stable public name of the key.
    pub handle: ProductAccountId,
    /// Rings the owning product declared this key for.
    pub rings: Vec<RingLocation>,
    /// Present when the caller owns the key or requested/granted disclosure.
    pub public_key: Option<RingVrfPublicKey>,
}

/// How much of a registry entry the caller asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum RingVrfKeyDisclosure {
    /// Handle and declared rings only.
    Anonymized,
    /// Include the member public key.
    PublicKey,
}

/// Request to register a ring-VRF key owned by the calling product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountRegisterRingVrfKeyRequest {
    /// Key derivation index within the caller's ring-VRF domain.
    pub index: DerivationIndex,
    /// Ring this key is declared for.
    pub ring: RingLocation,
}

/// Request to list registered ring-VRF keys for an owner product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountListRingVrfKeysRequest {
    /// Product whose registry entries should be listed.
    pub owner: String,
    /// Disclosure level requested by the caller.
    pub disclosure: RingVrfKeyDisclosure,
}

/// Request to sign bytes with a registered ring-VRF key.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountRingVrfSignRequest {
    /// Registered key handle.
    pub key_handle: ProductAccountId,
    /// Opaque message to sign.
    pub message: Vec<u8>,
}

/// User's authentication state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum HostAccountConnectionStatusSubscribeItem {
    /// User is not logged in to the host.
    Disconnected,
    /// User is logged in to the host.
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
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
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
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Error returned when ring VRF proof creation fails.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostAccountCreateProofError {
    /// Ring not available at the specified location.
    RingNotFound,
    /// The registered member key is not a member of the requested ring.
    NotMember,
    /// The key handle is not registered.
    KeyNotRegistered,
    /// The key handle is not registered for the requested ring.
    KeyNotInRing,
    /// The foreign key owner has not allowlisted the caller.
    NotAllowlisted,
    /// User or host rejected.
    Rejected,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Error returned when contextual alias derivation fails.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostAccountGetAliasError {
    /// Ring not available at the specified location.
    RingNotFound,
    /// The registered member key is not a member of the requested ring.
    NotMember,
    /// The key handle is not registered.
    KeyNotRegistered,
    /// The key handle is not registered for the requested ring.
    KeyNotInRing,
    /// User or host rejected.
    Rejected,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
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
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Request to retrieve the contextual alias for a context and ring.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostAccountGetAliasRequest {
    /// Ring-VRF key handle naming the member key to use.
    pub key_handle: ProductAccountId,
    /// Product-scoped context to derive the alias for.
    pub context: ProductProofContext,
    /// Ring whose member key the host should use; matches `create_proof`.
    pub ring_location: RingLocation,
}

/// Error returned when ring-VRF key registration fails.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostAccountRegisterRingVrfKeyError {
    /// User is not logged in.
    NotConnected,
    /// Ring not available at the specified location.
    RingNotFound,
    /// User or host rejected.
    Rejected,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Error returned when listing ring-VRF keys fails.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostAccountListRingVrfKeysError {
    /// User is not logged in.
    NotConnected,
    /// User or host rejected.
    Rejected,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Error returned when direct ring-VRF key signing fails.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostAccountRingVrfSignError {
    /// User is not logged in.
    NotConnected,
    /// The key handle is not registered.
    KeyNotRegistered,
    /// The foreign key owner has not allowlisted the caller.
    NotAllowlisted,
    /// User or host rejected.
    Rejected,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
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

/// One `append_message` call replayed against the signing transcript.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct VrfTranscriptItem {
    /// Merlin `append_message` label.
    pub label: Vec<u8>,
    /// Merlin `append_message` value.
    pub value: Vec<u8>,
}

/// Request to produce an sr25519 VRF signature from a product account over a
/// caller-supplied Merlin transcript.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct HostAccountSignVrfRequest {
    /// Account whose key signs the VRF.
    pub account: ProductAccountId,
    /// Root domain-separation label: `Transcript::new(transcript_label)`.
    pub transcript_label: Vec<u8>,
    /// Transcript items replayed in order as `append_message(label, value)`.
    pub items: Vec<VrfTranscriptItem>,
}

/// An sr25519 (schnorrkel) VRF signature: the VRF pre-output and its proof.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct VrfSignature {
    /// schnorrkel `VRFPreOut` — the 32-byte VRF output point.
    pub pre_output: [u8; 32],
    /// schnorrkel `VRFProof` — the 64-byte DLEQ proof.
    pub proof: [u8; 64],
}

/// Error returned when VRF signing fails.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostAccountSignVrfError {
    /// User is not logged in.
    NotConnected,
    /// User or host rejected the signing confirmation.
    Rejected,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}
