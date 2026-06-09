//! Versioned wrappers for [`Navigation`](crate::api::Navigation) methods.

use crate::v01;

versioned_type! {
    pub enum HostNavigateToRequest { V1 => v01::HostNavigateToRequest }
    pub enum HostNavigateToResponse { V1 }
    pub enum HostNavigateToError { V1 => v01::HostNavigateToError }
    pub enum HostRouteGetRequest { V1 }
    pub enum HostRouteGetResponse { V1 => v01::HostRouteGetResponse }
    pub enum HostRouteGetError { V1 => v01::GenericError }
    pub enum HostRouteSetRequest { V1 => v01::HostRouteSetRequest }
    pub enum HostRouteSetResponse { V1 }
    pub enum HostRouteSetError { V1 => v01::GenericError }
    pub enum HostRouteChangedItem { V1 => v01::HostRouteChangedItem }
}
