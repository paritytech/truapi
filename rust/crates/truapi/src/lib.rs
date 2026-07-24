//! TrUAPI trait and type definitions for the host product SDK.
//!
//! Concrete wire types live in per-version modules. Versioned envelopes are in
//! [`versioned`].

#![allow(async_fn_in_trait)]

use core::convert::Infallible;
use core::fmt;
use core::future::Future;
use core::mem;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use core::time::Duration;
use std::sync::Arc;
use std::sync::Mutex;

use futures::Stream;
use parity_scale_codec::{Decode, Encode};

pub mod api;
pub mod v01;
pub mod versioned;

/// Latest-version protocol payload types, unwrapped from their versioned
/// envelopes. Runtime code should use these instead of per-version modules.
pub mod latest {
    use crate::versioned::{self, Versioned};

    pub use crate::v01::{
        AccountId, AllocatableResource, AllocationOutcome, ContextualAlias, DerivationIndex,
        GenericError, HostSignPayloadData, NotificationId, OperationStartedResult,
        ProductAccountId, ProductProofContext, RawPayload, RemotePermission, RingLocation,
        RuntimeApi, RuntimeSpec, RuntimeType, StorageQueryItem, StorageQueryType,
        StorageResultItem, ThemeVariant, TxPayloadExtension,
    };

    /// Latest payload type of a versioned envelope.
    pub type LatestOf<T> = <T as Versioned>::Latest;

    /// Ring VRF proof creation result.
    pub type HostAccountCreateProofResponse =
        LatestOf<versioned::account::HostAccountCreateProofResponse>;
    /// Contextual alias derivation result.
    pub type HostAccountGetAliasResponse =
        LatestOf<versioned::account::HostAccountGetAliasResponse>;
    /// Transaction creation result.
    pub type HostCreateTransactionResponse =
        LatestOf<versioned::signing::HostCreateTransactionResponse>;
    /// Device-capability permission request.
    pub type HostDevicePermissionRequest =
        LatestOf<versioned::permissions::HostDevicePermissionRequest>;
    /// Device-capability permission outcome.
    pub type HostDevicePermissionResponse =
        LatestOf<versioned::permissions::HostDevicePermissionResponse>;
    /// Feature-support query.
    pub type HostFeatureSupportedRequest = LatestOf<versioned::system::HostFeatureSupportedRequest>;
    /// Feature-support query result.
    pub type HostFeatureSupportedResponse =
        LatestOf<versioned::system::HostFeatureSupportedResponse>;
    /// Local storage operation error.
    pub type HostLocalStorageReadError =
        LatestOf<versioned::local_storage::HostLocalStorageReadError>;
    /// Navigation request error.
    pub type HostNavigateToError = LatestOf<versioned::system::HostNavigateToError>;
    /// Push notification scheduling request.
    pub type HostPushNotificationRequest =
        LatestOf<versioned::notifications::HostPushNotificationRequest>;
    /// Push notification scheduling result.
    pub type HostPushNotificationResponse =
        LatestOf<versioned::notifications::HostPushNotificationResponse>;
    /// Login request error.
    pub type HostRequestLoginError = LatestOf<versioned::account::HostRequestLoginError>;
    /// Login request result.
    pub type HostRequestLoginResponse = LatestOf<versioned::account::HostRequestLoginResponse>;
    /// Batched resource pre-allocation request.
    pub type HostRequestResourceAllocationRequest =
        LatestOf<versioned::resource_allocation::HostRequestResourceAllocationRequest>;
    /// Per-resource allocation outcomes.
    pub type HostRequestResourceAllocationResponse =
        LatestOf<versioned::resource_allocation::HostRequestResourceAllocationResponse>;
    /// Extrinsic payload signing request for a product account.
    pub type HostSignPayloadRequest = LatestOf<versioned::signing::HostSignPayloadRequest>;
    /// Signing operation result.
    pub type HostSignPayloadResponse = LatestOf<versioned::signing::HostSignPayloadResponse>;
    /// Extrinsic payload signing request for a legacy account.
    pub type HostSignPayloadWithLegacyAccountRequest =
        LatestOf<versioned::signing::HostSignPayloadWithLegacyAccountRequest>;
    /// Raw-bytes signing request for a product account.
    pub type HostSignRawRequest = LatestOf<versioned::signing::HostSignRawRequest>;
    /// Raw-bytes signing request for a legacy account.
    pub type HostSignRawWithLegacyAccountRequest =
        LatestOf<versioned::signing::HostSignRawWithLegacyAccountRequest>;
    /// Transaction creation payload for a legacy account.
    pub type LegacyAccountTxPayload =
        LatestOf<versioned::signing::HostCreateTransactionWithLegacyAccountRequest>;
    /// Preimage submission error.
    pub type PreimageSubmitError = LatestOf<versioned::preimage::RemotePreimageSubmitError>;
    /// Transaction creation payload for a product account.
    pub type ProductAccountTxPayload = LatestOf<versioned::signing::HostCreateTransactionRequest>;
    /// Chain-head subscription item.
    pub type RemoteChainHeadFollowItem = LatestOf<versioned::chain::RemoteChainHeadFollowItem>;
    /// Chain-head subscription request.
    pub type RemoteChainHeadFollowRequest =
        LatestOf<versioned::chain::RemoteChainHeadFollowRequest>;
    /// Chain-head storage query request.
    pub type RemoteChainHeadStorageRequest =
        LatestOf<versioned::chain::RemoteChainHeadStorageRequest>;
    /// Chain-head storage query result.
    pub type RemoteChainHeadStorageResponse =
        LatestOf<versioned::chain::RemoteChainHeadStorageResponse>;
    /// Remote-operation permission request.
    pub type RemotePermissionRequest = LatestOf<versioned::permissions::RemotePermissionRequest>;
    /// Remote-operation permission outcome.
    pub type RemotePermissionResponse = LatestOf<versioned::permissions::RemotePermissionResponse>;
}

