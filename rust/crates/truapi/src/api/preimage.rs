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
/// The default body flags the call as unavailable through
/// [`CallContext::fail_unavailable`]; hosts override only if they actually
/// support preimage lookup.
#[async_trait::async_trait]
pub trait Preimage: Send + Sync {
    /// Subscribe to preimage lookups for a given key.
    #[wire(id = 64)]
    async fn remote_preimage_lookup_subscribe(
        &self,
        cx: &CallContext,
        _request: RemotePreimageLookupSubscribeRequest,
    ) -> Subscription<RemotePreimageLookupSubscribeItem> {
        cx.fail_unavailable();
        Subscription::empty()
    }
}
