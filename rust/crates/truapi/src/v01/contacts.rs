use parity_scale_codec::{Decode, Encode};

use crate::v01::ProductAccountId;

/// Context key for a contact entry, scoped to a specific product account.
pub type ContactContext = ProductAccountId;

/// A contact's identity within a specific product context.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ContextContactInfo {
    /// Ring VRF alias in this context, if known.
    pub alias: Option<Vec<u8>>,
    /// Account public key in this context, if known.
    pub account_id: Option<Vec<u8>>,
}

/// Host-local metadata for a contact (not context-scoped).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct LocalContactInfo {
    /// User-chosen display name for this contact.
    pub display_name: Option<String>,
}

/// A single contact from the user's address book.
///
/// Pairs host-local metadata with a map of context-scoped entries keyed by
/// [`ProductAccountId`]. Depending on the caller's access tier, `entries` may
/// contain only the requesting product's context (tier 1) or entries across
/// all contexts (tier 2).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Contact {
    /// Host-local metadata (display name, etc.).
    pub local: LocalContactInfo,
    /// Context-scoped entries keyed by `ProductAccountId`.
    pub entries: Vec<ContactEntry>,
}

/// A single context-scoped entry within a [`Contact`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ContactEntry {
    /// The product context this entry belongs to.
    pub context: ContactContext,
    /// Identity information within this context.
    pub info: ContextContactInfo,
}

/// Request to retrieve the user's contact list.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostContactsGetRequest {
    /// Optional context filter. When `None`, the host uses the calling
    /// product's own `DotNsIdentifier` (tier 1). When `Some`, the host
    /// filters entries to that product's context — cross-context access
    /// requires `ContactsCrossContext` permission.
    pub context: Option<String>,
}

/// Response containing the user's filtered contact list.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostContactsGetResponse {
    /// Contacts from the user's address book, filtered by access tier.
    pub contacts: Vec<Contact>,
}

/// Subscription item delivering an updated snapshot of the contact list.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostContactsSubscribeItem {
    /// Full filtered contact list at the time of the update.
    pub contacts: Vec<Contact>,
}

/// Error returned by contacts operations.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostContactsError {
    /// User is not logged in.
    NotConnected,
    /// User denied the permission prompt.
    Rejected,
    /// Catch-all.
    Unknown { reason: String },
}
