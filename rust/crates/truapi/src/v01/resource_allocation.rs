use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum AllocatableResource {
    StatementStoreAllowance,
    BulletinAllowance,
    SmartContractAllowance(u32),
    AutoSigning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum AllocationOutcome {
    Allocated,
    Rejected,
    NotAvailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRequestResourceAllocationRequest {
    /// Resources to allocate.
    pub resources: Vec<AllocatableResource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRequestResourceAllocationResponse {
    /// Per-resource allocation outcomes, in the same order as the request.
    pub outcomes: Vec<AllocationOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ResourceAllocationError {
    Unknown { reason: String },
}
