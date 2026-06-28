//! Debug-only API used to verify wire-version and framework-error handling.

use crate::v01;
use crate::v02;
use crate::versioned::testing::{
    TestingFrameworkErrorRequest, TestingProbeError, TestingProbeRequest, TestingProbeResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Development-only probes for generated client/runtime compatibility.
pub trait Testing: Send + Sync {
    /// Echo the request version back to the caller.
    ///
    /// ```ts
    /// const result = await truapi.testing.probe({
    ///   message: "hello from V2",
    ///   marker: 42,
    /// });
    /// assert(result.isOk(), "testing probe failed:", result);
    /// console.log("testing probe:", result.value);
    /// ```
    #[wire(request_id = 164)]
    async fn probe(
        &self,
        _cx: &CallContext,
        request: TestingProbeRequest,
    ) -> Result<TestingProbeResponse, CallError<TestingProbeError>> {
        match request {
            TestingProbeRequest::V1(inner) => {
                Ok(TestingProbeResponse::V1(v01::TestingProbeResponse {
                    received_version: 1,
                    message: inner.message,
                }))
            }
            TestingProbeRequest::V2(inner) => {
                Ok(TestingProbeResponse::V2(v02::TestingProbeResponse {
                    received_version: 2,
                    message: inner.message,
                    marker: inner.marker,
                }))
            }
        }
    }

    /// Force a framework-level error on the public response channel.
    ///
    /// ```ts
    /// const result = await truapi.testing.frameworkError({
    ///   error: "HostFailure",
    /// });
    /// assert(result.isErr(), "expected host failure");
    /// console.log("framework error:", result.error);
    /// ```
    #[wire(request_id = 166)]
    async fn framework_error(
        &self,
        _cx: &CallContext,
        request: TestingFrameworkErrorRequest,
    ) -> Result<(), CallError<TestingProbeError>> {
        let TestingFrameworkErrorRequest::V1(inner) = request;
        force_framework_error(inner.error)
    }
}

fn force_framework_error<E>(error: v01::TestingFrameworkError) -> Result<(), CallError<E>> {
    match error {
        v01::TestingFrameworkError::Denied => Err(CallError::Denied),
        v01::TestingFrameworkError::Unsupported => Err(CallError::Unsupported),
        v01::TestingFrameworkError::MalformedFrame => Err(CallError::MalformedFrame {
            reason: "forced by testing.framework_error".to_string(),
        }),
        v01::TestingFrameworkError::HostFailure => Err(CallError::HostFailure {
            reason: "forced by testing.framework_error".to_string(),
        }),
    }
}
