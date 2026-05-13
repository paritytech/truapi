//! Unified [`Entropy`] trait.

use crate::versioned::entropy::{
    HostDeriveEntropyError, HostDeriveEntropyRequest, HostDeriveEntropyResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Deterministic entropy derivation.
#[async_trait::async_trait]
pub trait Entropy: Send + Sync {
    /// Derive deterministic entropy.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostDeriveEntropyResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function deriveEntropy(
    ///   truapi: Client,
    /// ): Promise<HostDeriveEntropyResponse> {
    ///   const result = await truapi.entropy.derive({
    ///     context: "0x70726f647563742d6b6579",
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
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
