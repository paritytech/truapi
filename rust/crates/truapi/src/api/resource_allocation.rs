//! Unified [`ResourceAllocation`] trait.

use crate::versioned::resource_allocation::{
    HostRequestResourceAllocationError, HostRequestResourceAllocationRequest,
    HostRequestResourceAllocationResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Resource pre-allocation (allowance management).
///
/// Default methods return [`CallError::HostFailure`] with an `unavailable`
/// reason. Hosts override only the methods they actually support.
#[async_trait::async_trait]
pub trait ResourceAllocation: Send + Sync {
    /// Request the host to pre-allocate one or more resources (statement store
    /// allowance, bulletin allowance, smart contract allowance, auto-signing).
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
    ///     await truapi.resourceAllocation.requestResourceAllocation({
    ///       resources: ["StatementStoreAllowance", "AutoSigning"],
    ///     });
    ///
    ///   if (result.isErr()) throw result.error;
    ///   return result.value;
    /// }
    /// ```
    #[wire(request_id = 130)]
    async fn host_request_resource_allocation(
        &self,
        _cx: &CallContext,
        _request: HostRequestResourceAllocationRequest,
    ) -> Result<HostRequestResourceAllocationResponse, CallError<HostRequestResourceAllocationError>>
    {
        Err(CallError::unavailable())
    }
}
