//! Unified [`System`] trait.

use crate::versioned::system::{
    HostFeatureSupportedError, HostFeatureSupportedRequest, HostFeatureSupportedResponse,
    HostHandshakeError, HostHandshakeRequest, HostHandshakeResponse, HostInfoError,
    HostInfoRequest, HostInfoResponse, HostNavigateToError, HostNavigateToRequest,
    HostNavigateToResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// General-purpose TrUAPI methods for handshake, feature detection,
/// and navigation.
pub trait System: Send + Sync {
    /// Negotiate the wire codec version with the product.
    ///
    /// ```ts
    /// const result = await truapi.system.handshake();
    /// assert(result.isOk(), "handshake failed:", result);
    /// console.log("handshake succeeded");
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
    /// import { PASEO_NEXT_V2_ASSET_HUB } from "@parity/truapi";
    ///
    /// const result = await truapi.system.featureSupported({
    ///   tag: "Chain",
    ///   value: {
    ///     genesisHash: PASEO_NEXT_V2_ASSET_HUB.genesis,
    ///   },
    /// });
    /// assert(result.isOk(), "featureSupported failed:", result);
    /// console.log("feature supported:", result.value.supported);
    /// ```
    #[wire(request_id = 2)]
    async fn feature_supported(
        &self,
        cx: &CallContext,
        request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, CallError<HostFeatureSupportedError>>;

    /// Request the host to open a URL.
    ///
    /// ```ts
    /// const result = await truapi.system.navigateTo({
    ///   url: "https://example.com",
    /// });
    /// assert(result.isOk(), "navigateTo failed:", result);
    /// console.log("navigation succeeded");
    /// ```
    #[wire(request_id = 6)]
    async fn navigate_to(
        &self,
        cx: &CallContext,
        request: HostNavigateToRequest,
    ) -> Result<HostNavigateToResponse, CallError<HostNavigateToError>>;

    /// Report the host's identity and version.
    ///
    /// Returns the host's platform, name, and version so a product knows
    /// exactly which host — and which build of it — is running it: for
    /// adapting to the host, telemetry, and attributing behaviour to a
    /// concrete build in diagnostics and bug reports.
    ///
    /// ```ts
    /// const result = await truapi.system.info();
    /// assert(result.isOk(), "info failed:", result);
    /// const info = result.value;
    /// console.log(`${info.name} ${info.version} on ${info.platform}`);
    /// ```
    #[wire(request_id = 164)]
    async fn host_info(
        &self,
        cx: &CallContext,
        request: HostInfoRequest,
    ) -> Result<HostInfoResponse, CallError<HostInfoError>>;
}
