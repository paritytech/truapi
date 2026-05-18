//! Unified [`Notifications`] trait.

use crate::versioned::notifications::{
    HostPushAddRulesError, HostPushAddRulesRequest, HostPushAddRulesResponse,
    HostPushListRulesError, HostPushListRulesRequest, HostPushListRulesResponse,
    HostPushNotificationError, HostPushNotificationRequest, HostPushNotificationResponse,
    HostPushRemoveRulesError, HostPushRemoveRulesRequest, HostPushRemoveRulesResponse,
    HostPushSetRulesError, HostPushSetRulesRequest, HostPushSetRulesResponse,
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

    /// Register one or more topic rules so the user is woken up by a push
    /// when a signed statement matching any registered rule appears on the
    /// Statement Store. Mirrors `POST /v1/subscriptions/rules` from the v2
    /// push backend spec. The signer is injected by the host (based on the
    /// calling product's identity) when relaying the rule to the backend.
    ///
    /// ```ts
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function addAnnouncementsRules(
    ///   truapi: Client,
    /// ): Promise<void> {
    ///   const result = await truapi.notifications.pushAddRules({
    ///     rules: [{ topic: "0x00" }],
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 134)]
    async fn push_add_rules(
        &self,
        cx: &CallContext,
        request: HostPushAddRulesRequest,
    ) -> Result<HostPushAddRulesResponse, CallError<HostPushAddRulesError>>;

    /// Remove one or more previously registered subscription rules. Mirrors
    /// `DELETE /v1/subscriptions/rules` from the v2 push backend spec.
    ///
    /// ```ts
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function removeAnnouncementsRules(
    ///   truapi: Client,
    /// ): Promise<void> {
    ///   const result = await truapi.notifications.pushRemoveRules({
    ///     rules: [{ topic: "0x00" }],
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 136)]
    async fn push_remove_rules(
        &self,
        cx: &CallContext,
        request: HostPushRemoveRulesRequest,
    ) -> Result<HostPushRemoveRulesResponse, CallError<HostPushRemoveRulesError>>;

    /// List the calling product's currently registered subscription rules.
    /// Useful for reconciling local UI state with what the host believes is
    /// active (e.g. after logout/login). Mirrors
    /// `GET /v1/subscriptions` from the v2 push backend spec.
    ///
    /// ```ts
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function listRules(truapi: Client) {
    ///   const result = await truapi.notifications.pushListRules({});
    ///   if (result.isErr()) throw result.error;
    ///   return result.value.rules;
    /// }
    /// ```
    #[wire(request_id = 138)]
    async fn push_list_rules(
        &self,
        cx: &CallContext,
        request: HostPushListRulesRequest,
    ) -> Result<HostPushListRulesResponse, CallError<HostPushListRulesError>>;

    /// Atomically replace the calling product's entire rule set with the
    /// supplied vector. After a successful call, the product's active rules
    /// are exactly `rules`. Mirrors `PUT /v1/subscriptions/rules` from the
    /// v2 push backend spec.
    ///
    /// ```ts
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function setRules(truapi: Client): Promise<void> {
    ///   const result = await truapi.notifications.pushSetRules({
    ///     rules: [{ topic: "0x00" }],
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(request_id = 140)]
    async fn push_set_rules(
        &self,
        cx: &CallContext,
        request: HostPushSetRulesRequest,
    ) -> Result<HostPushSetRulesResponse, CallError<HostPushSetRulesError>>;
}
