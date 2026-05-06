//! Unified [`TrUApiCalls`] trait.

use crate::v02::HandshakeError;
use crate::versioned::calls::{
    HostFeatureSupportedError, HostFeatureSupportedRequest, HostFeatureSupportedResponse,
    HostHandshakeError, HostHandshakeRequest, HostHandshakeResponse, HostNavigateToError,
    HostNavigateToRequest, HostNavigateToResponse, HostPushNotificationError,
    HostPushNotificationRequest, HostPushNotificationResponse,
};
use crate::wire;
use crate::CallContext;

/// General-purpose TrUAPI methods for feature detection, navigation, and
/// notifications.
///
/// # Wire id reservations
///
/// Slots 68-75 are reserved for legacy Novasama methods TrUAPI does not
/// implement; if we ever need them, annotate the trait method with
/// `#[wire(id = ...)]` matching the slot below.
///
/// - 68-69: `remote_preimage_submit` (request, response)
/// - 70-71: `host_jsonrpc_message_send` (request, response)
/// - 72-75: `host_jsonrpc_message_subscribe` (start, stop, interrupt, receive)
#[async_trait::async_trait]
pub trait TrUApiCalls: Send + Sync {
    /// Negotiates the wire codec version with the product. Required for
    /// compatibility with `@novasamatech/host-api`-built products that gate
    /// "connected" state on a successful handshake response.
    ///
    /// Default impl accepts codec version `1` (Novasama's `JAM_CODEC_PROTOCOL_ID`)
    /// and rejects everything else with `UnsupportedProtocolVersion`. Hosts that
    /// want to gate handshake on additional preconditions can override.
    #[wire(id = 0)]
    async fn host_handshake(
        &self,
        _cx: &CallContext,
        request: HostHandshakeRequest,
    ) -> Result<HostHandshakeResponse, HostHandshakeError> {
        let HostHandshakeRequest::V1(version) = request;
        if version == 1 {
            Ok(HostHandshakeResponse::V1)
        } else {
            Err(HostHandshakeError::V1(
                HandshakeError::UnsupportedProtocolVersion,
            ))
        }
    }

    /// Queries whether the host supports a specific feature.
    #[wire(id = 2)]
    async fn host_feature_supported(
        &self,
        cx: &CallContext,
        request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, HostFeatureSupportedError>;

    /// Sends a push notification to the user.
    #[wire(id = 4)]
    async fn host_push_notification(
        &self,
        cx: &CallContext,
        request: HostPushNotificationRequest,
    ) -> Result<HostPushNotificationResponse, HostPushNotificationError>;

    /// Requests the host to open a URL.
    #[wire(id = 6)]
    async fn host_navigate_to(
        &self,
        cx: &CallContext,
        request: HostNavigateToRequest,
    ) -> Result<HostNavigateToResponse, HostNavigateToError>;
}
