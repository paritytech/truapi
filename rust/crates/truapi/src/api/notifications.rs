//! Unified [`Notifications`] trait.

use crate::versioned::notifications::{
    HostPushNotificationCancelError, HostPushNotificationCancelRequest,
    HostPushNotificationCancelResponse, HostPushNotificationError, HostPushNotificationRequest,
    HostPushNotificationResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Notification methods for locally-rendered push notifications.
pub trait Notifications: Send + Sync {
    /// Send a push notification to the user.
    ///
    /// Returns a [`NotificationId`](crate::v01::NotificationId) that can be
    /// passed to [`cancel_push_notification`](Self::cancel_push_notification)
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
    ///   const result = await truapi.notifications.sendPushNotification({
    ///     text: "Hello!",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 4)]
    async fn send_push_notification(
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
    ///   const result = await truapi.notifications.cancelPushNotification({
    ///     id: 1,
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 134)]
    async fn cancel_push_notification(
        &self,
        cx: &CallContext,
        request: HostPushNotificationCancelRequest,
    ) -> Result<HostPushNotificationCancelResponse, CallError<HostPushNotificationCancelError>>;
}
