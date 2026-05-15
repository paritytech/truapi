//! Unified [`Navigation`] trait.

use crate::versioned::navigation::{
    HostNavigateToError, HostNavigateToRequest, HostNavigateToResponse, HostRouteChangedItem,
    HostRouteGetError, HostRouteGetResponse, HostRouteSetError, HostRouteSetRequest,
    HostRouteSetResponse,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// Host navigation surface: external URL opens and the app's own route.
pub trait Navigation: Send + Sync {
    /// Request the host to open a URL.
    ///
    /// ```ts
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function navigateToDocs(truapi: Client): Promise<void> {
    ///   const result = await truapi.navigation.navigateTo({
    ///     url: "https://example.com",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 6)]
    async fn navigate_to(
        &self,
        cx: &CallContext,
        request: HostNavigateToRequest,
    ) -> Result<HostNavigateToResponse, CallError<HostNavigateToError>>;

    /// Read the route the host currently holds for this app.
    ///
    /// At bootstrap this returns the route the host was launched with, so the
    /// app can restore deep-linked state. Returns `None` when the app is at
    /// its home.
    ///
    /// ```ts
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function bootstrapRoute(truapi: Client): Promise<string | null> {
    ///   const result = await truapi.navigation.routeGet();
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value.route ?? null;
    /// }
    /// ```
    #[wire(request_id = 134)]
    async fn route_get(
        &self,
        cx: &CallContext,
    ) -> Result<HostRouteGetResponse, CallError<HostRouteGetError>>;

    /// Publish the app's current route to the host's address bar.
    ///
    /// The host renders `route` as part of the user-visible URL so it can be
    /// copied, shared, and reloaded. The host treats the route as opaque.
    ///
    /// ```ts
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function pushRoute(truapi: Client): Promise<void> {
    ///   const result = await truapi.navigation.routeSet({
    ///     route: "Permissions/host_device_permission",
    ///     replace: false,
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 136)]
    async fn route_set(
        &self,
        cx: &CallContext,
        request: HostRouteSetRequest,
    ) -> Result<HostRouteSetResponse, CallError<HostRouteSetError>>;

    /// Subscribe to route changes that originated outside the app.
    ///
    /// Emits on host back/forward and pasted-URL navigation. The host MUST
    /// NOT emit for changes that originated from `route_set` in this app
    /// session. The stream does not emit the initial value; the app reads
    /// that from `route_get`.
    ///
    /// ```ts
    /// import {
    ///   type Client,
    ///   type Subscription,
    ///   type HostRouteChangedItem,
    /// } from "@parity/truapi";
    ///
    /// export function watchRoute(truapi: Client): Subscription {
    ///   return truapi.navigation.routeChanged().subscribe({
    ///     next: (event: HostRouteChangedItem) => console.log(event.route),
    ///     error: (error: Error) => console.error(error),
    ///     complete: () => console.log("completed"),
    ///   });
    /// }
    /// ```
    #[wire(start_id = 138)]
    async fn route_changed(&self, _cx: &CallContext) -> Subscription<HostRouteChangedItem> {
        Subscription::empty()
    }
}
