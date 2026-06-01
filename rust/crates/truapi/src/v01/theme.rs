use parity_scale_codec::{Decode, Encode};

/// Identifies a named theme.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ThemeName {
    /// The host's default theme.
    Default,
    /// A custom named theme.
    Custom(String),
}

/// Light or dark variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ThemeVariant {
    Light,
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
