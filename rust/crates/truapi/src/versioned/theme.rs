//! Versioned wrappers for [`Theme`](crate::api::Theme) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostThemeSubscribeItem { V1 => v01::HostThemeSubscribeItem }
}
