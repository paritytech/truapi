use parity_scale_codec::{Decode, Encode};

/// A resource the host can pre-allocate on behalf of the product (RFC 0010).
///
/// For the slot-table allowances (`StatementStoreAllowance`,
/// `BulletinAllowance`, `SmartContractAllowance`), pre-allocation is
/// opportunistic and the host may also fulfil the allowance implicitly on the
/// first submission. `AutoSigning` must be requested explicitly through this
/// call.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum AllocatableResource {
    /// Statement Store slot allowance for the product's own allowance account.
    StatementStoreAllowance,
    /// Bulletin chain slot allowance for the product's own allowance account.
    BulletinAllowance,
    /// Pre-warmed PGAS balance for the smart-contract account at the given
    /// derivation index.
    SmartContractAllowance(u32),
    /// Permission to sign on the product's behalf without per-call user prompts.
    AutoSigning,
}

/// Outcome of allocating a single resource (RFC 0010).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum AllocationOutcome {
    /// Resource is now available for use.
    Allocated,
    /// User or host refused the allocation.
    Rejected,
    /// Host cannot provide this resource on the current chain or environment.
    NotAvailable,
}

/// Batched resource pre-allocation request (RFC 0010).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRequestResourceAllocationRequest {
    /// Resources to allocate.
    pub resources: Vec<AllocatableResource>,
}

/// Per-resource outcomes for a batched allocation request (RFC 0010).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRequestResourceAllocationResponse {
    /// Per-resource allocation outcomes, in the same order as the request.
    pub outcomes: Vec<AllocationOutcome>,
}

/// Error from [`crate::api::ResourceAllocation::request`].
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ResourceAllocationError {
    /// Catch-all.
    Unknown { reason: String },
}
