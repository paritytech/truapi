//! Versioned wrappers for [`Preimage`](crate::api::Preimage) methods.

use crate::v01;

versioned_type! {
    /// Subscription request wrapper for `remote_preimage_lookup_subscribe`.
    pub enum RemotePreimageLookupSubscribeRequest { V1 => v01::PreimageKey }
    /// Subscription item wrapper for `remote_preimage_lookup_subscribe`.
    pub enum RemotePreimageLookupSubscribeItem { V1 => Option<v01::PreimageValue> }
}
