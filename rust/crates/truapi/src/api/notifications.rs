//! Unified [`Notifications`] trait.

use crate::versioned::notifications::{
    HostPushAddRulesError, HostPushAddRulesRequest, HostPushAddRulesResponse,
    HostPushBroadcastError, HostPushBroadcastRequest, HostPushBroadcastResponse,
    HostPushListRulesError, HostPushListRulesRequest, HostPushListRulesResponse,
    HostPushNotificationCancelError, HostPushNotificationCancelRequest,
    HostPushNotificationCancelResponse, HostPushNotificationError, HostPushNotificationRequest,
    HostPushNotificationResponse, HostPushRemoveRulesError, HostPushRemoveRulesRequest,
    HostPushRemoveRulesResponse, HostPushSetRulesError, HostPushSetRulesRequest,
    HostPushSetRulesResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Notification methods: locally-rendered notifications and Statement Store
/// subscription rules for backend-delivered pushes.
///
/// The rule-management methods (`push_add_rules`, `push_remove_rules`,
/// `push_list_rules`, `push_set_rules`) mirror the rule-management endpoints
/// of the push-notifications v2 backend design:
///
/// - <https://hackmd.io/@1JCaGppGSUqHtJilikYaKw/r16YTVg5Ze> — v2,
///   backend-mediated
/// - <https://hackmd.io/@1JCaGppGSUqHtJilikYaKw/SyPN2yV6lx> — v1,
///   peer-to-peer (historical context)
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
    /// const result = await truapi.notifications.sendPushNotification({
    ///   text: "Hello!",
    /// });
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
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
    /// const result = await truapi.notifications.cancelPushNotification({
    ///   id: 1,
    /// });
    /// result.match(
    ///   () => console.log("ok"),
    ///   (error) => console.error(error),
    /// );
    /// ```
    #[wire(request_id = 134)]
    async fn cancel_push_notification(
        &self,
        cx: &CallContext,
        request: HostPushNotificationCancelRequest,
    ) -> Result<HostPushNotificationCancelResponse, CallError<HostPushNotificationCancelError>>;

    /// Register one or more `(signer, topic)` rules so the user is woken by a
    /// push when a signed statement matching a rule appears on the Statement
    /// Store. Mirrors `POST /v1/subscriptions/rules` from the v2 push backend
    /// spec. `signer` is mandatory — the publisher whose statements should wake
    /// the user (the calling product's own identity to self-subscribe, or
    /// another product's).
    ///
    /// ```ts
    /// const result = await truapi.notifications.pushAddRules({
    ///   topics: ["0x00"],
    ///   signer: "0x…",
    /// });
    /// result.match(
    ///   () => console.log("ok"),
    ///   (error) => console.error(error),
    /// );
    /// ```
    #[wire(request_id = 164)]
    async fn push_add_rules(
        &self,
        cx: &CallContext,
        request: HostPushAddRulesRequest,
    ) -> Result<HostPushAddRulesResponse, CallError<HostPushAddRulesError>>;

    /// Remove one or more previously registered topics. Mirrors
    /// `DELETE /v1/subscriptions/rules` from the v2 push backend spec.
    ///
    /// ```ts
    /// const result = await truapi.notifications.pushRemoveRules({
    ///   topics: ["0x00"],
    ///   signer: "0x…",
    /// });
    /// result.match(
    ///   () => console.log("ok"),
    ///   (error) => console.error(error),
    /// );
    /// ```
    #[wire(request_id = 166)]
    async fn push_remove_rules(
        &self,
        cx: &CallContext,
        request: HostPushRemoveRulesRequest,
    ) -> Result<HostPushRemoveRulesResponse, CallError<HostPushRemoveRulesError>>;

    /// List the calling product's currently registered topics. Useful for
    /// reconciling local UI state with what the host believes is active
    /// (e.g. after logout/login). Mirrors `GET /v1/subscriptions` from the
    /// v2 push backend spec.
    ///
    /// ```ts
    /// const result = await truapi.notifications.pushListRules({});
    /// result.match(
    ///   (value) => console.log(value.topics),
    ///   (error) => console.error(error),
    /// );
    /// ```
    #[wire(request_id = 168)]
    async fn push_list_rules(
        &self,
        cx: &CallContext,
        request: HostPushListRulesRequest,
    ) -> Result<HostPushListRulesResponse, CallError<HostPushListRulesError>>;

    /// Atomically replace the calling product's entire topic set with the
    /// supplied vector. After a successful call, the product's active
    /// topics are exactly `topics`. Mirrors `PUT /v1/subscriptions/rules`
    /// from the v2 push backend spec.
    ///
    /// ```ts
    /// const result = await truapi.notifications.pushSetRules({
    ///   topics: ["0x00"],
    ///   signer: "0x…",
    /// });
    /// result.match(
    ///   () => console.log("ok"),
    ///   (error) => console.error(error),
    /// );
    /// ```
    #[wire(request_id = 170)]
    async fn push_set_rules(
        &self,
        cx: &CallContext,
        request: HostPushSetRulesRequest,
    ) -> Result<HostPushSetRulesResponse, CallError<HostPushSetRulesError>>;

    /// Publish an announcement to subscribers. Interim distribution that does
    /// not use the Statement Store as the distribution layer: the host sets the
    /// publisher `signer` to the calling product's identity (the product cannot
    /// override it) and submits the announcement to the push backend, which fans
    /// out using the same `(signer, topic)` rule matching.
    ///
    /// ```ts
    /// const result = await truapi.notifications.pushBroadcast({
    ///   topics: ["0x00"],
    ///   content: { title: "Web3 Summit", body: "Keynote moved to Hall A" },
    /// });
    /// result.match(
    ///   (value) => console.log(value.matched),
    ///   (error) => console.error(error),
    /// );
    /// ```
    #[wire(request_id = 172)]
    async fn push_broadcast(
        &self,
        cx: &CallContext,
        request: HostPushBroadcastRequest,
    ) -> Result<HostPushBroadcastResponse, CallError<HostPushBroadcastError>>;
}
