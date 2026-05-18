//! Feature-detection delegation.
//!
//! `feature_supported` is a platform syscall: each host owns the set of
//! chains it can service. This module is a thin shim that forwards the
//! request through to [`truapi_platform::Features`].

use truapi::v01::GenericError;
use truapi::versioned::system::{HostFeatureSupportedRequest, HostFeatureSupportedResponse};
use truapi_platform::Features;

/// Forward a feature-support query to the platform implementation.
pub async fn feature_supported<P: Features>(
    platform: &P,
    request: HostFeatureSupportedRequest,
) -> Result<HostFeatureSupportedResponse, GenericError> {
    platform.feature_supported(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use truapi::v01;

    struct AlwaysSupported;

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
