//! Versioned wrappers for [`EntropyDerivation`](super::super::v02::EntropyDerivation) methods (V0.2+).

use parity_scale_codec::{Decode, Encode};

use super::Versioned;
use crate::v02::Entropy;

/// Request wrapper for `host_derive_entropy`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostDeriveEntropyRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(Vec<u8>),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(Vec<u8>),
}

impl Versioned for HostDeriveEntropyRequest {
    type Inner = Vec<u8>;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}

/// Response wrapper for `host_derive_entropy`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum HostDeriveEntropyResponse {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(Entropy),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(Entropy),
}

impl Versioned for HostDeriveEntropyResponse {
    type Inner = Entropy;
    fn wrap(version: u8, inner: Self::Inner) -> Self {
        match version {
            1 => Self::V1(inner),
            _ => Self::V2(inner),
        }
    }
    fn into_inner(self) -> Self::Inner {
        match self {
            Self::V1(x) | Self::V2(x) => x,
        }
    }
}