pub use truapi_macros::wire;

/// Per-message id carried from the transport frame.
pub type RequestId = String;

/// Framework-level outcomes shared by API methods.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CallError<D> {
    /// Method-specific failure.
    Domain(D),
    /// The caller is not allowed to perform this operation.
    Denied,
    /// The host does not support this operation.
    Unsupported,
    /// The incoming request payload could not be decoded or validated.
    MalformedFrame {
        /// Why decoding or validation failed.
        reason: String,
    },
    /// Host-side failure with a diagnostic reason.
    HostFailure {
        /// Diagnostic reason for the failure.
        reason: String,
    },
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
/// tokens fire when a runtime explicitly cancels them or attaches a timeout.
/// Subscription runtimes can cancel this token when the peer sends `_stop` or
/// disconnects.
#[derive(Clone, Default)]
pub struct CancellationToken {
    inner: Arc<CancellationInner>,
}

#[derive(Default)]
struct CancellationInner {
    state: Mutex<CancellationState>,
}

#[derive(Default)]
struct CancellationState {
    reason: Option<CancellationReason>,
    next_id: u64,
    wakers: Vec<(u64, Waker)>,
}

/// Cause attached to a cancelled call.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum CancellationReason {
    /// The caller or runtime explicitly cancelled the call.
    #[display("cancelled")]
    Cancelled,
    /// The call exceeded the configured timeout.
    #[display("timed out after {}", format_timeout(timeout))]
    TimedOut {
        /// Timeout that elapsed.
        timeout: Duration,
    },
}

/// Render a timeout as whole seconds when possible, milliseconds otherwise.
fn format_timeout(timeout: &Duration) -> String {
    if timeout.subsec_millis() == 0 {
        format!("{}s", timeout.as_secs())
    } else {
        format!("{}ms", timeout.as_millis())
    }
}

/// Future resolved when a [`CancellationToken`] is cancelled.
pub struct CancellationFuture {
    inner: Arc<CancellationInner>,
    id: Option<u64>,
}

impl fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CancellationToken")
            .field("reason", &self.reason())
            .finish_non_exhaustive()
    }
}

