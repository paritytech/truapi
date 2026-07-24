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
    /// ```ts
    /// const result = await truapi.resourceAllocation.request({
    ///   resources: [
    ///     { tag: "StatementStoreAllowance" },
    ///     { tag: "BulletinAllowance" },
    ///     { tag: "SmartContractAllowance", value: { tag: "Left", value: 0 } },
    ///     { tag: "AutoSigning" },
    ///   ],
    /// });
    /// assert(result.isOk(), "request failed:", result);
    /// assert(result.value.outcomes.length === 4, "missing allocation outcomes:", result.value);
    /// assert(
    ///   result.value.outcomes.slice(0, 3).every((outcome) => outcome === "Allocated"),
    ///   "one or more on-chain allowances are unavailable:",
    ///   result.value,
    /// );
    /// assert(
    ///   result.value.outcomes[3] === "NotAvailable",
    ///   "AutoSigning support changed; update this example:",
    ///   result.value,
    /// );
    /// console.log("statement-store, bulletin, and smart-contract allowances allocated");
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
