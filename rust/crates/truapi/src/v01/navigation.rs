use parity_scale_codec::{Decode, Encode};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostNavigateToError {
    PermissionDenied,
    Unknown { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNavigateToRequest {
    /// URL to open.
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRouteGetResponse {
    /// Current route the host holds for this app, or `None` when the app is at its home.
    pub route: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRouteSetRequest {
    /// Opaque route segment defined by the app.
    pub route: String,
    /// `true` replaces the current history entry; `false` pushes a new one.
    pub replace: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRouteChangedItem {
    /// New route, or `None` when the user is at the app's home.
    pub route: Option<String>,
}
