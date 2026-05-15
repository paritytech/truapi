//! Unified [`System`] trait.

use crate::versioned::system::{
    HostFeatureSupportedError, HostFeatureSupportedRequest, HostFeatureSupportedResponse,
    HostHandshakeError, HostHandshakeRequest, HostHandshakeResponse, HostNavigateToError,
    HostNavigateToRequest, HostNavigateToResponse, HostPushNotificationError,
    HostPushNotificationRequest, HostPushNotificationResponse, HostRouteChangedItem,
    HostRouteGetError, HostRouteGetResponse, HostRouteSetError, HostRouteSetRequest,
    HostRouteSetResponse,
};
use crate::wire;
use crate::{CallContext, CallError, Subscription};

/// General-purpose TrUAPI methods for handshake, feature detection,
/// navigation, and notifications.
pub trait System: Send + Sync {
    /// Negotiate the wire codec version with the product.
    ///
    /// ```ts
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function handshake(truapi: Client): Promise<void> {
    ///   const result = await truapi.system.handshake();
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 0)]
    async fn handshake(
        &self,
        _cx: &CallContext,
        request: HostHandshakeRequest,
    ) -> Result<HostHandshakeResponse, CallError<HostHandshakeError>> {
        let HostHandshakeRequest::V1(version) = request;
        if version.codec_version == 1 {
            Ok(HostHandshakeResponse::V1)
        } else {
            Err(CallError::Domain(HostHandshakeError::V1(
                crate::v01::HostHandshakeError::UnsupportedProtocolVersion,
            )))
        }
    }

    /// Query whether the host supports a specific feature.
    ///
    /// ```ts
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function supportsChain(truapi: Client): Promise<boolean> {
    ///   const result = await truapi.system.featureSupported({
    ///     tag: "Chain",
    ///     value: {
    ///       genesisHash: "0xd6eec26135305a8ad257a20d003357284c8aa03d0bdb2b357ab0a22371e11ef2",
    ///     },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value.supported;
    /// }
    /// ```
    #[wire(request_id = 2)]
    async fn feature_supported(
        &self,
        cx: &CallContext,
        request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, CallError<HostFeatureSupportedError>>;

    /// Send a push notification to the user.
    ///
    /// ```ts
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function pushNotification(truapi: Client): Promise<void> {
    ///   const result = await truapi.system.pushNotification({
    ///     text: "Hello!",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 4)]
    async fn push_notification(
        &self,
        cx: &CallContext,
        request: HostPushNotificationRequest,
    ) -> Result<HostPushNotificationResponse, CallError<HostPushNotificationError>>;

    /// Request the host to open a URL.
    ///
    /// ```ts
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function navigateToDocs(truapi: Client): Promise<void> {
    ///   const result = await truapi.system.navigateTo({
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
    ///   const result = await truapi.system.routeGet();
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
    ///   const result = await truapi.system.routeSet({
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
    ///   return truapi.system.routeChanged().subscribe({
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
