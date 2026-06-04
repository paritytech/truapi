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
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// // Submit a preimage first so the lookup resolves to a value.
    /// const submitted = await truapi.preimage.submit("0xdeadbeef");
    /// assert(submitted.isOk(), "submit failed:", submitted);
    ///
    /// const item = await firstValueFrom(
    ///   from(truapi.preimage.lookupSubscribe({ request: { key: submitted.value } })),
    /// );
    /// console.log(item);
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
    /// assert(result.isOk(), "submit failed:", result);
    /// console.log(result.value);
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
