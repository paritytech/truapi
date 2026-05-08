//! Unified [`TrUApiCalls`] trait.

use crate::versioned::calls::{
    HostFeatureSupportedError, HostFeatureSupportedRequest, HostFeatureSupportedResponse,
    HostHandshakeError, HostHandshakeRequest, HostHandshakeResponse, HostNavigateToError,
    HostNavigateToRequest, HostNavigateToResponse, HostPushNotificationError,
    HostPushNotificationRequest, HostPushNotificationResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// General-purpose TrUAPI methods for feature detection, navigation, and
/// notifications.
///
/// # Wire id reservations
///
/// Some slots are reserved for upstream `triangle-js-sdks` methods that
/// TrUAPI does not implement, but whose discriminants must remain free to
/// keep our wire-table positionally aligned with the canonical host
/// `MessagePayload` enum. If we ever need them, annotate the trait method
/// with `#[wire(id = ...)]` matching the slot below.
///
/// - 34-35: `host_sign_raw_with_legacy_account` (request, response)
/// - 36-37: `host_sign_payload_with_legacy_account` (request, response)
/// - 68-69: `remote_preimage_submit` (request, response)
/// - 70-71: `host_jsonrpc_message_send` (request, response)
/// - 72-75: `host_jsonrpc_message_subscribe` (start, stop, interrupt, receive)
/// - 104-107: `host_theme_subscribe` (start, stop, interrupt, receive)
/// - 112-113: `host_request_login` (request, response)
#[async_trait::async_trait]
pub trait TrUApiCalls: Send + Sync {
    /// Negotiates the wire codec version with the product.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function handshake(truapi: Client) {
    ///   const result = await truapi.trUApiCalls.handshake();
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(id = 0)]
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
                crate::v02::HostHandshakeError::UnsupportedProtocolVersion,
            )))
        }
    }

    /// Queries whether the host supports a specific feature.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function supportsChain(truapi: Client) {
    ///   const result = await truapi.trUApiCalls.featureSupported({
    ///     tag: "Chain",
    ///     value: { genesisHash: new Uint8Array() },
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value.supported;
    /// }
    /// ```
    #[wire(id = 2)]
    async fn host_feature_supported(
        &self,
        cx: &CallContext,
        request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, CallError<HostFeatureSupportedError>>;

    /// Sends a push notification to the user.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function pushNotification(truapi: Client) {
    ///   const result = await truapi.trUApiCalls.pushNotification({
    ///     text: "Hello!",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(id = 4)]
    async fn host_push_notification(
        &self,
        cx: &CallContext,
        request: HostPushNotificationRequest,
    ) -> Result<HostPushNotificationResponse, CallError<HostPushNotificationError>>;

    /// Requests the host to open a URL.
    ///
    /// ```truapi-client-example
    /// import { type Client } from "@parity/truapi";
    ///
    /// export async function navigateToDocs(truapi: Client) {
    ///   const result = await truapi.trUApiCalls.navigateTo({
    ///     url: "https://example.com",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    /// }
    /// ```
    #[wire(id = 6)]
    async fn host_navigate_to(
        &self,
        cx: &CallContext,
        request: HostNavigateToRequest,
    ) -> Result<HostNavigateToResponse, CallError<HostNavigateToError>>;
}