impl CancellationToken {
    /// Mark the token as cancelled.
    pub fn cancel(&self) {
        self.cancel_with_reason(CancellationReason::Cancelled);
    }

    /// Mark the token as cancelled with an explicit `reason`.
    pub fn cancel_with_reason(&self, reason: CancellationReason) {
        let wakers = {
            let mut state = self.inner.state.lock().expect("cancel state poisoned");
            if state.reason.is_some() {
                return;
            }
            state.reason = Some(reason);
            mem::take(&mut state.wakers)
        };
        for (_, waker) in wakers {
            waker.wake();
        }
    }

    /// Returns the cancellation reason, if cancellation has been requested.
    pub fn reason(&self) -> Option<CancellationReason> {
        self.inner
            .state
            .lock()
            .expect("cancel state poisoned")
            .reason
            .clone()
    }

    /// Returns whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.reason().is_some()
    }

    /// Future resolved with the cancellation reason when cancellation is requested.
    pub fn cancelled(&self) -> CancellationFuture {
        CancellationFuture {
            inner: self.inner.clone(),
            id: None,
        }
    }
}

impl Future for CancellationFuture {
    type Output = CancellationReason;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let mut state = this.inner.state.lock().expect("cancel state poisoned");
        if let Some(reason) = state.reason.clone() {
            this.id = None;
            return Poll::Ready(reason);
        }

        if let Some(id) = this.id
            && let Some((_, waker)) = state
                .wakers
                .iter_mut()
                .find(|(waiter_id, _)| *waiter_id == id)
        {
            if !waker.will_wake(cx.waker()) {
                *waker = cx.waker().clone();
            }
            return Poll::Pending;
        }

        state.next_id = state.next_id.wrapping_add(1);
        let id = state.next_id;
        state.wakers.push((id, cx.waker().clone()));
        this.id = Some(id);
        Poll::Pending
    }
}

impl Drop for CancellationFuture {
    fn drop(&mut self) {
        let Some(id) = self.id.take() else {
            return;
        };
        let mut state = self.inner.state.lock().expect("cancel state poisoned");
        if state.reason.is_some() {
            return;
        }
        state.wakers.retain(|(waiter_id, _)| *waiter_id != id);
    }
}

/// Ambient context passed to every trait method.
#[derive(Clone, Default)]
pub struct CallContext {
    request_id: RequestId,
    cancel: CancellationToken,
    timeout: Option<Duration>,
}

impl CallContext {
    /// Construct a context bound to the given `request_id` with a fresh cancellation token.
    pub fn with_request_id(request_id: RequestId) -> Self {
        Self {
            request_id,
            cancel: CancellationToken::default(),
            timeout: None,
        }
    }

    /// Construct a context from explicit `request_id` and `cancel` parts.
    pub fn with_parts(request_id: RequestId, cancel: CancellationToken) -> Self {
        Self {
            request_id,
            cancel,
            timeout: None,
        }
    }

    /// Attach a timeout to this call.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = Some(timeout);
    }

    /// Return the request id this context is associated with.
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Return the cancellation token that signals when the call should abort.
    pub fn cancel(&self) -> &CancellationToken {
        &self.cancel
    }

    /// Return the timeout attached to this call, if any.
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_context_timeout_can_be_set_and_replaced() {
        let default = Duration::from_secs(180);
        let explicit = Duration::from_millis(25);

        let mut cx = CallContext::with_request_id("request-1".to_string());
        assert_eq!(cx.timeout(), None);
        cx.set_timeout(default);
        assert_eq!(cx.timeout(), Some(default));

        cx.set_timeout(explicit);
        assert_eq!(cx.timeout(), Some(explicit));
    }

    #[test]
    fn cancellation_token_clones_share_cancellation() {
        let token = CancellationToken::default();
        let cloned = token.clone();
        let wait = cloned.cancelled();

        token.cancel();

        let reason = futures::executor::block_on(wait);
        assert_eq!(reason, CancellationReason::Cancelled);
        assert!(cloned.is_cancelled());
    }
}
