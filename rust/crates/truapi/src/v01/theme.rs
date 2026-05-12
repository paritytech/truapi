use parity_scale_codec::{Decode, Encode};

/// Host UI theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum Theme {
    /// Light appearance.
    Light,
    /// Dark appearance.
    Dark,
}

/// Item emitted by the theme subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct HostThemeSubscribeItem {
    /// Current theme.
    pub theme: Theme,
}
