use parity_scale_codec::{Decode, Encode};

/// A contact peer addressed by DotNS username or identity public key
/// (RFC 0022).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ContactPeer {
    /// DotNS username, resolved to its owning identity account.
    Username(String),
    /// Identity account public key (the primary-username owner).
    Identity([u8; 32]),
}

/// Request to resolve a peer's identity and published exchange key.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostContactsResolveRequest {
    /// Peer to resolve.
    pub peer: ContactPeer,
}

/// A peer's reachability record.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostContactsResolveResponse {
    /// Identity account public key.
    pub identity: [u8; 32],
    /// Published X25519 exchange public key. `None`: the identity exists but
    /// has no published exchange key (e.g. its host predates RFC 0022); the
    /// peer is not yet reachable.
    pub exchange_key: Option<[u8; 32]>,
}

/// Peer resolution error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostContactsResolveError {
    /// No identity is registered for the peer.
    NotFound,
    /// Catch-all.
    Unknown { reason: String },
}

/// Request to derive a symmetric key shared with a peer, scoped to the
/// calling product and a caller-chosen context.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostContactsDeriveSharedKeyRequest {
    /// Peer to derive the shared key with.
    pub peer: ContactPeer,
    /// Domain-separation context, at most 32 bytes (as in RFC 0007, callers
    /// hash longer contexts down).
    pub context: Vec<u8>,
}

/// A derived shared key.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostContactsDeriveSharedKeyResponse {
    /// The derived 32-byte symmetric key.
    pub key: [u8; 32],
}

/// Shared-key derivation error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostContactsDeriveSharedKeyError {
    /// No identity is registered for the peer.
    NotFound,
    /// The peer has no published exchange key.
    NotReachable,
    /// The context exceeds 32 bytes.
    ContextTooLong,
    /// No identity session is active.
    NotConnected,
    /// Catch-all.
    Unknown { reason: String },
}

/// Request to seal a payload to a recipient and submit it to their contact
/// inbox.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostContactsSendRequest {
    /// Recipient of the contact.
    pub recipient: ContactPeer,
    /// Opaque product payload carried inside the sealed envelope.
    pub payload: Vec<u8>,
}

/// Contact send error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostContactsSendError {
    /// No identity is registered for the recipient.
    NotFound,
    /// The recipient has no published exchange key.
    NotReachable,
    /// The sealed statement would exceed the store's statement size limit.
    PayloadTooLarge,
    /// The user denied the `ContactSend` permission.
    PermissionDenied,
    /// No identity session is active.
    NotConnected,
    /// Catch-all.
    Unknown { reason: String },
}

/// One contact delivered to the calling product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ContactDelivery {
    /// Authenticated sender identity public key.
    pub sender_identity: [u8; 32],
    /// Sender's DotNS username, when the host can resolve one.
    pub sender_username: Option<String>,
    /// Opaque product payload.
    pub payload: Vec<u8>,
    /// Unix-seconds timestamp the sender stamped on the contact.
    pub sent_at: u64,
}

/// Page of contacts delivered by the contacts subscription. The
/// `is_complete` flag distinguishes the replay of contacts received before
/// this subscription (`false`) from the live-update phase (`true`), as in
/// RFC 0008.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostContactsSubscribeItem {
    /// Contacts addressed to the calling product.
    pub contacts: Vec<ContactDelivery>,
    /// `false` while the host is still replaying persisted contacts (more
    /// pages to follow). `true` once the replay is complete; all subsequent
    /// pages are also `true` and carry only newly-arrived contacts.
    pub is_complete: bool,
}

/// Contacts subscription error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostContactsSubscribeError {
    /// No identity session is active.
    NotConnected,
    /// Catch-all.
    Unknown { reason: String },
}
