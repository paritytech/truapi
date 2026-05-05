//! Framework-level call outcomes that live alongside domain errors.
//!
//! [`CallContext`] is passed to every unified trait method. Implementations
//! signal non-domain outcomes (unsupported, unavailable, denied, host-failure)
//! by calling `cx.fail_*()`. The dispatcher inspects [`CallContext::take_failure`]
//! after the method returns and encodes an `Interrupt` frame if one was
//! recorded, otherwise it encodes the method's `Result` normally.

use std::sync::Mutex;

const RUNTIME_ERROR_PREFIX: &str = "truapi-runtime";

/// Classification of framework-level failures separate from domain errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeFailureKind {
    Unsupported,
    Unavailable,
    Denied,
    HostFailure,
    MalformedFrame,
}

impl RuntimeFailureKind {
    pub fn label(self) -> &'static str {
        match self {
            RuntimeFailureKind::Unsupported => "unsupported",
            RuntimeFailureKind::Unavailable => "unavailable",
            RuntimeFailureKind::Denied => "denied",
            RuntimeFailureKind::HostFailure => "host-failure",
            RuntimeFailureKind::MalformedFrame => "malformed-frame",
        }
    }
}

/// A framework-level failure tagged with the method it originated from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeFailure {
    kind: RuntimeFailureKind,
    method: &'static str,
    reason: Option<String>,
}

impl RuntimeFailure {
    pub fn unsupported(method: &'static str) -> Self {
        Self {
            kind: RuntimeFailureKind::Unsupported,
            method,
            reason: None,
        }
    }

    pub fn unavailable(method: &'static str) -> Self {
        Self {
            kind: RuntimeFailureKind::Unavailable,
            method,
            reason: None,
        }
    }

    pub fn denied(method: &'static str) -> Self {
        Self {
            kind: RuntimeFailureKind::Denied,
            method,
            reason: None,
        }
    }

    pub fn host_failure(method: &'static str, reason: impl Into<String>) -> Self {
        Self {
            kind: RuntimeFailureKind::HostFailure,
            method,
            reason: Some(reason.into()),
        }
    }

    pub fn malformed_frame(method: &'static str, reason: impl Into<String>) -> Self {
        Self {
            kind: RuntimeFailureKind::MalformedFrame,
            method,
            reason: Some(reason.into()),
        }
    }

    pub fn kind(&self) -> RuntimeFailureKind {
        self.kind
    }

    pub fn method(&self) -> &'static str {
        self.method
    }

    pub fn reason(&self) -> String {
        match &self.reason {
            Some(reason) => format!(
                "{RUNTIME_ERROR_PREFIX}:{}:{}: {reason}",
                self.kind.label(),
                self.method,
            ),
            None => format!(
                "{RUNTIME_ERROR_PREFIX}:{}:{}",
                self.kind.label(),
                self.method
            ),
        }
    }
}

/// Ambient context passed to every trait method. Implementations call
/// `fail_*()` to signal a framework outcome; the dispatcher consumes the
/// recorded failure via [`Self::take_failure`] after the method returns.
pub struct CallContext {
    method: &'static str,
    request_id: String,
    failure: Mutex<Option<RuntimeFailure>>,
}

impl CallContext {
    pub fn new(method: &'static str) -> Self {
        Self::with_request_id(method, String::new())
    }

    pub fn with_request_id(method: &'static str, request_id: String) -> Self {
        Self {
            method,
            request_id,
            failure: Mutex::new(None),
        }
    }

    pub fn method(&self) -> &'static str {
        self.method
    }

    /// Per-message id carried from the transport frame. For subscription
    /// starts this doubles as the follow-subscription-id.
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn fail_unsupported(&self) {
        self.record(RuntimeFailure::unsupported(self.method));
    }

    pub fn fail_unavailable(&self) {
        self.record(RuntimeFailure::unavailable(self.method));
    }

    pub fn fail_denied(&self) {
        self.record(RuntimeFailure::denied(self.method));
    }

    pub fn fail_host_failure(&self, detail: impl Into<String>) {
        self.record(RuntimeFailure::host_failure(self.method, detail));
    }

    /// Record an externally-constructed [`RuntimeFailure`]. Used when a lower
    /// layer (e.g. the chain runtime) already produced one.
    pub fn fail_from(&self, failure: RuntimeFailure) {
        self.record(failure);
    }

    /// Removes and returns the recorded failure, if any. Intended for the
    /// dispatcher to call once after the trait method resolves.
    pub fn take_failure(&self) -> Option<RuntimeFailure> {
        self.failure
            .lock()
            .expect("CallContext mutex poisoned")
            .take()
    }

    fn record(&self, failure: RuntimeFailure) {
        let mut slot = self.failure.lock().expect("CallContext mutex poisoned");
        if slot.is_none() {
            *slot = Some(failure);
        }
    }
}
