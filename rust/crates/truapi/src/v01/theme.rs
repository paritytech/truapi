use parity_scale_codec::{Decode, Encode};

/// Identifies a named theme.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ThemeName {
    /// A custom named theme.
    Custom(String),
    /// The host's default theme.
    Default,
}

/// Light or dark variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ThemeVariant {
    /// Light appearance.
    Light,
    /// Dark appearance.
    Dark,
}

/// Current theme state pushed to subscribers.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostThemeSubscribeItem {
    /// Theme name.
    pub name: ThemeName,
    /// Light or dark variant.
    pub variant: ThemeVariant,
}
