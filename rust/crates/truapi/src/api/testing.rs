//! Debug-only API used to verify wire-version and framework-error handling.

use crate::v01;
use crate::v02;
use crate::versioned::testing::{
    TestingVersionProbeError, TestingVersionProbeRequest, TestingVersionProbeResponse,
};
use crate::wire;
use crate::{CallContext, CallError};

/// Development-only probes for generated client/runtime compatibility.
pub trait Testing: Send + Sync {
    /// Echo the request version back to the caller.
    ///
    /// ```ts
    /// const result = await truapi.testing.versionProbe({
    ///   message: "hello from V2",
    ///   marker: 42,
    /// });
    /// assert(result.isOk(), "testing version probe failed:", result);
    /// console.log("testing version probe:", result.value);
    /// ```
    #[wire(request_id = 164)]
    async fn version_probe(
        &self,
        _cx: &CallContext,
        request: TestingVersionProbeRequest,
    ) -> Result<TestingVersionProbeResponse, CallError<TestingVersionProbeError>> {
        match request {
            TestingVersionProbeRequest::V1(inner) => Ok(TestingVersionProbeResponse::V1(
                v01::TestingVersionProbeResponse {
                    received_version: 1,
                    message: inner.message,
                },
            )),
            TestingVersionProbeRequest::V2(inner) => Ok(TestingVersionProbeResponse::V2(
                v02::TestingVersionProbeResponse {
                    received_version: 2,
                    message: inner.message,
                    marker: inner.marker,
                },
            )),
        }
    }

    /// Echo a framework/domain error on the public response channel.
    ///
    /// ```ts
    /// const result = await truapi.testing.echoError({
    ///   error: { tag: "HostFailure", value: { reason: "forced by test" } },
    /// });
    /// assert(result.isErr(), "expected host failure");
    /// console.log("echo error:", result.error);
    /// ```
    #[wire(request_id = 166)]
    async fn echo_error(
        &self,
        _cx: &CallContext,
        request: v01::EchoErrorRequest,
    ) -> Result<(), CallError<v01::TestingVersionProbeError>> {
        Err(request.error)
    }
}
