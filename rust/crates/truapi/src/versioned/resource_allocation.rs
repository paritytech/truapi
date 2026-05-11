//! Versioned wrappers for [`ResourceAllocation`](crate::api::ResourceAllocation) methods.

use crate::v01;

versioned_type! {
    /// Versioned wrapper for [`v01::HostRequestResourceAllocationRequest`].
    pub enum HostRequestResourceAllocationRequest { V1 => v01::HostRequestResourceAllocationRequest }
    /// Versioned wrapper for [`v01::HostRequestResourceAllocationResponse`].
    pub enum HostRequestResourceAllocationResponse { V1 => v01::HostRequestResourceAllocationResponse }
    /// Versioned wrapper for [`v01::ResourceAllocationError`].
    pub enum HostRequestResourceAllocationError { V1 => v01::ResourceAllocationError }
}
