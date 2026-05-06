//! TrUAPI trait and type definitions for the dotli product SDK.
//!
//! This crate provides two protocol versions as separate modules:
//!
//! - [`v01`] -- Protocol v0.1 (stable).
//! - [`v02`] -- Protocol v0.2.

#![forbid(unsafe_code)]

use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;

pub mod api;
pub mod failure;
pub mod serde_helpers;
pub mod v01;
pub mod v02;
pub mod versioned;

pub use failure::{CallContext, RuntimeFailure, RuntimeFailureKind};
pub use truapi_macros::wire;

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
