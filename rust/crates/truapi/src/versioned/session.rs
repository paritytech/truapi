//! Versioned wrappers for [`Session`](crate::api::Session) methods.

use crate::v01;

versioned_type! {
    pub enum HostSessionLifecycleSubscribeRequest { V1 => v01::HostSessionLifecycleSubscribeRequest }
    pub enum HostSessionLifecycleSubscribeItem { V1 => v01::HostSessionLifecycleSubscribeItem }
    pub enum HostSessionLifecycleSubscribeError { V1 => v01::HostSessionLifecycleSubscribeError }
}
