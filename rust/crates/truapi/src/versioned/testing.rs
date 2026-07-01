//! Versioned wrappers for the debug-only [`Testing`](crate::api::Testing) API.

use crate::{v01, v02};

truapi_macros::versioned_type! {
    pub enum TestingVersionProbeRequest {
        V1 => v01::TestingVersionProbeRequest,
        V2 => v02::TestingVersionProbeRequest,
    }
    pub enum TestingVersionProbeResponse {
        V1 => v01::TestingVersionProbeResponse,
        V2 => v02::TestingVersionProbeResponse,
    }
    pub enum TestingVersionProbeError {
        V1 => v01::TestingVersionProbeError,
        V2 => v02::TestingVersionProbeError,
    }
}
