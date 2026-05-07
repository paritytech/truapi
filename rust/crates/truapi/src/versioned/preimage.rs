//! Versioned wrappers for [`Preimage`](crate::api::Preimage) methods.

use crate::v01;

versioned_type! {
    /// Versioned wrapper for [`v01::RemotePreimageLookupSubscribeRequest`] and older versions.
    pub enum RemotePreimageLookupSubscribeRequest { V1 => v01::RemotePreimageLookupSubscribeRequest }
    /// Versioned wrapper for [`v01::RemotePreimageLookupSubscribeItem`] and older versions.
    pub enum RemotePreimageLookupSubscribeItem { V1 => v01::RemotePreimageLookupSubscribeItem }
}
