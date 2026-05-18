use parity_scale_codec::{Decode, Encode};

use super::GenericErr;

/// Milliseconds since the Unix epoch.
pub type TimestampMs = u64;

/// Host-assigned stable identifier for one lifecycle event.
pub type SessionLifecycleEventId = String;

/// Request to subscribe to host session lifecycle events.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostSessionLifecycleSubscribeRequest {
    /// Ask the host to replay the current lifecycle state when one is active.
    pub replay_current_state: bool,
}

/// Lifecycle event emitted by the host before a product transition.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostSessionLifecycleSubscribeItem {
    /// The product should checkpoint state before it is suspended.
    WillSuspend(SessionLifecycleRequest),
    /// The product should checkpoint state before its WebView may be evicted.
    WillEvict(SessionLifecycleRequest),
    /// The product should checkpoint state before it is closed.
    WillClose(SessionLifecycleRequest),
}

/// Details for a single lifecycle checkpoint request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SessionLifecycleRequest {
    /// Host-assigned event id for de-duplicating repeated notifications.
    pub event_id: SessionLifecycleEventId,
    /// Reason the host is asking the product to checkpoint state.
    pub reason: SessionLifecycleReason,
    /// Best-effort deadline for checkpoint completion.
    pub deadline_ms: Option<TimestampMs>,
}

/// Reason for a lifecycle checkpoint request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum SessionLifecycleReason {
    /// User switched away from this product in the host app switcher.
    AppSwitcher,
    /// Host application moved to the background.
    HostBackgrounded,
    /// Host application is terminating or restarting.
    HostTerminating,
    /// Platform memory pressure may evict the product WebView.
    MemoryPressure,
    /// User explicitly closed this product.
    UserClosedProduct,
    /// Host policy requires a checkpoint.
    HostPolicy,
}

/// Error from session lifecycle subscription setup.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostSessionLifecycleSubscribeError {
    /// The host does not support product session lifecycle events.
    Unsupported,
    /// Catch-all.
    Unknown(GenericErr),
}
