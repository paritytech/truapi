use parity_scale_codec::{Decode, Encode};

/// Response containing the app's current route as held by the host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRouteGetResponse {
    /// Current route the host holds for this app, or `None` when the app is at its home.
    pub route: Option<String>,
}

/// Request to publish the app's current route to the host's address bar.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRouteSetRequest {
    /// Opaque route segment defined by the app.
    pub route: String,
    /// `true` replaces the current history entry; `false` pushes a new one.
    pub replace: bool,
}

/// Subscription item emitted when the route changes from outside the app.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostRouteChangedItem {
    /// New route, or `None` when the user is at the app's home.
    pub route: Option<String>,
}
