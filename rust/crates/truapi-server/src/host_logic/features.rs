//! Feature-detection delegation.
//!
//! `feature_supported` and `supported_chains` are platform syscalls: each
//! host owns the set of chains it can service. This module is a thin shim
//! that forwards through to [`truapi_platform::Features`], plus the in-core
//! RFC-0026 resolution that answers `get_chain_info` from the host's chain
//! set so per-request semantics (ordering, `NotSupported`) stay core-owned.

use truapi::latest::{RemoteChainInfoError, RemoteChainInfoRequest, RemoteChainInfoResponse};
use truapi::v01::{GenericError, HostFeatureSupportedRequest, HostFeatureSupportedResponse};
use truapi_platform::{Features, HostChainSet};

/// Forward a feature-support query to the platform implementation.
pub async fn feature_supported<P: Features + ?Sized>(
    platform: &P,
    request: HostFeatureSupportedRequest,
) -> Result<HostFeatureSupportedResponse, GenericError> {
    platform.feature_supported(request).await
}

/// Fetch the host's chain set from the platform implementation.
pub async fn supported_chains<P: Features + ?Sized>(
    platform: &P,
) -> Result<HostChainSet, GenericError> {
    platform.supported_chains().await
}

/// Resolve a `get_chain_info` request against the host's chain set: the
/// requested identifier's genesis hash plus the host's network, echoing the
/// identifier, or `NotSupported` when the host does not serve it.
pub fn chain_info(
    set: &HostChainSet,
    request: &RemoteChainInfoRequest,
) -> Result<RemoteChainInfoResponse, RemoteChainInfoError> {
    set.chains
        .iter()
        .find(|entry| entry.identifier == request.chain)
        .map(|entry| RemoteChainInfoResponse {
            network: set.network.clone(),
            chain: entry.identifier,
            genesis_hash: entry.genesis_hash,
        })
        .ok_or(RemoteChainInfoError::NotSupported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use truapi::latest::ChainIdentifier;
    use truapi_platform::HostChainEntry;

    fn paseo_set() -> HostChainSet {
        HostChainSet {
            network: "paseo".to_string(),
            chains: vec![
                HostChainEntry {
                    identifier: ChainIdentifier::AssetHub,
                    genesis_hash: [0xaa; 32],
                },
                HostChainEntry {
                    identifier: ChainIdentifier::People,
                    genesis_hash: [0xbb; 32],
                },
            ],
        }
    }

    struct AlwaysSupported;

    #[truapi_platform::async_trait]
    impl Features for AlwaysSupported {
        async fn feature_supported(
            &self,
            request: HostFeatureSupportedRequest,
        ) -> Result<HostFeatureSupportedResponse, GenericError> {
            assert!(matches!(request, HostFeatureSupportedRequest::Chain { .. }));
            Ok(HostFeatureSupportedResponse { supported: true })
        }

        async fn supported_chains(&self) -> Result<HostChainSet, GenericError> {
            Ok(paseo_set())
        }
    }

    struct AlwaysUnsupported;

    #[truapi_platform::async_trait]
    impl Features for AlwaysUnsupported {
        async fn feature_supported(
            &self,
            request: HostFeatureSupportedRequest,
        ) -> Result<HostFeatureSupportedResponse, GenericError> {
            assert!(matches!(request, HostFeatureSupportedRequest::Chain { .. }));
            Ok(HostFeatureSupportedResponse { supported: false })
        }

        async fn supported_chains(&self) -> Result<HostChainSet, GenericError> {
            Err(GenericError {
                reason: "no chains".to_string(),
            })
        }
    }

    fn req() -> HostFeatureSupportedRequest {
        HostFeatureSupportedRequest::Chain {
            genesis_hash: vec![0u8; 32],
        }
    }

    #[test]
    fn delegates_supported_to_platform() {
        let resp = futures::executor::block_on(feature_supported(&AlwaysSupported, req())).unwrap();
        assert!(resp.supported);
    }

    #[test]
    fn delegates_unsupported_to_platform() {
        let resp =
            futures::executor::block_on(feature_supported(&AlwaysUnsupported, req())).unwrap();
        assert!(!resp.supported);
    }

    #[test]
    fn delegates_supported_chains_to_platform() {
        let set = futures::executor::block_on(supported_chains(&AlwaysSupported)).unwrap();
        assert_eq!(set, paseo_set());
    }

    #[test]
    fn surfaces_supported_chains_platform_error() {
        let err = futures::executor::block_on(supported_chains(&AlwaysUnsupported)).unwrap_err();
        assert_eq!(err.reason, "no chains");
    }

    #[test]
    fn resolves_identifier_and_echoes_it() {
        let request = RemoteChainInfoRequest {
            chain: ChainIdentifier::People,
        };
        let response = chain_info(&paseo_set(), &request).unwrap();
        assert_eq!(response.network, "paseo");
        assert_eq!(response.chain, ChainIdentifier::People);
        assert_eq!(response.genesis_hash, [0xbb; 32]);
    }

    #[test]
    fn unserved_identifier_is_not_supported() {
        let request = RemoteChainInfoRequest {
            chain: ChainIdentifier::Bulletin,
        };
        let err = chain_info(&paseo_set(), &request).unwrap_err();
        assert_eq!(err, RemoteChainInfoError::NotSupported);
    }
}
