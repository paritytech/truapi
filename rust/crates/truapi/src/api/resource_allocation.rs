//! Unified [`ResourceAllocation`] trait.

use crate::versioned::resource_allocation::{
    HostRequestResourceAllocationError, HostRequestResourceAllocationRequest,
    HostRequestResourceAllocationResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Resource pre-allocation (allowance management).
#[async_trait::async_trait]
pub trait ResourceAllocation: Send + Sync {
    /// Request the host to pre-allocate one or more resources.
    ///
    /// ```truapi-client-example
    /// import {
    ///   type Client,
    ///   type HostRequestResourceAllocationResponse,
    /// } from "@parity/truapi";
    ///
    /// export async function requestAllocation(
    ///   truapi: Client,
    /// ): Promise<HostRequestResourceAllocationResponse> {
    ///   const result =
    ///     await truapi.resourceAllocation.request({
    ///       resources: [
    ///         { tag: "StatementStoreAllowance" },
    ///         { tag: "AutoSigning" },
    ///       ],
    ///     });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 130)]
    async fn request(
        &self,
        _cx: &CallContext,
        _request: HostRequestResourceAllocationRequest,
    ) -> Result<HostRequestResourceAllocationResponse, CallError<HostRequestResourceAllocationError>>
    {
        Err(CallError::unavailable())
    }
}
