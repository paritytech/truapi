//! Unified [`Session`] trait.

use crate::versioned::session::{
    HostSessionLifecycleSubscribeError, HostSessionLifecycleSubscribeItem,
    HostSessionLifecycleSubscribeRequest,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Product session lifecycle operations.
///
/// Default methods return [`CallError::HostFailure`] with an `unavailable`
/// reason. Hosts override when they support product session restore.
pub trait Session: Send + Sync {
    /// Subscribe to host lifecycle signals so a product can checkpoint semantic
    /// session state through scoped local storage before suspend, eviction, or
    /// close.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostSessionLifecycleSubscribeError,
    ///   type HostSessionLifecycleSubscribeItem,
    ///   type Subscription,
    ///   type SubscriptionError,
    /// } from "@parity/truapi";
    ///
    /// export function watchSessionLifecycle(truapi: Client): Subscription {
    ///   return truapi.session
    ///     .sessionLifecycleSubscribe({
    ///       request: { replayCurrentState: true },
    ///     })
    ///     .subscribe({
    ///       next: (event: HostSessionLifecycleSubscribeItem) =>
    ///         console.log(event),
    ///       error: (error: SubscriptionError<HostSessionLifecycleSubscribeError>) =>
    ///         console.error(error),
    ///       complete: () => console.log("completed"),
    ///     });
    /// }
    /// ```
    #[wire(start_id = 162)]
    async fn lifecycle_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostSessionLifecycleSubscribeRequest,
    ) -> Result<
        Subscription<HostSessionLifecycleSubscribeItem>,
        CallError<HostSessionLifecycleSubscribeError>,
    > {
        Err(CallError::unavailable())
    }
}
