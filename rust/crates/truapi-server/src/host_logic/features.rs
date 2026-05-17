//! Feature-detection delegation.
//!
//! Unlike older drafts that baked a genesis-hash allow-list into core, the
//! v0.1 surface treats `feature_supported` as a pure platform syscall: each
//! host owns the set of chains it can service. This module is a thin shim
//! that forwards the request through to [`truapi_platform::Features`].

use truapi::v01::GenericError;
use truapi::versioned::system::{HostFeatureSupportedRequest, HostFeatureSupportedResponse};
use truapi_platform::Features;

/// Forward a feature-support query to the platform implementation.
pub async fn feature_supported<P: Features + ?Sized>(
    platform: &P,
    request: HostFeatureSupportedRequest,
) -> Result<HostFeatureSupportedResponse, GenericError> {
    platform.feature_supported(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use truapi::v01;

    struct AlwaysSupported;

    #[async_trait]
    impl Features for AlwaysSupported {
        async fn feature_supported(
            &self,
            request: HostFeatureSupportedRequest,
        ) -> Result<HostFeatureSupportedResponse, GenericError> {
            let HostFeatureSupportedRequest::V1(_) = request;
            Ok(HostFeatureSupportedResponse::V1(
                v01::HostFeatureSupportedResponse { supported: true },
            ))
        }
    }

    struct AlwaysUnsupported;

    #[async_trait]
    impl Features for AlwaysUnsupported {
        async fn feature_supported(
            &self,
            request: HostFeatureSupportedRequest,
        ) -> Result<HostFeatureSupportedResponse, GenericError> {
            let HostFeatureSupportedRequest::V1(_) = request;
            Ok(HostFeatureSupportedResponse::V1(
                v01::HostFeatureSupportedResponse { supported: false },
            ))
        }
    }

    fn req() -> HostFeatureSupportedRequest {
        HostFeatureSupportedRequest::V1(v01::HostFeatureSupportedRequest::Chain {
            genesis_hash: vec![0u8; 32],
        })
    }

    #[test]
    fn delegates_supported_to_platform() {
        let resp = futures::executor::block_on(feature_supported(&AlwaysSupported, req())).unwrap();
        let HostFeatureSupportedResponse::V1(inner) = resp;
        assert!(inner.supported);
    }

    #[test]
    fn delegates_unsupported_to_platform() {
        let resp =
            futures::executor::block_on(feature_supported(&AlwaysUnsupported, req())).unwrap();
        let HostFeatureSupportedResponse::V1(inner) = resp;
        assert!(!inner.supported);
    }
}
