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
    pub resources: Vec<AllocatableResource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRequestResourceAllocationResponse {
    pub outcomes: Vec<AllocationOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ResourceAllocationError {
    Unknown { reason: String },
}
