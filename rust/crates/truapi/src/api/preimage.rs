//! Unified [`Preimage`] trait.

use crate::versioned::preimage::{
    RemotePreimageLookupSubscribeItem, RemotePreimageLookupSubscribeRequest,
    RemotePreimageSubmitError, RemotePreimageSubmitRequest, RemotePreimageSubmitResponse,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Preimage lookup and submission methods.
pub trait Preimage: Send + Sync {
    /// Subscribe to preimage lookups for a given key.
    ///
    /// ```ts
    /// import { from, take } from "rxjs";
    ///
    /// // Submit a preimage first so the lookup resolves to a value.
    /// const submitted = await truapi.preimage.submit("0xdeadbeef");
    /// if (submitted.isErr()) {
    ///   console.error(submitted.error);
    /// } else {
    ///   from(truapi.preimage.lookupSubscribe({ request: { key: submitted.value } }))
    ///     .pipe(take(1))
    ///     .subscribe({
    ///       next: (item) => console.log(item),
    ///       error: (error) => console.error(error),
    ///       complete: () => console.log("completed"),
    ///     });
    /// }
    /// ```
    #[wire(start_id = 64)]
    async fn lookup_subscribe(
        &self,
        _cx: &CallContext,
        _request: RemotePreimageLookupSubscribeRequest,
    ) -> Subscription<RemotePreimageLookupSubscribeItem> {
        Subscription::empty()
    }

    /// Submit a preimage. Returns the preimage key (hash) on success.
    ///
    /// ```ts
    /// const result = await truapi.preimage.submit("0xdeadbeef");
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
    /// ```
    #[wire(request_id = 68)]
    async fn submit(
        &self,
        _cx: &CallContext,
        _request: RemotePreimageSubmitRequest,
    ) -> Result<RemotePreimageSubmitResponse, CallError<RemotePreimageSubmitError>> {
        Err(CallError::unavailable())
    }
}
