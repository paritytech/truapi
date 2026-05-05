//! Versioned wrappers for [`Preimage`](super::super::v02::Preimage) methods.

use parity_scale_codec::{Decode, Encode};

use super::Versioned;
use crate::v02::{PreimageKey, PreimageValue};

/// Subscription request wrapper for `remote_preimage_lookup_subscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemotePreimageLookupSubscribeRequest {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(PreimageKey),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(PreimageKey),
}

impl Versioned for RemotePreimageLookupSubscribeRequest {
    type Inner = PreimageKey;
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

/// Subscription item wrapper for `remote_preimage_lookup_subscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
#[serde(tag = "tag", content = "value")]
pub enum RemotePreimageLookupSubscribeItem {
    /// Pre-RFC-0001 variant.
    #[codec(index = 0)]
    V1(Option<PreimageValue>),
    /// RFC-0001 variant.
    #[codec(index = 1)]
    V2(Option<PreimageValue>),
}

impl Versioned for RemotePreimageLookupSubscribeItem {
    type Inner = Option<PreimageValue>;
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
