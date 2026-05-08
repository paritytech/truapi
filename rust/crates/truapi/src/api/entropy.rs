//! Unified [`EntropyDerivation`] trait (V0.2+).

use crate::versioned::entropy::{
    HostDeriveEntropyError, HostDeriveEntropyRequest, HostDeriveEntropyResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Deterministic entropy derivation.
///
/// The default body returns [`CallError::HostFailure`] with an `unavailable`
/// reason; hosts override only if they can derive entropy.
#[async_trait::async_trait]
pub trait EntropyDerivation: Send + Sync {
    /// Derive 32 bytes of entropy from the user's root BIP-39 entropy for the
    /// given key.
    ///
    /// ```truapi-playground-request
    /// { "context": "0x" }
    /// ```
    ///
    /// ```truapi-client-example
    /// import { createClient, createTransport, type Provider } from "@parity/truapi";
    ///
    /// export async function deriveEntropy(provider: Provider, context: Uint8Array) {
    ///   const truapi = createClient(createTransport(provider));
    ///
    ///   const result = await truapi.entropyDerivation.deriveEntropy({
    ///     context,
    ///   });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(id = 108)]
    async fn host_derive_entropy(
        &self,
        _cx: &CallContext,
        _request: HostDeriveEntropyRequest,
    ) -> Result<HostDeriveEntropyResponse, CallError<HostDeriveEntropyError>> {
        Err(CallError::unavailable())
    }
}
