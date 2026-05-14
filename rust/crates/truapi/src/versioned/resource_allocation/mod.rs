//! Versioned wrappers for [`ResourceAllocation`](crate::api::ResourceAllocation) methods.

use crate::v01;

versioned_type! {
    pub enum HostRequestResourceAllocationRequest { V1 => v01::HostRequestResourceAllocationRequest }
    pub enum HostRequestResourceAllocationResponse { V1 => v01::HostRequestResourceAllocationResponse }
    pub enum HostRequestResourceAllocationError { V1 => v01::ResourceAllocationError }
}
