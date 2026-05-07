//! Framework-level call outcomes and per-call lifecycle context.
//!
//! Handlers return [`CallError`] for framework outcomes instead of recording
//! side effects on [`CallContext`]. Existing per-method wire error enums remain
//! wire/client DTOs; handler authors should put method-specific failures in the
//! `Domain` variant and use top-level variants for framework outcomes.

use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Per-message id carried from the transport frame.
pub type RequestId = String;

/// Framework-level outcomes shared by API methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError<D> {
    /// Method-specific failure.
    Domain(D),
    /// The caller is not allowed to perform this operation.
    Denied,
    /// The host does not support this operation.
    Unsupported,
    /// The incoming request payload could not be decoded or validated.
    MalformedFrame { reason: String },
    /// Host-side failure with a diagnostic reason.
    HostFailure { reason: String },
}

impl<D> CallError<D> {
    /// Convenience for default handlers whose implementation is not wired.
    pub fn unavailable() -> Self {
        Self::HostFailure {
            reason: "unavailable".into(),
        }
    }
}

/// Error type for methods with no domain-specific failures.
pub type FrameworkOnlyError = CallError<Infallible>;

/// Cooperative cancellation token exposed to handlers.
///
/// Current one-shot request frames have no cancel control message, so request
/// tokens only fire when a future runtime explicitly cancels them. Subscription
/// runtimes can cancel this token when the peer sends `_stop` or disconnects.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Create a token in the non-cancelled state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark the token as cancelled.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Returns whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// Ambient context passed to every trait method.
pub struct CallContext {
    request_id: RequestId,
    cancel: CancellationToken,
}

impl CallContext {
    pub fn new() -> Self {
        Self::with_request_id(String::new())
    }

    pub fn with_request_id(request_id: RequestId) -> Self {
        Self {
            request_id,
            cancel: CancellationToken::new(),
        }
    }

    pub fn with_parts(request_id: RequestId, cancel: CancellationToken) -> Self {
        Self { request_id, cancel }
    }

    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn cancel(&self) -> &CancellationToken {
        &self.cancel
    }
}

impl Default for CallContext {
    fn default() -> Self {
        Self::new()
    }
}
