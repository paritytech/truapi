//! Unified [`Entropy`] trait.

use crate::versioned::entropy::{
    HostDeriveEntropyError, HostDeriveEntropyRequest, HostDeriveEntropyResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Deterministic entropy derivation.
pub trait Entropy: Send + Sync {
    /// Derive deterministic entropy.
    ///
    /// ```ts
    /// const result = await truapi.entropy.derive({
    ///   context: "0x70726f647563742d6b6579",
    /// });
    /// result.match(
    ///   (value) => console.log(value),
    ///   (error) => console.error(error),
    /// );
    /// ```
    #[wire(request_id = 108)]
    async fn derive(
        &self,
        _cx: &CallContext,
        _request: HostDeriveEntropyRequest,
    ) -> Result<HostDeriveEntropyResponse, CallError<HostDeriveEntropyError>> {
        Err(CallError::unavailable())
    }
}
