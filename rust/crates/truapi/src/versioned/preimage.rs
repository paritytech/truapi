//! Versioned wrappers for [`Preimage`](crate::api::Preimage) methods.

use crate::v01;

versioned_type! {
    /// Versioned wrapper for [`v01::RemotePreimageLookupSubscribeRequest`] and older versions.
    pub enum RemotePreimageLookupSubscribeRequest { V1 => v01::RemotePreimageLookupSubscribeRequest }
    /// Versioned wrapper for [`v01::RemotePreimageLookupSubscribeItem`] and older versions.
    pub enum RemotePreimageLookupSubscribeItem { V1 => v01::RemotePreimageLookupSubscribeItem }
    /// Versioned wrapper for the preimage submit request (raw bytes).
    pub enum RemotePreimageSubmitRequest { V1 => Vec<u8> }
    /// Versioned wrapper for the preimage submit response (preimage key).
    pub enum RemotePreimageSubmitResponse { V1 => Vec<u8> }
    /// Versioned wrapper for [`v01::PreimageSubmitError`].
    pub enum RemotePreimageSubmitError { V1 => v01::PreimageSubmitError }
}
