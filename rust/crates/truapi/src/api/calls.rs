//! Unified [`TrUApiCalls`] trait.

use crate::versioned::calls::{
    HostFeatureSupportedError, HostFeatureSupportedRequest, HostFeatureSupportedResponse,
    HostHandshakeError, HostHandshakeRequest, HostHandshakeResponse, HostNavigateToError,
    HostNavigateToRequest, HostNavigateToResponse, HostPushNotificationError,
    HostPushNotificationRequest, HostPushNotificationResponse,
};
use crate::versioned::notifications::{
    HostPushNotificationCancelError, HostPushNotificationCancelRequest,
    HostPushNotificationCancelResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// General-purpose TrUAPI methods for feature detection, navigation, and
/// notifications.
///
/// # Wire id reservations
///
/// The discriminants below are listed in [`super::RESERVED_WIRE_IDS`] so
/// codegen rejects any `#[wire(...)]` annotation that collides with them.
/// Slots are held back for upstream `triangle-js-sdks` methods that TrUAPI
/// does not implement, but whose ids must remain free to keep our wire-table
/// positionally aligned with the canonical host `MessagePayload` enum. If we
/// ever need one, annotate the trait method with the matching id and remove
/// it from `RESERVED_WIRE_IDS`.
///
#[async_trait::async_trait]
pub trait TrUApiCalls: Send + Sync {
    /// Negotiates the wire codec version with the product.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function handshake(truapi: Client): Promise<void> {
    ///   const result = await truapi.trUApiCalls.handshake();
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 0)]
    async fn host_handshake(
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

    /// Queries whether the host supports a specific feature.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function supportsChain(truapi: Client): Promise<boolean> {
    ///   const result = await truapi.trUApiCalls.featureSupported({
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
    async fn host_feature_supported(
        &self,
        cx: &CallContext,
        request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, CallError<HostFeatureSupportedError>>;

    /// Sends a push notification to the user, or schedules it for a future
    /// wall-clock instant. The response carries a per-product
    /// [`crate::v02::NotificationId`] (v0.2 only) which can later be passed to
    /// [`Self::host_push_notification_cancel`] to retract a pending
    /// scheduled delivery.
    ///
    /// Behaviour per RFC 0019:
    /// - If `scheduled_at` is `None`, fire immediately (v0.1 behaviour).
    /// - If `scheduled_at` is `Some(t)` and `t <= now`, fire immediately.
    /// - Scheduled notifications MUST survive app and device restart.
    /// - The host maintains a single shared queue across all installed
    ///   products with capacity 64; over-cap calls return
    ///   [`crate::v02::HostPushNotificationError::ScheduleLimitReached`].
    ///   Immediate notifications do not count against the cap.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function pushNotification(truapi: Client): Promise<void> {
    ///   const result = await truapi.trUApiCalls.pushNotification({
    ///     text: "Hello!",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 4)]
    async fn host_push_notification(
        &self,
        cx: &CallContext,
        request: HostPushNotificationRequest,
    ) -> Result<HostPushNotificationResponse, CallError<HostPushNotificationError>>;

    /// Cancels a previously scheduled notification by its
    /// [`crate::v02::NotificationId`]. Idempotent — succeeds whether the id
    /// refers to a pending entry, an already-fired notification, an unknown
    /// id, or one owned by another product. Introduced in v0.2 (RFC 0019).
    #[wire(request_id = 134)]
    async fn host_push_notification_cancel(
        &self,
        cx: &CallContext,
        request: HostPushNotificationCancelRequest,
    ) -> Result<HostPushNotificationCancelResponse, CallError<HostPushNotificationCancelError>>;

    /// Requests the host to open a URL.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function navigateToDocs(truapi: Client): Promise<void> {
    ///   const result = await truapi.trUApiCalls.navigateTo({
    ///     url: "https://example.com",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 6)]
    async fn host_navigate_to(
        &self,
        cx: &CallContext,
        request: HostNavigateToRequest,
    ) -> Result<HostNavigateToResponse, CallError<HostNavigateToError>>;
}
