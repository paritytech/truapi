use parity_scale_codec::{Decode, Encode};

/// A resource the product can request the host to pre-allocate.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum AllocatableResource {
    /// Statement store allowance.
    StatementStoreAllowance,
    /// Bulletin board allowance.
    BulletinAllowance,
    /// Smart contract allowance with a derivation index.
    SmartContractAllowance(u32),
    /// Auto-signing capability.
    AutoSigning,
}

/// Outcome of a resource allocation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum AllocationOutcome {
    /// Resource was allocated.
    Allocated,
    /// User or host rejected the allocation.
    Rejected,
    /// Resource type is not available on this host.
    NotAvailable,
}

/// Request to allocate one or more resources.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRequestResourceAllocationRequest {
    /// Resources to allocate.
    pub resources: Vec<AllocatableResource>,
}

/// Response containing the outcome for each requested resource.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRequestResourceAllocationResponse {
    /// Per-resource allocation outcomes, in the same order as the request.
    pub outcomes: Vec<AllocationOutcome>,
}

/// Resource allocation error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ResourceAllocationError {
    /// Catch-all.
    Unknown { reason: String },
}
