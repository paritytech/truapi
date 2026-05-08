//! TrUAPI trait and type definitions for the dotli product SDK.
//!
//! This crate provides two protocol versions as separate modules:
//!
//! - [`v01`] -- Protocol v0.1 (stable).
//! - [`v02`] -- Protocol v0.2.

#![forbid(unsafe_code)]

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::Stream;

pub mod api;
pub mod v01;
pub mod v02;
pub mod versioned;

pub use truapi_macros::wire;

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
    /// Construct an empty context with a fresh cancellation token.
    pub fn new() -> Self {
        Self::with_request_id(String::new())
    }

    /// Construct a context bound to the given `request_id` with a fresh cancellation token.
    pub fn with_request_id(request_id: RequestId) -> Self {
        Self {
            request_id,
            cancel: CancellationToken::new(),
        }
    }

    /// Construct a context from explicit `request_id` and `cancel` parts.
    pub fn with_parts(request_id: RequestId, cancel: CancellationToken) -> Self {
        Self { request_id, cancel }
    }

    /// Return the request id this context is associated with.
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Return the cancellation token that signals when the call should abort.
    pub fn cancel(&self) -> &CancellationToken {
        &self.cancel
    }
}

impl Default for CallContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle to an active subscription. Implements [`Stream`] to yield values
/// pushed by the host. Drop to unsubscribe.
pub struct Subscription<T> {
    inner: Pin<Box<dyn Stream<Item = T> + Send>>,
}

impl<T> Stream for Subscription<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

impl<T> Subscription<T> {
    /// Creates a new subscription from a boxed stream.
    pub fn new(stream: Pin<Box<dyn Stream<Item = T> + Send>>) -> Self {
        Self { inner: stream }
    }

    /// Creates a subscription that yields no items. Useful as a placeholder for
    /// default "unavailable" trait bodies where the dispatcher will discard the
    /// stream and emit an Interrupt frame.
    pub fn empty() -> Self
    where
        T: Send + 'static,
    {
        Self::new(Box::pin(futures::stream::empty()))
    }
}
