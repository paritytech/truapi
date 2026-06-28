//! Versioned wrappers for the debug-only [`Testing`](crate::api::Testing) API.

use crate::{v01, v02};

truapi_macros::versioned_type! {
    pub enum TestingProbeRequest {
        V1 => v01::TestingProbeRequest,
        V2 => v02::TestingProbeRequest,
    }
    pub enum TestingProbeResponse {
        V1 => v01::TestingProbeResponse,
        V2 => v02::TestingProbeResponse,
    }
    pub enum TestingProbeError {
        V1 => v01::TestingProbeError,
        V2 => v02::TestingProbeError,
    }
    pub enum TestingFrameworkErrorRequest { V1 => v01::TestingFrameworkErrorRequest }
}
