//! Unified [`System`] trait.

use crate::versioned::system::{
    HostFeatureSupportedError, HostFeatureSupportedRequest, HostFeatureSupportedResponse,
    HostHandshakeError, HostHandshakeRequest, HostHandshakeResponse, HostNavigateToError,
    HostNavigateToRequest, HostNavigateToResponse, HostPushNotificationCancelError,
    HostPushNotificationCancelRequest, HostPushNotificationCancelResponse,
    HostPushNotificationError, HostPushNotificationRequest, HostPushNotificationResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

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
    /// Returns a [`NotificationId`](crate::v01::NotificationId) that can be
    /// passed to [`host_push_notification_cancel`](Self::host_push_notification_cancel)
    /// to retract a scheduled notification. When `scheduled_at` is set the host
    /// persists the notification across restarts and fires it through the
    /// platform-native scheduler. See [RFC 0019].
    ///
    /// [RFC 0019]: https://github.com/paritytech/truapi/blob/main/docs/rfcs/0019-scheduled-notifications.md
    ///
    /// ```ts
    /// import {
    ///   type Client,
    ///   type HostPushNotificationResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function pushNotification(
    ///   truapi: Client,
    /// ): Promise<HostPushNotificationResponse> {
    ///   const result = await truapi.system.pushNotification({
    ///     text: "Hello!",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 4)]
    async fn push_notification(
        &self,
        cx: &CallContext,
        request: HostPushNotificationRequest,
    ) -> Result<HostPushNotificationResponse, CallError<HostPushNotificationError>>;

    /// Cancels a previously issued push notification.
    ///
    /// Cancellation is idempotent: returns `Ok(())` whether the notification is
    /// still pending, already fired, or was never issued. See [RFC 0019].
    ///
    /// [RFC 0019]: https://github.com/paritytech/truapi/blob/main/docs/rfcs/0019-scheduled-notifications.md
    ///
    /// ```ts
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function cancelNotification(
    ///   truapi: Client,
    /// ): Promise<void> {
    ///   const result = await truapi.system.pushNotificationCancel({
    ///     id: 1,
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 134)]
    async fn host_push_notification_cancel(
        &self,
        cx: &CallContext,
        request: HostPushNotificationCancelRequest,
    ) -> Result<HostPushNotificationCancelResponse, CallError<HostPushNotificationCancelError>>;

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
}
