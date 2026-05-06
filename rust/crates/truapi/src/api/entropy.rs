//! Unified [`EntropyDerivation`] trait (V0.2+).

use crate::versioned::entropy::{
    HostDeriveEntropyError, HostDeriveEntropyRequest, HostDeriveEntropyResponse,
};
use crate::wire;
use crate::CallContext;

/// Deterministic entropy derivation.
///
/// The default body flags the call as unavailable through
/// [`CallContext::fail_unavailable`]; hosts override only if they can derive
/// entropy.
#[async_trait::async_trait]
pub trait EntropyDerivation: Send + Sync {
    /// Derive 32 bytes of entropy from the user's root BIP-39 entropy for the
    /// given key.
    #[wire(id = 108)]
    async fn host_derive_entropy(
        &self,
        cx: &CallContext,
        _request: HostDeriveEntropyRequest,
    ) -> Result<HostDeriveEntropyResponse, HostDeriveEntropyError> {
        cx.fail_unavailable();
        Ok(HostDeriveEntropyResponse::V2([0u8; 32]))
    }
}
