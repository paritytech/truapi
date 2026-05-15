//! Unified [`System`] trait.

use crate::versioned::system::{
    HostFeatureSupportedError, HostFeatureSupportedRequest, HostFeatureSupportedResponse,
    HostHandshakeError, HostHandshakeRequest, HostHandshakeResponse, HostNavigateToError,
    HostNavigateToRequest, HostNavigateToResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// General-purpose TrUAPI methods for handshake, feature detection, and
/// navigation.
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
