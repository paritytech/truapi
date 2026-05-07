//! Unified [`Preimage`] trait.

use crate::versioned::preimage::{
    RemotePreimageLookupSubscribeItem, RemotePreimageLookupSubscribeRequest,
};
use crate::wire;
use crate::{CallContext, Subscription};

/// Preimage lookup.
///
/// The v01 `remote_preimage_submit` method is intentionally not carried into
/// the unified contract because v02 removed it.
///
/// Hosts override only if they actually support preimage lookup.
#[async_trait::async_trait]
pub trait Preimage: Send + Sync {
    /// Subscribe to preimage lookups for a given key.
    ///
    /// ```truapi-playground-request
    /// { "key": "0x0000000000000000000000000000000000000000000000000000000000000000" }
    /// ```
    #[wire(id = 64)]
    async fn remote_preimage_lookup_subscribe(
        &self,
        _cx: &CallContext,
        _request: RemotePreimageLookupSubscribeRequest,
    ) -> Subscription<RemotePreimageLookupSubscribeItem> {
        Subscription::empty()
    }
}
