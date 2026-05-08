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
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type Subscription,
    ///   type RemotePreimageLookupSubscribeItem,
    /// } from "@parity/truapi";
    ///
    /// export function lookupPreimage(truapi: Client): Subscription {
    ///   return truapi.preimage
    ///     .preimageLookupSubscribe({
    ///       request: { key: new Uint8Array() },
    ///     })
    ///     .subscribe({
    ///       next: (item: RemotePreimageLookupSubscribeItem) =>
    ///         console.log(item),
    ///       error: (error: Error) => console.error(error),
    ///       complete: () => console.log("completed"),
    ///     });
    /// }
    /// ```
    #[wire(start_id = 64)]
    async fn remote_preimage_lookup_subscribe(
        &self,
        _cx: &CallContext,
        _request: RemotePreimageLookupSubscribeRequest,
    ) -> Subscription<RemotePreimageLookupSubscribeItem> {
        Subscription::empty()
    }
}
