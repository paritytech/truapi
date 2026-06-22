//! Unified [`ResourceAllocation`] trait.

use crate::versioned::resource_allocation::{
    HostRequestResourceAllocationError, HostRequestResourceAllocationRequest,
    HostRequestResourceAllocationResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Resource pre-allocation (allowance management).
pub trait ResourceAllocation: Send + Sync {
    /// Request the host to pre-allocate one or more resources.
    ///
    /// # Permissions
    ///
    /// - **auth**: required
    /// - **prompt**: per-resource allocation confirmation
    ///
    /// ```ts
    /// const result = await truapi.resourceAllocation.request({
    ///   resources: [
    ///     { tag: "StatementStoreAllowance" },
    ///     { tag: "AutoSigning" },
    ///   ],
    /// });
    /// assert(result.isOk(), "request failed:", result);
    /// console.log("resource allocation result:", result.value);
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
