//! Unified [`Preimage`] trait.

use crate::versioned::preimage::{
    RemotePreimageLookupSubscribeItem, RemotePreimageLookupSubscribeRequest,
    RemotePreimageSubmitError, RemotePreimageSubmitRequest, RemotePreimageSubmitResponse,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Preimage lookup and submission methods.
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
    ///     .lookupSubscribe({
    ///       request: {
    ///         key: "0x0000000000000000000000000000000000000000000000000000000000000000",
    ///       },
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
    async fn lookup_subscribe(
        &self,
        _cx: &CallContext,
        _request: RemotePreimageLookupSubscribeRequest,
    ) -> Subscription<RemotePreimageLookupSubscribeItem> {
        Subscription::empty()
    }

    /// Submit a preimage. Returns the preimage key (hash) on success.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HexString,
    /// } from "@parity/truapi";
    ///
    /// export async function submitPreimage(
    ///   truapi: Client,
    /// ): Promise<HexString> {
    ///   const result = await truapi.preimage.submit("0xdeadbeef");
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
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
