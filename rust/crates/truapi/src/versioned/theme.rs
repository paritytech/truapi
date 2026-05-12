//! Versioned wrappers for [`HostTheme`](crate::api::HostTheme) methods.

use crate::v01;

versioned_type! {
    pub enum HostThemeSubscribeItem { V1 => v01::HostThemeSubscribeItem }
}
