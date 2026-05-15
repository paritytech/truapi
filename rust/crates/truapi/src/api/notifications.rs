//! Unified [`Notifications`] trait.

use crate::versioned::notifications::{
    HostPushNotificationError, HostPushNotificationRequest, HostPushNotificationResponse,
    HostPushSubscribeError, HostPushSubscribeRequest, HostPushSubscribeResponse,
    HostPushUnsubscribeError, HostPushUnsubscribeRequest, HostPushUnsubscribeResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Notification methods: locally-rendered notifications and Statement Store
/// subscription rules for backend-delivered pushes.
pub trait Notifications: Send + Sync {
    /// Send a notification to the user, rendered immediately by the host.
    ///
    /// ```ts
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function pushNotification(truapi: Client): Promise<void> {
    ///   const result = await truapi.notifications.pushNotification({
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

    /// Register a `(signer, topic)` rule so the user is woken up by a push
    /// when a signed statement matching the rule appears on the Statement
    /// Store.
    ///
    /// ```ts
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function subscribeToAnnouncements(
    ///   truapi: Client,
    ///   signer: Uint8Array,
    ///   topic: Uint8Array,
    /// ): Promise<void> {
    ///   const result = await truapi.notifications.pushSubscribe({
    ///     rule: { signer, topic },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 134)]
    async fn push_subscribe(
        &self,
        cx: &CallContext,
        request: HostPushSubscribeRequest,
    ) -> Result<HostPushSubscribeResponse, CallError<HostPushSubscribeError>>;

    /// Remove a previously registered subscription rule.
    ///
    /// ```ts
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function unsubscribeFromAnnouncements(
    ///   truapi: Client,
    ///   signer: Uint8Array,
    ///   topic: Uint8Array,
    /// ): Promise<void> {
    ///   const result = await truapi.notifications.pushUnsubscribe({
    ///     rule: { signer, topic },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 136)]
    async fn push_unsubscribe(
        &self,
        cx: &CallContext,
        request: HostPushUnsubscribeRequest,
    ) -> Result<HostPushUnsubscribeResponse, CallError<HostPushUnsubscribeError>>;
}
